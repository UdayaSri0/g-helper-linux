//! Mount and unmount through UDisks2 after independent, fresh validation.

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::future::Future;
use std::io::{ErrorKind, Write};
use std::os::fd::{AsFd, OwnedFd};
use std::os::unix::fs::MetadataExt;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use rog_core::{
    StorageExecutionOutcome as Outcome, StorageExecutionResult as Result, StorageOperationIntent,
    StorageOperationRequest, POLKIT_ACTION_STORAGE_MOUNT, POLKIT_ACTION_STORAGE_UNMOUNT,
};
use rog_providers::storage::{
    readback_storage_execution_target, resolve_storage_execution_target, ResolvedStorageTarget,
};
use tokio::time::{sleep, timeout};
use zbus::zvariant::{Fd, OwnedValue};
use zbus::{message::Header, Connection, Proxy};

const UDISKS_NAME: &str = "org.freedesktop.UDisks2";
const FILESYSTEM_IFACE: &str = "org.freedesktop.UDisks2.Filesystem";
const POLKIT_NAME: &str = "org.freedesktop.PolicyKit1";
const POLKIT_PATH: &str = "/org/freedesktop/PolicyKit1/Authority";
const POLKIT_IFACE: &str = "org.freedesktop.PolicyKit1.Authority";
static AUTH_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct AuthorizationCancellation {
    connection: Connection,
    id: String,
    armed: bool,
}

impl Drop for AuthorizationCancellation {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            let connection = self.connection.clone();
            let id = self.id.clone();
            runtime.spawn(async move {
                if let Ok(proxy) =
                    Proxy::new(&connection, POLKIT_NAME, POLKIT_PATH, POLKIT_IFACE).await
                {
                    let _ = timeout(
                        Duration::from_secs(2),
                        proxy.call::<_, _, ()>("CancelCheckAuthorization", &(id.as_str(),)),
                    )
                    .await;
                }
            });
        }
    }
}

const PENDING_MARKER: &str = "storage-operation.pending";

fn pending_marker_path() -> std::result::Result<PathBuf, Result> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or_else(|| {
            result(
                Outcome::Failed,
                "Home directory is unavailable for safe storage operation tracking",
                vec![],
            )
        })?;
    Ok(home.join(".local/state/rog-helper").join(PENDING_MARKER))
}

fn pending_marker_exists() -> std::result::Result<bool, Result> {
    let path = pending_marker_path()?;
    pending_marker_exists_at(&path)
}

/// Any unavailable state is treated as unresolved at daemon startup.
pub(crate) fn pending_marker_unresolved() -> bool {
    pending_marker_exists().unwrap_or(true)
}

fn pending_marker_exists_at(path: &Path) -> std::result::Result<bool, Result> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(_) => Err(result(
            Outcome::Uncertain,
            "Pending storage operation state is unreadable",
            vec![],
        )),
    }
}

struct PendingOperation {
    path: PathBuf,
}

impl PendingOperation {
    fn begin_at(path: PathBuf) -> std::result::Result<Self, Result> {
        let directory = path.parent().expect("pending marker has a parent");
        fs::create_dir_all(directory).map_err(|_| {
            result(
                Outcome::Failed,
                "Cannot create storage operation state directory",
                vec![],
            )
        })?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .map_err(|error| {
                if error.kind() == ErrorKind::AlreadyExists {
                    result(
                        Outcome::Uncertain,
                        "A prior storage operation is unresolved",
                        vec![],
                    )
                } else {
                    result(
                        Outcome::Failed,
                        "Cannot persist pending storage operation",
                        vec![],
                    )
                }
            })?;
        file.write_all(b"pending\n")
            .and_then(|_| file.sync_all())
            .and_then(|_| File::open(directory)?.sync_all())
            .map_err(|_| {
                result(
                    Outcome::Uncertain,
                    "Pending storage operation could not be durably recorded",
                    vec![],
                )
            })?;
        Ok(Self { path })
    }

