//! Saving a report on a fit, to be read or passed on.

/// Put a report where it can be read: a file beside where the program was
/// started on the desktop, the clipboard in the browser (a page cannot write
/// files). Returns what to tell the player.
#[cfg(not(target_arch = "wasm32"))]
pub fn save_report(text: &str) -> String {
    // arcs-report-1.txt, -2, ...: the first number not yet used, so that every
    // save is kept. (One fixed name lost every report but the last.)
    let folder = std::env::current_dir().unwrap_or_default();
    let path = (1..10_000).map(|n| folder.join(format!("arcs-report-{n}.txt"))).find(|p| !p.exists()).unwrap_or_else(|| folder.join("arcs-report.txt"));
    match std::fs::write(&path, text) {
        Ok(()) => format!("Report saved to {}", path.display()),
        Err(e) => format!("Could not save the report to {}: {e}. Start the editor from a folder you can write to.", path.display()),
    }
}

#[cfg(target_arch = "wasm32")]
pub fn save_report(text: &str) -> String {
    match web_sys::window() {
        // the browser allows this only on a page served from localhost or over https
        Some(window) => {
            let _ = window.navigator().clipboard().write_text(text);
            "Report copied to the clipboard. Paste it into a file or a message.".into()
        }
        None => "Could not reach the clipboard.".into(),
    }
}
