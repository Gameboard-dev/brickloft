//! `serve`: the same commands, asked a line at a time on standard input,
//! with the models kept loaded between them.
//!
//! Started for one command, this program spends its first seconds loading
//! weights from the disk and getting the graphics backend ready, every time
//! (docs/generate.md has the times). A person who looks at four pictures and
//! asks for four more, or tries a second picture's mesh, pays that again for
//! each. Kept running, it pays once.
//!
//! A request is one line of JSON with a `command` and that command's
//! options under the names the command line uses, without the dashes:
//!
//! ```text
//! {"command":"picture","prompt":"an oak barrel","seed":7,"count":4,"out":"C:\\...\\pictures"}
//! {"command":"mesh","picture":"C:\\...\\candidate-7.png","out":"C:\\...\\barrel.glb"}
//! ```
//!
//! Each is answered as the command line would be: progress, then one
//! `result` or one `error`. Requests are done one at a time, in the order
//! they arrive. The end of standard input ends the program; if it comes
//! while a request is being done, that request is cancelled, with its
//! part-files removed, exactly as `--watch-stdin` does for a single command.
//!
//! A line is whatever the other end sent, so it is bounded in length and
//! read against a list of the names each command takes: a name that is not
//! on it is a mistake to be told about, never passed over.

use crate::args::{self, Command, DEFAULT_RESOLUTION, DEFAULT_STEPS, MAX_COUNT, RESOLUTIONS, STEPS};
use crate::error::{Error, Result};
use crate::files;
use crate::protocol::Out;
use crate::Session;
use serde_json::{json, Map, Value};
use std::io::{BufRead, Read};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};

/// The longest request. A prompt is 500 characters and a path a few hundred.
pub const MAX_REQUEST: usize = 16 << 10;

fn bad(why: impl Into<String>) -> Error {
    Error::BadRequest { why: why.into() }
}

/// A request's fields, each taken once, so that what is left over can be refused.
struct Fields(Map<String, Value>);

impl Fields {
    fn text(&mut self, name: &str) -> Result<Option<String>> {
        match self.0.remove(name) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(text)) => Ok(Some(text)),
            Some(_) => Err(bad(format!("\"{name}\" must be text"))),
        }
    }

    fn needed(&mut self, name: &str) -> Result<String> {
        self.text(name)?.ok_or_else(|| bad(format!("\"{name}\" is missing")))
    }

    fn flag(&mut self, name: &str, default: bool) -> Result<bool> {
        match self.0.remove(name) {
            None | Some(Value::Null) => Ok(default),
            Some(Value::Bool(flag)) => Ok(flag),
            Some(_) => Err(bad(format!("\"{name}\" must be true or false"))),
        }
    }

    fn number(&mut self, name: &str, range: std::ops::RangeInclusive<u32>, default: u32) -> Result<u32> {
        match self.0.remove(name) {
            None | Some(Value::Null) => Ok(default),
            Some(value) => value.as_u64().and_then(|n| u32::try_from(n).ok()).filter(|n| range.contains(n)).ok_or_else(|| bad(format!("\"{name}\" must be a whole number from {} to {}", range.start(), range.end()))),
        }
    }

    fn done(self) -> Result<()> {
        match self.0.keys().next() {
            Some(stray) => Err(bad(format!("\"{}\" is not something this command takes", stray.chars().take(40).collect::<String>()))),
            None => Ok(()),
        }
    }
}

/// What a line asks for. `None` for `quit`.
pub fn request(line: &str) -> Result<Option<Command>> {
    let Value::Object(fields) = serde_json::from_str::<Value>(line).map_err(|e| bad(format!("it is not JSON ({e})")))? else { return Err(bad("it is not a JSON object")) };
    let mut fields = Fields(fields);
    let name = fields.needed("command")?;
    let command = match name.as_str() {
        "quit" => None,
        "help" => Some(Command::Help),
        "check" => Some(Command::Check { verify: fields.flag("verify", false)? }),
        "fetch" => Some(Command::Fetch { what: fields.needed("what")?, accept_licence: fields.flag("accept_licence", false)? }),
        "picture" => Some(Command::Picture {
            prompt: args::prompt(&fields.needed("prompt")?)?,
            seed: fields.number("seed", 0..=u32::MAX, 0)?,
            count: fields.number("count", 1..=MAX_COUNT, 1)?,
            steps: fields.number("steps", STEPS, DEFAULT_STEPS)?,
            out: PathBuf::from(fields.needed("out")?),
        }),
        "mesh" => {
            let command = Command::Mesh {
                picture: PathBuf::from(fields.needed("picture")?),
                out: PathBuf::from(fields.needed("out")?),
                cutout: fields.flag("cutout", true)?,
                level: fields.flag("level", true)?,
                resolution: fields.number("resolution", RESOLUTIONS, DEFAULT_RESOLUTION)?,
            };
            if !matches!(&command, Command::Mesh { out, .. } if out.extension().is_some_and(|e| e.eq_ignore_ascii_case("glb"))) {
                return Err(bad("\"out\" must be a .glb file"));
            }
            Some(command)
        }
        other => return Err(bad(format!("\"{}\" is not a command", other.chars().take(40).collect::<String>()))),
    };
    fields.done()?;
    Ok(command)
}

