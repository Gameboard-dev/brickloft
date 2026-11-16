//! Saving a report on a fit, or a picture of the window, to be read or
//! passed on.

/// Write a picture of the window, given as rows of pixels from the top,
/// to a numbered PNG file in the screenshot folder, and put the picture on
/// the clipboard as well. Returns what to tell the player.
#[cfg(not(target_arch = "wasm32"))]
pub fn save_screenshot(width: u32, height: u32, pixels: &[[u8; 4]]) -> String {
    if pixels.len() != width as usize * height as usize || pixels.is_empty() {
        return "No screenshot: the picture read back is not the size of the window".into();
    }
    let written = (|| -> Result<std::path::PathBuf, String> {
        let folder = crate::places::screenshots()?;
        // brickloft-1.png, -2, ...: the first number not yet used
        let path = (1..100_000).map(|n| folder.join(format!("brickloft-{n}.png"))).find(|p| !p.exists()).ok_or("the folder is full")?;
        std::fs::write(&path, png_of(width, height, pixels)?).map_err(|e| format!("{}: {e}", path.display()))?;
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

/// A picture, given as rows of pixels from the top, as the bytes of a PNG
/// file.
#[cfg(not(target_arch = "wasm32"))]
pub fn png_of(width: u32, height: u32, pixels: &[[u8; 4]]) -> Result<Vec<u8>, String> {
    if pixels.len() != width as usize * height as usize || pixels.is_empty() {
        return Err("the picture is not the size it is said to be".into());
    }
    // the window's own see-through value is left out: a picture of the
    // window is solid whatever was drawn into that channel
    let bytes: Vec<u8> = pixels.iter().flat_map(|p| [p[0], p[1], p[2]]).collect();
    let mut file = Vec::new();
    let mut encoder = png::Encoder::new(&mut file, width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(&bytes).map_err(|e| e.to_string())?;
    drop(writer);
    Ok(file)
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
    fn only_files_named_as_reports_are_counted_as_reports() {
        assert_eq!(super::report_number("brickloft-report-34.txt"), Some(34));
        for other in ["brickloft-report-.txt", "brickloft-report-3.txt.bak", "brickloft-report-x.txt", "brickloft-report--3.txt", "brickloft-build.brz", "notes.txt", "brickloft-report-1234567.txt", "../brickloft-report-1.txt"] {
            assert_eq!(super::report_number(other), None, "{other}");
        }
    }

    #[test]
    fn clearing_takes_the_reports_out_of_a_folder_and_leaves_the_rest() {
        let folder = std::env::temp_dir().join(format!("brickloft-test-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        for name in ["brickloft-report-1.txt", "brickloft-report-7.txt", "brickloft-build.brz", "notes.txt"] {
            std::fs::write(folder.join(name), "x").unwrap();
        }
        let mut found = super::reports_in(&folder);
        found.sort();
        assert_eq!(found, [1, 7]);
        std::fs::remove_dir_all(&folder).unwrap();
    }

    /// Run by hand (`cargo test -p brickloft-editor clipboard -- --ignored`):
    /// it takes over the clipboard of whoever runs it.
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn a_picture_put_on_the_clipboard_is_there_as_a_picture() {
        // (a window's size, and a width that is no multiple of four, so that the rows are padded)
        let (wide, tall) = (1917u32, 1003u32);
        let pixels: Vec<[u8; 4]> = (0..wide * tall).map(|k| [(k % wide / 8) as u8, (k / wide / 4) as u8, 90, 255]).collect();
        super::picture_on_clipboard(wide, tall, &pixels).expect("the picture was not copied");
        let _open = clipboard_win::Clipboard::new_attempts(10).unwrap();
        // (2 is a bitmap, 8 a bitmap with its header, 17 the same with the later header: the clipboard makes each from the others)
        let there: Vec<u32> = [2u32, 8, 17].into_iter().filter(|format| clipboard_win::is_format_avail(*format)).collect();
        assert!(!there.is_empty(), "no picture is on the clipboard");
        println!("picture formats on the clipboard: {there:?}");
    }

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

/// The number in a report's file name, `brickloft-report-<number>.txt`, if
/// `name` is one.
#[cfg(not(target_arch = "wasm32"))]
fn report_number(name: &str) -> Option<u32> {
    let digits = name.strip_prefix("brickloft-report-")?.strip_suffix(".txt")?;
    (!digits.is_empty() && digits.len() <= 6 && digits.bytes().all(|b| b.is_ascii_digit())).then(|| digits.parse().ok())?
}

/// The numbers of the reports in `folder`.
#[cfg(not(target_arch = "wasm32"))]
fn reports_in(folder: &std::path::Path) -> Vec<u32> {
    let Ok(entries) = std::fs::read_dir(folder) else { return Vec::new() };
    entries.flatten().filter(|e| e.path().is_file()).filter_map(|e| report_number(e.file_name().to_str()?)).collect()
}

/// Put a report where it can be read: on the desktop a file in the
/// program's temporary folder (see `places`), numbered one past the last
/// report there so that every save is kept; in the browser the clipboard (a
/// page cannot write files). Returns what to tell the player.
#[cfg(not(target_arch = "wasm32"))]
pub fn save_report(text: &str) -> String {
    let saved = crate::places::passing().and_then(|folder| {
        let number = reports_in(&folder).into_iter().max().unwrap_or(0) + 1;
        let path = folder.join(format!("brickloft-report-{number}.txt"));
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(path)
    });
    match saved {
        Ok(path) => format!("Report saved to {}", path.display()),
        Err(e) => format!("Could not save the report: {e}"),
    }
}

/// Delete every report in the program's temporary folder, and nothing else
/// there: only files named as [`save_report`] names them. Returns what to
/// tell the player.
#[cfg(not(target_arch = "wasm32"))]
pub fn clear_reports() -> String {
    let folder = match crate::places::passing() {
        Ok(folder) => folder,
        Err(e) => return format!("No reports cleared: {e}"),
    };
    let numbers = reports_in(&folder);
    let gone = numbers.iter().filter(|n| std::fs::remove_file(folder.join(format!("brickloft-report-{n}.txt"))).is_ok()).count();
    match (gone, numbers.len() - gone) {
        (0, 0) => format!("There are no reports in {}", folder.display()),
        (gone, 0) => format!("Cleared {gone} report{} from {}", if gone == 1 { "" } else { "s" }, folder.display()),
        (gone, left) => format!("Cleared {gone} reports from {}; {left} could not be deleted", folder.display()),
    }
}

/// A page has no report files to clear: its reports go to the clipboard.
#[cfg(target_arch = "wasm32")]
pub fn clear_reports() -> String {
    String::new()
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
