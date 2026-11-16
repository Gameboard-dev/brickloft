//! Where the weights are kept when no folder is given.
//!
//! The editor passes `--weights` with a folder of its own choosing (its
//! `places` module keeps the editor's fixed places), so this is only what a
//! person at a command line gets. It is worked out from the environment each
//! time and no path is compiled in.
//!
//! Not beside the program: it may have been started from a folder that
//! cannot be written to, and several gigabytes do not belong beside a
//! program a person may copy about. Not under Documents either, where the
//! editor keeps saved forms: Documents is commonly synchronised to a cloud
//! drive, and the weights are 3.8 GB that can be fetched again. So: the
//! system's folder for a program's own data on this computer.
//!
//! | System | Folder |
//! |---|---|
//! | Windows | `%LOCALAPPDATA%\brickloft\weights` |
//! | macOS | `~/Library/Application Support/brickloft/weights` |
//! | others | `$XDG_DATA_HOME/brickloft/weights`, or `~/.local/share/brickloft/weights` |
//!
//! Taken from the environment and not from the `dirs` crate for the reason
//! the editor's `places` gives: the crate wraps a system interface per
//! platform to find folders whose usual places are these.
//! https://specifications.freedesktop.org/basedir-spec/latest/

use std::ffi::OsString;
use std::path::PathBuf;

const OURS: &str = "brickloft";

/// The default weights folder, or why there is none. `var` reads the
/// environment (a parameter so that a test can stand in for it).
pub fn weights_from(var: impl Fn(&str) -> Option<OsString>) -> Result<PathBuf, String> {
    // an empty or relative value is not a place: the specification says to pass over one
    let folder = |name: &str| var(name).map(PathBuf::from).filter(|p| p.is_absolute());
    let base = if cfg!(windows) {
        folder("LOCALAPPDATA").ok_or("LOCALAPPDATA is not set")?
    } else if cfg!(target_os = "macos") {
        folder("HOME").ok_or("HOME is not set")?.join("Library").join("Application Support")
    } else {
        match folder("XDG_DATA_HOME") {
            Some(data) => data,
            None => folder("HOME").ok_or("HOME is not set")?.join(".local").join("share"),
        }
    };
    Ok(base.join(OURS).join("weights"))
}

pub fn weights() -> Result<PathBuf, String> {
    weights_from(|name| std::env::var_os(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_folder_comes_from_the_environment() {
        let home = if cfg!(windows) { "C:\\Users\\someone" } else { "/home/someone" };
        let found = weights_from(|name| matches!(name, "LOCALAPPDATA" | "HOME").then(|| OsString::from(home))).unwrap();
        assert!(found.starts_with(home), "{}", found.display());
        assert!(found.ends_with(PathBuf::from("brickloft").join("weights")));
        // with nothing to go by there is no folder, and no guess at one
        assert!(weights_from(|_| None).is_err());
        assert!(weights_from(|_| Some(OsString::from("relative/place"))).is_err());
    }
}
