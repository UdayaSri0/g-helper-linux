//! Mount and unmount through UDisks2 after independent, fresh validation.

use std::collections::HashMap;
use std::fs;
use std::future::Future;
use std::os::unix::fs::MetadataExt;
use std::time::Duration;

use rog_core::{
    StorageExecutionOutcome as Outcome, StorageExecutionResult as Result, StorageOperationIntent,
    StorageOperationRequest, POLKIT_ACTION_STORAGE_MOUNT, POLKIT_ACTION_STORAGE_UNMOUNT,
};
use rog_providers::storage::{
    readback_storage_execution_target, resolve_storage_execution_target, ResolvedStorageTarget,
};
use tokio::time::{sleep, timeout};
use zbus::zvariant::OwnedValue;
use zbus::{message::Header, Connection, Proxy};

const UDISKS_NAME: &str = "org.freedesktop.UDisks2";
const FILESYSTEM_IFACE: &str = "org.freedesktop.UDisks2.Filesystem";
const POLKIT_NAME: &str = "org.freedesktop.PolicyKit1";
const POLKIT_PATH: &str = "/org/freedesktop/PolicyKit1/Authority";
const POLKIT_IFACE: &str = "org.freedesktop.PolicyKit1.Authority";

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
        (BackendOutcome::Rejected, Some((mount_points, false))) => result(
            Outcome::Failed,
            "UDisks2 rejected the operation",
            mount_points,
        ),
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

async fn authorized_operation<F, G>(
    authorization: std::result::Result<(), Result>,
    operation: G,
) -> Result
where
    G: FnOnce() -> F,
    F: Future<Output = Result>,
{
    match authorization {
        Ok(()) => operation().await,
        Err(outcome) => outcome,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CallerIdentity {
    sender: String,
    pid: u32,
    uid: u32,
    start_time: u64,
}

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
    let pid = proxy
        .get_connection_unix_process_id(name.clone())
        .await
        .map_err(|_| error())?;
    let uid = proxy
        .get_connection_unix_user(name)
        .await
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
    let subject = (
        "unix-process",
        HashMap::from([
            ("pid".to_string(), OwnedValue::from(caller.pid)),
            ("uid".to_string(), OwnedValue::from(caller.uid as i32)),
            (
                "start-time".to_string(),
                OwnedValue::from(caller.start_time),
            ),
        ]),
    );
    let details = HashMap::<String, String>::new();
    let decision: (bool, bool, HashMap<String, String>) = timeout(
        Duration::from_secs(120),
        proxy.call("CheckAuthorization", &(subject, action, details, 1_u32, "")),
    )
    .await
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

pub async fn execute(
    request: &StorageOperationRequest,
    header: &Header<'_>,
    session: &Connection,
) -> Result {
    if action(request.intent).is_none() {
        return result(
            Outcome::Failed,
            "Only mount and unmount can execute",
            vec![],
        );
    }
    let before = match resolve_storage_execution_target(request).await {
        Ok(target) => target,
        Err(reason) => return result(Outcome::Failed, &reason, vec![]),
    };
    let Some(sender) = header.sender() else {
        return result(
            Outcome::Failed,
            "D-Bus caller identity is unavailable",
            vec![],
        );
    };
    let caller = match caller_identity(session, sender.as_str()).await {
        Ok(caller) => caller,
        Err(outcome) => return outcome,
    };
    let connection = match Connection::system().await {
        Ok(connection) => connection,
        Err(_) => return result(Outcome::Failed, "System D-Bus is unavailable", vec![]),
    };
    authorized_operation(authorize(&connection, &caller, request.intent).await, || async {
    if !matches!(caller_identity(session, &caller.sender).await, Ok(identity) if identity == caller)
    {
        return result(
            Outcome::Failed,
            "D-Bus caller changed during authorization",
            vec![],
        );
    }
    let current = match resolve_storage_execution_target(request).await {
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
    let proxy = match Proxy::new(
        &connection,
        UDISKS_NAME,
        current.object_path.as_str(),
        FILESYSTEM_IFACE,
    )
    .await
    {
        Ok(proxy) => proxy,
        Err(_) => return result(Outcome::Failed, "UDisks2 filesystem is unavailable", vec![]),
    };
    if !matches!(caller_identity(session, &caller.sender).await, Ok(identity) if identity == caller)
        || !matches!(resolve_storage_execution_target(request).await, Ok(target) if same_target(&current, &target) && target.mount_points == current.mount_points)
    {
        return result(
            Outcome::Failed,
            "Caller or target changed before execution",
            vec![],
        );
    }
    let options = HashMap::<String, OwnedValue>::new();
    let backend = timeout(Duration::from_secs(30), async {
        match request.intent {
            StorageOperationIntent::Mount => proxy
                .call::<_, _, String>("Mount", &(options,))
                .await
                .map(|_| ()),
            StorageOperationIntent::Unmount => proxy.call::<_, _, ()>("Unmount", &(options,)).await,
            _ => unreachable!(),
        }
    })
    .await;
    let mut readback = None;
    for _ in 0..5 {
        if let Ok(target) = readback_storage_execution_target(request).await {
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
        sleep(Duration::from_millis(200)).await;
    }
    let backend = match backend {
        Ok(Ok(())) => BackendOutcome::Completed,
        Ok(Err(_)) => BackendOutcome::Rejected,
        Err(_) => BackendOutcome::TimedOut,
    };
    confirmed_result(backend, readback)
    }).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

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
            Outcome::Failed
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

    #[tokio::test]
    async fn denied_cancelled_and_failed_authorization_never_call_backend() {
        for status in [Outcome::Denied, Outcome::Cancelled, Outcome::Failed] {
            let calls = Cell::new(0);
            let outcome = authorized_operation(
                Err(result(status, "authorization stopped", vec![])),
                || async {
                    calls.set(calls.get() + 1);
                    result(Outcome::Succeeded, "unexpected", vec![])
                },
            )
            .await;
            assert_eq!(outcome.outcome, status);
            assert_eq!(calls.get(), 0);
        }
    }

    #[tokio::test]
    async fn authorized_mock_backend_runs_once_and_requires_readback() {
        let calls = Cell::new(0);
        let outcome = authorized_operation(Ok(()), || async {
            calls.set(calls.get() + 1);
            confirmed_result(
                BackendOutcome::Completed,
                Some((vec!["/run/media/test".into()], true)),
            )
        })
        .await;
        assert_eq!(calls.get(), 1);
        assert_eq!(outcome.outcome, Outcome::Succeeded);
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
}
