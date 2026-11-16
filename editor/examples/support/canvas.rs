//! A small canvas that fills triangles a pixel at a time, shared by the
//! examples that draw pictures (`shape_pictures`, `form_picture`).
//!
//! The editor's own renderer needs a window and a graphics card. These
//! pictures are of a few thousand flat triangles at most, which a loop over
//! pixels draws in well under a second with nothing to set up.

// each example uses the part of this it needs
#![allow(dead_code)]

use brickloft_core::DVec3;
use brickloft_editor::mesh::shape_mesh;

pub const BRICK: [f64; 3] = [242.0, 199.0, 68.0];
const EDGE: [f64; 3] = [70.0, 52.0, 12.0];
const BOX: [f64; 3] = [140.0, 140.0, 140.0];

pub struct Canvas {
    /// pixels drawn along a side: `fine` for each pixel of the picture
    pub side: usize,
    pub fine: usize,
    /// the point of the scene at the middle of the picture, and how many
    /// drawn pixels one unit of the scene is
    pub middle: DVec3,
    pub scale: f64,
    pub brick: [f64; 3],
    pub colour: Vec<[f64; 4]>,
    pub depth: Vec<f64>,
    pub right: DVec3,
    pub up: DVec3,
    pub towards: DVec3,
}

impl Canvas {
    /// An empty canvas looking along `-towards`, with z up. `scale` is how
    /// many pixels of the picture one unit of the scene is.
    pub fn new(size: usize, fine: usize, middle: DVec3, scale: f64, towards: DVec3) -> Canvas {
        let side = size * fine;
        let right = DVec3::Z.cross(towards).normalize();
        Canvas { side, fine, middle, scale: scale * fine as f64, brick: BRICK, colour: vec![[0.0; 4]; side * side], depth: vec![f64::NEG_INFINITY; side * side], right, up: towards.cross(right), towards }
    }

    /// Where a point of the scene lands: across, down, and how near.
    fn place(&self, p: DVec3) -> [f64; 3] {
        let p = p - self.middle;
        let half = self.side as f64 / 2.0;
        [half + p.dot(self.right) * self.scale, half - p.dot(self.up) * self.scale, p.dot(self.towards)]
    }

    /// Fill one triangle. `rim` is, per corner, how far it is from the
    /// outline of its face, in box widths: a line is drawn near zero.
    pub fn triangle(&mut self, corners: [DVec3; 3], rim: [f64; 3], shade: f64) {
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
                let c = if from_rim < 0.014 { EDGE } else { self.brick.map(|v| v * shade) };
                self.colour[at] = [c[0], c[1], c[2], 1.0];
            }
        }
    }

    /// A line of the box's outline, drawn only where it is in front of the
    /// shape, so the shape hides the part of its box behind it.
    pub fn line(&mut self, from: DVec3, to: DVec3) {
        let (a, b) = (self.place(from), self.place(to));
        let steps = self.side * 2;
        let reach = self.fine as i64 / 2;
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
    pub fn pixels(&self) -> Vec<u8> {
        let (fine, size) = (self.fine, self.side / self.fine);
        let mut out = Vec::with_capacity(size * size * 4);
        for y in 0..size {
            for x in 0..size {
                let mut sum = [0.0; 4];
                for fy in 0..fine {
                    for fx in 0..fine {
                        let c = self.colour[(y * fine + fy) * self.side + x * fine + fx];
                        for k in 0..3 {
                            sum[k] += c[k] * c[3];
                        }
                        sum[3] += c[3];
                    }
                }
                for k in 0..3 {
                    out.push(if sum[3] > 0.0 { (sum[k] / sum[3]).round() as u8 } else { 0 });
                }
                out.push((sum[3] / (fine * fine) as f64 * 255.0).round() as u8);
            }
        }
        out
    }
}

impl Canvas {
    /// Draw the shape with corner mask `mask` filling the box from `min`,
    /// `ext` along each side, lit from over the viewer's left shoulder.
    pub fn shape(&mut self, mask: u8, min: DVec3, ext: DVec3) {
        let light = (self.towards - self.right * 0.6 + DVec3::Z * 0.5).normalize();
        let mesh = shape_mesh(mask);
        let point = |i: usize| min + ext * DVec3::new(mesh.positions[i][0] as f64, mesh.positions[i][1] as f64, mesh.positions[i][2] as f64);
        // edge lines are drawn within 0.014 of a unit of a face's outline (see
        // `triangle`), which suits one shape filling the picture; in a picture
        // of a whole form that is narrowed to about a pixel and a half
        let thin = (0.014 * self.scale / (1.5 * self.fine as f64)).max(1.0);
        for t in (0..mesh.positions.len()).step_by(3) {
            let corners = [point(t), point(t + 1), point(t + 2)];
            // a stretched box tilts its sloped faces, so the face's direction
            // is taken from the stretched triangle; the mesh's own says which
            // side of it is out
            let unit = DVec3::new(mesh.normals[t][0] as f64, mesh.normals[t][1] as f64, mesh.normals[t][2] as f64);
            let mut normal = (corners[1] - corners[0]).cross(corners[2] - corners[0]).normalize();
            if normal.dot(unit / ext) < 0.0 {
                normal = -normal;
            }
            // the mesh marks the face's middle 1 and its outline 0; the middle
            // is this far from the outline, which turns the mark into a distance
            let along = (corners[2] - corners[1]).normalize();
            let height = (corners[0] - corners[1]).cross(along).length();
            let rim = [0, 1, 2].map(|k| mesh.rim[t + k] as f64 * height * thin);
            self.triangle(corners, rim, 0.5 + 0.5 * normal.dot(light).max(0.0));
        }
    }
}
