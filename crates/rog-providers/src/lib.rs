//! Provider layer: DBus clients + sysfs readers.

pub mod dbus;
pub mod hwmon;
pub mod kbd_backlight;
pub mod nvidia_smi;
pub mod upower;

pub mod traits;
