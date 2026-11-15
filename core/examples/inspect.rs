//! Build a form from a report or spec file and print a fresh report on it.
//!
//!   cargo run -p arcs-core --example inspect -- arcs-report.txt
//!   cargo run -p arcs-core --example inspect -- arcs-report.txt simplify=0 grain=1
//!
//! Extra `key=value` arguments override the file's settings, to see how a
//! change would fit the same form.

use arcs_core::report::{parse_spec, report};

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("usage: inspect <report or spec file> [key=value ...]");
        std::process::exit(2);
    };
    let mut text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) => {
            eprintln!("inspect: could not read {path}: {e}");
            std::process::exit(1);
        }
    };
    // overrides go in a second [spec] section at the end, where a later line wins
    let overrides: Vec<String> = args.map(|a| a.replacen('=', " = ", 1)).collect();
    text = format!("{text}\n[spec]\n{}\n", overrides.join("\n"));
    match parse_spec(&text).and_then(|spec| report(&spec)) {
        Ok(report) => print!("{report}"),
        Err(e) => {
            eprintln!("inspect: {e}");
            std::process::exit(1);
        }
    }
}
