use std::fs;
use std::path::{Path, PathBuf};

use rog_core::{FanWritePolicyDecision, FanWriteSupportState};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FanHardwareIdentity {
    pub system_vendor: String,
    pub product_name: String,
    pub product_version: String,
    pub board_vendor: String,
    pub board_name: String,
    pub bios_vendor: String,
    pub bios_version: String,
    pub kernel_release: String,
    pub kernel_build: String,
    pub driver_name: String,
    pub driver_version: String,
}

#[derive(Debug, Clone)]
pub struct FanIdentityPaths {
    pub dmi: PathBuf,
    pub kernel_release: PathBuf,
    pub kernel_build: PathBuf,
    pub driver: PathBuf,
}

impl Default for FanIdentityPaths {
    fn default() -> Self {
        Self {
            dmi: PathBuf::from("/sys/class/dmi/id"),
            kernel_release: PathBuf::from("/proc/sys/kernel/osrelease"),
            kernel_build: PathBuf::from("/proc/version"),
            driver: PathBuf::from("/sys/module/asus_nb_wmi"),
        }
    }
}

impl FanIdentityPaths {
    pub fn read(&self) -> Result<FanHardwareIdentity, String> {
        Ok(FanHardwareIdentity {
            system_vendor: required(&self.dmi.join("sys_vendor"), "system vendor")?,
            product_name: required(&self.dmi.join("product_name"), "product name")?,
            product_version: required(&self.dmi.join("product_version"), "product version")?,
            board_vendor: required(&self.dmi.join("board_vendor"), "board vendor")?,
            board_name: required(&self.dmi.join("board_name"), "board name")?,
            bios_vendor: required(&self.dmi.join("bios_vendor"), "BIOS vendor")?,
            bios_version: required(&self.dmi.join("bios_version"), "BIOS version")?,
            kernel_release: required(&self.kernel_release, "kernel release")?,
            kernel_build: required(&self.kernel_build, "kernel build")?,
            driver_name: self
                .driver
                .file_name()
                .and_then(|value| value.to_str())
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "driver identity is unavailable".to_string())?
                .to_string(),
            driver_version: required(&self.driver.join("srcversion"), "driver version")?,
        })
    }
}

fn required(path: &Path, label: &str) -> Result<String, String> {
    fs::read_to_string(path)
        .map(|value| value.trim().to_string())
        .ok()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{label} is missing or unreadable"))
}

#[derive(Debug, Clone, Copy)]
pub struct VerifiedFanWriteRecord {
    pub record_id: &'static str,
    pub system_vendor: &'static str,
    pub product_name: &'static str,
    pub product_version: &'static str,
    pub board_vendor: &'static str,
    pub board_name: &'static str,
    pub bios_vendor: &'static str,
    pub bios_version: &'static str,
    pub kernel_release: &'static str,
    pub kernel_build: &'static str,
    pub driver_name: &'static str,
    pub driver_version: &'static str,
    pub fan_id: &'static str,
    pub channel: u32,
    pub channel_label: &'static str,
    pub backend: &'static str,
    pub point_count: usize,
    pub minimum_duty_percent: u8,
    pub minimum_temperature_c: u8,
    pub maximum_temperature_c: u8,
    pub terminal_temperature_c: u8,
    pub readback_semantics: &'static str,
    pub auto_restore_semantics: &'static str,
    pub restart_recovery: &'static str,
    pub suspend_resume_behavior: &'static str,
    pub evidence: &'static str,
    pub tested_on: &'static str,
    pub tester: &'static str,
    pub reviewed_by: &'static str,
    pub approved: bool,
}

/// No physical custom-fan-write validation record is currently committed.
/// Adding an entry requires dated evidence and maintainer review.
pub const PRODUCTION_FAN_WRITE_RECORDS: &[VerifiedFanWriteRecord] = &[];

