use crate::PowerSource;

const CLOCK_SUSPEND_GAP_TOLERANCE_MS: u64 = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FreshnessSample {
    pub source: PowerSource,
    pub update_time_secs: u64,
    pub source_consistent: bool,
    pub fresh: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FreshnessEvent {
    DaemonStartup,
    ClockAnomaly,
    ClockResume,
    UPowerReconnect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FreshnessDecision {
    Eligible { generation: u64 },
    Invalidated { generation: u64 },
    AwaitingRefresh { generation: u64 },
    Suspended { generation: u64 },
}

#[derive(Debug, Clone, Copy)]
struct RefreshBarrier {
    after_update_time_secs: u64,
    expected_source: Option<PowerSource>,
}

#[derive(Debug, Default)]
pub struct BatteryFreshnessBarrier {
    last_update_time_secs: Option<u64>,
    last_source: Option<PowerSource>,
    barrier: Option<RefreshBarrier>,
    generation: u64,
    suspended: bool,
}

impl BatteryFreshnessBarrier {
    pub fn observe(
        &mut self,
        sample: Option<FreshnessSample>,
        now_epoch_secs: u64,
    ) -> FreshnessDecision {
        let Some(sample) = sample else {
            return self.block_missing(now_epoch_secs, None);
        };
        if sample.update_time_secs == 0 || !sample.source_consistent || !sample.fresh {
            self.last_update_time_secs = Some(
                self.last_update_time_secs
                    .unwrap_or_default()
                    .max(sample.update_time_secs),
            );
            return self.block_missing(now_epoch_secs, Some(sample.update_time_secs));
        }
        if self
            .last_update_time_secs
            .is_some_and(|previous| sample.update_time_secs < previous)
        {
            return self.invalidate(FreshnessEvent::ClockAnomaly, now_epoch_secs);
        }

        if self
            .last_source
            .is_some_and(|previous| previous != sample.source)
        {
            let result = self.begin_barrier(
                now_epoch_secs,
                Some(sample.update_time_secs),
                Some(sample.source),
            );
            self.last_source = Some(sample.source);
            self.last_update_time_secs = Some(
                self.last_update_time_secs
                    .unwrap_or_default()
                    .max(sample.update_time_secs),
            );
            return result;
        }

        if let Some(barrier) = self.barrier {
            if barrier
                .expected_source
                .is_some_and(|expected| expected != sample.source)
            {
                let result = self.begin_barrier(
                    now_epoch_secs,
                    Some(sample.update_time_secs),
                    Some(sample.source),
                );
                self.last_source = Some(sample.source);
                self.last_update_time_secs = Some(
                    self.last_update_time_secs
                        .unwrap_or_default()
                        .max(sample.update_time_secs),
                );
                return result;
            }
            if self.suspended {
                return FreshnessDecision::Suspended {
                    generation: self.generation,
                };
            }
            if sample.update_time_secs <= barrier.after_update_time_secs {
                return FreshnessDecision::AwaitingRefresh {
                    generation: self.generation,
                };
            }
            self.barrier = None;
        }

        self.last_source = Some(sample.source);
        self.last_update_time_secs = Some(sample.update_time_secs);
        FreshnessDecision::Eligible {
            generation: self.generation,
        }
    }

    pub fn prepare_for_sleep(&mut self, preparing: bool, now_epoch_secs: u64) -> FreshnessDecision {
        if preparing {
            if !self.suspended {
                let result = self.begin_barrier(now_epoch_secs, None, self.last_source);
                self.suspended = true;
                return result;
            }
            return FreshnessDecision::Suspended {
                generation: self.generation,
            };
        }

        self.suspended = false;
        self.begin_barrier(now_epoch_secs, None, self.last_source)
    }

    pub fn invalidate(&mut self, event: FreshnessEvent, now_epoch_secs: u64) -> FreshnessDecision {
        if event == FreshnessEvent::ClockResume {
            self.suspended = false;
        }
        self.begin_barrier(now_epoch_secs, None, self.last_source)
    }

    pub fn is_eligible(&self, sample: FreshnessSample) -> bool {
        self.barrier.is_none()
            && !self.suspended
            && sample.source_consistent
            && sample.fresh
            && sample.update_time_secs > 0
            && Some(sample.source) == self.last_source
            && Some(sample.update_time_secs) == self.last_update_time_secs
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn prior_update_time_secs(&self) -> Option<u64> {
        self.last_update_time_secs
    }

    fn block_missing(
        &mut self,
        now_epoch_secs: u64,
        observed_update_time_secs: Option<u64>,
    ) -> FreshnessDecision {
        if let Some(update_time) = observed_update_time_secs {
            if let Some(barrier) = self.barrier.as_mut() {
                barrier.after_update_time_secs = barrier.after_update_time_secs.max(update_time);
            }
        }
        if self.barrier.is_none() {
            self.begin_barrier(now_epoch_secs, observed_update_time_secs, self.last_source)
        } else if self.suspended {
            FreshnessDecision::Suspended {
                generation: self.generation,
            }
        } else {
            FreshnessDecision::AwaitingRefresh {
                generation: self.generation,
            }
        }
    }

    fn begin_barrier(
        &mut self,
        now_epoch_secs: u64,
        observed_update_time_secs: Option<u64>,
        expected_source: Option<PowerSource>,
    ) -> FreshnessDecision {
        let floor = self
            .last_update_time_secs
            .unwrap_or_default()
            .max(observed_update_time_secs.unwrap_or_default())
            .max(now_epoch_secs);
        self.generation = self.generation.saturating_add(1);
        self.barrier = Some(RefreshBarrier {
            after_update_time_secs: floor,
            expected_source,
        });
        FreshnessDecision::Invalidated {
            generation: self.generation,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockObservation {
    Baseline,
    Steady,
    Resumed { gap_ms: u64 },
    Anomaly,
}

#[derive(Debug, Default)]
pub struct SuspendGapDetector {
    previous: Option<(u64, u64)>, // (CLOCK_BOOTTIME, CLOCK_MONOTONIC), milliseconds
}

impl SuspendGapDetector {
    pub fn reset(&mut self) {
        self.previous = None;
    }

    pub fn observe(&mut self, boottime_ms: u64, monotonic_ms: u64) -> ClockObservation {
        let Some((previous_boot, previous_mono)) = self.previous else {
            self.previous = Some((boottime_ms, monotonic_ms));
            return ClockObservation::Baseline;
        };
        let (Some(boot_delta), Some(mono_delta)) = (
            boottime_ms.checked_sub(previous_boot),
            monotonic_ms.checked_sub(previous_mono),
        ) else {
            self.previous = None;
            return ClockObservation::Anomaly;
        };
        self.previous = Some((boottime_ms, monotonic_ms));
        let Some(gap_ms) = boot_delta.checked_sub(mono_delta) else {
            return if mono_delta - boot_delta > CLOCK_SUSPEND_GAP_TOLERANCE_MS {
                ClockObservation::Anomaly
            } else {
                ClockObservation::Steady
            };
        };
        if gap_ms > CLOCK_SUSPEND_GAP_TOLERANCE_MS {
            ClockObservation::Resumed { gap_ms }
        } else {
            ClockObservation::Steady
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PowerSource;

    fn sample(source: PowerSource, update_time_secs: u64) -> FreshnessSample {
        FreshnessSample {
            source,
            update_time_secs,
            source_consistent: true,
            fresh: true,
        }
    }

    #[test]
    fn transition_blocks_cached_update_until_post_transition_refresh() {
        let mut barrier = BatteryFreshnessBarrier::default();
        assert_eq!(
            barrier.observe(Some(sample(PowerSource::Ac, 100)), 101),
            FreshnessDecision::Eligible { generation: 0 }
        );
        assert!(matches!(
            barrier.observe(Some(sample(PowerSource::Battery, 104)), 105),
            FreshnessDecision::Invalidated { generation: 1 }
        ));
        assert!(matches!(
            barrier.observe(Some(sample(PowerSource::Battery, 104)), 106),
            FreshnessDecision::AwaitingRefresh { .. }
        ));
        assert_eq!(
            barrier.observe(Some(sample(PowerSource::Battery, 106)), 106),
            FreshnessDecision::Eligible { generation: 1 }
        );
    }

    #[test]
    fn startup_rejects_cached_sample_until_update_time_advances() {
        let mut barrier = BatteryFreshnessBarrier::default();
        barrier.invalidate(FreshnessEvent::DaemonStartup, 100);
        assert_eq!(
            barrier.observe(Some(sample(PowerSource::Ac, 100)), 101),
            FreshnessDecision::AwaitingRefresh { generation: 1 }
        );
        assert_eq!(
            barrier.observe(Some(sample(PowerSource::Ac, 101)), 101),
            FreshnessDecision::Eligible { generation: 1 }
        );
    }

    #[test]
    fn missing_or_inconsistent_source_blocks_until_new_consistent_update() {
        let mut barrier = BatteryFreshnessBarrier::default();
        barrier.observe(Some(sample(PowerSource::Ac, 100)), 101);
        assert!(matches!(
            barrier.observe(
                Some(FreshnessSample {
                    source_consistent: false,
                    ..sample(PowerSource::Battery, 102)
                }),
                103
            ),
            FreshnessDecision::Invalidated { generation: 1 }
        ));
        assert!(matches!(
            barrier.observe(Some(sample(PowerSource::Battery, 102)), 104),
            FreshnessDecision::Invalidated { generation: 2 }
        ));
        assert!(matches!(
            barrier.observe(Some(sample(PowerSource::Battery, 104)), 104),
            FreshnessDecision::AwaitingRefresh { generation: 2 }
        ));
        assert!(matches!(
            barrier.observe(Some(sample(PowerSource::Battery, 105)), 105),
            FreshnessDecision::Eligible { .. }
        ));
    }

    #[test]
    fn repeated_source_changes_rebase_barrier_and_generation() {
        let mut barrier = BatteryFreshnessBarrier::default();
        barrier.observe(Some(sample(PowerSource::Ac, 100)), 101);
        barrier.observe(Some(sample(PowerSource::Battery, 102)), 103);
        assert!(matches!(
            barrier.observe(Some(sample(PowerSource::Ac, 103)), 104),
            FreshnessDecision::Invalidated { generation: 2 }
        ));
        assert!(matches!(
            barrier.observe(Some(sample(PowerSource::Ac, 104)), 105),
            FreshnessDecision::AwaitingRefresh { generation: 2 }
        ));
        assert!(matches!(
            barrier.observe(Some(sample(PowerSource::Ac, 105)), 105),
            FreshnessDecision::Eligible { generation: 2 }
        ));
    }

    #[test]
    fn suspend_signal_requires_new_update_after_resume() {
        let mut barrier = BatteryFreshnessBarrier::default();
        barrier.observe(Some(sample(PowerSource::Ac, 100)), 101);
        assert!(matches!(
            barrier.prepare_for_sleep(true, 102),
            FreshnessDecision::Invalidated { generation: 1 }
        ));
        assert!(matches!(
            barrier.observe(Some(sample(PowerSource::Ac, 103)), 104),
            FreshnessDecision::Suspended { .. }
        ));
        assert!(matches!(
            barrier.prepare_for_sleep(false, 110),
            FreshnessDecision::Invalidated { generation: 2 }
        ));
        assert!(matches!(
            barrier.observe(Some(sample(PowerSource::Ac, 110)), 111),
            FreshnessDecision::AwaitingRefresh { generation: 2 }
        ));
        assert!(matches!(
            barrier.observe(Some(sample(PowerSource::Ac, 111)), 112),
            FreshnessDecision::Eligible { generation: 2 }
        ));
    }

    #[test]
    fn missing_suspend_signal_is_detected_by_boottime_gap() {
        let mut detector = SuspendGapDetector::default();
        assert_eq!(detector.observe(1_000, 1_000), ClockObservation::Baseline);
        assert_eq!(detector.observe(2_000, 2_000), ClockObservation::Steady);
        assert_eq!(
            detector.observe(12_000, 2_000),
            ClockObservation::Resumed { gap_ms: 10_000 }
        );
    }

    #[test]
    fn clock_regression_and_small_suspicious_gap_are_not_eligible() {
        let mut detector = SuspendGapDetector::default();
        detector.observe(10_000, 10_000);
        assert_eq!(detector.observe(9_000, 11_000), ClockObservation::Anomaly);
        assert_eq!(detector.observe(12_000, 12_000), ClockObservation::Baseline);
        assert_eq!(detector.observe(12_040, 12_000), ClockObservation::Steady);
    }

    #[test]
    fn stale_or_future_update_does_not_clear_barrier() {
        let mut barrier = BatteryFreshnessBarrier::default();
        barrier.observe(Some(sample(PowerSource::Ac, 100)), 101);
        barrier.invalidate(FreshnessEvent::ClockAnomaly, 110);
        assert!(matches!(
            barrier.observe(
                Some(FreshnessSample {
                    fresh: false,
                    ..sample(PowerSource::Ac, 111)
                }),
                111
            ),
            FreshnessDecision::AwaitingRefresh { .. }
        ));
        assert!(matches!(
            barrier.observe(Some(sample(PowerSource::Ac, 111)), 111),
            FreshnessDecision::AwaitingRefresh { .. }
        ));
        assert!(matches!(
            barrier.observe(Some(sample(PowerSource::Ac, 112)), 112),
            FreshnessDecision::Eligible { .. }
        ));
    }

    #[test]
    fn unavailable_sample_cancels_current_eligibility() {
        let mut barrier = BatteryFreshnessBarrier::default();
        barrier.observe(Some(sample(PowerSource::Ac, 100)), 101);
        assert!(matches!(
            barrier.observe(None, 102),
            FreshnessDecision::Invalidated { generation: 1 }
        ));
        assert!(matches!(
            barrier.observe(Some(sample(PowerSource::Ac, 102)), 103),
            FreshnessDecision::AwaitingRefresh { .. }
        ));
        assert!(matches!(
            barrier.observe(Some(sample(PowerSource::Ac, 103)), 104),
            FreshnessDecision::Eligible { .. }
        ));
    }

    #[test]
    fn update_time_regression_invalidates_eligibility() {
        let mut barrier = BatteryFreshnessBarrier::default();
        barrier.observe(Some(sample(PowerSource::Ac, 100)), 101);
        assert!(matches!(
            barrier.observe(Some(sample(PowerSource::Ac, 99)), 102),
            FreshnessDecision::Invalidated { generation: 1 }
        ));
        assert!(!barrier.is_eligible(sample(PowerSource::Ac, 99)));
    }
}
