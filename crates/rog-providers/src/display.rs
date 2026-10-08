use std::process::{Command, Output};

const XRANDR: &str = "/usr/bin/xrandr";

#[derive(Debug, Clone, PartialEq)]
pub struct DisplaySnapshot {
    pub session_type: String,
    pub backend: String,
    pub internal_output: Option<String>,
    pub current_mode: Option<String>,
    pub current_rate: Option<f64>,
    pub supported_rates: Vec<f64>,
    pub reason: String,
}

#[derive(Default)]
struct ParsedOutput {
    name: String,
    connected: bool,
    mode: Option<String>,
    current_rate: Option<f64>,
    rates: Vec<f64>,
}

impl DisplaySnapshot {
    fn unavailable(session_type: &str, backend: &str, reason: &str) -> Self {
        Self {
            session_type: session_type.to_string(),
            backend: backend.to_string(),
            internal_output: None,
            current_mode: None,
            current_rate: None,
            supported_rates: Vec::new(),
            reason: reason.to_string(),
        }
    }

    pub fn supported(&self) -> bool {
        self.internal_output.is_some()
            && self.current_mode.is_some()
            && self.current_rate.is_some()
            && self.supported_rates.len() > 1
    }
}

pub fn discover() -> DisplaySnapshot {
    let session_type = std::env::var("XDG_SESSION_TYPE").unwrap_or_default();
    if !session_type.eq_ignore_ascii_case("x11") {
        return DisplaySnapshot::unavailable(
            if session_type.is_empty() { "unknown" } else { &session_type },
            "unsupported",
            "Refresh-rate control is available only in an X11 session; Wayland needs a supported compositor API.",
        );
    }
    let output = match Command::new(XRANDR).arg("--query").output() {
        Ok(output) if output.status.success() => output,
        Ok(output) => {
            return DisplaySnapshot::unavailable(
                "x11",
                "XRandR",
                &format!("xrandr query failed: {}", stderr(&output)),
            )
        }
        Err(error) => {
            return DisplaySnapshot::unavailable(
                "x11",
                "XRandR",
                &format!("Unable to run /usr/bin/xrandr: {error}"),
            )
        }
    };
    parse_xrandr_query("x11", &String::from_utf8_lossy(&output.stdout))
}

pub fn parse_xrandr_query(session_type: &str, query: &str) -> DisplaySnapshot {
    if !session_type.eq_ignore_ascii_case("x11") {
        return DisplaySnapshot::unavailable(
            session_type,
            "unsupported",
            "XRandR is not used on Wayland sessions.",
        );
    }

    let mut internal = Vec::new();
    let mut current: Option<ParsedOutput> = None;
    for line in query.lines() {
        if !line.starts_with(char::is_whitespace) {
            if let Some(output) = current.take() {
                if is_internal_connector(&output.name) && output.connected {
                    internal.push(output);
                }
            }
            let mut fields = line.split_whitespace();
            let Some(name) = fields.next() else { continue };
            let Some(connection) = fields.next() else {
                continue;
            };
            let connected = connection == "connected";
            let mode = if connected {
                fields
                    .find_map(|field| field.split('+').next().filter(|part| is_mode(part)))
                    .map(str::to_string)
            } else {
                None
            };
            current = Some(ParsedOutput {
                name: name.to_string(),
                connected,
                mode,
                ..ParsedOutput::default()
            });
        } else if let Some(output) = current.as_mut() {
            if !output.connected {
                continue;
            }
            let mut fields = line.split_whitespace();
            let Some(candidate_mode) = fields.next() else {
                continue;
            };
            if !is_mode(candidate_mode) || output.mode.as_deref() != Some(candidate_mode) {
                continue;
            }
            for field in fields {
                let marked_current = field.contains('*');
                let value = field.trim_matches(['*', '+']).parse::<f64>();
                if let Ok(value) = value {
                    if value.is_finite() && value > 0.0 {
                        if !output
                            .rates
                            .iter()
                            .any(|known| (known - value).abs() < 0.01)
                        {
                            output.rates.push(value);
                        }
                        if marked_current {
                            output.current_rate = Some(value);
                        }
                    }
                }
            }
        }
    }
    if let Some(output) = current {
        if is_internal_connector(&output.name) && output.connected {
            internal.push(output);
        }
    }
    if internal.len() != 1 {
        let reason = if internal.is_empty() {
            "No connected internal display was identified; external-display-only sessions are read-only."
        } else {
            "More than one internal display was identified; automatic target selection is disabled."
        };
        return DisplaySnapshot::unavailable("x11", "XRandR", reason);
    }
    let output = internal.pop().unwrap();
    DisplaySnapshot {
        session_type: "x11".into(),
        backend: "XRandR (X11)".into(),
        internal_output: Some(output.name),
        current_mode: output.mode,
        current_rate: output.current_rate,
        supported_rates: output.rates,
        reason: "Only refresh rates reported for the current internal-panel mode are offered."
            .into(),
    }
}

fn is_internal_connector(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.starts_with("edp-") || lower.starts_with("lvds-")
}

fn is_mode(value: &str) -> bool {
    let Some((width, height)) = value.split_once('x') else {
        return false;
    };
    width.parse::<u32>().is_ok_and(|value| value > 0)
        && height.parse::<u32>().is_ok_and(|value| value > 0)
}

pub fn apply_refresh_rate(rate: f64) -> Result<DisplaySnapshot, String> {
    let before = discover();
    if !before.supported() {
        return Err(before.reason);
    }
    apply_refresh_rate_with(&before, rate, run_xrandr, discover)
}

