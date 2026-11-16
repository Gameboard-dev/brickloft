//! Draw the bricks of a form from a report or spec file as a picture, to
//! look at a fit without starting the editor:
//!
//!   cargo run --release -p brickloft-editor --example form_picture -- report.txt out.png [turn] [tilt] [zoom] [x y z] [colours:N] [key=value ...]
//!
//! `turn` is the side it is seen from, in degrees about z (default 35), and
//! `tilt` how far above, in degrees (default 35). `zoom` enlarges the
//! picture about the cell `x y z` (the middle of the form if not given). Extra `key=value`
//! arguments override the file's settings, as with the `inspect` example.

use brickloft_core::report::parse_spec;
use brickloft_core::DVec3;

#[path = "support/canvas.rs"]
mod canvas;
use canvas::Canvas;

/// Pixels along a side of the picture.
const SIZE: usize = 1400;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(from), Some(to)) = (args.first(), args.get(1)) else {
        eprintln!("usage: form_picture <report or spec file> <picture.png> [turn] [tilt] [zoom] [x y z] [key=value ...]");
        std::process::exit(2);
    };
    let numbers: Vec<f64> = args[2..].iter().filter_map(|a| a.parse().ok()).collect();
    let (turn, tilt) = (numbers.first().copied().unwrap_or(35.0).to_radians(), numbers.get(1).copied().unwrap_or(35.0).to_radians());
    let overrides: Vec<String> = args[2..].iter().filter(|a| a.contains('=')).map(|a| a.replacen('=', " = ", 1)).collect();
    let text = std::fs::read_to_string(from).unwrap_or_else(|e| panic!("could not read {from}: {e}"));
    // overrides go in a second [spec] section at the end, where a later line wins
    let spec = parse_spec(&format!("{text}\n[spec]\n{}\n", overrides.join("\n"))).unwrap_or_else(|e| panic!("{from}: {e}"));
    let built = brickloft_core::spec::build(&spec).unwrap_or_else(|e| panic!("not built: {e}"));
    let corner = |pick: fn(i32, i32) -> i32, far: bool| {
        DVec3::from_array([0, 1, 2].map(|k| built.bricks.iter().map(|b| b.min[k] + if far { b.ext[k] } else { 0 }).reduce(pick).unwrap_or(0) as f64))
    };
    let (lo, hi) = (corner(i32::min, false), corner(i32::max, true));
    let towards = DVec3::new(turn.cos() * tilt.cos(), turn.sin() * tilt.cos(), tilt.sin());
    // a closer look: `zoom` times as large, about the cell `x y z`
    let zoom = numbers.get(2).copied().unwrap_or(1.0);
    let middle = match numbers.get(3..6) {
        Some(at) => DVec3::new(at[0], at[1], at[2]),
        None => (lo + hi) / 2.0,
    };
    let mut canvas = Canvas::new(SIZE, 2, middle, zoom * SIZE as f64 / (hi - lo).length() / 1.02, towards);
    // A pattern's colours, if the form has one: a mosaic's ground, then the
    // colours its features share. `colours:N` uses the first N of them.
    const SHADES: [[f64; 3]; 8] = [[226.0, 214.0, 190.0], [176.0, 84.0, 60.0], [44.0, 74.0, 122.0], [204.0, 160.0, 72.0], [70.0, 120.0, 90.0], [120.0, 60.0, 100.0], [40.0, 40.0, 48.0], [150.0, 170.0, 190.0]];
    let colours = args.iter().find_map(|a| a.strip_prefix("colours:")?.parse::<usize>().ok()).unwrap_or(4).clamp(1, SHADES.len());
    for (n, brick) in built.bricks.iter().enumerate() {
        if let Some(shade) = built.shades.get(n) {
            canvas.brick = SHADES[brickloft_core::pattern::colour_of(*shade, colours)];
        }
        canvas.shape(brick.mask, DVec3::from_array(brick.min.map(|v| v as f64)), DVec3::from_array(brick.ext.map(|v| v as f64)));
    }
    let file = std::io::BufWriter::new(std::fs::File::create(to).unwrap_or_else(|e| panic!("could not make {to}: {e}")));
    let mut encoder = png::Encoder::new(file, SIZE as u32, SIZE as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("could not start the picture");
    writer.write_image_data(&canvas.pixels()).expect("could not write the picture");
    println!("{} bricks, discrepancy {:.1}%, drawn to {to}", built.bricks.len(), 100.0 * built.error / built.solid.max(1e-9));
}