    fn clear(self) -> std::result::Result<(), Result> {
        fs::remove_file(&self.path)
            .and_then(|_| {
                File::open(self.path.parent().expect("pending marker has a parent"))?.sync_all()
            })
            .map_err(|_| {
                result(
                    Outcome::Uncertain,
                    "Storage state was confirmed but pending marker could not be cleared",
                    vec![],
                )
            })
    }
}

fn result(outcome: Outcome, message: &str, mount_points: Vec<String>) -> Result {
    Result {
        outcome,
        message: message.into(),
        mount_points,
    }
}

fn action(intent: StorageOperationIntent) -> Option<&'static str> {
    match intent {
        StorageOperationIntent::Mount => Some(POLKIT_ACTION_STORAGE_MOUNT),
        StorageOperationIntent::Unmount => Some(POLKIT_ACTION_STORAGE_UNMOUNT),
        _ => None,
    }
}

fn same_target(before: &ResolvedStorageTarget, after: &ResolvedStorageTarget) -> bool {
    before.object_path == after.object_path
        && before.device_number == after.device_number
        && before.filesystem == after.filesystem
}

#[derive(Clone, Copy)]
enum BackendOutcome {
    Completed,
    Rejected,
    TimedOut,
}

fn confirmed_result(backend: BackendOutcome, readback: Option<(Vec<String>, bool)>) -> Result {
    match (backend, readback) {
        (BackendOutcome::Completed, Some((mount_points, true))) => result(
            Outcome::Succeeded,
            "Operation confirmed by fresh storage readback",
            mount_points,
        ),
        // An error reply does not prove that an already-sent request cannot complete later.
        (_, Some((mount_points, _))) => result(
            Outcome::Uncertain,
            "Operation outcome could not be confirmed",
            mount_points,
        ),
        _ => result(
            Outcome::Uncertain,
            "Storage target disappeared or readback failed",
            vec![],
        ),
    }
}

#[derive(Debug, Clone)]
struct CallerIdentity {
    sender: String,
    pid: u32,
    uid: u32,
    start_time: u64,
    pidfd: Option<Arc<OwnedFd>>,
}

impl PartialEq for CallerIdentity {
    fn eq(&self, other: &Self) -> bool {
        self.sender == other.sender
            && self.pid == other.pid
            && self.uid == other.uid
            && self.start_time == other.start_time
    }
}

impl Eq for CallerIdentity {}

fn process_start_time(pid: u32) -> Option<u64> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let rest = stat.rsplit_once(')')?.1;
    rest.split_whitespace()
        .nth(19)?
        .parse::<u64>()
        .ok()
        .filter(|v| *v != 0)
}

async fn caller_identity(
    session: &Connection,
    sender: &str,
) -> std::result::Result<CallerIdentity, Result> {
    let error = || {
        result(
            Outcome::Failed,
            "D-Bus caller identity is unavailable",
            vec![],
        )
    };
    let proxy = zbus::fdo::DBusProxy::new(session)
        .await
        .map_err(|_| error())?;
    let name = zbus::names::BusName::try_from(sender).map_err(|_| error())?;
    if !sender.starts_with(':') {
        return Err(error());
    }
    let credentials = proxy
        .get_connection_credentials(name)
        .await
        .map_err(|_| error())?;
    let pid = credentials.process_id().ok_or_else(error)?;
    let uid = credentials.unix_user_id().ok_or_else(error)?;
    let pidfd = credentials
        .process_fd()
        .ok_or_else(error)?
        .as_fd()
        .try_clone_to_owned()
        .map_err(|_| error())?;
    let process = fs::metadata(format!("/proc/{pid}")).map_err(|_| error())?;
    let daemon = fs::metadata("/proc/self").map_err(|_| error())?;
    if pid == 0 || uid > i32::MAX as u32 || process.uid() != uid || daemon.uid() != uid {
        return Err(error());
    }
    let start_time = process_start_time(pid).ok_or_else(error)?;
    Ok(CallerIdentity {
        sender: sender.into(),
        pid,
        uid,
        start_time,
        pidfd: Some(Arc::new(pidfd)),
    })
}

