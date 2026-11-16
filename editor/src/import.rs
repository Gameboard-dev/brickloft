//! Mesh files made by other programs: read, kept, and found again.
//!
//! The core library fits a mesh it is handed as data, and knows nothing
//! of files (it is built for the browser too). This is the part that
//! does: three readers (`glb`, `obj`, `ply`), the folder opened meshes are
//! kept in, and the meshes in hand so that a file is read and indexed
//! once and not at every build.
//!
//! # Where a mesh is kept
//!
//! A file that is opened is copied into the editor's own folder
//! (`places::imports`), under a name made from its content: sixteen
//! hexadecimal digits and its ending. A form says which mesh it is by
//! that name alone (`brickloft_core::mesh::Import::name`). So a save
//! opens again after the file it came from has moved, two forms of one
//! file share one copy, and a form can never name a file outside the
//! folder: a name of that shape has nothing in it a path could be made of
//! (`Import::named`). Only [`adopt`] takes a path, and only the person at
//! the editor gives it one, by typing it or dropping a file on the
//! window. A program driving the editor (`mcp`) gives names.
//!
//! The name is the FNV-1a hash of the file
//! (<http://www.isthe.com/chongo/tech/comp/fnv/>), which is what tells two
//! files apart here, not what keeps anyone out: two files made to share a
//! name would show one mesh for the other and nothing worse. A
//! cryptographic hash would stop that, at the price of a crate or two
//! hundred lines of SHA-256, to guard a folder of the user's own models
//! from the user.
//!
//! # Errors
//!
//! Everything here fails with `brickloft_core::mesh::MeshError`, which is
//! where each failure's wording is written.

pub mod glb;
pub mod obj;
pub mod panel;
pub mod ply;
mod srgb;
pub mod write;

use brickloft_core::mesh::{Count, Import, Mesh, MeshError, Parts, Up};
use brickloft_core::spec::{Built, Form, Spec};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// The largest file that is read. A mesh at the limit of its triangles
/// (`brickloft_core::mesh::MAX_TRIANGLES`) with colours is about 100 MB as
/// a `.glb` and three times that as an OBJ, which is text.
pub const MAX_FILE: u64 = 400 << 20;

/// How many meshes are kept in hand at once. Each is its triangles and its
/// hierarchy, up to 150 MB at the limit, so not many; the forms of a
/// workspace hold on to their own whatever this is.
const IN_HAND: usize = 4;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Glb,
    Obj,
    Ply,
}

impl Kind {
    /// The kind of file `name` is, by its ending, in capitals or not.
    pub fn of(name: &str) -> Result<Kind, MeshError> {
        let ending = name.rsplit_once('.').map(|(_, ending)| ending.to_ascii_lowercase());
        match ending.as_deref() {
            Some("glb") => Ok(Kind::Glb),
            Some("obj") => Ok(Kind::Obj),
            Some("ply") => Ok(Kind::Ply),
            _ => Err(MeshError::UnknownKind { name: name.chars().rev().take(80).collect::<Vec<char>>().into_iter().rev().collect() }),
        }
    }

    pub fn ending(self) -> &'static str {
        match self {
            Kind::Glb => "glb",
            Kind::Obj => "obj",
            Kind::Ply => "ply",
        }
    }

    /// Which way is up in a file of this kind, as far as its kind says
    /// (see `brickloft_core::mesh::Up::usual_for`).
    pub fn usual_up(self) -> Up {
        Up::usual_for(&format!(".{}", self.ending()))
    }
}

/// The parts of the mesh in a file's bytes.
pub fn read(bytes: &[u8], kind: Kind) -> Result<Parts, MeshError> {
    if bytes.len() as u64 > MAX_FILE {
        return Err(MeshError::TooMany { what: Count::Bytes, found: bytes.len() as u64, most: MAX_FILE });
    }
    match kind {
        Kind::Glb => glb::read(bytes),
        Kind::Obj => obj::read(bytes),
        Kind::Ply => ply::read(bytes),
    }
}

