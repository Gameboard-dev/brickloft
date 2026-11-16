//! The command line, read by hand.
//!
//! Five commands with a handful of options each. `clap` would read them in
//! fewer lines; it would also write its own messages to standard error and
//! end the program itself when an argument is wrong, where this program's
//! rule is one `error` line on standard output with wording from
//! `error.rs`. Getting that out of clap means turning its errors back into
//! ours one kind at a time, which is no shorter than reading the arguments.
//!
//! Arguments are taken as the system gives them (`args_os`), so a path
//! that is not Unicode still names its file. The prompt has to be Unicode,
//! since it is text for a model. Nothing here is ever passed to a shell or
//! made part of a command: a prompt is one string from the moment it
//! arrives to the moment it is cut into word-pieces.

use crate::error::{Error, Result, MAX_PROMPT_CHARS};
use std::ffi::OsString;
use std::path::PathBuf;

/// The most pictures one `picture` makes. Each is ten seconds or more.
pub const MAX_COUNT: u32 = 8;
/// The cells along each side of the grid the mesh is drawn from. 128 was
/// enough detail for bricks in the trial; 256 is the model's own default
/// and takes eight times the work.
pub const DEFAULT_RESOLUTION: u32 = 128;
pub const RESOLUTIONS: std::ops::RangeInclusive<u32> = 32..=256;
/// Steps of denoising for a picture. The trial's pictures were made at 25.
pub const DEFAULT_STEPS: u32 = 25;
pub const STEPS: std::ops::RangeInclusive<u32> = 1..=50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Want {
    /// CUDA if this program was built with it and it starts, then Vulkan.
    Auto,
    Cuda,
    Vulkan,
}

#[derive(Debug, PartialEq)]
pub enum Command {
    Help,
    Check { verify: bool },
    Fetch { what: String, accept_licence: bool },
    Picture { prompt: String, seed: u32, count: u32, steps: u32, out: PathBuf },
    Mesh { picture: PathBuf, out: PathBuf, cutout: bool, level: bool, resolution: u32 },
    Serve,
}

#[derive(Debug, PartialEq)]
pub struct Args {
    pub command: Command,
    /// The weights folder, if one was given (see `places`).
    pub weights: Option<PathBuf>,
    pub device: Want,
    pub watch_stdin: bool,
}

fn usage(why: impl Into<String>) -> Error {
    Error::Usage { why: why.into() }
}

/// A prompt as the model is given it: trimmed, with line breaks and other
/// control characters made into spaces, and within the bound.
pub fn prompt(text: &str) -> Result<String> {
    let clean: String = text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let clean = clean.trim();
    if clean.is_empty() {
        return Err(Error::PromptEmpty);
    }
    let chars = clean.chars().count();
    if chars > MAX_PROMPT_CHARS {
        return Err(Error::PromptTooLong { chars });
    }
    Ok(clean.to_string())
}

fn number(option: &str, value: &OsString, range: std::ops::RangeInclusive<u32>) -> Result<u32> {
    let text = value.to_string_lossy();
    match text.parse::<u32>() {
        Ok(n) if range.contains(&n) => Ok(n),
        _ => Err(usage(format!("{option} takes a whole number from {} to {}, and was given \"{text}\"", range.start(), range.end()))),
    }
}

