//! The bricks of the form being edited, kept by the square of ground each
//! stands on, so that a change to a very large form gives the graphics
//! card only the squares it changed.
//!
//! The view gives the card a form's bricks as buffers of instances (see
//! `bricks`), and filling a buffer costs by its size: measured, 1.0 to 1.5
//! ms for the 28,000 bricks of a lawn 180 cells by 120, about 45
//! nanoseconds a brick. That is little to save on. A lawn ten times the
//! size would be 10 to 15 ms a touch, and a touch of the brush changes
//! the bricks of a few tiles of it only (a lawn is built in tiles so that
//! this is so: see `brickloft_core::grass`). So a form of many bricks is
//! divided among several buffers, a set for each tile of ground, and only
//! the tiles whose bricks changed are filled again.
//!
//! A tile costs a draw call for each brick shape in it at every frame,
//! whether anything changes or not, so a form has no more tiles than its
//! size earns it: one for every [`EACH`] bricks, and a form smaller than
//! twice that is one tile, drawn exactly as if this were not here.
//!
//! The other way was one buffer with a tile's bricks in a part of it,
//! filled a part at a time (`InstanceBuffer::fill_subset`). A touch
//! changes how many bricks a tile has as well as which, so each part
//! would need room to grow into and be moved when it outgrew it: an
//! allocator, to save a few draw calls.
//!
//! What is here is the keeping and the comparing, which need no graphics
//! card and are tested without one. The buffers are filled in `lib.rs`
//! with what [`Tiles::take`] says has changed.

use brickloft_core::shapes::CellBrick;
use std::collections::BTreeMap;

/// What one brick is drawn from: the brick, and its colour.
pub type Drawn = (CellBrick, [u8; 3]);

/// How many bricks earn a form one more tile.
///
/// Filling a buffer was measured at about 45 nanoseconds a brick. A
/// touch on a form of `n` bricks in `t` tiles fills about `4 n / t`
/// bricks' worth (a touch reaches four tiles or so, and fewer where the
/// tiles are large), and every frame draws `t` tiles for each of some
/// dozen shapes. At 16,000 bricks a tile a touch fills 3 ms of buffers at
/// the most, whatever the form's size. What a tile's draw calls cost a
/// frame was not measured apart from the rest: a lawn of 86,000 bricks in
/// four tiles and one of 28,000 in one both took 1.0 ms a frame to draw.
pub const EACH: usize = 16_000;

/// The most tiles a form is divided into, however many bricks it has.
pub const MOST: usize = 64;

/// The least a tile is along each side, in cells: the size a lawn is built
/// in (`brickloft_core::grass::TILE`), so that a tile here is one or more
/// whole tiles there, and never part of one.
pub const LEAST: i32 = brickloft_core::grass::TILE;

/// Which tile a brick is in: the one its lowest corner is in.
pub type Key = [i32; 2];

/// The side of a tile that holds the whole form, wherever it is: every
/// brick is then in the tile numbered 0, 0. Not a size, since tiles lie
/// on a lattice from the form's own zero and a form that reaches to both
/// sides of zero is in two tiles each way however large they are.
const WHOLE: i32 = i32::MAX;

#[derive(Default)]
pub struct Tiles {
    /// how many cells a tile is along each side now, or [`WHOLE`]; 0 before any bricks have been taken
    side: i32,
    held: BTreeMap<Key, Vec<Drawn>>,
}

/// What [`Tiles::take`] found changed.
#[derive(Debug, Default, PartialEq)]
pub struct Changed {
    /// the tiles whose bricks are not what they were, the new ones among them
    pub now: Vec<Key>,
    /// the tiles that had bricks and have none
    pub gone: Vec<Key>,
}

impl Tiles {
    /// The side of a tile for a form of `bricks` bricks whose lowest
    /// corners are in the cells from `lo` to before `hi`: the least that
    /// divides it into no more tiles than its bricks earn it (see [`EACH`]).
    fn side_for(lo: [i32; 2], hi: [i32; 2], bricks: usize) -> i32 {
        let most = (bricks / EACH).clamp(1, MOST);
        if most == 1 {
            return WHOLE;
        }
        let mut side = LEAST;
        let count = |side: i32| (0..2).map(|k| ((hi[k] - 1).div_euclid(side) - lo[k].div_euclid(side) + 1).max(1) as usize).product::<usize>();
        // (while halving the tiles' number can still help: a form about its own zero is four tiles at the fewest)
        while count(side) > most && count(side) > 4 {
            side *= 2;
        }
        side
    }

