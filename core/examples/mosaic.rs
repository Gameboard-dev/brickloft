//! Print what a mosaic sample grows into, as text, to judge a sample
//! without starting the editor:
//!
//!   cargo run --release -p brickloft-core --example mosaic -- core/src/mosaics/key.txt [window] [turned] [seed]
//!
//! `window` is how many cells across the windows learnt are (3 if not
//! given), `turned` is `turned` to let the motifs lie any way round, and
//! `seed` is which filling.

use brickloft_core::wfc::{learn, Grid, Sample};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.first() else {
        eprintln!("usage: mosaic <sample file> [window] [turned] [seed]");
        std::process::exit(2);
    };
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("could not read {path}: {e}"));
    let sample = Sample::read(&text).unwrap_or_else(|e| panic!("{path}: {e}"));
    let window = args.get(1).and_then(|a| a.parse().ok()).unwrap_or(3);
    let turned = args.iter().any(|a| a == "turned");
    let seed = args.iter().skip(2).find_map(|a| a.parse().ok()).unwrap_or(1);
    let learnt = learn(&sample, window, turned).unwrap_or_else(|e| panic!("{path}: {e}"));
    let (across, down) = (64, 28);
    let start = std::time::Instant::now();
    match Grid::new(&learnt.rules, [across, down]).fill(seed, 20) {
        Ok(filled) => {
            for row in filled.chunks(across) {
                println!("{}", row.iter().map(|tile| [' ', '#', '+', 'o', '.', ':', '=', '%', '@', '*'][tile.map_or(0, |t| learnt.colour[t] as usize).min(9)]).collect::<String>());
            }
            println!("{} tiles learnt, filled in {:.0} ms", learnt.rules.tiles(), start.elapsed().as_secs_f64() * 1000.0);
        }
        Err(e) => println!("{} tiles learnt: {e}", learnt.rules.tiles()),
    }
}
