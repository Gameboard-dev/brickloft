//! The editor as a desktop window.
//! `brickloft 5` opens on the sixth form in the menu, and `brickloft --claude` opens
//! already listening for Claude (see `brickloft_editor::mcp`), for a script that
//! starts the editor and Claude together. Without it the listener is off
//! until the box in the panel is ticked.

fn main() {
    // (first of all, so that a crash while starting is on record too)
    brickloft_editor::crash::log_to_file();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let form = args.iter().find_map(|a| a.parse().ok()).unwrap_or(0);
    brickloft_editor::run_form(form, args.iter().any(|a| a == "--claude"));
}
