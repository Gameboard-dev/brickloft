//! Saving a report on a fit, or a picture of the window, to be read or
//! passed on.

/// A file beside where the program was started: `stem-1.ext`, -2, ...: the
/// first number not yet used, so that every save is kept. (One fixed name
/// lost every report but the last.)
#[cfg(not(target_arch = "wasm32"))]
fn numbered(stem: &str, ext: &str) -> std::path::PathBuf {
    let folder = std::env::current_dir().unwrap_or_default();
    (1..10_000).map(|n| folder.join(format!("{stem}-{n}.{ext}"))).find(|p| !p.exists()).unwrap_or_else(|| folder.join(format!("{stem}.{ext}")))
}

/// Write a picture of the window, given as rows of pixels from the top,
/// to a numbered PNG file. Returns what to tell the player.
#[cfg(not(target_arch = "wasm32"))]
pub fn save_screenshot(width: u32, height: u32, pixels: &[[u8; 4]]) -> String {
    let path = numbered("arcs-screenshot", "png");
    // the window's own see-through value is left out: a picture of the
    // window is solid whatever was drawn into that channel
    let bytes: Vec<u8> = pixels.iter().flat_map(|p| [p[0], p[1], p[2]]).collect();
    let written = (|| -> Result<(), String> {
        if bytes.len() != width as usize * height as usize * 3 {
            return Err("the picture read back is not the size of the window".into());
        }
        let file = std::io::BufWriter::new(std::fs::File::create(&path).map_err(|e| e.to_string())?);
        let mut encoder = png::Encoder::new(file, width, height);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
        writer.write_image_data(&bytes).map_err(|e| e.to_string())
    })();
    match written {
        Ok(()) => format!("Screenshot saved to {}", path.display()),
        Err(e) => format!("Could not save the screenshot to {}: {e}", path.display()),
    }
}

/// Put a report where it can be read: a file beside where the program was
/// started on the desktop, the clipboard in the browser (a page cannot write
/// files). Returns what to tell the player.
#[cfg(not(target_arch = "wasm32"))]
pub fn save_report(text: &str) -> String {
    let path = numbered("arcs-report", "txt");
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