/// One line of at most `MAX_REQUEST` bytes, without its line break. `None`
/// at the end of input. A longer line is read to its end and thrown away,
/// and comes back as an error, so that its tail is not taken for a request.
pub fn line_from(input: &mut impl BufRead) -> Option<Result<String>> {
    let mut line = Vec::new();
    match input.by_ref().take(MAX_REQUEST as u64 + 1).read_until(b'\n', &mut line) {
        Ok(0) | Err(_) => return None,
        Ok(_) => {}
    }
    if line.len() > MAX_REQUEST && line.last() != Some(&b'\n') {
        let mut rest = Vec::new();
        loop {
            rest.clear();
            match input.by_ref().take(MAX_REQUEST as u64).read_until(b'\n', &mut rest) {
                Ok(0) | Err(_) => break,
                Ok(_) if rest.last() == Some(&b'\n') => break,
                Ok(_) => {}
            }
        }
        return Some(Err(bad(format!("it is longer than {MAX_REQUEST} bytes"))));
    }
    Some(String::from_utf8(line).map(|l| l.trim_end_matches(['\n', '\r']).to_string()).map_err(|_| bad("it is not Unicode text")))
}

pub fn serve(session: &mut Session, out: Arc<Out>) -> Result<()> {
    let busy = Arc::new(AtomicBool::new(false));
    let (send, receive) = mpsc::channel::<Result<String>>();
    {
        // Standard input is read on a thread of its own, so that its end is seen while a request is being done.
        let (busy, out) = (busy.clone(), out.clone());
        std::thread::spawn(move || {
            let mut input = std::io::stdin().lock();
            while let Some(line) = line_from(&mut input) {
                if send.send(line).is_err() {
                    return;
                }
            }
            if busy.load(Ordering::SeqCst) {
                files::cancel(&out);
            }
            // otherwise dropping `send` ends the loop below
        });
    }
    out.result("serve", json!({ "ready": true }));
    for line in receive {
        if line.as_ref().is_ok_and(|line| line.trim().is_empty()) {
            continue;
        }
        let command = match line.and_then(|line| request(&line)) {
            Ok(Some(command)) => command,
            Ok(None) => break,
            Err(error) => {
                out.error(&error);
                continue;
            }
        };
        busy.store(true, Ordering::SeqCst);
        // an error has been said by `answer`; the next request is another matter
        let _ = session.answer(&command, &out);
        busy.store(false, Ordering::SeqCst);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_is_read_as_the_command_line_would_be() {
        let hostile = "a barrel\"; & del /q * ; $(rm -rf ~)";
        let line = json!({ "command": "picture", "prompt": hostile, "seed": 7, "count": 4, "out": "pictures" }).to_string();
        assert_eq!(request(&line).unwrap(), Some(Command::Picture { prompt: hostile.into(), seed: 7, count: 4, steps: DEFAULT_STEPS, out: "pictures".into() }));
        assert_eq!(request(r#"{"command":"mesh","picture":"a.png","out":"a.glb","level":false}"#).unwrap(), Some(Command::Mesh { picture: "a.png".into(), out: "a.glb".into(), cutout: true, level: false, resolution: DEFAULT_RESOLUTION }));
        assert_eq!(request(r#"{"command":"check"}"#).unwrap(), Some(Command::Check { verify: false }));
        assert_eq!(request(r#"{"command":"quit"}"#).unwrap(), None);
    }

    #[test]
    fn what_is_wrong_with_a_request_is_said() {
        let why = |line: &str| match request(line) {
            Err(Error::BadRequest { why }) => why,
            other => panic!("expected a bad request, got {other:?}"),
        };
        assert!(why("not json").starts_with("it is not JSON"));
        assert_eq!(why("[1]"), "it is not a JSON object");
        assert_eq!(why("{}"), "\"command\" is missing");
        assert_eq!(why(r#"{"command":"paint"}"#), "\"paint\" is not a command");
        assert_eq!(why(r#"{"command":"picture","prompt":"x","out":"p","count":9}"#), "\"count\" must be a whole number from 1 to 8");
        assert_eq!(why(r#"{"command":"picture","prompt":"x","out":"p","seed":-1}"#), "\"seed\" must be a whole number from 0 to 4294967295");
        assert_eq!(why(r#"{"command":"picture","prompt":7,"out":"p"}"#), "\"prompt\" must be text");
        assert_eq!(why(r#"{"command":"mesh","picture":"a.png","out":"a.obj"}"#), "\"out\" must be a .glb file");
        // a name the command does not take: an address for `fetch`, for one
        assert_eq!(why(r#"{"command":"fetch","what":"sd15","url":"https://example.com/x"}"#), "\"url\" is not something this command takes");
        assert_eq!(why(r#"{"command":"check","weights":"elsewhere"}"#), "\"weights\" is not something this command takes");
        // the prompt's bound is the command line's
        let long = json!({ "command": "picture", "prompt": "a".repeat(501), "out": "p" }).to_string();
        assert!(matches!(request(&long), Err(Error::PromptTooLong { chars: 501 })));
    }

    #[test]
    fn a_line_too_long_is_refused_whole_and_the_next_is_read() {
        let mut input = Vec::new();
        input.extend_from_slice(b"{\"command\":\"check\"}\r\n");
        input.extend(std::iter::repeat(b'x').take(3 * MAX_REQUEST + 5));
        input.extend_from_slice(b"\n{\"command\":\"quit\"}\n");
        let mut input = std::io::Cursor::new(input);
        assert_eq!(line_from(&mut input).unwrap().unwrap(), "{\"command\":\"check\"}");
        assert!(matches!(line_from(&mut input), Some(Err(Error::BadRequest { .. }))));
        assert_eq!(line_from(&mut input).unwrap().unwrap(), "{\"command\":\"quit\"}", "the long line's tail was not taken for a request");
        assert!(line_from(&mut input).is_none());
        // exactly the most is allowed
        let mut just = vec![b'y'; MAX_REQUEST - 1];
        just.push(b'\n');
        assert_eq!(line_from(&mut std::io::Cursor::new(just)).unwrap().unwrap().len(), MAX_REQUEST - 1);
    }
}
