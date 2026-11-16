//! Draw a lawn made a landscape by Randomize, with its plants, as a
//! picture: the bricks the editor would show, without starting it.
//!
//!   cargo run --release -p brickloft-editor --example landscape_picture -- long wide seed foliage out.png [turn] [tilt] [zoom] [set]
//!
//! `long` and `wide` are the lawn's sides in microbricks, `seed` which
//! landscape, `foliage` how thickly it is planted (0 to 100). `turn` and
//! `tilt` are as the `form_picture` example has them, `zoom` enlarges
//! about the middle, and `set` is which of grass's sets of colours the
//! lawn is in (0 Lawn to 4 Winter), which the plants' colours follow.
//!
//! It goes through the editor's own state (`State::randomize_lawn`), so
//! what is drawn is what one press of the button makes: the lawn's bricks
//! with the plants joined into it, each brick in the colour the editor
//! gives it.

use brickloft_core::spec::{Form, Spec};
use brickloft_core::tree::{Kind, Tree};
use brickloft_core::DVec3;
use brickloft_editor::state::{State, TREE};

#[path = "support/canvas.rs"]
mod canvas;
use canvas::Canvas;

/// Pixels along a side of the picture.
const SIZE: usize = 1400;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let number = |n: usize, usual: f64| args.get(n).and_then(|a| a.parse::<f64>().ok()).unwrap_or(usual);
    let (Some(to), true) = (args.get(4), args.len() >= 5) else {
        eprintln!("usage: landscape_picture <long> <wide> <seed> <foliage> <picture.png> [turn] [tilt] [zoom] [set]");
        std::process::exit(2);
    };
    let (long, wide, seed, foliage) = (number(0, 180.0), number(1, 120.0), number(2, 1.0) as u64, number(3, 50.0) as i32);
    let (turn, tilt, zoom, set) = (number(5, 35.0).to_radians(), number(6, 35.0).to_radians(), number(7, 1.0), number(8, 0.0) as usize);
    // a lawn with square corners, as one laid out in the editor by its four dots
    let mut state = State::new();
    state.form = TREE;
    state.preset();
    state.usual_tree(Kind::Grass);
    let corner = |x: f64, y: f64| DVec3::new(x + 0.5, y + 0.5, 0.5);
    let lawn = Spec { form: Form::Tree, points: vec![corner(-long / 2.0, -wide / 2.0), corner(long / 2.0, -wide / 2.0), corner(long / 2.0, wide / 2.0), corner(-long / 2.0, wide / 2.0)], closed: true, protrude: true, radius: 4.0, depth: 4, tree: Tree { seed: 1, baubles: 2, ..Tree::of(Kind::Grass) }, ..state.full_spec() };
    state.load_spec(lawn);
    let sets = brickloft_editor::paint::palettes("grass");
    state.paint.colours = sets[set.min(sets.len() - 1)].to_vec();
    state.spec.fill = set == 4;
    state.refresh(|| 0.0);
    let made = state.randomize_lawn(seed, foliage).unwrap_or_else(|why| panic!("no landscape: {why}"));
    state.build_owed_now();
    state.refresh(|| 0.0);
    let bricks = state.painted();
    // where each brick is in the workspace: its own cells, and where its form stands (in half cells)
    let at = |brick: &brickloft_core::shapes::CellBrick, place: [i32; 3]| DVec3::from_array([0, 1, 2].map(|k| brick.min[k] as f64 + place[k] as f64 / 2.0));
    let (lo, hi) = bricks.iter().fold((DVec3::MAX, DVec3::MIN), |(lo, hi), (brick, _, _, place)| (lo.min(at(brick, *place)), hi.max(at(brick, *place) + DVec3::from_array(brick.ext.map(|e| e as f64)))));
    let towards = DVec3::new(turn.cos() * tilt.cos(), turn.sin() * tilt.cos(), tilt.sin());
    let mut canvas = Canvas::new(SIZE, 2, (lo + hi) / 2.0, zoom * SIZE as f64 / (hi - lo).length() / 1.02, towards);
    // the water last and seen through, as the editor draws it (ice more solid)
    let waters = state.waters();
    let water = |colour: &Option<[u8; 3]>, form: usize| waters.get(form).copied().flatten().filter(|w| Some(w.0) == *colour);
    for wet in [false, true] {
        for (brick, colour, form, place) in bricks.iter().filter(|(_, colour, form, _)| water(colour, *form).is_some() == wet) {
            canvas.solid = water(colour, *form).map_or(1.0, |w| w.1 as f64 / 255.0);
            canvas.brick = colour.unwrap_or([200, 200, 200]).map(f64::from);
            canvas.shape(brick.mask, at(brick, *place), DVec3::from_array(brick.ext.map(|v| v as f64)));
        }
    }
    let file = std::io::BufWriter::new(std::fs::File::create(to).unwrap_or_else(|e| panic!("could not make {to}: {e}")));
    let mut encoder = png::Encoder::new(file, SIZE as u32, SIZE as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("could not start the picture");
    writer.write_image_data(&canvas.pixels()).expect("could not write the picture");
    let names = brickloft_core::tree::KINDS;
    let kinds: Vec<&str> = made.plants.iter().map(|plant| names[plant.0 as usize].1).collect();
    println!("landscape {seed} on {long} by {wide}: {} touches, {} ponds, fall {:?} (try {}), {} plants [{}], {} bricks in all", made.touches, made.ponds, made.fall, made.tries, made.plants.len(), kinds.join(" "), bricks.len());
}
