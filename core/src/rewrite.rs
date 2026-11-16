//! Rule programs: buildings and other things grown on a grid by rewrite
//! rules, and written out as bricks.
//!
//! The language and the programs are Maxim Gumin's MarkovJunior
//! (https://github.com/mxgmn/MarkovJunior, MIT, 2022). A program is an
//! XML file: a tree of nodes that each find a small pattern of cells in a
//! grid and write another in its place, one match at a time, all at once,
//! or as wave function collapse over a set of tiles. Run from an empty
//! grid, a page of such rules builds a block of flats with stairs,
//! columns and windows. This module is an interpreter for that language,
//! written here from the original's C# source and behaving as it does,
//! so that its programs run unchanged:
//!
//! - [`xml`] and [`vox`] read the two kinds of file a program is kept in
//! - [`rule`] is a grid, a rule, and the symmetries a rule is copied by
//! - [`nodes`] is the machine: a program loaded, and run a turn at a time
//! - [`wfc`] is the wave function collapse node
//! - [`bricks`] writes a grid out as bricks, plain or with wedges
//! - [`fault`] is every way any of that can fail, and the words for it
//!
//! ## Why an interpreter here, and not the original program
//!
//! The original is a .NET program. Starting it from the editor would
//! mean a second runtime to install, a process to manage and files
//! passed between the two, for a result that arrives only when it is
//! finished. The point of these programs in an editor is to watch them
//! grow and stop them, which needs the machine in this process, stepped
//! by the frame ([`nodes::Machine::advance`]).
//!
//! ## What differs from the original
//!
//! Random numbers. The original draws them from .NET's `System.Random`;
//! this draws them from the project's own generator, so that a seed gives
//! the same bricks on every machine. A seed here therefore gives a
//! different building from the same seed there, of the same kind.
//! Everything else that could differ between machines (the logarithm in
//! an entropy, the power in a temperature) is done by arithmetic alone:
//! see [`nodes::ln`].
//!
//! Not run yet: `convchain`, `wfc` with a sample picture in place of a
//! tileset, `observe` (inference and search), and rules kept in .png
//! pictures, which only flat programs use. A program that uses one is
//! refused with a message naming it and its line.
//!
//! ## Files from outside
//!
//! A program may be anyone's file. Every size it gives is held to
//! [`fault::Limits`]; the files it refers to are read by plain name from
//! the folders it is allowed and nowhere else ([`Resources`]); and a run
//! is a number of turns the caller asks for, so it can always be stopped.

// (Each part is named by its path while the interpreter is on trial and brought into an example
// and a test by path, where a module's parts are looked for beside the file that names it.)
#[path = "rewrite/bricks.rs"]
pub mod bricks;
#[path = "rewrite/fault.rs"]
pub mod fault;
#[path = "rewrite/nodes.rs"]
pub mod nodes;
#[path = "rewrite/rule.rs"]
pub mod rule;
#[path = "rewrite/vox.rs"]
pub mod vox;
#[path = "rewrite/wfc.rs"]
pub mod wfc;
#[path = "rewrite/xml.rs"]
pub mod xml;

pub use bricks::{bricks, Bricks, Finish};
pub use fault::{Fault, Limits};
pub use nodes::{Machine, View};

use std::path::PathBuf;

/// Where the files a program refers to may be read from: its rules'
/// larger patterns (`rules/<folder>/<name>.vox`) and its tilesets
/// (`tilesets/<name>.xml` and `tilesets/<name>/<tile>.vox`).
///
/// A file is looked for under each of `roots` in turn, there and in a
/// `resources` folder there (which is how the original lays them out),
/// and then among `bundled`, the files compiled into the program. A name
/// is taken apart into folder and file names before it is used, and each
/// must be a plain name: letters, digits, and `_ - .` and spaces, not
/// starting with a dot. So no name a program can give reaches a parent
/// folder, a drive or another machine; and a link inside a root that
/// leads out of it is refused, by comparing where the file really is.
#[derive(Clone, Debug, Default)]
pub struct Resources {
    pub roots: Vec<PathBuf>,
    pub bundled: &'static [(&'static str, &'static [u8])],
}

impl Resources {
    /// The bytes of a file, and the name to call it in messages. `parts` are its folders and its own name.
    pub fn read(&self, parts: &[&str], limit: u64) -> Result<(String, Vec<u8>), Fault> {
        let name = parts.join("/");
        let plain = |part: &&str| !part.is_empty() && part.len() <= 120 && !part.starts_with('.') && !part.contains("..") && part.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ' '));
        if !parts.iter().all(plain) {
            return Err(Fault::Outside { name });
        }
        let unreadable = |why: std::io::Error| Fault::Unreadable { name: name.clone(), why: why.to_string() };
        for root in &self.roots {
            for inside in ["", "resources"] {
                let mut path = root.clone();
                if !inside.is_empty() {
                    path.push(inside);
                }
                path.extend(parts);
                if !path.is_file() {
                    continue;
                }
                let (real_root, real) = (root.canonicalize().map_err(unreadable)?, path.canonicalize().map_err(unreadable)?);
                if !real.starts_with(&real_root) {
                    return Err(Fault::Outside { name });
                }
                let size = std::fs::metadata(&real).map_err(unreadable)?.len();
                fault::within(fault::Thing::File, size, limit, &name, 0)?;
                return Ok((name.clone(), std::fs::read(&real).map_err(unreadable)?));
            }
        }
        match self.bundled.iter().find(|(known, _)| *known == name) {
            Some((_, bytes)) => Ok((name, bytes.to_vec())),
            None => Err(Fault::NoResource { name }),
        }
    }
}

/// The colour of each letter, read from a palette file: `<color symbol="B" value="000000"/>` and so on.
pub fn palette(file: &str, bytes: &[u8], limits: &Limits) -> Result<Vec<(char, [u8; 3])>, Fault> {
    let root = xml::read(file, bytes, limits)?;
    let mut colours = Vec::new();
    for colour in root.elements(&["color"]) {
        let missing = |attribute| Fault::Missing { file: file.to_string(), line: colour.line, node: "color".into(), attribute };
        let symbol = colour.get("symbol").and_then(|s| s.chars().next()).ok_or_else(|| missing("symbol"))?;
        let value = colour.get("value").ok_or_else(|| missing("value"))?;
        let rgb = u32::from_str_radix(value, 16).ok().filter(|_| value.len() == 6).ok_or(Fault::BadValue { file: file.to_string(), line: colour.line, attribute: "value", value: value.to_string() })?;
        colours.push((symbol, [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8]));
    }
    Ok(colours)
}

#[cfg(test)]
#[path = "rewrite/tests.rs"]
mod tests;
