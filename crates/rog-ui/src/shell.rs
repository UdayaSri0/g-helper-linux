use adw::prelude::*;
use gtk4 as gtk;

#[derive(Clone, Copy)]
pub struct NavigationItem {
    pub name: &'static str,
    pub label: &'static str,
    pub icon: &'static str,
}

pub fn build_shell(
    stack: &adw::ViewStack,
    status_label: &gtk::Label,
    items: &[NavigationItem],
) -> adw::ToolbarView {
    let brand = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let title = gtk::Label::new(Some("ROG Helper"));
    title.set_xalign(0.0);
    title.add_css_class("heading");
    let subtitle = gtk::Label::new(Some("Hardware Control Centre"));
    subtitle.set_xalign(0.0);
    subtitle.add_css_class("caption");
    subtitle.add_css_class("dim-label");
    brand.append(&title);
    brand.append(&subtitle);

    status_label.add_css_class("connection-status");
    status_label.set_text("Connecting…");

    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&brand));
    header.pack_end(status_label);

    let navigation = gtk::ListBox::new();
    navigation.add_css_class("navigation-list");
    navigation.set_selection_mode(gtk::SelectionMode::Single);
    navigation.set_activate_on_single_click(true);

    for item in items {
        let row = gtk::ListBoxRow::new();
        row.add_css_class("navigation-row");
        row.set_tooltip_text(Some(item.label));

        let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        content.set_margin_top(10);
        content.set_margin_bottom(10);
        content.set_margin_start(12);
        content.set_margin_end(12);

        let icon = gtk::Image::from_icon_name(item.icon);
        icon.set_pixel_size(18);
        icon.set_accessible_role(gtk::AccessibleRole::Img);
        let label = gtk::Label::new(Some(item.label));
        label.set_xalign(0.0);
        label.set_hexpand(true);

        content.append(&icon);
        content.append(&label);
        row.set_child(Some(&content));
        navigation.append(&row);
    }

    let names = items.iter().map(|item| item.name).collect::<Vec<_>>();
    let names_for_stack = names.clone();
    let stack_for_selection = stack.clone();
    navigation.connect_row_selected(move |_, row| {
        let Some(row) = row else {
            return;
        };
        if let Some(name) = names.get(row.index() as usize) {
            stack_for_selection.set_visible_child_name(name);
        }
    });
    if let Some(first) = navigation.row_at_index(0) {
        navigation.select_row(Some(&first));
    }
    let navigation_for_stack = navigation.clone();
    stack.connect_visible_child_name_notify(move |stack| {
        let Some(visible) = stack.visible_child_name() else {
            return;
        };
        if let Some(index) = names_for_stack
            .iter()
            .position(|name| *name == visible.as_str())
        {
            if let Some(row) = navigation_for_stack.row_at_index(index as i32) {
                navigation_for_stack.select_row(Some(&row));
            }
        }
    });

    let nav_header = gtk::Label::new(Some("CONTROL CENTRE"));
    nav_header.set_xalign(0.0);
    nav_header.add_css_class("navigation-caption");

    let sidebar = gtk::Box::new(gtk::Orientation::Vertical, 12);
    sidebar.add_css_class("sidebar");
    sidebar.set_size_request(210, -1);
    sidebar.set_margin_top(16);
    sidebar.set_margin_bottom(16);
    sidebar.set_margin_start(12);
    sidebar.set_margin_end(12);
    sidebar.append(&nav_header);
    sidebar.append(&navigation);

    let separator = gtk::Separator::new(gtk::Orientation::Vertical);
    let body = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    body.set_vexpand(true);
    body.append(&sidebar);
    body.append(&separator);
    body.append(stack);

    let view = adw::ToolbarView::new();
    view.add_top_bar(&header);
    view.set_content(Some(&body));
    view
}
