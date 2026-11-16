//! Cutting an object out of a picture with a plain ground behind it.
//!
//! TripoSR wants the object alone: see-through around it, filling about 85%
//! of a square. The models made for removing a ground were all turned down
//! on their licences (see `models`), so this is done with arithmetic on the
//! colours, which works for the pictures this program asks Stable Diffusion
//! for (one object on a plain studio ground) and for a photograph taken the
//! same way. It does not work for an object in a scene, and says so by the
//! numbers in its report (`Report`), which `rank` turns into a score.
//!
//! The steps:
//!
//! 1. **Where the ground is known to be.** The picture's corners are taken
//!    for ground, and each is followed along the frame for as long as the
//!    colour changes gently. A corner counts only if another corner is
//!    something like its colour, so an object that fills one corner is not
//!    mistaken for ground. The colours found this way are the ground's
//!    palette. A ground of two tones, a floor and a wall, has both in it.
//! 2. **A flood** from those frame pixels inwards, over pixels that are a
//!    palette colour or that differ little from their neighbours. The second
//!    rule follows a studio ground's slow change of brightness and the soft
//!    edge of a shadow. Two things keep it out of the object. It may not
//!    stray further from the palette than `DRIFT` (or than a shadow of a
//!    palette colour would be), so a smooth red car is not ground however
//!    gently its own colour changes. And it runs on a mask shrunk by two
//!    pixels, so a gap of a few pixels in the object's outline does not let
//!    it in.
//! 3. **Shadows put back if they were the object.** The flood is run with
//!    and without leave to follow shadows; if following them takes much of
//!    what was left, as it does with a grey object on a white ground, the
//!    result without is kept.
//! 4. **Ground seen through the object** (between a statue's legs): pieces
//!    the flood cannot reach that are a palette colour and smooth.
//! 5. **Thin leftovers**: the mask is opened (shrunk and grown back within
//!    itself), which removes lines and specks thinner than five pixels, such
//!    as the hard edge of a shadow. The largest piece is kept.
//!
//! The trial's cut-out was the flood alone, started from every pixel of the
//! frame, with the largest piece kept. It left shadow edges as lines, ate a
//! smooth object whose outline had a soft place in it, and ate an object
//! that ran off the picture. A model of the ground as one smooth surface
//! (a quadratic fitted to the frame) was tried here in its place and was
//! worse: half of Stable Diffusion's studio pictures have a floor and a
//! wall, which no one surface fits, and it left the floor standing as
//! object.
//!
//! Flood fill and morphological opening are in any text on image processing,
//! for instance Gonzalez and Woods, "Digital Image Processing", chapter 9.
//! They are written out here and not taken from the `imageproc` crate: that
//! crate would bring its own dependencies for a few dozen lines, and the
//! flood's rule for joining is this module's own.

use image::{Rgba, RgbaImage};
use std::collections::VecDeque;

/// The side the cut-out is worked at and written at. TripoSR takes 512.
pub const SIDE: u32 = 512;
/// The share of the square the object fills, TripoSR's own figure.
const FILL: f32 = 0.85;

// Differences between colours are from 0 to 255, the largest in any channel.
/// A pixel within this of a palette colour is ground wherever the flood reaches it.
const NEAR: f32 = 18.0;
/// A pixel that differs by no more than this from each neighbour may be ground.
const GENTLE: f32 = 4.0;
/// A corner is followed along the frame across steps no larger than this.
const ALONG_FRAME: f32 = 6.0;
/// unless the frame's next `STRETCH` pixels are, taken together, within `GRAIN` of the last `STRETCH`.
const STRETCH: usize = 8;
const GRAIN: f32 = 10.0;
/// Two corners within this of each other are the same ground.
const SAME_GROUND: f32 = 45.0;
/// The gentle rule may take the flood no further than this from every palette colour.
const DRIFT: f32 = 45.0;
/// A pixel on the object's edge within this of the ground beside it is a blend of the two, and is taken off.
const HALO: f32 = 30.0;
/// A shadow is a palette colour times a factor in this range, the same in each channel to within `SHADOW_HUE`.
const SHADOW: std::ops::Range<f32> = 0.3..1.0;
const SHADOW_HUE: f32 = 0.08;
/// If following shadows takes more than this share of what was left, it is undone.
const SHADOW_MOST: f32 = 0.35;
/// How far the mask is shrunk for the flood's probe and for the opening, in pixels.
const PROBE: usize = 2;
/// A see-through piece inside the object is at least this share of the picture.
const HOLE_LEAST: f32 = 0.0005;
/// A second piece at least this share of the largest counts as a second object.
const PIECE_LEAST: f32 = 0.03;

