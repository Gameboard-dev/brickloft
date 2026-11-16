//! What this program says: one JSON object to a line on standard output,
//! and nothing else there.
//!
//! The editor starts this program and reads its output a line at a time, so
//! a line is the unit: it is written whole and flushed, and each is an object
//! with an `event` that says what kind it is.
//!
//! | `event` | when | fields |
//! |---|---|---|
//! | `progress` | now and then while a command runs | `stage`, `done`, `of`, and what else the stage has to say |
//! | `licence` | from `fetch`, before anything is downloaded | the model, its licence's name, link and restrictions |
//! | `result` | once, when a command has worked | `command`, `protocol`, and the command's own fields |
//! | `error` | once, when it has not | `code` (see `Error::code`) and `message`, a sentence for a person |
//!
//! A run ends with exactly one `result` or one `error`. docs/generate.md
//! has every field of every command.
//!
//! JSON lines and not one JSON document at the end: progress has to arrive
//! while the work is going on, and a reader of lines needs no parser that can
//! stop half-way through a document. The libraries underneath write what
//! they have to say to standard error, which the editor may keep for a report
//! and never reads as an answer.
//! https://jsonlines.org

use crate::error::Error;
use serde_json::{json, Map, Value};
use std::io::Write;
use std::sync::Mutex;

/// The protocol's number, in every result. It goes up when a field changes
/// its meaning or is taken away; a new field does not change it.
pub const PROTOCOL: u32 = 1;

/// Where the lines go. A writer and not standard output itself so that a
/// test can read what was said.
pub struct Out {
    to: Mutex<Box<dyn Write + Send>>,
}

impl Out {
    pub fn stdout() -> Out {
        Out { to: Mutex::new(Box::new(std::io::stdout())) }
    }

    pub fn to(writer: Box<dyn Write + Send>) -> Out {
        Out { to: Mutex::new(writer) }
    }

    /// One line. A failure to write is not reported anywhere: the only
    /// reader has gone, and with `--watch-stdin` the run is about to be
    /// stopped for the same reason.
    fn line(&self, value: &Value) {
        if let Ok(mut to) = self.to.lock() {
            // `Value`'s own writer puts no line break inside an object: one in a string is written as \n
            let _ = writeln!(to, "{value}");
            let _ = to.flush();
        }
    }

    fn event(&self, event: &str, fields: Value) {
        let mut object = Map::new();
        object.insert("event".into(), event.into());
        if let Value::Object(fields) = fields {
            object.extend(fields);
        }
        self.line(&Value::Object(object));
    }

    /// `done` of `of` in `stage`, with anything more the stage has to say.
    pub fn progress(&self, stage: &str, done: u64, of: u64, more: Value) {
        let mut fields = json!({ "stage": stage, "done": done, "of": of });
        if let (Value::Object(fields), Value::Object(more)) = (&mut fields, more) {
            fields.extend(more);
        }
        self.event("progress", fields);
    }

    pub fn licence(&self, fields: Value) {
        self.event("licence", fields);
    }

    pub fn result(&self, command: &str, fields: Value) {
        let mut all = json!({ "command": command, "protocol": PROTOCOL });
        if let (Value::Object(all), Value::Object(fields)) = (&mut all, fields) {
            all.extend(fields);
        }
        self.event("result", all);
    }

    pub fn error(&self, error: &Error) {
        self.event("error", json!({ "code": error.code(), "message": error.to_string() }));
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::sync::Arc;

    /// A writer whose bytes can be read back.
    #[derive(Clone, Default)]
    pub struct Kept(pub Arc<Mutex<Vec<u8>>>);

    impl Write for Kept {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Kept {
        /// The lines written, each read back as JSON. Fails if any line is not an object.
        pub fn lines(&self) -> Vec<Value> {
            let bytes = self.0.lock().unwrap().clone();
            String::from_utf8(bytes).unwrap().lines().map(|line| {
                let value: Value = serde_json::from_str(line).unwrap_or_else(|e| panic!("not JSON ({e}): {line}"));
                assert!(value.is_object(), "not an object: {line}");
                value
            }).collect()
        }
    }

    pub fn kept() -> (Out, Kept) {
        let kept = Kept::default();
        (Out::to(Box::new(kept.clone())), kept)
    }

    #[test]
    fn every_line_is_one_object_with_an_event() {
        let (out, kept) = kept();
        out.progress("download", 3, 10, json!({ "file": "a\nb\"c" }));
        out.licence(json!({ "model": "sd15" }));
        out.result("check", json!({ "device": null }));
        out.error(&Error::PromptEmpty);
        let lines = kept.lines();
        assert_eq!(lines.len(), 4, "a line break inside a string must not make a new line");
        assert_eq!(lines[0], json!({ "event": "progress", "stage": "download", "done": 3, "of": 10, "file": "a\nb\"c" }));
        assert_eq!(lines[1]["event"], "licence");
        assert_eq!(lines[2], json!({ "event": "result", "command": "check", "protocol": PROTOCOL, "device": null }));
        assert_eq!(lines[3]["event"], "error");
        assert_eq!(lines[3]["code"], "prompt_empty");
        assert_eq!(lines[3]["message"], Error::PromptEmpty.to_string());
    }
}