/// The mesh in a file's bytes, stood with `up` upwards and indexed.
pub fn mesh_of(bytes: &[u8], kind: Kind, up: Up) -> Result<Mesh, MeshError> {
    let mut parts = read(bytes, kind)?;
    for p in &mut parts.positions {
        *p = up.stood(*p);
    }
    Mesh::with(parts)
}

/// The name a file of these bytes is kept under.
pub fn name_of(bytes: &[u8], kind: Kind) -> String {
    let hash = bytes.iter().fold(0xcbf29ce484222325u64, |hash, b| (hash ^ *b as u64).wrapping_mul(0x100000001b3));
    format!("{hash:016x}.{}", kind.ending())
}

/// The folder opened meshes are kept in. (Under a test it is one in the
/// system's temporary folder, so that no test writes among the user's
/// own, or reads what happens to be there.)
pub fn folder() -> Result<PathBuf, MeshError> {
    #[cfg(all(not(test), not(target_arch = "wasm32")))]
    let found = crate::places::imports();
    #[cfg(all(not(test), target_arch = "wasm32"))]
    let found: Result<PathBuf, String> = Err("a web page keeps no files".into());
    #[cfg(test)]
    let found: Result<PathBuf, String> = Ok(std::env::temp_dir().join(format!("brickloft-test-imports-{}", std::process::id())));
    found.map_err(|why| MeshError::File { path: "the imports folder".into(), why })
}

fn file_error(path: &Path, error: std::io::Error) -> MeshError {
    MeshError::File { path: path.display().to_string(), why: error.to_string() }
}

type InHand = Mutex<Vec<(String, Up, Arc<Mesh>)>>;

fn in_hand() -> &'static InHand {
    static MESHES: InHand = Mutex::new(Vec::new());
    &MESHES
}

fn keep(name: &str, up: Up, mesh: Arc<Mesh>) {
    // (a thread that failed while holding the list left it as it was: it is still a list of meshes)
    let mut held = in_hand().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    held.retain(|(known, way, _)| !(known == name && *way == up));
    held.insert(0, (name.to_string(), up, mesh));
    held.truncate(IN_HAND);
}

/// The whole of a file, if it is no larger than is read: by its size as
/// the system gives it, before any of it is read.
fn bytes_of(path: &Path) -> Result<Vec<u8>, MeshError> {
    let about = std::fs::metadata(path).map_err(|e| file_error(path, e))?;
    if !about.is_file() {
        return Err(MeshError::File { path: path.display().to_string(), why: "it is not a file".into() });
    }
    if about.len() > MAX_FILE {
        return Err(MeshError::TooMany { what: Count::Bytes, found: about.len(), most: MAX_FILE });
    }
    std::fs::read(path).map_err(|e| file_error(path, e))
}

/// Take in the mesh file at `path`: read it, and if it is a mesh that can
/// be built, copy it into the imports folder. Returns the name it is kept
/// under and which way up a file of its kind usually is.
///
/// It is read before it is copied, so that the folder holds only files
/// that were once read whole; and the mesh made in reading it is kept in
/// hand, so that the form it is opened for does not read it again.
pub fn adopt(path: &Path) -> Result<(String, Up), MeshError> {
    adopt_into(&folder()?, path)
}

fn adopt_into(folder: &Path, path: &Path) -> Result<(String, Up), MeshError> {
    let kind = Kind::of(&path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())?;
    let bytes = bytes_of(path)?;
    let up = kind.usual_up();
    let mesh = mesh_of(&bytes, kind, up)?;
    let name = name_of(&bytes, kind);
    let kept = folder.join(&name);
    if !kept.exists() {
        std::fs::create_dir_all(folder).map_err(|e| file_error(folder, e))?;
        std::fs::write(&kept, &bytes).map_err(|e| file_error(&kept, e))?;
    }
    keep(&name, up, Arc::new(mesh));
    Ok((name, up))
}

/// The mesh kept under `name`, stood with `up` upwards: the one in hand,
/// or read from the imports folder.
pub fn open(name: &str, up: Up) -> Result<Arc<Mesh>, MeshError> {
    open_from(&folder()?, name, up)
}