/// What was found, for `rank` and for the result's `cutout` field.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    /// The share of the picture that is object.
    pub coverage: f32,
    /// The share of the frame's own pixels that are object: above nothing, the object is cut by the frame.
    pub on_frame: f32,
    /// Pieces of a size to be objects, the kept one included.
    pub pieces: u32,
    /// The share of the frame that is ground: low when the picture is a scene.
    pub plain_frame: f32,
    /// Whether a shadow was found and taken off.
    pub shadow_removed: bool,
    /// See-through pieces found inside the object.
    pub holes: u32,
    /// The share of the object that is something like the ground's colour: high when ground was left standing
    /// around it, or when the object is the ground's colour and the cut could not have been clean.
    pub ground_like: f32,
    /// How much the object's lower outline rises to either side of its
    /// lowest point, as a share of its width: near 0 for an object seen
    /// level, and near the sine of the camera's height for one seen from
    /// above (see `rise`).
    pub rise: f32,
    /// The object's box in the worked picture: left, top, width, height.
    pub bounds: [u32; 4],
}

pub struct Cut {
    /// The object alone on a see-through square.
    pub picture: RgbaImage,
    pub report: Report,
}

/// A picture as rows of flags.
#[derive(Clone)]
pub struct Mask {
    pub w: usize,
    pub h: usize,
    pub on: Vec<bool>,
}

impl Mask {
    pub fn new(w: usize, h: usize, value: bool) -> Mask {
        Mask { w, h, on: vec![value; w * h] }
    }

    pub fn count(&self) -> usize {
        self.on.iter().filter(|&&v| v).count()
    }

    fn neighbours(&self, i: usize) -> impl Iterator<Item = usize> {
        let (w, h, x, y) = (self.w, self.h, i % self.w, i / self.w);
        [(x > 0).then(|| i - 1), (x + 1 < w).then(|| i + 1), (y > 0).then(|| i - w), (y + 1 < h).then(|| i + w)].into_iter().flatten()
    }

    fn on_frame(&self, i: usize) -> bool {
        let (x, y) = (i % self.w, i / self.w);
        x == 0 || y == 0 || x + 1 == self.w || y + 1 == self.h
    }

    /// Shrinks the mask by one pixel (a pixel stays if it and its four neighbours are on; the frame counts as off).
    fn shrunk(&self) -> Mask {
        let mut out = Mask::new(self.w, self.h, false);
        for i in 0..self.on.len() {
            out.on[i] = self.on[i] && self.neighbours(i).filter(|&j| self.on[j]).count() == 4;
        }
        out
    }

    /// Grows the mask by one pixel, within `within`.
    fn grown(&self, within: &Mask) -> Mask {
        let mut out = self.clone();
        for i in 0..self.on.len() {
            if !self.on[i] && within.on[i] && self.neighbours(i).any(|j| self.on[j]) {
                out.on[i] = true;
            }
        }
        out
    }

    /// The pieces of the mask (joined by their sides), as a label for each pixel (0 for off) and each piece's size.
    fn pieces(&self) -> (Vec<u32>, Vec<usize>) {
        let mut label = vec![0u32; self.on.len()];
        let mut sizes = vec![0usize];
        let mut queue = VecDeque::new();
        for start in 0..self.on.len() {
            if !self.on[start] || label[start] != 0 {
                continue;
            }
            let piece = sizes.len() as u32;
            label[start] = piece;
            queue.push_back(start);
            let mut size = 0;
            while let Some(i) = queue.pop_front() {
                size += 1;
                for j in self.neighbours(i) {
                    if self.on[j] && label[j] == 0 {
                        label[j] = piece;
                        queue.push_back(j);
                    }
                }
            }
            sizes.push(size);
        }
        (label, sizes)
    }

    /// The pixels of `self` that can be reached from `starts` through `self`.
    fn reached_from(&self, starts: impl Iterator<Item = usize>) -> Mask {
        let mut out = Mask::new(self.w, self.h, false);
        let mut queue = VecDeque::new();
        for i in starts {
            if self.on[i] && !out.on[i] {
                out.on[i] = true;
                queue.push_back(i);
            }
        }
        while let Some(i) = queue.pop_front() {
            for j in self.neighbours(i) {
                if self.on[j] && !out.on[j] {
                    out.on[j] = true;
                    queue.push_back(j);
                }
            }
        }
        out
    }
}

type Colour = [f32; 3];

fn apart(a: Colour, b: Colour) -> f32 {
    (0..3).map(|k| (a[k] - b[k]).abs()).fold(0.0, f32::max)
}

/// Each pixel averaged with its eight neighbours, so that the grain of a
/// picture does not stop a flood that is following a gradient.
fn blurred(picture: &RgbaImage) -> Vec<Colour> {
    let (w, h) = (picture.width() as usize, picture.height() as usize);
    let raw = picture.as_raw();
    let at = |x: usize, y: usize| -> Colour {
        let o = (y * w + x) * 4;
        [raw[o] as f32, raw[o + 1] as f32, raw[o + 2] as f32]
    };
    let mut out = vec![[0f32; 3]; w * h];
    for y in 0..h {
        for x in 0..w {
            let (mut sum, mut n) = ([0f32; 3], 0f32);
            for yy in y.saturating_sub(1)..=(y + 1).min(h - 1) {
                for xx in x.saturating_sub(1)..=(x + 1).min(w - 1) {
                    let p = at(xx, yy);
                    for k in 0..3 {
                        sum[k] += p[k];
                    }
                    n += 1.0;
                }
            }
            out[y * w + x] = [sum[0] / n, sum[1] / n, sum[2] / n];
        }
    }
    out
}

