//! Every way this program can fail, and what it says when it does.
//!
//! One enum and one `Display` for the whole program. Each module returns
//! `Result<_, Error>` and builds a variant from what it knows (a path, a
//! size, the library's own words); the sentence a person reads is written
//! here and nowhere else. The other way, a `String` made where the failure
//! happens, puts the wording in a dozen places, lets two modules say the
//! same thing differently, and gives the editor nothing to tell failures
//! apart by but the text.
//!
//! Each sentence says what failed, where, why, and what to do about it
//! where there is something to do. The editor shows it as it is. `code`
//! is the name the editor may act on (offer the download for
//! `weights_missing`, for instance); it is part of the protocol
//! (docs/generate.md) and a sentence may be reworded without changing it.

use std::fmt;
use std::path::PathBuf;

/// The most characters a prompt may have. Stable Diffusion's text encoder
/// reads 77 word-pieces, which is 300 or so characters of English, so more
/// than this could not be used; the bound is on characters because that is
/// what a person typing can count.
pub const MAX_PROMPT_CHARS: usize = 500;

#[derive(Debug)]
pub enum Error {
    /// The arguments could not be understood. `why` is one of this
    /// program's own sentences (see `args`).
    Usage { why: String },
    PromptEmpty,
    PromptTooLong { chars: usize },
    /// The prompt is within the bound on characters and still more than the
    /// text encoder reads.
    PromptTooManyPieces { pieces: usize, most: usize },
    /// `fetch` was asked for something that is not on its list.
    NotOnList { asked: String },
    LicenceNotAccepted { model: &'static str, licence: &'static str },
    /// A file or folder could not be read, written, made or renamed.
    Io { doing: &'static str, path: PathBuf, source: std::io::Error },
    WeightsMissing { model: &'static str, file: PathBuf },
    /// A weights file is there and is not the file this program fetches.
    WeightsWrongSize { file: PathBuf, expected: u64, found: u64 },
    HashMismatch { file: String, expected: &'static str, found: String },
    /// The server answered, with something other than the file.
    Download { file: String, why: String },
    /// The server could not be reached or the connection broke.
    Network { file: String, why: String },
    /// A redirect led somewhere this program does not download from.
    RedirectRefused { file: String, to: String },
    PictureUnreadable { path: PathBuf, why: String },
    PictureTooLarge { path: PathBuf, width: u32, height: u32, most: u32 },
    PictureFileTooLarge { path: PathBuf, bytes: u64, most: u64 },
    /// `--no-cutout` was given for a picture in which nothing is see-through.
    NoTransparency { path: PathBuf },
    /// The cut-out took the whole picture for ground, or left a speck.
    NothingToCutOut { path: PathBuf },
    /// The model gave a mesh with nothing in it.
    EmptyMesh,
    NoDevice { asked: &'static str, why: String },
    NotEnoughMemory { step: &'static str, needs_mib: u64, free_mib: u64 },
    /// A model failed while loading or running. `why` is the library's own
    /// message, which is the only account of it there is.
    Model { step: &'static str, why: String },
    /// A line sent to `serve` was not a request it knows.
    BadRequest { why: String },
    /// Stdin was closed while `--watch-stdin` was given.
    Cancelled,
}

impl Error {
    /// The failure's name in the protocol.
    pub fn code(&self) -> &'static str {
        match self {
            Error::Usage { .. } => "usage",
            Error::PromptEmpty => "prompt_empty",
            Error::PromptTooLong { .. } | Error::PromptTooManyPieces { .. } => "prompt_too_long",
            Error::NotOnList { .. } => "not_on_list",
            Error::LicenceNotAccepted { .. } => "licence_not_accepted",
            Error::Io { .. } => "io",
            Error::WeightsMissing { .. } => "weights_missing",
            Error::WeightsWrongSize { .. } => "weights_damaged",
            Error::HashMismatch { .. } => "hash_mismatch",
            Error::Download { .. } => "download_failed",
            Error::Network { .. } => "network",
            Error::RedirectRefused { .. } => "redirect_refused",
            Error::PictureUnreadable { .. } => "picture_unreadable",
            Error::PictureTooLarge { .. } | Error::PictureFileTooLarge { .. } => "picture_too_large",
            Error::NoTransparency { .. } => "no_transparency",
            Error::NothingToCutOut { .. } => "nothing_to_cut_out",
            Error::EmptyMesh => "empty_mesh",
            Error::NoDevice { .. } => "no_device",
            Error::NotEnoughMemory { .. } => "not_enough_memory",
            Error::Model { .. } => "model_failed",
            Error::BadRequest { .. } => "bad_request",
            Error::Cancelled => "cancelled",
        }
    }

    /// What the program ends with: 2 for arguments it could not understand,
    /// 3 for a run that was cancelled, 1 for everything else.
    pub fn exit_code(&self) -> u8 {
        match self {
            Error::Usage { .. } => 2,
            Error::Cancelled => 3,
            _ => 1,
        }
    }
}

/// Megabytes as gigabytes to one place, the way a person says them.
pub fn gigabytes(mib: u64) -> String {
    format!("{:.1} GB", mib as f64 / 1024.0)
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Usage { why } => write!(f, "{why}. Run `brickloft-generate help` for the commands"),
            Error::PromptEmpty => write!(f, "the prompt is empty: say what to make, such as \"an oak barrel\""),
            Error::PromptTooLong { chars } => write!(f, "the prompt is {chars} characters long and the most is {MAX_PROMPT_CHARS}: shorten it"),
            Error::PromptTooManyPieces { pieces, most } => write!(f, "the prompt comes to {pieces} word-pieces and the model reads {most}: shorten it"),
            Error::NotOnList { asked } => write!(f, "\"{asked}\" is not something this program fetches. It fetches: {}", crate::models::names()),
            Error::LicenceNotAccepted { model, licence } => write!(f, "the weights for {model} are under the licence \"{licence}\", shown above: to download them, run this again with --accept-licence"),
            Error::Io { doing, path, source } => write!(f, "could not {doing} {}: {source}", path.display()),
            Error::WeightsMissing { model, file } => write!(f, "the weights for {model} are not there ({} is missing): fetch them with `brickloft-generate fetch {model}`", file.display()),
            Error::WeightsWrongSize { file, expected, found } => write!(f, "{} is {found} bytes and should be {expected}: it is damaged or is another file. Delete it and fetch the weights again", file.display()),
            Error::HashMismatch { file, expected, found } => write!(f, "{file} is not the file that was expected (its SHA-256 is {found} and should be {expected}), so it was not kept. Try again; if it happens twice the file at the source has changed, and this program needs updating"),
            Error::Download { file, why } => write!(f, "could not download {file}: {why}"),
            Error::Network { file, why } => write!(f, "could not download {file}: {why}. Check the connection and run the same command again: it carries on from where it stopped"),
            Error::RedirectRefused { file, to } => write!(f, "could not download {file}: the server sent this program on to {to}, which is not a place it downloads from"),
            Error::PictureUnreadable { path, why } => write!(f, "could not read the picture {}: {why}. PNG, JPEG and WebP are read", path.display()),
            Error::PictureTooLarge { path, width, height, most } => write!(f, "the picture {} is {width} by {height} pixels and the most is {most} a side: make it smaller", path.display()),
            Error::PictureFileTooLarge { path, bytes, most } => write!(f, "the picture {} is {} MB and the most is {} MB: make it smaller", path.display(), bytes >> 20, most >> 20),
            Error::NoTransparency { path } => write!(f, "--no-cutout was given, and nothing in the picture {} is see-through: leave --no-cutout out to have the ground removed", path.display()),
            Error::NothingToCutOut { path } => write!(f, "no object was found in the picture {}: it needs one object on a plain ground that differs from it in colour", path.display()),
            Error::EmptyMesh => write!(f, "the model made nothing from this picture: try another picture, with one whole object on a plain ground"),
            Error::NoDevice { asked, why } => write!(f, "no graphics device could be used ({asked}): {why}"),
            Error::NotEnoughMemory { step, needs_mib, free_mib } => write!(f, "{step} needs about {} of free graphics memory and there is {}: close other programs that use the graphics card", gigabytes(*needs_mib), gigabytes(*free_mib)),
            Error::Model { step, why } => write!(f, "{step} failed: {why}"),
            Error::BadRequest { why } => write!(f, "the request was not understood: {why}"),
            Error::Cancelled => write!(f, "cancelled"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
