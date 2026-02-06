//! Pure domain model (no hardware I/O).

pub mod error;
pub mod model;
pub mod policy;

pub use error::{RogError, RogResult, ValidationWarning};
pub use model::*;
pub use policy::*;
