//! Read-only system inventory and native package-manager handoff plans.

use std::{fs, io::Read, path::Path};

use rog_core::{DependencyKind, SetupAction, SetupPlan, SetupStatus, SetupSystem};
use sha2::{Digest, Sha256};

const MANUAL_REASON: &str = "Automatic setup is unavailable because the native backend cannot enforce the exact reviewed package changes";

pub fn inspect_setup_system() -> SetupSystem {
    inspect_setup_system_at(Path::new("/"))
}

fn read(root: &Path, path: &str) -> String {
    let Ok(file) = fs::File::open(root.join(path)) else {
        return String::new();
    };
    let mut value = String::new();
    if file.take(16385).read_to_string(&mut value).is_err() || value.len() > 16384 {
        return String::new();
    }
    value
        .trim()
        .chars()
        .filter(|character| !character.is_control() || *character == '\n')
        .collect()
}

fn os_value(input: &str, key: &str) -> String {
    input
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once('=')?;
            if name != key {
                return None;
            }
            Some(
                value
                    .trim_matches(['\'', '"'])
                    .chars()
                    .filter(|character| !character.is_control())
                    .collect(),
            )
        })
        .unwrap_or_default()
}

fn inspect_setup_system_at(root: &Path) -> SetupSystem {
    let os = read(root, "etc/os-release");
    let mut drivers = Vec::new();
    for name in ["asus_wmi", "asus_nb_wmi", "asus_armoury", "hid_asus"] {
        if root.join("sys/module").join(name).is_dir() {
            drivers.push(name.into());
        }
    }
    let mut conflicts = Vec::new();
    if let Ok(processes) = fs::read_dir(root.join("proc")) {
        for process in processes.take(4096).flatten() {
            if !process
                .file_name()
                .to_string_lossy()
                .chars()
                .all(|value| value.is_ascii_digit())
            {
                continue;
            }
            if let Ok(name) = fs::read_to_string(process.path().join("comm")) {
                if matches!(
                    name.trim(),
                    "power-profiles-"
                        | "power-profiles-daemon"
                        | "tlp"
                        | "tuned"
                        | "auto-cpufreq"
                        | "supergfxd"
                        | "cardwired"
                        | "optimus-manager"
                ) {
                    conflicts.push(name.trim().to_string());
                }
            }
        }
    }
    conflicts.sort();
    conflicts.dedup();
    SetupSystem {
        distribution_id: os_value(&os, "ID"),
        distribution_version: os_value(&os, "VERSION_ID"),
        kernel: read(root, "proc/sys/kernel/osrelease"),
        manufacturer: read(root, "sys/class/dmi/id/sys_vendor"),
        product: read(root, "sys/class/dmi/id/product_name"),
        asus_drivers: drivers,
        packagekit_available: [
            "usr/share/dbus-1/system-services/org.freedesktop.PackageKit.service",
            "usr/lib/systemd/system/packagekit.service",
            "lib/systemd/system/packagekit.service",
        ]
        .iter()
        .any(|path| root.join(path).is_file()),
        conflicts,
    }
}

pub fn plan_setup(system: &SetupSystem, status: &SetupStatus) -> SetupPlan {
    let distribution_guidance = match (system.distribution_id.as_str(), system.distribution_version.as_str()) {
        ("ubuntu", "26.10") => "Ubuntu 26.10 has no verified ASUS package source in this assistant. Review packages with Ubuntu's native tools.",
        ("ubuntu" | "debian" | "fedora", _) => "This distribution is identified, but automatic package transactions have no verified effect-set guarantee. Review packages with its native tools.",
        _ => "This distribution is unsupported by automatic setup. Consult the distribution's installation documentation.",
    };
    let mut actions = vec![SetupAction {
        component: "rog_helper".into(),
        description: "Repair ROG Helper integration".into(),
        packages: vec![], enabled: false, disabled_reason: MANUAL_REASON.into(),
        guidance: format!("{distribution_guidance} Review a reinstall of the original ROG Helper package, then recheck services. A downloaded DEB does not establish a trusted repository candidate."),
    }];
    for dependency in &status.dependencies {
        if dependency.state.is_ready() {
            continue;
        }
        let guidance = match dependency.kind {
            DependencyKind::RogHelperd => "Check the installed ROG Helper session service and refresh. Use the existing session-daemon recovery controls if its binary is stale.",
            DependencyKind::Asusd => "Review your distribution's ASUS Linux support documentation. No trusted ASUS package candidate has been verified. Native battery and lighting fallbacks remain independently detected.",
            DependencyKind::Supergfxd => "GPU switching is optional. Review existing GPU ownership before choosing a supported manager. Cardwire is experimental; no installation or GPU transition is offered.",
            DependencyKind::UPower => "Review the UPower package in your distribution's native package manager and recheck its API after installation.",
            DependencyKind::NvidiaSmi => "Review your distribution's NVIDIA driver guidance. Driver changes require a separate manual review.",
        };
        actions.push(SetupAction {
            component: dependency.kind.as_str().into(),
            description: format!(
                "Review {} ({})",
                dependency.kind.label(),
                dependency.state.label()
            ),
            packages: vec![],
            enabled: false,
            disabled_reason: MANUAL_REASON.into(),
            guidance: guidance.into(),
        });
    }
    // Include semantic state and guidance, excluding the volatile probe timestamp.
    let revision = format!(
        "{:x}",
        Sha256::digest(format!(
            "{system:?}|{actions:?}|{:?}|{:?}|{:?}",
            status.dependencies, status.permissions, status.issues
        ))
    );
    SetupPlan { revision, actions }
}

