//! Everything that can go wrong with a mesh from a file, in one place.
//!
//! A mesh comes from a file that may be anyone's, by way of three readers
//! (in the editor: `import/glb.rs`, `import/obj.rs`, `import/ply.rs`), the
//! checks of [`super::Mesh::with`] and the fitter. Each of those returns
//! `Result<_, MeshError>`, and the one `Display` below is the only place
//! the wording of a failure is written. The alternative, which the rest of
//! this library uses, is a `String` made where the failure is found. That
//! is fine for a form with three ways to fail. Here there are a score of
//! them across five files, and as strings the same failure was worded
//! three ways by three readers, no test could ask which failure it had
//! been given without matching on prose, and a reworded message meant
//! finding every copy.
//!
//! A variant carries the numbers of its failure and no prose. Where the
//! rest of the library wants a `String` (`spec::build` returns one), the
//! error is turned into its `Display` at that boundary and nowhere else
//! (`impl From<MeshError> for String`).

use std::fmt;

/// A count that is held to a limit, or that a file is short of: what the
/// number is a number of. Named here, once, so that a reader cannot word
/// the same count two ways.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Count {
    /// the file's own size
    Bytes,
    Points,
    Triangles,
    /// a glTF scene's nodes, meshes, accessors and the like, taken together
    Parts,
    /// the pixels of a texture
    Pixels,
    /// the cells a build would span
    Cells,
    /// characters in one line of a text file
    LineLength,
}

