//! Saved forms, kept in named folders.
//!
//! On the desktop a form is a small text file (the same `key = value` text a
//! report's `[spec]` section has) under `arcs-forms/<folder>/` beside where
//! the editor was started, so saved forms can be read, copied and shared as
//! files. A web page cannot write files, so in the browser the same folders
//! and forms live in the page's local storage:
//! https://developer.mozilla.org/en-US/docs/Web/API/Window/localStorage
//!
//! Folder and form names come from a text box, and on the desktop they
//! become part of a file path, so only [`clean`] names are ever used.

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

    fn root() -> PathBuf {
        std::env::current_dir().unwrap_or_default().join("arcs-forms")
    }

    /// Names of the entries of `dir` that are folders (or, if not, `.txt` files).
    fn names(dir: PathBuf, folders: bool) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
        entries
            .flatten()
            .filter_map(|e| {
                let path = e.path();
                let name = if folders {
                    path.is_dir().then(|| path.file_name()?.to_str().map(str::to_string))?
                } else {
                    (path.extension()? == "txt").then(|| path.file_stem()?.to_str().map(str::to_string))?
                }?;
                // a file put there by hand under a name the editor would not
                // have given is left alone, not listed
                clean(&name).ok().filter(|c| *c == name)
            })
            .collect()
    }

    pub fn folders() -> Vec<String> {
        sorted_with_starters(names(root(), true))
    }

    pub fn forms(folder: &str) -> Vec<String> {
        let Ok(folder) = clean(folder) else { return Vec::new() };
        let mut found = names(root().join(folder), false);
        found.sort_by_key(|s| s.to_lowercase());
        found
    }

    pub fn add_folder(folder: &str) -> Result<String, String> {
        let folder = clean(folder)?;
        let path = root().join(&folder);
        std::fs::create_dir_all(&path).map_err(|e| format!("could not make the folder {}: {e}", path.display()))?;
        Ok(folder)
    }

    pub fn save(folder: &str, name: &str, text: &str) -> Result<String, String> {
        let (folder, name) = (add_folder(folder)?, clean(name)?);
        let path = root().join(folder).join(format!("{name}.txt"));
        std::fs::write(&path, text).map_err(|e| format!("could not save {}: {e}", path.display()))?;
        Ok(format!("Saved to {}", path.display()))
    }

    pub fn load(folder: &str, name: &str) -> Result<String, String> {
        let path = root().join(clean(folder)?).join(format!("{}.txt", clean(name)?));
        std::fs::read_to_string(&path).map_err(|e| format!("could not open {}: {e}", path.display()))
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

    pub fn save(folder: &str, name: &str, text: &str) -> Result<String, String> {
        let (folder, name) = (add_folder(folder)?, clean(name)?);
        storage()?.set_item(&key(&folder, &name), text).map_err(|_| "the browser's storage for this page is full".to_string())?;
        Ok(format!("Saved as {folder} / {name} in this browser"))
    }

    pub fn load(folder: &str, name: &str) -> Result<String, String> {
        storage()?.get_item(&key(&clean(folder)?, &clean(name)?)).ok().flatten().ok_or_else(|| "that form is no longer there".to_string())
    }
}

pub use files::{add_folder, folders, forms, load, save};

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

    #[test]
    fn starter_folders_are_always_listed_once() {
        let all = sorted_with_starters(vec!["Arches".into(), "towers".into()]);
        assert_eq!(all, ["Arches", "domes", "towers"]);
    }
}
