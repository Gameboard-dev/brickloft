//! A record of a crash, written to a file.
//!
//! The editor is a window with no console, so what Rust prints when it
//! panics (what went wrong, and where) goes nowhere and a crash is the
//! window vanishing. That message is the one thing needed to mend the
//! fault, and a player cannot be asked to start the editor from a terminal
//! and make it happen again. So it is written to a file beside the reports,
//! where the player can find it and send it.
//!
//! A panic hook does this: the function Rust calls first when anything
//! panics, on whichever thread.
//! https://doc.rust-lang.org/std/panic/fn.set_hook.html
//! The hook Rust had is called afterwards, so a run from a terminal still
//! prints as it always did.
//!
//! A crash reporter that sends the record somewhere was not considered:
//! the editor sends nothing anywhere, and a file the player chooses to
//! pass on keeps it so.

use std::io::Write;

/// The file's name, in the folder for files that need not be kept.
pub const FILE: &str = "brickloft-crash.txt";

/// The most the file is let grow to. Past it the file is started afresh: a
/// fault that panics every frame would otherwise fill the disk.
const MOST_BYTES: u64 = 1 << 20;

/// What is written for one panic: when, which version, what it said, where,
/// and how the program came to be there.
fn record(message: &str, at: &str, trace: &str) -> String {
    let when = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.as_secs());
    format!("# brickloft {} crashed, {when} seconds after 1970\n{message}\nat {at}\n{trace}\n\n", env!("CARGO_PKG_VERSION"))
}

/// From now on, write every panic to [`FILE`] as well as wherever Rust
/// writes it. Nothing is done about a file that cannot be written: the
/// program is already failing, and there is nowhere left to say so.
pub fn log_to_file() {
    let before = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic| {
        // (a panic's payload is a `&str` or a `String`, as `panic!` makes it)
        let message = panic.payload().downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| panic.payload().downcast_ref::<String>().cloned()).unwrap_or_else(|| "a panic with no message".into());
        let at = panic.location().map_or("an unknown place".into(), |l| format!("{}:{}:{}", l.file(), l.line(), l.column()));
        // (captured whether or not RUST_BACKTRACE is set: a player has not set it)
        let trace = std::backtrace::Backtrace::force_capture().to_string();
        if let Ok(folder) = crate::places::passing() {
            let path = folder.join(FILE);
            let full = std::fs::metadata(&path).is_ok_and(|file| file.len() > MOST_BYTES);
            if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(!full).write(true).truncate(full).open(&path) {
                let _ = file.write_all(record(&message, &at, &trace).as_bytes());
            }
        }
        before(panic);
    }));
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_record_says_what_went_wrong_and_where() {
        let text = super::record("index out of bounds", "editor/src/panel.rs:10:5", "0: somewhere");
        assert!(text.starts_with("# brickloft "));
        for part in ["index out of bounds", "at editor/src/panel.rs:10:5", "0: somewhere"] {
            assert!(text.contains(part), "{part} is not in the record");
        }
    }
}
