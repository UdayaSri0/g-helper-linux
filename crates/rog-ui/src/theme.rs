use gtk4 as gtk;

pub fn install() {
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::PreferDark);

    let css = r#"
window { background: #101318; }
headerbar { background: #151920; border-bottom: 1px solid alpha(white, 0.06); }
.brand-mark { color: #73b7ff; }
.brand-title { color: #f4f7fb; font-size: 16px; font-weight: 750; }
.brand-subtitle { color: alpha(white, 0.52); font-size: 11px; }
.sidebar { background: #12161c; }
.navigation-caption {
  color: alpha(white, 0.48);
  font-size: 11px;
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
.navigation-row:focus-visible { outline: 2px solid alpha(#4da3ff, 0.7); outline-offset: -2px; }
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
.status-chip-success { background: alpha(#39c58a, 0.14); color: #7de0b2; }
.status-chip-info { background: alpha(#4da3ff, 0.12); color: #b9dcff; }
.status-chip-warning { background: alpha(#e9a23b, 0.16); color: #f4c775; }
.status-chip-error { background: alpha(#ef5b68, 0.14); color: #ff9ba4; }
.setup-status-value {
  padding: 4px 9px;
  border-radius: 999px;
  background: alpha(#4da3ff, 0.10);
  color: #b9dcff;
  font-size: 11px;
  font-weight: 650;
}
.setup-status-value.status-ok { background: alpha(#39c58a, 0.14); color: #7de0b2; }
.setup-status-value.status-info { background: alpha(#4da3ff, 0.12); color: #b9dcff; }
.setup-status-value.status-warning { background: alpha(#e9a23b, 0.16); color: #f4c775; }
.setup-status-value.status-error { background: alpha(#ef5b68, 0.14); color: #ff9ba4; }
.page-header { margin: 2px 0 4px 0; }
.page-title { font-size: 27px; font-weight: 750; color: #f4f7fb; }
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
  padding: 16px;
  border-radius: 12px;
  background: #1a1f27;
  border: 1px solid alpha(white, 0.07);
  box-shadow: 0 1px 3px alpha(black, 0.18);
}
.metric-card-primary {
  min-width: 260px;
  min-height: 148px;
  padding: 18px;
  background-image: linear-gradient(145deg, alpha(#4da3ff, 0.08), alpha(#1a1f27, 0.0) 62%);
}
.metric-card-compact { padding: 12px 14px; }
.dashboard-secondary-card { min-width: 176px; }
.metric-card-icon,
.dashboard-panel-icon,
.dashboard-fan-icon { color: #73b7ff; }
.metric-card-title { color: alpha(white, 0.58); font-size: 12px; font-weight: 650; }
.metric-value { color: #f6f8fb; font-size: 30px; font-weight: 750; }
.metric-unit { color: alpha(white, 0.50); font-size: 16px; font-weight: 600; padding-bottom: 3px; }
.metric-subtitle { color: alpha(white, 0.56); font-size: 12px; }
.metric-sparkline { background: transparent; }
.dashboard-card-button {
  padding: 0;
  border: 0;
  border-radius: 12px;
  background: transparent;
  box-shadow: none;
}
.dashboard-card-button:hover .metric-card {
  border-color: alpha(#4da3ff, 0.38);
  background-color: #1d2430;
}
.dashboard-card-button:focus-visible {
  outline: 2px solid alpha(#4da3ff, 0.82);
  outline-offset: 2px;
}
.dashboard-card-button:active .metric-card { background-color: #202936; }
.dashboard-system-details {
  color: alpha(white, 0.70);
  font-size: 12px;
  padding-top: 2px;
}
.dashboard-panel { min-width: 300px; }
.dashboard-quick-panel { min-width: 540px; }
.dashboard-cooling-panel { min-width: 320px; }
.dashboard-panel-title { color: #f4f7fb; font-size: 15px; font-weight: 700; }
.dashboard-mode-strip { padding: 12px 14px; }
.dashboard-mode-item {
  min-width: 118px;
  padding: 7px 10px;
  border-radius: 8px;
  background: alpha(black, 0.12);
}
.dashboard-mode-title {
  color: alpha(white, 0.48);
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 0.4px;
}
.dashboard-mode-value { color: #f4f7fb; font-size: 13px; font-weight: 650; }
.dashboard-dependency-hint {
  padding: 9px 11px;
  border-radius: 9px;
  background: alpha(#e9a23b, 0.10);
  color: #f4c775;
}
.dashboard-control-tile {
  min-width: 238px;
  padding: 10px 12px;
  border-radius: 9px;
  background: alpha(black, 0.12);
  border: 1px solid alpha(white, 0.05);
}
.dashboard-control-title { color: #f4f7fb; font-size: 13px; font-weight: 650; }
.dashboard-control-subtitle { color: alpha(white, 0.50); font-size: 10px; }
.dashboard-thermal-strip {
  padding: 8px;
  border-radius: 9px;
  background: alpha(black, 0.14);
}
.dashboard-thermal-value { color: alpha(white, 0.76); font-size: 12px; font-weight: 650; }
.dashboard-fan-row,
.dashboard-status-row { padding: 6px 2px; }
.dashboard-fan-row { border-bottom: 1px solid alpha(white, 0.05); }
.dashboard-fan-dot { font-size: 10px; }
.dashboard-status-name { color: alpha(white, 0.62); }
.dashboard-status-indicator { font-size: 8px; }
.dashboard-status-value { font-weight: 650; }
.dashboard-status-value.status-ok,
.dashboard-status-indicator.status-ok,
.dashboard-fan-dot.status-ok { color: #7de0b2; }
.dashboard-status-value.status-info,
.dashboard-status-indicator.status-info,
.dashboard-fan-dot.status-info { color: #b9dcff; }
.dashboard-status-value.status-warning,
.dashboard-status-indicator.status-warning { color: #f4c775; }
.dashboard-status-value.status-error,
.dashboard-status-indicator.status-error { color: #ff9ba4; }
.dashboard-trend-card {
  min-width: 150px;
  padding: 10px 12px;
  box-shadow: none;
  background: alpha(black, 0.10);
}
.dashboard-trend-card .metric-value { font-size: 22px; }
.history-graph {
  border-radius: 10px;
  background: alpha(black, 0.14);
  border: 1px solid alpha(white, 0.055);
}
.capability-panel {
  padding: 10px 12px;
  border-radius: 10px;
  background: alpha(#4da3ff, 0.07);
  border: 1px solid alpha(#4da3ff, 0.13);
}
.compact-banner { margin: 0; }
.dashboard-warning.warning-info,
.compact-banner,
.fans-warning-banner { background: alpha(#4da3ff, 0.16); }
.dashboard-warning.warning-warning { background: alpha(#e9a23b, 0.18); }
.dashboard-warning.warning-error { background: alpha(#ef5b68, 0.18); }
.about-identity { padding: 22px; }
.about-icon { color: #73b7ff; }
.process-header { color: alpha(white, 0.48); font-size: 11px; font-weight: 700; }
.process-row { padding: 8px 12px; border-bottom: 1px solid alpha(white, 0.05); }
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