/// Reads the arguments after the program's own name.
pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Args> {
    let mut args = args.into_iter();
    let command = args.next().ok_or_else(|| usage("no command was given"))?;
    let command = command.to_string_lossy().into_owned();

    // what the options said, before it is known whether the command takes them
    let (mut weights, mut device, mut watch_stdin) = (None, Want::Auto, false);
    let (mut verify, mut accept_licence, mut no_cutout, mut no_level) = (false, false, false, false);
    let (mut prompt_given, mut seed, mut count, mut steps, mut resolution) = (None, None, None, None, None);
    let (mut out, mut picture, mut what) = (None::<PathBuf>, None::<PathBuf>, None::<String>);
    let mut given: Vec<String> = Vec::new();

    while let Some(arg) = args.next() {
        let name = arg.to_string_lossy().into_owned();
        if !name.starts_with("--") {
            if what.is_some() {
                return Err(usage(format!("\"{name}\" was not expected")));
            }
            what = Some(name);
            continue;
        }
        let mut value = || args.next().ok_or_else(|| usage(format!("{name} needs a value after it")));
        match name.as_str() {
            "--weights" => weights = Some(PathBuf::from(value()?)),
            "--device" => {
                let v = value()?;
                device = match v.to_string_lossy().as_ref() {
                    "auto" => Want::Auto,
                    "cuda" => Want::Cuda,
                    "vulkan" => Want::Vulkan,
                    other => return Err(usage(format!("--device takes auto, cuda or vulkan, and was given \"{other}\""))),
                };
            }
            "--watch-stdin" => watch_stdin = true,
            "--verify" => verify = true,
            "--accept-licence" => accept_licence = true,
            "--no-cutout" => no_cutout = true,
            "--no-level" => no_level = true,
            "--prompt" => prompt_given = Some(value()?.into_string().map_err(|_| usage("the prompt is not Unicode text"))?),
            "--seed" => seed = Some(number("--seed", &value()?, 0..=u32::MAX)?),
            "--count" => count = Some(number("--count", &value()?, 1..=MAX_COUNT)?),
            "--steps" => steps = Some(number("--steps", &value()?, STEPS)?),
            "--resolution" => resolution = Some(number("--resolution", &value()?, RESOLUTIONS)?),
            "--out" => out = Some(PathBuf::from(value()?)),
            "--picture" => picture = Some(PathBuf::from(value()?)),
            _ => return Err(usage(format!("\"{name}\" is not an option"))),
        }
        given.push(name);
    }

    // an option that the command does not take is a mistake to be told about, not passed over
    let takes = |allowed: &[&str]| -> Result<()> {
        match given.iter().find(|g| !["--weights", "--device", "--watch-stdin"].contains(&g.as_str()) && !allowed.contains(&g.as_str())) {
            Some(stray) => Err(usage(format!("{command} does not take {stray}"))),
            None => Ok(()),
        }
    };
    let no_name = |what: &Option<String>| -> Result<()> {
        match what {
            Some(stray) => Err(usage(format!("\"{stray}\" was not expected"))),
            None => Ok(()),
        }
    };
    let needs = |option: &str| usage(format!("{command} needs {option}"));

    let command = match command.as_str() {
        "help" | "--help" | "-h" => Command::Help,
        "check" => {
            takes(&["--verify"])?;
            no_name(&what)?;
            Command::Check { verify }
        }
        "fetch" => {
            takes(&["--accept-licence"])?;
            Command::Fetch { what: what.ok_or_else(|| usage(format!("fetch needs the name of what to fetch: {}", crate::models::names())))?, accept_licence }
        }
        "picture" => {
            takes(&["--prompt", "--seed", "--count", "--steps", "--out"])?;
            no_name(&what)?;
            Command::Picture { prompt: prompt(&prompt_given.ok_or_else(|| needs("--prompt"))?)?, seed: seed.unwrap_or(0), count: count.unwrap_or(1), steps: steps.unwrap_or(DEFAULT_STEPS), out: out.ok_or_else(|| needs("--out, the folder for the pictures"))? }
        }
        "mesh" => {
            takes(&["--picture", "--out", "--no-cutout", "--no-level", "--resolution"])?;
            no_name(&what)?;
            let out = out.ok_or_else(|| needs("--out, the .glb file to write"))?;
            if !out.extension().is_some_and(|e| e.eq_ignore_ascii_case("glb")) {
                return Err(usage(format!("mesh writes a .glb file, and --out was given \"{}\"", out.display())));
            }
            Command::Mesh { picture: picture.ok_or_else(|| needs("--picture"))?, out, cutout: !no_cutout, level: !no_level, resolution: resolution.unwrap_or(DEFAULT_RESOLUTION) }
        }
        "serve" => {
            takes(&[])?;
            no_name(&what)?;
            Command::Serve
        }
        other => return Err(usage(format!("\"{other}\" is not a command"))),
    };
    Ok(Args { command, weights, device, watch_stdin })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(args: &[&str]) -> Result<Args> {
        parse(args.iter().map(OsString::from))
    }

    #[test]
    fn a_prompt_is_one_argument_whatever_is_in_it() {
        // quotes, ampersands, semicolons, a dollar and a backtick: all of it is the prompt, none of it is acted on
        let hostile = "a barrel\"; & del /q * ; $(rm -rf ~) `id` | more --out x";
        let args = parsed(&["picture", "--prompt", hostile, "--out", "pictures", "--seed", "7", "--count", "4"]).unwrap();
        assert_eq!(args.command, Command::Picture { prompt: hostile.to_string(), seed: 7, count: 4, steps: DEFAULT_STEPS, out: PathBuf::from("pictures") });
    }

    #[test]
    fn the_prompt_is_bounded() {
        let just = "a".repeat(MAX_PROMPT_CHARS);
        assert_eq!(prompt(&just).unwrap(), just);
        // counted in characters, not bytes: 500 of these are 1000 bytes
        assert!(prompt(&"é".repeat(MAX_PROMPT_CHARS)).is_ok());
        let over = "a".repeat(MAX_PROMPT_CHARS + 1);
        assert!(matches!(prompt(&over), Err(Error::PromptTooLong { chars }) if chars == MAX_PROMPT_CHARS + 1));
        assert!(matches!(parsed(&["picture", "--prompt", &over, "--out", "p"]), Err(Error::PromptTooLong { .. })));
        assert!(matches!(prompt("  \n\t "), Err(Error::PromptEmpty)));
        assert_eq!(prompt(" an oak\nbarrel\u{0} ").unwrap(), "an oak barrel");
    }

    #[test]
    fn mesh_and_its_options() {
        let args = parsed(&["mesh", "--picture", "in.png", "--out", "out.glb", "--weights", "w", "--device", "vulkan", "--watch-stdin"]).unwrap();
        assert_eq!(args.command, Command::Mesh { picture: "in.png".into(), out: "out.glb".into(), cutout: true, level: true, resolution: DEFAULT_RESOLUTION });
        assert_eq!((args.weights, args.device, args.watch_stdin), (Some(PathBuf::from("w")), Want::Vulkan, true));
        let args = parsed(&["mesh", "--no-cutout", "--no-level", "--resolution", "64", "--picture", "in.png", "--out", "out.GLB"]).unwrap();
        assert_eq!(args.command, Command::Mesh { picture: "in.png".into(), out: "out.GLB".into(), cutout: false, level: false, resolution: 64 });
    }

    #[test]
    fn what_is_wrong_is_said() {
        let why = |args: &[&str]| match parsed(args) {
            Err(Error::Usage { why }) => why,
            other => panic!("expected a usage error, got {other:?}"),
        };
        assert_eq!(why(&[]), "no command was given");
        assert_eq!(why(&["paint"]), "\"paint\" is not a command");
        assert_eq!(why(&["mesh", "--picture", "a.png", "--out", "a.obj"]), "mesh writes a .glb file, and --out was given \"a.obj\"");
        assert_eq!(why(&["mesh", "--out", "a.glb"]), "mesh needs --picture");
        assert_eq!(why(&["mesh", "--picture"]), "--picture needs a value after it");
        assert_eq!(why(&["picture", "--prompt", "x", "--out", "p", "--count", "9"]), "--count takes a whole number from 1 to 8, and was given \"9\"");
        assert_eq!(why(&["picture", "--prompt", "x", "--out", "p", "--seed", "-1"]), "--seed takes a whole number from 0 to 4294967295, and was given \"-1\"");
        assert_eq!(why(&["check", "--prompt", "x"]), "check does not take --prompt");
        assert_eq!(why(&["check", "--frobnicate"]), "\"--frobnicate\" is not an option");
        assert_eq!(why(&["check", "extra"]), "\"extra\" was not expected");
        assert_eq!(why(&["fetch"]), "fetch needs the name of what to fetch: triposr, sd15");
        assert_eq!(why(&["check", "--device", "cpu"]), "--device takes auto, cuda or vulkan, and was given \"cpu\"");
    }

    #[test]
    fn fetch_takes_a_name_and_no_address() {
        assert_eq!(parsed(&["fetch", "sd15"]).unwrap().command, Command::Fetch { what: "sd15".into(), accept_licence: false });
        assert_eq!(parsed(&["fetch", "triposr", "--accept-licence"]).unwrap().command, Command::Fetch { what: "triposr".into(), accept_licence: true });
        // there is no option by which a repository, a revision or an address could be given
        for stray in ["--url", "--repo", "--revision", "--sha256"] {
            assert!(matches!(parsed(&["fetch", "sd15", stray, "x"]), Err(Error::Usage { .. })));
        }
    }
}
