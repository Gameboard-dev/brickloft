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
    /// each separate piece of the form one of the colours: the stones of a path
    Patches,
    /// each brick the colour its pattern gives it: the tiles of a mosaic
    Pattern,
}

pub const MODES: [(Mode, &str); 6] = [
    (Mode::Solid, "One colour"),
    (Mode::Blend, "Blend"),
    (Mode::Stripes, "Stripes"),
    (Mode::Speckle, "Speckled blend"),
    (Mode::Patches, "Each piece its own"),
    (Mode::Pattern, "As the pattern has it"),
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

/// The colours a path of stones starts in: four browns and greys of
/// weathered stone.
pub const STONE: [[u8; 3]; 4] = [[168, 140, 120], [146, 122, 104], [190, 160, 130], [124, 110, 98]];

/// The colours a mosaic starts in: cream for the ground, then terracotta,
/// a deep blue and ochre, which the features of its pattern share (see
/// `brickloft_core::pattern::colour_of`). Four and not one for each of a
/// pattern's seven features: a floor reads as one design in a few colours,
/// and anyone who wants each feature picked out adds colours to the list.
pub const MOSAIC: [[u8; 3]; 4] = [[226, 214, 190], [176, 84, 60], [44, 74, 122], [204, 160, 72]];

/// The colours a rug starts in: a deep red ground, then gold, a dark
/// blue, ivory, a warm red, leaf green, a dark green and saffron, one for
/// each feature of the rug (see `brickloft_core::pattern::rug_garden`).
pub const RUG: [[u8; 3]; 8] = [[112, 22, 30], [212, 165, 70], [24, 50, 92], [236, 220, 180], [198, 74, 44], [62, 110, 62], [20, 72, 60], [240, 196, 96]];

/// The colours a floor of planks starts in: eight browns a little apart,
/// one for each number a board may have.
pub const WOOD: [[u8; 3]; 8] = [[150, 104, 64], [133, 90, 54], [166, 118, 76], [120, 80, 48], [158, 110, 66], [140, 98, 62], [172, 126, 84], [128, 88, 56]];

/// The sets of colours a mosaic of a style may be given, the first being
/// the set it starts in. Asking for another seed takes the next set too,
/// so that a new pattern comes in new colours. They are sets chosen to go
/// together, and not colours drawn at random, which seldom do.
///
/// In every set the first colour is the ground and the rest are for the
/// pattern's features (see `brickloft_core::pattern::colour_of`).
pub fn palettes(style: &str) -> &'static [&'static [[u8; 3]]] {
    match style {
        // a deep red ground; dark blue; teal; ivory; black and gold
        "rug" => &[
            &RUG,
            &[[20, 36, 72], [212, 165, 70], [150, 30, 36], [236, 220, 180], [198, 74, 44], [62, 110, 62], [110, 22, 30], [240, 196, 96]],
            &[[16, 70, 78], [226, 200, 140], [170, 44, 36], [244, 228, 190], [214, 120, 60], [200, 160, 70], [120, 24, 30], [250, 210, 120]],
            &[[236, 222, 186], [120, 24, 30], [24, 50, 92], [196, 150, 60], [170, 44, 36], [62, 110, 62], [20, 60, 60], [212, 165, 70]],
            &[[26, 18, 20], [212, 165, 70], [112, 22, 30], [236, 220, 180], [176, 60, 40], [90, 110, 60], [70, 14, 24], [240, 196, 96]],
        ],
        // browns; pale ash; ebony; painted blue; weathered grey; mahogany
        "planks" => &[
            &WOOD,
            &[[226, 214, 196], [214, 200, 180], [234, 224, 208], [204, 190, 170], [220, 208, 190], [210, 196, 176], [238, 230, 216], [198, 184, 164]],
            &[[44, 40, 38], [36, 32, 30], [54, 50, 46], [30, 27, 25], [48, 44, 40], [40, 36, 34], [58, 54, 50], [34, 30, 28]],
            &[[70, 98, 128], [58, 84, 112], [84, 112, 142], [50, 74, 100], [76, 104, 134], [64, 90, 120], [92, 120, 150], [54, 80, 106]],
            &[[140, 138, 132], [124, 122, 116], [154, 152, 146], [112, 110, 104], [146, 144, 138], [132, 130, 124], [162, 160, 154], [118, 116, 110]],
            &[[120, 52, 36], [104, 44, 30], [134, 62, 42], [92, 38, 26], [126, 56, 38], [112, 48, 32], [142, 68, 46], [98, 42, 28]],
        ],
        // pale mortar and red brick; yellow stock brick; dark engineering brick; whitewashed
        "bricks" => &[
            &[[196, 190, 178], [150, 62, 44], [136, 54, 40], [162, 72, 50], [124, 48, 36], [156, 66, 46], [142, 58, 42], [170, 78, 54]],
            &[[160, 154, 140], [196, 168, 104], [182, 152, 92], [206, 180, 118], [170, 140, 84], [200, 172, 110], [188, 160, 98], [212, 186, 126]],
            &[[150, 148, 144], [72, 70, 80], [62, 60, 70], [82, 80, 90], [56, 54, 62], [76, 74, 84], [66, 64, 74], [88, 86, 96]],
            &[[176, 172, 164], [236, 232, 224], [226, 222, 212], [242, 238, 230], [218, 214, 204], [232, 228, 218], [222, 218, 208], [246, 242, 236]],
        ],
        // dark mortar and grey stone; sandstone; slate
        "stones" => &[
            &[[70, 68, 64], [150, 146, 138], [132, 128, 120], [164, 160, 150], [120, 116, 108], [156, 150, 140], [140, 136, 126], [172, 166, 154]],
            &[[96, 82, 64], [204, 176, 130], [190, 160, 116], [214, 188, 144], [178, 148, 106], [208, 180, 136], [196, 166, 122], [220, 196, 152]],
            &[[40, 44, 52], [96, 108, 124], [84, 96, 112], [108, 120, 136], [74, 86, 100], [100, 112, 128], [90, 102, 118], [114, 126, 142]],
        ],
        // ice on a night sky; pale blues; white on slate
        "snowflake" => &[
            &[[24, 44, 84], [240, 246, 252], [170, 205, 235], [120, 170, 220], [210, 230, 245], [90, 140, 200], [255, 255, 255], [150, 190, 230]],
            &[[232, 240, 248], [60, 110, 170], [110, 160, 210], [30, 70, 130], [150, 190, 225], [80, 130, 190], [20, 50, 100], [130, 175, 220]],
            &[[52, 58, 68], [250, 250, 252], [200, 210, 222], [228, 234, 242], [176, 188, 204], [240, 244, 248], [255, 255, 255], [214, 222, 232]],
        ],
        // cream with terracotta, blue and ochre; blue and green tiles; black, grey and gold; night blue; terracotta; greens
        _ => &[
            &MOSAIC,
            &[[240, 236, 224], [22, 82, 120], [30, 120, 90], [220, 160, 40]],
            &[[236, 232, 224], [30, 30, 34], [150, 150, 150], [190, 150, 60]],
            &[[28, 40, 60], [230, 224, 206], [190, 150, 70], [120, 160, 180]],
            &[[176, 84, 60], [236, 220, 190], [44, 74, 122], [40, 40, 44]],
            &[[226, 232, 214], [40, 92, 70], [150, 180, 90], [196, 120, 50]],
        ],
    }
}

/// The colours a mosaic of a style starts in.
pub fn palette(style: &str) -> &'static [[u8; 3]] {
    palettes(style)[0]
}

/// The most colours a paint may have: one for each number a pattern may
/// give a brick.
pub const MOST_COLOURS: usize = 8;

impl Paint {
    /// The colour of a brick whose pattern gives it colour number `shade`
    /// (see `brickloft_core::spec::Built::shades`): the first colour for
    /// the pattern's ground, and the rest shared among its features (see
    /// `brickloft_core::pattern::colour_of`). `None` for a paint with no
    /// colours.
    pub fn shade(&self, shade: u8) -> Option<[u8; 3]> {
        (!self.colours.is_empty()).then(|| self.colours[brickloft_core::pattern::colour_of(shade, self.colours.len())])
    }

    /// The colour of separate piece number `piece` of a form (see
    /// `brickloft_core::pattern::pieces`): one of the colours, the same one
    /// for that piece every time. `None` for a paint with no colours.
    pub fn patch(&self, piece: u32) -> Option<[u8; 3]> {
        let count = self.colours.len();
        (count > 0).then(|| self.colours[((scatter([piece as i32, 7, 13]) * count as f64) as usize).min(count - 1)])
    }

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
