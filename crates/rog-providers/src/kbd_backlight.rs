use std::fs::{read_dir, read_to_string, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use rog_core::{RogError, RogResult};

const SYSFS_LEDS_ROOT: &str = "/sys/class/leds";

#[derive(Debug, Clone)]
pub struct KbdBacklightSysfs {
    led_name: String,
    led_path: PathBuf,
    max_brightness: u32,
}

impl KbdBacklightSysfs {
    pub fn probe() -> RogResult<Option<Self>> {
        let root = Path::new(SYSFS_LEDS_ROOT);
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

    pub fn led_name(&self) -> &str {
        &self.led_name
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
        let p = self.led_path.join("brightness");
        OpenOptions::new().write(true).open(&p).is_ok()
    }

    pub fn set_brightness(&self, brightness: u32) -> RogResult<()> {
        let p = self.led_path.join("brightness");
        let clamped = brightness.min(self.max_brightness);

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
        write!(f, "{clamped}\n")
            .map_err(|e| RogError::Unexpected(format!("failed to write {}: {e}", p.display())))?;
        Ok(())
    }
}
