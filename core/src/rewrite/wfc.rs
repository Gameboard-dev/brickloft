//! The `wfc` node with a tileset: wave function collapse over tiles.
//!
//! Each cell of the grid the node is given becomes one tile in a new,
//! finer grid. A tileset names its tiles (each a small box of cells kept
//! in a .vox file), and says which may stand beside or on top of which.
//! Every cell starts able to be any tile the program's `rule`s allow for
//! the value it holds. Then the cell with the fewest tiles left is given
//! one of them by weight, and what that rules out next to it is struck
//! out, and so on until every cell has one tile.
//!
//! This is `WaveFunctionCollapse.cs` and `TileModel.cs` of the original,
//! and through them Gumin's simple tiled model
//! (https://github.com/mxgmn/WaveFunctionCollapse), which is in turn
//! Merrell's discrete model synthesis (Paul Merrell, "Model Synthesis",
//! PhD thesis, 2009: https://paulmerrell.org/thesis.pdf) with the cell of
//! least entropy taken first. A contradiction is met as the original
//! meets it: the whole node is tried again from another seed, up to
//! `tries` times, before any of it is shown, and the try that worked is
//! then played back a cell a turn. Stepping back a choice at a time was
//! the alternative; it would show cells being undone, and would not be
//! what a program written for the original expects of `tries`.
//!
//! The count each tile keeps of the neighbours that still allow it (the
//! AC-4 scheme of Mohr and Henderson, 1986) is the original's too. It was
//! kept, over the plain sets of bits `core/src/wfc.rs` uses, because the
//! order in which cells are struck out, and so which seed first succeeds,
//! follows from it.

use super::fault::{within, Fault, Limits, Thing};
use super::nodes::{ln, Ip, Random};
use super::rule::{cube_symmetries, flag, number, square_symmetries, Grid};
use super::xml::Element;
use super::Resources;

const DX: [i64; 6] = [1, 0, -1, 0, 0, 0];
const DY: [i64; 6] = [0, 1, 0, -1, 0, 0];
const DZ: [i64; 6] = [0, 0, 0, 0, 1, -1];
const OPPOSITE: [usize; 6] = [2, 3, 0, 1, 5, 4];

/// The most cells one node may be given a tile for across all its tries
/// before it is given up: a node that keeps failing on a large grid would
/// otherwise hold the turn it is tried in for minutes.
const MOST_OBSERVATIONS: u64 = 4_000_000;

#[derive(Clone)]
struct Wave {
    /// for each cell and tile, whether the cell may still be that tile
    data: Vec<bool>,
    /// for each cell, tile and direction, how many tiles of the neighbour that way still allow it
    compatible: Vec<i32>,
    sums_of_ones: Vec<i32>,
    sums_of_weights: Vec<f64>,
    sums_of_weight_log_weights: Vec<f64>,
    entropies: Vec<f64>,
}

pub struct TileNode {
    /// the grid whose cells become tiles
    pub from: usize,
    line: usize,
    tiledata: Vec<Vec<u8>>,
    /// a tile's side across, and its height
    s: usize,
    sz: usize,
    /// how far each tile is laid over the last; under nothing, how far apart they stand
    overlap: i64,
    overlapz: i64,
    periodic: bool,
    shannon: bool,
    tries: usize,
    weights: Vec<f64>,
    weight_log_weights: Vec<f64>,
    sum_of_weights: f64,
    sum_of_weight_log_weights: f64,
    starting_entropy: f64,
    /// for each direction and tile, the tiles that may be its neighbour that way
    propagator: [Vec<Vec<u32>>; 6],
    /// for each value of the old grid, the tiles a cell holding it may be; none for any
    map: Vec<Option<Vec<bool>>>,
    wave: Wave,
    startwave: Wave,
    stack: Vec<(u32, u32)>,
    pub first_go: bool,
    random: Random,
    /// the values of the new grid, counted for showing a cell not yet decided
    values: usize,
}

