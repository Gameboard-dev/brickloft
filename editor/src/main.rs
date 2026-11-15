//! The editor as a desktop window, for development.
//! `cargo run -p arcs-editor -- 5` opens on the sixth form in the menu.

fn main() {
    let form = std::env::args().nth(1).and_then(|a| a.parse().ok()).unwrap_or(0);
    arcs_editor::run_form(form);
}
