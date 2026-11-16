//! Saved forms, kept in named folders.
//!
//! A save is the whole workspace: every form, with its colours, where it
//! stands and the grid it is on.
//!
//! On the desktop a save is a Brickadia prefab, `<folder>/<name>.brz` under
//! `Documents/brickloft/forms` (see `places`): the same file Ctrl+C makes (see
//! `brz`), holding the bricks and, in its description, the forms they were
//! fitted to. That format was chosen over a text format of the editor's own
//! (YAML, TOML, JSON) for three reasons. The editor already writes and
//! reads it, for copy and paste, so saving adds no second format to keep in
//! step and no parser to depend on. Reading it back goes through the checks
//! a paste goes through. And a saved file is useful without the editor: the
//! game loads it as a prefab. What it gives up is being edited by hand,
//! which the forms' text inside it would allow and nothing has needed.
//!
//! A web page cannot write files, and the browser build has no .brz writer,
//! so in the browser the same folders live in the page's local storage and
//! each save is the forms' text alone:
//! https://developer.mozilla.org/en-US/docs/Web/API/Window/localStorage
//!
//! Before saves were prefabs they were text files of one form's settings,
//! `<name>.txt`. Those are still listed and opened.
//!
//! Folder and form names come from a text box, and on the desktop they
//! become part of a file path, so only [`clean`] names are ever used.

use crate::paint::{Mode, Paint};

/// The way of colouring each number in a saved form stands for. A save
/// holds the number, so the list keeps its length and order for good. 3
/// was a speckled blend and 4 each piece its own colour, which the
/// editor no longer has: a save that used them opens as the nearest
/// thing, a blend and the pattern's own colours.
const SAVED_MODES: [Mode; 6] = [Mode::Solid, Mode::Blend, Mode::Stripes, Mode::Blend, Mode::Pattern, Mode::Pattern];
use crate::state::{Painted, Placed};
use brickloft_core::report::{parse_spec, spec_text};

/// The line that begins each form in a file's description. (It keeps the
/// program's first name, as do the names a browser keeps saves under below:
/// they are part of what a save is, and changing them would orphan every
/// save made before.)
const FORM: &str = "[arcs form]";


/// The workspace's forms as text, kept in a .brz file's description.
///
/// A .brz holds bricks, and bricks cannot be turned back into the form they
/// were fitted to. With the forms written beside them, a file copied from
/// the editor pastes back as forms that can still be reshaped, not as a
/// fixed lump; the game ignores the description.
pub fn describe(forms: &[Placed]) -> String {
    let mut text = String::new();
    for (spec, paint, place) in forms {
        let mode = SAVED_MODES.iter().position(|m| *m == paint.mode).unwrap_or(0);
        let colours: Vec<String> = paint.colours.iter().map(|c| format!("{},{},{}", c[0], c[1], c[2])).collect();
        text += &format!("{FORM}\nplace {} {} {}\npaint {mode} {} {} {}\n{}", place[0], place[1], place[2], paint.axis, paint.width, colours.join(" "), spec_text(spec));
    }
    text
}

/// The reverse of [`describe`]. The text is from a file, which may be anyone's.
pub fn forms_in(description: &str) -> Result<Vec<Placed>, String> {
    let mut forms = Vec::new();
    for part in description.split(FORM).skip(1) {
        // where it stands, in units; a file from before forms had a place has none
        let part = part.trim_start();
        let (place, part) = match part.strip_prefix("place ").and_then(|rest| rest.split_once('\n')) {
            Some((line, rest)) => {
                let units: Vec<i32> = line.split_whitespace().filter_map(|v| v.parse().ok()).collect();
                (<[i32; 3]>::try_from(units).map_err(|_| "where a form stands cannot be read")?.map(|u| u.clamp(-800, 800)), rest)
            }
            None => ([0; 3], part),
        };
        let (first, spec) = part.split_once('\n').ok_or("a form in it is cut short")?;
        let mut words = first.strip_prefix("paint ").ok_or("a form in it has no colours")?.split_whitespace();
        let mut number = || words.next().and_then(|w| w.parse::<usize>().ok());
        let (Some(mode), Some(axis), Some(width)) = (number(), number(), number()) else { return Err("a form's colours cannot be read".into()) };
        let colours: Option<Vec<[u8; 3]>> = words
            .map(|w| {
                let parts: Vec<u8> = w.split(',').filter_map(|v| v.parse().ok()).collect();
                <[u8; 3]>::try_from(parts).ok()
            })
            .collect();
        // (as many as a paint may have: four was the most until a pattern could have eight, and a form in a pattern's eight colours could then be copied and not pasted)
        let colours = colours.filter(|c| c.len() <= crate::paint::MOST_COLOURS).ok_or("a form's colours cannot be read")?;
        let paint = Paint { mode: *SAVED_MODES.get(mode).ok_or("a form's colouring is not one this editor has")?, colours, axis: axis.min(2), width: width.clamp(1, 1000) as i32 };
        forms.push((parse_spec(spec)?, paint, place));
    }
    Ok(forms)
}

