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

/// The folder screenshots go in: `omegga-arcs` inside the user's pictures
/// folder, made if it is not there.
///
/// The pictures folder is taken to be `Pictures` in the home folder, which
/// is where Windows, macOS and most Linux desktops keep it; on Linux
/// `XDG_PICTURES_DIR` is used when it is set. A user who has moved the
/// folder elsewhere gets a `Pictures` folder in the old place. Asking each
/// system where the folder really is takes a system interface per platform
/// (or the `dirs` crate, which wraps them), for a folder the message names
/// anyway.
#[cfg(not(target_arch = "wasm32"))]
fn screenshot_folder() -> Result<std::path::PathBuf, String> {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).ok_or("the home folder is not known")?;
    let pictures = std::env::var_os("XDG_PICTURES_DIR").filter(|_| cfg!(target_os = "linux")).map(std::path::PathBuf::from).unwrap_or_else(|| std::path::Path::new(&home).join("Pictures"));
    let folder = pictures.join("omegga-arcs");
    std::fs::create_dir_all(&folder).map_err(|e| format!("could not make {}: {e}", folder.display()))?;
    Ok(folder)
}

/// Write a picture of the window, given as rows of pixels from the top,
/// to a numbered PNG file in the screenshot folder, and put the picture on
/// the clipboard as well. Returns what to tell the player.
#[cfg(not(target_arch = "wasm32"))]
pub fn save_screenshot(width: u32, height: u32, pixels: &[[u8; 4]]) -> String {
    if pixels.len() != width as usize * height as usize || pixels.is_empty() {
        return "No screenshot: the picture read back is not the size of the window".into();
    }
    let written = (|| -> Result<std::path::PathBuf, String> {
        let folder = screenshot_folder()?;
        // arcs-1.png, -2, ...: the first number not yet used
        let path = (1..100_000).map(|n| folder.join(format!("arcs-{n}.png"))).find(|p| !p.exists()).ok_or("the folder is full")?;
        // the window's own see-through value is left out: a picture of the
        // window is solid whatever was drawn into that channel
        let bytes: Vec<u8> = pixels.iter().flat_map(|p| [p[0], p[1], p[2]]).collect();
        let file = std::io::BufWriter::new(std::fs::File::create(&path).map_err(|e| format!("{}: {e}", path.display()))?);
        let mut encoder = png::Encoder::new(file, width, height);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
        writer.write_image_data(&bytes).map_err(|e| e.to_string())?;
        Ok(path)
    })();
    let copied = picture_on_clipboard(width, height, pixels);
    match (written, copied) {
        (Ok(path), Ok(())) => format!("Screenshot copied, and saved to {}", path.display()),
        (Ok(path), Err(why)) => format!("Screenshot saved to {} (not copied: {why})", path.display()),
        (Err(e), Ok(())) => format!("Screenshot copied, but not saved: {e}"),
        (Err(e), Err(_)) => format!("Could not save the screenshot: {e}"),
    }
}

/// The picture as the bytes of a .bmp file, which is what the Windows
/// clipboard takes: a 14-byte file header, a 40-byte picture header, then
/// the rows from the bottom up, blue first, each padded to four bytes.
/// https://learn.microsoft.com/en-us/windows/win32/gdi/bitmap-storage
#[cfg(not(target_arch = "wasm32"))]
fn bmp(width: u32, height: u32, pixels: &[[u8; 4]]) -> Vec<u8> {
    let row = (width as usize * 3).div_ceil(4) * 4;
    let size = 54 + row * height as usize;
    let mut out = Vec::with_capacity(size);
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&(size as u32).to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&54u32.to_le_bytes()); // where the rows start
    out.extend_from_slice(&40u32.to_le_bytes()); // the picture header's own size
    out.extend_from_slice(&(width as i32).to_le_bytes());
    out.extend_from_slice(&(height as i32).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // one plane
    out.extend_from_slice(&24u16.to_le_bytes()); // bits a pixel
    out.extend_from_slice(&[0; 4]); // not compressed
    out.extend_from_slice(&((row * height as usize) as u32).to_le_bytes());
    out.extend_from_slice(&[0; 16]); // no print size, no palette
    for y in (0..height as usize).rev() {
        let line = &pixels[y * width as usize..(y + 1) * width as usize];
        out.extend(line.iter().flat_map(|p| [p[2], p[1], p[0]]));
        out.resize(out.len() + row - width as usize * 3, 0);
    }
    out
}

/// Put the picture on the clipboard, to paste into a document or a message.
#[cfg(windows)]
fn picture_on_clipboard(width: u32, height: u32, pixels: &[[u8; 4]]) -> Result<(), String> {
    use clipboard_win::{options::DoClear, raw, Clipboard};
    // opening can fail while another program holds the clipboard: try a few times
    let _open = Clipboard::new_attempts(10).map_err(|e| e.to_string())?;
    // Emptied first. The plain call adds the picture beside whatever is
    // there, so a form copied earlier would stay on the clipboard with it,
    // and which of the two a paste took would be up to the program pasting.
    raw::set_bitmap_with(&bmp(width, height, pixels), DoClear).map_err(|e| e.to_string())
}

/// Only Windows has a clipboard this program can reach without another
/// dependency (see `brz::copy`, which is in the same position).
#[cfg(all(not(windows), not(target_arch = "wasm32")))]
fn picture_on_clipboard(_: u32, _: u32, _: &[[u8; 4]]) -> Result<(), String> {
    Err("the clipboard is only reached on Windows".into())
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    #[test]
    fn a_bmp_holds_its_rows_from_the_bottom_blue_first_and_padded() {
        // two pixels across, two down: red, green over blue, white
        let pixels = [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255], [255, 255, 255, 255]];
        let file = super::bmp(2, 2, &pixels);
        // each row is 6 bytes of colour padded to 8
        assert_eq!(file.len(), 54 + 16);
        assert_eq!(&file[0..2], b"BM");
        assert_eq!(u32::from_le_bytes(file[2..6].try_into().unwrap()), 70);
        assert_eq!(&file[54..62], &[255, 0, 0, 255, 255, 255, 0, 0], "the bottom row: blue, white");
        assert_eq!(&file[62..70], &[0, 0, 255, 0, 255, 0, 0, 0], "the top row: red, green");
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
