//! The box a form's bricks fill: `cargo run -p arcs-core --example extent -- spec.txt [key=value ...]`
use arcs_core::report::parse_spec;
use arcs_core::spec::build;

fn main() {
    let mut args = std::env::args().skip(1);
    let mut text = std::fs::read_to_string(args.next().expect("a spec file")).expect("unreadable");
    for extra in args {
        text.push_str(&format!("\n{}\n", extra.replacen('=', " = ", 1)));
    }
    match build(&parse_spec(&text).expect("no spec")) {
        Ok(built) => {
            let lo = [0, 1, 2].map(|k| built.bricks.iter().map(|b| b.min[k]).min().unwrap());
            let hi = [0, 1, 2].map(|k| built.bricks.iter().map(|b| b.min[k] + b.ext[k]).max().unwrap());
            println!("{} bricks from {lo:?} to {hi:?}, {:.1}% off", built.bricks.len(), 100.0 * built.error / built.solid.max(1e-9));
        }
        Err(e) => println!("does not build: {e}"),
    }
}
