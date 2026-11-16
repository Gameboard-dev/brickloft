//! Where the program keeps its files, on the desktop.
//!
//! Nothing is written beside where the program was started: a player with
//! only the executable may have started it from anywhere, a read-only
//! folder included. Each kind of file has one place instead, the same on
//! Windows, macOS and Linux relative to the user's home folder:
//!
//! | Files | Folder |
//! |---|---|
//! | saved forms | `Documents/brickloft/forms/` |
//! | rule programs brought in from outside | `Documents/brickloft/programs/` |
//! | screenshots, and recordings of the window | `Pictures/brickloft/` |
//! | reports, and the prefab a copy puts on the clipboard | `brickloft/` in the system's temporary folder |
//!
//! `Documents` and `Pictures` are taken to be in the home folder, which is
//! where all three systems keep them unless the user has moved them. A user
//! who has gets a folder of that name in the old place. Asking each system
//! where the folders really are takes a system interface per platform (or
//! the `dirs` crate, which wraps them), for folders every message names
//! anyway.
//!
//! The temporary folder is the system's own, from the standard library:
//! https://doc.rust-lang.org/std/env/fn.temp_dir.html

use std::path::PathBuf;

/// The name of the program's folder in each place.
const OURS: &str = "brickloft";

fn made(folder: PathBuf) -> Result<PathBuf, String> {
    std::fs::create_dir_all(&folder).map_err(|e| format!("could not make {}: {e}", folder.display()))?;
    Ok(folder)
}

fn home() -> Result<PathBuf, String> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from).ok_or_else(|| "the home folder is not known".to_string())
}

/// Where saved forms are kept. Not made until something is saved: see `store`.
pub fn forms() -> Result<PathBuf, String> {
    Ok(home()?.join("Documents").join(OURS).join("forms"))
}

/// Where rule programs brought in from outside are kept, each in a folder
/// of its name with the files it reads (see `programs`). Not made until
/// one is brought in. Beside the saved forms, which name them: the two
/// are backed up or moved to another computer together.
pub fn programs() -> Result<PathBuf, String> {
    Ok(home()?.join("Documents").join(OURS).join("programs"))
}

/// Where saved forms were kept while the program was called omegga-arcs,
/// for `store` to bring them from.
pub fn forms_once_at() -> Option<PathBuf> {
    Some(home().ok()?.join("Documents").join("omegga-arcs").join("forms"))
}

/// Where screenshots and GIF recordings go, made if it is not there.
pub fn screenshots() -> Result<PathBuf, String> {
    made(home()?.join("Pictures").join(OURS))
}

/// Where files that need not be kept go, made if it is not there.
pub fn passing() -> Result<PathBuf, String> {
    made(std::env::temp_dir().join(OURS))
}
