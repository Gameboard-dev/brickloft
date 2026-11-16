//! Wave function collapse: filling a grid with tiles so that every tile
//! agrees with its neighbours.
//!
//! The method is Maxim Gumin's:
//! https://github.com/mxgmn/WaveFunctionCollapse
//! Every cell of the grid starts able to hold any tile. Then, over and
//! over: the cell with the fewest choices left is given one of them at
//! random (it is "observed"), and every choice that no longer agrees with
//! a neighbour is struck out of the cells round it, and round those, until
//! nothing more changes (the choice is "propagated"). It ends with one tile
//! in every cell, or with a cell left no tile at all: a contradiction.
//!
//! Two kinds of rule are given here, as in the original. [`Rules`] says
//! outright which tile may sit beside which. [`learn`] works the rules out
//! from a small sample picture: its tiles are the sample's own N by N
//! windows, and two may sit side by side where they agree on the part
//! they share. A grid filled that way looks like the sample everywhere
//! and repeats it nowhere, which is what a floor mosaic wants: the motifs
//! of the sample, laid out afresh.
//!
//! ## A contradiction
//!
//! The original stops at a contradiction and reports failure. This starts
//! again with the next numbers from the same seed, up to a given number of
//! times, which is what the algorithm it grew from did (Merrell's model
//! synthesis). Stepping back one choice at a time is the other answer. It
//! finds a filling whenever one exists, and can take unboundedly long
//! over a grid that has none; a bounded number of fresh starts cannot.
//! One seed is still one result, since the fresh starts are part of it.
//!
//! ## What is kept for each cell
//!
//! One bit a tile, in whole words: striking tiles out and asking what a
//! neighbour may still be are then a few machine instructions a word,
//! whatever the number of tiles. The original keeps a count for every
//! tile of how many neighbours still support it, which propagates faster
//! on large grids of many tiles and takes four numbers a tile a cell. A
//! floor here is tens of cells across with a few dozen tiles, where the
//! bits are both smaller and simpler.

/// The four ways to a neighbour in a flat grid: +x, -x, +y, -y. A way and
/// its opposite differ in the lowest bit.
const WAYS: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

/// The most tiles a set of rules may have. A sample a few dozen cells
/// across has under two hundred windows; past this the sample is not a
/// pattern but a picture, and the grid would take seconds.
pub const MOST_TILES: usize = 1024;

/// Numbers that look random and are the same every time for one seed
/// (SplitMix64: see `pattern::Numbers`, which this repeats so that neither
/// module needs the other).
/// https://prng.di.unimi.it/splitmix64.c
struct Numbers(u64);

