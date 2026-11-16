//! Colouring a form's bricks.
//!
//! A brick is one colour, so a blend across a form is made brick by brick:
//! each takes the colour for where its centre lies. A smooth blend picks from
//! a few steps between the colours; a speckled one gives each brick one of
//! the pure colours, more often the nearer one, which reads as a texture.

use brickloft_core::shapes::CellBrick;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// one colour throughout
    Solid,
    /// a gradual change from the first colour to the last
    Blend,
    /// bands of each colour in turn
    Stripes,
    /// each brick one of the colours, weighted by position: a dithered blend
    Speckle,
}

pub const MODES: [(Mode, &str); 4] = [
    (Mode::Solid, "One colour"),
    (Mode::Blend, "Blend"),
    (Mode::Stripes, "Stripes"),
    (Mode::Speckle, "Speckled blend"),
];

/// Steps between two neighbouring colours of a blend. More would be smoother
/// and would also be more distinct colours for the game to carry; past about
/// this many the eye stops seeing bands on a form a few dozen bricks long.
const STEPS: f64 = 6.0;

#[derive(Clone, PartialEq, Debug)]
pub struct Paint {
    pub mode: Mode,
    /// one to four colours, as sRGB bytes
    pub colours: Vec<[u8; 3]>,
    /// the direction a blend or stripes run in: 0, 1, 2 for x, y, z
    pub axis: usize,
    /// how wide a stripe is, in microbricks
    pub width: i32,
}

impl Default for Paint {
    fn default() -> Self {
        Paint { mode: Mode::Solid, colours: vec![[214, 208, 196], [70, 110, 190]], axis: 2, width: 2 }
    }
}

/// A well-mixed number in 0..1 from a brick's position: the same brick always
/// gets the same one, so a speckle does not shimmer as the form is edited.
fn scatter(at: [i32; 3]) -> f64 {
    // a multiply-xorshift mix; any hash that spreads neighbours apart would do
    let mut h = (at[0] as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (at[1] as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F) ^ (at[2] as u64).wrapping_mul(0x1656_67B1_9E37_79F9);
    h ^= h >> 29;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 32;
    (h >> 11) as f64 / (1u64 << 53) as f64
}

impl Paint {
    /// The colour of `brick`, in a form occupying cells `lo..hi`. `None` only
    /// for a paint with no colours at all, which is then left to whatever
    /// colour the player has selected in the game.
    pub fn colour(&self, brick: &CellBrick, lo: [i32; 3], hi: [i32; 3]) -> Option<[u8; 3]> {
        let colours = &self.colours;
        let (count, axis) = (colours.len(), self.axis.min(2));
        if count == 0 {
            return None;
        }
        if self.mode == Mode::Solid || count == 1 {
            return Some(colours[0]);
        }
        let centre = brick.min[axis] as f64 + brick.ext[axis] as f64 / 2.0;
        // 0 at the form's low end along the axis, 1 at its high end
        let span = (hi[axis] - lo[axis]).max(1) as f64;
        let along = ((centre - lo[axis] as f64) / span).clamp(0.0, 1.0);
        // where that falls among the colours: 0 is the first, count - 1 the last
        let place = along * (count - 1) as f64;
        Some(match self.mode {
            Mode::Stripes => {
                let band = ((centre - lo[axis] as f64) / self.width.max(1) as f64).floor().max(0.0) as usize;
                colours[band % count]
            }
            Mode::Speckle => {
                // nudged by up to half a colour either way, then the nearest
                let nudged = place + scatter(brick.min) - 0.5;
                colours[(nudged.round().max(0.0) as usize).min(count - 1)]
            }
            _ => {
                let below = (place.floor() as usize).min(count - 2);
                let part = ((place - below as f64) * STEPS).round() / STEPS;
                let (a, b) = (colours[below], colours[below + 1]);
                [0, 1, 2].map(|k| (a[k] as f64 + (b[k] as f64 - a[k] as f64) * part).round() as u8)
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn brick(z: i32) -> CellBrick {
        CellBrick { mask: 0xff, min: [z * 7 % 5, 0, z], ext: [1, 1, 1] }
    }

    const RED: [u8; 3] = [200, 0, 0];
    const BLUE: [u8; 3] = [0, 0, 200];

    fn paint(mode: Mode) -> Paint {
        Paint { mode, colours: vec![RED, BLUE], axis: 2, width: 3 }
    }

    #[test]
    fn game_and_solid() {
        assert_eq!(Paint { colours: Vec::new(), ..paint(Mode::Solid) }.colour(&brick(4), [0; 3], [1, 1, 20]), None);
        assert_eq!(paint(Mode::Solid).colour(&brick(4), [0; 3], [1, 1, 20]), Some(RED));
        // one colour is one colour, whatever the mode
        let single = Paint { colours: vec![BLUE], ..paint(Mode::Blend) };
        assert_eq!(single.colour(&brick(4), [0; 3], [1, 1, 20]), Some(BLUE));
    }

    #[test]
    fn a_blend_runs_from_the_first_colour_to_the_last_in_a_few_steps() {
        let p = paint(Mode::Blend);
        let all: Vec<[u8; 3]> = (0..40).map(|z| p.colour(&brick(z), [0; 3], [1, 1, 40]).unwrap()).collect();
        assert_eq!(all[0], RED);
        assert_eq!(all[39], BLUE);
        // red only ever falls and blue only ever rises
        assert!(all.windows(2).all(|w| w[1][0] <= w[0][0] && w[1][2] >= w[0][2]));
        let distinct: std::collections::HashSet<[u8; 3]> = all.iter().copied().collect();
        assert_eq!(distinct.len(), STEPS as usize + 1);
        // three colours: the middle one is reached half way
        let three = Paint { colours: vec![RED, [0, 200, 0], BLUE], ..p };
        assert_eq!(three.colour(&brick(20), [0; 3], [1, 1, 41]), Some([0, 200, 0]));
    }

    #[test]
    fn stripes_take_each_colour_in_turn() {
        let p = paint(Mode::Stripes);
        let at = |z| p.colour(&brick(z), [0; 3], [1, 1, 40]).unwrap();
        assert_eq!([at(0), at(1), at(2), at(3), at(5), at(6)], [RED, RED, RED, BLUE, BLUE, RED]);
    }

    #[test]
    fn a_speckle_uses_only_the_pure_colours_and_leans_the_right_way() {
        let p = paint(Mode::Speckle);
        let all: Vec<[u8; 3]> = (0..400).map(|z| p.colour(&brick(z), [0; 3], [1, 1, 400]).unwrap()).collect();
        assert!(all.iter().all(|c| *c == RED || *c == BLUE));
        let blue_in = |range: std::ops::Range<usize>| all[range].iter().filter(|c| **c == BLUE).count();
        // almost none at the red end and almost all at the blue end
        assert!(blue_in(0..40) <= 4, "{} blue in the first 40", blue_in(0..40));
        assert!(blue_in(360..400) >= 36, "{} blue in the last 40", blue_in(360..400));
        let middle = blue_in(160..240);
        assert!((25..=55).contains(&middle), "{middle} of 80 blue in the middle");
        // the same brick is always the same colour
        assert_eq!(p.colour(&brick(200), [0; 3], [1, 1, 400]), p.colour(&brick(200), [0; 3], [1, 1, 400]));
    }
}