/// The workspace as the bytes of a save.
#[cfg(not(target_arch = "wasm32"))]
pub fn pack(bricks: &[Painted], forms: &[Placed]) -> Result<Vec<u8>, String> {
    crate::brz::prefab(bricks, forms).and_then(|world| world.to_brz_vec().map_err(|e| e.to_string()))
}

#[cfg(target_arch = "wasm32")]
pub fn pack(_: &[Painted], forms: &[Placed]) -> Result<Vec<u8>, String> {
    Ok(describe(forms).into_bytes())
}

/// The forms in a save's bytes; `prefab` says they are a .brz file's. The
/// bytes are from a file, which may be anyone's.
pub fn unpack(bytes: &[u8], prefab: bool) -> Result<Vec<Placed>, String> {
    if prefab {
        #[cfg(not(target_arch = "wasm32"))]
        return crate::brz::read(bytes);
        #[cfg(target_arch = "wasm32")]
        return Err("a .brz file cannot be read in the browser".into());
    }
    let text = std::str::from_utf8(bytes).map_err(|_| "it is not text")?;
    if text.contains(FORM) {
        return forms_in(text);
    }
    // a save from before saves held the workspace: one form's settings
    Ok(vec![(parse_spec(text)?, Paint::default(), [0; 3])])
}

/// Save the workspace under `name` in `folder`. Returns what to tell the player.
pub fn save(folder: &str, name: &str, bricks: &[Painted], forms: &[Placed]) -> Result<String, String> {
    files::write(folder, name, &pack(bricks, forms)?)
}

/// The forms saved under `name` in `folder`.
pub fn load(folder: &str, name: &str) -> Result<Vec<Placed>, String> {
    let (bytes, prefab) = files::read(folder, name)?;
    unpack(&bytes, prefab)
}

/// Folders offered even before anything is saved in them.
pub const STARTER_FOLDERS: [&str; 2] = ["arches", "domes"];

/// A name that is safe as a file or folder name on every system: letters,
/// digits, spaces, hyphens and underscores, at most 40 characters, with no
/// space at either end. Anything else (a path separator, `..`, a drive
/// letter's colon) is refused, not repaired, so that the name shown is
/// always the name used.
pub fn clean(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("give it a name first".into());
    }
    if name.chars().count() > 40 {
        return Err("that name is too long (40 characters at most)".into());
    }
    if !name.chars().all(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_')) {
        return Err("names may use letters, digits, spaces, hyphens and underscores only".into());
    }
    Ok(name.to_string())
}

fn sorted_with_starters(mut found: Vec<String>) -> Vec<String> {
    found.extend(STARTER_FOLDERS.iter().map(|s| s.to_string()));
    found.sort_by_key(|s| s.to_lowercase());
    found.dedup_by_key(|s| s.to_lowercase());
    found
}

#[cfg(not(target_arch = "wasm32"))]
mod files {
    use super::*;
    use std::path::PathBuf;

