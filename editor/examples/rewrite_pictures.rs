//! Run rule programs and draw what they make, to judge them without the editor:
//!
//!   cargo run --release -p brickloft-editor --example rewrite_pictures -- <folder of pictures> [model ...]
//!
//! For each program it runs seeds until four have reached the end and draws each from a
//! three-quarter view, with wedges and without, and draws eight frames of
//! the first seed's run in one picture, to show what a person watching
//! would see, and the four seeds with wedges side by side. It prints, for each run, the turns taken, the time, and the
//! bricks plain and with wedges.
//!
//! The programs and their tiles are read from `core/models/markov`, found
//! from this crate's own folder.
//!
//! Until the form is in the editor, the interpreter is not part of the
//! core's library: it is brought in here by path, so that trying it
//! touches no file another branch is working on. The two `mod`s below
//! stand in for the core's own, which the interpreter names as `crate::`.

use brickloft_core::DVec3;

mod shapes {
    pub use brickloft_core::shapes::*;
}
mod pattern {
    pub use brickloft_core::pattern::*;
}
#[allow(dead_code, unused_imports)]
#[path = "../../core/src/rewrite.rs"]
mod rewrite;
#[path = "support/canvas.rs"]
mod canvas;
use canvas::Canvas;
use rewrite::{Finish, Limits, Machine, Resources};

/// Each program, the size it is run at (the original's `models.xml` gives these) and microbricks to a cell.
const PROGRAMS: [(&str, [usize; 3]); 7] = [("Apartemazements", [8, 8, 8]), ("CarmaTower", [12, 12, 18]), ("ModernHouse", [9, 9, 4]), ("SeaVilla", [10, 10, 4]), ("PillarsOfEternity", [9, 9, 9]), ("Escher", [8, 8, 8]), ("StrangeDungeon", [29, 29, 29])];

fn draw(bricks: &rewrite::Bricks, colours: &[[f64; 3]], side: usize, turn: f64, tilt: f64) -> Vec<u8> {
    let corner = |pick: fn(i32, i32) -> i32, far: bool| DVec3::from_array([0, 1, 2].map(|k| bricks.bricks.iter().map(|b| b.min[k] + if far { b.ext[k] } else { 0 }).reduce(pick).unwrap_or(0) as f64));
    let (lo, hi) = (corner(i32::min, false), corner(i32::max, true));
    let (turn, tilt) = (turn.to_radians(), tilt.to_radians());
    let towards = DVec3::new(turn.cos() * tilt.cos(), turn.sin() * tilt.cos(), tilt.sin());
    let mut canvas = Canvas::new(side, 2, (lo + hi) / 2.0, side as f64 / (hi - lo).length().max(1.0) / 1.02, towards);
    for (brick, shade) in bricks.bricks.iter().zip(&bricks.shades) {
        canvas.brick = colours[*shade as usize];
        canvas.shape(brick.mask, DVec3::from_array(brick.min.map(|v| v as f64)), DVec3::from_array(brick.ext.map(|v| v as f64)));
    }
    canvas.pixels()
}