/// The frame's pixels in order round the picture, starting at the top left and going clockwise.
fn round_the_frame(w: usize, h: usize) -> Vec<usize> {
    let top = 0..w;
    let right = (1..h).map(|y| y * w + w - 1);
    let bottom = (0..w - 1).rev().map(|x| (h - 1) * w + x);
    let left = (1..h - 1).rev().map(|y| y * w);
    top.chain(right).chain(bottom).chain(left).collect()
}

/// Step 1. The frame pixels that are ground: each corner that has a fellow,
/// followed both ways round the frame while the colour changes gently.
fn ground_on_frame(colours: &[Colour], w: usize, h: usize) -> Vec<usize> {
    let frame = round_the_frame(w, h);
    let n = frame.len();
    // where the four corners are in that order
    let corners = [0, w - 1, w - 1 + h - 1, w - 1 + h - 1 + w - 1];
    let colour_at = |k: usize| colours[frame[k % n]];
    let has_fellow = |c: usize| corners.iter().any(|&other| other != corners[c] && apart(colour_at(corners[c]), colour_at(other)) <= SAME_GROUND);
    let mut ground = vec![false; n];
    for c in 0..4 {
        if !has_fellow(c) {
            continue;
        }
        ground[corners[c]] = true;
        for forwards in [true, false] {
            let mut k = corners[c];
            for _ in 0..n {
                let next = if forwards { (k + 1) % n } else { (k + n - 1) % n };
                if ground[next] {
                    break;
                }
                // A step that is not gentle ends the walk only if the stretch of frame beyond it is another colour
                // than the stretch behind. A ground with a grain to it (a table's wood, a cloth) is all steps, and
                // its stretches are all one colour; where an object runs off the picture they are not.
                if apart(colour_at(k), colour_at(next)) > ALONG_FRAME {
                    let stretch = |from: usize, onwards: bool| {
                        let mut sum = [0f32; 3];
                        for d in 0..STRETCH {
                            let c = colour_at(if onwards == forwards { from + d } else { from + n - d });
                            sum = [sum[0] + c[0], sum[1] + c[1], sum[2] + c[2]];
                        }
                        sum.map(|v| v / STRETCH as f32)
                    };
                    if apart(stretch(k, false), stretch(next, true)) > GRAIN {
                        break;
                    }
                }
                ground[next] = true;
                k = next;
            }
        }
    }
    (0..n).filter(|&k| ground[k]).map(|k| frame[k]).collect()
}

/// The ground's colours, as tables that answer "is this colour within so much of one of them".
struct Palette {
    /// The palette's colours, one for each cell of a coarse grid of colours that has any.
    colours: Vec<Colour>,
    near: Vec<bool>,
    drift: Vec<bool>,
}

/// The side of a cell of the colour grid, and how many there are along each channel.
const CELL: f32 = 8.0;
const CELLS: usize = 32;

impl Palette {
    fn cell(colour: Colour) -> [usize; 3] {
        colour.map(|v| ((v / CELL) as usize).min(CELLS - 1))
    }

    fn index(colour: Colour) -> usize {
        let [r, g, b] = Palette::cell(colour);
        (r * CELLS + g) * CELLS + b
    }

    fn of(colours: impl Iterator<Item = Colour>) -> Palette {
        let mut has = vec![false; CELLS * CELLS * CELLS];
        let mut list = Vec::new();
        for colour in colours {
            if !std::mem::replace(&mut has[Palette::index(colour)], true) {
                list.push(Palette::cell(colour).map(|c| (c as f32 + 0.5) * CELL));
            }
        }
        // Each table has the cells whose middle is within its distance of a palette colour. Filled from the palette
        // outwards, once, so that asking is one look-up for each of a quarter of a million pixels.
        let table = |within: f32| {
            let reach = (within / CELL).ceil() as i64;
            let last = CELLS as i64 - 1;
            let mut table = vec![false; CELLS * CELLS * CELLS];
            for colour in &list {
                let [r, g, b] = Palette::cell(*colour).map(|c| c as i64);
                for rr in (r - reach).max(0)..=(r + reach).min(last) {
                    for gg in (g - reach).max(0)..=(g + reach).min(last) {
                        for bb in (b - reach).max(0)..=(b + reach).min(last) {
                            let middle = [rr, gg, bb].map(|c| (c as f32 + 0.5) * CELL);
                            if apart(middle, *colour) <= within {
                                table[Palette::index(middle)] = true;
                            }
                        }
                    }
                }
            }
            table
        };
        Palette { near: table(NEAR), drift: table(DRIFT), colours: list }
    }

    fn is_near(&self, colour: Colour) -> bool {
        self.near[Palette::index(colour)]
    }

    fn within_drift(&self, colour: Colour) -> bool {
        self.drift[Palette::index(colour)]
    }

