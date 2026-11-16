//! The program: read the arguments, run the one command, say how it ended.
//! Everything else is in the library beside this file (see `lib.rs`).

use generate::args::{self, Command};
use generate::error::Error;
use generate::protocol::Out;
use generate::{files, serve, weights_folder, Session};
use std::process::ExitCode;
use std::sync::Arc;

fn main() -> ExitCode {
    let out = Arc::new(Out::stdout());
    let ended = (|| -> Result<(), Error> {
        let said = |e: Error| {
            out.error(&e);
            e
        };
        let args = args::parse(std::env::args_os().skip(1)).map_err(said)?;
        let weights = weights_folder(args.weights).map_err(said)?;
        let mut session = Session::new(weights, args.device);
        if args.command == Command::Serve {
            // standard input is the requests; its end is the end of the program
            return serve::serve(&mut session, out.clone());
        }
        if args.watch_stdin {
            files::watch_stdin(out.clone());
        }
        session.answer(&args.command, &out)
    })();
    match ended {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => ExitCode::from(e.exit_code()),
    }
}