async fn authorize(
    connection: &Connection,
    caller: &CallerIdentity,
    intent: StorageOperationIntent,
) -> std::result::Result<(), Result> {
    let Some(action) = action(intent) else {
        return Err(result(
            Outcome::Failed,
            "Unsupported storage operation",
            vec![],
        ));
    };
    let proxy = Proxy::new(connection, POLKIT_NAME, POLKIT_PATH, POLKIT_IFACE)
        .await
        .map_err(|_| result(Outcome::Failed, "PolicyKit is unavailable", vec![]))?;
    let subject = polkit_subject(caller)?;
    let details = HashMap::<String, String>::new();
    let mut cancellation = AuthorizationCancellation {
        connection: connection.clone(),
        id: format!(
            "rog-storage-{}-{}",
            std::process::id(),
            AUTH_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ),
        armed: true,
    };
    let response = timeout(
        Duration::from_secs(120),
        proxy.call(
            "CheckAuthorization",
            &(subject, action, details, 1_u32, cancellation.id.as_str()),
        ),
    )
    .await;
    if response.is_ok() {
        cancellation.armed = false;
    }
    let decision: (bool, bool, HashMap<String, String>) = response
        .map_err(|_| result(Outcome::Cancelled, "Authorization timed out", vec![]))?
        .map_err(|error: zbus::Error| {
            let cancelled = error.to_string().to_ascii_lowercase().contains("cancel");
            result(
                if cancelled {
                    Outcome::Cancelled
                } else {
                    Outcome::Failed
                },
                if cancelled {
                    "Authorization was cancelled"
                } else {
                    "PolicyKit authorization failed"
                },
                vec![],
            )
        })?;
    if decision.0 {
        Ok(())
    } else {
        Err(result(Outcome::Denied, "Authorization was denied", vec![]))
    }
}

fn polkit_subject(
    caller: &CallerIdentity,
) -> std::result::Result<(&'static str, HashMap<String, OwnedValue>), Result> {
    let pidfd = caller
        .pidfd
        .as_ref()
        .ok_or_else(|| {
            result(
                Outcome::Failed,
                "Caller process descriptor is unavailable",
                vec![],
            )
        })?
        .as_fd()
        .try_clone_to_owned()
        .map_err(|_| {
            result(
                Outcome::Failed,
                "Caller process descriptor is unavailable",
                vec![],
            )
        })?;
    let pidfd_value = OwnedValue::try_from(Fd::from(pidfd)).map_err(|_| {
        result(
            Outcome::Failed,
            "Cannot encode caller process descriptor",
            vec![],
        )
    })?;
    Ok((
        "unix-process",
        HashMap::from([
            ("pidfd".to_string(), pidfd_value),
            ("uid".to_string(), OwnedValue::from(caller.uid as i32)),
        ]),
    ))
}

type Step<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

trait ExecutionOps: Sync {
    fn identify(&self) -> Step<'_, std::result::Result<CallerIdentity, Result>>;
    fn resolve<'a>(
        &'a self,
        request: &'a StorageOperationRequest,
        initial: bool,
    ) -> Step<'a, std::result::Result<ResolvedStorageTarget, String>>;
    fn authorize<'a>(
        &'a self,
        caller: &'a CallerIdentity,
        intent: StorageOperationIntent,
    ) -> Step<'a, std::result::Result<(), Result>>;
    /// Err means no mutating request was sent; an error after dispatch is BackendOutcome::Rejected.
    fn call<'a>(
        &'a self,
        request: &'a StorageOperationRequest,
        caller: &'a CallerIdentity,
        target: &'a ResolvedStorageTarget,
    ) -> Step<'a, std::result::Result<BackendOutcome, Result>>;
}

