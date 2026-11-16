//! Parts that more than one kind of furniture is made of: a round top, a
//! tapering ring, a leg of each sort, a chair's back.

use super::style::{Back, Leg, Set};
use super::{mask_of, shared, Furniture, Lay};

/// What a piece is laid into: the box it fills, the bricks so far, and
/// the choices its style and seed have made.
pub(super) struct Room<'a> {
    pub lay: &'a mut Lay,
    pub lo: [i32; 3],
    pub hi: [i32; 3],
    pub set: Set,
    pub f: Furniture,
    pub slopes: bool,
    /// the bricks that carry a light in the game (see `Model::lamps`), by their place in `lay`
    pub lamps: Vec<usize>,
    /// for a plant fitted into the piece (see `pots::grow`): the volume of its shape, the volume where its bricks disagree with it, and the volume of its bricks
    pub grown: (f64, f64, f64),
}

impl Room<'_> {
    /// Its width, depth and height.
    pub fn size(&self) -> (i32, i32, i32) {
        (self.hi[0] - self.lo[0], self.hi[1] - self.lo[1], self.hi[2] - self.lo[2])
    }

    /// A box: see `Lay::block`.
    pub fn block(&mut self, lo: [i32; 3], hi: [i32; 3], shade: u8) {
        self.lay.block(lo, hi, shade);
    }

    /// A box that is a light in the game: the brick of it laid first
    /// carries the light, so one lamp is one light however its bulb is cut.
    pub fn lamp(&mut self, lo: [i32; 3], hi: [i32; 3]) {
        if (0..3).all(|k| hi[k] > lo[k]) {
            self.lamps.push(self.lay.bricks.len());
            self.lay.block(lo, hi, super::LIGHT);
        }
    }

    /// Whether a sloped piece `across` by `tall` in section may be laid:
    /// sloped pieces are wanted, and a brick may be that long.
    pub fn sloped(&self, across: i32, tall: i32) -> bool {
        self.slopes && across >= 1 && tall >= 1 && across.max(tall) <= self.lay.longest
    }
}

/// `part` of `of`, to the nearest cell, kept from `least` to `most`.
pub(super) fn share(of: i32, part: f64, least: i32, most: i32) -> i32 {
    ((of as f64 * part).round() as i32).clamp(least, most.max(least))
}

/// A stretch `want` long in the middle of `lo..hi`, made a cell longer or
/// shorter where that is what puts it in the middle exactly: a stem one
/// cell thick cannot stand at the middle of a base four wide, and a stem
/// off the middle is the first thing the eye finds.
pub(super) fn centred(lo: i32, hi: i32, want: i32) -> (i32, i32) {
    let room = hi - lo;
    let mut long = want.clamp(1, room.max(1));
    if (room - long) % 2 != 0 {
        long = if long < room { long + 1 } else { long - 1 }.max(1);
    }
    let at = lo + (room - long) / 2;
    (at, at + long)
}

/// A flat piece with its four corners cut off at 45 degrees, filling
/// `lo..hi`: the nearest a round top comes in bricks laid by rule. It is
/// an eight-sided outline, three boxes and a sloped piece at each corner:
/// seven bricks at any size. The outline fitter (`fit_planar`) would give
/// a rounder edge. It is not used, because it is a fit to be run and
/// measured for every top, and furniture is laid without one. A corner
/// is cut by three tenths of the shorter side, which makes the eight
/// sides nearly equal on a square.
pub(super) fn octagon(r: &mut Room, lo: [i32; 3], hi: [i32; 3], shade: u8) {
    let (w, d) = (hi[0] - lo[0], hi[1] - lo[1]);
    let cut = ((w.min(d) as f64 * 0.3).round() as i32).min((w.min(d) - 1) / 2);
    if cut < 1 || !r.sloped(cut, cut) {
        return r.block(lo, hi, shade);
    }
    r.block([lo[0] + cut, lo[1], lo[2]], [hi[0] - cut, hi[1], hi[2]], shade);
    for (x, low) in [(lo[0], 0u8), (hi[0] - cut, 1)] {
        r.block([x, lo[1] + cut, lo[2]], [x + cut, hi[1] - cut, hi[2]], shade);
        for (y, front) in [(lo[1], 0u8), (hi[1] - cut, 1)] {
            // all of its box but the upright edge at the outside corner
            r.lay.piece(mask_of(|x, y, _| !(x == low && y == front)), [x, y, lo[2]], [x + cut, y + cut, hi[2]], shade, 2);
        }
    }
}