fn save(path: &std::path::Path, pixels: &[u8], width: usize, height: usize) {
    let file = std::io::BufWriter::new(std::fs::File::create(path).unwrap_or_else(|e| panic!("could not make {}: {e}", path.display())));
    let mut encoder = png::Encoder::new(file, width as u32, height as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header().expect("could not start the picture").write_image_data(pixels).expect("could not write the picture");
}

/// Several square pictures side by side, `across` to a row, on white.
fn sheet(frames: &[Vec<u8>], side: usize, across: usize) -> (Vec<u8>, usize, usize) {
    let down = frames.len().div_ceil(across);
    let (width, height) = (side * across, side * down);
    let mut out = vec![255u8; width * height * 4];
    for (n, frame) in frames.iter().enumerate() {
        let (ox, oy) = (n % across * side, n / across * side);
        for y in 0..side {
            for x in 0..side {
                let (from, to) = ((y * side + x) * 4, ((oy + y) * width + ox + x) * 4);
                let a = frame[from + 3] as u32;
                for k in 0..3 {
                    out[to + k] = ((frame[from + k] as u32 * a + 255 * (255 - a)) / 255) as u8;
                }
            }
        }
    }
    (out, width, height)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(folder) = args.first().map(std::path::PathBuf::from) else {
        eprintln!("usage: rewrite_pictures <folder of pictures> [model ...]");
        std::process::exit(2);
    };
    std::fs::create_dir_all(&folder).expect("could not make the folder");
    let home = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../core/models/markov");
    let resources = Resources { roots: vec![home.clone()], bundled: &[] };
    let limits = Limits::USUAL;
    let palette = rewrite::palette("palette.xml", &std::fs::read(home.join("resources/palette.xml")).expect("no palette"), &limits).unwrap_or_else(|e| panic!("{e}"));
    for (name, size) in PROGRAMS.iter().filter(|(name, _)| args.len() < 2 || args[1..].iter().any(|a| a == name)) {
        let text = std::fs::read(home.join(format!("models/{name}.xml"))).expect("no such program");
        let began = std::time::Instant::now();
        let mut machine = match Machine::load(&format!("{name}.xml"), &text, *size, &resources, &limits) {
            Ok(machine) => machine,
            Err(fault) => {
                println!("{name}: NOT LOADED: {fault}");
                continue;
            }
        };
        println!("{name}: loaded in {:.0} ms", began.elapsed().as_secs_f64() * 1e3);
        let colour = |letters: &[char]| -> Vec<[f64; 3]> { letters.iter().map(|l| palette.iter().find(|(known, _)| known == l).map_or([128.0; 3], |(_, c)| c.map(f64::from))).collect() };
        // a cell is as many microbricks as keeps the finished thing some 150 across
        // the first four seeds that reach the end; one that does not is said and passed over
        let (mut steps_of_first, mut first, mut drawn) = (0, 0, 0);
        let mut four = Vec::new();
        for seed in 1..=24u64 {
            if drawn == 4 {
                break;
            }
            let began = std::time::Instant::now();
            machine.start(seed);
            let ended = machine.finish();
            let (steps, took) = (machine.steps(), began.elapsed().as_secs_f64() * 1e3);
            if let Err(fault) = &ended {
                println!("  seed {seed}: {steps} turns, {took:.0} ms: {fault}");
                continue;
            }
            drawn += 1;
            if first == 0 {
                (first, steps_of_first) = (seed, steps);
            }
            let view = machine.view();
            let scale = (150 / view.size.iter().max().copied().unwrap_or(1) as i32).clamp(1, 5);
            let colours = colour(view.letters);
            let size = view.size;
            let made = [Finish::Plain, Finish::Wedged].map(|finish| rewrite::bricks(&view, scale, finish, 80, &limits));
            match made {
                [Ok(plain), Ok(wedged)] => {
                    println!("  seed {seed}: {steps} turns, {took:.0} ms, grid {}x{}x{}, {} bricks plain, {} with wedges ({} sloped){}", size[0], size[1], size[2], plain.bricks.len(), wedged.bricks.len(), wedged.wedges, ended.map_or_else(|fault| format!("; {fault}"), |_| String::new()));
                    let frames = vec![draw(&wedged, &colours, 800, 215.0, 30.0), draw(&plain, &colours, 800, 215.0, 30.0)];
                    let (pixels, width, height) = sheet(&frames, 800, 2);
                    save(&folder.join(format!("{name}-seed{seed}.png")), &pixels, width, height);
                    four.push(draw(&wedged, &colours, 500, 215.0, 30.0));
                }
                [Err(fault), _] | [_, Err(fault)] => println!("  seed {seed}: {steps} turns, no bricks: {fault}"),
            }
        }
        let (pixels, width, height) = sheet(&four, 500, 4);
        save(&folder.join(format!("{name}-four-seeds.png")), &pixels, width, height);
        // the first seed again, stopped eight times on the way
        machine.start(first);
        let mut frames = Vec::new();
        for frame in 1..=8 {
            let until = steps_of_first * frame / 8;
            while machine.steps() < until && machine.running() {
                if machine.step().is_err() {
                    break;
                }
            }
            let view = machine.view();
            let scale = (150 / view.size.iter().max().copied().unwrap_or(1) as i32).clamp(1, 5);
            let colours = colour(view.letters);
            if let Ok(made) = rewrite::bricks(&view, scale, Finish::Plain, 80, &Limits { bricks: u64::MAX, ..limits }) {
                frames.push(draw(&made, &colours, 500, 215.0, 30.0));
            }
        }
        let (pixels, width, height) = sheet(&frames, 500, 4);
        save(&folder.join(format!("{name}-growing.png")), &pixels, width, height);
    }
}
