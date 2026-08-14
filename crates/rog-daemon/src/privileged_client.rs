use std::time::Duration;

use rog_core::{
    AuthorizationState, CpuControlRequest, FanCurve, PrivilegedCapabilities, PrivilegedError,
    PrivilegedErrorCode, PrivilegedStatus, POLKIT_ACTION_CPU_CONTROL, PRIVILEGED_API_VERSION,
    PRIVILEGED_DBUS_INTERFACE, PRIVILEGED_DBUS_NAME, PRIVILEGED_DBUS_PATH,
};
use tokio::time::timeout;
use zbus::{Connection, Proxy};

const PROBE_TIMEOUT: Duration = Duration::from_secs(3);
const DBUS_NAME: &str = "org.freedesktop.DBus";
const DBUS_PATH: &str = "/org/freedesktop/DBus";
const DBUS_INTERFACE: &str = "org.freedesktop.DBus";
const POLKIT_NAME: &str = "org.freedesktop.PolicyKit1";

pub async fn probe() -> PrivilegedStatus {
    match timeout(PROBE_TIMEOUT, probe_inner()).await {
        Ok(status) => status,
        Err(_) => PrivilegedStatus::unavailable(false, false),
    }
}

async fn probe_inner() -> PrivilegedStatus {
    let Ok(connection) = Connection::system().await else {
        return PrivilegedStatus::unavailable(false, false);
    };
    let Ok(bus) = Proxy::new(&connection, DBUS_NAME, DBUS_PATH, DBUS_INTERFACE).await else {
        let mut status = PrivilegedStatus::unavailable(false, false);
        status.system_bus_connected = true;
        return status;
    };

    let activatable: Vec<String> = bus
        .call("ListActivatableNames", &())
        .await
        .unwrap_or_default();
    let helper_owned: bool = bus
        .call("NameHasOwner", &(PRIVILEGED_DBUS_NAME,))
        .await
        .unwrap_or(false);
    let polkit_owned: bool = bus
        .call("NameHasOwner", &(POLKIT_NAME,))
        .await
        .unwrap_or(false);
    let helper_installed =
        helper_owned || activatable.iter().any(|name| name == PRIVILEGED_DBUS_NAME);
    let polkit_available = polkit_owned || activatable.iter().any(|name| name == POLKIT_NAME);

    let Ok(helper) = Proxy::new(
        &connection,
        PRIVILEGED_DBUS_NAME,
        PRIVILEGED_DBUS_PATH,
        PRIVILEGED_DBUS_INTERFACE,
    )
    .await
    else {
        let mut status = PrivilegedStatus::unavailable(helper_installed, polkit_available);
        status.system_bus_connected = true;
        return status;
    };

    let ping: Result<bool, _> = helper.call("Ping", &()).await;
    if !matches!(ping, Ok(true)) {
        let mut status = PrivilegedStatus::unavailable(helper_installed, polkit_available);
        status.system_bus_connected = true;
        return status;
    }

    let version = helper.call::<_, _, String>("GetVersion", &()).await.ok();
    let capabilities = helper
        .call::<_, _, (u32, Vec<String>)>("GetCapabilities", &())
        .await
        .ok()
        .and_then(|(api_version, categories)| {
            PrivilegedCapabilities::decode(api_version, categories).ok()
        });
    let compatible = capabilities
        .as_ref()
        .is_some_and(|caps| caps.api_version == PRIVILEGED_API_VERSION);
    let categories = capabilities
        .filter(|caps| caps.api_version == PRIVILEGED_API_VERSION)
        .map(|caps| caps.categories)
        .unwrap_or_default();
    let authorization_state = if !polkit_available {
        AuthorizationState::Unavailable
    } else {
        match helper
            .call::<_, _, bool>("CanPerform", &(POLKIT_ACTION_CPU_CONTROL,))
            .await
        {
            Ok(true) => AuthorizationState::Authorized,
            Ok(false) => AuthorizationState::NotChecked,
            Err(_) => AuthorizationState::Unavailable,
        }
    };

    PrivilegedStatus {
        system_bus_connected: true,
        privileged_helper_installed: true,
        privileged_helper_reachable: true,
        privileged_helper_compatible: compatible,
        privileged_helper_version: version,
        polkit_available,
        authorization_backend: if polkit_available {
            "polkit".to_string()
        } else {
            "unavailable".to_string()
        },
        authorization_state,
        privileged_categories_available: categories,
    }
}

pub async fn apply_cpu(request: &CpuControlRequest) -> Result<(), PrivilegedError> {
    let connection = Connection::system()
        .await
        .map_err(|_| helper_unavailable())?;
    let helper = Proxy::new(
        &connection,
        PRIVILEGED_DBUS_NAME,
        PRIVILEGED_DBUS_PATH,
        PRIVILEGED_DBUS_INTERFACE,
    )
    .await
    .map_err(|_| helper_unavailable())?;
    let result: zbus::Result<()> = match request {
        CpuControlRequest::Turbo(enabled) => helper.call("SetCpuTurbo", &(*enabled,)).await,
        CpuControlRequest::PowerMode(mode) => {
            let value = match mode {
                rog_core::CpuPowerMode::Quiet => "Quiet",
                rog_core::CpuPowerMode::Balanced => "Balanced",
                rog_core::CpuPowerMode::Performance => "Performance",
            };
            helper.call("SetCpuPowerMode", &(value,)).await
        }
        CpuControlRequest::Governor(governor) => {
            helper.call("SetCpuGovernor", &(governor.as_str(),)).await
        }
        CpuControlRequest::Epp(epp) => helper.call("SetCpuEpp", &(epp.as_str(),)).await,
        CpuControlRequest::FrequencyLimits { min_mhz, max_mhz } => {
            helper
                .call(
                    "SetCpuFrequencyLimits",
                    &(
                        u64::from(min_mhz.unwrap_or(0)),
                        u64::from(max_mhz.unwrap_or(0)),
                    ),
                )
                .await
        }
        CpuControlRequest::CoreOnline { core_id, online } => {
            helper
                .call("SetCpuCoreOnline", &(u64::from(*core_id), *online))
                .await
        }
    };
    result.map_err(map_zbus_error)
}