fn open_from(folder: &Path, name: &str, up: Up) -> Result<Arc<Mesh>, MeshError> {
    // nothing but a name of the folder's own shape is ever joined to its path
    Import::named(name)?;
    let held = in_hand().lock().unwrap_or_else(|poisoned| poisoned.into_inner()).iter().find(|(known, way, _)| known == name && *way == up).map(|found| found.2.clone());
    if let Some(mesh) = held {
        return Ok(mesh);
    }
    let path = folder.join(name);
    if !path.is_file() {
        return Err(MeshError::Missing { name: name.to_string() });
    }
    let mesh = Arc::new(mesh_of(&bytes_of(&path)?, Kind::of(name)?, up)?);
    keep(name, up, mesh.clone());
    Ok(mesh)
}

/// Give an Imported form its mesh, if it names one and the mesh can be
/// found. A form whose mesh cannot be found is left without, and says so
/// when it is built (`MeshError::Missing`).
///
/// Called wherever a form comes in from outside: a save, the clipboard,
/// a program driving the editor. A form already in the workspace keeps
/// the mesh it has.
pub fn attach(spec: &mut Spec) {
    if spec.form == Form::Imported && spec.import.mesh.is_none() && !spec.import.name.is_empty() {
        spec.import.mesh = open(&spec.import.name, spec.import.up).ok();
    }
}

/// The names of the meshes in the imports folder, in order, with each
/// file's size in bytes. At most a thousand: a folder is not listed
/// without end for a program that asks.
pub fn listed() -> Vec<(String, u64)> {
    let Ok(entries) = folder().and_then(|folder| std::fs::read_dir(&folder).map_err(|e| file_error(&folder, e))) else { return Vec::new() };
    let mut found: Vec<(String, u64)> = entries.flatten().filter_map(|entry| {
        let name = entry.file_name().into_string().ok()?;
        Import::named(&name).ok()?;
        Some((name, entry.metadata().ok()?.len()))
    }).take(1000).collect();
    found.sort();
    found
}

/// The colours an Imported form starts in: its mesh's own, reduced to as
/// many as the form asks for. `None` for a mesh with no colours, or a
/// form that asks for none.
pub fn paint_of(spec: &Spec) -> Option<crate::paint::Paint> {
    let colours = spec.import.mesh.as_ref()?.palette(spec.import.colours.max(0) as usize);
    (!colours.is_empty()).then(|| crate::paint::Paint { mode: crate::paint::Mode::Pattern, colours, ..crate::paint::Paint::default() })
}

/// `brickloft_core::spec::build`, remembering the last mesh built.
///
/// A form is built in several places that each want its bricks at once
/// (when it is added, to find where it stands; when a program asks for a
/// change, to refuse what cannot be built; when the workspace is put
/// back). For the editor's other forms a build is milliseconds and each
/// place builds for itself. A mesh takes seconds, and adding one through
/// MCP built it three times over. So the last Imported form built here is
/// kept with what it was built from, and asked for again it is handed
/// back. One, since the places that build twice build the same form twice
/// in a row.
pub fn built(spec: &Spec) -> Result<Built, String> {
    if spec.form != Form::Imported {
        return brickloft_core::spec::build(spec);
    }
    static LAST: Mutex<Option<(Spec, u64, Built)>> = Mutex::new(None);
    // (where it stands among other forms changes none of its bricks)
    let key = Spec { grid: [0.0, 0.0, 0.0, 1.0], grid_at: [0.0; 3], on_grid: 0, blend: None, carve: false, ..spec.clone() };
    // (two specs are the same whether or not the mesh is in hand, so which mesh it was is kept beside)
    let id = spec.import.mesh.as_ref().map_or(0, |mesh| mesh.id);
    let mut last = LAST.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some((_, _, found)) = last.as_ref().filter(|(of, mesh, _)| *of == key && *mesh == id) {
        return Ok(found.clone());
    }
    let fresh = brickloft_core::spec::build(spec)?;
    *last = Some((key, id, fresh.clone()));
    Ok(fresh)
}

#[cfg(test)]
mod tests;
