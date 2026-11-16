//! `brickloft-generate`: a mesh from a picture, and a picture from a prompt,
//! with models that run on this computer.
//!
//! A program of its own and not part of the editor, because of what it is
//! built on: Candle and Lux3D are four hundred crates, ten minutes to
//! compile, and a C++ toolchain to compile them with, where the editor's rule
//! is few dependencies and a build in under a minute. The editor starts this
//! program with a list of arguments and reads what it prints
//! (docs/generate.md has the protocol); a person can do the same from a
//! command line.
//!
//! | Module | What it is |
//! |---|---|
//! | `args` | the command line |
//! | `protocol` | the JSON lines it answers in |
//! | `error` | every failure and its wording |
//! | `models` | what can be fetched: sources, hashes, licences |
//! | `fetch` | the download |
//! | `check` | what this computer can do |
//! | `device` | the graphics device and its memory |
//! | `picture` | prompt to picture (Stable Diffusion 1.5) |
//! | `cutout`, `rank` | the object cut from its ground; candidates judged |
//! | `mesh`, `level`, `glb` | picture to mesh (TripoSR); the mesh tidied and levelled; the file it is written to |
//! | `serve` | the same commands from standard input, with the models kept loaded |
//! | `files`, `places` | files written whole or not at all; where weights are kept |

pub mod args;
pub mod check;
pub mod cutout;
pub mod device;
pub mod error;
pub mod fetch;
pub mod files;
pub mod glb;
pub mod level;
pub mod mesh;
pub mod models;
pub mod picture;
pub mod places;
pub mod protocol;
pub mod rank;
pub mod serve;

use args::{Command, Want};
use error::{Error, Result};
use protocol::Out;
use serde_json::{json, Value};
use std::path::PathBuf;

/// What a run holds on to: the device once it is open, and the models once
/// they are loaded. One command uses it once; `serve` uses it for as long as
/// it runs.
pub struct Session {
    pub weights: PathBuf,
    pub want: Want,
    chosen: Option<device::Chosen>,
    painter: Option<picture::Painter>,
    sculptor: Option<mesh::Sculptor>,
}

impl Session {
    pub fn new(weights: PathBuf, want: Want) -> Session {
        Session { weights, want, chosen: None, painter: None, sculptor: None }
    }

    /// Opens the device if it is not open, and checks that it has the memory for `step`.
    fn ready_for(&mut self, step: device::Step, loaded: bool) -> Result<()> {
        if self.chosen.is_none() {
            self.chosen = Some(device::choose(self.want)?);
        }
        let Some(chosen) = &self.chosen else { return Ok(()) };
        // A model that is already loaded has its memory: what is free now says nothing about it.
        if !loaded {
            device::enough(step, chosen.kind, device::memory(chosen.name.as_deref()).as_ref())?;
        }
        Ok(())
    }

    /// Runs one command and gives its name and its result's fields.
    pub fn run(&mut self, command: &Command, out: &Out) -> Result<(&'static str, Value)> {
        match command {
            Command::Help => Ok(("help", json!({ "text": include_str!("help.txt") }))),
            Command::Check { verify } => Ok(("check", check::check(&self.weights, self.want, *verify, out)?)),
            Command::Fetch { what, accept_licence } => {
                let model = models::named(what).ok_or_else(|| Error::NotOnList { asked: what.chars().take(60).collect() })?;
                Ok(("fetch", fetch::fetch(model, &self.weights, *accept_licence, &mut fetch::Https::new(), out)?))
            }
            Command::Picture { prompt, seed, count, steps, out: folder } => {
                // the weights before the device: "fetch them" is the more useful thing to be told first
                picture::weights_in(&self.weights)?;
                // The two models do not fit an 8 GB card together, so the one not in use is let go first.
                self.sculptor = None;
                self.ready_for(device::Step::Picture, self.painter.is_some())?;
                if self.painter.is_none() {
                    let chosen = self.chosen.as_ref().ok_or(Error::NoDevice { asked: "any", why: "no device is open".into() })?;
                    self.painter = Some(picture::Painter::load(&self.weights, chosen, out)?);
                }
                let painter = self.painter.as_ref().ok_or(Error::Model { step: "loading the model", why: "it is not loaded".into() })?;
                Ok(("picture", painter.candidates(prompt, *seed, *count, *steps, folder, out)?))
            }
            Command::Mesh { picture, out: to, cutout, level, resolution } => {
                mesh::weights_in(&self.weights)?;
                // The picture is read before the model is loaded: a file that is not a picture is found out in a moment.
                mesh::prepared(&mesh::read_picture(picture)?, picture, *cutout)?;
                self.painter = None;
                self.ready_for(device::Step::Mesh, self.sculptor.is_some())?;
                if self.sculptor.is_none() {
                    let chosen = self.chosen.as_ref().ok_or(Error::NoDevice { asked: "any", why: "no device is open".into() })?;
                    self.sculptor = Some(mesh::Sculptor::load(&self.weights, chosen, out)?);
                }
                let sculptor = self.sculptor.as_ref().ok_or(Error::Model { step: "loading the model", why: "it is not loaded".into() })?;
                Ok(("mesh", sculptor.mesh(picture, to, *cutout, &mesh::Tidy { level: *level, seen_from: None }, *resolution, out)?))
            }
            Command::Serve => Err(Error::Usage { why: "serve cannot be asked of a program that is already serving".into() }),
        }
    }

    /// Runs one command and says how it ended: one `result` line or one
    /// `error` line. A panic in a library is caught and said as an error like
    /// any other, so that the reader is never left without an ending.
    pub fn answer(&mut self, command: &Command, out: &Out) -> std::result::Result<(), Error> {
        let ran = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.run(command, out)));
        let ran = match ran {
            Ok(ran) => ran,
            Err(panic) => {
                // what was loaded may be half-made: nothing of it is used again
                self.painter = None;
                self.sculptor = None;
                Err(Error::Model { step: "the program", why: device::panic_text(panic) })
            }
        };
        match ran {
            Ok((name, fields)) => {
                out.result(name, fields);
                Ok(())
            }
            Err(error) => {
                out.error(&error);
                Err(error)
            }
        }
    }
}

/// The weights folder: the one given, or the default.
pub fn weights_folder(given: Option<PathBuf>) -> Result<PathBuf> {
    match given {
        Some(folder) => Ok(folder),
        None => places::weights().map_err(|why| Error::Usage { why: format!("there is no default folder for the weights ({why}): give one with --weights") }),
    }
}
