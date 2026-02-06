use std::time::Duration;

use rog_core::{RogError, RogResult};
use tokio::process::Command;
use tokio::time::timeout;

#[derive(Debug, Clone, Copy)]
pub struct NvidiaSmiTelemetryProvider {
    pub timeout_ms: u64,
}

impl Default for NvidiaSmiTelemetryProvider {
    fn default() -> Self {
        Self { timeout_ms: 800 }
    }
}

impl NvidiaSmiTelemetryProvider {
    pub fn new(timeout_ms: u64) -> Self {
        Self { timeout_ms }
    }

    /// Returns `Ok(None)` if `nvidia-smi` is not available.
    pub async fn read_gpu_temp_c(&self) -> RogResult<Option<f32>> {
        let mut cmd = Command::new("nvidia-smi");
        cmd.args([
            "--query-gpu=temperature.gpu",
            "--format=csv,noheader,nounits",
        ]);

        let output = match timeout(Duration::from_millis(self.timeout_ms), cmd.output()).await {
            Ok(r) => match r {
                Ok(o) => o,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(e) => {
                    return Err(RogError::Unexpected(format!(
                        "failed to execute nvidia-smi: {e}"
                    )))
                }
            },
            Err(_) => {
                return Err(RogError::TransientFailure(
                    "nvidia-smi timed out".to_string(),
                ))
            }
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(RogError::TransientFailure(format!(
                "nvidia-smi failed: {stderr}"
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let first = stdout.lines().find(|l| !l.trim().is_empty());
        let Some(first) = first else {
            return Ok(None);
        };

        let temp: f32 = match first.trim().parse::<f32>() {
            Ok(v) => v,
            Err(_) => return Ok(None),
        };

        // Sanity-check: Celsius readings outside this range are almost certainly parsing errors.
        if !(0.0..=200.0).contains(&temp) {
            return Ok(None);
        }

        Ok(Some(temp))
    }
}