    /// Where saves are kept: see `places`. With no home folder to put them
    /// in, a folder that cannot exist, so that nothing is listed and a save
    /// says why it failed.
    fn root() -> PathBuf {
        let Ok(root) = crate::places::forms() else { return PathBuf::new().join("\0no home folder") };
        // Saves have been kept in two other places: `arcs-forms` beside
        // where the program was started, and then `Documents/omegga-arcs`,
        // which was the program's name. The first time the present folder
        // is wanted, saves found in the later of those are copied over (and
        // left where they were), so that neither move loses anyone's forms.
        let before = [crate::places::forms_once_at(), Some(std::env::current_dir().unwrap_or_default().join("arcs-forms"))];
        if let Some(old) = before.into_iter().flatten().find(|old| old.is_dir()).filter(|_| !root.exists()) {
            for folder in names(old.clone(), true) {
                let (from, to) = (old.join(&folder), root.join(&folder));
                if std::fs::create_dir_all(&to).is_ok() {
                    for name in names(from.clone(), false) {
                        for ext in ["brz", "txt"] {
                            let _ = std::fs::copy(from.join(format!("{name}.{ext}")), to.join(format!("{name}.{ext}")));
                        }
                    }
                }
            }
        }
        root
    }

    /// Names of the entries of `dir` that are folders (or, if not, saves:
    /// `.brz` files, and the `.txt` files of earlier versions).
    fn names(dir: PathBuf, folders: bool) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
        entries
            .flatten()
            .filter_map(|e| {
                let path = e.path();
                let name = if folders {
                    path.is_dir().then(|| path.file_name()?.to_str().map(str::to_string))?
                } else {
                    path.extension().is_some_and(|x| x == "brz" || x == "txt").then(|| path.file_stem()?.to_str().map(str::to_string))?
                }?;
                // a file put there by hand under a name the editor would not
                // have given is left alone, not listed
                clean(&name).ok().filter(|c| *c == name)
            })
            .collect()
    }

    /// Where a folder of saves is on disk.
    pub fn folder_path(folder: &str) -> Result<PathBuf, String> {
        Ok(root().join(clean(folder)?))
    }

    pub fn folders() -> Vec<String> {
        sorted_with_starters(names(root(), true))
    }

    pub fn forms(folder: &str) -> Vec<String> {
        let Ok(folder) = clean(folder) else { return Vec::new() };
        let mut found = names(root().join(folder), false);
        found.sort_by_key(|s| s.to_lowercase());
        // one name saved both ways is one save: see `read`
        found.dedup();
        found
    }

    pub fn add_folder(folder: &str) -> Result<String, String> {
        let folder = clean(folder)?;
        let path = root().join(&folder);
        std::fs::create_dir_all(&path).map_err(|e| format!("could not make the folder {}: {e}", path.display()))?;
        Ok(folder)
    }

    pub fn write(folder: &str, name: &str, bytes: &[u8]) -> Result<String, String> {
        let (folder, name) = (add_folder(folder)?, clean(name)?);
        let path = root().join(folder).join(format!("{name}.brz"));
        std::fs::write(&path, bytes).map_err(|e| format!("could not save {}: {e}", path.display()))?;
        Ok(format!("Saved to {}", path.display()))
    }

    /// Delete a save: its .brz, and the .txt of the same name an earlier
    /// version may have left, which would otherwise be listed in its place.
    pub fn remove(folder: &str, name: &str) -> Result<(), String> {
        let stem = root().join(clean(folder)?).join(clean(name)?);
        let mut gone = false;
        for ext in ["brz", "txt"] {
            let path = stem.with_extension(ext);
            if path.is_file() {
                std::fs::remove_file(&path).map_err(|e| format!("could not delete {}: {e}", path.display()))?;
                gone = true;
            }
        }
        if gone { Ok(()) } else { Err("that form is no longer there".into()) }
    }

    /// A save's bytes, and whether they are a .brz file's. Where a name was
    /// saved both ways, the .brz is the later one and is the one read.
    pub fn read(folder: &str, name: &str) -> Result<(Vec<u8>, bool), String> {
        let stem = root().join(clean(folder)?).join(clean(name)?);
        let prefab = stem.with_extension("brz").is_file();
        let path = stem.with_extension(if prefab { "brz" } else { "txt" });
        let size = std::fs::metadata(&path).map_err(|e| format!("could not open {}: {e}", path.display()))?.len();
        if size > crate::brz::MAX_FILE {
            return Err(format!("{} is {size} bytes, more than the editor will read", path.display()));
        }
        std::fs::read(&path).map(|bytes| (bytes, prefab)).map_err(|e| format!("could not open {}: {e}", path.display()))
    }
}

#[cfg(target_arch = "wasm32")]
mod files {
    use super::*;

    // one entry per form, "arcs-form/<folder>/<name>", and one listing the
    // folders, since an empty folder has no form to be found by
    const FOLDERS: &str = "arcs-folders";