    /// Take the form's bricks as they now are, and say which tiles differ
    /// from what was held. The bricks of a tile are compared as they come,
    /// in order: a build that lays the same bricks in another order is
    /// taken for another build, which costs a tile's bricks given to the
    /// card once more and nothing else.
    pub fn take(&mut self, bricks: impl Iterator<Item = Drawn> + Clone) -> Changed {
        // (once through them for how many there are and how far they reach, which the tiles' size goes by)
        let (mut count, mut lo, mut hi) = (0, [i32::MAX; 2], [i32::MIN; 2]);
        for (b, _) in bricks.clone() {
            count += 1;
            (lo, hi) = ([lo[0].min(b.min[0]), lo[1].min(b.min[1])], [hi[0].max(b.min[0] + 1), hi[1].max(b.min[1] + 1)]);
        }
        let side = Tiles::side_for(lo, hi, count);
        let mut changed = Changed::default();
        if side != self.side {
            // (tiles of another size are other tiles: all that were held are gone)
            changed.gone = std::mem::take(&mut self.held).into_keys().collect();
            self.side = side;
        }
        // Each brick to its tile. A build lays its bricks a tile at a
        // time, so the next brick is nearly always of the tile the last
        // was: that tile is kept in hand, and the table of tiles is
        // looked in only when the tile changes. Looked in for every
        // brick, taking a lawn of 130,000 bricks was 6 to 7 ms, and so
        // it is 3.5 (`how_long_taking_a_lawn_is`, below).
        let mut now: BTreeMap<Key, Vec<Drawn>> = BTreeMap::new();
        let mut hand: Option<(Key, Vec<Drawn>)> = None;
        for drawn in bricks {
            let key = if side == WHOLE { [0, 0] } else { [drawn.0.min[0].div_euclid(side), drawn.0.min[1].div_euclid(side)] };
            match &mut hand {
                Some((held, tile)) if *held == key => tile.push(drawn),
                _ => {
                    // (A tile new to this taking is given the room it filled the last time, which is near what it will fill now: grown as its bricks came, taking 130,000 bricks was 2.5 ms, and so it is 2.0.)
                    let room = || Vec::with_capacity(self.held.get(&key).map_or(0, |was| was.len() + was.len() / 16));
                    if let Some((held, tile)) = hand.replace((key, now.remove(&key).unwrap_or_else(room))) {
                        now.insert(held, tile);
                    }
                    hand.as_mut().unwrap().1.push(drawn);
                }
            }
        }
        if let Some((held, tile)) = hand {
            now.insert(held, tile);
        }
        changed.gone.extend(self.held.keys().filter(|key| !now.contains_key(*key)).copied());
        changed.now = now.iter().filter(|(key, bricks)| self.held.get(*key) != Some(*bricks)).map(|(key, _)| *key).collect();
        // (a tile gone and come again in one go, as when the size changed, is one that is new)
        changed.gone.retain(|key| !changed.now.contains(key));
        self.held = now;
        changed
    }

    /// The bricks held for a tile.
    pub fn of(&self, key: &Key) -> &[Drawn] {
        self.held.get(key).map_or(&[], |bricks| bricks)
    }