impl TileNode {
    /// The node an element describes, and the grid it will fill.
    pub fn load(element: &Element, grid: &Grid, from: usize, resources: &Resources, file: &str, limits: &Limits) -> Result<(TileNode, Grid), Fault> {
        let line = element.line;
        let name = element.get("tileset").ok_or(Fault::Missing { file: file.to_string(), line, node: "wfc".into(), attribute: "tileset" })?;
        let tilesname = element.get("tiles").unwrap_or(name);
        let (overlap, overlapz): (i64, i64) = (number(element, "overlap", 0, file)?, number(element, "overlapz", 0, file)?);
        let (set_file, bytes) = resources.read(&["tilesets", &format!("{name}.xml")], limits.text)?;
        let root = super::xml::read(&set_file, &bytes, limits)?;
        let full = flag(&root, "fullSymmetry", &set_file)?;
        let lacks = |node: &str, attribute, line| Fault::Missing { file: set_file.clone(), line, node: node.to_string(), attribute };
        let xtiles: Vec<&Element> = root.elements(&["tiles"]).next().ok_or_else(|| lacks("tileset", "tiles", root.line))?.elements(&["tile"]).collect();
        if xtiles.is_empty() {
            return Err(lacks("tiles", "tile", root.line));
        }

        // the tiles, each with its turned and mirrored copies; colours are numbered in the order they are first met
        let mut tiledata: Vec<Vec<u8>> = Vec::new();
        let mut weights: Vec<f64> = Vec::new();
        let mut named: Vec<(String, std::ops::Range<usize>)> = Vec::new();
        let mut seen: Vec<i32> = Vec::new();
        let (mut s, mut sz) = (0, 0);
        let mut made: Option<Grid> = None;
        for xtile in &xtiles {
            let tilename = xtile.get("name").ok_or_else(|| lacks("tile", "name", xtile.line))?;
            let weight: f64 = number(xtile, "weight", 1.0, &set_file)?;
            let (shown, bytes) = resources.read(&["tilesets", tilesname, &format!("{tilename}.vox")], limits.vox)?;
            let vox = super::vox::read(&shown, &bytes, limits)?;
            if made.is_none() {
                [s, sz] = [vox.size[0], vox.size[2]];
                within(Thing::Tile, s.max(sz) as u64, limits.tile, &shown, 0)?;
                if vox.size[0] != vox.size[1] {
                    return Err(Fault::TileShape { file: shown, size: vox.size, cube: false });
                }
                if full && s != sz {
                    return Err(Fault::TileShape { file: shown, size: vox.size, cube: true });
                }
                let side = |tile: usize, lap: i64, cells: usize| ((tile as i64 - lap).max(0) as u64).saturating_mul(cells as u64).saturating_add_signed(lap).min(u32::MAX as u64) as usize;
                made = Some(Grid::load(element, [side(s, overlap, grid.size[0]), side(s, overlap, grid.size[1]), side(sz, overlapz, grid.size[2])], file, limits)?);
            }
            if vox.size != [s, s, sz] {
                return Err(Fault::TileShape { file: shown, size: vox.size, cube: full });
            }
            let values = made.as_ref().map_or(0, |g| g.characters.len());
            let mut flat = Vec::with_capacity(vox.cells.len());
            for colour in &vox.cells {
                let order = seen.iter().position(|known| known == colour).unwrap_or_else(|| {
                    seen.push(*colour);
                    seen.len() - 1
                });
                flat.push(order as u8);
            }
            if seen.len() > values {
                return Err(Fault::Colours { file: shown, line, figure: seen.len(), limit: values });
            }
            let same = |a: &Vec<u8>, b: &Vec<u8>| a == b;
            let local = if full { cube_symmetries(flat, |p| z_rotate(p, s, sz), |p| y_rotate(p, s), |p| x_reflect(p, s, sz), same, None) } else { square_symmetries(flat, |p| z_rotate(p, s, sz), |p| x_reflect(p, s, sz), same, None) };
            named.push((tilename.to_string(), tiledata.len()..tiledata.len() + local.len()));
            weights.extend(std::iter::repeat_n(weight, local.len()));
            tiledata.extend(local);
            within(Thing::Tiles, tiledata.len() as u64, limits.tiles, &set_file, xtile.line)?;
            if !(weight > 0.0) || !weight.is_finite() {
                return Err(Fault::BadValue { file: set_file.clone(), line: xtile.line, attribute: "weight", value: xtile.get("weight").unwrap_or("").to_string() });
            }
        }
        let Some(new) = made else { return Err(lacks("tiles", "tile", root.line)) };
        let p = tiledata.len();
        within(Thing::Wave, (grid.state.len() as u64).saturating_mul(p as u64), limits.wave, file, line)?;

        // which tiles each value of the old grid may become
        let mut map: Vec<Option<Vec<bool>>> = vec![None; grid.characters.len()];
        for xrule in element.elements(&["rule"]) {
            let missing = |attribute| Fault::Missing { file: file.to_string(), line: xrule.line, node: "rule".into(), attribute };
            let input = grid.value_at(xrule.get("in").and_then(|s| s.chars().next()).ok_or_else(|| missing("in"))?, file, xrule.line)?;
            let mut allowed = vec![false; p];
            for wanted in xrule.get("out").ok_or_else(|| missing("out"))?.split('|') {
                let (_, range) = named.iter().find(|(known, _)| known == wanted).ok_or(Fault::UnknownTile { file: file.to_string(), line: xrule.line, tile: wanted.to_string() })?;
                allowed[range.clone()].iter_mut().for_each(|on| *on = true);
            }
            map[input as usize] = Some(allowed);
        }

        // which tiles may be neighbours, from the pairs the tileset lists and their turned and mirrored copies
        let mut dense = vec![false; 6 * p * p];
        let neighbors: Vec<&Element> = root.elements(&["neighbors"]).flat_map(|n| n.elements(&["neighbor"])).collect();
        for xneighbor in neighbors {
            let unknown = |attribute: &str| Fault::UnknownTile { file: set_file.clone(), line: xneighbor.line, tile: attribute.to_string() };
            // a tile as an attribute names it: a name, or turns (x, y, z, applied last to first) and a name
            let tile = |attribute: &str| -> Result<Vec<u8>, Fault> {
                let (action, last) = attribute.rsplit_once(' ').unwrap_or(("", attribute));
                let (_, range) = named.iter().find(|(known, _)| known == last).ok_or_else(|| unknown(attribute))?;
                let mut turned = tiledata[range.start].clone();
                for sym in action.chars().rev() {
                    turned = match sym {
                        'x' if s == sz => x_rotate(&turned, s),
                        'y' if s == sz => y_rotate(&turned, s),
                        'z' => z_rotate(&turned, s, sz),
                        _ => return Err(Fault::BadValue { file: set_file.clone(), line: xneighbor.line, attribute: "left", value: attribute.to_string() }),
                    };
                }
                Ok(turned)
            };
            let mut allow = |d: usize, a: &[u8], b: &[u8], attribute: &str| -> Result<(), Fault> {
                let index = |tile: &[u8]| tiledata.iter().position(|known| known == tile).ok_or_else(|| unknown(attribute));
                dense[(d * p + index(a)?) * p + index(b)?] = true;
                Ok(())
            };
            let xr = |t: &Vec<u8>| x_reflect(t, s, sz);
            let yr = |t: &Vec<u8>| y_reflect(t, s, sz);
            let zr = |t: &Vec<u8>| z_reflect(t, s, sz);
            let zrot = |t: &Vec<u8>| z_rotate(t, s, sz);
            let never = |_: &Vec<u8>, _: &Vec<u8>| false;
            match (xneighbor.get("left"), xneighbor.get("right"), xneighbor.get("top"), xneighbor.get("bottom")) {
                (Some(left), Some(right), _, _) if full => {
                    let (ltile, rtile) = (tile(left)?, tile(right)?);
                    let (lsym, rsym) = (square_symmetries(ltile.clone(), |t| x_rotate(t, s), yr, never, None), square_symmetries(rtile.clone(), |t| x_rotate(t, s), yr, never, None));
                    for (l, r) in lsym.iter().zip(&rsym) {
                        allow(0, l, r, left)?;
                        allow(0, &xr(r), &xr(l), left)?;
                    }
                    let (dsym, usym) = (square_symmetries(zrot(&ltile), |t| y_rotate(t, s), zr, never, None), square_symmetries(zrot(&rtile), |t| y_rotate(t, s), zr, never, None));
                    for (d, u) in dsym.iter().zip(&usym) {
                        allow(1, d, u, left)?;
                        allow(1, &yr(u), &yr(d), left)?;
                    }
                    let (bsym, tsym) = (square_symmetries(y_rotate(&ltile, s), zrot, xr, never, None), square_symmetries(y_rotate(&rtile, s), zrot, xr, never, None));
                    for (b, t) in bsym.iter().zip(&tsym) {
                        allow(4, b, t, left)?;
                        allow(4, &zr(t), &zr(b), left)?;
                    }
                }
                (Some(left), Some(right), _, _) => {
                    let (ltile, rtile) = (tile(left)?, tile(right)?);
                    allow(0, &ltile, &rtile, left)?;
                    allow(0, &yr(&ltile), &yr(&rtile), left)?;
                    allow(0, &xr(&rtile), &xr(&ltile), left)?;
                    allow(0, &yr(&xr(&rtile)), &yr(&xr(&ltile)), left)?;
                    let (dtile, utile) = (zrot(&ltile), zrot(&rtile));
                    allow(1, &dtile, &utile, left)?;
                    allow(1, &xr(&dtile), &xr(&utile), left)?;
                    allow(1, &yr(&utile), &yr(&dtile), left)?;
                    allow(1, &xr(&yr(&utile)), &xr(&yr(&dtile)), left)?;
                }
                (_, _, Some(top), Some(bottom)) => {
                    let (tsym, bsym) = (square_symmetries(tile(top)?, zrot, xr, never, None), square_symmetries(tile(bottom)?, zrot, xr, never, None));
                    for (b, t) in bsym.iter().zip(&tsym) {
                        allow(4, b, t, top)?;
                    }
                }
                _ => return Err(Fault::Missing { file: set_file.clone(), line: xneighbor.line, node: "neighbor".into(), attribute: "left" }),
            }
        }
        for a in 0..p {
            for b in 0..p {
                for (back, forth) in [(2, 0), (3, 1), (5, 4)] {
                    dense[(back * p + b) * p + a] = dense[(forth * p + a) * p + b];
                }
            }
        }
        let propagator: [Vec<Vec<u32>>; 6] = std::array::from_fn(|d| (0..p).map(|a| (0..p).filter(|b| dense[(d * p + a) * p + b]).map(|b| b as u32).collect()).collect());

        let shannon = flag(element, "shannon", file)?;
        let weight_log_weights: Vec<f64> = weights.iter().map(|w| w * ln(*w)).collect();
        let (sum_of_weights, sum_of_weight_log_weights): (f64, f64) = (weights.iter().sum(), weight_log_weights.iter().sum());
        let cells = grid.state.len();
        let wave = Wave { data: vec![true; cells * p], compatible: vec![0; cells * p * 6], sums_of_ones: vec![0; cells], sums_of_weights: vec![0.0; cells], sums_of_weight_log_weights: vec![0.0; cells], entropies: vec![0.0; cells] };
        let node = TileNode { from, line, tiledata, s, sz, overlap, overlapz, periodic: flag(element, "periodic", file)?, shannon, tries: number(element, "tries", 1000usize, file)?.max(1), weights, weight_log_weights, sum_of_weights, sum_of_weight_log_weights, starting_entropy: ln(sum_of_weights) - sum_of_weight_log_weights / sum_of_weights, propagator, map, startwave: wave.clone(), wave, stack: Vec::new(), first_go: true, random: Random::new(0), values: new.characters.len() };
        Ok((node, new))
    }