impl Numbers {
    fn unit(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        ((z ^ (z >> 31)) >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Which tiles may sit beside which, and how often each is wanted.
#[derive(Clone, Debug)]
pub struct Rules {
    tiles: usize,
    /// words to a set of tiles
    words: usize,
    /// how much more often than another a tile is chosen
    weights: Vec<f64>,
    /// for each way and each tile, the set of tiles that may be its
    /// neighbour that way
    beside: Vec<u64>,
}

impl Rules {
    /// Rules for `weights.len()` tiles, none of which may yet sit beside any.
    pub fn new(weights: Vec<f64>) -> Result<Rules, String> {
        let tiles = weights.len();
        if tiles == 0 || tiles > MOST_TILES {
            return Err(format!("a pattern needs from 1 to {MOST_TILES} tiles, and this has {tiles}"));
        }
        if weights.iter().any(|w| !(w.is_finite() && *w > 0.0)) {
            return Err("a tile's weight must be more than nothing".into());
        }
        let words = tiles.div_ceil(64);
        Ok(Rules { tiles, words, weights, beside: vec![0; 4 * tiles * words] })
    }

    pub fn tiles(&self) -> usize {
        self.tiles
    }

    /// Let tile `b` sit beside tile `a` on its `way` side (0 to 3: +x, -x,
    /// +y, -y), which lets `a` sit beside `b` on the opposite side.
    pub fn allow(&mut self, a: usize, way: usize, b: usize) {
        for (from, way, to) in [(a, way, b), (b, way ^ 1, a)] {
            self.beside[(way * self.tiles + from) * self.words + to / 64] |= 1 << (to % 64);
        }
    }

    fn beside(&self, way: usize, tile: usize) -> &[u64] {
        let at = (way * self.tiles + tile) * self.words;
        &self.beside[at..at + self.words]
    }
}

/// A grid to be filled: its size, the tiles each cell may still hold, and
/// the cells that are left out.
pub struct Grid<'a> {
    rules: &'a Rules,
    size: [usize; 2],
    /// for each cell, the set of tiles it may hold
    may: Vec<u64>,
    /// a cell left out holds nothing and asks nothing of its neighbours
    out: Vec<bool>,
}

impl<'a> Grid<'a> {
    /// A grid `size` cells across and down, every cell able to hold any tile.
    pub fn new(rules: &'a Rules, size: [usize; 2]) -> Grid<'a> {
        let mut may = vec![u64::MAX; size[0] * size[1] * rules.words];
        // (the last word of a set has no tiles past the last)
        let spare = rules.words * 64 - rules.tiles;
        for cell in 0..size[0] * size[1] {
            may[(cell + 1) * rules.words - 1] = u64::MAX >> spare;
        }
        Grid { rules, size, may, out: vec![false; size[0] * size[1]] }
    }

    /// Leave a cell out: outside the floor.
    pub fn leave_out(&mut self, x: usize, y: usize) {
        self.out[y * self.size[0] + x] = true;
    }

    /// Let a cell hold only the tiles `keep` says yes to: a part of the
    /// floor that is already decided.
    pub fn only(&mut self, x: usize, y: usize, keep: impl Fn(usize) -> bool) {
        let words = self.rules.words;
        let at = (y * self.size[0] + x) * words;
        for tile in (0..self.rules.tiles).filter(|t| !keep(*t)) {
            self.may[at + tile / 64] &= !(1 << (tile % 64));
        }
    }

    /// Fill the grid. Returns the tile in each cell, row by row (`None`
    /// for a cell left out), or why it could not be filled. `tries` is how
    /// many times to start again after a contradiction.
    pub fn fill(&self, seed: u64, tries: usize) -> Result<Vec<Option<usize>>, String> {
        let mut numbers = Numbers(seed ^ 0x2545_F491_4F6C_DD1D);
        for _ in 0..tries.max(1) {
            if let Some(filled) = self.attempt(&mut numbers) {
                return Ok(filled);
            }
        }
        Err(format!("the pattern could not be completed in {} tries: try another seed", tries.max(1)))
    }

    fn attempt(&self, numbers: &mut Numbers) -> Option<Vec<Option<usize>>> {
        let (rules, words, [across, down]) = (self.rules, self.rules.words, self.size);
        let mut may = self.may.clone();
        let count = |may: &[u64], cell: usize| may[cell * words..(cell + 1) * words].iter().map(|w| w.count_ones()).sum::<u32>();
        // How much choice each cell has left, by Shannon entropy over its
        // tiles' weights, which is the original's measure: among cells with
        // the same number of tiles left, it prefers the one whose tiles are
        // most unequal, since that one is nearest decided. A cell with one
        // tile left, or left out, has none to make and is never picked.
        // https://en.wikipedia.org/wiki/Entropy_(information_theory)
        //
        // Kept for every cell and worked out again only for a cell whose
        // tiles have changed. Working all of them out before every choice
        // was nine tenths of the time: a floor of 1,800 cells and 88 tiles
        // took 290 ms that way.
        let choice = |may: &[u64], cell: usize, noise: f64| {
            if self.out[cell] || count(may, cell) < 2 {
                return f64::INFINITY;
            }
            let (mut sum, mut logs) = (0.0, 0.0);
            for tile in (0..rules.tiles).filter(|t| may[cell * words + t / 64] >> (t % 64) & 1 == 1) {
                let w = rules.weights[tile];
                sum += w;
                logs += w * w.ln();
            }
            sum.ln() - logs / sum + noise
        };
        // (a little noise, fixed for each cell, so that equal cells are not always taken in reading order)
        let noise: Vec<f64> = (0..across * down).map(|_| 1e-6 * numbers.unit()).collect();
        let mut left: Vec<f64> = (0..across * down).map(|cell| choice(&may, cell, noise[cell])).collect();
        // what was decided before the start is propagated first
        let mut changed: Vec<usize> = (0..across * down).filter(|c| !self.out[*c]).collect();
        loop {
            // strike out of each neighbour what nothing in this cell allows beside it
            while let Some(cell) = changed.pop() {
                let (x, y) = ((cell % across) as i32, (cell / across) as i32);
                for (way, (dx, dy)) in WAYS.iter().enumerate() {
                    let (nx, ny) = (x + dx, y + dy);
                    if nx < 0 || ny < 0 || nx >= across as i32 || ny >= down as i32 {
                        continue;
                    }
                    let other = ny as usize * across + nx as usize;
                    if self.out[other] {
                        continue;
                    }
                    let mut allowed = vec![0u64; words];
                    for tile in 0..rules.tiles {
                        if may[cell * words + tile / 64] >> (tile % 64) & 1 == 1 {
                            for (word, beside) in allowed.iter_mut().zip(rules.beside(way, tile)) {
                                *word |= beside;
                            }
                        }
                    }
                    let mut struck = false;
                    for (word, allowed) in may[other * words..(other + 1) * words].iter_mut().zip(&allowed) {
                        struck |= *word & !allowed != 0;
                        *word &= allowed;
                    }
                    if struck {
                        if count(&may, other) == 0 {
                            return None; // a contradiction
                        }
                        left[other] = choice(&may, other, noise[other]);
                        changed.push(other);
                    }
                }
            }
            // the cell with the least choice left
            let least = left.iter().enumerate().filter(|(_, l)| l.is_finite()).min_by(|a, b| a.1.total_cmp(b.1)).map(|(cell, l)| (*l, cell));
            let Some((_, cell)) = least else { break };
            // one of its tiles, the heavier the likelier
            let held: Vec<usize> = (0..rules.tiles).filter(|t| may[cell * words + t / 64] >> (t % 64) & 1 == 1).collect();
            let mut pick = numbers.unit() * held.iter().map(|t| rules.weights[*t]).sum::<f64>();
            let chosen = *held.iter().find(|t| {
                pick -= rules.weights[**t];
                pick < 0.0
            })
            .unwrap_or(held.last().unwrap());
            may[cell * words..(cell + 1) * words].fill(0);
            may[cell * words + chosen / 64] = 1 << (chosen % 64);
            left[cell] = f64::INFINITY;
            changed.push(cell);
        }
        Some((0..across * down).map(|cell| (!self.out[cell]).then(|| (0..rules.tiles).find(|t| may[cell * words + t / 64] >> (t % 64) & 1 == 1).unwrap_or(0))).collect())
    }
}

/// A sample picture: colours (small numbers) row by row.
#[derive(Clone, Debug, PartialEq)]
pub struct Sample {
    pub across: usize,
    pub down: usize,
    pub colours: Vec<u8>,
}

impl Sample {
    /// Read a sample from text: a row of digits to a line, each digit a
    /// colour. Lines that are empty or start with `#` are skipped.
    pub fn read(text: &str) -> Result<Sample, String> {
        let rows: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).collect();
        let (down, across) = (rows.len(), rows.first().map_or(0, |r| r.len()));
        if across == 0 || across > 64 || down > 64 || rows.iter().any(|r| r.len() != across) {
            return Err("a sample is 1 to 64 rows of digits, all the same length".into());
        }
        let colours: Option<Vec<u8>> = rows.iter().flat_map(|r| r.bytes()).map(|b| b.is_ascii_digit().then(|| b - b'0')).collect();
        Ok(Sample { across, down, colours: colours.ok_or("a sample is made of digits only")? })
    }

    /// The colour at a place, the sample repeating in every direction.
    fn at(&self, x: i32, y: i32) -> u8 {
        self.colours[y.rem_euclid(self.down as i32) as usize * self.across + x.rem_euclid(self.across as i32) as usize]
    }
}

/// The rules a sample teaches, and the colour each tile stands for.
pub struct Learnt {
    pub rules: Rules,
    /// the colour a cell holding each tile is: the top left of its window
    pub colour: Vec<u8>,
}

/// Work the rules out from a sample: the tiles are its `n` by `n` windows
/// (the sample taken to repeat past its edges), each as often wanted as it
/// is found, and two may sit side by side where the parts of their windows
/// that then overlap are the same.
///
/// `turned` adds each window's quarter turns and mirror images as tiles
/// too, for a sample whose motifs may lie any way round.
pub fn learn(sample: &Sample, n: usize, turned: bool) -> Result<Learnt, String> {
    if !(2..=4).contains(&n) {
        return Err("windows are 2 to 4 cells across".into());
    }
    let n = n as i32;
    let mut windows: Vec<(Vec<u8>, f64)> = Vec::new();
    for y in 0..sample.down as i32 {
        for x in 0..sample.across as i32 {
            let window: Vec<u8> = (0..n * n).map(|k| sample.at(x + k % n, y + k / n)).collect();
            // the window itself, then its turns and mirror images
            let mut ways = vec![window];
            if turned {
                for k in 0..7 {
                    let last = &ways[k];
                    // (a quarter turn each time, and a mirror image half way through)
                    let next: Vec<u8> = if k == 3 { (0..n * n).map(|i| last[(i / n * n + (n - 1 - i % n)) as usize]).collect() } else { (0..n * n).map(|i| last[((n - 1 - i % n) * n + i / n) as usize]).collect() };
                    ways.push(next);
                }
            }
            for window in ways {
                match windows.iter_mut().find(|w| w.0 == window) {
                    Some(found) => found.1 += 1.0,
                    None => windows.push((window, 1.0)),
                }
            }
        }
    }
    let mut rules = Rules::new(windows.iter().map(|w| w.1).collect())?;
    for (a, (first, _)) in windows.iter().enumerate() {
        for (b, (second, _)) in windows.iter().enumerate() {
            // `second` one cell on from `first` along x, and along y
            for (way, (dx, dy)) in [(0, (1, 0)), (2, (0, 1))] {
                let agree = (0..n * n).all(|i| {
                    let (x, y) = (i % n + dx, i / n + dy);
                    x >= n || y >= n || first[(y * n + x) as usize] == second[((y - dy) * n + (x - dx)) as usize]
                });
                if agree {
                    rules.allow(a, way, b);
                }
            }
        }
    }
    Ok(Learnt { colour: windows.iter().map(|w| w.0[0]).collect(), rules })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two tiles that may only alternate: a chessboard.
    fn chess() -> Rules {
        let mut rules = Rules::new(vec![1.0, 1.0]).unwrap();
        for way in 0..4 {
            rules.allow(0, way, 1);
        }
        rules
    }

    #[test]
    fn tiles_that_must_alternate_fill_a_grid_as_a_chessboard() {
        let rules = chess();
        let filled = Grid::new(&rules, [9, 7]).fill(1, 1).unwrap();
        let first = filled[0].unwrap();
        for (cell, tile) in filled.iter().enumerate() {
            assert_eq!(tile.unwrap(), (first + cell % 9 + cell / 9) % 2, "cell {cell}");
        }
    }

    #[test]
    fn a_cell_decided_beforehand_is_kept_and_an_impossible_one_is_reported() {
        let rules = chess();
        let mut grid = Grid::new(&rules, [6, 6]);
        grid.only(2, 3, |t| t == 1);
        let filled = grid.fill(5, 1).unwrap();
        assert_eq!(filled[3 * 6 + 2], Some(1));
        assert_eq!(filled[3 * 6 + 3], Some(0));
        // two neighbours both made the same tile cannot be a chessboard
        grid.only(3, 3, |t| t == 1);
        assert!(grid.fill(5, 4).is_err());
    }

    #[test]
    fn cells_left_out_hold_nothing_and_ask_nothing() {
        let rules = chess();
        let mut grid = Grid::new(&rules, [5, 1]);
        grid.leave_out(2, 0);
        // either side of the gap may be anything: both made tile 1, two apart, which a row could not be
        grid.only(1, 0, |t| t == 1);
        grid.only(3, 0, |t| t == 1);
        let filled = grid.fill(3, 1).unwrap();
        assert_eq!(filled, [Some(0), Some(1), None, Some(1), Some(0)]);
    }

    #[test]
    fn one_seed_is_one_filling_and_another_seed_another() {
        // three tiles, any beside any: nothing but the seed decides
        let mut rules = Rules::new(vec![1.0, 2.0, 1.0]).unwrap();
        for a in 0..3 {
            for b in 0..3 {
                for way in 0..4 {
                    rules.allow(a, way, b);
                }
            }
        }
        let grid = Grid::new(&rules, [12, 12]);
        assert_eq!(grid.fill(7, 1).unwrap(), grid.fill(7, 1).unwrap());
        assert_ne!(grid.fill(7, 1).unwrap(), grid.fill(8, 1).unwrap());
        // the heavier tile is the commoner
        let filled = grid.fill(7, 1).unwrap();
        let of = |tile: usize| filled.iter().filter(|t| **t == Some(tile)).count();
        assert!(of(1) > of(0) && of(1) > of(2), "{} {} {}", of(0), of(1), of(2));
    }

    #[test]
    fn a_grid_learnt_from_a_sample_holds_only_windows_the_sample_has() {
        // diagonal stripes, three colours
        let sample = Sample::read("012\n120\n201\n").unwrap();
        let learnt = learn(&sample, 2, false).unwrap();
        // nine places, and each window is found three times over
        assert_eq!(learnt.rules.tiles(), 3);
        let (across, down) = (10, 8);
        let filled = Grid::new(&learnt.rules, [across, down]).fill(11, 4).unwrap();
        let colour = |x: usize, y: usize| learnt.colour[filled[y * across + x].unwrap()];
        // stripes again: one step right or down is the next colour
        for y in 0..down - 1 {
            for x in 0..across - 1 {
                assert_eq!((colour(x + 1, y), colour(x, y + 1)), ((colour(x, y) + 1) % 3, (colour(x, y) + 1) % 3), "at {x}, {y}");
            }
        }
        assert!(Sample::read("01\n0\n").is_err() && Sample::read("0a\n01\n").is_err() && Sample::read("").is_err());
    }

    #[test]
    fn turned_windows_let_a_motif_lie_any_way_round() {
        // a single L of colour 1 on a field of 0
        let sample = Sample::read("0000\n0100\n0110\n0000\n").unwrap();
        let (plain, turned) = (learn(&sample, 3, false).unwrap(), learn(&sample, 3, true).unwrap());
        assert!(turned.rules.tiles() > plain.rules.tiles(), "{} turned, {} plain", turned.rules.tiles(), plain.rules.tiles());
        assert!(Grid::new(&turned.rules, [14, 14]).fill(2, 8).is_ok());
    }
}