struct LiveOps<'a> {
    session: &'a Connection,
    sender: &'a str,
    system: &'a Connection,
}

impl ExecutionOps for LiveOps<'_> {
    fn identify(&self) -> Step<'_, std::result::Result<CallerIdentity, Result>> {
        Box::pin(caller_identity(self.session, self.sender))
    }

    fn resolve<'a>(
        &'a self,
        request: &'a StorageOperationRequest,
        initial: bool,
    ) -> Step<'a, std::result::Result<ResolvedStorageTarget, String>> {
        Box::pin(async move {
            if initial {
                resolve_storage_execution_target(request).await
            } else {
                readback_storage_execution_target(request).await
            }
        })
    }

    fn authorize<'a>(
        &'a self,
        caller: &'a CallerIdentity,
        intent: StorageOperationIntent,
    ) -> Step<'a, std::result::Result<(), Result>> {
        Box::pin(authorize(self.system, caller, intent))
    }

    fn call<'a>(
        &'a self,
        request: &'a StorageOperationRequest,
        caller: &'a CallerIdentity,
        target: &'a ResolvedStorageTarget,
    ) -> Step<'a, std::result::Result<BackendOutcome, Result>> {
        Box::pin(async move {
            let proxy = Proxy::new(
                self.system,
                UDISKS_NAME,
                target.object_path.as_str(),
                FILESYSTEM_IFACE,
            )
            .await
            .map_err(|_| result(Outcome::Failed, "UDisks2 filesystem is unavailable", vec![]))?;
            if !matches!(self.identify().await, Ok(identity) if identity == *caller)
                || !matches!(self.resolve(request, true).await, Ok(now) if same_target(target, &now) && target.mount_points == now.mount_points)
            {
                return Err(result(
                    Outcome::Failed,
                    "Caller or target changed before execution",
                    vec![],
                ));
            }
            // UDisks2 sees the daemon as its caller and must not prompt for that identity.
            let options = HashMap::from([(
                "auth.no_user_interaction".to_string(),
                OwnedValue::from(true),
            )]);
            let call = timeout(Duration::from_secs(30), async {
                match request.intent {
                    StorageOperationIntent::Mount => proxy
                        .call::<_, _, String>("Mount", &(options,))
                        .await
                        .map(|_| ()),
                    StorageOperationIntent::Unmount => {
                        proxy.call::<_, _, ()>("Unmount", &(options,)).await
                    }
                    _ => unreachable!(),
                }
            })
            .await;
            Ok(match call {
                Ok(Ok(())) => BackendOutcome::Completed,
                Ok(Err(_)) => BackendOutcome::Rejected,
                Err(_) => BackendOutcome::TimedOut,
            })
        })
    }
}