#[cfg(test)]
pub(crate) static TEST_FAN_WRITE_RECORDS: &[VerifiedFanWriteRecord] = &[VerifiedFanWriteRecord {
    record_id: "test-only-record",
    system_vendor: "ASUSTeK COMPUTER INC.",
    product_name: "TEST-MODEL",
    product_version: "1.0",
    board_vendor: "ASUSTeK COMPUTER INC.",
    board_name: "TEST-BOARD",
    bios_vendor: "American Megatrends International, LLC.",
    bios_version: "TEST.100",
    kernel_release: "6.99.0-test",
    kernel_build: "Linux test build",
    driver_name: "asus_nb_wmi",
    driver_version: "TESTDRIVER",
    fan_id: "asus-wmi:cpu",
    channel: 1,
    channel_label: "cpu_fan",
    backend: "asus-wmi-curve",
    point_count: 8,
    minimum_duty_percent: 20,
    minimum_temperature_c: 30,
    maximum_temperature_c: 100,
    terminal_temperature_c: 95,
    readback_semantics: "exact raw PWM readback",
    auto_restore_semantics: "pwm enable mode 2",
    restart_recovery: "test marker restores Auto",
    suspend_resume_behavior: "test fixture remains exact",
    evidence: "test-only fixture",
    tested_on: "2099-01-01",
    tester: "automated-test",
    reviewed_by: "automated-test",
    approved: true,
}];

#[derive(Debug, Clone, Copy)]
pub struct FanWriteContext<'a> {
    pub fan_id: &'a str,
    pub channel: u32,
    pub channel_label: &'a str,
    pub abi_detected: bool,
    pub curve_readable: bool,
    pub backend_writable: bool,
    pub recovery_available: bool,
}

pub fn evaluate(
    paths: &FanIdentityPaths,
    records: &[VerifiedFanWriteRecord],
    context: FanWriteContext<'_>,
) -> FanWritePolicyDecision {
    if !context.abi_detected {
        return decision(
            FanWriteSupportState::ReadOnly,
            "curve_abi_not_detected",
            "The ASUS custom fan-curve ABI was not structurally recognized.",
            context,
            false,
        );
    }
    let identity = match paths.read() {
        Ok(identity) => identity,
        Err(error) => {
            let explanation = format!(
                "Custom Apply is blocked because runtime identity evidence is incomplete: {error}."
            );
            return decision(
                FanWriteSupportState::Unverified,
                "identity_incomplete",
                &explanation,
                context,
                false,
            );
        }
    };
    if !identity.system_vendor.to_ascii_lowercase().contains("asus")
        || !identity.board_vendor.to_ascii_lowercase().contains("asus")
    {
        return decision(
            FanWriteSupportState::Unsupported,
            "manufacturer_not_supported",
            "The runtime DMI identity is not an approved ASUS platform.",
            context,
            false,
        );
    }
    let matches = records
        .iter()
        .filter(|record| record.matches(&identity, context))
        .collect::<Vec<_>>();
    if matches.len() > 1 {
        return decision(
            FanWriteSupportState::Unverified,
            "ambiguous_validation_records",
            "Multiple support records match the current environment; custom Apply is blocked until the policy is unambiguous.",
            context,
            false,
        );
    }
    let Some(record) = matches.into_iter().next() else {
        return decision(
            FanWriteSupportState::Unverified,
            "no_exact_validation_record",
            "The writable kernel ABI is present, but this exact model, board, BIOS, kernel, driver, and channel combination has no approved physical validation record.",
            context,
            false,
        );
    };
    if !record.approved || record.evidence.is_empty() || record.reviewed_by.is_empty() {
        return decision(
            FanWriteSupportState::Unverified,
            "record_not_approved",
            "A matching support record exists but is not eligible for production authorization.",
            context,
            false,
        );
    }
    decision_with_minimum(
        FanWriteSupportState::VerifiedWrite,
        if context.backend_writable {
            "exact_record_match"
        } else {
            "exact_record_match_helper_required"
        },
        &format!(
            "Eligible under reviewed physical validation record {}; the current process {} direct endpoint access.",
            record.record_id,
            if context.backend_writable { "has" } else { "requires privileged" }
        ),
        context,
        true,
        Some(record),
    )
}