    /// Whether a colour is a palette colour in shadow: darker by one factor in every channel.
    fn shadow(&self, colour: Colour) -> bool {
        self.colours.iter().any(|ground| {
            let ratio = [0, 1, 2].map(|k| colour[k] / ground[k].max(1.0));
            let mean = (ratio[0] + ratio[1] + ratio[2]) / 3.0;
            SHADOW.contains(&mean) && ratio.iter().all(|r| (r - mean).abs() <= SHADOW_HUE)
        })
    }
}

/// Step 2. The ground: flooded from `starts` over what could be ground,
/// through a mask shrunk by `PROBE` and grown back.
fn flooded(colours: &[Colour], w: usize, h: usize, palette: &Palette, starts: &[usize], shadows: bool) -> Mask {
    let mut could = Mask::new(w, h, false);
    for i in 0..colours.len() {
        let c = colours[i];
        let gentle = could.neighbours(i).all(|j| apart(c, colours[j]) <= GENTLE);
        could.on[i] = palette.is_near(c) || (gentle && (palette.within_drift(c) || (shadows && palette.shadow(c))));
    }
    // The probe: shrunk, except at the frame pixels the flood starts from.
    let mut is_start = vec![false; colours.len()];
    for &i in starts {
        is_start[i] = true;
    }
    let mut probe = could.clone();
    for _ in 0..PROBE {
        let shrunk = probe.shrunk();
        for i in 0..probe.on.len() {
            probe.on[i] = shrunk.on[i] || is_start[i];
        }
    }
    let mut ground = probe.reached_from(starts.iter().copied());
    for _ in 0..PROBE {
        ground = ground.grown(&could);
    }
    ground
}

/// The object's mask in a picture, and what was noticed on the way.
/// `None` if there is nothing that could be an object.
pub fn object_in(picture: &RgbaImage) -> Option<(Mask, Report)> {
    let (w, h) = (picture.width() as usize, picture.height() as usize);
    if w < 16 || h < 16 {
        return None;
    }
    let colours = blurred(picture);
    let frame = round_the_frame(w, h);
    let starts = ground_on_frame(&colours, w, h);
    if starts.is_empty() {
        // no two corners alike: there is no plain ground to go by
        return None;
    }
    let palette = Palette::of(starts.iter().map(|&i| colours[i]));

    // Steps 2 and 3: with shadows followed, unless that takes much of what is left without.
    let plain = flooded(&colours, w, h, &palette, &starts, false);
    let with_shadows = flooded(&colours, w, h, &palette, &starts, true);
    let left = plain.on.len() - plain.count();
    let taken = with_shadows.count().saturating_sub(plain.count());
    let shadow_removed = taken > 0 && taken as f32 <= SHADOW_MOST * left as f32;
    let ground = if shadow_removed { with_shadows } else { plain };
    let plain_frame = frame.iter().filter(|&&i| ground.on[i]).count() as f32 / frame.len() as f32;

    let mut object = Mask { w, h, on: ground.on.iter().map(|&g| !g).collect() };

    // Step 4: ground seen through the object.
    let mut see_through = Mask::new(w, h, false);
    for i in 0..colours.len() {
        see_through.on[i] = object.on[i] && palette.is_near(colours[i]);
    }
    let (label, sizes) = see_through.pieces();
    let least = (((w * h) as f32 * HOLE_LEAST) as usize).max(16);
    let (mut steps, mut pairs, mut touches) = (vec![0f32; sizes.len()], vec![0usize; sizes.len()], vec![false; sizes.len()]);
    for i in (0..label.len()).filter(|&i| label[i] != 0) {
        let piece = label[i] as usize;
        // a piece on the frame is not seen through anything
        touches[piece] |= object.on_frame(i);
        for j in see_through.neighbours(i).filter(|&j| label[j] == label[i]) {
            steps[piece] += apart(colours[i], colours[j]);
            pairs[piece] += 1;
        }
    }
    // smooth all through, as ground is: the mean step between neighbours inside the piece is small
    let is_hole: Vec<bool> = (0..sizes.len()).map(|p| p > 0 && sizes[p] >= least && !touches[p] && pairs[p] > 0 && steps[p] / pairs[p] as f32 <= GENTLE * 0.75).collect();
    let holes = is_hole.iter().filter(|&&hole| hole).count() as u32;
    for i in 0..label.len() {
        if is_hole[label[i] as usize] {
            object.on[i] = false;
        }
    }

    // The halo: edge pixels that are part ground.
    let edge: Vec<usize> = (0..object.on.len()).filter(|&i| object.on[i] && object.neighbours(i).any(|j| !object.on[j] && apart(colours[i], colours[j]) <= HALO)).collect();
    for i in edge {
        object.on[i] = false;
    }

    // Step 5: opening. What survives being shrunk is grown back within the mask, a pixel further than it was shrunk
    // so that the outline is the mask's own and not a rounded copy of it.
    let mut core = object.clone();
    for _ in 0..PROBE {
        core = core.shrunk();
    }
    let (label, sizes) = core.pieces();
    let largest = (1..sizes.len()).max_by_key(|&p| sizes[p])?;
    let pieces = (1..sizes.len()).filter(|&p| sizes[p] as f32 >= sizes[largest] as f32 * PIECE_LEAST).count() as u32;
    let mut kept = Mask { w, h, on: label.iter().map(|&l| l == largest as u32).collect() };
    for _ in 0..PROBE + 1 {
        kept = kept.grown(&object);
    }
    let area = kept.count();
    if area < 64 {
        return None;
    }

    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for i in (0..kept.on.len()).filter(|&i| kept.on[i]) {
        x0 = x0.min(i % w);
        x1 = x1.max(i % w);
        y0 = y0.min(i / w);
        y1 = y1.max(i / w);
    }
    let report = Report {
        coverage: area as f32 / (w * h) as f32,
        on_frame: frame.iter().filter(|&&i| kept.on[i]).count() as f32 / frame.len() as f32,
        pieces,
        plain_frame,
        shadow_removed,
        holes,
        ground_like: (0..kept.on.len()).filter(|&i| kept.on[i] && palette.within_drift(colours[i])).count() as f32 / area as f32,
        rise: rise(&kept),
        bounds: [x0 as u32, y0 as u32, (x1 - x0 + 1) as u32, (y1 - y0 + 1) as u32],
    };
    Some((kept, report))
}

