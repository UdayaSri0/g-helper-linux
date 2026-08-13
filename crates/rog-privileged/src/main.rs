use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Context;
use rog_core::{
    require_authorized, validate_polkit_action, CpuControlRequest, CpuPowerMode, FanCurve,
    FanCurvePolicy, FanDomain, FanPoint, PrivilegedCapabilities, PrivilegedError,
    PrivilegedErrorCode, RogError, POLKIT_ACTION_CPU_CONTROL, POLKIT_ACTION_FANS_CONTROL,
    PRIVILEGED_DBUS_INTERFACE, PRIVILEGED_DBUS_NAME, PRIVILEGED_DBUS_PATH,
};
use rog_providers::cpu::CpuTelemetryProvider;
use rog_providers::hwmon::{HwmonTelemetryProvider, ASUS_WMI_CURVE_POINTS};
use tokio::time::interval;
use tracing::info;
use zbus::message::Header;
use zbus::zvariant::{OwnedValue, Value};
use zbus::{connection, fdo, interface, Connection, Proxy};

const IDLE_TIMEOUT: Duration = Duration::from_secs(120);
const POLKIT_NAME: &str = "org.freedesktop.PolicyKit1";
const POLKIT_PATH: &str = "/org/freedesktop/PolicyKit1/Authority";
const POLKIT_INTERFACE: &str = "org.freedesktop.PolicyKit1.Authority";
const FAN_SAFETY_MARKER: &str = "/run/rog-helper/fan-control-active";

#[derive(Debug, Clone)]
struct PrivilegedService {
    last_activity: Arc<Mutex<Instant>>,
    cpu: CpuTelemetryProvider,
    fans: HwmonTelemetryProvider,
    fan_safety_marker: PathBuf,
}

impl PrivilegedService {
    fn new() -> Self {
        Self {
            last_activity: Arc::new(Mutex::new(Instant::now())),
            cpu: CpuTelemetryProvider::default(),
            fans: HwmonTelemetryProvider::default(),
            fan_safety_marker: PathBuf::from(FAN_SAFETY_MARKER),
        }
    }

    fn touch(&self) {
        *self.last_activity.lock().expect("activity mutex poisoned") = Instant::now();
    }

    fn idle_for(&self) -> Duration {
        self.last_activity
            .lock()
            .expect("activity mutex poisoned")
            .elapsed()
    }
}

#[interface(name = "io.github.roghelper.Privileged1")]
impl PrivilegedService {
    fn ping(&self) -> bool {
        self.touch();
        true
    }

    fn get_version(&self) -> &str {
        self.touch();
        env!("CARGO_PKG_VERSION")
    }

    #[zbus(out_args("api_version", "categories"))]
    fn get_capabilities(&self) -> (u32, Vec<String>) {
        self.touch();
        PrivilegedCapabilities::cpu_and_fan_control().encode()
    }

    /// Non-interactive diagnostic probe. Real control methods added later must use
    /// the same caller-derived subject with the interactive PolicyKit flag.
    async fn can_perform(
        &self,
        action: &str,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
    ) -> fdo::Result<bool> {
        self.touch();
        validate_polkit_action(action).map_err(|error| {
            fdo::Error::InvalidArgs(format!("{}: {}", error.code.as_str(), error.message))
        })?;
        let sender = header.sender().ok_or_else(|| {
            fdo::Error::Failed("unexpected: D-Bus caller identity is unavailable".to_string())
        })?;
        check_polkit_authorization(connection, sender.as_str(), action, false).await
    }

    async fn set_cpu_turbo(
        &self,
        enabled: bool,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
    ) -> fdo::Result<()> {
        self.apply_cpu(connection, &header, CpuControlRequest::Turbo(enabled))
            .await
    }

    async fn set_cpu_power_mode(
        &self,
        mode: &str,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
    ) -> fdo::Result<()> {
        let mode = CpuPowerMode::parse(mode).map_err(map_cpu_error)?;
        self.apply_cpu(connection, &header, CpuControlRequest::PowerMode(mode))
            .await
    }

    async fn set_cpu_governor(
        &self,
        governor: &str,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
    ) -> fdo::Result<()> {
        self.apply_cpu(
            connection,
            &header,
            CpuControlRequest::Governor(governor.to_string()),
        )
        .await
    }

    async fn set_cpu_epp(
        &self,
        epp: &str,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
    ) -> fdo::Result<()> {
        self.apply_cpu(connection, &header, CpuControlRequest::Epp(epp.to_string()))
            .await
    }