fn decision(
    state: FanWriteSupportState,
    reason: &str,
    explanation: &str,
    context: FanWriteContext<'_>,
    physical_validation_recorded: bool,
) -> FanWritePolicyDecision {
    decision_with_minimum(
        state,
        reason,
        explanation,
        context,
        physical_validation_recorded,
        None,
    )
}

fn decision_with_minimum(
    state: FanWriteSupportState,
    reason: &str,
    explanation: &str,
    context: FanWriteContext<'_>,
    physical_validation_recorded: bool,
    record: Option<&VerifiedFanWriteRecord>,
) -> FanWritePolicyDecision {
    let custom_curve_authorized =
        state == FanWriteSupportState::VerifiedWrite && context.backend_writable;
    FanWritePolicyDecision {
        state,
        reason: reason.to_string(),
        explanation: explanation.to_string(),
        abi_detected: context.abi_detected,
        curve_readable: context.curve_readable,
        backend_writable: context.backend_writable,
        helper_ready: false,
        physical_validation_recorded,
        custom_curve_authorized,
        recovery_available: context.recovery_available,
        minimum_duty_percent: record.map(|value| value.minimum_duty_percent),
        minimum_temperature_c: record.map(|value| value.minimum_temperature_c),
        maximum_temperature_c: record.map(|value| value.maximum_temperature_c),
        terminal_temperature_c: record.map(|value| value.terminal_temperature_c),
    }
}

impl VerifiedFanWriteRecord {
    fn matches(&self, identity: &FanHardwareIdentity, context: FanWriteContext<'_>) -> bool {
        self.system_vendor == identity.system_vendor
            && self.product_name == identity.product_name
            && self.product_version == identity.product_version
            && self.board_vendor == identity.board_vendor
            && self.board_name == identity.board_name
            && self.bios_vendor == identity.bios_vendor
            && self.bios_version == identity.bios_version
            && self.kernel_release == identity.kernel_release
            && self.kernel_build == identity.kernel_build
            && self.driver_name == identity.driver_name
            && self.driver_version == identity.driver_version
            && self.fan_id == context.fan_id
            && self.channel == context.channel
            && self.channel_label == context.channel_label
            && self.backend == "asus-wmi-curve"
            && self.point_count == 8
            && !self.readback_semantics.is_empty()
            && !self.auto_restore_semantics.is_empty()
            && !self.restart_recovery.is_empty()
            && !self.suspend_resume_behavior.is_empty()
            && !self.tested_on.is_empty()
            && !self.tester.is_empty()
            && (1..=100).contains(&self.minimum_duty_percent)
            && self.minimum_temperature_c < self.maximum_temperature_c
            && self.terminal_temperature_c >= self.minimum_temperature_c
            && self.terminal_temperature_c <= self.maximum_temperature_c
            && self.minimum_temperature_c <= self.maximum_temperature_c
            && self.maximum_temperature_c <= 100
            && self.terminal_temperature_c <= self.maximum_temperature_c
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn fixture() -> (PathBuf, FanIdentityPaths) {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("rog-fan-policy-{stamp}"));
        let dmi = root.join("dmi");
        let driver = root.join("asus_nb_wmi");
        fs::create_dir_all(&dmi).unwrap();
        fs::create_dir_all(&driver).unwrap();
        for (name, value) in [
            ("sys_vendor", "ASUSTeK COMPUTER INC."),
            ("product_name", "TEST-MODEL"),
            ("product_version", "1.0"),
            ("board_vendor", "ASUSTeK COMPUTER INC."),
            ("board_name", "TEST-BOARD"),
            ("bios_vendor", "American Megatrends International, LLC."),
            ("bios_version", "TEST.100"),
        ] {
            fs::write(dmi.join(name), value).unwrap();
        }
        fs::write(root.join("osrelease"), "6.99.0-test").unwrap();
        fs::write(root.join("version"), "Linux test build").unwrap();
        fs::write(driver.join("srcversion"), "TESTDRIVER").unwrap();
        let paths = FanIdentityPaths {
            dmi,
            kernel_release: root.join("osrelease"),
            kernel_build: root.join("version"),
            driver,
        };
        (root, paths)
    }

