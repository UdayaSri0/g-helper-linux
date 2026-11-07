/// Shared typed identity carried by the session daemon's `GetDaemonInfo` map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonApiInfo {
    pub api_version: u32,
    pub package_version: String,
}

impl DaemonApiInfo {
    pub fn current(package_version: impl Into<String>) -> Self {
        Self {
            api_version: crate::dbus_keys::DAEMON_API_VERSION,
            package_version: package_version.into(),
        }
    }

    pub fn is_compatible_with(&self, api_version: u32, package_version: &str) -> bool {
        self.api_version == api_version && self.package_version == package_version
    }

    pub fn to_wire_fields(&self) -> (u64, String) {
        (u64::from(self.api_version), self.package_version.clone())
    }

    pub fn from_wire_fields(
        api_version: Option<u64>,
        package_version: Option<String>,
    ) -> Option<Self> {
        Some(Self {
            api_version: u32::try_from(api_version?).ok()?,
            package_version: package_version?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daemon_identity_requires_exact_api_and_package_versions() {
        let info = DaemonApiInfo::current("0.4.0");
        assert!(info.is_compatible_with(crate::dbus_keys::DAEMON_API_VERSION, "0.4.0"));
        assert!(!info.is_compatible_with(crate::dbus_keys::DAEMON_API_VERSION + 1, "0.4.0"));
        assert!(!info.is_compatible_with(crate::dbus_keys::DAEMON_API_VERSION, "0.3.0"));
    }

    #[test]
    fn daemon_api_fields_round_trip_and_reject_missing_or_unrepresentable_values() {
        let original = DaemonApiInfo::current("0.4.0");
        let (api_version, package_version) = original.to_wire_fields();
        assert_eq!(
            DaemonApiInfo::from_wire_fields(Some(api_version), Some(package_version)),
            Some(original)
        );
        assert!(DaemonApiInfo::from_wire_fields(None, Some("0.4.0".to_string())).is_none());
        assert!(
            DaemonApiInfo::from_wire_fields(Some(u64::MAX), Some("0.4.0".to_string())).is_none()
        );
    }
}