async fn execute_with<B: ExecutionOps>(
    request: &StorageOperationRequest,
    ops: &B,
    marker_path: &Path,
) -> Result {
    if action(request.intent).is_none() {
        return result(
            Outcome::Failed,
            "Only mount and unmount can execute",
            vec![],
        );
    }
    match pending_marker_exists_at(marker_path) {
        Ok(false) => {}
        Ok(true) => {
            return result(
                Outcome::Uncertain,
                "A prior storage operation is unresolved",
                vec![],
            )
        }
        Err(outcome) => return outcome,
    }
    let caller = match ops.identify().await {
        Ok(caller) => caller,
        Err(outcome) => return outcome,
    };
    let before = match ops.resolve(request, true).await {
        Ok(target) => target,
        Err(reason) => return result(Outcome::Failed, &reason, vec![]),
    };
    if let Err(outcome) = ops.authorize(&caller, request.intent).await {
        return outcome;
    }
    if !matches!(ops.identify().await, Ok(identity) if identity == caller) {
        return result(
            Outcome::Failed,
            "D-Bus caller changed during authorization",
            vec![],
        );
    }
    let current = match ops.resolve(request, true).await {
        Ok(target)
            if same_target(&before, &target) && before.mount_points == target.mount_points =>
        {
            target
        }
        _ => {
            return result(
                Outcome::Failed,
                "Storage target changed during authorization",
                vec![],
            )
        }
    };
    let pending = match PendingOperation::begin_at(marker_path.to_path_buf()) {
        Ok(pending) => pending,
        Err(outcome) => return outcome,
    };
    if !matches!(ops.identify().await, Ok(identity) if identity == caller)
        || !matches!(ops.resolve(request, true).await, Ok(target) if same_target(&current, &target) && target.mount_points == current.mount_points)
    {
        let outcome = result(
            Outcome::Failed,
            "Caller or target changed before execution",
            vec![],
        );
        return pending.clear().map_or_else(|error| error, |()| outcome);
    }
    let backend = match ops.call(request, &caller, &current).await {
        Ok(backend) => backend,
        Err(outcome) => return pending.clear().map_or_else(|error| error, |()| outcome),
    };
    let mut readback = None;
    for attempt in 0..5 {
        if let Ok(target) = ops.resolve(request, false).await {
            if same_target(&before, &target) {
                let desired = match request.intent {
                    StorageOperationIntent::Mount => !target.mount_points.is_empty(),
                    StorageOperationIntent::Unmount => target.mount_points.is_empty(),
                    _ => false,
                };
                readback = Some((target.mount_points, desired));
                if desired {
                    break;
                }
            }
        }
        if attempt < 4 {
            sleep(Duration::from_millis(200)).await;
        }
    }
    let outcome = confirmed_result(backend, readback);
    if outcome.outcome == Outcome::Uncertain {
        outcome
    } else {
        pending.clear().map_or_else(|error| error, |()| outcome)
    }
}

