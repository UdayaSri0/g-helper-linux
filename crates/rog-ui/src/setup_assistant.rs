use std::cell::Cell;
use std::rc::Rc;

use adw::prelude::*;
use gtk4 as gtk;
use rog_core::{PrivilegedStatus, SetupStatus};

const STAGES: [(&str, &str); 8] = [
    ("Welcome", "Review setup at your own pace. Checks inspect services and capabilities. System package changes require a separate native administrator review."),
    ("Core components", "The native ROG Helper package owns the desktop application, session daemon, privileged helper, D-Bus, PolicyKit and udev integration."),
    ("Optional services", "Optional providers extend supported features. Choose an item to review its guidance. Existing compatible providers should be preserved."),
    ("Hardware compatibility", "Available controls depend on the exact laptop, kernel and provider APIs. A successful installation does not establish physical hardware support."),
    ("Review plan", "Review the diagnostics below before making changes. Package-manager repair restores package-owned files. Inspect the proposed packages and source in the native installer."),
    ("Progress", "Perform any approved native package actions separately, then refresh checks here. This assistant records the results of live probes."),
    ("Verification", "Review current service health and remaining issues. Unsupported hardware can remain unavailable after the installation is healthy."),
    ("Finish", "Save that this setup review has been completed. You can reopen the assistant from Setup & Access whenever you need it."),
];

pub struct SetupAssistant {
    pub widget: gtk::Box,
    details: gtk::Label,
    verification: gtk::Label,
    report: gtk::TextBuffer,
    refresh: gtk::Button,
    recheck: gtk::Button,
    spinner: gtk::Spinner,
}

impl SetupAssistant {
    pub fn new(
        reviewed: bool,
        on_refresh: impl Fn() + 'static,
        on_complete: impl Fn() + 'static,
    ) -> Self {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 12);
        widget.add_css_class("card");
        let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
        content.set_margin_top(18);
        content.set_margin_bottom(18);
        content.set_margin_start(18);
        content.set_margin_end(18);
        content.set_visible(!reviewed);
        widget.append(&content);
        let reopen = gtk::Button::with_label("Reopen setup assistant");
        reopen.set_halign(gtk::Align::Start);
        reopen.set_visible(reviewed);
        widget.prepend(&reopen);
        {
            let content = content.clone();
            reopen.connect_clicked(move |button| {
                content.set_visible(true);
                button.set_visible(false);
            });
        }

        let heading = label("Guided Setup Assistant");
        heading.add_css_class("title-2");
        content.append(&heading);
        let counter = label("Step 1 of 8 · Welcome");
        counter.add_css_class("dim-label");
        content.append(&counter);
        let pages = gtk::Stack::new();
        pages.set_transition_type(gtk::StackTransitionType::SlideLeftRight);
        let mut optional_page = None;
        for (index, (title, description)) in STAGES.iter().enumerate() {
            let page = gtk::Box::new(gtk::Orientation::Vertical, 10);
            let title_label = label(title);
            title_label.add_css_class("heading");
            page.append(&title_label);
            page.append(&label(description));
            pages.add_named(&page, Some(&index.to_string()));
            if index == 2 {
                optional_page = Some(page);
            }
        }
        if let Some(page) = optional_page {
            for (name, description) in [
                ("asusd", "Profiles and model-dependent ASUS controls require a compatible API. Use trusted distribution packaging where available."),
                ("supergfxd", "GPU switching requires separate provider review. Upstream is phasing this service out; installation is never automatic."),
                ("UPower", "Battery and power telemetry can use the distribution's upower package."),
                ("NVIDIA telemetry", "nvidia-smi comes with a compatible NVIDIA driver. Driver changes require the distribution's dedicated driver tools."),
            ] {
                let expander = gtk::Expander::new(Some(name));
                expander.set_child(Some(&label(description)));
                page.append(&expander);
            }
        }
        content.append(&pages);

        let details = label("Waiting for live setup checks…");
        details.set_selectable(true);
        content.append(&details);
        let verification = label("Refresh checks before assessing setup readiness.");
        verification.set_selectable(true);
        content.append(&verification);
        let report = gtk::TextBuffer::new(None);
        report.set_text("Diagnostics will appear when setup checks finish.");
        let view = gtk::TextView::with_buffer(&report);
        view.set_editable(false);
        view.set_cursor_visible(false);
        view.set_wrap_mode(gtk::WrapMode::WordChar);
        view.add_css_class("monospace");
        let scroll = gtk::ScrolledWindow::new();
        scroll.set_min_content_height(220);
        scroll.set_child(Some(&view));
        let logs = gtk::Expander::new(Some("Setup plan and read-only check log"));
        content.append(&label("Package installation is manual. No package installation log exists in this assistant. The log below contains read-only checks and guidance."));
        logs.set_child(Some(&scroll));
        content.append(&logs);

