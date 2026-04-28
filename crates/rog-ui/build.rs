fn main() {
    glib_build_tools::compile_resources(
        &["resources"],
        "resources/rog-ui.gresource.xml",
        "rog-ui.gresource",
    );
}