    async fn set_cpu_frequency_limits(
        &self,
        min_mhz: u64,
        max_mhz: u64,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
    ) -> fdo::Result<()> {
        let request = CpuControlRequest::FrequencyLimits {
            min_mhz: optional_u32(min_mhz, "min_mhz")?,
            max_mhz: optional_u32(max_mhz, "max_mhz")?,
        };
        self.apply_cpu(connection, &header, request).await
    }

    async fn set_cpu_core_online(
        &self,
        core_id: u64,
        online: bool,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
    ) -> fdo::Result<()> {
        let core_id = u32::try_from(core_id).map_err(|_| {
            map_privileged_error(PrivilegedError::new(
                PrivilegedErrorCode::InvalidInput,
                "logical CPU id does not fit into u32",
            ))
        })?;
        self.apply_cpu(
            connection,
            &header,
            CpuControlRequest::CoreOnline { core_id, online },
        )
        .await
    }

    async fn set_fan_auto(
        &self,
        fan_id: &str,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
    ) -> fdo::Result<()> {
        self.authorize_fans(connection, &header).await?;
        self.fans
            .set_fan_auto((!fan_id.trim().is_empty()).then_some(fan_id))
            .map_err(map_fan_error)?;
        self.clear_fan_safety_marker().map_err(map_fan_error)
    }

    async fn set_fan_curve(
        &self,
        fan_id: &str,
        points: Vec<(u8, u8)>,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
    ) -> fdo::Result<()> {
        self.authorize_fans(connection, &header).await?;
        let curve = FanCurve {
            domain: fan_domain(fan_id)?,
            points: points
                .into_iter()
                .map(|(temp_c, duty_percent)| FanPoint {
                    temp_c,
                    duty_percent,
                })
                .collect(),
        };
        curve
            .validate_safe(FanCurvePolicy {
                exact_point_count: Some(ASUS_WMI_CURVE_POINTS),
                ..Default::default()
            })
            .map_err(map_fan_error)?;
        if !self
            .fans
            .list_fans()
            .map_err(map_fan_error)?
            .iter()
            .any(|fan| fan.id == fan_id && fan.supports_curve && fan.pwm_endpoint_verified)
        {
            return Err(map_fan_error(RogError::NotSupported(
                "fan mapping is not safe for curve control".to_string(),
            )));
        }
        self.mark_fan_control_active().map_err(map_fan_error)?;
        if let Err(error) = self.fans.set_fan_curve(fan_id, curve) {
            if self.fans.set_fan_auto(None).is_ok() {
                let _ = self.clear_fan_safety_marker();
            }
            return Err(map_fan_error(error));
        }
        Ok(())
    }

    async fn reset_fans_to_auto(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
    ) -> fdo::Result<()> {
        self.authorize_fans(connection, &header).await?;
        self.fans.set_fan_auto(None).map_err(map_fan_error)?;
        self.clear_fan_safety_marker().map_err(map_fan_error)
    }
}

impl PrivilegedService {
    async fn authorize_fans(
        &self,
        connection: &Connection,
        header: &Header<'_>,
    ) -> fdo::Result<()> {
        self.touch();
        let sender = header.sender().ok_or_else(|| {
            map_privileged_error(PrivilegedError::new(
                PrivilegedErrorCode::Unexpected,
                "D-Bus caller identity is unavailable",
            ))
        })?;
        let authorized = check_polkit_authorization(
            connection,
            sender.as_str(),
            POLKIT_ACTION_FANS_CONTROL,
            true,
        )
        .await?;
        require_authorized(authorized).map_err(map_privileged_error)
    }

    fn mark_fan_control_active(&self) -> rog_core::RogResult<()> {
        fs::write(&self.fan_safety_marker, b"asus-wmi-curve\n")
            .map_err(|_| RogError::Unexpected("could not arm the fan fail-safe marker".to_string()))
    }