    fn storage() -> Result<web_sys::Storage, String> {
        web_sys::window()
            .and_then(|w| w.local_storage().ok().flatten())
            .ok_or_else(|| "this browser is not letting the page keep anything (private browsing, or storage switched off)".to_string())
    }

    fn key(folder: &str, name: &str) -> String {
        format!("arcs-form/{folder}/{name}")
    }

    pub fn folders() -> Vec<String> {
        let listed = storage().ok().and_then(|s| s.get_item(FOLDERS).ok().flatten()).unwrap_or_default();
        sorted_with_starters(listed.lines().filter_map(|l| clean(l).ok()).collect())
    }

    pub fn forms(folder: &str) -> Vec<String> {
        let (Ok(folder), Ok(storage)) = (clean(folder), storage()) else { return Vec::new() };
        let prefix = key(&folder, "");
        let count = storage.length().unwrap_or(0);
        let mut found: Vec<String> = (0..count).filter_map(|i| storage.key(i).ok().flatten()).filter_map(|k| k.strip_prefix(&prefix).map(str::to_string)).filter(|n| clean(n).is_ok()).collect();
        found.sort_by_key(|s| s.to_lowercase());
        found
    }

    pub fn add_folder(folder: &str) -> Result<String, String> {
        let folder = clean(folder)?;
        let mut all = folders();
        if !all.iter().any(|f| f.eq_ignore_ascii_case(&folder)) {
            all.push(folder.clone());
        }
        storage()?.set_item(FOLDERS, &all.join("\n")).map_err(|_| "the browser's storage for this page is full".to_string())?;
        Ok(folder)
    }

    pub fn write(folder: &str, name: &str, bytes: &[u8]) -> Result<String, String> {
        let (folder, name) = (add_folder(folder)?, clean(name)?);
        let text = std::str::from_utf8(bytes).map_err(|_| "the forms could not be written as text")?;
        storage()?.set_item(&key(&folder, &name), text).map_err(|_| "the browser's storage for this page is full".to_string())?;
        Ok(format!("Saved as {folder} / {name} in this browser"))
    }

    pub fn remove(folder: &str, name: &str) -> Result<(), String> {
        storage()?.remove_item(&key(&clean(folder)?, &clean(name)?)).map_err(|_| "the browser would not delete it".to_string())
    }

    pub fn read(folder: &str, name: &str) -> Result<(Vec<u8>, bool), String> {
        let text = storage()?.get_item(&key(&clean(folder)?, &clean(name)?)).ok().flatten().ok_or_else(|| "that form is no longer there".to_string())?;
        Ok((text.into_bytes(), false))
    }
}

pub use files::{add_folder, folders, forms};

/// Show `folder` of the saved forms in the system's file manager, making it
/// first if nothing has been saved there yet. Returns what to tell the player.
#[cfg(not(target_arch = "wasm32"))]
pub fn show_folder(folder: &str) -> Result<String, String> {
    let path = files::folder_path(folder)?;
    std::fs::create_dir_all(&path).map_err(|e| format!("could not make {}: {e}", path.display()))?;
    // (the path is handed over as one argument to the system's own opener,
    // never as text for a shell to read: see the `open` crate)
    open::that_detached(&path).map_err(|e| format!("could not open {}: {e}", path.display()))?;
    Ok(format!("Opened {}", path.display()))
}

