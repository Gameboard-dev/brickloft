//! Draw the nine brick shapes as the pictures in `docs/images/`:
//!
//!   cargo run -p arcs-editor --example shape_pictures
//!
//! Each picture is the shape's own mesh (`arcs_editor::mesh::shape_mesh`,
//! the one the editor draws) inside the outline of its box, so the
//! documentation shows what the code builds and cannot drift from it.
//!
//! The triangles are filled here, a pixel at a time, instead of by the
//! editor's renderer: that needs a window and a graphics card, and nine
//! small pictures of at most 28 triangles do not.

use arcs_core::shapes::{Kind, KINDS};
use arcs_core::DVec3;
use arcs_editor::mesh::shape_mesh;

/// Pixels along a side of a finished picture.
const SIZE: usize = 220;
/// Each pixel is the average of FINE x FINE drawn ones, to smooth the edges.
const FINE: usize = 4;
const BRICK: [f64; 3] = [242.0, 199.0, 68.0];
const EDGE: [f64; 3] = [70.0, 52.0, 12.0];
const BOX: [f64; 3] = [140.0, 140.0, 140.0];
const NAMES: [&str; 9] = ["cube", "ramp", "tetrahedron", "cut-cube", "hip", "inner-corner", "half-outer-corner", "half-inner-corner", "half-inner-corner-inv"];

struct Canvas {
    side: usize,
    colour: Vec<[f64; 4]>,
    depth: Vec<f64>,
    right: DVec3,
    up: DVec3,
    towards: DVec3,
}

impl Canvas {
    /// Where a point of the unit box lands: across, down, and how near.
    fn place(&self, p: DVec3) -> [f64; 3] {
        let p = p - DVec3::splat(0.5);
        // the box's corners are 0.87 from its middle; leave a margin
        let scale = self.side as f64 / 2.0 / 0.95;
        let half = self.side as f64 / 2.0;
        [half + p.dot(self.right) * scale, half - p.dot(self.up) * scale, p.dot(self.towards)]
    }

    /// Fill one triangle. `rim` is, per corner, how far it is from the
    /// outline of its face, in box widths: a line is drawn near zero.
    fn triangle(&mut self, corners: [DVec3; 3], rim: [f64; 3], shade: f64) {
        let s = corners.map(|c| self.place(c));
        let area = (s[1][0] - s[0][0]) * (s[2][1] - s[0][1]) - (s[2][0] - s[0][0]) * (s[1][1] - s[0][1]);
        if area.abs() < 1e-9 {
            return;
        }
        let low = |k: usize| s.iter().map(|c| c[k]).fold(f64::INFINITY, f64::min).floor().max(0.0) as usize;
        let high = |k: usize| (s.iter().map(|c| c[k]).fold(0.0, f64::max).ceil() as usize).min(self.side - 1);
        for y in low(1)..=high(1) {
            for x in low(0)..=high(0) {
                let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
                let weight = |a: usize, b: usize| ((s[a][0] - px) * (s[b][1] - py) - (s[b][0] - px) * (s[a][1] - py)) / area;
                let w = [weight(1, 2), weight(2, 0), weight(0, 1)];
                if w.iter().any(|v| *v < 0.0) {
                    continue;
                }
                let near = w[0] * s[0][2] + w[1] * s[1][2] + w[2] * s[2][2];
                let at = y * self.side + x;
                if near <= self.depth[at] {
                    continue;
                }
                self.depth[at] = near;
                let from_rim = w[0] * rim[0] + w[1] * rim[1] + w[2] * rim[2];
                let c = if from_rim < 0.014 { EDGE } else { BRICK.map(|v| v * shade) };
                self.colour[at] = [c[0], c[1], c[2], 1.0];
            }
        }
    }

    /// A line of the box's outline, drawn only where it is in front of the
    /// shape, so the shape hides the part of its box behind it.
    fn line(&mut self, from: DVec3, to: DVec3) {
        let (a, b) = (self.place(from), self.place(to));
        let steps = self.side * 2;
        let reach = FINE as i64 / 2;
        for i in 0..=steps {
            let t = i as f64 / steps as f64;
            let p = [0, 1, 2].map(|k| a[k] + (b[k] - a[k]) * t);
            for dy in -reach..=reach {
                for dx in -reach..=reach {
                    let (x, y) = (p[0] as i64 + dx, p[1] as i64 + dy);
                    if x < 0 || y < 0 || x >= self.side as i64 || y >= self.side as i64 {
                        continue;
                    }
                    let at = y as usize * self.side + x as usize;
                    if p[2] > self.depth[at] + 0.02 {
                        self.colour[at] = [BOX[0], BOX[1], BOX[2], 1.0];
                    }
                }
            }
        }
    }

    /// The finished picture: each pixel the average of the ones drawn for
    /// it, with see-through where nothing was drawn.
    fn pixels(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(SIZE * SIZE * 4);
        for y in 0..SIZE {
            for x in 0..SIZE {
                let mut sum = [0.0; 4];
                for fy in 0..FINE {
                    for fx in 0..FINE {
                        let c = self.colour[(y * FINE + fy) * self.side + x * FINE + fx];
                        for k in 0..3 {
                            sum[k] += c[k] * c[3];
                        }
                        sum[3] += c[3];
                    }
                }
                for k in 0..3 {
                    out.push(if sum[3] > 0.0 { (sum[k] / sum[3]).round() as u8 } else { 0 });
                }
                out.push((sum[3] / (FINE * FINE) as f64 * 255.0).round() as u8);
            }
        }
        out
    }
}

fn picture(mask: u8) -> Vec<u8> {
    let side = SIZE * FINE;
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
    let right = DVec3::Z.cross(towards).normalize();
    let mut canvas = Canvas { side, colour: vec![[0.0; 4]; side * side], depth: vec![f64::NEG_INFINITY; side * side], right, up: towards.cross(right), towards };
    // lit from over the viewer's left shoulder
    let light = (towards - right * 0.6 + DVec3::Z * 0.5).normalize();
    for t in (0..mesh.positions.len()).step_by(3) {
        let corners = [point(t), point(t + 1), point(t + 2)];
        let normal = DVec3::new(mesh.normals[t][0] as f64, mesh.normals[t][1] as f64, mesh.normals[t][2] as f64);
        // the mesh marks the face's middle 1 and its outline 0; the middle
        // is this far from the outline, which turns the mark into a distance
        let along = (corners[2] - corners[1]).normalize();
        let height = (corners[0] - corners[1]).cross(along).length();
        let rim = [0, 1, 2].map(|k| mesh.rim[t + k] as f64 * height);
        canvas.triangle(corners, rim, 0.5 + 0.5 * normal.dot(light).max(0.0));
    }
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
