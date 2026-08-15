use std::fs::{read_dir, read_to_string, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use rog_core::{RogError, RogResult};

const SYSFS_LEDS_ROOT: &str = "/sys/class/leds";
const ASUS_KBD_LED_NAME: &str = "asus::kbd_backlight";
const ASUS_KBD_MAX_BRIGHTNESS: u32 = 3;

#[derive(Debug, Clone)]
pub struct KbdBacklightSysfs {
    led_name: String,
    led_path: PathBuf,
    max_brightness: u32,
}

impl KbdBacklightSysfs {
    pub fn probe() -> RogResult<Option<Self>> {
        Self::probe_at(Path::new(SYSFS_LEDS_ROOT))
    }

    pub fn probe_approved_asus() -> RogResult<Option<Self>> {
        Self::probe_approved_asus_at(Path::new(SYSFS_LEDS_ROOT))
    }

    fn probe_at(root: &Path) -> RogResult<Option<Self>> {
        let entries = match read_dir(root) {
            Ok(e) => e,
            Err(_) => return Ok(None),
        };

        let mut best: Option<(String, PathBuf)> = None;

        for ent in entries.flatten() {
            let name = ent.file_name().to_string_lossy().to_string();
            let name_lc = name.to_ascii_lowercase();
            let is_match = name_lc.contains("kbd_backlight")
                || name_lc.contains("keyboard_backlight")
                || name_lc.contains("multicolor:keyboard");
            if !is_match {
                continue;
            }

            // Prefer the canonical ASUS laptop backlight LED when present.
            if name == "asus::kbd_backlight" {
                best = Some((name, ent.path()));
                break;
            }

            // Otherwise keep the first match.
            if best.is_none() {
                best = Some((name, ent.path()));
            }
        }

        let Some((led_name, led_path)) = best else {
            return Ok(None);
        };

        let max_path = led_path.join("max_brightness");
        let max_txt = read_to_string(&max_path).map_err(|e| {
            RogError::Unexpected(format!("failed to read {}: {e}", max_path.display()))
        })?;
        let max_brightness: u32 = max_txt.trim().parse().map_err(|e| {
            RogError::Unexpected(format!(
                "failed to parse {} as integer: {e}",
                max_path.display()
            ))
        })?;

        Ok(Some(Self {
            led_name,
            led_path,
            max_brightness,
        }))
    }

    fn probe_approved_asus_at(root: &Path) -> RogResult<Option<Self>> {
        let led_path = root.join(ASUS_KBD_LED_NAME);
        if !led_path.is_dir() {
            return Ok(None);
        }
        let canonical = led_path.canonicalize().map_err(|_| {
            RogError::TemporarilyUnavailable(
                "approved ASUS keyboard backlight device could not be resolved".to_string(),
            )
        })?;
        let identity_is_valid = canonical.file_name().and_then(|name| name.to_str())
            == Some(ASUS_KBD_LED_NAME)
            && canonical
                .parent()
                .and_then(Path::file_name)
                .and_then(|name| name.to_str())
                == Some("leds")
            && canonical
                .parent()
                .and_then(Path::parent)
                .and_then(Path::file_name)
                .and_then(|name| name.to_str())
                == Some("asus-nb-wmi");
        if !identity_is_valid {
            return Ok(None);
        }

        let max_brightness = read_u32(&led_path.join("max_brightness"))?;
        if max_brightness != ASUS_KBD_MAX_BRIGHTNESS {
            return Ok(None);
        }
        let brightness = read_u32(&led_path.join("brightness"))?;
        if brightness > max_brightness {
            return Err(RogError::Unexpected(
                "ASUS keyboard backlight reported brightness above its maximum".to_string(),
            ));
        }
        Ok(Some(Self {
            led_name: ASUS_KBD_LED_NAME.to_string(),
            led_path,
            max_brightness,
        }))
    }

    pub fn led_name(&self) -> &str {
        &self.led_name
    }

    pub fn led_path(&self) -> &Path {
        &self.led_path
    }

    pub fn brightness_path(&self) -> PathBuf {
        self.led_path.join("brightness")
    }

    pub fn max_brightness(&self) -> u32 {
        self.max_brightness
    }

    pub fn read_brightness(&self) -> RogResult<u32> {
        let p = self.led_path.join("brightness");
        let txt = read_to_string(&p)
            .map_err(|e| RogError::Unexpected(format!("failed to read {}: {e}", p.display())))?;
        let v: u32 = txt.trim().parse().map_err(|e| {
            RogError::Unexpected(format!("failed to parse {} as integer: {e}", p.display()))
        })?;
        Ok(v)
    }

    pub fn can_set_brightness(&self) -> bool {
        let p = self.brightness_path();
        OpenOptions::new().write(true).open(&p).is_ok()
    }

    pub fn can_read_brightness(&self) -> bool {
        self.read_brightness().is_ok()
    }

    pub fn privileged_write_approved(&self) -> bool {
        if self.led_name != ASUS_KBD_LED_NAME || self.max_brightness != ASUS_KBD_MAX_BRIGHTNESS {
            return false;
        }
        let Ok(canonical) = self.led_path.canonicalize() else {
            return false;
        };
        canonical.file_name().and_then(|name| name.to_str()) == Some(ASUS_KBD_LED_NAME)
            && canonical
                .parent()
                .and_then(Path::file_name)
                .and_then(|name| name.to_str())
                == Some("leds")
            && canonical
                .parent()
                .and_then(Path::parent)
                .and_then(Path::file_name)
                .and_then(|name| name.to_str())
                == Some("asus-nb-wmi")
            && self.approved_attribute("brightness")
            && self.approved_attribute("max_brightness")
            && read_u32(&self.led_path.join("max_brightness")).ok() == Some(ASUS_KBD_MAX_BRIGHTNESS)
    }

    fn approved_attribute(&self, name: &str) -> bool {
        let path = self.led_path.join(name);
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            return false;
        };
        if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
            return false;
        }
        let Ok(canonical_led) = self.led_path.canonicalize() else {
            return false;
        };
        let Ok(canonical_attribute) = path.canonicalize() else {
            return false;
        };
        canonical_attribute.parent() == Some(canonical_led.as_path())
            && canonical_attribute
                .file_name()
                .and_then(|value| value.to_str())
                == Some(name)
    }

    pub fn set_approved_brightness(&self, brightness: u32) -> RogResult<()> {
        if !self.privileged_write_approved() {
            return Err(RogError::NotSupported(
                "keyboard backlight identity changed before the privileged write".to_string(),
            ));
        }
        self.set_brightness(brightness)
    }

    pub fn set_brightness(&self, brightness: u32) -> RogResult<()> {
        self.validate_brightness(brightness)?;
        let p = self.led_path.join("brightness");

        let mut f = OpenOptions::new().write(true).open(&p).map_err(|e| {
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                RogError::PermissionDenied(format!(
                    "{} not writable (need asusd or a udev rule): {e}",
                    p.display()
                ))
            } else {
                RogError::Unexpected(format!("failed to open {} for write: {e}", p.display()))
            }
        })?;

        // sysfs prefers a newline-terminated write.
        writeln!(f, "{brightness}")
            .map_err(|e| RogError::Unexpected(format!("failed to write {}: {e}", p.display())))?;
        drop(f);
        let actual = self.read_brightness()?;
        if actual != brightness {
            return Err(RogError::Unexpected(format!(
                "keyboard brightness readback was {actual}, expected {brightness}"
            )));
        }
        Ok(())
    }

    pub fn validate_brightness(&self, brightness: u32) -> RogResult<()> {
        if brightness > self.max_brightness {
            return Err(RogError::InvalidInput(format!(
                "keyboard brightness {brightness} exceeds backend maximum {}",
                self.max_brightness
            )));
        }
        Ok(())
    }
}

