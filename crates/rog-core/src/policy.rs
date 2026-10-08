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
    candidate_power_source: Option<PowerSource>,
    candidate_since_ms: Option<u64>,
    resume_pending: bool,
    last_stable_power_source: Option<PowerSource>,
}

impl PolicyState {
    pub fn new(config: PolicyConfig) -> Self {
        Self {
            auto_enabled: false,
            manual_override: false,
            last_apply_at_ms: None,
            last_power_source: None,
            config,
            candidate_power_source: None,
            candidate_since_ms: None,
            resume_pending: false,
            last_stable_power_source: None,
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        if self.auto_enabled != enabled {
            self.auto_enabled = enabled;
            self.candidate_power_source = None;
            self.candidate_since_ms = None;
        }
    }

    pub fn set_manual_override(&mut self, paused: bool) {
        if self.manual_override == paused {
            return;
        }
        if self.manual_override && !paused {
            self.resume_pending = true;
        }
        self.manual_override = paused;
        self.candidate_power_source = None;
        self.candidate_since_ms = None;
    }

    pub fn resume(&mut self) {
        self.manual_override = false;
        self.resume_pending = true;
        self.candidate_power_source = None;
        self.candidate_since_ms = None;
    }

    pub fn stable_power_source(&self) -> Option<PowerSource> {
        self.last_stable_power_source
    }

    pub fn request_current_rule(&mut self, at_ms: u64) -> Vec<PolicyAction> {
        if !self.auto_enabled || self.manual_override {
            return Vec::new();
        }
        let Some(source) = self.last_stable_power_source else {
            return Vec::new();
        };
        self.last_apply_at_ms = Some(at_ms);
        vec![PolicyAction::ApplyFor(source)]
    }

    pub fn observe_power_source(
        &mut self,
        at_ms: u64,
        source: Option<PowerSource>,
    ) -> Vec<PolicyAction> {
        let Some(source) = source else {
            self.candidate_power_source = None;
            self.candidate_since_ms = None;
            return Vec::new();
        };
        self.last_power_source = Some(source);
        if !self.auto_enabled || self.manual_override {
            return Vec::new();
        }
        if self.resume_pending {
            self.resume_pending = false;
            self.last_apply_at_ms = Some(at_ms);
            return vec![PolicyAction::ApplyFor(source)];
        }
        if self.last_stable_power_source == Some(source) {
            self.candidate_power_source = None;
            self.candidate_since_ms = None;
            return Vec::new();
        }
        if self.candidate_power_source != Some(source) {
            self.candidate_power_source = Some(source);
            self.candidate_since_ms = Some(at_ms);
            return Vec::new();
        }
        if at_ms.saturating_sub(self.candidate_since_ms.unwrap_or(at_ms)) < self.config.debounce_ms
        {
            return Vec::new();
        }
        self.last_stable_power_source = Some(source);
        self.candidate_power_source = None;
        self.candidate_since_ms = None;
        self.last_apply_at_ms = Some(at_ms);
        vec![PolicyAction::ApplyFor(source)]
    }

    pub fn handle_event(&mut self, event: PolicyEvent) -> Vec<PolicyAction> {
        match event {
            PolicyEvent::PowerSourceChanged { at_ms, source } => {
                self.observe_power_source(at_ms, Some(source))
            }
            PolicyEvent::ManualOverrideEnabled { at_ms: _ } => {
                self.set_manual_override(true);
                Vec::new()
            }
            PolicyEvent::ManualOverrideDisabled { at_ms: _ } => {
                self.set_manual_override(false);
                Vec::new()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debounce_blocks_rapid_reapply() {
        let mut s = PolicyState::new(PolicyConfig { debounce_ms: 5_000 });
        s.set_enabled(true);
        let a1 = s.handle_event(PolicyEvent::PowerSourceChanged {
            at_ms: 1_000,
            source: PowerSource::Ac,
        });
        assert!(a1.is_empty());
        assert_eq!(
            s.observe_power_source(6_000, Some(PowerSource::Ac)),
            vec![PolicyAction::ApplyFor(PowerSource::Ac)]
        );

        let a2 = s.handle_event(PolicyEvent::PowerSourceChanged {
            at_ms: 7_000,
            source: PowerSource::Battery,
        });
        assert!(a2.is_empty());

        let a3 = s.handle_event(PolicyEvent::PowerSourceChanged {
            at_ms: 12_000,
            source: PowerSource::Battery,
        });
        assert_eq!(a3, vec![PolicyAction::ApplyFor(PowerSource::Battery)]);
    }

    #[test]
    fn manual_override_pauses_auto() {
        let mut s = PolicyState::new(PolicyConfig { debounce_ms: 0 });
        s.set_enabled(true);
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

    #[test]
    fn transition_requires_stable_power_source_and_emits_once() {
        let mut state = PolicyState::new(PolicyConfig { debounce_ms: 5_000 });
        state.set_enabled(true);
        assert!(state
            .observe_power_source(1_000, Some(PowerSource::Ac))
            .is_empty());
        assert!(state
            .observe_power_source(5_999, Some(PowerSource::Ac))
            .is_empty());
        assert_eq!(
            state.observe_power_source(6_000, Some(PowerSource::Ac)),
            vec![PolicyAction::ApplyFor(PowerSource::Ac)]
        );
        for now in [7_000, 8_000, 20_000] {
            assert!(state
                .observe_power_source(now, Some(PowerSource::Ac))
                .is_empty());
        }
    }

    #[test]
    fn missing_power_sample_resets_candidate_without_fabricating_a_transition() {
        let mut state = PolicyState::new(PolicyConfig { debounce_ms: 1_000 });
        state.set_enabled(true);
        state.observe_power_source(0, Some(PowerSource::Ac));
        assert!(state.observe_power_source(500, None).is_empty());
        assert!(state
            .observe_power_source(800, Some(PowerSource::Ac))
            .is_empty());
        assert_eq!(
            state.observe_power_source(1_800, Some(PowerSource::Ac)),
            vec![PolicyAction::ApplyFor(PowerSource::Ac)]
        );
    }
}
