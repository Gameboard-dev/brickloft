//! Draw several forms as the workspace builds them together, to look at a
//! join without starting the editor (`form_picture` draws one form alone):
//!
//!   cargo run --release -p brickloft-editor --example join_picture -- out.png turn tilt zoom x y z form@x,y,z[@key=value...] ...
//!
//! `turn`, `tilt` and `zoom` are as in `form_picture`; a `zoom` of 0 shows
//! everything, and otherwise the picture is that many times as large about
//! the microbrick `x y z`. Each form is a report or spec file, where it
//! stands in microbricks (which may end in a half), and any settings to
//! change: `ball.txt@6,0,4@blend=2` is the ball joined with a blend of 2.
//! Each form is drawn in a colour of its own. `zone` as a last argument
//! darkens every brick that is not one of a form's own: the bricks the
//! join cut or fitted afresh. It also says how long the join took.

use brickloft_core::report::parse_spec;
use brickloft_core::DVec3;
use brickloft_editor::paint::Paint;
use brickloft_editor::state::State;

#[path = "support/canvas.rs"]
mod canvas;
use canvas::Canvas;

/// Pixels along a side of the picture.
const SIZE: usize = 1400;

const COLOURS: [[u8; 3]; 4] = [[214, 208, 196], [96, 140, 200], [200, 120, 80], [120, 170, 110]];

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let numbers: Vec<f64> = args.iter().skip(1).take(6).filter_map(|a| a.parse().ok()).collect();
    let (Some(to), true) = (args.first(), numbers.len() == 6) else {
        eprintln!("usage: join_picture <picture.png> <turn> <tilt> <zoom> <x> <y> <z> <form@x,y,z[@key=value...]> ... [zone]");
        std::process::exit(2);
    };
    let mut forms = Vec::new();
    for (n, arg) in args[7..].iter().filter(|a| a.contains('@')).enumerate() {
        let mut parts = arg.split('@');
        let from = parts.next().unwrap();
        let at: Vec<f64> = parts.next().unwrap_or("").split(',').filter_map(|v| v.parse().ok()).collect();
        assert!(at.len() == 3, "{arg}: a form stands at x,y,z");
        let overrides: Vec<String> = parts.map(|a| a.replacen('=', " = ", 1)).collect();
        let text = std::fs::read_to_string(from).unwrap_or_else(|e| panic!("could not read {from}: {e}"));
        let spec = parse_spec(&format!("{text}\n[spec]\n{}\n", overrides.join("\n"))).unwrap_or_else(|e| panic!("{from}: {e}"));
        let paint = Paint { colours: vec![COLOURS[n % COLOURS.len()]], ..Paint::default() };
        forms.push((spec, paint, [0, 1, 2].map(|k| (at[k] * 2.0).round() as i32)));
    }
    assert!(!forms.is_empty(), "no forms to draw");
    let mut state = State::new();
    state.open(forms);
    state.refresh(|| 0.0);
    let started = std::time::Instant::now();
    let all = state.painted();
    let took = started.elapsed();
    let at = |b: &brickloft_editor::state::Painted, far: bool| DVec3::from_array([0, 1, 2].map(|k| (b.0.min[k] + if far { b.0.ext[k] } else { 0 }) as f64 + b.3[k] as f64 / 2.0));
    let (lo, hi) = all.iter().fold((DVec3::MAX, DVec3::MIN), |(lo, hi), b| (lo.min(at(b, false)), hi.max(at(b, true))));
    let (turn, tilt, zoom) = (numbers[0].to_radians(), numbers[1].to_radians(), numbers[2]);
    let towards = DVec3::new(turn.cos() * tilt.cos(), turn.sin() * tilt.cos(), tilt.sin());
    let (middle, zoom) = if zoom > 0.0 { (DVec3::new(numbers[3], numbers[4], numbers[5]), zoom) } else { ((lo + hi) / 2.0, 1.0) };
    let mut canvas = Canvas::new(SIZE, 2, middle, zoom * SIZE as f64 / (hi - lo).length() / 1.02, towards);
    let show_zone = args.iter().any(|a| a == "zone");
    let own: Vec<&[brickloft_core::shapes::CellBrick]> = std::iter::once(&state.built).chain(state.others.iter().map(|o| &o.built)).map(|b| b.bricks.as_slice()).collect();
    let mut changed = 0;
    for brick in &all {
        let colour = brick.1.unwrap_or(COLOURS[0]).map(f64::from);
        let fresh = !own[brick.2].contains(&brick.0);
        changed += fresh as usize;
        canvas.brick = if show_zone && fresh { colour.map(|c| c * 0.72) } else { colour };
        canvas.shape(brick.0.mask, at(brick, false), DVec3::from_array(brick.0.ext.map(|v| v as f64)));
    }
    let file = std::io::BufWriter::new(std::fs::File::create(to).unwrap_or_else(|e| panic!("could not make {to}: {e}")));
    let mut encoder = png::Encoder::new(file, SIZE as u32, SIZE as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("could not start the picture");
    writer.write_image_data(&canvas.pixels()).expect("could not write the picture");
    println!("{} bricks, {changed} of them cut or fitted afresh, put together in {:.1} ms, drawn to {to}", all.len(), took.as_secs_f64() * 1000.0);
}
