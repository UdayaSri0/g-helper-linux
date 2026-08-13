//! Shared, path-free contract for the privileged system helper.

use serde::{Deserialize, Serialize};

pub const PRIVILEGED_DBUS_NAME: &str = "io.github.roghelper.Privileged";
pub const PRIVILEGED_DBUS_PATH: &str = "/io/github/roghelper/Privileged";
pub const PRIVILEGED_DBUS_INTERFACE: &str = "io.github.roghelper.Privileged1";
pub const PRIVILEGED_API_VERSION: u32 = 1;

pub const POLKIT_ACTION_CPU_CONTROL: &str = "io.github.roghelper.cpu.control";
pub const POLKIT_ACTION_FANS_CONTROL: &str = "io.github.roghelper.fans.control";
pub const POLKIT_ACTION_LIGHTING_CONTROL: &str = "io.github.roghelper.lighting.control";
pub const POLKIT_ACTION_SYSTEM_CONFIGURE: &str = "io.github.roghelper.system.configure";

pub const POLKIT_ACTIONS: [&str; 4] = [
    POLKIT_ACTION_CPU_CONTROL,
    POLKIT_ACTION_FANS_CONTROL,
    POLKIT_ACTION_LIGHTING_CONTROL,
    POLKIT_ACTION_SYSTEM_CONFIGURE,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrivilegedCategory {
    Cpu,
    Fans,
    Lighting,
    System,
}

impl PrivilegedCategory {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Fans => "fans",
            Self::Lighting => "lighting",
            Self::System => "system",
        }
    }

    pub fn parse(value: &str) -> Result<Self, PrivilegedError> {
        match value {
            "cpu" => Ok(Self::Cpu),
            "fans" => Ok(Self::Fans),
            "lighting" => Ok(Self::Lighting),
            "system" => Ok(Self::System),
            _ => Err(PrivilegedError::new(
                PrivilegedErrorCode::InvalidInput,
                "unknown privileged capability category",
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrivilegedCapabilities {
    pub api_version: u32,
    pub categories: Vec<PrivilegedCategory>,
}

impl PrivilegedCapabilities {
    /// Phase 1 deliberately advertises no hardware-write backends.
    pub const fn foundation_only() -> Self {
        Self {
            api_version: PRIVILEGED_API_VERSION,
            categories: Vec::new(),
        }
    }

    pub fn cpu_control() -> Self {
        Self {
            api_version: PRIVILEGED_API_VERSION,
            categories: vec![PrivilegedCategory::Cpu],
        }
    }

    pub fn cpu_and_fan_control() -> Self {
        Self {
            api_version: PRIVILEGED_API_VERSION,
            categories: vec![PrivilegedCategory::Cpu, PrivilegedCategory::Fans],
        }
    }

    pub fn implemented_controls() -> Self {
        Self {
            api_version: PRIVILEGED_API_VERSION,
            categories: vec![
                PrivilegedCategory::Cpu,
                PrivilegedCategory::Fans,
                PrivilegedCategory::Lighting,
            ],
        }
    }

    pub fn supports(&self, category: PrivilegedCategory) -> bool {
        self.api_version == PRIVILEGED_API_VERSION && self.categories.contains(&category)
    }

    pub fn encode(&self) -> (u32, Vec<String>) {
        (
            self.api_version,
            self.categories
                .iter()
                .map(|category| category.as_str().to_string())
                .collect(),
        )
    }

    pub fn decode(api_version: u32, categories: Vec<String>) -> Result<Self, PrivilegedError> {
        if api_version == 0 {
            return Err(PrivilegedError::new(
                PrivilegedErrorCode::InvalidInput,
                "privileged API version must be non-zero",
            ));
        }
        let mut decoded = Vec::with_capacity(categories.len());
        for value in categories {
            let category = PrivilegedCategory::parse(&value)?;
            if decoded.contains(&category) {
                return Err(PrivilegedError::new(
                    PrivilegedErrorCode::InvalidInput,
                    "duplicate privileged capability category",
                ));
            }
            decoded.push(category);
        }
        Ok(Self {
            api_version,
            categories: decoded,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrivilegedErrorCode {
    NotAuthorized,
    NotSupported,
    InvalidInput,
    HardwareUnavailable,
    PermissionDenied,
    BackendFailure,
    Unexpected,
}

impl PrivilegedErrorCode {
    pub const ALL: [Self; 7] = [
        Self::NotAuthorized,
        Self::NotSupported,
        Self::InvalidInput,
        Self::HardwareUnavailable,
        Self::PermissionDenied,
        Self::BackendFailure,
        Self::Unexpected,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotAuthorized => "not_authorized",
            Self::NotSupported => "not_supported",
            Self::InvalidInput => "invalid_input",
            Self::HardwareUnavailable => "hardware_unavailable",
            Self::PermissionDenied => "permission_denied",
            Self::BackendFailure => "backend_failure",
            Self::Unexpected => "unexpected",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[error("{code}: {message}")]
pub struct PrivilegedError {
    pub code: PrivilegedErrorCode,
    /// A sanitized, user-facing message. Backend/root error strings must not be copied here.
    pub message: String,
}

impl PrivilegedError {
    pub fn new(code: PrivilegedErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for PrivilegedErrorCode {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

pub fn validate_polkit_action(action: &str) -> Result<&str, PrivilegedError> {
    let action = action.trim();
    if POLKIT_ACTIONS.contains(&action) {
        Ok(action)
    } else {
        Err(PrivilegedError::new(
            PrivilegedErrorCode::InvalidInput,
            "action is not part of the privileged helper allow-list",
        ))
    }
}

pub fn require_authorized(authorized: bool) -> Result<(), PrivilegedError> {
    if authorized {
        Ok(())
    } else {
        Err(PrivilegedError::new(
            PrivilegedErrorCode::NotAuthorized,
            "PolicyKit did not authorize this operation",
        ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorizationState {
    Authorized,
    Denied,
    Unavailable,
    NotChecked,
}

impl AuthorizationState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Authorized => "authorized",
            Self::Denied => "denied",
            Self::Unavailable => "unavailable",
            Self::NotChecked => "not_checked",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrivilegedStatus {
    pub privileged_helper_installed: bool,
    pub privileged_helper_reachable: bool,
    pub privileged_helper_compatible: bool,
    pub privileged_helper_version: Option<String>,
    pub polkit_available: bool,
    pub authorization_backend: String,
    pub authorization_state: AuthorizationState,
    pub privileged_categories_available: Vec<PrivilegedCategory>,
}

impl PrivilegedStatus {
    pub fn unavailable(installed: bool, polkit_available: bool) -> Self {
        Self {
            privileged_helper_installed: installed,
            privileged_helper_reachable: false,
            privileged_helper_compatible: false,
            privileged_helper_version: None,
            polkit_available,
            authorization_backend: if polkit_available {
                "polkit".to_string()
            } else {
                "unavailable".to_string()
            },
            authorization_state: AuthorizationState::Unavailable,
            privileged_categories_available: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dbus_identity_is_stable() {
        assert_eq!(PRIVILEGED_DBUS_NAME, "io.github.roghelper.Privileged");
        assert_eq!(PRIVILEGED_DBUS_PATH, "/io/github/roghelper/Privileged");
        assert_eq!(PRIVILEGED_DBUS_INTERFACE, "io.github.roghelper.Privileged1");
    }

    #[test]
    fn action_validation_accepts_only_application_actions() {
        for action in POLKIT_ACTIONS {
            assert_eq!(validate_polkit_action(action), Ok(action));
        }
        assert_eq!(
            validate_polkit_action("io.github.roghelper.root")
                .unwrap_err()
                .code,
            PrivilegedErrorCode::InvalidInput
        );
        assert_eq!(
            validate_polkit_action("org.example.other")
                .unwrap_err()
                .code,
            PrivilegedErrorCode::InvalidInput
        );
    }

    #[test]
    fn capabilities_round_trip_and_reject_unknown_values() {
        let original = PrivilegedCapabilities {
            api_version: 1,
            categories: vec![PrivilegedCategory::Cpu, PrivilegedCategory::Fans],
        };
        let (version, categories) = original.encode();
        assert_eq!(
            PrivilegedCapabilities::decode(version, categories).unwrap(),
            original
        );
        assert_eq!(
            PrivilegedCapabilities::decode(1, vec!["filesystem".to_string()])
                .unwrap_err()
                .code,
            PrivilegedErrorCode::InvalidInput
        );
    }

    #[test]
    fn denied_authorization_has_a_stable_error() {
        let error = require_authorized(false).unwrap_err();
        assert_eq!(error.code, PrivilegedErrorCode::NotAuthorized);
        assert_eq!(error.code.as_str(), "not_authorized");
    }

    #[test]
    fn every_privileged_error_has_a_stable_wire_code() {
        assert_eq!(
            PrivilegedErrorCode::ALL.map(PrivilegedErrorCode::as_str),
            [
                "not_authorized",
                "not_supported",
                "invalid_input",
                "hardware_unavailable",
                "permission_denied",
                "backend_failure",
                "unexpected",
            ]
        );
    }

    #[test]
    fn unavailable_status_keeps_privileged_support_optional() {
        let status = PrivilegedStatus::unavailable(false, true);
        assert!(!status.privileged_helper_installed);
        assert!(!status.privileged_helper_reachable);
        assert_eq!(status.authorization_backend, "polkit");
        assert_eq!(status.authorization_state, AuthorizationState::Unavailable);
        assert!(status.privileged_categories_available.is_empty());
    }
}
