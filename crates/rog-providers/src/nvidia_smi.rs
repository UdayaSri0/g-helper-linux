use std::time::Duration;

use rog_core::{RogError, RogResult};
use tokio::process::Command;
use tokio::time::timeout;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NvidiaGpuClocks {
    pub core_clock_mhz: Option<u32>,
    pub memory_clock_mhz: Option<u32>,
}

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

    /// Returns graphics/core and memory clocks in MHz when `nvidia-smi` exposes them.
    pub async fn read_gpu_clocks_mhz(&self) -> RogResult<Option<NvidiaGpuClocks>> {
        let mut cmd = Command::new("nvidia-smi");
        cmd.args([
            "--query-gpu=clocks.gr,clocks.mem",
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
        Ok(parse_clock_row(&stdout))
    }
}

fn parse_clock_row(stdout: &str) -> Option<NvidiaGpuClocks> {
    let first = stdout.lines().find(|line| !line.trim().is_empty())?;
    let mut parts = first.split(',').map(parse_clock_mhz);
    let core_clock_mhz = parts.next().flatten();
    let memory_clock_mhz = parts.next().flatten();
    if core_clock_mhz.is_none() && memory_clock_mhz.is_none() {
        return None;
    }
    Some(NvidiaGpuClocks {
        core_clock_mhz,
        memory_clock_mhz,
    })
}

fn parse_clock_mhz(value: &str) -> Option<u32> {
    let mhz = value.trim().parse::<u32>().ok()?;
    // Consumer GPU clocks outside this broad range are almost certainly not useful telemetry.
    (1..=50_000).contains(&mhz).then_some(mhz)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nvidia_clock_csv() {
        let clocks = parse_clock_row("2100, 7000\n").unwrap();
        assert_eq!(clocks.core_clock_mhz, Some(2100));
        assert_eq!(clocks.memory_clock_mhz, Some(7000));
    }

    #[test]
    fn unsupported_nvidia_clock_values_become_unavailable() {
        assert_eq!(parse_clock_row("[Not Supported], [Not Supported]\n"), None);
    }
}
