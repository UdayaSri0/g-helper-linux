#![allow(async_fn_in_trait)]

use rog_core::{
    BatteryLimitPercent, FanCurve, FanDomain, GpuMode, LightingState, PerformanceProfile,
    TelemetrySnapshot,
};

use rog_core::RogResult;

pub trait ProfileProvider {
    async fn get_profile(&self) -> RogResult<PerformanceProfile>;
    async fn set_profile(&self, profile: PerformanceProfile) -> RogResult<()>;
}

pub trait FanProvider {
    async fn get_fan_rpm(&self) -> RogResult<Vec<(FanDomain, u32)>>;
    async fn supports_curves(&self) -> RogResult<bool>;
    async fn get_curve(&self, domain: FanDomain) -> RogResult<FanCurve>;
    async fn set_curve(&self, domain: FanDomain, curve: FanCurve) -> RogResult<()>;
}

pub trait GpuProvider {
    async fn get_mode(&self) -> RogResult<GpuMode>;
    async fn set_mode(&self, mode: GpuMode) -> RogResult<()>;

    /// Returns `Ok(None)` if switching is considered safe right now, or `Ok(Some(msg))` with a
    /// hint/warning for the UI (e.g. "dGPU busy").
    async fn can_switch_now(&self) -> RogResult<Option<String>>;
}

pub trait BatteryProvider {
    async fn get_limit(&self) -> RogResult<BatteryLimitPercent>;
    async fn set_limit(&self, limit: BatteryLimitPercent) -> RogResult<()>;
}

pub trait LightingProvider {
    async fn get_state(&self) -> RogResult<LightingState>;
    async fn set_state(&self, state: LightingState) -> RogResult<()>;
}

pub trait TelemetryProvider {
    async fn get_snapshot(&self) -> RogResult<TelemetrySnapshot>;
}