    fn p(&self) -> usize {
        self.tiledata.len()
    }

    fn ban(&mut self, i: usize, t: usize) {
        let p = self.p();
        self.wave.data[i * p + t] = false;
        let at = (i * p + t) * 6;
        self.wave.compatible[at..at + 6].iter_mut().for_each(|c| *c = 0);
        self.stack.push((i as u32, t as u32));
        self.wave.sums_of_ones[i] -= 1;
        if self.shannon {
            let sum = self.wave.sums_of_weights[i];
            self.wave.entropies[i] += self.wave.sums_of_weight_log_weights[i] / sum - ln(sum);
            self.wave.sums_of_weights[i] -= self.weights[t];
            self.wave.sums_of_weight_log_weights[i] -= self.weight_log_weights[t];
            let sum = self.wave.sums_of_weights[i];
            self.wave.entropies[i] -= self.wave.sums_of_weight_log_weights[i] / sum - ln(sum);
        }
    }

    /// Strike out everything the tiles struck out so far rule out. Whether
    /// a tile is left for the first cell, which is how the original tells
    /// a contradiction: a cell left with none empties its neighbours in
    /// turn until every cell is empty.
    fn propagate(&mut self, size: [usize; 3]) -> bool {
        let p = self.p();
        let [mx, my, mz] = size.map(|v| v as i64);
        while let Some((i1, t1)) = self.stack.pop() {
            let i1 = i1 as i64;
            let (x1, y1, z1) = (i1 % mx, (i1 % (mx * my)) / mx, i1 / (mx * my));
            for d in 0..6 {
                let (mut x2, mut y2, mut z2) = (x1 + DX[d], y1 + DY[d], z1 + DZ[d]);
                if !self.periodic && (x2 < 0 || y2 < 0 || z2 < 0 || x2 >= mx || y2 >= my || z2 >= mz) {
                    continue;
                }
                x2 = x2.rem_euclid(mx);
                y2 = y2.rem_euclid(my);
                z2 = z2.rem_euclid(mz);
                let i2 = (x2 + y2 * mx + z2 * mx * my) as usize;
                for l in 0..self.propagator[d][t1 as usize].len() {
                    let t2 = self.propagator[d][t1 as usize][l] as usize;
                    let at = (i2 * p + t2) * 6 + d;
                    self.wave.compatible[at] -= 1;
                    if self.wave.compatible[at] == 0 {
                        self.ban(i2, t2);
                    }
                }
            }
        }
        self.wave.sums_of_ones[0] > 0
    }