/// A ring that slopes: `m` wide all round the box `lo..hi`, widest at
/// its top if `up` is 1 and at its foot if 0, of a ramp along each side
/// and a hip at each corner (as a bookcase's cornice is, on four sides).
/// What is inside the ring is left to whoever asks. Square bands where
/// sloped pieces cannot be had.
pub(super) fn ring(r: &mut Room, lo: [i32; 3], hi: [i32; 3], m: i32, up: u8, shade: u8) {
    let (ix, iy, z) = ((lo[0] + m, hi[0] - m), (lo[1] + m, hi[1] - m), (lo[2], hi[2]));
    if m < 1 || ix.1 < ix.0 || iy.1 < iy.0 {
        return;
    }
    if !r.sloped(m, z.1 - z.0) {
        r.block([lo[0], lo[1], z.0], [hi[0], iy.0, z.1], shade);
        r.block([lo[0], iy.1, z.0], [hi[0], hi[1], z.1], shade);
        r.block([lo[0], iy.0, z.0], [ix.0, iy.1, z.1], shade);
        r.block([ix.1, iy.0, z.0], [hi[0], iy.1, z.1], shade);
        return;
    }
    for (y, outer) in [(lo[1], 0u8), (iy.1, 1)] {
        r.lay.piece(mask_of(|_, y, z| !(y == outer && z != up)), [ix.0, y, z.0], [ix.1, y + m, z.1], shade, 0);
        for (x, side) in [(lo[0], 0u8), (ix.1, 1)] {
            // the whole face at the wide end, and of the narrow end the one corner that touches what is inside
            r.lay.piece(mask_of(|x, y, z| z == up || (x != side && y != outer)), [x, y, z.0], [x + m, y + m, z.1], shade, 2);
        }
    }
    for (x, outer) in [(lo[0], 0u8), (ix.1, 1)] {
        r.lay.piece(mask_of(|x, _, z| !(x == outer && z != up)), [x, iy.0, z.0], [x + m, iy.1, z.1], shade, 1);
    }
}

/// One leg, `s` square with its low corner at `at`, from `z.0` up to
/// `z.1`. `inner` says which way the middle of the piece lies along x and
/// y (true for towards the high side): a tapered leg narrows on those two
/// faces and keeps its outside corner upright, as a real one does, so
/// that the piece's outline is still square at the floor.
///
/// A leg too thin or too short for its kind is a square one. A turned
/// leg needs three cells across for a neck to be thinner than a ball, and
/// a tapered one two for there to be anything to take off.
pub(super) fn leg(r: &mut Room, at: [i32; 2], s: i32, z: (i32, i32), kind: Leg, inner: [bool; 2], shade: u8) {
    let [x, y] = at;
    let tall = z.1 - z.0;
    if tall < 1 || s < 1 {
        return;
    }
    match kind {
        Leg::Tapered if s >= 2 && tall >= 4 && r.sloped(s / 2, tall) => {
            // square for the top fifth, where an apron would meet it, and tapering below
            let head = (tall / 5).max(1);
            let (k, zt) = (s / 2, z.1 - head);
            r.block([x, y, zt], [x + s, y + s, z.1], shade);
            // the part that goes all the way down, at the outside corner; and beside it along each axis the part that tapers away
            let core = |inner: bool, at: i32| if inner { (at, at + s - k, at + s - k) } else { (at + k, at + s, at) };
            let ((cx0, cx1, tx), (cy0, cy1, ty)) = (core(inner[0], x), core(inner[1], y));
            r.block([cx0, cy0, z.0], [cx1, cy1, zt], shade);
            let (far_x, far_y) = (inner[0] as u8, inner[1] as u8);
            r.lay.piece(mask_of(|x, _, z| !(x == far_x && z == 0)), [tx, cy0, z.0], [tx + k, cy1, zt], shade, 1);
            r.lay.piece(mask_of(|_, y, z| !(y == far_y && z == 0)), [cx0, ty, z.0], [cx1, ty + k, zt], shade, 0);
            r.lay.piece(mask_of(|x, y, z| z == 1 || (x != far_x && y != far_y)), [tx, ty, z.0], [tx + k, ty + k, zt], shade, 2);
        }
        Leg::Turned if s >= 3 && tall >= 7 => {
            // from the floor up: a foot, a ball, a long shaft, a collar, a neck, and the square block an apron meets
            let (thin, ball, head) = (s - 2, (tall / 8).max(1), (tall / 4).max(2));
            let mut at = z.0;
            for (high, wide) in [(1, thin), (ball, s), (tall - head - ball - 3, thin), (1, s), (1, thin), (head, s)] {
                let inset = (s - wide) / 2;
                r.block([x + inset, y + inset, at], [x + s - inset, y + s - inset, at + high], shade);
                at += high;
            }
        }
        _ => r.block([x, y, z.0], [x + s, y + s, z.1], shade),
    }
}

