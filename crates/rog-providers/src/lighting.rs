use rog_core::{LightingDiagnostics, LightingMode, LightingState};

use crate::aura::{AuraProbeDiagnostics, AuraProvider};
use crate::kbd_backlight::KbdBacklightSysfs;

pub fn build_lighting_diagnostics(
    kbd_backlight: Option<&KbdBacklightSysfs>,
    kbd_backlight_detected: bool,
    kbd_backlight_probe_error: Option<&str>,
    aura_provider: Option<&AuraProvider>,
    aura_probe: &AuraProbeDiagnostics,
    aura_state: Option<&LightingState>,
    aura_state_error: Option<&str>,
) -> LightingDiagnostics {
    let mut diagnostics = LightingDiagnostics::unknown();

    diagnostics.keyboard_backlight_detected = kbd_backlight_detected || kbd_backlight.is_some();
    diagnostics.asusd_service_detected = aura_probe.service_detected;
    diagnostics.asusd_service_name = aura_probe.service_name.clone();
    diagnostics.asusd_services_checked = aura_probe.services_checked.clone();
    diagnostics.asusd_object_paths_checked = aura_probe.object_paths_checked.clone();
    diagnostics.asusd_interfaces_detected = aura_probe.interfaces_detected.clone();
    diagnostics.asusd_aura_interface_detected = aura_probe.aura_interface_detected;
    diagnostics.asusd_keyboard_interface_detected = aura_probe.keyboard_interface_detected;
    diagnostics.asusd_potential_aura_interfaces = aura_probe.potential_aura_interfaces.clone();
    diagnostics.asusd_rgb_methods_detected = aura_probe.rgb_methods_detected.clone();
    diagnostics.asusd_rgb_properties_detected = aura_probe.rgb_properties_detected.clone();
    diagnostics.probe_errors = aura_probe.probe_errors.clone();

    if let Some(error) = kbd_backlight_probe_error {
        diagnostics.last_probe_error = Some(format!("keyboard backlight probe failed: {error}"));
        diagnostics
            .probe_errors
            .push(format!("keyboard backlight probe failed: {error}"));
    }
    if let Some(error) = aura_state_error {
        diagnostics.last_probe_error = Some(format!("Aura state read failed: {error}"));
        diagnostics
            .probe_errors
            .push(format!("Aura state read failed: {error}"));
    }

    if let Some(kbd) = kbd_backlight {
        let current_brightness = kbd.read_brightness().ok();
        diagnostics.keyboard_backlight_backend = Some("sysfs-led".to_string());
        diagnostics.keyboard_backlight_device = Some(kbd.led_name().to_string());
        diagnostics.keyboard_backlight_path = Some(kbd.led_path().display().to_string());
        diagnostics.keyboard_backlight_brightness_path =
            Some(kbd.brightness_path().display().to_string());
        diagnostics.keyboard_backlight_current_brightness = current_brightness;
        diagnostics.keyboard_backlight_max_brightness = Some(kbd.max_brightness());
        diagnostics.keyboard_backlight_readable = current_brightness.is_some();
        diagnostics.keyboard_backlight_writable = kbd.can_set_brightness();
        diagnostics.supports_brightness = true;

        if !diagnostics.keyboard_backlight_writable {
            diagnostics.permission_warning = Some(
                "Keyboard backlight brightness was detected, but the brightness file is not writable by the current user/session daemon."
                    .to_string(),
            );
        }
    }

    if let Some(state) = aura_state {
        diagnostics.supports_brightness |= state.brightness.is_some();
        diagnostics.supports_modes |= !state.supported_modes.is_empty() || state.mode.is_some();
        diagnostics.supported_modes = state.supported_mode_labels();
        diagnostics.active_mode = state.mode_label();
        diagnostics.supports_rgb = state.supports_rgb;
        diagnostics.rgb_current_hex = state.rgb.map(|rgb| rgb.to_hex());
        if state.supports_rgb || aura_provider.is_some() {
            diagnostics.rgb_backend_detected = true;
            diagnostics.rgb_backend_name = Some(state.backend.clone());
        }
        if state.last_error.is_some() {
            diagnostics.last_probe_error = state.last_error.clone();
        }
    }

    if diagnostics.supported_modes.is_empty() && kbd_backlight.is_some() {
        diagnostics.supported_modes = vec![LightingMode::Off.label(), LightingMode::Static.label()];
    }
    diagnostics.supports_modes =
        diagnostics.supports_modes || !diagnostics.supported_modes.is_empty();
    if diagnostics.active_mode.is_none() {
        diagnostics.active_mode = kbd_backlight
            .and_then(|kbd| kbd.read_brightness().ok())
            .map(|brightness| {
                if brightness == 0 {
                    LightingMode::Off.label()
                } else {
                    LightingMode::Static.label()
                }
            });
    }

    if let Some(provider) = aura_provider {
        diagnostics.active_backend = "asusd-aura".to_string();
        diagnostics.rgb_backend_detected = true;
        diagnostics
            .rgb_backend_name
            .get_or_insert_with(|| provider.endpoint_tag());
        diagnostics.supports_rgb |= provider.supports_rgb();
        diagnostics.supports_brightness |= provider.supports_brightness();
        if diagnostics.supported_modes.is_empty() {
            diagnostics.supported_modes = provider
                .supported_modes_hint()
                .iter()
                .map(LightingMode::label)
                .collect();
        }
        diagnostics.supports_modes =
            diagnostics.supports_modes || !diagnostics.supported_modes.is_empty();
        if !provider.supports_rgb() {
            diagnostics.fallback_reason = Some(
                "Aura/keyboard lighting was detected through asusd, but writable RGB colour control was not exposed by that backend."
                    .to_string(),
            );
        }
    } else if kbd_backlight.is_some() {
        diagnostics.active_backend = "sysfs-led".to_string();
        diagnostics.fallback_reason = if diagnostics.asusd_service_detected {
            Some(
                "Keyboard brightness is available through the sysfs LED backend, but asusd did not expose a supported Aura/RGB keyboard interface."
                    .to_string(),
            )
        } else {
            Some(
                "Keyboard brightness is available through the sysfs LED backend, but RGB colour control is not available because no Aura/RGB backend was detected."
                    .to_string(),
            )
        };
    } else {
        diagnostics.active_backend = "none".to_string();
        diagnostics.unavailable_reason = if diagnostics.asusd_service_detected {
            Some(
                "asusd is running, but no supported keyboard lighting backend was detected."
                    .to_string(),
            )
        } else {
            Some(
                "No sysfs keyboard backlight and no asusd Aura/RGB backend were detected."
                    .to_string(),
            )
        };
    }

    diagnostics.recommended_action = recommended_action(&diagnostics);
    diagnostics
}

fn recommended_action(diagnostics: &LightingDiagnostics) -> Option<String> {
    if diagnostics.supports_rgb {
        return None;
    }
    if !diagnostics.keyboard_backlight_writable && diagnostics.keyboard_backlight_detected {
        return Some(
            "Inspect sysfs LED permissions if you need brightness writes, and run the lighting diagnostics command when filing a report."
                .to_string(),
        );
    }
    if !diagnostics.asusd_service_detected {
        return Some(
            "Install/start asusd if this laptop should expose ASUS Aura RGB, then rerun lighting diagnostics."
                .to_string(),
        );
    }
    if !diagnostics.asusd_potential_aura_interfaces.is_empty() && !diagnostics.rgb_backend_detected
    {
        return Some(
            "Potential Aura/RGB DBus interfaces were found but not implemented; include the introspection output in a GitHub issue."
                .to_string(),
        );
    }
    if diagnostics.asusd_service_detected && !diagnostics.rgb_backend_detected {
        return Some(
            "asusd is reachable but did not expose Aura/RGB keyboard control; attach `rog-helper lighting-diagnostics` and DBus introspection output to a GitHub issue if the hardware supports RGB."
                .to_string(),
        );
    }
    None
}
