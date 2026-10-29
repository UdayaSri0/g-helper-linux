use serde::{Deserialize, Serialize};

use crate::PowerSource;

pub fn battery_threshold_satisfied(
    threshold_percent: u8,
    battery_percent: Option<f32>,
) -> Option<bool> {
    battery_percent
        .filter(|percent| percent.is_finite())
        .map(|percent| percent <= f32::from(threshold_percent))
}

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
    /// Apply the explicitly selected battery-recovery preset.
    ApplyRecovery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AutomationPreset {
    Ac,
    Battery,
    Recovery,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ComponentRetryState {
    desired_key: Option<String>,
    source: Option<PowerSource>,
    generation: u64,
    attempts: u8,
    next_attempt_boottime_ms: Option<u64>,
    confirmed: bool,
    failed: bool,
}

impl ComponentRetryState {
    pub fn ready_for(
        &mut self,
        desired_key: &str,
        source: PowerSource,
        generation: u64,
        now_boottime_ms: u64,
    ) -> bool {
        if self.desired_key.as_deref() != Some(desired_key)
            || self.source != Some(source)
            || self.generation != generation
        {
            *self = Self {
                desired_key: Some(desired_key.to_owned()),
                source: Some(source),
                generation,
                ..Self::default()
            };
        }
        !self.failed
            && (self.attempts == 0
                || self
                    .next_attempt_boottime_ms
                    .is_some_and(|retry_at| now_boottime_ms >= retry_at))
    }

    pub fn started(&mut self) {
        self.attempts = self.attempts.saturating_add(1).min(3);
        self.next_attempt_boottime_ms = None;
        self.confirmed = false;
    }

    pub fn failed_at(&mut self, now_boottime_ms: u64) {
        self.next_attempt_boottime_ms = match self.attempts {
            1 => Some(now_boottime_ms.saturating_add(5_000)),
            2 => Some(now_boottime_ms.saturating_add(30_000)),
            _ => {
                self.confirmed = false;
                self.failed = true;
                None
            }
        };
    }

    pub fn confirmed(&mut self) {
        self.next_attempt_boottime_ms = None;
        self.confirmed = true;
        self.failed = false;
    }

    pub fn cancel(&mut self) {
        *self = Self::default();
    }

    pub fn attempts(&self) -> u8 {
        self.attempts
    }

    pub fn next_attempt_boottime_ms(&self) -> Option<u64> {
        self.next_attempt_boottime_ms
    }

    pub fn is_failed(&self) -> bool {
        self.failed
    }

    pub fn is_confirmed(&self) -> bool {
        self.confirmed
    }
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
    last_stable_power_source: Option<PowerSource>,
    active_preset: Option<AutomationPreset>,
    pending_preset: Option<AutomationPreset>,
    battery_candidate_preset: Option<AutomationPreset>,
    battery_candidate_since_ms: Option<u64>,
    battery_rule: Option<(u8, Option<u8>)>,
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
            last_stable_power_source: None,
            active_preset: None,
            pending_preset: None,
            battery_candidate_preset: None,
            battery_candidate_since_ms: None,
            battery_rule: None,
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        if self.auto_enabled != enabled {
            self.auto_enabled = enabled;
            self.candidate_power_source = None;
            self.candidate_since_ms = None;
            self.battery_candidate_preset = None;
            self.battery_candidate_since_ms = None;
            self.last_stable_power_source = None;
            self.active_preset = None;
            self.pending_preset = None;
        }
    }

    pub fn set_manual_override(&mut self, paused: bool) {
        if self.manual_override == paused {
            return;
        }
        self.manual_override = paused;
        self.candidate_power_source = None;
        self.candidate_since_ms = None;
        self.battery_candidate_preset = None;
        self.battery_candidate_since_ms = None;
        if !paused {
            self.last_stable_power_source = None;
            self.active_preset = None;
            self.pending_preset = None;
        }
    }

    pub fn resume(&mut self) {
        self.manual_override = false;
        self.candidate_power_source = None;
        self.candidate_since_ms = None;
        self.battery_candidate_preset = None;
        self.battery_candidate_since_ms = None;
        self.last_stable_power_source = None;
        self.active_preset = None;
        self.pending_preset = None;
    }

    pub fn reset_for_reconfiguration(&mut self) {
        self.candidate_power_source = None;
        self.candidate_since_ms = None;
        self.last_stable_power_source = None;
        self.active_preset = None;
        self.pending_preset = None;
        self.battery_candidate_preset = None;
        self.battery_candidate_since_ms = None;
        self.battery_rule = None;
    }

    pub fn stable_power_source(&self) -> Option<PowerSource> {
        self.last_stable_power_source
    }

    pub fn active_preset(&self) -> Option<AutomationPreset> {
        self.active_preset
    }

    pub fn confirm_preset_applied(&mut self, preset: AutomationPreset) {
        self.active_preset = Some(preset);
        self.pending_preset = None;
    }

    pub fn request_current_rule(&mut self, at_ms: u64) -> Vec<PolicyAction> {
        if !self.auto_enabled || self.manual_override {
            return Vec::new();
        }
        let Some(source) = self.active_preset else {
            return Vec::new();
        };
        self.last_apply_at_ms = Some(at_ms);
        vec![match source {
            AutomationPreset::Ac => PolicyAction::ApplyFor(PowerSource::Ac),
            AutomationPreset::Battery => PolicyAction::ApplyFor(PowerSource::Battery),
            AutomationPreset::Recovery => PolicyAction::ApplyRecovery,
        }]
    }

    pub fn observe_power_source(
        &mut self,
        at_ms: u64,
        source: Option<PowerSource>,
    ) -> Vec<PolicyAction> {
        self.observe_power_source_with_battery_threshold(at_ms, source, false)
    }

    pub fn observe_power_source_with_battery_threshold(
        &mut self,
        at_ms: u64,
        source: Option<PowerSource>,
        battery_threshold_enabled: bool,
    ) -> Vec<PolicyAction> {
        let Some(source) = source else {
            self.candidate_power_source = None;
            self.candidate_since_ms = None;
            self.battery_candidate_preset = None;
            self.battery_candidate_since_ms = None;
            self.last_stable_power_source = None;
            self.pending_preset = None;
            return Vec::new();
        };
        self.last_power_source = Some(source);
        if !self.auto_enabled || self.manual_override {
            return Vec::new();
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
        self.battery_candidate_preset = None;
        self.battery_candidate_since_ms = None;
        self.pending_preset = None;
        let action = match source {
            PowerSource::Ac => Some(PolicyAction::ApplyFor(PowerSource::Ac)),
            PowerSource::Battery if !battery_threshold_enabled => {
                self.pending_preset = Some(AutomationPreset::Battery);
                Some(PolicyAction::ApplyFor(PowerSource::Battery))
            }
            PowerSource::Battery => None,
        };
        if source == PowerSource::Ac {
            self.pending_preset = Some(AutomationPreset::Ac);
        }
        self.last_apply_at_ms = Some(at_ms);
        action.into_iter().collect()
    }

    pub fn observe_battery_threshold(
        &mut self,
        at_ms: u64,
        battery_percent: Option<f32>,
        lower_percent: u8,
        upper_percent: Option<u8>,
        recovery_preset_available: bool,
    ) -> Vec<PolicyAction> {
        let rule = (lower_percent, upper_percent);
        if self.battery_rule != Some(rule) {
            self.battery_rule = Some(rule);
            self.battery_candidate_preset = None;
            self.battery_candidate_since_ms = None;
            self.pending_preset = None;
        }
        let percent =
            battery_percent.filter(|value| value.is_finite() && (0.0..=100.0).contains(value));
        let target = if self.auto_enabled
            && !self.manual_override
            && self.last_stable_power_source == Some(PowerSource::Battery)
        {
            percent.and_then(|value| {
                if value <= f32::from(lower_percent) {
                    Some(AutomationPreset::Battery)
                } else if upper_percent.is_some_and(|upper| value >= f32::from(upper)) {
                    recovery_preset_available.then_some(AutomationPreset::Recovery)
                } else {
                    None
                }
            })
        } else {
            None
        };
        let Some(target) = target else {
            self.battery_candidate_preset = None;
            self.battery_candidate_since_ms = None;
            return Vec::new();
        };
        if self.active_preset == Some(target) || self.pending_preset == Some(target) {
            self.battery_candidate_preset = None;
            self.battery_candidate_since_ms = None;
            return Vec::new();
        }
        if self.battery_candidate_preset != Some(target) {
            self.battery_candidate_preset = Some(target);
            self.battery_candidate_since_ms = Some(at_ms);
            return Vec::new();
        }
        if at_ms.saturating_sub(self.battery_candidate_since_ms.unwrap_or(at_ms))
            < self.config.debounce_ms
        {
            return Vec::new();
        }
        self.pending_preset = Some(target);
        self.battery_candidate_preset = None;
        self.battery_candidate_since_ms = None;
        self.last_apply_at_ms = Some(at_ms);
        vec![match target {
            AutomationPreset::Ac => PolicyAction::ApplyFor(PowerSource::Ac),
            AutomationPreset::Battery => PolicyAction::ApplyFor(PowerSource::Battery),
            AutomationPreset::Recovery => PolicyAction::ApplyRecovery,
        }]
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
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn disabled_or_paused_policy_never_emits_actions(
            paused in any::<bool>(),
            source_is_ac in any::<bool>(),
            at in any::<u64>(),
            percent in prop_oneof![Just(None), Just(Some(f32::NAN)), Just(Some(-1.0)), Just(Some(101.0))],
        ) {
            let source = if source_is_ac { PowerSource::Ac } else { PowerSource::Battery };
            let mut policy = PolicyState::new(PolicyConfig { debounce_ms: 0 });
            policy.set_enabled(false);
            policy.set_manual_override(paused);
            prop_assert!(policy.observe_power_source(at, Some(source)).is_empty());
            prop_assert!(policy.observe_battery_threshold(at, percent, 20, Some(80), true).is_empty());
            prop_assert!(policy.request_current_rule(at).is_empty());
        }

        #[test]
        fn generated_policy_sequences_never_apply_while_disabled_or_paused(
            steps in prop::collection::vec((any::<bool>(), any::<bool>(), any::<bool>()), 1..40),
        ) {
            let mut policy = PolicyState::new(PolicyConfig { debounce_ms: 0 });
            for (index, (enabled, paused, ac)) in steps.into_iter().enumerate() {
                policy.set_enabled(enabled);
                policy.set_manual_override(paused);
                let source = if ac { PowerSource::Ac } else { PowerSource::Battery };
                let at = index as u64 * 10_000;
                let first = policy.observe_power_source_with_battery_threshold(at, Some(source), true);
                let second = policy.observe_power_source_with_battery_threshold(at + 1, Some(source), true);
                if !enabled || paused {
                    prop_assert!(first.is_empty());
                    prop_assert!(second.is_empty());
                    prop_assert!(policy.request_current_rule(at + 2).is_empty());
                }
            }
        }

        #[test]
        fn invalid_battery_telemetry_never_schedules_a_component_action(
            lower in 1u8..100,
            invalid_percent in prop_oneof![
                Just(None),
                Just(Some(f32::NAN)),
                Just(Some(f32::INFINITY)),
                Just(Some(-0.1)),
                Just(Some(100.1)),
            ],
        ) {
            let mut policy = PolicyState::new(PolicyConfig { debounce_ms: 5_000 });
            policy.set_enabled(true);
            prop_assert!(policy.observe_power_source_with_battery_threshold(
                0, Some(PowerSource::Battery), true,
            ).is_empty());
            prop_assert!(policy.observe_power_source_with_battery_threshold(
                5_000, Some(PowerSource::Battery), true,
            ).is_empty());
            prop_assert!(policy.observe_battery_threshold(
                5_000, invalid_percent, lower, Some(lower + 1), true,
            ).is_empty());
            prop_assert!(policy.observe_battery_threshold(
                10_000, invalid_percent, lower, Some(lower + 1), true,
            ).is_empty());
        }

        #[test]
        fn retry_attempts_never_exceed_three(failures in 0u8..12, start in 0u64..1_000_000) {
            let mut retry = ComponentRetryState::default();
            let mut now = start;
            for attempt in 0..failures.min(3) {
                prop_assert!(retry.ready_for("same-desired-state", PowerSource::Battery, 1, now));
                retry.started();
                retry.failed_at(now);
                now = now.saturating_add(if attempt == 0 { 5_000 } else { 30_000 });
            }
            prop_assert_eq!(retry.attempts(), failures.min(3));
            prop_assert_eq!(retry.is_failed(), failures >= 3);
            if failures >= 3 {
                prop_assert!(!retry.ready_for("same-desired-state", PowerSource::Battery, 1, now.saturating_add(60_000)));
            }
        }

        #[test]
        fn source_or_relevant_generation_change_cancels_retry_backoff(
            start in 0u64..1_000_000,
            generation in 0u64..100,
        ) {
            let mut retry = ComponentRetryState::default();
            prop_assert!(retry.ready_for("limit:80", PowerSource::Battery, generation, start));
            retry.started();
            retry.failed_at(start);
            prop_assert!(!retry.ready_for("limit:80", PowerSource::Battery, generation, start + 4_999));
            prop_assert!(retry.ready_for("limit:80", PowerSource::Ac, generation, start + 4_999));
            retry.started();
            retry.failed_at(start + 4_999);
            prop_assert!(retry.ready_for("limit:80", PowerSource::Battery, generation.wrapping_add(1), start + 5_000));
        }
    }

    #[test]
    fn component_retry_state_allows_three_total_attempts_and_stops() {
        let mut retry = ComponentRetryState::default();
        assert!(retry.ready_for("balanced", PowerSource::Battery, 3, 10));
        retry.started();
        retry.failed_at(10);
        assert!(!retry.ready_for("balanced", PowerSource::Battery, 3, 5_009));
        assert!(retry.ready_for("balanced", PowerSource::Battery, 3, 5_010));
        retry.started();
        retry.failed_at(5_010);
        assert!(!retry.ready_for("balanced", PowerSource::Battery, 3, 35_009));
        assert!(retry.ready_for("balanced", PowerSource::Battery, 3, 35_010));
        retry.started();
        retry.failed_at(35_010);
        assert!(retry.is_failed());
        assert!(!retry.ready_for("balanced", PowerSource::Battery, 3, 100_000));
    }

    #[test]
    fn component_retry_state_resets_only_for_desired_generation_or_explicit_cancel() {
        let mut retry = ComponentRetryState::default();
        assert!(retry.ready_for("balanced", PowerSource::Battery, 7, 1));
        retry.started();
        retry.failed_at(1);
        assert!(!retry.ready_for("balanced", PowerSource::Battery, 7, 4_999));
        assert!(!retry.ready_for("balanced", PowerSource::Battery, 7, 4_999));
        assert!(retry.ready_for("balanced", PowerSource::Ac, 7, 4_999));
        assert_eq!(retry.attempts(), 0);
        retry.started();
        retry.failed_at(4_999);
        assert!(retry.ready_for("quiet", PowerSource::Battery, 7, 4_999));
        assert_eq!(retry.attempts(), 0);
        retry.started();
        retry.failed_at(4_999);
        assert!(retry.ready_for("quiet", PowerSource::Battery, 8, 5_000));
        retry.started();
        retry.cancel();
        assert_eq!(retry.attempts(), 0);
        assert!(retry.ready_for("quiet", PowerSource::Battery, 8, 5_000));
    }

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
        assert!(a2.is_empty());
        assert_eq!(
            s.observe_power_source(4_000, Some(PowerSource::Ac)),
            vec![PolicyAction::ApplyFor(PowerSource::Ac)]
        );
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
    fn ac_battery_ac_transitions_each_emit_once_after_debounce() {
        let mut state = PolicyState::new(PolicyConfig { debounce_ms: 1_000 });
        state.set_enabled(true);
        for (at_ms, source, expected) in [
            (0, PowerSource::Ac, None),
            (1_000, PowerSource::Ac, Some(PowerSource::Ac)),
            (2_000, PowerSource::Battery, None),
            (3_000, PowerSource::Battery, None),
        ] {
            let actions =
                state.observe_power_source_with_battery_threshold(at_ms, Some(source), true);
            let actual = actions.first().map(|action| match action {
                PolicyAction::ApplyFor(source) => *source,
                PolicyAction::ApplyRecovery => PowerSource::Ac,
            });
            assert_eq!(actual, expected);
        }
        assert!(state
            .observe_battery_threshold(3_000, Some(20.0), 25, None, false)
            .is_empty());
        assert_eq!(
            state.observe_battery_threshold(4_000, Some(20.0), 25, None, false),
            vec![PolicyAction::ApplyFor(PowerSource::Battery)]
        );
        assert!(state
            .observe_power_source(5_000, Some(PowerSource::Ac))
            .is_empty());
        assert_eq!(
            state.observe_power_source(6_000, Some(PowerSource::Ac)),
            vec![PolicyAction::ApplyFor(PowerSource::Ac)]
        );
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

    #[test]
    fn disabled_policy_never_emits_and_resume_is_explicit() {
        let mut state = PolicyState::new(PolicyConfig { debounce_ms: 0 });
        assert!(state
            .observe_power_source(1_000, Some(PowerSource::Ac))
            .is_empty());
        state.set_enabled(true);
        assert!(state
            .observe_power_source(2_000, Some(PowerSource::Ac))
            .is_empty());
        assert_eq!(
            state.observe_power_source(2_000, Some(PowerSource::Ac)),
            vec![PolicyAction::ApplyFor(PowerSource::Ac)]
        );
        state.set_manual_override(true);
        assert!(state
            .observe_power_source(3_000, Some(PowerSource::Battery))
            .is_empty());
        state.resume();
        assert!(state
            .observe_power_source_with_battery_threshold(4_000, Some(PowerSource::Battery), true)
            .is_empty());
        assert!(state
            .observe_power_source_with_battery_threshold(4_000, Some(PowerSource::Battery), true)
            .is_empty());
        assert!(state
            .observe_battery_threshold(4_000, Some(20.0), 25, None, false)
            .is_empty());
        assert_eq!(
            state.observe_battery_threshold(4_001, Some(20.0), 25, None, false),
            vec![PolicyAction::ApplyFor(PowerSource::Battery)]
        );
        state.set_enabled(false);
        state.resume();
        assert!(state
            .observe_power_source(5_000, Some(PowerSource::Ac))
            .is_empty());
    }

    #[test]
    fn persisted_manual_override_survives_reconstructed_runtime() {
        let persisted = crate::AutomationPreferences {
            enabled: true,
            manual_override: true,
            ..crate::AutomationPreferences::default()
        };
        let mut state = PolicyState::new(PolicyConfig::default());
        state.set_enabled(persisted.enabled);
        state.set_manual_override(persisted.manual_override);
        for now in [0, 5_000, 10_000] {
            assert!(state
                .observe_power_source(now, Some(PowerSource::Ac))
                .is_empty());
        }
    }

    #[test]
    fn battery_threshold_requires_finite_telemetry_and_includes_boundary() {
        assert_eq!(battery_threshold_satisfied(25, Some(25.0)), Some(true));
        assert_eq!(battery_threshold_satisfied(25, Some(26.0)), Some(false));
        assert_eq!(battery_threshold_satisfied(25, None), None);
        assert_eq!(battery_threshold_satisfied(25, Some(f32::NAN)), None);
    }

    #[test]
    fn battery_threshold_hysteresis_selects_explicit_recovery_after_upper_dwell() {
        let mut state = PolicyState::new(PolicyConfig { debounce_ms: 5_000 });
        state.set_enabled(true);
        state.observe_power_source_with_battery_threshold(0, Some(PowerSource::Battery), true);
        assert!(state
            .observe_power_source_with_battery_threshold(5_000, Some(PowerSource::Battery), true)
            .is_empty());

        assert!(state
            .observe_battery_threshold(5_000, Some(25.0), 25, Some(80), true)
            .is_empty());
        assert!(state
            .observe_battery_threshold(9_999, Some(24.0), 25, Some(80), true)
            .is_empty());
        assert_eq!(
            state.observe_battery_threshold(10_000, Some(24.0), 25, Some(80), true),
            vec![PolicyAction::ApplyFor(PowerSource::Battery)]
        );
        assert_eq!(state.active_preset(), None);
        state.confirm_preset_applied(AutomationPreset::Battery);
        assert!(state
            .observe_battery_threshold(11_000, Some(50.0), 25, Some(80), true)
            .is_empty());
        assert_eq!(state.active_preset(), Some(AutomationPreset::Battery));
        assert!(state
            .observe_battery_threshold(12_000, Some(80.0), 25, Some(80), true)
            .is_empty());
        assert!(state
            .observe_battery_threshold(16_999, Some(82.0), 25, Some(80), true)
            .is_empty());
        assert_eq!(
            state.observe_battery_threshold(17_000, Some(82.0), 25, Some(80), true),
            vec![PolicyAction::ApplyRecovery]
        );
        state.confirm_preset_applied(AutomationPreset::Recovery);
    }

    #[test]
    fn upper_threshold_without_explicit_recovery_preset_never_selects_ac() {
        let mut state = PolicyState::new(PolicyConfig { debounce_ms: 5_000 });
        state.set_enabled(true);
        state.observe_power_source_with_battery_threshold(0, Some(PowerSource::Battery), true);
        state.observe_power_source_with_battery_threshold(5_000, Some(PowerSource::Battery), true);
        assert!(state
            .observe_battery_threshold(5_000, Some(90.0), 25, Some(80), false)
            .is_empty());
        assert!(state
            .observe_battery_threshold(10_000, Some(90.0), 25, Some(80), false)
            .is_empty());
        assert_eq!(state.active_preset(), None);
    }

    #[test]
    fn hysteresis_band_holds_known_target_but_restart_in_band_does_not_choose_one() {
        let mut known = PolicyState::new(PolicyConfig { debounce_ms: 0 });
        known.set_enabled(true);
        known.observe_power_source(0, Some(PowerSource::Ac));
        assert_eq!(
            known.observe_power_source(0, Some(PowerSource::Ac)),
            vec![PolicyAction::ApplyFor(PowerSource::Ac)]
        );
        known.confirm_preset_applied(AutomationPreset::Ac);
        known.observe_power_source_with_battery_threshold(1, Some(PowerSource::Battery), true);
        known.observe_power_source_with_battery_threshold(1, Some(PowerSource::Battery), true);
        assert!(known
            .observe_battery_threshold(2, Some(50.0), 25, Some(80), true)
            .is_empty());
        assert_eq!(known.active_preset(), Some(AutomationPreset::Ac));

        let mut restarted = PolicyState::new(PolicyConfig { debounce_ms: 0 });
        restarted.set_enabled(true);
        restarted.observe_power_source_with_battery_threshold(0, Some(PowerSource::Battery), true);
        restarted.observe_power_source_with_battery_threshold(0, Some(PowerSource::Battery), true);
        assert!(restarted
            .observe_battery_threshold(1, Some(50.0), 25, Some(80), true)
            .is_empty());
        assert_eq!(restarted.active_preset(), None);
    }

    #[test]
    fn invalid_readings_and_noisy_threshold_oscillation_cancel_dwell() {
        let mut state = PolicyState::new(PolicyConfig { debounce_ms: 5_000 });
        state.set_enabled(true);
        state.observe_power_source_with_battery_threshold(0, Some(PowerSource::Battery), true);
        assert!(state
            .observe_power_source_with_battery_threshold(5_000, Some(PowerSource::Battery), true)
            .is_empty());
        for (at_ms, percent) in [
            (5_000, Some(24.0)),
            (9_000, Some(26.0)),
            (10_000, Some(24.0)),
            (12_000, None),
            (13_000, Some(24.0)),
            (17_999, Some(24.0)),
        ] {
            assert!(state
                .observe_battery_threshold(at_ms, percent, 25, Some(80), true)
                .is_empty());
        }
        assert_eq!(
            state.observe_battery_threshold(18_000, Some(24.0), 25, Some(80), true),
            vec![PolicyAction::ApplyFor(PowerSource::Battery)]
        );
    }

    #[test]
    fn resume_requires_a_new_stable_source_and_threshold_dwell() {
        let mut state = PolicyState::new(PolicyConfig { debounce_ms: 5_000 });
        state.set_enabled(true);
        state.observe_power_source_with_battery_threshold(0, Some(PowerSource::Battery), true);
        assert!(state
            .observe_power_source_with_battery_threshold(5_000, Some(PowerSource::Battery), true)
            .is_empty());
        state.observe_battery_threshold(5_000, Some(20.0), 25, Some(80), true);
        assert_eq!(
            state.observe_battery_threshold(10_000, Some(20.0), 25, Some(80), true),
            vec![PolicyAction::ApplyFor(PowerSource::Battery)]
        );
        state.confirm_preset_applied(AutomationPreset::Battery);

        state.resume();
        assert!(state
            .observe_power_source_with_battery_threshold(11_000, Some(PowerSource::Battery), true)
            .is_empty());
        assert!(state
            .observe_power_source_with_battery_threshold(15_999, Some(PowerSource::Battery), true)
            .is_empty());
        state.observe_power_source_with_battery_threshold(16_000, Some(PowerSource::Battery), true);
        assert!(state
            .observe_battery_threshold(16_000, Some(20.0), 25, Some(80), true)
            .is_empty());
        assert_eq!(
            state.observe_battery_threshold(21_000, Some(20.0), 25, Some(80), true),
            vec![PolicyAction::ApplyFor(PowerSource::Battery)]
        );
    }
}