/// Four legs at the corners of the rectangle `x` by `y`.
pub(super) fn four_legs(r: &mut Room, x: (i32, i32), y: (i32, i32), s: i32, z: (i32, i32), kind: Leg, shade: u8) {
    for (lx, right) in [(x.0, false), (x.1 - s, true)] {
        for (ly, back) in [(y.0, false), (y.1 - s, true)] {
            leg(r, [lx, ly], s, z, kind, [!right, !back], shade);
        }
    }
}

/// What fills a back between two posts: across `x`, one cell thick at
/// `y`, from `z.0` (the seat, or a cushion on it) up to `z.1` (the top of
/// the posts). Every member touches a post or a rail that does, so none
/// hangs in the air.
pub(super) fn back(r: &mut Room, x: (i32, i32), y: i32, z: (i32, i32), kind: Back, shade: u8, heavy: bool) {
    let (wide, tall) = (x.1 - x.0, z.1 - z.0);
    if wide < 1 || tall < 1 {
        return;
    }
    let rail = (tall / 6).clamp(1, 3) + (heavy && tall >= 10) as i32;
    // (how far above the seat the lowest member is: a gap to sit back into)
    let gap = if tall >= 5 { (tall / 5).max(1) } else { 0 };
    // upright members a cell wide with gaps between, as many as leave each gap a cell at least
    let uprights = |r: &mut Room, z: (i32, i32), most: i32| {
        let n = ((wide - 1) / 2).clamp(1, most);
        let gaps = shared(wide - n, n + 1, false);
        let mut at = x.0;
        for gap in &gaps[..n as usize] {
            at += gap;
            r.block([at, y, z.0], [at + 1, y + 1, z.1], shade);
            at += 1;
        }
    };
    match kind {
        Back::Panel => r.block([x.0, y, z.0 + gap], [x.1, y + 1, z.1], shade),
        Back::Ladder => {
            // rails across, the top one at the top, as many as leave two cells between them
            let n = ((tall - gap + 1) / (rail + 2)).clamp(1, 5);
            let gaps = shared(tall - gap - n * rail, n, true);
            let mut at = z.1;
            for space in gaps.iter().rev() {
                r.block([x.0, y, at - rail], [x.1, y + 1, at], shade);
                at -= rail + space;
            }
        }
        Back::Slats if tall - gap >= 2 * rail + 1 => {
            r.block([x.0, y, z.1 - rail], [x.1, y + 1, z.1], shade);
            r.block([x.0, y, z.0 + gap], [x.1, y + 1, z.0 + gap + rail], shade);
            uprights(r, (z.0 + gap + rail, z.1 - rail), 6);
        }
        Back::Spindles if tall >= rail + 2 => {
            // a deeper crest rail, and spindles from the seat itself up to it
            let crest = (rail + 1).min(tall - 1);
            r.block([x.0, y, z.1 - crest], [x.1, y + 1, z.1], shade);
            uprights(r, (z.0, z.1 - crest), 12);
        }
        _ => r.block([x.0, y, z.0 + gap], [x.1, y + 1, z.1], shade),
    }
}