pub fn format_setup_report(system: &SetupSystem, plan: &SetupPlan) -> String {
    let mut report = format!("{} {} · kernel {}\nHardware: {} {}\nASUS drivers: {}\nPackageKit service installed: {}\nObserved power/GPU owner processes: {}\n\n", system.distribution_id, system.distribution_version, system.kernel, system.manufacturer, system.product, system.asus_drivers.join(", "), system.packagekit_available, system.conflicts.join(", "));
    for action in &plan.actions {
        report.push_str(&format!(
            "{}\n{}\n{}\n\n",
            action.description, action.disabled_reason, action.guidance
        ));
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inventory_uses_fixture_paths_and_limits_metadata() {
        let root = std::env::temp_dir().join(format!(
            "rog-setup-inventory-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("etc")).unwrap();
        fs::create_dir_all(root.join("sys/module/asus_wmi")).unwrap();
        fs::create_dir_all(root.join("proc/123")).unwrap();
        fs::create_dir_all(root.join("proc/sys/kernel")).unwrap();
        fs::create_dir_all(root.join("sys/class/dmi/id")).unwrap();
        fs::write(root.join("proc/sys/kernel/osrelease"), "7.3.0-test\n").unwrap();
        fs::write(
            root.join("sys/class/dmi/id/sys_vendor"),
            "ASUSTeK COMPUTER INC.\n",
        )
        .unwrap();
        fs::write(root.join("sys/class/dmi/id/product_name"), "G615JMR\n").unwrap();
        fs::write(
            root.join("etc/os-release"),
            "ID=ubuntu\nVERSION_ID=\"26.10\"\n",
        )
        .unwrap();
        fs::write(root.join("proc/123/comm"), "tuned\n").unwrap();
        let system = inspect_setup_system_at(&root);
        assert_eq!(system.distribution_id, "ubuntu");
        assert_eq!(system.distribution_version, "26.10");
        assert_eq!(system.asus_drivers, ["asus_wmi"]);
        assert_eq!(system.kernel, "7.3.0-test");
        assert_eq!(system.product, "G615JMR");
        assert_eq!(system.manufacturer, "ASUSTeK COMPUTER INC.");
        assert_eq!(system.conflicts, ["tuned"]);
        assert!(!system.packagekit_available);
        fs::write(root.join("etc/os-release"), "x".repeat(16385)).unwrap();
        assert!(inspect_setup_system_at(&root).distribution_id.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn plans_are_manual_and_revision_ignores_timestamp() {
        let system = SetupSystem {
            distribution_id: "ubuntu".into(),
            ..SetupSystem::default()
        };
        let mut status = SetupStatus::unknown();
        let plan = plan_setup(&system, &status);
        assert!(plan.actions.iter().all(|action| !action.enabled
            && action.packages.is_empty()
            && !action.disabled_reason.is_empty()));
        status.checked_at_ms = 999;
        assert_eq!(plan, plan_setup(&system, &status));
        let mut changed = system.clone();
        changed.conflicts.push("tuned".into());
        assert_ne!(plan.revision, plan_setup(&changed, &status).revision);
    }

    #[test]
    fn unknown_distribution_never_gets_executable_package_actions() {
        let plan = plan_setup(&SetupSystem::default(), &SetupStatus::unknown());
        assert!(plan
            .actions
            .iter()
            .all(|action| !action.enabled && action.packages.is_empty()));
        assert_eq!(
            os_value("ID=ubuntu\nVERSION_ID=\"26.10\"", "VERSION_ID"),
            "26.10"
        );
        assert_eq!(os_value("garbage", "ID"), "");
        assert!(plan.actions[0].guidance.contains("unsupported"));
        let ubuntu = SetupSystem {
            distribution_id: "ubuntu".into(),
            distribution_version: "26.10".into(),
            ..SetupSystem::default()
        };
        assert!(plan_setup(&ubuntu, &SetupStatus::unknown()).actions[0]
            .guidance
            .contains("Ubuntu 26.10"));
        assert!(plan_setup(&ubuntu, &SetupStatus::unknown())
            .actions
            .iter()
            .all(|action| !action.enabled));
    }
}