/// Delete the save under `name` in `folder`. It cannot be brought back, so
/// the panel asks first.
pub fn delete(folder: &str, name: &str) -> Result<(), String> {
    files::remove(folder, name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_names_are_clean() {
        assert_eq!(clean("  Gothic arch 2 ").unwrap(), "Gothic arch 2");
        assert_eq!(clean("dome_big-v3").unwrap(), "dome_big-v3");
        assert_eq!(clean("name\n").unwrap(), "name", "space at the ends is trimmed, not refused");
        for bad in ["", "   ", "..", "../secrets", "a/b", "a\\b", "C:", "con.txt", "na\nme","x".repeat(41).as_str(), "tab\tname", "semi;colon", "quo\"te", "amp&ersand"] {
            assert!(clean(bad).is_err(), "{bad:?} was accepted");
        }
    }

    /// A workspace of three forms: one coloured, one on a tilted grid of
    /// its own, and the plain one being edited.
    fn workspace() -> crate::state::State {
        let mut state = crate::state::State::new();
        state.paint = Paint { mode: crate::paint::Mode::Blend, colours: vec![[200, 30, 30], [20, 40, 220]], axis: 0, width: 3 };
        state.add_copy();
        state.add_grid();
        state.spec.grid = [0.0, 0.0, 0.6, 0.8];
        state.spec.grid_at = [30.0, 4.0, 12.0];
        state.refresh(|| 0.0);
        state
    }

    #[test]
    fn a_save_gives_back_every_form_with_its_colours_place_and_grid() {
        let state = workspace();
        let forms = state.forms();
        assert_eq!(forms.len(), 3);
        let bytes = pack(&state.painted(), &forms).unwrap();
        assert_eq!(unpack(&bytes, cfg!(not(target_arch = "wasm32"))).unwrap(), forms);
        // and as the text a browser keeps
        assert_eq!(unpack(describe(&forms).as_bytes(), false).unwrap(), forms);
    }

    #[test]
    fn a_form_in_eight_colours_is_saved_and_opened_again() {
        // a form that took a pattern's eight colours and was then set to blend, as a player's was
        let mut state = crate::state::State::new();
        for mode in [crate::paint::Mode::Blend, crate::paint::Mode::Pattern] {
            state.paint = Paint { mode, colours: crate::paint::palette("rug").to_vec(), axis: 2, width: 2 };
            assert_eq!(state.paint.colours.len(), crate::paint::MOST_COLOURS);
            state.refresh(|| 0.0);
            let forms = state.forms();
            assert_eq!(unpack(&pack(&state.painted(), &forms).unwrap(), cfg!(not(target_arch = "wasm32"))).unwrap(), forms);
        }
        // one colour more than a paint may have is still refused
        let nine = describe(&state.forms()).replacen("paint ", "paint ", 1).lines().map(|line| if line.starts_with("paint ") { format!("{line} 1,2,3") } else { line.to_string() }).collect::<Vec<_>>().join("
");
        assert!(unpack(nine.as_bytes(), false).is_err());
    }

    #[test]
    fn a_save_from_before_saves_held_the_workspace_still_opens() {
        let spec = workspace().full_spec();
        let opened = unpack(spec_text(&spec).as_bytes(), false).unwrap();
        assert_eq!(opened, vec![(spec, Paint::default(), [0; 3])]);
        assert!(unpack(&[0xff, 0xfe, 0x00], false).is_err(), "bytes that are not text were taken as a form");
    }

    #[test]
    fn starter_folders_are_always_listed_once() {
        let all = sorted_with_starters(vec!["Arches".into(), "towers".into()]);
        assert_eq!(all, ["Arches", "domes", "towers"]);
    }
    #[test]
    fn a_cube_is_saved_and_opened_as_it_was() {
        let mut state = crate::state::State::new();
        state.add_cube();
        // a hollow box 30 by 12 by 6, patterned, and one left as it started
        state.spec.stretch = brickloft_core::DVec3::new(30.0, 12.0, 6.0) / 17.0;
        (state.spec.fill, state.spec.thick) = (false, 2.0);
        state.spec.mosaic = Some(brickloft_core::pattern::Mosaic { style: 1, size: 2, bevel: false, border: 1, seed: 5, mortar: true, stagger: -1, upright: false, uneven: [0; 2] });
        state.refresh(|| 0.0);
        state.add_cube();
        state.refresh(|| 0.0);
        let forms = state.forms();
        assert_eq!(forms.iter().filter(|f| f.0.form == brickloft_core::spec::Form::Cube).count(), 2);
        let bytes = pack(&state.painted(), &forms).unwrap();
        let opened = unpack(&bytes, cfg!(not(target_arch = "wasm32"))).unwrap();
        assert_eq!(opened, forms);
        assert_eq!(unpack(describe(&forms).as_bytes(), false).unwrap(), forms);
        // and builds the same bricks once opened
        let mut other = crate::state::State::new();
        other.open(opened);
        other.refresh(|| 0.0);
        assert_eq!(other.built.bricks, state.built.bricks);
        assert_eq!(other.others[1].built.bricks, state.others[1].built.bricks);
        assert!(!other.others[1].built.bricks.is_empty());
    }
}
