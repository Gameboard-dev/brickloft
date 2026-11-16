//! Draw several pieces of furniture in one picture, each in its style's
//! own colours, to look at a set together without starting the editor
//! (`form_picture` draws one form alone, in one palette):
//!
//!   cargo run --release -p brickloft-editor --example furniture_picture -- out.png turn tilt [across] piece ...
//!
//! `turn` and `tilt` are as in `form_picture`. A piece is
//! `kind,WxDxH[,the rest of the furniture setting][@x,y,z][/set]`: its
//! kind, its width, depth and height in microbricks, and then whatever
//! follows the kind in a form's `furniture` setting (the seed first; see
//! docs/mcp.md), so `chair,11x11x23,3,2,0,11,34,3,1,1,85,farmhouse` is
//! the third farmhouse chair. `@x,y,z` is where its low corner stands;
//! without it the pieces stand in rows of `across` (8 if not given),
//! left to right and then front to back. `/set` is which of its style's
//! sets of colours it is drawn in (the first if not given). It says how
//! many bricks each piece is.

use brickloft_core::report::parse_spec;
use brickloft_core::DVec3;

#[path = "support/canvas.rs"]
mod canvas;
use canvas::Canvas;

/// Pixels along a side of the picture.
const SIZE: usize = 1400;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let numbers: Vec<f64> = args.iter().skip(1).take(3).map_while(|a| a.parse().ok()).collect();
    let (Some(to), true) = (args.first(), numbers.len() >= 2) else {
        eprintln!("usage: furniture_picture <picture.png> <turn> <tilt> [across] <kind,WxDxH[,settings][@x,y,z][/set]> ...");
        std::process::exit(2);
    };
    let across = numbers.get(2).map_or(8, |n| (*n as usize).max(1));
    let (mut next, mut row_deep, mut row_at, mut in_row) = (0.0f64, 0.0f64, 0.0f64, 0);
    let mut all = Vec::new();
    for arg in args[1 + numbers.len()..].iter() {
        let (arg, set) = arg.split_once('/').map_or((arg.as_str(), 0), |(a, s)| (a, s.parse::<usize>().unwrap_or(0)));
        let (piece, at) = arg.split_once('@').map_or((arg, None), |(p, at)| (p, Some(at)));
        let mut parts = piece.splitn(3, ',');
        let (kind, size, rest) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""), parts.next());
        let size: Vec<f64> = size.split('x').filter_map(|v| v.parse().ok()).collect();
        assert!(size.len() == 3, "{arg}: a piece is kind,WxDxH");
        // (kept as a form keeps it: half its depth, and a stretch along each axis)
        let text = format!("form = furniture\nfurniture = {kind}{}\nradius = {}\nstretch = {},1,{}\npoints = {},{},{}\n", rest.map_or(String::new(), |r| format!(",{r}")), size[1] / 2.0, size[0] / size[1], size[2] / size[1], size[0] / 2.0, size[1] / 2.0, size[2] / 2.0);
        let spec = parse_spec(&text).unwrap_or_else(|e| panic!("{arg}: {e}"));
        let built = brickloft_core::spec::build(&spec).unwrap_or_else(|e| panic!("{arg}: not built: {e}"));
        let stands = match at {
            Some(at) => {
                let at: Vec<f64> = at.split(',').filter_map(|v| v.parse().ok()).collect();
                assert!(at.len() == 3, "{arg}: a piece stands at x,y,z");
                DVec3::new(at[0], at[1], at[2])
            }
            None => {
                if in_row == across {
                    (next, row_at, row_deep, in_row) = (0.0, row_at + row_deep + 14.0, 0.0, 0);
                }
                let stands = DVec3::new(next, row_at, 0.0);
                (next, row_deep, in_row) = (next + size[0] + 8.0, row_deep.max(size[1]), in_row + 1);
                stands
            }
        };
        let sets = brickloft_editor::paint::palettes(spec.furniture.colours());
        let colours: Vec<[f64; 3]> = sets[set.min(sets.len() - 1)].iter().map(|c| c.map(f64::from)).collect();
        println!("{kind} {}x{}x{}: {} bricks, discrepancy {}", size[0], size[1], size[2], built.bricks.len(), built.error);
        all.push((built, stands, colours));
    }
    assert!(!all.is_empty(), "no pieces to draw");
    let corners = all.iter().flat_map(|(built, stands, _)| built.bricks.iter().flat_map(move |b| [0.0, 1.0].map(|far| *stands + DVec3::from_array([0, 1, 2].map(|k| b.min[k] as f64 + far * b.ext[k] as f64)))));
    let (lo, hi) = corners.fold((DVec3::MAX, DVec3::MIN), |(lo, hi), p| (lo.min(p), hi.max(p)));
    let (turn, tilt) = (numbers[0].to_radians(), numbers[1].to_radians());
    let towards = DVec3::new(turn.cos() * tilt.cos(), turn.sin() * tilt.cos(), tilt.sin());
    let mut canvas = Canvas::new(SIZE, 2, (lo + hi) / 2.0, SIZE as f64 / (hi - lo).length() / 1.02, towards);
    for (built, stands, colours) in &all {
        for (brick, shade) in built.bricks.iter().zip(&built.shades) {
            canvas.brick = colours[brickloft_core::pattern::colour_of(*shade, colours.len())];
            canvas.shape(brick.mask, *stands + DVec3::from_array(brick.min.map(|v| v as f64)), DVec3::from_array(brick.ext.map(|v| v as f64)));
        }
    }
    let file = std::io::BufWriter::new(std::fs::File::create(to).unwrap_or_else(|e| panic!("could not make {to}: {e}")));
    let mut encoder = png::Encoder::new(file, SIZE as u32, SIZE as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("could not start the picture");
    writer.write_image_data(&canvas.pixels()).expect("could not write the picture");
    println!("drawn to {to}");
}