/// How steeply an object's lower outline rises to either side of its
/// lowest point: the two slopes' geometric mean.
///
/// A box standing on the ground, seen from a corner, shows its base as a V
/// whose arms rise at the sine of the camera's height above level; a round
/// base shows as half an ellipse that is the same sine as deep as it is
/// wide. So the rise says how high the picture was taken from, without
/// knowing what the object is. It is taken on the outline's lower hull, so
/// that the gaps between legs or wheels do not count, and over the middle
/// 70% of the width, so that a wing or a roof that overhangs the base does
/// not either. It is a guide and no more: a boat's hull rises whatever the
/// camera does.
pub fn rise(mask: &Mask) -> f32 {
    // the lowest object pixel in each column (rows count downwards)
    let lowest: Vec<Option<usize>> = (0..mask.w).map(|x| (0..mask.h).rev().find(|&y| mask.on[y * mask.w + x])).collect();
    let points: Vec<(f32, f32)> = lowest.iter().enumerate().filter_map(|(x, y)| y.map(|y| (x as f32, y as f32))).collect();
    let (Some(first), Some(last)) = (points.first(), points.last()) else { return 0.0 };
    let width = last.0 - first.0;
    if width < 8.0 {
        return 0.0;
    }
    // the lower hull: of the outline's lowest points, those no straight line between two others passes below
    let mut hull: Vec<(f32, f32)> = Vec::new();
    for &p in &points {
        while hull.len() >= 2 {
            let (a, b) = (hull[hull.len() - 2], hull[hull.len() - 1]);
            // b is dropped if it is above the line from a to p (rows count downwards, so above is a smaller y)
            if (b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0) >= 0.0 {
                hull.pop();
            } else {
                break;
            }
        }
        hull.push(p);
    }
    let height_at = |x: f32| -> f32 {
        let x = x.clamp(first.0, last.0);
        let k = hull.windows(2).position(|s| x <= s[1].0).unwrap_or(hull.len().saturating_sub(2));
        let (a, b) = (hull[k], hull[(k + 1).min(hull.len() - 1)]);
        if b.0 == a.0 { a.1 } else { a.1 + (b.1 - a.1) * (x - a.0) / (b.0 - a.0) }
    };
    let low = hull.iter().copied().max_by(|a, b| a.1.total_cmp(&b.1)).unwrap_or(*first);
    let reach = 0.35 * width;
    let (left, right) = ((low.0 - reach).max(first.0), (low.0 + reach).min(last.0));
    // Each arm's rise over its own run. The two are multiplied and not added: a box turned so that one side faces the
    // camera has one arm steep and the other flat, the first as many times steeper as the second is flatter, and
    // their product is the same whichever way the box is turned (the sine of the camera's height, squared).
    let arm = |to: f32| if (to - low.0).abs() < 2.0 { 0.0 } else { ((low.1 - height_at(to)) / (to - low.0).abs()).max(0.0) };
    (arm(left) * arm(right)).sqrt()
}

/// A picture no larger than `SIDE` a side, made smaller if it was larger.
pub fn worked(picture: &RgbaImage) -> RgbaImage {
    let longest = picture.width().max(picture.height());
    if longest <= SIDE {
        return picture.clone();
    }
    let scale = SIDE as f32 / longest as f32;
    let (w, h) = (((picture.width() as f32 * scale).round() as u32).max(1), ((picture.height() as f32 * scale).round() as u32).max(1));
    image::imageops::resize(picture, w, h, image::imageops::FilterType::Triangle)
}