    fn context() -> FanWriteContext<'static> {
        FanWriteContext {
            fan_id: "asus-wmi:cpu",
            channel: 1,
            channel_label: "cpu_fan",
            abi_detected: true,
            curve_readable: true,
            backend_writable: true,
            recovery_available: true,
        }
    }

    #[test]
    fn empty_production_records_fail_closed() {
        let (root, paths) = fixture();
        let result = evaluate(&paths, PRODUCTION_FAN_WRITE_RECORDS, context());
        assert_eq!(result.state, FanWriteSupportState::Unverified);
        assert!(!result.custom_curve_authorized);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn exact_test_record_authorizes_only_its_channel() {
        let (root, paths) = fixture();
        let accepted = evaluate(&paths, TEST_FAN_WRITE_RECORDS, context());
        assert_eq!(accepted.state, FanWriteSupportState::VerifiedWrite);
        assert!(accepted.custom_curve_authorized);
        let rejected = evaluate(
            &paths,
            TEST_FAN_WRITE_RECORDS,
            FanWriteContext {
                fan_id: "asus-wmi:gpu",
                channel: 2,
                ..context()
            },
        );
        assert!(!rejected.custom_curve_authorized);
        let helper_required = evaluate(
            &paths,
            TEST_FAN_WRITE_RECORDS,
            FanWriteContext {
                backend_writable: false,
                ..context()
            },
        );
        assert_eq!(helper_required.state, FanWriteSupportState::VerifiedWrite);
        assert!(!helper_required.custom_curve_authorized);
        assert_eq!(helper_required.reason, "exact_record_match_helper_required");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn changed_bios_kernel_driver_board_and_missing_identity_fail_closed() {
        for (path, changed) in [
            ("dmi/sys_vendor", "OTHER VENDOR"),
            ("dmi/product_name", "OTHER-MODEL"),
            ("dmi/product_version", "2.0"),
            ("dmi/bios_version", "TEST.101"),
            ("osrelease", "7.0.0-test"),
            ("version", "Other Linux build"),
            ("asus_nb_wmi/srcversion", "OTHERDRIVER"),
            ("dmi/board_name", "OTHER-BOARD"),
        ] {
            let (root, paths) = fixture();
            fs::write(root.join(path), changed).unwrap();
            assert!(!evaluate(&paths, TEST_FAN_WRITE_RECORDS, context()).custom_curve_authorized);
            fs::remove_dir_all(root).unwrap();
        }
        let (root, paths) = fixture();
        fs::remove_file(root.join("dmi/product_name")).unwrap();
        let missing = evaluate(&paths, TEST_FAN_WRITE_RECORDS, context());
        assert_eq!(missing.reason, "identity_incomplete");
        assert!(!missing.custom_curve_authorized);
        fs::remove_dir_all(root).unwrap();

        let (root, paths) = fixture();
        let wrong_label = evaluate(
            &paths,
            TEST_FAN_WRITE_RECORDS,
            FanWriteContext {
                channel_label: "gpu_fan",
                ..context()
            },
        );
        assert!(!wrong_label.custom_curve_authorized);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ambiguous_and_malformed_records_fail_closed() {
        let (root, paths) = fixture();
        let record = TEST_FAN_WRITE_RECORDS[0];
        let duplicates = [record, record];
        let ambiguous = evaluate(&paths, &duplicates, context());
        assert_eq!(ambiguous.reason, "ambiguous_validation_records");
        assert!(!ambiguous.custom_curve_authorized);

        let malformed = [VerifiedFanWriteRecord {
            minimum_temperature_c: 90,
            maximum_temperature_c: 60,
            terminal_temperature_c: 50,
            ..record
        }];
        assert!(!evaluate(&paths, &malformed, context()).custom_curve_authorized);

        let unapproved = [VerifiedFanWriteRecord {
            approved: false,
            ..record
        }];
        assert!(!evaluate(&paths, &unapproved, context()).custom_curve_authorized);
        fs::remove_dir_all(root).unwrap();
    }
}
