use serde::{Deserialize, Serialize};

use crate::PowerSource;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyConfig {
    /// Minimum time between applies (debounce window).
    pub debounce_ms: u64,
}

impl Default for PolicyConfig {
    fn default() -> Self {
        Self { debounce_ms: 5_000 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PolicyEvent {
    PowerSourceChanged { at_ms: u64, source: PowerSource },
    ManualOverrideEnabled { at_ms: u64 },
    ManualOverrideDisabled { at_ms: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PolicyAction {
    /// Apply automation rules for the given power source.
    ApplyFor(PowerSource),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyState {
    pub auto_enabled: bool,
    pub manual_override: bool,
    pub last_apply_at_ms: Option<u64>,
    pub last_power_source: Option<PowerSource>,
    pub config: PolicyConfig,
}

impl PolicyState {
    pub fn new(config: PolicyConfig) -> Self {
        Self {
            auto_enabled: true,
            manual_override: false,
            last_apply_at_ms: None,
            last_power_source: None,
            config,
        }
    }

    pub fn handle_event(&mut self, event: PolicyEvent) -> Vec<PolicyAction> {
        let (at_ms, power_source_opt) = match event {
            PolicyEvent::PowerSourceChanged { at_ms, source } => (at_ms, Some(source)),
            PolicyEvent::ManualOverrideEnabled { at_ms: _ } => {
                self.manual_override = true;
                return Vec::new();
            }
            PolicyEvent::ManualOverrideDisabled { at_ms: _ } => {
                self.manual_override = false;
                return Vec::new();
            }
        };

        let Some(source) = power_source_opt else {
            return Vec::new();
        };

        self.last_power_source = Some(source);

        if !self.auto_enabled || self.manual_override {
            return Vec::new();
        }

        if let Some(last_apply) = self.last_apply_at_ms {
            if at_ms.saturating_sub(last_apply) < self.config.debounce_ms {
                return Vec::new();
            }
        }

        self.last_apply_at_ms = Some(at_ms);
        vec![PolicyAction::ApplyFor(source)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debounce_blocks_rapid_reapply() {
        let mut s = PolicyState::new(PolicyConfig { debounce_ms: 5_000 });
        let a1 = s.handle_event(PolicyEvent::PowerSourceChanged {
            at_ms: 1_000,
            source: PowerSource::Ac,
        });
        assert_eq!(a1, vec![PolicyAction::ApplyFor(PowerSource::Ac)]);

        let a2 = s.handle_event(PolicyEvent::PowerSourceChanged {
            at_ms: 2_000,
            source: PowerSource::Battery,
        });
        assert!(a2.is_empty());

        let a3 = s.handle_event(PolicyEvent::PowerSourceChanged {
            at_ms: 7_000,
            source: PowerSource::Battery,
        });
        assert_eq!(a3, vec![PolicyAction::ApplyFor(PowerSource::Battery)]);
    }

    #[test]
    fn manual_override_pauses_auto() {
        let mut s = PolicyState::new(PolicyConfig { debounce_ms: 0 });
        s.handle_event(PolicyEvent::ManualOverrideEnabled { at_ms: 1_000 });

        let a1 = s.handle_event(PolicyEvent::PowerSourceChanged {
            at_ms: 2_000,
            source: PowerSource::Ac,
        });
        assert!(a1.is_empty());

        s.handle_event(PolicyEvent::ManualOverrideDisabled { at_ms: 3_000 });

        let a2 = s.handle_event(PolicyEvent::PowerSourceChanged {
            at_ms: 4_000,
            source: PowerSource::Ac,
        });
        assert_eq!(a2, vec![PolicyAction::ApplyFor(PowerSource::Ac)]);
    }
}
