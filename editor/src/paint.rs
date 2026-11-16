//! Colouring a form's bricks.
//!
//! A brick is one colour, so a blend across a form is made brick by brick:
//! each takes the colour for where its centre lies. A smooth blend picks from
//! a few steps between the colours. A pattern's bricks are coloured by the
//! numbers the pattern gives them.

use brickloft_core::shapes::CellBrick;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// one colour throughout
    Solid,
    /// a gradual change from the first colour to the last
    Blend,
    /// bands of each colour in turn
    Stripes,
    /// each brick the colour its pattern gives it: the tiles of a mosaic, the stones of a path
    Pattern,
}

pub const MODES: [(Mode, &str); 4] = [
    (Mode::Solid, "One colour"),
    (Mode::Blend, "Blend"),
    (Mode::Stripes, "Stripes"),
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

/// The colours a path of stones starts in: four browns and greys of
/// weathered stone.
pub const STONE: [[u8; 3]; 4] = [[168, 140, 120], [146, 122, 104], [190, 160, 130], [124, 110, 98]];

/// The colours a mosaic starts in: cream for the ground, then terracotta,
/// a deep blue and ochre, which the features of its pattern share (see
/// `brickloft_core::pattern::colour_of`). Four and not one for each of a
/// pattern's seven features: a floor reads as one design in a few colours,
/// and anyone who wants each feature picked out adds colours to the list.
pub const MOSAIC: [[u8; 3]; 4] = [[226, 214, 190], [176, 84, 60], [44, 74, 122], [204, 160, 72]];

/// The colours a rug starts in: a deep red ground, then old gold, a dark
/// blue, ivory, a warm red, leaf green, a dark green and saffron, one for
/// each feature of the rug (see `brickloft_core::pattern::rug_garden`):
/// its lines, its dark ground and its pale one, petals, leaves, the
/// shaded side of a leaf and the ground of the corners, and the hearts of
/// flowers. The gold is darker than the saffron, so that a heart shows
/// inside an outline, and the leaf green light enough to show on the red.
/// In every set the second colour must show on all the others, being the
/// outline of everything, and the third and fourth on each other and on
/// the first, being grounds that lie side by side.
pub const RUG: [[u8; 3]; 8] = [[112, 22, 30], [200, 150, 56], [24, 50, 92], [236, 220, 180], [198, 74, 44], [96, 140, 72], [24, 84, 66], [244, 204, 104]];

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
            &[[20, 36, 72], [200, 150, 56], [140, 28, 34], [236, 220, 180], [198, 74, 44], [96, 140, 72], [30, 96, 80], [244, 204, 104]],
            &[[16, 70, 78], [226, 200, 140], [170, 44, 36], [244, 228, 190], [214, 120, 60], [200, 160, 70], [120, 24, 30], [250, 210, 120]],
            &[[236, 222, 186], [120, 24, 30], [24, 50, 92], [206, 160, 70], [170, 44, 36], [62, 110, 62], [20, 60, 60], [226, 132, 52]],
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
        // red brick in pale mortar; yellow stock brick; dark engineering brick; whitewash
        "bricks" => &[
            &[[196, 190, 178], [150, 62, 44], [136, 54, 40], [162, 72, 50], [124, 48, 36], [156, 66, 46], [142, 58, 42], [170, 78, 54]],
            &[[160, 154, 140], [196, 168, 104], [182, 152, 92], [206, 180, 118], [170, 140, 84], [200, 172, 110], [188, 160, 98], [212, 186, 126]],
            &[[150, 148, 144], [72, 70, 80], [62, 60, 70], [82, 80, 90], [56, 54, 62], [76, 74, 84], [66, 64, 74], [88, 86, 96]],
            &[[176, 172, 164], [236, 232, 224], [226, 222, 212], [242, 238, 230], [218, 214, 204], [232, 228, 218], [222, 218, 208], [246, 242, 236]],
        ],
        // grey stone in dark mortar; sandstone; slate
        "cobblestone" => &[
            &[[70, 68, 64], [150, 146, 138], [132, 128, 120], [164, 160, 150], [120, 116, 108], [156, 150, 140], [140, 136, 126], [172, 166, 154]],
            &[[96, 82, 64], [204, 176, 130], [190, 160, 116], [214, 188, 144], [178, 148, 106], [208, 180, 136], [196, 166, 122], [220, 196, 152]],
            &[[40, 44, 52], [96, 108, 124], [84, 96, 112], [108, 120, 136], [74, 86, 100], [100, 112, 128], [90, 102, 118], [114, 126, 142]],
        ],
        // A tree's colours are for its parts, in the order of
        // `brickloft_core::tree`'s numbers: the trunk, leaves, leaves in
        // shade, new growth, limbs, the top of a pine's boughs or the rib
        // of a frond, and two for its extras (baubles and a star,
        // coconuts and the fibre under a palm's crown).
        // spruce green; under snow; a blue spruce; a fir in frost
        "pine" => &[
            &[[86, 60, 42], [36, 90, 54], [22, 62, 42], [72, 126, 66], [104, 76, 52], [52, 108, 60], [190, 38, 44], [238, 196, 70]],
            &[[80, 58, 44], [30, 78, 54], [20, 56, 42], [48, 100, 62], [98, 74, 54], [240, 244, 248], [190, 38, 44], [238, 196, 70]],
            &[[90, 70, 56], [70, 112, 108], [48, 84, 86], [112, 150, 140], [108, 86, 68], [92, 132, 124], [200, 200, 210], [70, 110, 190]],
            &[[70, 52, 42], [34, 70, 56], [22, 50, 44], [150, 184, 170], [88, 68, 54], [214, 228, 228], [176, 30, 40], [240, 240, 244]],
        ],
        // a coconut palm; a paler, dustier one; at sunset
        "palm" => &[
            &[[150, 126, 98], [58, 126, 56], [42, 98, 48], [112, 162, 64], [112, 92, 70], [146, 158, 72], [92, 64, 38], [124, 96, 62]],
            &[[168, 150, 124], [96, 136, 78], [72, 110, 66], [150, 172, 96], [132, 114, 92], [170, 170, 104], [70, 110, 50], [140, 112, 78]],
            &[[112, 84, 66], [44, 96, 60], [30, 70, 52], [150, 150, 60], [84, 62, 50], [176, 140, 64], [70, 48, 32], [100, 72, 50]],
        ],
        // an oak in summer; in early autumn; in late autumn; in spring
        "oak" => &[
            &[[82, 64, 48], [70, 116, 46], [46, 88, 38], [112, 152, 60], [100, 80, 60], [112, 152, 60], [120, 90, 50], [150, 120, 70]],
            &[[80, 62, 48], [204, 142, 40], [168, 92, 32], [226, 186, 64], [98, 78, 60], [226, 186, 64], [120, 90, 50], [150, 120, 70]],
            &[[70, 54, 44], [170, 62, 32], [120, 40, 30], [214, 120, 40], [88, 68, 54], [214, 120, 40], [120, 90, 50], [150, 120, 70]],
            &[[90, 72, 54], [130, 174, 70], [86, 136, 58], [176, 204, 96], [108, 88, 66], [176, 204, 96], [120, 90, 50], [150, 120, 70]],
        ],
        // A bush and a flower use the same eight numbers: stems, three
        // greens, canes or the green back of a flower's head, petals, a
        // second colour of petal or the outer seeds, and the heart.
        // box green; a dark yew; golden privet
        "boxwood" => &[
            &[[92, 74, 54], [58, 110, 52], [38, 82, 42], [96, 142, 62], [106, 88, 64], [96, 142, 62], [200, 40, 44], [240, 210, 90]],
            &[[70, 56, 46], [36, 80, 50], [24, 58, 40], [60, 108, 62], [84, 68, 54], [60, 108, 62], [206, 44, 40], [240, 210, 90]],
            &[[100, 80, 56], [150, 164, 60], [108, 130, 50], [198, 196, 84], [112, 92, 66], [198, 196, 84], [232, 128, 36], [240, 210, 90]],
        ],
        // a beech in summer; in its copper autumn; in fresh spring green; a copper beech
        "beech" => &[
            &[[150, 152, 148], [60, 112, 50], [40, 84, 42], [104, 150, 64], [126, 128, 122], [104, 150, 64], [120, 90, 50], [150, 120, 70]],
            &[[146, 148, 144], [190, 112, 40], [150, 78, 30], [222, 160, 56], [122, 124, 118], [222, 160, 56], [120, 90, 50], [150, 120, 70]],
            &[[156, 158, 152], [136, 184, 70], [96, 148, 60], [186, 214, 100], [132, 134, 128], [186, 214, 100], [120, 90, 50], [150, 120, 70]],
            &[[140, 140, 138], [108, 44, 56], [76, 30, 44], [150, 66, 70], [118, 118, 114], [150, 66, 70], [120, 90, 50], [150, 120, 70]],
        ],
        // a box hedge with red berries; a dark yew; golden privet with orange berries
        "hedge" => &[
            &[[92, 74, 54], [58, 110, 52], [38, 82, 42], [96, 142, 62], [106, 88, 64], [96, 142, 62], [200, 40, 44], [240, 210, 90]],
            &[[70, 56, 46], [36, 80, 50], [24, 58, 40], [60, 108, 62], [84, 68, 54], [60, 108, 62], [206, 44, 40], [240, 210, 90]],
            &[[100, 80, 56], [150, 164, 60], [108, 130, 50], [198, 196, 84], [112, 92, 66], [198, 196, 84], [232, 128, 36], [240, 210, 90]],
        ],
        // A cherry: dark bark; blossom in a middle tone, a deep one and a
        // pale one; dark limbs; a bronze young leaf; dark buds; and the
        // pale bands on its trunk. Pale pink; deep pink; white; mixed
        // pinks and white; and out of blossom, in summer leaf and in
        // autumn, where the same parts are leaves.
        "cherry" => &[
            &[[62, 44, 42], [246, 196, 212], [232, 150, 178], [252, 236, 240], [52, 38, 38], [150, 110, 70], [176, 60, 100], [124, 96, 88]],
            &[[62, 44, 42], [236, 130, 170], [208, 84, 136], [248, 190, 210], [52, 38, 38], [146, 100, 66], [150, 40, 86], [124, 96, 88]],
            &[[62, 44, 42], [250, 244, 244], [238, 214, 222], [255, 252, 250], [52, 38, 38], [156, 126, 80], [214, 150, 170], [124, 96, 88]],
            &[[62, 44, 42], [244, 170, 196], [214, 96, 146], [254, 246, 246], [52, 38, 38], [150, 110, 70], [168, 50, 94], [124, 96, 88]],
            &[[66, 48, 44], [78, 130, 58], [54, 100, 48], [116, 162, 72], [56, 42, 40], [140, 170, 80], [60, 90, 44], [128, 100, 90]],
            &[[66, 48, 44], [214, 120, 44], [176, 60, 36], [236, 176, 60], [56, 42, 40], [200, 150, 50], [140, 40, 30], [128, 100, 90]],
        ],
        // a birch: white bark, its marks the seventh colour; autumn gold; spring
        "birch" => &[
            &[[232, 230, 222], [92, 142, 60], [66, 112, 50], [136, 176, 76], [70, 62, 56], [136, 176, 76], [44, 40, 38], [150, 120, 70]],
            &[[232, 230, 222], [222, 178, 50], [190, 140, 40], [240, 208, 84], [70, 62, 56], [240, 208, 84], [44, 40, 38], [150, 120, 70]],
            &[[236, 234, 226], [140, 186, 80], [104, 152, 66], [184, 214, 108], [74, 66, 60], [184, 214, 108], [48, 44, 42], [150, 120, 70]],
        ],
        // a willow: grey-green; the gold of its shoots in spring; autumn
        "willow" => &[
            &[[96, 84, 66], [108, 146, 70], [78, 116, 58], [156, 184, 96], [110, 96, 74], [156, 184, 96], [120, 90, 50], [150, 120, 70]],
            &[[96, 84, 66], [150, 172, 70], [112, 140, 58], [204, 204, 96], [112, 98, 74], [204, 204, 96], [120, 90, 50], [150, 120, 70]],
            &[[92, 80, 64], [196, 170, 60], [160, 130, 48], [224, 204, 96], [106, 92, 72], [224, 204, 96], [120, 90, 50], [150, 120, 70]],
        ],
        // a date palm: grey-green fronds and orange dates; a dustier one
        "datepalm" => &[
            &[[128, 104, 78], [86, 122, 84], [62, 96, 68], [124, 152, 100], [98, 78, 58], [150, 150, 96], [214, 130, 40], [120, 92, 60]],
            &[[150, 130, 104], [110, 136, 100], [84, 110, 84], [146, 164, 120], [118, 100, 80], [166, 162, 116], [196, 112, 44], [136, 110, 78]],
        ],
        // a Scots pine: grey bark below, orange limbs above, blue-green needles; under snow
        "scotspine" => &[
            &[[104, 88, 76], [50, 100, 76], [34, 76, 60], [84, 132, 94], [192, 112, 62], [84, 132, 94], [120, 90, 50], [150, 120, 70]],
            &[[100, 86, 76], [44, 88, 72], [30, 66, 56], [236, 240, 244], [184, 108, 62], [236, 240, 244], [120, 90, 50], [150, 120, 70]],
        ],
        // a cypress: dark green; a blue juniper; golden
        "cypress" => &[
            &[[96, 76, 58], [38, 84, 54], [24, 60, 42], [62, 110, 64], [96, 76, 58], [62, 110, 64], [120, 90, 50], [150, 120, 70]],
            &[[100, 84, 70], [78, 116, 118], [54, 88, 94], [118, 152, 150], [100, 84, 70], [118, 152, 150], [120, 90, 50], [150, 120, 70]],
            &[[100, 80, 58], [150, 160, 56], [110, 126, 48], [200, 196, 80], [100, 80, 58], [200, 196, 80], [120, 90, 50], [150, 120, 70]],
        ],
        // a yucca: green blades and cream bells; a blue agave; yellow-edged
        "yucca" => &[
            &[[112, 96, 72], [78, 128, 76], [56, 100, 62], [118, 160, 96], [128, 150, 96], [244, 238, 214], [200, 80, 90], [240, 210, 90]],
            &[[112, 100, 84], [92, 132, 136], [66, 104, 112], [132, 166, 166], [120, 150, 140], [244, 238, 214], [200, 80, 90], [240, 210, 90]],
            &[[112, 96, 72], [86, 136, 70], [60, 104, 56], [222, 212, 110], [128, 150, 96], [244, 238, 214], [200, 80, 90], [240, 210, 90]],
        ],
        // daisies: white rays round yellow; pink; yellow
        "daisy" => &[
            &[[96, 138, 64], [72, 126, 62], [52, 100, 50], [112, 158, 78], [88, 128, 60], [248, 248, 244], [244, 200, 40], [200, 150, 30]],
            &[[96, 138, 64], [72, 126, 62], [52, 100, 50], [112, 158, 78], [88, 128, 60], [240, 150, 180], [244, 200, 40], [200, 150, 30]],
            &[[96, 138, 64], [72, 126, 62], [52, 100, 50], [112, 158, 78], [88, 128, 60], [250, 220, 60], [150, 100, 40], [110, 70, 30]],
        ],
        // daffodils: all yellow; white with a yellow trumpet; white with an orange one
        "daffodil" => &[
            &[[98, 140, 70], [74, 128, 72], [54, 102, 60], [114, 160, 90], [110, 150, 70], [250, 220, 50], [244, 178, 24], [200, 130, 20]],
            &[[98, 140, 70], [74, 128, 72], [54, 102, 60], [114, 160, 90], [110, 150, 70], [250, 248, 238], [248, 210, 50], [220, 150, 20]],
            &[[98, 140, 70], [74, 128, 72], [54, 102, 60], [114, 160, 90], [110, 150, 70], [250, 248, 238], [240, 120, 30], [200, 80, 20]],
        ],
        // a bed: petals, a second colour (trumpets, inner petals, discs) and hearts
        "bed" => &[
            &[[98, 140, 70], [74, 128, 72], [54, 102, 60], [114, 160, 90], [100, 140, 66], [240, 96, 110], [250, 214, 60], [250, 244, 230]],
            &[[98, 140, 70], [74, 128, 72], [54, 102, 60], [114, 160, 90], [100, 140, 66], [250, 246, 240], [240, 150, 180], [248, 210, 80]],
            &[[98, 140, 70], [74, 128, 72], [54, 102, 60], [114, 160, 90], [100, 140, 66], [230, 60, 40], [250, 170, 30], [250, 226, 80]],
        ],
        // grass: its soil first, then three greens, then its pond: the bed (and the heads of reeds), the water, sand and stones. A lawn; a meadow; dry
        "grass" => &[
            &[[92, 70, 46], [76, 150, 58], [54, 122, 48], [112, 176, 72], [70, 54, 40], [70, 130, 190], [214, 196, 150], [132, 130, 124]],
            &[[100, 78, 50], [96, 148, 62], [70, 118, 54], [150, 176, 86], [74, 58, 42], [64, 120, 170], [206, 190, 146], [126, 124, 118]],
            &[[132, 106, 70], [176, 164, 96], [146, 132, 78], [204, 192, 124], [96, 76, 52], [84, 130, 160], [222, 204, 160], [140, 136, 128]],
            // autumn: tawny and russet with some green left, and the reeds' heads dark; winter: frosted blades, pale sand and stones, and the pond ice
            &[[96, 72, 46], [176, 140, 62], [150, 96, 44], [134, 150, 72], [84, 58, 36], [72, 112, 150], [204, 184, 140], [128, 122, 114]],
            &[[120, 104, 88], [214, 226, 222], [168, 186, 180], [240, 246, 248], [150, 130, 96], [206, 228, 240], [226, 222, 208], [176, 178, 182]],
        ],
        // hydrangeas: blue; pink; white; blue running to pink, as on middling ground
        "hydrangea" => &[
            &[[96, 78, 56], [58, 112, 56], [38, 82, 44], [98, 146, 66], [106, 88, 64], [92, 130, 214], [120, 110, 206], [236, 236, 246]],
            &[[96, 78, 56], [58, 112, 56], [38, 82, 44], [98, 146, 66], [106, 88, 64], [232, 130, 172], [214, 100, 150], [250, 232, 238]],
            &[[96, 78, 56], [58, 112, 56], [38, 82, 44], [98, 146, 66], [106, 88, 64], [246, 246, 240], [226, 236, 214], [200, 214, 150]],
            &[[96, 78, 56], [58, 112, 56], [38, 82, 44], [98, 146, 66], [106, 88, 64], [98, 132, 216], [226, 124, 176], [240, 236, 246]],
        ],
        // petunias: purple with a dark throat; pink; white with a yellow throat; red; a striped blue and white
        "petunia" => &[
            &[[84, 116, 58], [62, 118, 58], [42, 90, 46], [100, 148, 70], [96, 136, 72], [150, 60, 180], [70, 20, 96], [240, 220, 120]],
            &[[84, 116, 58], [62, 118, 58], [42, 90, 46], [100, 148, 70], [96, 136, 72], [240, 120, 170], [170, 40, 100], [240, 220, 120]],
            &[[84, 116, 58], [62, 118, 58], [42, 90, 46], [100, 148, 70], [96, 136, 72], [248, 246, 240], [232, 206, 80], [240, 220, 120]],
            &[[84, 116, 58], [62, 118, 58], [42, 90, 46], [100, 148, 70], [96, 136, 72], [206, 30, 44], [110, 14, 28], [240, 220, 120]],
            &[[84, 116, 58], [62, 118, 58], [42, 90, 46], [100, 148, 70], [96, 136, 72], [80, 90, 200], [244, 244, 250], [240, 220, 120]],
        ],
        // bulrushes in summer; in autumn straw; common reed with purple plumes
        "reeds" => &[
            &[[96, 130, 62], [84, 132, 60], [58, 104, 50], [124, 160, 76], [96, 130, 62], [150, 110, 130], [96, 60, 36], [176, 150, 90]],
            &[[176, 156, 100], [186, 166, 104], [150, 130, 80], [206, 190, 126], [176, 156, 100], [200, 186, 150], [84, 52, 32], [150, 126, 80]],
            &[[104, 136, 70], [90, 136, 66], [62, 108, 54], [130, 164, 82], [104, 136, 70], [124, 84, 124], [96, 60, 36], [176, 150, 90]],
        ],
        // peonies: pink; deep red; white; coral; a pale blush
        "peony" => &[
            &[[92, 124, 60], [62, 116, 60], [42, 88, 48], [100, 146, 72], [84, 110, 56], [236, 150, 178], [222, 108, 150], [246, 210, 90]],
            &[[86, 116, 58], [58, 108, 58], [40, 82, 46], [94, 138, 70], [80, 104, 54], [176, 28, 60], [140, 20, 50], [244, 200, 80]],
            &[[94, 126, 62], [64, 118, 62], [44, 90, 50], [102, 148, 74], [86, 112, 58], [250, 246, 238], [240, 228, 214], [240, 206, 90]],
            &[[92, 124, 60], [62, 116, 60], [42, 88, 48], [100, 146, 72], [84, 110, 56], [250, 128, 100], [236, 96, 84], [248, 214, 100]],
            &[[92, 124, 60], [62, 116, 60], [42, 88, 48], [100, 146, 72], [84, 110, 56], [250, 220, 222], [244, 190, 202], [246, 214, 110]],
        ],
        // A single flower head's colours are for its parts, in the order
        // of `brickloft_core::tree::bloom`'s numbers: stem, sepals, the
        // throat of a petal, its lip, the swelling under the bloom, the
        // outer petals, the inner petals, and the stamens or disc. The
        // four tones of petal run from dark in the throat to light on the
        // lip in every set but the last, so that each petal's edge shows
        // against the one behind it.
        // red; pink; white; yellow; cream edged with red
        "flower" => &[
            &[[74, 110, 54], [70, 124, 60], [92, 6, 22], [228, 56, 70], [84, 118, 56], [178, 18, 38], [140, 10, 30], [244, 200, 80]],
            &[[74, 110, 54], [70, 124, 60], [196, 84, 124], [250, 200, 214], [84, 118, 56], [240, 150, 178], [222, 112, 150], [246, 210, 90]],
            &[[74, 110, 54], [70, 124, 60], [226, 214, 190], [255, 253, 248], [84, 118, 56], [246, 242, 232], [238, 228, 206], [244, 200, 70]],
            &[[74, 110, 54], [70, 124, 60], [214, 150, 30], [252, 232, 130], [84, 118, 56], [246, 208, 60], [236, 180, 40], [176, 104, 30]],
            &[[74, 110, 54], [70, 124, 60], [240, 214, 150], [206, 36, 60], [84, 118, 56], [250, 236, 200], [246, 226, 170], [244, 200, 80]],
        ],
        // a yellow sunflower; a red-bronze one; a pale lemon one
        "sunflower" => &[
            &[[92, 132, 56], [70, 124, 52], [48, 96, 44], [112, 156, 66], [84, 118, 50], [246, 196, 30], [150, 100, 40], [70, 46, 28]],
            &[[86, 122, 54], [66, 116, 52], [46, 90, 44], [104, 146, 64], [80, 110, 50], [176, 70, 30], [230, 160, 40], [50, 32, 24]],
            &[[100, 138, 62], [78, 130, 58], [54, 100, 48], [120, 162, 72], [92, 124, 56], [250, 232, 130], [170, 130, 60], [90, 64, 36]],
        ],
        // red tulips; yellow; pink; purple; white; orange flamed with red
        "tulip" => &[
            &[[104, 146, 70], [72, 128, 76], [52, 104, 64], [116, 160, 96], [90, 130, 70], [204, 34, 40], [176, 24, 34], [40, 30, 30]],
            &[[104, 146, 70], [72, 128, 76], [52, 104, 64], [116, 160, 96], [90, 130, 70], [248, 208, 40], [238, 180, 30], [60, 50, 30]],
            &[[104, 146, 70], [72, 128, 76], [52, 104, 64], [116, 160, 96], [90, 130, 70], [240, 150, 180], [226, 110, 150], [60, 40, 50]],
            &[[100, 140, 70], [70, 124, 76], [50, 100, 64], [112, 156, 96], [88, 126, 70], [110, 50, 140], [84, 36, 112], [30, 24, 40]],
            &[[104, 146, 70], [72, 128, 76], [52, 104, 64], [116, 160, 96], [90, 130, 70], [246, 244, 236], [232, 228, 214], [200, 180, 60]],
            &[[104, 146, 70], [72, 128, 76], [52, 104, 64], [116, 160, 96], [90, 130, 70], [240, 130, 30], [206, 50, 30], [50, 34, 26]],
        ],
        // A bookcase's colours are for its parts, in the order of
        // `brickloft_core::furniture`'s numbers: the wood of the carcass,
        // a second tone of it for the back panel, and six the books and
        // ornaments share. The back is the darker of the two woods in
        // every set but the black one, as the inside of a bookcase is in
        // shadow; and no book is close to either wood, or it would be
        // lost against the back.
        // oak with mixed cloth spines; dark walnut with leather; white with pastels; black with brights
        "bookcase" => &[
            &[[178, 138, 90], [146, 110, 70], [140, 40, 42], [38, 72, 112], [44, 98, 68], [204, 166, 72], [230, 220, 198], [66, 48, 44]],
            &[[78, 52, 38], [56, 38, 28], [124, 30, 34], [34, 74, 52], [150, 104, 60], [196, 160, 84], [36, 44, 78], [112, 64, 40]],
            &[[240, 238, 232], [214, 212, 204], [244, 182, 194], [170, 206, 232], [184, 224, 192], [250, 230, 160], [206, 186, 228], [250, 200, 162]],
            &[[36, 36, 40], [58, 58, 64], [228, 52, 52], [250, 200, 36], [40, 150, 230], [52, 190, 104], [240, 122, 32], [240, 240, 240]],
        ],
        // Light browns and greys of weathered stone, mixed; then the browns
        // alone; then the greys alone. Eight tones to a set, so that the
        // stones of a path differ as real ones do. (A sandstone yellow
        // and a slate blue were tried and were no colours for a path.)
        "stepping stones" => &[
            &[[168, 140, 120], [150, 148, 144], [190, 160, 130], [128, 126, 122], [146, 122, 104], [170, 168, 162], [178, 150, 124], [112, 110, 106]],
            &[[168, 140, 120], [146, 122, 104], [190, 160, 130], [124, 110, 98], [178, 150, 124], [156, 132, 110], [200, 172, 142], [136, 118, 102]],
            &[[150, 148, 144], [128, 126, 122], [170, 168, 162], [112, 110, 106], [160, 158, 152], [138, 136, 132], [180, 178, 172], [120, 118, 114]],
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

/// The colour the strip that marks a maze's way through starts in: a
/// yellow that shows on brick red, stone grey, wood brown and hedge green
/// alike.
pub const WAY: [u8; 3] = [246, 204, 46];

/// The sets of colours a maze may be given, by the texture of its walls:
/// `none` for plain walls, `hedge`, or a pattern's name.
///
/// Plain walls have three colours: the floor, the walls and the strip
/// that marks the way through. Every other texture has its own sets
/// (see [`palettes`]), spread over eight colours as its features share
/// them, with the first for the floor too (it is the mortar's, or a
/// hedge's stems') and the last given to the strip.
pub fn maze_palettes(texture: &str) -> Vec<Vec<[u8; 3]>> {
    if texture == "none" {
        // pale stone on grey; sandstone; dark stone
        return vec![vec![[128, 124, 116], [214, 208, 196], WAY], vec![[150, 126, 98], [214, 188, 144], WAY], vec![[96, 98, 104], [62, 64, 72], WAY]];
    }
    palettes(texture)
        .iter()
        .map(|set| {
            let mut colours: Vec<[u8; 3]> = (0..MOST_COLOURS as u8).map(|shade| set[brickloft_core::pattern::colour_of(shade, set.len())]).collect();
            colours[MOST_COLOURS - 1] = WAY;
            colours
        })
        .collect()
}

/// What a maze's walls are, as the name its colours go by (see
/// [`maze_palettes`]): `hedge`, the name of the pattern wrapped along
/// them, or `none` for plain walls.
pub fn maze_texture(spec: &brickloft_core::spec::Spec) -> &'static str {
    match spec.mosaic {
        _ if spec.maze.hedge => "hedge",
        Some(laid) if brickloft_core::maze::walls::wrapped(&laid) => brickloft_core::pattern::STYLES[laid.style].name,
        _ => "none",
    }
}

/// What each of a tree's sets of colours is called, in the order
/// [`palettes`] has them, for the list they are chosen from.
pub fn tree_palette_names(kind: &str) -> &'static [&'static str] {
    match kind {
        "pine" => &["Spruce green", "Under snow", "Blue spruce", "In frost"],
        "palm" => &["Coconut palm", "Dusty", "At sunset"],
        "boxwood" => &["Box green", "Dark yew", "Golden privet"],
        "beech" => &["Summer", "Autumn copper", "Spring", "Copper beech"],
        "hedge" => &["Box with red berries", "Dark yew", "Golden privet"],
        "peony" => &["Pink", "Deep red", "White", "Coral", "Blush"],
        "hydrangea" => &["Blue", "Pink", "White", "Blue to pink"],
        "grass" => &["Lawn", "Meadow", "Dry", "Autumn", "Winter"],
        "birch" => &["Summer", "Autumn gold", "Spring"],
        "willow" => &["Summer", "Spring gold", "Autumn"],
        "datepalm" => &["Date palm", "Dusty"],
        "scotspine" => &["Scots pine", "Under snow"],
        "cypress" => &["Cypress", "Blue juniper", "Golden"],
        "yucca" => &["Yucca", "Blue agave", "Variegated"],
        "daisy" => &["White", "Pink", "Yellow"],
        "daffodil" => &["Yellow", "White with a yellow trumpet", "White with an orange trumpet"],
        "bed" => &["Spring mix", "Pink and white", "Hot colours"],
        "cherry" => &["Pale pink", "Deep pink", "White", "Mixed", "Summer leaf", "Autumn"],
        "petunia" => &["Purple", "Pink", "White", "Red", "Blue and white"],
        "reeds" => &["Summer", "Autumn straw", "Purple plumes"],
        "sunflower" => &["Yellow", "Red-bronze", "Lemon"],
        "flower" => &["Red", "Pink", "White", "Yellow", "Cream edged with red"],
        "tulip" => &["Red", "Yellow", "Pink", "Purple", "White", "Orange flamed"],
        _ => &["Summer", "Early autumn", "Late autumn", "Spring"],
    }
}

