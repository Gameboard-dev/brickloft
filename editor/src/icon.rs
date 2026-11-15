//! The application's icon, drawn in code: a brick arch on a dark tile.
//!
//! It is the same drawing as `editor/web/icon.svg`, which the browser page
//! uses. A window needs pixels, and drawing these few shapes directly is
//! smaller than bringing in an image decoder (or an SVG renderer) to read
//! one small picture.

/// The arch's mortar joints, as line segments in the icon's 64 x 64 space.
const JOINTS: [[f32; 4]; 8] = [
    [12.0, 34.0, 22.0, 34.0],
    [42.0, 34.0, 52.0, 34.0],
    [12.0, 43.5, 22.0, 43.5],
    [42.0, 43.5, 52.0, 43.5],
    [23.9, 28.1, 15.8, 22.2],
    [28.9, 24.5, 25.8, 15.0],
    [35.1, 24.5, 38.2, 15.0],
    [40.1, 28.1, 48.2, 22.2],
];

const TILE: [f32; 3] = [29.0, 32.0, 38.0];
const BRICK: [f32; 3] = [242.0, 199.0, 68.0];

/// What is at a point of the 64 x 64 drawing: a colour, or nothing.
fn at(x: f32, y: f32) -> Option<[f32; 3]> {
    // the tile: a square with corners rounded to radius 13
    let (dx, dy) = ((x - 32.0).abs() - 19.0, (y - 32.0).abs() - 19.0);
    if dx.max(0.0).hypot(dy.max(0.0)) > 13.0 {
        return None;
    }
    // the arch: half a ring about (32, 34), and two legs below it
    let r = (x - 32.0).hypot(y - 34.0);
    let ring = y <= 34.0 && (10.0..=20.0).contains(&r);
    let legs = (34.0..=53.0).contains(&y) && ((12.0..=22.0).contains(&x) || (42.0..=52.0).contains(&x));
    if !(ring || legs) {
        return Some(TILE);
    }
    let on_joint = JOINTS.iter().any(|&[ax, ay, bx, by]| {
        let (ex, ey) = (bx - ax, by - ay);
        let t = (((x - ax) * ex + (y - ay) * ey) / (ex * ex + ey * ey)).clamp(0.0, 1.0);
        (x - ax - ex * t).hypot(y - ay - ey * t) < 1.1
    });
    Some(if on_joint { TILE } else { BRICK })
}

/// The icon as `size` x `size` RGBA bytes, row by row from the top.
pub fn rgba(size: u32) -> Vec<u8> {
    const SUB: u32 = 4; // samples per pixel each way, for smooth edges
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    for py in 0..size {
        for px in 0..size {
            let (mut sum, mut covered) = ([0.0f32; 3], 0.0f32);
            for s in 0..SUB * SUB {
                let x = (px as f32 + ((s % SUB) as f32 + 0.5) / SUB as f32) * 64.0 / size as f32;
                let y = (py as f32 + ((s / SUB) as f32 + 0.5) / SUB as f32) * 64.0 / size as f32;
                if let Some(c) = at(x, y) {
                    for k in 0..3 {
                        sum[k] += c[k];
                    }
                    covered += 1.0;
                }
            }
            for k in 0..3 {
                out.push(if covered > 0.0 { (sum[k] / covered).round() as u8 } else { 0 });
            }
            out.push((255.0 * covered / (SUB * SUB) as f32).round() as u8);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_icon_is_a_yellow_arch_on_a_dark_tile_with_clear_corners() {
        let size = 64;
        let px = |x: u32, y: u32| {
            let i = ((y * size + x) * 4) as usize;
            let p = rgba(size);
            [p[i], p[i + 1], p[i + 2], p[i + 3]]
        };
        assert_eq!(px(0, 0)[3], 0, "the corner is outside the rounded tile");
        assert_eq!(px(32, 5), [29, 32, 38, 255], "tile above the arch");
        assert_eq!(px(32, 19), [242, 199, 68, 255], "the crown of the arch");
        assert_eq!(px(17, 48), [242, 199, 68, 255], "the left leg");
        assert_eq!(px(32, 40), [29, 32, 38, 255], "the opening under the arch");
        assert_eq!(px(17, 34), [29, 32, 38, 255], "a mortar joint across the left leg");
        assert_eq!(rgba(32).len(), 32 * 32 * 4);
    }
}