/// The masked object on a see-through square of `SIDE`, filling `FILL` of it.
pub fn squared(picture: &RgbaImage, mask: &Mask, bounds: [u32; 4]) -> RgbaImage {
    let [x0, y0, bw, bh] = bounds;
    let side = ((bw.max(bh) as f32 / FILL).ceil() as u32).max(1);
    let mut square = RgbaImage::from_pixel(side, side, Rgba([0, 0, 0, 0]));
    let (dx, dy) = ((side - bw) / 2, (side - bh) / 2);
    for y in 0..bh {
        for x in 0..bw {
            if mask.on[(y0 + y) as usize * mask.w + (x0 + x) as usize] {
                let p = picture.get_pixel(x0 + x, y0 + y);
                square.put_pixel(dx + x, dy + y, Rgba([p[0], p[1], p[2], 255]));
            }
        }
    }
    if side == SIDE {
        return square;
    }
    let mut sized = image::imageops::resize(&square, SIDE, SIDE, image::imageops::FilterType::Triangle);
    // Scaling blends the edge into see-through black. A pixel is in or out: a half-in one would be a dark rim on the object.
    for p in sized.pixels_mut() {
        if p[3] >= 128 {
            let a = p[3] as f32 / 255.0;
            *p = Rgba([(p[0] as f32 / a).min(255.0) as u8, (p[1] as f32 / a).min(255.0) as u8, (p[2] as f32 / a).min(255.0) as u8, 255]);
        } else {
            *p = Rgba([0, 0, 0, 0]);
        }
    }
    sized
}

/// Cuts the object out of a picture with a plain ground.
pub fn cut_out(picture: &RgbaImage) -> Option<Cut> {
    let picture = worked(picture);
    let (mask, report) = object_in(&picture)?;
    Some(Cut { picture: squared(&picture, &mask, report.bounds), report })
}