        let on_refresh = Rc::new(on_refresh);
        let refresh = gtk::Button::with_label("Check System");
        {
            let on_refresh = on_refresh.clone();
            refresh.connect_clicked(move |_| on_refresh());
        }
        let recheck = gtk::Button::with_label("Recheck Services");
        recheck.connect_clicked(move |_| on_refresh());
        let plan = gtk::Button::with_label("View Setup Plan");
        {
            let logs = logs.clone();
            plan.connect_clicked(move |_| logs.set_expanded(true));
        }
        let install = gtk::Button::with_label("Install Supported Components");
        install.set_sensitive(false);
        install.set_tooltip_text(Some("Automatic package changes are unavailable. Review the plan and use your trusted native package manager."));
        let repair = gtk::Button::with_label("Repair ROG Helper Integration · Manual review");
        {
            let logs = logs.clone();
            repair.connect_clicked(move |_| logs.set_expanded(true));
        }
        let installation_log = gtk::Button::with_label("View Installation Log");
        {
            let logs = logs.clone();
            installation_log.connect_clicked(move |_| logs.set_expanded(true));
        }
        let copy = gtk::Button::with_label("Copy diagnostics");
        {
            let report = report.clone();
            copy.connect_clicked(move |_| {
                if let Some(display) = gtk::gdk::Display::default() {
                    display.clipboard().set_text(&report.text(
                        &report.start_iter(),
                        &report.end_iter(),
                        false,
                    ));
                }
            });
        }
        let spinner = gtk::Spinner::new();
        let actions = gtk::FlowBox::new();
        actions.set_selection_mode(gtk::SelectionMode::None);
        actions.set_min_children_per_line(1);
        actions.set_max_children_per_line(2);
        actions.set_column_spacing(8);
        actions.set_row_spacing(8);
        for button in [
            &refresh,
            &plan,
            &install,
            &repair,
            &recheck,
            &installation_log,
            &copy,
        ] {
            actions.insert(button, -1);
        }
        actions.insert(&spinner, -1);
        content.append(&actions);
        let back = gtk::Button::with_label("Back");
        back.set_sensitive(false);
        let next = gtk::Button::with_label("Next");
        next.add_css_class("suggested-action");
        let on_complete = Rc::new(on_complete);
        let complete = gtk::Button::with_label("Finish setup review");
        complete.set_visible(false);
        {
            let on_complete = on_complete.clone();
            let content = content.clone();
            let reopen = reopen.clone();
            complete.connect_clicked(move |_| {
                on_complete();
                content.set_visible(false);
                reopen.set_visible(true);
            });
        }
        let skip = gtk::Button::with_label("Skip for now");
        {
            let content = content.clone();
            let reopen = reopen.clone();
            skip.connect_clicked(move |_| {
                on_complete();
                content.set_visible(false);
                reopen.set_visible(true);
            });
        }
        let navigation = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        navigation.append(&back);
        navigation.append(&next);
        navigation.append(&complete);
        navigation.append(&skip);
        content.append(&navigation);
        let stage_actions = vec![
            (plan, 3..=4),
            (install, 4..=4),
            (repair, 4..=4),
            (recheck.clone(), 5..=6),
            (installation_log, 5..=7),
        ];
        for (button, _) in &stage_actions {
            button.set_visible(false);
        }
        let current = Rc::new(Cell::new(0));
        for (button, forward) in [(&back, false), (&next, true)] {
            let current = current.clone();
            let pages = pages.clone();
            let counter = counter.clone();
            let back = back.clone();
            let next = next.clone();
            let complete = complete.clone();
            let stage_actions = stage_actions.clone();
            button.connect_clicked(move |_| {
                let stage = next_stage(current.get(), forward);
                current.set(stage);
                pages.set_visible_child_name(&stage.to_string());
                counter.set_text(&format!("Step {} of 8 · {}", stage + 1, STAGES[stage].0));
                back.set_sensitive(stage > 0);
                next.set_visible(stage < STAGES.len() - 1);
                complete.set_visible(stage == STAGES.len() - 1);
                for (button, stages) in &stage_actions {
                    button.set_visible(stages.contains(&stage));
                }
            });
        }
        Self {
            widget,
            details,
            verification,
            report,
            refresh,
            recheck,
            spinner,
        }
    }

    pub fn update(
        &self,
        status: &SetupStatus,
        privileged: Option<&PrivilegedStatus>,
        report: &str,
        refreshing: bool,
    ) {
        self.refresh.set_sensitive(!refreshing);
        self.recheck.set_sensitive(!refreshing);
        self.spinner.set_spinning(refreshing);
        self.spinner.set_visible(refreshing);
        self.refresh.set_label(if refreshing {
            "Checking…"
        } else {
            "Check System"
        });
        let helper = privileged.map_or("Waiting for helper diagnostics", |helper| {
            if helper.privileged_helper_reachable && helper.privileged_helper_compatible {
                "Compatible privileged helper responds"
            } else if helper.privileged_helper_installed {
                "Privileged helper installation needs review"
            } else {
                "Privileged helper is missing"
            }
        });
        self.details.set_text(helper);
        self.verification
            .set_text(&verification_text(status, refreshing));
        self.report.set_text(if report.is_empty() {
            "Setup plan is unavailable. Refresh checks or use the diagnostics below."
        } else {
            report
        });
    }
}

fn label(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.set_xalign(0.0);
    label.set_wrap(true);
    label
}

fn next_stage(current: usize, forward: bool) -> usize {
    if forward {
        current.saturating_add(1).min(STAGES.len() - 1)
    } else {
        current.saturating_sub(1)
    }
}

fn verification_text(status: &SetupStatus, refreshing: bool) -> String {
    if refreshing {
        "Live verification is running…".to_string()
    } else if status.checked_at_ms == 0 {
        format!("Setup has not been verified in this session. {} item(s) need review. Refresh checks to inspect live services.", status.issue_count())
    } else {
        format!("Live check completed. {} setup item(s) need review. Read the diagnostics before changing packages or using controls.", status.issue_count())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_navigation_stays_within_the_assistant() {
        assert_eq!(next_stage(0, false), 0);
        assert_eq!(next_stage(7, true), 7);
        assert_eq!(next_stage(4, true), 5);
        assert_eq!(next_stage(4, false), 3);
    }

    #[test]
    fn unknown_checks_never_claim_verification_success() {
        let status = SetupStatus::unknown();
        assert!(verification_text(&status, false).contains("has not been verified"));
        assert!(verification_text(&status, true).contains("running"));
        assert!(
            verification_text(&SetupStatus::daemon_unavailable("offline"), false)
                .contains("review")
        );
    }
}
