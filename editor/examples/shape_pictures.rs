//! Draw the nine brick shapes as the pictures in `docs/images/`:
//!
//!   cargo run -p brickloft-editor --example shape_pictures
//!
//! Each picture is the shape's own mesh (`brickloft_editor::mesh::shape_mesh`,
//! the one the editor draws) inside the outline of its box, so the
//! documentation shows what the code builds and cannot drift from it.
//!
//! The triangles are filled by `support/canvas.rs`.

use brickloft_core::shapes::{Kind, KINDS};
use brickloft_core::DVec3;
use brickloft_editor::mesh::shape_mesh;

#[path = "support/canvas.rs"]
mod canvas;
use canvas::Canvas;

/// Pixels along a side of a finished picture.
const SIZE: usize = 220;
/// Each pixel is the average of FINE x FINE drawn ones, to smooth the edges.
const FINE: usize = 4;
const NAMES: [&str; 9] = ["cube", "ramp", "tetrahedron", "cut-cube", "hip", "inner-corner", "half-outer-corner", "half-inner-corner", "half-inner-corner-inv"];

fn picture(mask: u8) -> Vec<u8> {
    let mesh = shape_mesh(mask);
    let point = |i: usize| DVec3::new(mesh.positions[i][0] as f64, mesh.positions[i][1] as f64, mesh.positions[i][2] as f64);
    // Seen from above, from whichever side shows the most of the shape. One
    // fixed side would look straight at a ramp's slope and show it as a
    // flat square. Each side is scored by the square roots of the areas its
    // faces show, which is highest when several faces show about equally.
    let from = |step: usize| {
        let angle = (step as f64 * 15.0 + 7.0).to_radians();
        DVec3::new(angle.cos(), angle.sin(), 0.6).normalize()
    };
    let showing = |towards: DVec3| {
        let mut faces: Vec<(DVec3, f64)> = Vec::new();
        for t in (0..mesh.positions.len()).step_by(3) {
            let across = (point(t + 1) - point(t)).cross(point(t + 2) - point(t));
            let normal = DVec3::new(mesh.normals[t][0] as f64, mesh.normals[t][1] as f64, mesh.normals[t][2] as f64);
            let shown = across.length() / 2.0 * normal.dot(towards).max(0.0);
            match faces.iter_mut().find(|(n, _)| n.distance(normal) < 1e-6) {
                Some(face) => face.1 += shown,
                None => faces.push((normal, shown)),
            }
        }
        faces.iter().map(|(_, shown)| shown.sqrt()).sum::<f64>()
    };
    let towards = from((0..24).max_by(|a, b| showing(from(*a)).total_cmp(&showing(from(*b)))).unwrap());
    // the box's corners are 0.87 from its middle; leave a margin
    let mut canvas = Canvas::new(SIZE, FINE, DVec3::splat(0.5), SIZE as f64 / 2.0 / 0.95, towards);
    canvas.shape(mask, DVec3::ZERO, DVec3::ONE);
    let corner = |c: u8| DVec3::new((c & 1) as f64, (c >> 1 & 1) as f64, (c >> 2 & 1) as f64);
    for c in 0..8u8 {
        for k in 0..3 {
            if c >> k & 1 == 0 {
                canvas.line(corner(c), corner(c | 1 << k));
            }
        }
    }
    canvas.pixels()
}

fn main() {
    let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs/images");
    std::fs::create_dir_all(&folder).expect("could not make docs/images");
    for (kind, name) in KINDS.iter().zip(NAMES) {
        let path = folder.join(format!("{name}.png"));
        let file = std::io::BufWriter::new(std::fs::File::create(&path).expect("could not make the picture's file"));
        let mut encoder = png::Encoder::new(file, SIZE as u32, SIZE as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Best);
        let mut writer = encoder.write_header().expect("could not start the picture");
        // The ramp's own pose stands on end, its slope facing sideways. It
        // is drawn lying down instead (solid where x + z <= 1: corners 5
        // and 7 missing), which is how a ramp is usually met.
        let mask = if *kind == Kind::Ramp { 0x5f } else { kind.canonical() };
        writer.write_image_data(&picture(mask)).expect("could not write the picture");
        println!("wrote {}", path.display());
    }
}