pub async fn execute(
    request: &StorageOperationRequest,
    header: &Header<'_>,
    session: &Connection,
) -> Result {
    let Some(sender) = header.sender() else {
        return result(
            Outcome::Failed,
            "D-Bus caller identity is unavailable",
            vec![],
        );
    };
    let marker = match pending_marker_path() {
        Ok(marker) => marker,
        Err(outcome) => return outcome,
    };
    let system = match Connection::system().await {
        Ok(system) => system,
        Err(_) => return result(Outcome::Failed, "System D-Bus is unavailable", vec![]),
    };
    execute_with(
        request,
        &LiveOps {
            session,
            sender: sender.as_str(),
            system: &system,
        },
        &marker,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::Mutex;

    #[test]
    fn only_mount_and_unmount_have_authorization_actions() {
        assert_eq!(
            action(StorageOperationIntent::Mount),
            Some(POLKIT_ACTION_STORAGE_MOUNT)
        );
        assert_eq!(
            action(StorageOperationIntent::Unmount),
            Some(POLKIT_ACTION_STORAGE_UNMOUNT)
        );
        assert_eq!(action(StorageOperationIntent::FormatFilesystem), None);
    }

    #[test]
    fn mock_backend_requires_matching_readback_for_success() {
        for mounts in [vec!["/run/media/test".to_string()], vec![]] {
            assert_eq!(
                confirmed_result(BackendOutcome::Completed, Some((mounts, true))).outcome,
                Outcome::Succeeded
            );
        }
        assert_eq!(
            confirmed_result(BackendOutcome::Completed, Some((vec![], false))).outcome,
            Outcome::Uncertain
        );
        assert_eq!(
            confirmed_result(BackendOutcome::Rejected, Some((vec![], false))).outcome,
            Outcome::Uncertain
        );
        assert_eq!(
            confirmed_result(
                BackendOutcome::Rejected,
                Some((vec!["/media/x".into()], true))
            )
            .outcome,
            Outcome::Uncertain
        );
        assert_eq!(
            confirmed_result(BackendOutcome::TimedOut, None).outcome,
            Outcome::Uncertain
        );
    }

    #[test]
    fn process_start_time_uses_live_proc_stat() {
        assert!(process_start_time(std::process::id()).is_some());
        assert_eq!(process_start_time(u32::MAX), None);
    }

    #[test]
    fn polkit_subject_requires_process_descriptor_and_excludes_reusable_pid() {
        let mut caller = mock_caller();
        assert!(polkit_subject(&caller).is_err());
        let file = File::open("/proc/self/stat").unwrap();
        caller.pidfd = Some(Arc::new(file.into()));
        let (kind, details) = polkit_subject(&caller).unwrap();
        assert_eq!(kind, "unix-process");
        assert_eq!(details.len(), 2);
        assert!(details.contains_key("pidfd"));
        assert!(details.contains_key("uid"));
        assert!(!details.contains_key("pid"));
        assert!(!details.contains_key("start-time"));
    }

    #[test]
    fn execution_wire_request_rejects_preview_and_forged_fields() {
        let preview = r#"{"intent":"mount","drive_id":"disk","namespace":"sda","partition_id":"part","expected_capacity_bytes":1,"expected_start_bytes":0,"expected_length_bytes":1}"#;
        assert!(serde_json::from_str::<rog_core::StorageExecutionRequest>(preview).is_err());
        let wrapped = format!(r#"{{"target":{preview}}}"#);
        assert!(serde_json::from_str::<rog_core::StorageExecutionRequest>(&wrapped).is_ok());
        let forged = format!(r#"{{"target":{preview},"device_path":"/dev/sda1"}}"#);
        assert!(serde_json::from_str::<rog_core::StorageExecutionRequest>(&forged).is_err());
    }

    #[test]
    fn pending_marker_survives_interruption_and_blocks_reentry() {
        let directory = std::env::temp_dir().join(format!(
            "rog-storage-pending-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = directory.join(PENDING_MARKER);
        assert!(!pending_marker_exists_at(&path).unwrap());
        let pending = PendingOperation::begin_at(path.clone()).unwrap();
        assert!(pending_marker_exists_at(&path).unwrap());
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(PendingOperation::begin_at(path.clone()).is_err());
        drop(pending);
        assert!(pending_marker_exists_at(&path).unwrap());
        fs::remove_file(&path).unwrap();
        let pending = PendingOperation::begin_at(path.clone()).unwrap();
        pending.clear().unwrap();
        assert!(!pending_marker_exists_at(&path).unwrap());
        fs::remove_dir_all(directory).unwrap();
    }

    struct MockState {
        events: Vec<&'static str>,
        identities: VecDeque<std::result::Result<CallerIdentity, Result>>,
        targets: VecDeque<std::result::Result<ResolvedStorageTarget, String>>,
        readbacks: VecDeque<std::result::Result<ResolvedStorageTarget, String>>,
        authorization: Option<std::result::Result<(), Result>>,
        backend: Option<std::result::Result<BackendOutcome, Result>>,
    }

    struct MockOps(Mutex<MockState>);

    fn mock_caller() -> CallerIdentity {
        CallerIdentity {
            sender: ":1.42".into(),
            pid: 100,
            uid: 1000,
            start_time: 10,
            pidfd: None,
        }
    }

    fn mock_target(mounts: &[&str]) -> ResolvedStorageTarget {
        ResolvedStorageTarget {
            object_path: "/org/freedesktop/UDisks2/block_devices/mock"
                .try_into()
                .unwrap(),
            device_number: 2049,
            mount_points: mounts.iter().map(|point| (*point).into()).collect(),
            filesystem: "ext4".into(),
        }
    }

    fn mock_request(intent: StorageOperationIntent) -> StorageOperationRequest {
        StorageOperationRequest {
            intent,
            drive_id: "opaque-drive".into(),
            namespace: "mock".into(),
            partition_id: Some("opaque-partition".into()),
            expected_capacity_bytes: 16384,
            expected_start_bytes: Some(4096),
            expected_length_bytes: Some(8192),
        }
    }

    fn mock_ops(intent: StorageOperationIntent) -> MockOps {
        let before = if intent == StorageOperationIntent::Unmount {
            mock_target(&["/run/media/test"])
        } else {
            mock_target(&[])
        };
        let after = if intent == StorageOperationIntent::Unmount {
            mock_target(&[])
        } else {
            mock_target(&["/run/media/test"])
        };
        MockOps(Mutex::new(MockState {
            events: Vec::new(),
            identities: VecDeque::from(vec![Ok(mock_caller()); 3]),
            targets: VecDeque::from(vec![Ok(before.clone()); 3]),
            readbacks: VecDeque::from([Ok(after)]),
            authorization: Some(Ok(())),
            backend: Some(Ok(BackendOutcome::Completed)),
        }))
    }

    impl ExecutionOps for MockOps {
        fn identify(&self) -> Step<'_, std::result::Result<CallerIdentity, Result>> {
            Box::pin(async move {
                let mut state = self.0.lock().unwrap();
                state.events.push("identify");
                state
                    .identities
                    .pop_front()
                    .unwrap_or_else(|| Ok(mock_caller()))
            })
        }

        fn resolve<'a>(
            &'a self,
            _request: &'a StorageOperationRequest,
            initial: bool,
        ) -> Step<'a, std::result::Result<ResolvedStorageTarget, String>> {
            Box::pin(async move {
                let mut state = self.0.lock().unwrap();
                state
                    .events
                    .push(if initial { "resolve" } else { "readback" });
                if initial {
                    state
                        .targets
                        .pop_front()
                        .unwrap_or_else(|| Err("unexpected resolve".into()))
                } else {
                    state
                        .readbacks
                        .pop_front()
                        .unwrap_or_else(|| Err("readback unavailable".into()))
                }
            })
        }

        fn authorize<'a>(
            &'a self,
            _caller: &'a CallerIdentity,
            intent: StorageOperationIntent,
        ) -> Step<'a, std::result::Result<(), Result>> {
            Box::pin(async move {
                let mut state = self.0.lock().unwrap();
                state.events.push(match intent {
                    StorageOperationIntent::Mount => "authorize_mount",
                    StorageOperationIntent::Unmount => "authorize_unmount",
                    _ => "authorize_unsupported",
                });
                state.authorization.take().expect("one authorization")
            })
        }

        fn call<'a>(
            &'a self,
            _request: &'a StorageOperationRequest,
            _caller: &'a CallerIdentity,
            _target: &'a ResolvedStorageTarget,
        ) -> Step<'a, std::result::Result<BackendOutcome, Result>> {
            Box::pin(async move {
                let mut state = self.0.lock().unwrap();
                state.events.push("backend");
                state.backend.take().expect("one backend call")
            })
        }
    }

    fn mock_marker() -> PathBuf {
        std::env::temp_dir()
            .join(format!(
                "rog-storage-flow-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ))
            .join(PENDING_MARKER)
    }

    #[tokio::test]
    async fn mocked_mount_and_unmount_follow_complete_sequence() {
        for intent in [
            StorageOperationIntent::Mount,
            StorageOperationIntent::Unmount,
        ] {
            let ops = mock_ops(intent);
            let marker = mock_marker();
            let outcome = execute_with(&mock_request(intent), &ops, &marker).await;
            assert_eq!(outcome.outcome, Outcome::Succeeded);
            assert!(!pending_marker_exists_at(&marker).unwrap());
            assert_eq!(
                ops.0.lock().unwrap().events,
                vec![
                    "identify",
                    "resolve",
                    if intent == StorageOperationIntent::Mount {
                        "authorize_mount"
                    } else {
                        "authorize_unmount"
                    },
                    "identify",
                    "resolve",
                    "identify",
                    "resolve",
                    "backend",
                    "readback",
                ]
            );
            fs::remove_dir_all(marker.parent().unwrap()).unwrap();
        }
    }

    #[tokio::test]
    async fn mocked_rejections_never_call_mutating_backend() {
        for rejected in [Outcome::Denied, Outcome::Cancelled, Outcome::Failed] {
            let ops = mock_ops(StorageOperationIntent::Mount);
            ops.0.lock().unwrap().authorization =
                Some(Err(result(rejected, "auth stopped", vec![])));
            let marker = mock_marker();
            assert_eq!(
                execute_with(&mock_request(StorageOperationIntent::Mount), &ops, &marker)
                    .await
                    .outcome,
                rejected
            );
            assert!(!ops.0.lock().unwrap().events.contains(&"backend"));
            assert!(!pending_marker_exists_at(&marker).unwrap());
        }
        for phase in [0, 1, 2] {
            let ops = mock_ops(StorageOperationIntent::Mount);
            ops.0.lock().unwrap().targets[phase] = Err("removed or changed device".into());
            let marker = mock_marker();
            assert_eq!(
                execute_with(&mock_request(StorageOperationIntent::Mount), &ops, &marker)
                    .await
                    .outcome,
                Outcome::Failed
            );
            assert!(!ops.0.lock().unwrap().events.contains(&"backend"));
            assert!(!pending_marker_exists_at(&marker).unwrap());
            if marker.parent().unwrap().exists() {
                fs::remove_dir_all(marker.parent().unwrap()).unwrap();
            }
        }
        let ops = mock_ops(StorageOperationIntent::Mount);
        let mut replacement = mock_target(&[]);
        replacement.device_number += 1;
        ops.0.lock().unwrap().targets[1] = Ok(replacement);
        let marker = mock_marker();
        assert_eq!(
            execute_with(&mock_request(StorageOperationIntent::Mount), &ops, &marker)
                .await
                .outcome,
            Outcome::Failed
        );
        assert!(!ops.0.lock().unwrap().events.contains(&"backend"));
        for phase in [1, 2] {
            let ops = mock_ops(StorageOperationIntent::Mount);
            let mut changed = mock_caller();
            changed.start_time += 1;
            ops.0.lock().unwrap().identities[phase] = Ok(changed);
            let marker = mock_marker();
            assert_eq!(
                execute_with(&mock_request(StorageOperationIntent::Mount), &ops, &marker)
                    .await
                    .outcome,
                Outcome::Failed
            );
            assert!(!ops.0.lock().unwrap().events.contains(&"backend"));
            assert!(!pending_marker_exists_at(&marker).unwrap());
            if marker.parent().unwrap().exists() {
                fs::remove_dir_all(marker.parent().unwrap()).unwrap();
            }
        }
        let ops = mock_ops(StorageOperationIntent::Mount);
        let marker = mock_marker();
        let _pending = PendingOperation::begin_at(marker.clone()).unwrap();
        assert_eq!(
            execute_with(&mock_request(StorageOperationIntent::Mount), &ops, &marker)
                .await
                .outcome,
            Outcome::Uncertain
        );
        assert!(ops.0.lock().unwrap().events.is_empty());
        fs::remove_dir_all(marker.parent().unwrap()).unwrap();
    }

    #[tokio::test]
    async fn mocked_backend_error_and_readback_disagreement_remain_uncertain() {
        for backend in [
            BackendOutcome::Rejected,
            BackendOutcome::TimedOut,
            BackendOutcome::Completed,
        ] {
            let ops = mock_ops(StorageOperationIntent::Mount);
            ops.0.lock().unwrap().backend = Some(Ok(backend));
            ops.0.lock().unwrap().readbacks = VecDeque::from([Ok(mock_target(&[]))]);
            let marker = mock_marker();
            assert_eq!(
                execute_with(&mock_request(StorageOperationIntent::Mount), &ops, &marker)
                    .await
                    .outcome,
                Outcome::Uncertain
            );
            assert!(pending_marker_exists_at(&marker).unwrap());
            fs::remove_dir_all(marker.parent().unwrap()).unwrap();
        }
    }
}
