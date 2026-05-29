//! GTK4 + libadwaita UI shell for Darkroom.
//!
//! The legacy C/GTK3 UI lives under src/views, src/libs, src/gui. This crate
//! is the eventual replacement. The current state is a minimal entry point
//! (an empty main window) so the rest of the toolchain — workspace build,
//! container packaging, integration with darkroom-core — can be validated
//! end-to-end. Real views (lighttable, darkroom, slideshow, …) are added
//! incrementally as their dependencies in darkroom-core stabilise.

use adw::prelude::*;
use adw::Application;
use anyhow::Result;
use gtk4::ApplicationWindow;

/// Application identifier used by GIO for desktop integration (single-instance
/// lookup, file associations, settings backend, …). Keep stable across builds
/// — changing it orphans saved window state and per-app GSettings.
pub const APP_ID: &str = "org.darkroom.Darkroom";

/// Default window dimensions. The C UI persists last-known dimensions in
/// `ui_last/gui_w` / `ui_last/gui_h`; we'll wire that through once the
/// darkroom-core conf access lands in Rust.
pub const DEFAULT_WIDTH:  i32 = 1280;
pub const DEFAULT_HEIGHT: i32 = 800;

/// Boot the GTK4 application. Blocks until the main window is closed.
///
/// Returns the libadwaita / GLib exit code so the binary can propagate it as
/// its process exit status (matches the C `main()`'s gtk_main() contract).
pub fn run() -> Result<glib::ExitCode> {
    // libadwaita's Application is a thin wrapper around gtk4::Application that
    // also initialises the Adwaita stylesheet provider — needed for the
    // colour scheme tracking and switch widgets we rely on.
    let app = Application::builder()
        .application_id(APP_ID)
        .build();

    app.connect_activate(build_main_window);

    Ok(app.run())
}

fn build_main_window(app: &Application) {
    let window = ApplicationWindow::builder()
        .application(app)
        .title("Darkroom")
        .default_width(DEFAULT_WIDTH)
        .default_height(DEFAULT_HEIGHT)
        .build();

    // Placeholder: a centred label until the lighttable view is ported.
    // The real darkroom UI swaps this out for an AdwViewStack rooted at the
    // current dt_view_t (lighttable / darkroom / slideshow / …).
    let label = gtk4::Label::builder()
        .label("Darkroom — Rust + GTK4 shell (work in progress)")
        .halign(gtk4::Align::Center)
        .valign(gtk4::Align::Center)
        .build();
    window.set_child(Some(&label));

    window.present();
}
