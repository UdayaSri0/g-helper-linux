//! Read-only setup plans and explicit transaction outcomes.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetupSystem {
    pub distribution_id: String,
    pub distribution_version: String,
    pub kernel: String,
    pub manufacturer: String,
    pub product: String,
    pub asus_drivers: Vec<String>,
    pub packagekit_available: bool,
    pub conflicts: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetupAction {
    pub component: String,
    pub description: String,
    pub packages: Vec<String>,
    pub enabled: bool,
    pub disabled_reason: String,
    pub guidance: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetupPlan {
    pub revision: String,
    pub actions: Vec<SetupAction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetupSnapshot {
    pub system: SetupSystem,
    pub plan: SetupPlan,
}
