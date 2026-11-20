//! Isolated D-Bus rejection tests. The harness maps the system bus to a fresh test bus.

use std::collections::HashMap;
use std::fs;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use rog_core::{
    StorageExecutionOutcome, StorageExecutionRequest, StorageExecutionResult,
    StorageOperationIntent, StorageOperationRequest,
};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};
use zbus::{connection, interface, Connection, Proxy};

#[derive(Clone, Default)]
struct Calls {
    mount: Arc<AtomicUsize>,
    unmount: Arc<AtomicUsize>,
    authorize: Arc<AtomicUsize>,
}

struct MockFilesystem(Calls);

#[interface(name = "org.freedesktop.UDisks2.Filesystem")]
impl MockFilesystem {
    fn mount(&self, _options: HashMap<String, OwnedValue>) -> String {
        self.0.mount.fetch_add(1, Ordering::SeqCst);
        "/run/media/mock".into()
    }

    fn unmount(&self, _options: HashMap<String, OwnedValue>) {
        self.0.unmount.fetch_add(1, Ordering::SeqCst);
    }
}

struct MockManager;

#[interface(name = "org.freedesktop.DBus.ObjectManager")]
impl MockManager {
    fn get_managed_objects(
        &self,
    ) -> HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>> {
        HashMap::new()
    }
}

struct MockAuthority(Calls);

#[interface(name = "org.freedesktop.PolicyKit1.Authority")]
impl MockAuthority {
    fn check_authorization(
        &self,
        _subject: (&str, HashMap<String, OwnedValue>),
        _action_id: &str,
        _details: HashMap<String, String>,
        _flags: u32,
        _cancellation_id: &str,
    ) -> (bool, bool, HashMap<String, String>) {
        self.0.authorize.fetch_add(1, Ordering::SeqCst);
        (true, false, HashMap::new())
    }
}

struct Daemon(Child);

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn request(intent: StorageOperationIntent) -> StorageExecutionRequest {
    StorageExecutionRequest {
        target: StorageOperationRequest {
            intent,
            drive_id: "definitely-absent-test-drive".into(),
            namespace: "mock".into(),
            partition_id: Some("mock-partition".into()),
            expected_capacity_bytes: 8192,
            expected_start_bytes: Some(512),
            expected_length_bytes: Some(4096),
        },
    }
}

async fn daemon_proxy(connection: &Connection) -> Proxy<'_> {
    let bus = zbus::fdo::DBusProxy::new(connection).await.unwrap();
    let name = zbus::names::BusName::try_from("io.github.roghelper.Daemon").unwrap();
    for _ in 0..40 {
        if bus.get_name_owner(name.clone()).await.is_ok() {
            return Proxy::new(
                connection,
                "io.github.roghelper.Daemon",
                "/io/github/roghelper/Daemon",
                "io.github.roghelper.Daemon1",
            )
            .await
            .unwrap();
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("isolated daemon did not appear");
}

#[tokio::test]
async fn disabled_execution_rejects_valid_and_forged_requests_without_backend_calls() {
    if std::env::var("ROG_STORAGE_TEST_ISOLATED_DBUS").as_deref() != Ok("1") {
        eprintln!("isolated D-Bus test skipped outside the designated test bus");
        return;
    }
    assert_eq!(
        std::env::var("DBUS_SYSTEM_BUS_ADDRESS").unwrap(),
        std::env::var("DBUS_SESSION_BUS_ADDRESS").unwrap(),
        "test must never use the host system bus"
    );
    let calls = Calls::default();
    let _udisks = connection::Builder::session()
        .unwrap()
        .name("org.freedesktop.UDisks2")
        .unwrap()
        .serve_at("/org/freedesktop/UDisks2", MockManager)
        .unwrap()
        .serve_at(
            "/org/freedesktop/UDisks2/block_devices/mock",
            MockFilesystem(calls.clone()),
        )
        .unwrap()
        .build()
        .await
        .unwrap();
    let _polkit = connection::Builder::session()
        .unwrap()
        .name("org.freedesktop.PolicyKit1")
        .unwrap()
        .serve_at(
            "/org/freedesktop/PolicyKit1/Authority",
            MockAuthority(calls.clone()),
        )
        .unwrap()
        .build()
        .await
        .unwrap();
    let root = std::env::temp_dir().join(format!("rog-storage-dbus-test-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let daemon = Command::new(env!("CARGO_BIN_EXE_rog-helperd"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("XDG_DATA_HOME", root.join("data"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let _daemon = Daemon(daemon);
    let client = Connection::session().await.unwrap();
    let proxy = daemon_proxy(&client).await;
    for bad in [
        "{".to_string(),
        serde_json::to_string(&request(StorageOperationIntent::Mount).target).unwrap(),
        format!(
            r#"{{"target":{},"device_path":"/dev/nvme0n1"}}"#,
            serde_json::to_string(&request(StorageOperationIntent::Mount).target).unwrap()
        ),
        "x".repeat(4097),
        serde_json::to_string(&request(StorageOperationIntent::FormatFilesystem)).unwrap(),
    ] {
        assert!(proxy
            .call::<_, _, String>("ExecuteStorageOperation", &(bad.as_str(),))
            .await
            .is_err());
    }
    for intent in [
        StorageOperationIntent::Mount,
        StorageOperationIntent::Unmount,
    ] {
        let wire = serde_json::to_string(&request(intent)).unwrap();
        let response: String = proxy
            .call("ExecuteStorageOperation", &(wire,))
            .await
            .unwrap();
        let outcome: StorageExecutionResult = serde_json::from_str(&response).unwrap();
        assert_eq!(outcome.outcome, StorageExecutionOutcome::Failed);
        assert!(outcome.message.contains("disabled"));
    }
    assert_eq!(calls.mount.load(Ordering::SeqCst), 0);
    assert_eq!(calls.unmount.load(Ordering::SeqCst), 0);
    assert_eq!(calls.authorize.load(Ordering::SeqCst), 0);
    drop(_daemon);
    fs::remove_dir_all(root).unwrap();
}
