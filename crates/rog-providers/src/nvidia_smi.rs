use std::time::Duration;

use rog_core::{RogError, RogResult};
use tokio::process::Command;
use tokio::time::timeout;

const MIB: u64 = 1024 * 1024;
const TELEMETRY_FIELDS: &str = "index,uuid,pci.bus_id,name,temperature.gpu,utilization.gpu,memory.used,memory.total,clocks.gr,clocks.mem,power.draw";

#[derive(Debug, Clone, PartialEq)]
pub struct NvidiaGpuTelemetry {
    pub index: u32,
    pub uuid: String,
    pub pci_bus_id: String,
    pub name: String,
    pub temperature_c: Option<f32>,
    pub usage_percent: Option<f32>,
    pub vram_used_bytes: Option<u64>,
    pub vram_total_bytes: Option<u64>,
    pub core_clock_mhz: Option<u32>,
    pub memory_clock_mhz: Option<u32>,
    pub power_w: Option<f32>,
}

impl NvidiaGpuTelemetry {
    pub fn unavailable_field_count(&self) -> usize {
        [
            self.temperature_c.is_none(),
            self.usage_percent.is_none(),
            self.vram_used_bytes.is_none(),
            self.vram_total_bytes.is_none(),
            self.core_clock_mhz.is_none(),
            self.memory_clock_mhz.is_none(),
            self.power_w.is_none(),
        ]
        .into_iter()
        .filter(|missing| *missing)
        .count()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NvidiaSmiProbe {
    Available { gpu_names: Vec<String> },
    NotFound,
    Unavailable(String),
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

    /// Executes a harmless read query so setup checks can distinguish an installed,
    /// working CLI from a binary that merely exists on PATH.
    pub async fn probe(&self) -> NvidiaSmiProbe {
        let mut cmd = Command::new("nvidia-smi");
        cmd.args(["--query-gpu=name", "--format=csv,noheader"]);

        let output = match timeout(Duration::from_millis(self.timeout_ms), cmd.output()).await {
            Ok(Ok(output)) => output,
            Ok(Err(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                return NvidiaSmiProbe::NotFound
            }
            Ok(Err(error)) => {
                return NvidiaSmiProbe::Unavailable(format!(
                    "failed to execute nvidia-smi: {error}"
                ))
            }
            Err(_) => return NvidiaSmiProbe::Unavailable("nvidia-smi timed out".to_string()),
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return NvidiaSmiProbe::Unavailable(if stderr.is_empty() {
                format!("nvidia-smi exited with {}", output.status)
            } else {
                format!("nvidia-smi failed: {stderr}")
            });
        }

        let gpu_names = String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        NvidiaSmiProbe::Available { gpu_names }
    }

    /// Reads all supported metrics in one bounded process invocation. `Ok(None)` means the
    /// command is absent or produced no valid GPU rows, which is normal on non-NVIDIA systems.
    pub async fn read_telemetry(&self) -> RogResult<Option<NvidiaGpuTelemetry>> {
        let mut cmd = Command::new("nvidia-smi");
        cmd.args([
            &format!("--query-gpu={TELEMETRY_FIELDS}"),
            "--format=csv,noheader,nounits",
        ]);

        let output = match timeout(Duration::from_millis(self.timeout_ms), cmd.output()).await {
            Ok(Ok(output)) => output,
            Ok(Err(error)) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Ok(Err(error)) => {
                return Err(RogError::Unexpected(format!(
                    "failed to execute nvidia-smi: {error}"
                )))
            }
            Err(_) => {
                return Err(RogError::TransientFailure(
                    "nvidia-smi timed out".to_string(),
                ))
            }
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(RogError::TransientFailure(if stderr.is_empty() {
                format!("nvidia-smi exited with {}", output.status)
            } else {
                format!("nvidia-smi failed: {stderr}")
            }));
        }

        Ok(parse_telemetry_output(&String::from_utf8_lossy(
            &output.stdout,
        )))
    }
}

/// Parse every GPU row, then select the lowest reported NVIDIA index (with PCI bus ID as a
/// deterministic tie-breaker). This makes selection independent of command output row order while
/// retaining NVIDIA's stable primary-device identity for the daemon lifetime.
fn parse_telemetry_output(stdout: &str) -> Option<NvidiaGpuTelemetry> {
    let mut rows = stdout
        .lines()
        .filter_map(parse_telemetry_row)
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        left.index
            .cmp(&right.index)
            .then_with(|| left.pci_bus_id.cmp(&right.pci_bus_id))
            .then_with(|| left.uuid.cmp(&right.uuid))
    });
    rows.into_iter().next()
}

fn parse_telemetry_row(line: &str) -> Option<NvidiaGpuTelemetry> {
    let fields = parse_csv_line(line)?;
    if fields.len() != 11 {
        return None;
    }
    let index = parse_u32(&fields[0], 0, u32::MAX)?;
    let uuid = required_text(&fields[1])?;
    let pci_bus_id = required_text(&fields[2])?;
    let name = required_text(&fields[3])?;
    Some(NvidiaGpuTelemetry {
        index,
        uuid,
        pci_bus_id,
        name,
        temperature_c: parse_f32(&fields[4], 0.0, 200.0),
        usage_percent: parse_f32(&fields[5], 0.0, 100.0),
        vram_used_bytes: parse_mib(&fields[6]),
        vram_total_bytes: parse_mib(&fields[7]),
        core_clock_mhz: parse_u32(&fields[8], 1, 50_000),
        memory_clock_mhz: parse_u32(&fields[9], 1, 50_000),
        power_w: parse_f32(&fields[10], 0.0, 2_000.0),
    })
}

fn parse_csv_line(line: &str) -> Option<Vec<String>> {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut chars = line.chars().peekable();
    let mut quoted = false;
    while let Some(ch) = chars.next() {
        match ch {
            '"' if quoted && chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => {
                fields.push(field.trim().to_string());
                field.clear();
            }
            _ => field.push(ch),
        }
    }
    if quoted {
        return None;
    }
    fields.push(field.trim().to_string());
    Some(fields)
}

fn unavailable(value: &str) -> bool {
    let value = value.trim();
    value.is_empty()
        || value.eq_ignore_ascii_case("n/a")
        || value.eq_ignore_ascii_case("[n/a]")
        || value.eq_ignore_ascii_case("not supported")
        || value.eq_ignore_ascii_case("[not supported]")
}

fn required_text(value: &str) -> Option<String> {
    (!unavailable(value)).then(|| value.trim().to_string())
}

fn parse_f32(value: &str, minimum: f32, maximum: f32) -> Option<f32> {
    if unavailable(value) {
        return None;
    }
    let value = value.trim().parse::<f32>().ok()?;
    (value.is_finite() && (minimum..=maximum).contains(&value)).then_some(value)
}

fn parse_u32(value: &str, minimum: u32, maximum: u32) -> Option<u32> {
    if unavailable(value) {
        return None;
    }
    let value = value.trim().parse::<u32>().ok()?;
    (minimum..=maximum).contains(&value).then_some(value)
}

fn parse_mib(value: &str) -> Option<u64> {
    if unavailable(value) {
        return None;
    }
    let mib = value.trim().parse::<u64>().ok()?;
    mib.checked_mul(MIB)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_normal_result() {
        let sample = parse_telemetry_output(
            "0, GPU-a, 00000000:01:00.0, NVIDIA GeForce RTX 4070, 46, 32, 2150, 8188, 2100, 7000, 42.75\n",
        )
        .unwrap();
        assert_eq!(sample.index, 0);
        assert_eq!(sample.name, "NVIDIA GeForce RTX 4070");
        assert_eq!(sample.temperature_c, Some(46.0));
        assert_eq!(sample.usage_percent, Some(32.0));
        assert_eq!(sample.vram_used_bytes, Some(2150 * MIB));
        assert_eq!(sample.vram_total_bytes, Some(8188 * MIB));
        assert_eq!(sample.core_clock_mhz, Some(2100));
        assert_eq!(sample.memory_clock_mhz, Some(7000));
        assert_eq!(sample.power_w, Some(42.75));
    }

    #[test]
    fn na_fields_remain_absent() {
        let sample = parse_telemetry_output(
            "0, GPU-a, 00000000:01:00.0, NVIDIA GPU, N/A, [N/A], N/A, 8192, [Not Supported], N/A, N/A\n",
        )
        .unwrap();
        assert_eq!(sample.temperature_c, None);
        assert_eq!(sample.usage_percent, None);
        assert_eq!(sample.vram_used_bytes, None);
        assert_eq!(sample.vram_total_bytes, Some(8192 * MIB));
        assert_eq!(sample.core_clock_mhz, None);
        assert_eq!(sample.power_w, None);
    }

    #[test]
    fn malformed_rows_are_ignored() {
        assert_eq!(parse_telemetry_output("driver is unloaded\n"), None);
        assert_eq!(
            parse_telemetry_output("x, GPU-a, bus, name, 46, 32, 1, 2, 3, 4, 5\n"),
            None
        );
        assert_eq!(parse_telemetry_output("\"unterminated, row\n"), None);
    }

    #[test]
    fn multiple_gpus_select_lowest_index_not_first_row() {
        let sample = parse_telemetry_output(
            "2, GPU-c, 00000000:03:00.0, Secondary, 55, 60, 100, 1000, 1000, 2000, 80\n0, GPU-a, 00000000:01:00.0, Primary, 44, 20, 50, 8000, 900, 1800, 30\n",
        )
        .unwrap();
        assert_eq!(sample.index, 0);
        assert_eq!(sample.uuid, "GPU-a");
        assert_eq!(sample.name, "Primary");
    }

    #[test]
    fn quoted_names_are_parsed() {
        let sample = parse_telemetry_output(
            "0, GPU-a, 00000000:01:00.0, \"NVIDIA, Test GPU\", 46, 32, 1, 2, 3, 4, 5\n",
        )
        .unwrap();
        assert_eq!(sample.name, "NVIDIA, Test GPU");
    }
}
