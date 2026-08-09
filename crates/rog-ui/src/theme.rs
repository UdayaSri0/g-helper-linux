use gtk4 as gtk;

pub fn install() {
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::PreferDark);

    let css = r#"
window { background: #101318; }
headerbar { background: #151920; border-bottom: 1px solid alpha(white, 0.06); }
.sidebar { background: #12161c; }
.navigation-caption {
  color: alpha(white, 0.48);
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 1.2px;
  margin: 0 12px 4px 12px;
}
.navigation-list { background: transparent; }
.navigation-row {
  border-radius: 10px;
  margin: 2px 0;
  color: alpha(white, 0.72);
}
.navigation-row:hover { background: alpha(white, 0.055); }
.navigation-row:selected {
  background: alpha(#4da3ff, 0.16);
  color: #d9ecff;
  box-shadow: inset 3px 0 #4da3ff;
}
.connection-status,
.status-chip,
.fans-status-pill {
  padding: 4px 10px;
  border-radius: 999px;
  background: alpha(#4da3ff, 0.12);
  color: #b9dcff;
  font-size: 11px;
  font-weight: 600;
}
.connection-status.connected {
  background: alpha(#39c58a, 0.14);
  color: #7de0b2;
}
.connection-status.disconnected {
  background: alpha(#ef5b68, 0.14);
  color: #ff9ba4;
}
.page-header { margin: 4px 0 8px 0; }
.page-title { font-size: 28px; font-weight: 750; color: #f4f7fb; }
.page-subtitle { font-size: 14px; color: alpha(white, 0.58); }
.metric-card,
.surface-card,
.fans-header-card,
.fans-hero,
.fan-visual-card,
.fan-card,
.fan-control-panel,
.fan-curve-card,
.fan-diagnostics-box,
.fans-gauge-card {
  padding: 18px;
  border-radius: 12px;
  background: #1a1f27;
  border: 1px solid alpha(white, 0.07);
  box-shadow: 0 1px 3px alpha(black, 0.18);
}
.metric-card-primary { min-height: 126px; }
.metric-card-title { color: alpha(white, 0.58); font-size: 12px; font-weight: 650; }
.metric-value { color: #f6f8fb; font-size: 30px; font-weight: 750; }
.metric-unit { color: alpha(white, 0.50); font-size: 16px; font-weight: 600; padding-bottom: 3px; }
.metric-subtitle { color: alpha(white, 0.56); font-size: 12px; }
.history-graph {
  border-radius: 10px;
  background: alpha(black, 0.14);
  border: 1px solid alpha(white, 0.055);
}
.memory-progress progress,
.battery-progress progress { background: #4da3ff; min-height: 10px; border-radius: 99px; }
.memory-progress trough,
.battery-progress trough { background: alpha(white, 0.08); min-height: 10px; border-radius: 99px; }
.cpu-toggle-row { padding: 8px 4px; }
.cpu-toggle-title { font-weight: 600; }
.cpu-toggle-subtitle { opacity: 0.78; }
.fans-page { background: transparent; }
.fans-gauge-card-large { min-width: 280px; }
.fans-status-pill-read-only { background: alpha(#6fa8d6, 0.14); color: #a8cdea; }
.fans-status-pill-controllable { background: alpha(#39c58a, 0.14); color: #7de0b2; }
.fans-status-pill-warning { background: alpha(#e9a23b, 0.16); color: #f4c775; }
.fans-status-pill-error { background: alpha(#ef5b68, 0.14); color: #ff9ba4; }
.fans-warning-banner { border-radius: 10px; }
.fan-rotor-card { min-width: 145px; }
.fan-card-read-only { border-color: alpha(#6fa8d6, 0.13); }
.fan-card-controllable { border-color: alpha(#39c58a, 0.20); }
.fan-rpm-large { font-size: 22px; font-weight: 700; }
.fan-rpm-label { font-weight: 600; }
.fan-panel-note {
  padding: 8px;
  border-radius: 8px;
  background: alpha(white, 0.045);
}
.fan-warning-text {
  padding: 8px;
  border-radius: 8px;
  background: alpha(#e9a23b, 0.11);
  color: #f4c775;
}
.fan-rotor-widget,
.fan-gauge-widget,
.fan-curve-widget,
.fans-gauge-row,
.fans-rotor-grid { background: transparent; }
"#;

    let provider = gtk::CssProvider::new();
    provider.load_from_data(css);
    let Some(display) = gtk::gdk::Display::default() else {
        return;
    };
    gtk::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}