    fn clear_fan_safety_marker(&self) -> rog_core::RogResult<()> {
        match fs::remove_file(&self.fan_safety_marker) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(RogError::Unexpected(
                "could not clear the fan fail-safe marker".to_string(),
            )),
        }
    }

    fn restore_fans_if_armed(&self) -> rog_core::RogResult<()> {
        if !self.fan_safety_marker.exists() {
            return Ok(());
        }
        self.fans.set_fan_auto(None)?;
        self.clear_fan_safety_marker()
    }

    async fn apply_cpu(
        &self,
        connection: &Connection,
        header: &Header<'_>,
        request: CpuControlRequest,
    ) -> fdo::Result<()> {
        self.touch();
        let sender = header.sender().ok_or_else(|| {
            map_privileged_error(PrivilegedError::new(
                PrivilegedErrorCode::Unexpected,
                "D-Bus caller identity is unavailable",
            ))
        })?;
        let authorized = check_polkit_authorization(
            connection,
            sender.as_str(),
            POLKIT_ACTION_CPU_CONTROL,
            true,
        )
        .await?;
        require_authorized(authorized).map_err(map_privileged_error)?;
        self.cpu.apply_control(&request).map_err(map_cpu_error)
    }
}

fn fan_domain(fan_id: &str) -> fdo::Result<FanDomain> {
    match fan_id {
        "asus-wmi:cpu" => Ok(FanDomain::Cpu),
        "asus-wmi:gpu" => Ok(FanDomain::Gpu),
        "asus-wmi:mid" => Ok(FanDomain::Mid),
        _ => Err(fdo::Error::InvalidArgs(
            "invalid_input: unknown or unsafe fan id".to_string(),
        )),
    }
}

fn optional_u32(value: u64, field: &str) -> fdo::Result<Option<u32>> {
    if value == 0 {
        return Ok(None);
    }
    u32::try_from(value).map(Some).map_err(|_| {
        map_privileged_error(PrivilegedError::new(
            PrivilegedErrorCode::InvalidInput,
            format!("{field} does not fit into u32"),
        ))
    })
}

fn map_cpu_error(error: RogError) -> fdo::Error {
    let privileged = match error {
        RogError::NotSupported(_) => PrivilegedError::new(
            PrivilegedErrorCode::NotSupported,
            "the requested CPU control is not supported by this hardware",
        ),
        RogError::InvalidInput(message) => {
            PrivilegedError::new(PrivilegedErrorCode::InvalidInput, message)
        }
        RogError::TemporarilyUnavailable(_) => PrivilegedError::new(
            PrivilegedErrorCode::HardwareUnavailable,
            "the requested CPU control is temporarily unavailable",
        ),
        RogError::PermissionDenied(_) => PrivilegedError::new(
            PrivilegedErrorCode::PermissionDenied,
            "the privileged CPU endpoint could not be written",
        ),
        RogError::DependencyMissing(_) => PrivilegedError::new(
            PrivilegedErrorCode::BackendFailure,
            "a required CPU backend is unavailable",
        ),
        RogError::TransientFailure(_) | RogError::Unexpected(_) => PrivilegedError::new(
            PrivilegedErrorCode::BackendFailure,
            "the privileged CPU backend failed",
        ),
    };
    map_privileged_error(privileged)
}

fn map_fan_error(error: RogError) -> fdo::Error {
    let privileged = match error {
        RogError::NotSupported(_) => PrivilegedError::new(
            PrivilegedErrorCode::NotSupported,
            "the requested fan control is not safely supported by this hardware",
        ),
        RogError::InvalidInput(message) => {
            PrivilegedError::new(PrivilegedErrorCode::InvalidInput, message)
        }
        RogError::PermissionDenied(_) => PrivilegedError::new(
            PrivilegedErrorCode::PermissionDenied,
            "the verified fan endpoint could not be written",
        ),
        RogError::TemporarilyUnavailable(_) => PrivilegedError::new(
            PrivilegedErrorCode::HardwareUnavailable,
            "the verified fan hardware is temporarily unavailable",
        ),
        RogError::DependencyMissing(_) => PrivilegedError::new(
            PrivilegedErrorCode::BackendFailure,
            "the verified fan backend is unavailable",
        ),
        RogError::TransientFailure(_) => PrivilegedError::new(
            PrivilegedErrorCode::BackendFailure,
            "the privileged fan backend failed and Auto restore was attempted",
        ),
        RogError::Unexpected(_) => PrivilegedError::new(
            PrivilegedErrorCode::Unexpected,
            "the privileged fan write failed and Auto restore was attempted",
        ),
    };
    map_privileged_error(privileged)
}

fn map_privileged_error(error: PrivilegedError) -> fdo::Error {
    let message = format!("{}: {}", error.code.as_str(), error.message);
    match error.code {
        PrivilegedErrorCode::NotAuthorized | PrivilegedErrorCode::PermissionDenied => {
            fdo::Error::AccessDenied(message)
        }
        PrivilegedErrorCode::NotSupported => fdo::Error::NotSupported(message),
        PrivilegedErrorCode::InvalidInput => fdo::Error::InvalidArgs(message),
        _ => fdo::Error::Failed(message),
    }
}