fn read_u32(path: &Path) -> RogResult<u32> {
    let raw = read_to_string(path).map_err(|_| {
        RogError::Unexpected("keyboard LED attribute could not be read".to_string())
    })?;
    raw.trim()
        .parse::<u32>()
        .map_err(|_| RogError::Unexpected("keyboard LED attribute was not numeric".to_string()))
}

#[cfg(test)]
mod tests {
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::symlink;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn brightness_above_backend_maximum_is_rejected_before_io() {
        let provider = KbdBacklightSysfs {
            led_name: "test::kbd_backlight".to_string(),
            led_path: PathBuf::from("/path/that/must/not/be/opened"),
            max_brightness: 3,
        };

        let error = provider.set_brightness(4).unwrap_err();
        assert!(matches!(error, RogError::InvalidInput(_)));
        assert!(error.to_string().contains("exceeds backend maximum 3"));
    }

    #[test]
    fn direct_write_is_read_back() {
        let root = temp_root("direct");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("brightness"), "1\n").unwrap();
        let provider = KbdBacklightSysfs {
            led_name: ASUS_KBD_LED_NAME.to_string(),
            led_path: root.clone(),
            max_brightness: 3,
        };
        provider.set_brightness(2).unwrap();
        assert_eq!(provider.read_brightness().unwrap(), 2);
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn privileged_probe_requires_exact_asus_wmi_identity_and_bounds() {
        let root = temp_root("approved");
        let device = root
            .join("devices/platform/asus-nb-wmi/leds")
            .join(ASUS_KBD_LED_NAME);
        let leds = root.join("leds");
        fs::create_dir_all(&device).unwrap();
        fs::create_dir_all(&leds).unwrap();
        fs::write(device.join("brightness"), "1\n").unwrap();
        fs::write(device.join("max_brightness"), "3\n").unwrap();
        symlink(&device, leds.join(ASUS_KBD_LED_NAME)).unwrap();

        let provider = KbdBacklightSysfs::probe_approved_asus_at(&leds)
            .unwrap()
            .expect("canonical ASUS WMI LED should be accepted");
        assert_eq!(provider.max_brightness(), 3);
        assert!(provider.privileged_write_approved());

        fs::write(device.join("max_brightness"), "255\n").unwrap();
        assert!(KbdBacklightSysfs::probe_approved_asus_at(&leds)
            .unwrap()
            .is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn privileged_write_revalidates_attribute_identity() {
        let root = temp_root("attribute-race");
        let device = root
            .join("devices/platform/asus-nb-wmi/leds")
            .join(ASUS_KBD_LED_NAME);
        let leds = root.join("leds");
        fs::create_dir_all(&device).unwrap();
        fs::create_dir_all(&leds).unwrap();
        fs::write(device.join("brightness"), "1\n").unwrap();
        fs::write(device.join("max_brightness"), "3\n").unwrap();
        symlink(&device, leds.join(ASUS_KBD_LED_NAME)).unwrap();
        let provider = KbdBacklightSysfs::probe_approved_asus_at(&leds)
            .unwrap()
            .expect("approved device");

        let outside = root.join("outside-brightness");
        fs::write(&outside, "1\n").unwrap();
        fs::remove_file(device.join("brightness")).unwrap();
        symlink(&outside, device.join("brightness")).unwrap();
        assert!(!provider.privileged_write_approved());
        assert!(matches!(
            provider.set_approved_brightness(2),
            Err(RogError::NotSupported(_))
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn privileged_probe_rejects_lookalike_device() {
        let root = temp_root("lookalike");
        let device = root
            .join("devices/platform/not-asus/leds")
            .join(ASUS_KBD_LED_NAME);
        let leds = root.join("leds");
        fs::create_dir_all(&device).unwrap();
        fs::create_dir_all(&leds).unwrap();
        fs::write(device.join("brightness"), "1\n").unwrap();
        fs::write(device.join("max_brightness"), "3\n").unwrap();
        symlink(&device, leds.join(ASUS_KBD_LED_NAME)).unwrap();
        assert!(KbdBacklightSysfs::probe_approved_asus_at(&leds)
            .unwrap()
            .is_none());
        let _ = fs::remove_dir_all(root);
    }

    fn temp_root(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "rog-helper-kbd-test-{name}-{}-{stamp}",
            std::process::id()
        ))
    }
}
