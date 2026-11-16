//! The editor as a desktop window.
//! `arcs 5` opens on the sixth form in the menu, and `arcs --claude` opens
//! already listening for Claude (see `arcs_editor::mcp`), for a script that
//! starts the editor and Claude together. Without it the listener is off
//! until the box in the panel is ticked.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let form = args.iter().find_map(|a| a.parse().ok()).unwrap_or(0);
    arcs_editor::run_form(form, args.iter().any(|a| a == "--claude"));
}
