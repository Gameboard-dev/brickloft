//! What is kept of a lawn from one build to the next, so that a touch of
//! the brush costs by the ground it changes and not by the lawn's size.
//!
//! A lawn is built again at every touch. Built from nothing, a lawn 600
//! cells by 480 with 120 touches took 44 to 55 ms on sixteen threads and
//! 300 ms on one, about half of it the ground and the rest the tiles,
//! though a touch 20 in range changes the ground of some 2,000 of its
//! 288,000 cells and the bricks of six of its 320 tiles. From what is
//! kept the same touch is 4 to 8 ms (9 to 13 on one thread); see
//! docs/performance.md. Two things are kept:
//!
//! - the ground, with the touches it was made from, so the next ground is
//!   this one with the heights worked out again where the touches differ
//!   (`Ground::after`);
//! - each tile's bricks, with the piece of ground and the reeds they were
//!   laid from. A tile is laid again unless that piece is, number for
//!   number, the piece that would be cut for it now.
//!
//! What is kept can make a build quicker and can do nothing else. A tile
//! is laid from the piece of ground kept with it and from nothing more
//! (`Ground::cut`), in a build from nothing as in any other, so there is
//! nothing a tile's bricks could depend on that the comparison leaves
//! out. The other way to know a kept tile is still good was to mark the
//! cells a touch changed and take a tile whose cells were unmarked: less
//! to compare, and right only as long as every way the ground can change
//! remembers to mark. Comparing a tile's ground is 4 to 9 microseconds,
//! 1.3 to 2.8 ms for all 320 tiles of the lawn above; a hash of it in
//! place of the ground itself would save the memory and could, once in a
//! very long while, take one tile for another.
//!
//! The lawns kept are few ([`MOST_LAWNS`]) and take no more than
//! [`MOST_BYTES`] between them; the one built longest ago goes first. A
//! build takes its lawn out of the store, works on it with the store
//! unlocked, and puts it back, so two threads building at once (the
//! editor builds a slow form behind the window) never wait for one
//! another's building: if both want one lawn, the second finds nothing
//! kept and builds from nothing, which is the same lawn.

use super::{Ground, Model, Site, Standing, Sums, Tuft};
use crate::shapes::CellBrick;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

/// How many lawns have something kept of them at once: the one being
/// sculpted, and a few that stand beside it or were undone to.
pub(super) const MOST_LAWNS: usize = 4;

/// The most that is kept of all of them, in bytes. A lawn 600 cells by
/// 480 is about 45 million: 9 for its ground, 13 for the pieces of ground
/// its tiles were laid from, and 10 twice over for its 335,000 bricks, by
/// the tile and joined up.
pub(super) const MOST_BYTES: usize = 192 << 20;

/// For each cell a reed stands in or over, the heights it fills there, in the cells' order.
pub(super) type Stems = Vec<([i32; 2], Vec<(i32, i32)>)>;

/// One tile's bricks, and all that they were laid from.
pub(super) struct Tile {
    /// the rule's numbers (`Rule::bits`)
    pub rule: [u64; 8],
    /// the ground of the tile's cells and of those round them
    pub cut: Ground,
    /// the reeds over the tile's own cells
    pub stems: Stems,
    pub bricks: Vec<CellBrick>,
    pub shades: Vec<u8>,
    /// the water standing in it, which is joined with the other tiles' (see `pooled`)
    pub water: Vec<Standing>,
    /// the bricks' volume and their box
    pub sums: Sums,
}

/// What is kept of one lawn.
#[derive(Default)]
pub(super) struct Lawn {
    /// which lawn: `None` for none yet
    pub site: Option<Site>,
    pub ground: Option<Arc<Ground>>,
    pub tiles: HashMap<[i32; 2], Tile>,
    /// the tufts a reed may stand at, and the rule they were found by
    pub tufts: Option<([u64; 8], Vec<Tuft>)>,
    /// the water as it stood, the longest a brick might be, and the bricks it was joined into
    pub pool: Option<(Vec<Standing>, i32, Vec<(CellBrick, u8)>)>,
    /// the lawn last built, and the rule it was built by
    pub last: Option<([u64; 8], Arc<Model>)>,
    /// how many tiles the last build laid: what the keeping is for is that these are few
    pub laid: usize,
    /// when it was last built, as a number that only grows
    used: u64,
}

impl Lawn {
    /// How many bytes it takes, near enough.
    fn bytes(&self) -> usize {
        let tiles: usize = self.tiles.values().map(|tile| tile.cut.bytes() + tile.bricks.len() * 29 + tile.water.len() * 16 + tile.stems.len() * 48).sum();
        self.ground.as_ref().map_or(0, |ground| ground.bytes()) + tiles + self.last.as_ref().map_or(0, |(_, model)| model.bricks.len() * 29) + self.pool.as_ref().map_or(0, |(water, _, pool)| water.len() * 16 + pool.len() * 29) + self.tufts.as_ref().map_or(0, |(_, tufts)| tufts.len() * 32)
    }
}

static KEPT: Mutex<Vec<Lawn>> = Mutex::new(Vec::new());
static TURN: AtomicU64 = AtomicU64::new(1);

/// The store. (A build that came to grief while it held the store left it
/// as it was a moment before, which is a store like any other: nothing in
/// it is half written, since lawns are put in and taken out whole.)
fn store() -> MutexGuard<'static, Vec<Lawn>> {
    KEPT.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// `build`, given what is kept of the lawn at `site` to work on, which is
/// nothing if there is none or another thread is building it now. What
/// `build` leaves is kept for the next.
pub(super) fn with<R>(site: &Site, build: impl FnOnce(&mut Lawn) -> R) -> R {
    let mut lawn = {
        let mut kept = store();
        kept.iter().position(|lawn| lawn.site.as_ref() == Some(site)).map(|n| kept.swap_remove(n)).unwrap_or_default()
    };
    let built = build(&mut lawn);
    lawn.used = TURN.fetch_add(1, Ordering::Relaxed);
    // (a lawn too large to keep whole keeps its ground, or nothing)
    if lawn.bytes() > MOST_BYTES {
        (lawn.tiles, lawn.last, lawn.pool) = (HashMap::new(), None, None);
    }
    if lawn.bytes() > MOST_BYTES {
        return built;
    }
    let mut kept = store();
    kept.retain(|other| other.site != lawn.site);
    kept.push(lawn);
    while kept.len() > MOST_LAWNS || kept.iter().map(Lawn::bytes).sum::<usize>() > MOST_BYTES {
        let Some(oldest) = (0..kept.len()).min_by_key(|n| kept[*n].used) else { break };
        kept.swap_remove(oldest);
    }
    built
}

/// How many lawns are kept, and how many bytes they take.
#[cfg(test)]
pub(super) fn held() -> (usize, usize) {
    let kept = store();
    (kept.len(), kept.iter().map(Lawn::bytes).sum())
}