/// For a picture that is already cut out: its see-through pixels are the
/// ground. `None` if none of it is see-through, or all of it.
pub fn already_cut(picture: &RgbaImage) -> Option<Cut> {
    let picture = worked(picture);
    let (w, h) = (picture.width() as usize, picture.height() as usize);
    let mask = Mask { w, h, on: picture.pixels().map(|p| p[3] >= 128).collect() };
    let area = mask.count();
    if area < 64 || area == w * h {
        return None;
    }
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for i in (0..mask.on.len()).filter(|&i| mask.on[i]) {
        x0 = x0.min(i % w);
        x1 = x1.max(i % w);
        y0 = y0.min(i / w);
        y1 = y1.max(i / w);
    }
    let (_, sizes) = mask.pieces();
    let largest = sizes.iter().copied().max().unwrap_or(0);
    let frame_pixels = 2 * w + 2 * h - 4;
    let on_frame = round_the_frame(w, h).into_iter().filter(|&i| mask.on[i]).count();
    let report = Report {
        coverage: area as f32 / (w * h) as f32,
        on_frame: on_frame as f32 / frame_pixels as f32,
        pieces: sizes.iter().skip(1).filter(|&&s| s as f32 >= largest as f32 * PIECE_LEAST).count() as u32,
        plain_frame: 1.0 - on_frame as f32 / frame_pixels as f32,
        shadow_removed: false,
        holes: 0,
        ground_like: 0.0,
        rise: rise(&mask),
        bounds: [x0 as u32, y0 as u32, (x1 - x0 + 1) as u32, (y1 - y0 + 1) as u32],
    };
    Some(Cut { picture: squared(&picture, &mask, report.bounds), report })
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// A studio ground: light, darker towards the corners and towards the bottom, with a little grain.
    pub fn studio(w: u32, h: u32) -> RgbaImage {
        RgbaImage::from_fn(w, h, |x, y| {
            let (fx, fy) = (x as f32 / w as f32 - 0.5, y as f32 / h as f32 - 0.5);
            let light = 236.0 - 70.0 * (fx * fx + fy * fy) - 30.0 * (fy + 0.5);
            let grain = ((x * 7 + y * 13) % 5) as f32 - 2.0;
            let v = (light + grain).clamp(0.0, 255.0) as u8;
            Rgba([v, v, v.saturating_sub(4), 255])
        })
    }

    fn fill(picture: &mut RgbaImage, x: std::ops::Range<u32>, y: std::ops::Range<u32>, colour: impl Fn(u32, u32) -> [u8; 3]) {
        for yy in y {
            for xx in x.clone() {
                let c = colour(xx, yy);
                picture.put_pixel(xx, yy, Rgba([c[0], c[1], c[2], 255]));
            }
        }
    }

    /// A red box in the middle of a studio ground, 100 by 120 pixels at (78, 60).
    pub fn boxed() -> RgbaImage {
        let mut picture = studio(256, 256);
        fill(&mut picture, 78..178, 60..180, |x, y| [180 + ((x + y) % 7) as u8, 40, 30]);
        picture
    }

    fn share_agreeing(mask: &Mask, inside: impl Fn(usize, usize) -> bool) -> f32 {
        let agree = (0..mask.on.len()).filter(|&i| mask.on[i] == inside(i % mask.w, i / mask.w)).count();
        agree as f32 / mask.on.len() as f32
    }

    #[test]
    fn a_box_on_a_studio_ground_is_found() {
        let (mask, report) = object_in(&boxed()).unwrap();
        let agree = share_agreeing(&mask, |x, y| (78..178).contains(&x) && (60..180).contains(&y));
        assert!(agree > 0.985, "the mask is the box: {agree}");
        assert_eq!(report.pieces, 1);
        assert_eq!(report.on_frame, 0.0);
        assert!(report.plain_frame > 0.95, "{}", report.plain_frame);
        assert!((report.coverage - 100.0 * 120.0 / 65536.0).abs() < 0.02, "{}", report.coverage);
        let [x, y, w, h] = report.bounds;
        assert!(x.abs_diff(78) <= 3 && y.abs_diff(60) <= 3 && w.abs_diff(100) <= 5 && h.abs_diff(120) <= 5, "{:?}", report.bounds);
    }

    #[test]
    fn a_soft_shadow_and_its_edge_are_taken_off() {
        let mut picture = boxed();
        // a shadow to the right of the box's foot: the ground darkened to half at its deepest, which is further from
        // the ground's colour than the flood may stray by gentleness alone, fading out to the right, above and below
        for y in 100..220u32 {
            for x in 178..250u32 {
                let fade = 1.0 - (x - 178) as f32 / 72.0;
                let depth = 0.5 * fade * (std::f32::consts::FRAC_PI_2 * (y as f32 - 160.0) / 60.0).cos().max(0.0);
                let p = picture.get_pixel(x, y).0;
                picture.put_pixel(x, y, Rgba([(p[0] as f32 * (1.0 - depth)) as u8, (p[1] as f32 * (1.0 - depth)) as u8, (p[2] as f32 * (1.0 - depth)) as u8, 255]));
            }
        }
        let (mask, report) = object_in(&picture).unwrap();
        assert!(report.shadow_removed);
        let shadow_left = (100..220).flat_map(|y| (182..250).map(move |x| (x, y))).filter(|&(x, y)| mask.on[y * 256 + x]).count();
        assert_eq!(shadow_left, 0, "none of the shadow is object");
        let [_, _, w, _] = report.bounds;
        assert!(w.abs_diff(100) <= 5, "the object is as wide as the box and no wider: {w}");
    }

    #[test]
    fn a_ground_with_a_grain_is_still_ground() {
        // a cloth's weave: streaks two pixels wide running slantwise, forty levels apart with hard edges, so that
        // every side of the picture crosses them; and the box on it
        let mut picture = RgbaImage::from_fn(256, 256, |x, y| {
            let v = (150 + if ((x + y) / 2) % 2 == 0 { 40 } else { 0 }) as u8;
            Rgba([v, v, v, 255])
        });
        fill(&mut picture, 78..178, 60..180, |x, y| [180 + ((x + y) % 7) as u8, 40, 30]);
        let (mask, report) = object_in(&picture).unwrap();
        let agree = share_agreeing(&mask, |x, y| (78..178).contains(&x) && (60..180).contains(&y));
        assert!(agree > 0.98, "the mask is the box, and the cloth is not: {agree}");
        assert!(report.plain_frame > 0.95, "{}", report.plain_frame);
    }

    #[test]
    fn a_thin_line_and_a_speck_are_not_object() {
        let mut picture = boxed();
        // a dark line two pixels thick running off from the box, as the rim of a shadow does, and a speck
        fill(&mut picture, 178..240, 170..172, |_, _| [70, 70, 70]);
        fill(&mut picture, 30..34, 30..34, |_, _| [20, 20, 20]);
        let (mask, report) = object_in(&picture).unwrap();
        assert_eq!(report.pieces, 1);
        let [_, _, w, _] = report.bounds;
        assert!(w.abs_diff(100) <= 5, "the line is not part of the box: {w}");
        assert!(!mask.on[31 * 256 + 31]);
    }

    #[test]
    fn a_smooth_object_with_a_soft_place_in_its_outline_is_kept() {
        // a smooth blue ball whose colour changes gently all over, with a short stretch of its outline blended into
        // the ground: a flood that follows gentle change gets in there and takes the whole ball
        let mut picture = studio(256, 256);
        for y in 0..256u32 {
            for x in 0..256u32 {
                let (dx, dy) = (x as f32 - 128.0, y as f32 - 128.0);
                let d = (dx * dx + dy * dy).sqrt();
                if d < 70.0 {
                    let ground = picture.get_pixel(x, y).0;
                    let ball = [60.0 + dx * 0.3, 90.0 + dy * 0.3, 200.0 - d * 0.4];
                    // blended over the last 50 pixels, in a wedge at the top: three levels a pixel, which is gentle
                    let soft = if dy < 0.0 && dx.abs() < 5.0 { ((70.0 - d) / 50.0).min(1.0) } else { 1.0 };
                    let c = [0, 1, 2].map(|k| (ball[k] * soft + ground[k] as f32 * (1.0 - soft)) as u8);
                    picture.put_pixel(x, y, Rgba([c[0], c[1], c[2], 255]));
                }
            }
        }
        let (mask, _) = object_in(&picture).unwrap();
        let inside = (0..mask.on.len()).filter(|&i| {
            let (dx, dy) = ((i % 256) as f32 - 128.0, (i / 256) as f32 - 128.0);
            (dx * dx + dy * dy).sqrt() < 60.0 && mask.on[i]
        }).count();
        let ball = (std::f32::consts::PI * 60.0 * 60.0) as usize;
        assert!(inside as f32 > 0.97 * ball as f32, "the ball is kept: {inside} of {ball}");
    }

    #[test]
    fn ground_seen_through_the_object_is_see_through() {
        let mut picture = studio(256, 256);
        let ground = picture.clone();
        // an arch: a block with an opening through it
        fill(&mut picture, 60..196, 60..190, |x, y| [40, 110 + ((x * 3 + y) % 9) as u8, 60]);
        for y in 110..190u32 {
            for x in 100..156u32 {
                picture.put_pixel(x, y, *ground.get_pixel(x, y));
            }
        }
        // the opening does not reach the frame: the arch has a sill
        fill(&mut picture, 60..196, 180..190, |_, _| [40, 110, 60]);
        let (mask, report) = object_in(&picture).unwrap();
        assert_eq!(report.holes, 1);
        assert!(!mask.on[140 * 256 + 128], "the opening is not object");
        assert!(mask.on[80 * 256 + 128] && mask.on[185 * 256 + 128], "the arch and its sill are");
    }

    #[test]
    fn what_the_frame_cuts_and_what_is_in_pieces_is_reported() {
        let mut picture = studio(256, 256);
        fill(&mut picture, 0..120, 80..256, |_, _| [150, 60, 40]);
        fill(&mut picture, 170..230, 100..160, |_, _| [40, 60, 150]);
        let (_, report) = object_in(&picture).unwrap();
        assert!(report.on_frame > 0.2, "{}", report.on_frame);
        assert_eq!(report.pieces, 2);
    }

    #[test]
    fn nothing_to_find_is_none() {
        assert!(object_in(&studio(256, 256)).is_none());
        assert!(object_in(&RgbaImage::new(4, 4)).is_none());
        assert!(already_cut(&studio(64, 64)).is_none(), "nothing see-through");
        assert!(already_cut(&RgbaImage::new(64, 64)).is_none(), "nothing else");
    }

    #[test]
    fn the_object_is_centred_and_fills_the_square() {
        let cut = cut_out(&boxed()).unwrap();
        assert_eq!(cut.picture.dimensions(), (SIDE, SIDE));
        let (mut x0, mut y0, mut x1, mut y1) = (SIDE, SIDE, 0, 0);
        for (x, y, p) in cut.picture.enumerate_pixels() {
            assert!(p[3] == 0 || p[3] == 255, "a pixel is in or out");
            if p[3] == 255 {
                x0 = x0.min(x);
                x1 = x1.max(x);
                y0 = y0.min(y);
                y1 = y1.max(y);
            }
        }
        // the box is taller than wide: its height fills 85% of the square, and it is centred both ways
        let (w, h) = ((x1 - x0 + 1) as f32, (y1 - y0 + 1) as f32);
        assert!((h / SIDE as f32 - 0.85).abs() < 0.02, "{h}");
        assert!(((x0 + x1) as f32 / 2.0 - 255.5).abs() < 3.0 && ((y0 + y1) as f32 / 2.0 - 255.5).abs() < 3.0);
        assert!((w / h - 100.0 / 120.0).abs() < 0.03);
        // and it is the box's red, not the ground's grey
        let p = cut.picture.get_pixel(256, 256);
        assert!(p[0] > 150 && p[1] < 80);
    }

    #[test]
    fn a_picture_already_cut_out_is_taken_by_its_transparency() {
        let mut picture = RgbaImage::new(1024, 768);
        for y in 200..600 {
            for x in 300..500 {
                picture.put_pixel(x, y, Rgba([10, 200, 10, 255]));
            }
        }
        let cut = already_cut(&picture).unwrap();
        assert_eq!(cut.picture.dimensions(), (SIDE, SIDE));
        assert_eq!(cut.report.pieces, 1);
        assert_eq!(cut.report.on_frame, 0.0);
        let p = cut.picture.get_pixel(256, 256);
        assert_eq!((p[1], p[3]), (200, 255));
    }

    /// A box seen from a corner at `height` degrees above level: its base is a V and its top a flat cap.
    pub fn box_from(height: f32) -> Mask {
        let (w, h) = (256usize, 256usize);
        let slope = height.to_radians().sin();
        let mut mask = Mask::new(w, h, false);
        for y in 0..h {
            for x in 48..208usize {
                let from_middle = (x as f32 - 128.0).abs();
                let base = 200.0 - slope * from_middle;
                let top = 90.0 - slope * (80.0 - from_middle);
                mask.on[y * w + x] = (y as f32) <= base && (y as f32) >= top;
            }
        }
        mask
    }

    #[test]
    fn the_lower_outline_rises_with_the_cameras_height() {
        assert!(rise(&box_from(0.0)) < 0.02);
        for height in [15.0f32, 30.0, 50.0] {
            let found = rise(&box_from(height));
            assert!((found - height.to_radians().sin()).abs() < 0.04, "from {height} degrees: {found}");
        }
        // legs do not count: a table seen level has a flat lower hull
        let mut table = Mask::new(256, 256, false);
        for y in 80..200 {
            for x in 50..206 {
                table.on[y * 256 + x] = y < 110 || !(70..186).contains(&x);
            }
        }
        assert!(rise(&table) < 0.02);
    }
}