/// Which of the single flower's sets of colours a flower starts in: a red
/// rose, a white water lily and a yellow gerbera.
pub fn flower_set(flower: brickloft_core::tree::Flower) -> usize {
    use brickloft_core::tree::Flower;
    match flower {
        Flower::Rose => 0,
        Flower::WaterLily => 2,
        Flower::Gerbera => 3,
    }
}

/// What each of a kind of furniture's sets of colours is called, in the
/// order [`palettes`] has them.
pub fn furniture_palette_names(kind: &str) -> &'static [&'static str] {
    match kind {
        _ => &["Oak with mixed spines", "Dark walnut with leather", "White with pastels", "Black with brights"],
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
    #[test]
    fn every_pattern_on_the_list_has_colours_of_its_own() {
        // (medallions are the ones that start in the colours every other name falls back to)
        let fallback = palettes("no such pattern");
        for name in std::iter::once("stepping stones").chain(brickloft_core::pattern::STYLES.iter().map(|style| style.name)).filter(|name| *name != "medallions") {
            assert!(palettes(name) != fallback, "{name} has no colours of its own");
            assert!(palettes(name).iter().all(|set| set.len() >= 4 && set.len() <= MOST_COLOURS), "{name}: a set of too few colours or too many");
        }
        // stepping stones are light browns and greys: no colour of theirs is far from grey, or dark, or bright
        for colour in palettes("stepping stones").iter().flat_map(|set| set.iter()) {
            let (most, least) = (*colour.iter().max().unwrap() as i32, *colour.iter().min().unwrap() as i32);
            assert!(most - least <= 70 && (100..=205).contains(&most) && colour[0] >= colour[2], "{colour:?} is no light brown or grey");
        }
    }

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
}