impl Count {
    fn name(self) -> &'static str {
        match self {
            Count::Bytes => "bytes",
            Count::Points => "points",
            Count::Triangles => "triangles",
            Count::Parts => "nodes, meshes and other parts",
            Count::Pixels => "pixels in a texture",
            Count::Cells => "cells",
            Count::LineLength => "characters in one line",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum MeshError {
    /// The system would not read or write a file: which, and what it said.
    File { path: String, why: String },
    /// A file whose name ends in nothing a reader is written for.
    UnknownKind { name: String },
    /// More of something than is read: checked before anything is
    /// allocated for it, so a file that claims four billion points costs
    /// nothing to refuse.
    TooMany { what: Count, found: u64, most: u64 },
    /// The file ends before what it says it holds: `part` is what was
    /// being read, which `needs` so many bytes where there are `has`.
    CutShort { part: &'static str, needs: u64, has: u64 },
    /// The file is not laid out as its kind is: `kind` is the format and
    /// `why` what was found, in a few words.
    Malformed { kind: &'static str, why: String },
    /// A valid file that uses something not read here.
    Unsupported { what: &'static str },
    /// A coordinate that is infinite or not a number, or text where a
    /// number should be: `at` is the line of a text file, or the number of
    /// the point in a binary one.
    NotANumber { part: &'static str, at: u64 },
    /// A triangle that names a point the mesh does not have.
    NoSuchPoint { index: u64, points: u64 },
    /// glTF nodes nested deeper than is followed, or a node that is its
    /// own ancestor.
    TooDeep { most: u64 },
    /// No triangle with any area, or a mesh with no size.
    Empty,
    /// Colours or texture coordinates for some points and not for others.
    Uneven { part: &'static str, found: u64, points: u64 },
    /// A name that is not the name of a file in the imports folder: the
    /// only names a form may give (see `brickloft_core::mesh::Import::named`).
    NotAnImport { name: String },
    /// A form names a mesh that is not in the imports folder.
    Missing { name: String },
    /// An Imported form that has not been given a file yet.
    NoFile,
    /// Every brick was dropped, or the mesh is too small at this size to
    /// hold one.
    NothingBuilt,
}

impl fmt::Display for MeshError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MeshError::File { path, why } => write!(f, "the file {path} could not be used: {why}"),
            MeshError::UnknownKind { name } => write!(f, "{name} is not a kind of mesh file that is read: a name ending .glb, .obj or .ply is"),
            MeshError::TooMany { what: Count::Cells, found, most } => write!(f, "the mesh is too large to build at this size: it would span {found} cells and the limit is {most}; make it smaller or its bricks larger"),
            MeshError::TooMany { what, found, most } => write!(f, "the file has {found} {}, and the most that are read are {most}", what.name()),
            MeshError::CutShort { part, needs, has } => write!(f, "the file is cut short: its {part} needs {needs} bytes and there are {has}"),
            MeshError::Malformed { kind, why } => write!(f, "the file is not a well-formed {kind} file: {why}"),
            MeshError::Unsupported { what } => write!(f, "the file uses {what}, which is not read; save it again without"),
            MeshError::NotANumber { part, at } => write!(f, "the file's {part} holds something that is not a number, at {at}"),
            MeshError::NoSuchPoint { index, points } => write!(f, "a triangle of the mesh names point {index}, and it has {points}"),
            MeshError::TooDeep { most } => write!(f, "the file's nodes are nested more than {most} deep, or one of them is inside itself"),
            MeshError::Empty => write!(f, "the mesh has no triangle with any area, or no size"),
            MeshError::Uneven { part, found, points } => write!(f, "the mesh has {found} {part} for {points} points: one for each, or none"),
            MeshError::NotAnImport { name } => write!(f, "`{name}` is not the name of an imported mesh: a form may name only a file already in the imports folder, by the name it was given there"),
            MeshError::Missing { name } => write!(f, "the mesh {name} is not in the imports folder; open the file it came from again"),
            MeshError::NoFile => write!(f, "no mesh file has been opened for this form yet"),
            MeshError::NothingBuilt => write!(f, "nothing of the mesh is left to build at this size: make it larger, or drop fewer loose pieces"),
        }
    }
}

impl std::error::Error for MeshError {}

impl From<MeshError> for String {
    fn from(error: MeshError) -> String {
        error.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One of every variant, so that a variant added without its wording
    /// fails to compile in `Display` and one added without a line here is
    /// seen in review: the list is the checklist the tests elsewhere go by
    /// (each variant is raised by a real failure in a test named for it;
    /// see `mesh::tests`, `mesh::form::tests` and the editor's `import`).
    #[test]
    fn every_failure_says_what_it_is_in_words_of_its_own() {
        let all = [
            MeshError::File { path: "a.glb".into(), why: "not found".into() },
            MeshError::UnknownKind { name: "a.stl".into() },
            MeshError::TooMany { what: Count::Points, found: 9, most: 3 },
            MeshError::TooMany { what: Count::Cells, found: 9, most: 3 },
            MeshError::CutShort { part: "points", needs: 12, has: 4 },
            MeshError::Malformed { kind: "glTF", why: "no JSON".into() },
            MeshError::Unsupported { what: "Draco compression" },
            MeshError::NotANumber { part: "points", at: 7 },
            MeshError::NoSuchPoint { index: 5, points: 3 },
            MeshError::TooDeep { most: 32 },
            MeshError::Empty,
            MeshError::Uneven { part: "colours", found: 1, points: 3 },
            MeshError::NotAnImport { name: "../x".into() },
            MeshError::Missing { name: "0123456789abcdef.glb".into() },
            MeshError::NoFile,
            MeshError::NothingBuilt,
        ];
        let said: Vec<String> = all.iter().map(|e| e.to_string()).collect();
        for (n, text) in said.iter().enumerate() {
            assert!(text.len() > 20 && !text.ends_with('.'), "{:?} says `{text}`", all[n]);
            assert!(said.iter().filter(|other| *other == text).count() == 1, "two failures say `{text}`");
            assert_eq!(String::from(all[n].clone()), *text);
        }
        // the numbers a failure carries are in what it says
        assert!(said[2].contains('9') && said[2].contains('3') && said[2].contains("points"));
        assert!(said[4].contains("12") && said[4].contains('4'));
        assert!(said[8].contains('5') && said[12].contains("../x"));
    }
}