    /// The cell with the least choice left among those with any, or none when every cell is decided.
    fn next_unobserved(&mut self) -> Option<usize> {
        let (mut min, mut argmin) = (1e4, None);
        for i in 0..self.wave.sums_of_ones.len() {
            let remaining = self.wave.sums_of_ones[i];
            let entropy = if self.shannon { self.wave.entropies[i] } else { remaining as f64 };
            if remaining > 1 && entropy <= min {
                let noise = 1e-6 * self.random.unit();
                if entropy + noise < min {
                    min = entropy + noise;
                    argmin = Some(i);
                }
            }
        }
        argmin
    }

    fn observe(&mut self, node: usize) {
        let p = self.p();
        let allowed = |t: usize| self.wave.data[node * p + t];
        let sum: f64 = (0..p).filter(|t| allowed(*t)).map(|t| self.weights[t]).sum();
        let threshold = self.random.unit() * sum;
        let mut partial = 0.0;
        let mut chosen = 0;
        for t in (0..p).filter(|t| allowed(*t)) {
            partial += self.weights[t];
            if partial >= threshold {
                chosen = t;
                break;
            }
        }
        for t in 0..p {
            if self.wave.data[node * p + t] != (t == chosen) {
                self.ban(node, t);
            }
        }
    }

    /// One turn: the first finds a seed that fills the grid, and each after it decides one cell.
    pub fn go(&mut self, n: &mut i64, old: &Grid, new: &mut Grid, ip: &mut Ip) -> Result<bool, Fault> {
        let p = self.p();
        if self.first_go {
            // every tile allowed everywhere, each with as many supporters as the tileset gives it
            for i in 0..old.state.len() {
                for t in 0..p {
                    self.wave.data[i * p + t] = true;
                    for d in 0..6 {
                        self.wave.compatible[(i * p + t) * 6 + d] = self.propagator[OPPOSITE[d]][t].len() as i32;
                    }
                }
                self.wave.sums_of_ones[i] = p as i32;
                self.wave.sums_of_weights[i] = self.sum_of_weights;
                self.wave.sums_of_weight_log_weights[i] = self.sum_of_weight_log_weights;
                self.wave.entropies[i] = self.starting_entropy;
            }
            self.stack.clear();
            for i in 0..old.state.len() {
                for t in 0..p {
                    let allowed = self.map.get(old.state[i] as usize).and_then(|start| start.as_ref()).is_none_or(|start| start[t]);
                    if !allowed && self.wave.data[i * p + t] {
                        self.ban(i, t);
                    }
                }
            }
            let (file, line) = (ip.file.clone(), self.line);
            let contradiction = |tries, at_start| Fault::Contradiction { file: file.clone(), line, tries, at_start };
            if !self.propagate(old.size) {
                return Err(contradiction(0, true));
            }
            self.startwave = self.wave.clone();
            let mut observations = 0u64;
            let mut good = None;
            'tries: for k in 0..self.tries {
                let seed = ip.random.seed();
                self.random = Random::new(seed);
                self.stack.clear();
                self.wave.clone_from(&self.startwave);
                loop {
                    let Some(node) = self.next_unobserved() else {
                        good = Some(seed);
                        break 'tries;
                    };
                    self.observe(node);
                    observations += 1;
                    if !self.propagate(old.size) {
                        break;
                    }
                    if observations > MOST_OBSERVATIONS {
                        return Err(contradiction(k + 1, false));
                    }
                }
            }
            let Some(seed) = good else { return Err(contradiction(self.tries, false)) };
            self.random = Random::new(seed);
            self.stack.clear();
            self.wave.clone_from(&self.startwave);
            self.first_go = false;
            new.state.iter_mut().for_each(|v| *v = 0);
            return Ok(true);
        }
        match self.next_unobserved() {
            Some(node) => {
                self.observe(node);
                self.propagate(old.size);
            }
            None => {
                *n += 1;
                self.show(old, new);
            }
        }
        Ok(true)
    }

    /// Write the new grid from the tiles as they stand: in each cell of each tile's place, the value most of the tiles still allowed have there.
    pub fn show(&self, old: &Grid, new: &mut Grid) {
        let (p, s, sz) = (self.p(), self.s, self.sz);
        let mut votes = vec![0u32; s * s * sz * self.values];
        for z in 0..old.size[2] {
            for y in 0..old.size[1] {
                for x in 0..old.size[0] {
                    let i = old.at(x, y, z);
                    votes.iter_mut().for_each(|v| *v = 0);
                    for t in (0..p).filter(|t| self.wave.data[i * p + t]) {
                        for (di, value) in self.tiledata[t].iter().enumerate() {
                            votes[di * self.values + *value as usize] += 1;
                        }
                    }
                    for dz in 0..sz {
                        for dy in 0..s {
                            for dx in 0..s {
                                let at = (dx + dy * s + dz * s * s) * self.values;
                                // (the first of equals: the original breaks a tie by chance, with numbers of no seed, so its pictures of a wave half decided differ from run to run)
                                let most = (0..self.values).rev().max_by_key(|c| votes[at + c]).unwrap_or(0);
                                let place = [x as i64 * (s as i64 - self.overlap) + dx as i64, y as i64 * (s as i64 - self.overlap) + dy as i64, z as i64 * (sz as i64 - self.overlapz) + dz as i64];
                                if (0..3).all(|k| place[k] >= 0 && (place[k] as usize) < new.size[k]) {
                                    let cell = new.at(place[0] as usize, place[1] as usize, place[2] as usize);
                                    new.state[cell] = most as u8;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn moved(p: &[u8], s: usize, sz: usize, from: impl Fn(usize, usize, usize) -> usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(p.len());
    for z in 0..sz {
        for y in 0..s {
            for x in 0..s {
                out.push(p[from(x, y, z)]);
            }
        }
    }
    out
}

fn z_rotate(p: &[u8], s: usize, sz: usize) -> Vec<u8> {
    moved(p, s, sz, |x, y, z| y + (s - 1 - x) * s + z * s * s)
}

fn y_rotate(p: &[u8], s: usize) -> Vec<u8> {
    moved(p, s, s, |x, y, z| z + y * s + (s - 1 - x) * s * s)
}

fn x_rotate(p: &[u8], s: usize) -> Vec<u8> {
    moved(p, s, s, |x, y, z| x + z * s + (s - 1 - y) * s * s)
}

fn x_reflect(p: &[u8], s: usize, sz: usize) -> Vec<u8> {
    moved(p, s, sz, |x, y, z| (s - 1 - x) + y * s + z * s * s)
}

fn y_reflect(p: &[u8], s: usize, sz: usize) -> Vec<u8> {
    moved(p, s, sz, |x, y, z| x + (s - 1 - y) * s + z * s * s)
}

fn z_reflect(p: &[u8], s: usize, sz: usize) -> Vec<u8> {
    moved(p, s, sz, |x, y, z| x + y * s + (sz - 1 - z) * s * s)
}
