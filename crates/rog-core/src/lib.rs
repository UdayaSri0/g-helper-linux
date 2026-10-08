//! Pure domain model (no hardware I/O).

/// Base source revision at compilation; local uncommitted changes may be present.
pub const BUILD_COMMIT: &str = env!("ROG_HELPER_BUILD_COMMIT");

pub mod config;
pub mod cpu_control;
pub mod dbus_keys;
pub mod error;
pub mod model;
pub mod policy;
pub mod privileged;

pub use config::*;
pub use cpu_control::*;
pub use dbus_keys::*;
pub use error::{ErrorCategory, RogError, RogResult, ValidationWarning};
pub use model::*;
pub use policy::*;
pub use privileged::*;