pub async fn set_fan_auto(fan_id: &str) -> Result<(), PrivilegedError> {
    call_fan("SetFanAuto", &(fan_id,)).await
}

pub async fn set_fan_curve(fan_id: &str, curve: &FanCurve) -> Result<(), PrivilegedError> {
    let points = curve
        .points
        .iter()
        .map(|point| (point.temp_c, point.duty_percent))
        .collect::<Vec<_>>();
    call_fan("SetFanCurve", &(fan_id, points)).await
}

pub async fn reset_fans_to_auto() -> Result<(), PrivilegedError> {
    call_fan("ResetFansToAuto", &()).await
}

pub async fn set_keyboard_backlight_brightness(level: u32) -> Result<(), PrivilegedError> {
    let connection = Connection::system()
        .await
        .map_err(|_| lighting_helper_unavailable())?;
    let helper = Proxy::new(
        &connection,
        PRIVILEGED_DBUS_NAME,
        PRIVILEGED_DBUS_PATH,
        PRIVILEGED_DBUS_INTERFACE,
    )
    .await
    .map_err(|_| lighting_helper_unavailable())?;
    helper
        .call::<_, _, ()>("SetKeyboardBacklightBrightness", &(u64::from(level),))
        .await
        .map_err(|error| map_zbus_error_or(error, lighting_helper_unavailable))
}

pub async fn set_battery_charge_limit(limit: u8) -> Result<u8, PrivilegedError> {
    let connection = Connection::system()
        .await
        .map_err(|_| battery_helper_unavailable())?;
    let helper = Proxy::new(
        &connection,
        PRIVILEGED_DBUS_NAME,
        PRIVILEGED_DBUS_PATH,
        PRIVILEGED_DBUS_INTERFACE,
    )
    .await
    .map_err(|_| battery_helper_unavailable())?;
    let actual = helper
        .call::<_, _, u64>("SetBatteryChargeLimit", &(u64::from(limit),))
        .await
        .map_err(|error| map_zbus_error_or(error, battery_helper_unavailable))?;
    u8::try_from(actual).map_err(|_| {
        PrivilegedError::new(
            PrivilegedErrorCode::Unexpected,
            "the privileged battery helper returned an invalid percentage",
        )
    })
}

async fn call_fan<B>(method: &str, body: &B) -> Result<(), PrivilegedError>
where
    B: serde::ser::Serialize + zbus::zvariant::DynamicType,
{
    let connection = Connection::system()
        .await
        .map_err(|_| fan_helper_unavailable())?;
    let helper = Proxy::new(
        &connection,
        PRIVILEGED_DBUS_NAME,
        PRIVILEGED_DBUS_PATH,
        PRIVILEGED_DBUS_INTERFACE,
    )
    .await
    .map_err(|_| fan_helper_unavailable())?;
    helper
        .call::<_, _, ()>(method, body)
        .await
        .map_err(map_zbus_error)
}

fn helper_unavailable() -> PrivilegedError {
    PrivilegedError::new(
        PrivilegedErrorCode::BackendFailure,
        "the privileged CPU helper is unavailable",
    )
}

fn fan_helper_unavailable() -> PrivilegedError {
    PrivilegedError::new(
        PrivilegedErrorCode::BackendFailure,
        "the privileged fan helper is unavailable",
    )
}

fn lighting_helper_unavailable() -> PrivilegedError {
    PrivilegedError::new(
        PrivilegedErrorCode::BackendFailure,
        "the privileged lighting helper is unavailable",
    )
}

fn battery_helper_unavailable() -> PrivilegedError {
    PrivilegedError::new(
        PrivilegedErrorCode::BackendFailure,
        "the privileged battery helper is unavailable",
    )
}

fn map_zbus_error_or(error: zbus::Error, fallback: fn() -> PrivilegedError) -> PrivilegedError {
    let text = error.to_string();
    for code in PrivilegedErrorCode::ALL {
        let marker = format!("{}:", code.as_str());
        if let Some((_, message)) = text.split_once(&marker) {
            return PrivilegedError::new(code, message.trim());
        }
    }
    fallback()
}

fn map_zbus_error(error: zbus::Error) -> PrivilegedError {
    map_zbus_error_or(error, helper_unavailable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_helper_status_does_not_disable_the_daemon() {
        let status = PrivilegedStatus::unavailable(false, true);
        assert!(!status.privileged_helper_reachable);
        assert_eq!(status.authorization_state, AuthorizationState::Unavailable);
        assert!(status.privileged_categories_available.is_empty());
    }
}