fn apply_refresh_rate_with(
    before: &DisplaySnapshot,
    rate: f64,
    mut run: impl FnMut(&[String]) -> Result<(), String>,
    mut readback: impl FnMut() -> DisplaySnapshot,
) -> Result<DisplaySnapshot, String> {
    if !before.supported()
        || !rate.is_finite()
        || !before
            .supported_rates
            .iter()
            .any(|supported| (supported - rate).abs() < 0.01)
    {
        return Err(
            "The selected refresh rate is not currently advertised for the internal panel.".into(),
        );
    }
    let (Some(output), Some(mode), Some(previous_rate)) = (
        before.internal_output.as_deref(),
        before.current_mode.as_deref(),
        before.current_rate,
    ) else {
        return Err("The current internal display mode is unavailable; no change was made.".into());
    };
    if !is_internal_connector(output) || !is_mode(mode) {
        return Err("The discovered display identity was invalid; no change was made.".into());
    }
    if (previous_rate - rate).abs() < 0.01 {
        return Ok(before.clone());
    }

    let args = set_rate_args(output, mode, rate);
    if let Err(error) = run(&args) {
        let rollback = run(&set_rate_args(output, mode, previous_rate));
        return Err(match rollback {
            Ok(()) => format!("Refresh change failed; restored the previous {previous_rate:.2} Hz mode: {error}"),
            Err(rollback_error) => format!("Refresh change failed ({error}); restoring the previous rate also failed ({rollback_error}). Check the desktop Display settings."),
        });
    }

    let after = readback();
    if after.internal_output.as_deref() == Some(output)
        && after.current_mode.as_deref() == Some(mode)
        && after
            .current_rate
            .is_some_and(|actual| (actual - rate).abs() < 0.01)
    {
        return Ok(after);
    }

    let rollback = run(&set_rate_args(output, mode, previous_rate));
    Err(match rollback {
        Ok(()) => format!("The requested mode was not confirmed by readback; restored {previous_rate:.2} Hz."),
        Err(error) => format!("The requested mode was not confirmed and rollback failed ({error}). Check the desktop Display settings."),
    })
}

fn set_rate_args(output: &str, mode: &str, rate: f64) -> Vec<String> {
    vec![
        "--output".into(),
        output.into(),
        "--mode".into(),
        mode.into(),
        "--rate".into(),
        format!("{rate:.2}"),
    ]
}

fn run_xrandr(args: &[String]) -> Result<(), String> {
    let output = Command::new(XRANDR)
        .args(args)
        .output()
        .map_err(|error| format!("unable to run /usr/bin/xrandr: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "xrandr rejected the mode change: {}",
            stderr(&output)
        ))
    }
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const QUERY: &str = "Screen 0: minimum 8 x 8, current 1920 x 1200, maximum 32767 x 32767\nHDMI-0 connected primary 1920x1080+0+0 (normal) 530mm x 300mm\n   1920x1080 60.00*+ 59.94\neDP-1-2 connected 1920x1200+0+0 (normal) 345mm x 215mm\n   1920x1200 165.00*+ 60.00\n   1920x1080 60.00\nDP-1 disconnected (normal)\n";

    #[test]
    fn discovers_internal_output_and_rates_for_current_mode() {
        let display = parse_xrandr_query("x11", QUERY);
        assert_eq!(display.internal_output.as_deref(), Some("eDP-1-2"));
        assert_eq!(display.current_mode.as_deref(), Some("1920x1200"));
        assert_eq!(display.current_rate, Some(165.0));
        assert_eq!(display.supported_rates, vec![165.0, 60.0]);
        assert!(display.supported());
    }

    #[test]
    fn wayland_and_external_only_are_read_only() {
        let wayland = parse_xrandr_query("wayland", QUERY);
        assert!(!wayland.supported());
        assert_eq!(wayland.backend, "unsupported");
        let external = parse_xrandr_query(
            "x11",
            "HDMI-0 connected 1920x1080+0+0 (normal)\n   1920x1080 60.00*\n",
        );
        assert!(!external.supported());
        assert!(external.reason.contains("external-display-only"));
    }

    #[test]
    fn failed_mode_change_attempts_known_good_rollback() {
        let before = parse_xrandr_query("x11", QUERY);
        let mut calls = Vec::new();
        let result = apply_refresh_rate_with(
            &before,
            60.0,
            |args| {
                calls.push(args.to_vec());
                if calls.len() == 1 {
                    Err("mode failure".into())
                } else {
                    Ok(())
                }
            },
            || before.clone(),
        );
        assert!(result
            .unwrap_err()
            .contains("restored the previous 165.00 Hz"));
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0][5], "60.00");
        assert_eq!(calls[1][5], "165.00");
    }

    #[test]
    fn rate_not_advertised_for_current_mode_is_rejected_without_write() {
        let before = parse_xrandr_query("x11", QUERY);
        let mut wrote = false;
        let result = apply_refresh_rate_with(
            &before,
            59.94,
            |_| {
                wrote = true;
                Ok(())
            },
            || before.clone(),
        );
        assert!(result.is_err());
        assert!(!wrote);
    }

    #[test]
    fn successful_change_requires_matching_readback() {
        let before = parse_xrandr_query("x11", QUERY);
        let after_query = QUERY.replace("165.00*+ 60.00", "165.00+ 60.00*");
        let after = parse_xrandr_query("x11", &after_query);
        let mut command = None;
        let result = apply_refresh_rate_with(
            &before,
            60.0,
            |args| {
                command = Some(args.to_vec());
                Ok(())
            },
            || after.clone(),
        )
        .unwrap();
        assert_eq!(result.current_rate, Some(60.0));
        let command = command.unwrap();
        assert_eq!(command[1], "eDP-1-2");
        assert_eq!(command[3], "1920x1200");
        assert_eq!(command[5], "60.00");
    }
}
