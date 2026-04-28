//! Provider layer: DBus clients + sysfs readers.

pub mod asusd;
pub mod aura;
pub mod cpu;
pub mod dbus;
pub mod hwmon;
pub mod kbd_backlight;
pub mod memory;
pub mod nvidia_smi;
pub mod power_supply;
pub mod supergfx;
pub mod upower;

pub mod traits;