    /// How many bricks are held in all.
    pub fn count(&self) -> usize {
        self.held.values().map(|bricks| bricks.len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use brickloft_core::shapes::CUBE;

    fn brick(x: i32, y: i32, z: i32) -> Drawn {
        (CellBrick { mask: CUBE, min: [x, y, z], ext: [1; 3] }, [10, 20, 30])
    }

    /// A field of bricks, as many as earn it every tile it lies in: `across` tiles' width by `along` of the least size, from 40 cells below zero each way (so it lies in one tile more each way than that), each brick with a number of its own for a colour.
    fn field(across: i32, along: i32) -> Vec<Drawn> {
        let each = (EACH as i32 * 2).div_euclid(LEAST * LEAST) + 1;
        (0..along * LEAST).flat_map(|y| (0..across * LEAST).flat_map(move |x| (0..each).map(move |z| (CellBrick { mask: CUBE, min: [x - 40, y - 40, z], ext: [1; 3] }, [(x & 255) as u8, (y & 255) as u8, z as u8])))).collect()
    }

    #[test]
    fn only_the_tiles_whose_bricks_changed_are_said_to_have() {
        let mut tiles = Tiles::default();
        let first = field(4, 3);
        // (tiles to the left of and below zero too)
        let all = tiles.take(first.iter().copied());
        assert_eq!((all.now.len(), all.gone.len(), tiles.count()), (5 * 4, 0, first.len()));
        assert!(tiles.of(&[-2, -2]).len() > 0 && tiles.of(&[-2, -2]).iter().all(|(b, _)| b.min[0] < -32 && b.min[1] < -32));
        // the same again: nothing
        assert_eq!(tiles.take(first.iter().copied()), Changed::default());
        // one brick moved within its tile, one recoloured in another: those two tiles and no more
        let mut second = first.clone();
        let (moved, recoloured) = (second.iter().position(|(b, _)| b.min == [5, 5, 0]).unwrap(), second.iter().position(|(b, _)| b.min == [40, 5, 0]).unwrap());
        second[moved].0.min[2] = 99;
        second[recoloured].1 = [1, 2, 3];
        let some = tiles.take(second.iter().copied());
        assert_eq!((some.now, some.gone), (vec![[0, 0], [1, 0]], vec![]));
        assert!(tiles.of(&[1, 0]).contains(&second[recoloured]));
        // a tile emptied is gone, and the rest are as they were
        let third: Vec<Drawn> = second.iter().copied().filter(|(b, _)| b.min[0] < 64).collect();
        let last = tiles.take(third.iter().copied());
        assert!(last.now.is_empty() && last.gone.len() == 4 && last.gone.iter().all(|key| key[0] == 2), "{last:?}");
    }

    #[test]
    fn a_form_of_few_bricks_is_one_tile_however_far_it_reaches() {
        let mut tiles = Tiles::default();
        // (to both sides of zero, each way)
        let few: Vec<Drawn> = (-20..20).flat_map(|i| (-20..20).map(move |j| brick(i * 30, j * 30, 0))).collect();
        let all = tiles.take(few.iter().copied());
        assert_eq!((all.now.len(), tiles.count()), (1, 1600));
    }

    #[test]
    fn a_form_too_large_for_the_most_tiles_has_larger_ones() {
        let mut tiles = Tiles::default();
        // 9 tiles by 9 of the least size, which is over the most however many bricks: tiles of twice the side, 5 by 5
        let large = field(8, 8);
        assert_eq!(tiles.take(large.iter().copied()).now.len(), 25);
        // a quarter of the field lies in 5 tiles by 5 of the least size and has the bricks for them: all of those are new, and the larger tiles that are no tile's number now are gone
        let quarter = field(4, 4);
        let changed = tiles.take(quarter.iter().copied());
        assert_eq!((changed.now.len(), changed.gone.len(), tiles.count()), (25, 9, quarter.len()));
    }

    /// A lawn of over a hundred thousand bricks, as the view takes it: each brick with a colour for its part.
    fn lawn(touches: &[(f64, f64)]) -> Vec<Drawn> {
        use brickloft_core::spec::{build, Form, Spec};
        use brickloft_core::tree::{Kind, Tree};
        use brickloft_core::DVec3;
        let pulls = touches.iter().map(|(x, y)| (DVec3::new(*x, *y, 0.5), DVec3::new(20.0, 0.0, 7.0))).collect();
        // (dry: a pond's level is found from the whole of the ground)
        let spec = Spec { form: Form::Tree, points: vec![DVec3::new(-99.5, 0.5, 0.5), DVec3::new(0.5, 0.5, 0.5), DVec3::new(100.5, 0.5, 0.5)], radius: 120.0, rise: Some(5.0), tree: Tree { count: 0, ..Tree::of(Kind::Grass) }, pulls, ..Spec::default() };
        let built = build(&spec).unwrap();
        built.bricks.iter().zip(&built.shades).map(|(b, shade)| (*b, [*shade, 100, 50])).collect()
    }

    #[test]
    fn a_touch_on_a_large_lawn_changes_a_few_of_its_tiles() {
        let mut tiles = Tiles::default();
        let before = lawn(&[(-150.5, 40.5), (90.5, -60.5)]);
        let all = tiles.take(before.iter().copied());
        assert!(before.len() > 6 * EACH && all.now.len() >= 6, "{} bricks in {} tiles", before.len(), all.now.len());
        // one more touch, 20 in range, near one end
        let after = lawn(&[(-150.5, 40.5), (90.5, -60.5), (-60.5, -30.5)]);
        let changed = tiles.take(after.iter().copied());
        let given: usize = changed.now.iter().map(|key| tiles.of(key).len()).sum();
        assert!(!changed.now.is_empty() && changed.gone.is_empty());
        // (a touch 40 across with a blade's reach round it lies in four tiles at most, of the lawn's six or more)
        assert!(changed.now.len() <= 4 && given * 10 <= after.len() * 7, "{} of {} tiles changed: {given} of {} bricks to give the card again", changed.now.len(), all.now.len(), after.len());
    }

    /// Not a test: how long taking a large lawn's bricks is. Run by name with `--release -- --ignored --nocapture`.
    #[test]
    #[ignore = "a measurement, not a check"]
    fn how_long_taking_a_lawn_is() {
        let (before, after) = (lawn(&[(-150.5, 40.5), (90.5, -60.5)]), lawn(&[(-150.5, 40.5), (90.5, -60.5), (-60.5, -30.5)]));
        let mut tiles = Tiles::default();
        let mut least = [std::time::Duration::MAX; 2];
        for _ in 0..20 {
            for (n, bricks) in [&before, &after].into_iter().enumerate() {
                let at = std::time::Instant::now();
                std::hint::black_box(tiles.take(bricks.iter().copied()));
                least[n] = least[n].min(at.elapsed());
            }
        }
        println!("taking {} bricks: {:?}, and after a touch {:?}", before.len(), least[0], least[1]);
    }
}
