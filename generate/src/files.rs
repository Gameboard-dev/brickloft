//! Files written so that none is ever seen half-made, and a run that can be
//! stopped at any moment.
//!
//! A file is written under another name in the folder it is meant for and
//! renamed when it is whole. Renaming within a folder replaces the old file
//! in one step on every system this runs on
//! (https://doc.rust-lang.org/std/fs/fn.rename.html), so a reader finds the
//! old file or the new one and never part of either. If the program is
//! killed, what is left is a file whose name has `.part` in it, which
//! nothing reads and the next run writes over.
//!
//! Stopping: the editor cannot send a signal to a program on Windows without
//! a system call per platform, and a killed program cannot tidy up. So the
//! editor holds this program's standard input open and closes it to cancel;
//! `watch_stdin` sees the end of input, removes the part-files in hand and
//! ends the program. It is asked for with `--watch-stdin` and is not the
//! default, because a program started with no input at all (from a script,
//! or by a scheduler) reads the end of input at once and would cancel
//! itself before it began.

use crate::error::{Error, Result};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The part-files being written now, for `cancel` to remove.
static PARTS: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// The name a file is written under until it is whole: `barrel.glb` is
/// written as `barrel.part.glb`. The ending is kept because the libraries
/// that write pictures and meshes choose the format by it.
pub fn part_name(path: &Path) -> PathBuf {
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let name = match path.extension() {
        Some(ending) => format!("{stem}.part.{}", ending.to_string_lossy()),
        None => format!("{stem}.part"),
    };
    path.with_file_name(name)
}

/// A file on its way to being written. Dropped without `keep`, the
/// part-file is removed, so a failure half-way leaves nothing behind.
pub struct Part {
    part: PathBuf,
    whole: PathBuf,
}

impl Part {
    /// Makes the folder if it is not there. Nothing is written yet.
    pub fn of(whole: &Path) -> Result<Part> {
        if let Some(folder) = whole.parent().filter(|f| !f.as_os_str().is_empty()) {
            std::fs::create_dir_all(folder).map_err(|source| Error::Io { doing: "make the folder", path: folder.to_path_buf(), source })?;
        }
        let part = part_name(whole);
        if let Ok(mut parts) = PARTS.lock() {
            parts.push(part.clone());
        }
        Ok(Part { part, whole: whole.to_path_buf() })
    }

    /// Where to write.
    pub fn path(&self) -> &Path {
        &self.part
    }

    /// The file is whole: give it its name.
    pub fn keep(self) -> Result<()> {
        std::fs::rename(&self.part, &self.whole).map_err(|source| Error::Io { doing: "give its name to", path: self.whole.clone(), source })
        // `drop` then finds no part-file to remove
    }
}

impl Drop for Part {
    fn drop(&mut self) {
        if let Ok(mut parts) = PARTS.lock() {
            parts.retain(|p| p != &self.part);
        }
        let _ = std::fs::remove_file(&self.part);
    }
}

/// Writes `bytes` as `whole`, by way of a part-file.
pub fn write(whole: &Path, bytes: &[u8]) -> Result<()> {
    let part = Part::of(whole)?;
    std::fs::write(part.path(), bytes).map_err(|source| Error::Io { doing: "write", path: part.path().to_path_buf(), source })?;
    part.keep()
}

/// Removes the part-files in hand and ends the program. Never returns.
pub fn cancel(out: &crate::protocol::Out) -> ! {
    // Held until the program ends: a part-file cannot be made while these are being removed.
    let parts = PARTS.lock();
    if let Ok(parts) = &parts {
        for part in parts.iter() {
            let _ = std::fs::remove_file(part);
        }
    }
    out.error(&Error::Cancelled);
    std::process::exit(Error::Cancelled.exit_code().into())
}

/// Ends the program when standard input ends. What arrives on it before
/// then is read and thrown away.
pub fn watch_stdin(out: std::sync::Arc<crate::protocol::Out>) {
    std::thread::spawn(move || {
        let mut sink = [0u8; 256];
        let mut stdin = std::io::stdin().lock();
        loop {
            match stdin.read(&mut sink) {
                Ok(0) | Err(_) => cancel(&out),
                Ok(_) => {}
            }
        }
    });
}

/// How long a file is, or that it is missing.
pub fn length(path: &Path) -> Option<u64> {
    std::fs::metadata(path).ok().filter(|m| m.is_file()).map(|m| m.len())
}

/// Reads a small file whole, refusing one longer than `most` before any of
/// it is read: the file is whatever is on the disk, which may not be what
/// was put there.
pub fn read_small(path: &Path, most: u64) -> Result<Vec<u8>> {
    let io = |source| Error::Io { doing: "read", path: path.to_path_buf(), source };
    let file = std::fs::File::open(path).map_err(io)?;
    let mut bytes = Vec::new();
    // `take` and not the length from the folder: the file can grow between the asking and the reading
    file.take(most + 1).read_to_end(&mut bytes).map_err(io)?;
    if bytes.len() as u64 > most {
        return Err(Error::Io { doing: "read", path: path.to_path_buf(), source: std::io::Error::new(std::io::ErrorKind::InvalidData, format!("it is longer than the {most} bytes such a file may be")) });
    }
    Ok(bytes)
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// A folder for one test, removed when it is dropped.
    pub struct Scratch(pub PathBuf);

    impl Scratch {
        pub fn new(name: &str) -> Scratch {
            let folder = std::env::temp_dir().join(format!("brickloft-generate-test-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&folder);
            std::fs::create_dir_all(&folder).unwrap();
            Scratch(folder)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_file_has_its_name_only_when_whole() {
        let scratch = Scratch::new("part");
        let whole = scratch.0.join("deep").join("mesh.glb");
        let part = Part::of(&whole).unwrap();
        assert_eq!(part.path(), scratch.0.join("deep").join("mesh.part.glb"));
        std::fs::write(part.path(), b"half").unwrap();
        assert!(!whole.exists(), "nothing is under the final name while it is being written");
        part.keep().unwrap();
        assert_eq!(std::fs::read(&whole).unwrap(), b"half");
        assert!(!scratch.0.join("deep").join("mesh.part.glb").exists());
    }

    #[test]
    fn a_failure_half_way_leaves_nothing() {
        let scratch = Scratch::new("dropped");
        let whole = scratch.0.join("picture.png");
        std::fs::write(&whole, b"the old one").unwrap();
        {
            let part = Part::of(&whole).unwrap();
            std::fs::write(part.path(), b"half of the new").unwrap();
            // dropped without `keep`, as when an error is returned
        }
        assert_eq!(std::fs::read(&whole).unwrap(), b"the old one");
        assert_eq!(std::fs::read_dir(&scratch.0).unwrap().count(), 1);
    }

    #[test]
    fn a_small_file_that_is_not_small_is_refused() {
        let scratch = Scratch::new("small");
        let path = scratch.0.join("vocab.json");
        std::fs::write(&path, vec![b'x'; 101]).unwrap();
        assert!(read_small(&path, 100).is_err());
        assert_eq!(read_small(&path, 101).unwrap().len(), 101);
    }
}