async fn check_polkit_authorization(
    connection: &Connection,
    caller: &str,
    action: &str,
    allow_user_interaction: bool,
) -> fdo::Result<bool> {
    validate_polkit_action(action).map_err(|error| {
        fdo::Error::InvalidArgs(format!("{}: {}", error.code.as_str(), error.message))
    })?;

    let proxy = Proxy::new(connection, POLKIT_NAME, POLKIT_PATH, POLKIT_INTERFACE)
        .await
        .map_err(|_| fdo::Error::Failed("backend_failure: PolicyKit is unavailable".to_string()))?;
    let mut subject_details = HashMap::<String, OwnedValue>::new();
    subject_details.insert("name".to_string(), owned_value(caller.to_string()));
    let subject = ("system-bus-name", subject_details);
    let details = HashMap::<String, String>::new();
    let flags = if allow_user_interaction { 1_u32 } else { 0_u32 };
    let result: (bool, bool, HashMap<String, String>) = proxy
        .call("CheckAuthorization", &(subject, action, details, flags, ""))
        .await
        .map_err(|error| {
            let message = error.to_string().to_ascii_lowercase();
            if message.contains("cancel") || message.contains("notauthorized") {
                fdo::Error::AccessDenied(
                    "not_authorized: PolicyKit authorization was denied or cancelled".to_string(),
                )
            } else {
                fdo::Error::Failed(
                    "backend_failure: PolicyKit authorization check failed".to_string(),
                )
            }
        })?;
    Ok(result.0)
}

fn owned_value<T>(value: T) -> OwnedValue
where
    T: Into<Value<'static>>,
{
    OwnedValue::try_from(value.into()).expect("OwnedValue conversion should succeed")
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,rog_helper=debug".into()),
        )
        .with_target(false)
        .init();

    let service = PrivilegedService::new();
    service
        .restore_fans_if_armed()
        .context("restore fan Auto state left by an interrupted helper")?;
    let idle_service = service.clone();
    let connection = connection::Builder::system()
        .context("connect to the system D-Bus")?
        .name(PRIVILEGED_DBUS_NAME)
        .context("claim privileged D-Bus name")?
        .serve_at(PRIVILEGED_DBUS_PATH, service)
        .context("export privileged D-Bus object")?
        .build()
        .await
        .context("start privileged D-Bus service")?;

    info!(
        "privileged D-Bus service online: {PRIVILEGED_DBUS_NAME} {PRIVILEGED_DBUS_PATH} ({PRIVILEGED_DBUS_INTERFACE})"
    );
    let mut idle_check = interval(Duration::from_secs(15));
    loop {
        tokio::select! {
            signal = tokio::signal::ctrl_c() => {
                signal.context("listen for shutdown signal")?;
                break;
            }
            _ = idle_check.tick() => {
                if idle_service.idle_for() >= IDLE_TIMEOUT {
                    idle_service
                        .restore_fans_if_armed()
                        .context("restore fans before privileged helper idle exit")?;
                    info!("privileged helper idle timeout reached; exiting");
                    break;
                }
            }
        }
    }
    idle_service
        .restore_fans_if_armed()
        .context("restore fans before privileged helper shutdown")?;
    drop(connection);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rog_core::{
        require_authorized, PrivilegedErrorCode, POLKIT_ACTION_CPU_CONTROL,
        POLKIT_ACTION_SYSTEM_CONFIGURE,
    };

    #[test]
    fn helper_advertises_only_implemented_write_categories() {
        let (_, categories) = PrivilegedCapabilities::cpu_and_fan_control().encode();
        assert_eq!(categories, vec!["cpu", "fans"]);
    }

    #[test]
    fn authorization_failure_maps_to_not_authorized() {
        assert_eq!(
            require_authorized(false).unwrap_err().code,
            PrivilegedErrorCode::NotAuthorized
        );
    }

    #[test]
    fn intended_probe_actions_are_allow_listed() {
        assert!(validate_polkit_action(POLKIT_ACTION_CPU_CONTROL).is_ok());
        assert!(validate_polkit_action(POLKIT_ACTION_FANS_CONTROL).is_ok());
        assert!(validate_polkit_action(POLKIT_ACTION_SYSTEM_CONFIGURE).is_ok());
    }
}
