//! Rule programs brought in from outside: an .xml file someone wrote in
//! MarkovJunior's language, copied into the editor's own folder with the
//! files it reads.
//!
//! A form names its program and never a path
//! (`brickloft_core::rewrite::form::Program`), so a save that names
//! `imported:Castle` opens wherever there is a `Castle` in this folder,
//! and a save from someone else cannot make the editor read a file of its
//! choosing. That is why a program is copied here and not run from where
//! it lies: the copy is what the name means, and it stays when the
//! download it came from is tidied away.
//!
//! What is copied is the program and exactly the files it reads when it
//! is loaded: its tileset, the tiles, the rules it keeps in .vox files,
//! and a palette if there is one beside it. The loader says which those
//! are (`Resources::seen`), so nothing is guessed from the text and
//! nothing else in the folder is touched. They are looked for beside the
//! program and in a `resources` folder beside it, and nowhere else on the
//! disk; a file found in neither place is taken from those that come with
//! the editor, which are the original's own.
//!
//! Only a person brings a program in, by typing its path in the panel or
//! dropping the file on the window. Claude, through MCP, may name a
//! program that is already here and has no tool that takes a path.

use brickloft_core::rewrite::form::{self, Program};
use brickloft_core::rewrite::{bundled, fault, Fault, Limits, Machine, Resources};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// The file a program's usual size is kept in, in its folder: three whole numbers.
const SIZE_FILE: &str = "size.txt";

/// The names of the programs that have been brought in, in order.
pub fn imported() -> Vec<String> {
    let Some(library) = form::library() else { return Vec::new() };
    let Ok(folders) = std::fs::read_dir(library) else { return Vec::new() };
    let mut names: Vec<String> = folders.flatten().filter_map(|folder| folder.file_name().into_string().ok()).filter(|name| form::plain(name) && form::library().is_some_and(|library| library.join(name).join(format!("{name}.xml")).is_file())).collect();
    names.sort();
    names
}

/// The size a program brought in starts at, if a size was found for it when it was brought in.
pub fn usual_size(name: &str) -> Option<[i32; 3]> {
    let text = std::fs::read_to_string(form::library()?.join(name).join(SIZE_FILE)).ok()?;
    let sides: Vec<i32> = text.split_whitespace().filter_map(|side| side.parse().ok()).collect();
    <[i32; 3]>::try_from(sides).ok().map(|size| size.map(|side| side.clamp(1, form::MOST_SIDE)))
}

/// The size the original's list of models gives a program, read as the
/// original reads it (`Program.cs`: `size`, or `length`, `width` and
/// `height`, and `d` for whether a square is a cube). None if the list
/// does not name the program or gives it no size.
/// https://github.com/mxgmn/MarkovJunior/blob/main/source/Program.cs
fn listed_size(models: &[u8], name: &str) -> Option<[i32; 3]> {
    let root = brickloft_core::rewrite::xml::read("models.xml", models, &Limits::USUAL).ok()?;
    let model = root.elements(&["model"]).find(|model| model.get("name") == Some(name))?;
    let number = |attribute: &str| model.get(attribute).and_then(|value| value.trim().parse::<i32>().ok());
    let linear = number("size");
    let flat = number("d").unwrap_or(2) == 2;
    let size = [number("length").or(linear)?, number("width").or(linear)?, number("height").or(if flat { Some(1) } else { linear })?];
    Some(size.map(|side| side.clamp(1, form::MOST_SIDE)))
}

fn unreadable(name: &str) -> impl Fn(std::io::Error) -> Fault + '_ {
    move |why| Fault::Unreadable { name: name.to_string(), why: why.to_string() }
}

/// A file's bytes, if it is no larger than `limit`.
fn read(path: &Path, shown: &str, limit: u64) -> Result<Vec<u8>, Fault> {
    let size = std::fs::metadata(path).map_err(unreadable(shown))?.len();
    fault::within(fault::Thing::File, size, limit, shown, 0)?;
    std::fs::read(path).map_err(unreadable(shown))
}

/// Bring in the program at `path`: check that it loads, copy it and what
/// it reads into the editor's folder, and give the name it now goes by
/// (its file's name, less `.xml`). One of that name brought in before is
/// replaced. `Err` is why not, in words to show.
pub fn import(path: &Path) -> Result<String, String> {
    Ok(brought_in(path)?)
}

fn brought_in(path: &Path) -> Result<String, Fault> {
    let shown = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    let name = path.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default();
    if !path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("xml")) {
        return Err(Fault::NotXml { file: shown });
    }
    if !form::plain(&name) {
        return Err(Fault::BadName { name });
    }
    let library = form::library().ok_or(Fault::NoProgram { name: name.clone(), imported: true })?;
    let limits = Limits::USUAL;
    let text = read(path, &shown, limits.text)?;
    let beside = path.parent().map(Path::to_path_buf).unwrap_or_default();
    // its size: what a list of models beside it says, or the usual
    let size = read(&beside.join("models.xml"), "models.xml", limits.text).ok().and_then(|models| listed_size(&models, &name)).unwrap_or(form::USUAL_SIZE);
    // Loaded once from where it lies, to learn that it loads and which files it reads.
    let seen = Arc::new(Mutex::new(Vec::new()));
    let resources = Resources { roots: vec![beside], bundled: bundled::RESOURCES, seen: Some(seen.clone()) };
    Machine::load(&shown, &text, size.map(|side| side as usize), &resources, &limits)?;
    // (and its palette, if it has one of its own: the form's colours are read from it)
    let _ = resources.read(&["palette.xml"], limits.text);
    let mut files: Vec<(String, PathBuf)> = seen.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    files.sort();
    files.dedup();
    // Copied. (The folder is this program's own, under a plain name: taking it away takes away nothing else.)
    let folder = library.join(&name);
    let copied = || -> Result<(), Fault> {
        if folder.exists() {
            std::fs::remove_dir_all(&folder).map_err(unreadable(&name))?;
        }
        std::fs::create_dir_all(&folder).map_err(unreadable(&name))?;
        std::fs::write(folder.join(format!("{name}.xml")), &text).map_err(unreadable(&shown))?;
        std::fs::write(folder.join(SIZE_FILE), size.map(|side| side.to_string()).join(" ")).map_err(unreadable(SIZE_FILE))?;
        for (asked, found) in &files {
            // (a name the loader has already held to plain parts: see `Resources::read`)
            let to = asked.split('/').fold(folder.clone(), |to, part| to.join(part));
            if let Some(parent) = to.parent() {
                std::fs::create_dir_all(parent).map_err(unreadable(asked))?;
            }
            std::fs::copy(found, &to).map_err(unreadable(asked))?;
        }
        Ok(())
    };
    form::forget(&name);
    // and loaded again from the copy, which is what a form will run: a copy that does not load is not left behind
    let checked = copied().and_then(|()| form::Grower::new(&Program { size, ..Program::of(&name, true) }).map(|_| ()));
    if let Err(fault) = checked {
        let _ = std::fs::remove_dir_all(&folder);
        return Err(fault);
    }
    Ok(name)
}

#[cfg(test)]
mod tests;
