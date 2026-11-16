//! The interpreter: a program's nodes, and the machine that runs them a
//! turn at a time.
//!
//! A program is a tree. Its leaves change the grid: rule nodes (`one`
//! applies one match of its rules a turn, `all` every match that does not
//! overlap another, `prl` every match at once), `path` draws a shortest
//! path, `convolution` is a cellular automaton. Its branches say what runs
//! when: a `sequence` runs each child until it has nothing left to do and
//! goes on to the next, a `markov` goes back to its first child after
//! every turn, and `map` and `wfc` make a new, usually larger, grid and
//! run their children on that.
//!
//! This follows the original's `Interpreter.cs`, `Node.cs`, `RuleNode.cs`,
//! `OneNode.cs`, `AllNode.cs`, `ParallelNode.cs`, `Field.cs`, `Path.cs`,
//! `Map.cs` and `Convolution.cs` turn for turn, quirks included (a branch
//! that has just finished is asked once more; a `map` or `wfc` node ends
//! the program when its children are done), because a program is written
//! against that behaviour and not against a description of it. Two things
//! differ on purpose. The random numbers are this project's own, so a
//! seed here does not give the picture the same seed gives there. And
//! nothing is recursive in the data: nodes live in one list and name each
//! other by number, so the machine is a plain value that can be stepped,
//! stopped and dropped at any turn.
//! https://github.com/mxgmn/MarkovJunior (MIT, Maxim Gumin, 2022)

use super::fault::{within, Fault, Limits, Thing};
use super::rule::{flag, number, symmetry, Grid, Rule, KEEP};
use super::wfc::TileNode;
use super::xml::Element;
use super::Resources;
use std::collections::VecDeque;

/// The names an element may have to be a node. Any other element inside a
/// branch is passed over, as the original passes over it: its programs
/// set a node aside by renaming it (`altone`, `altrule`).
const NODES: [&str; 10] = ["one", "all", "prl", "markov", "sequence", "path", "map", "convolution", "convchain", "wfc"];

/// Numbers that look random and are the same on every machine for one
/// seed (SplitMix64, as `pattern::Numbers`: https://prng.di.unimi.it/splitmix64.c).
/// The original uses .NET's `System.Random`, whose numbers this does not
/// reproduce: its algorithm is a detail of one runtime's library.
#[derive(Clone, Debug)]
pub struct Random(u64);

impl Random {
    pub fn new(seed: u64) -> Random {
        Random(seed ^ 0x6A09_E667_F3BC_C909)
    }

    fn bits(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// From 0 up to, not including, 1.
    pub fn unit(&mut self) -> f64 {
        (self.bits() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// A whole number from 0 up to, not including, `n`.
    pub fn below(&mut self, n: usize) -> usize {
        ((self.unit() * n as f64) as usize).min(n.saturating_sub(1))
    }

    /// A number to start another run of numbers from.
    pub fn seed(&mut self) -> u64 {
        self.bits()
    }
}

/// The natural logarithm, by arithmetic alone. `f64::ln` is the
/// platform's own and may differ in its last digit from one machine to
/// the next, which is enough to change which of two nearly equal cells a
/// wfc node takes first, and so the whole result. Sums, products and
/// quotients are the same everywhere (IEEE 754), so this is too.
/// The series is that of the inverse hyperbolic tangent:
/// https://en.wikipedia.org/wiki/Logarithm#Power_series
pub fn ln(x: f64) -> f64 {
    if !(x > 0.0) || !x.is_finite() {
        return if x == 0.0 { f64::NEG_INFINITY } else { f64::NAN };
    }
    // x = m * 2^e with m from 1/sqrt 2 to sqrt 2
    let (mut m, mut e) = (x, 0.0);
    while m > std::f64::consts::SQRT_2 {
        m *= 0.5;
        e += 1.0;
    }
    while m < std::f64::consts::FRAC_1_SQRT_2 {
        m *= 2.0;
        e -= 1.0;
    }
    let s = (m - 1.0) / (m + 1.0);
    let (s2, mut term, mut sum) = (s * s, s, 0.0);
    for k in 0..12 {
        sum += term / (2 * k + 1) as f64;
        term *= s2;
    }
    2.0 * sum + e * std::f64::consts::LN_2
}

/// e to a power, by arithmetic alone, for the reason [`ln`] is.
pub fn exp(x: f64) -> f64 {
    if x.is_nan() {
        return x;
    }
    let x = x.clamp(-745.0, 709.0);
    let k = (x / std::f64::consts::LN_2).round();
    let r = x - k * std::f64::consts::LN_2;
    let (mut term, mut sum) = (1.0, 1.0);
    for n in 1..18 {
        term *= r / n as f64;
        sum += term;
    }
    let mut k = k as i32;
    while k > 0 {
        sum *= 2.0;
        k -= 1;
    }
    while k < 0 {
        sum *= 0.5;
        k += 1;
    }
    sum
}

/// What every node may read and write besides its own grid.
pub struct Ip {
    /// every cell changed, in order, and where each turn's changes begin
    pub changes: Vec<[u32; 3]>,
    pub first: Vec<usize>,
    /// turns taken
    pub counter: usize,
    pub random: Random,
    /// the branch that runs next turn, or none when the program is done
    current: Option<usize>,
    /// the grid being worked on, which is the one to show
    pub grid: usize,
    pub limits: Limits,
    pub file: String,
}

#[derive(Clone, Copy, PartialEq)]
enum RuleKind {
    One,
    All,
    Prl,
}

struct Field {
    recompute: bool,
    essential: bool,
    inversed: bool,
    zero: u32,
    substrate: u32,
}

impl Field {
    /// How far each cell is from the nearest `zero` cell, going through `substrate` cells; -1 where it cannot be reached.
    fn compute(&self, potential: &mut [i32], grid: &Grid) -> bool {
        let [mx, my, mz] = grid.size;
        let mut front: VecDeque<(i32, usize, usize, usize)> = VecDeque::new();
        for (i, value) in grid.state.iter().enumerate() {
            potential[i] = -1;
            if self.zero >> value & 1 == 1 {
                potential[i] = 0;
                front.push_back((0, i % mx, i / mx % my, i / (mx * my)));
            }
        }
        if front.is_empty() {
            return false;
        }
        while let Some((t, x, y, z)) = front.pop_front() {
            let mut visit = |nx: usize, ny: usize, nz: usize| {
                let i = nx + ny * mx + nz * mx * my;
                if potential[i] == -1 && self.substrate >> grid.state[i] & 1 == 1 {
                    front.push_back((t + 1, nx, ny, nz));
                    potential[i] = t + 1;
                }
            };
            if x > 0 { visit(x - 1, y, z) }
            if x < mx - 1 { visit(x + 1, y, z) }
            if y > 0 { visit(x, y - 1, z) }
            if y < my - 1 { visit(x, y + 1, z) }
            if z > 0 { visit(x, y, z - 1) }
            if z < mz - 1 { visit(x, y, z + 1) }
        }
        true
    }
}

/// How much applying a rule at a place would change the sum of the potentials; none if it would write a value where that value cannot be.
fn delta_pointwise(grid: &Grid, rule: &Rule, x: usize, y: usize, z: usize, fields: &[Option<Field>], potentials: &[Vec<i32>]) -> Option<i32> {
    let [mx, my, _] = grid.size;
    let mut sum = 0;
    let mut di = 0;
    for dz in 0..rule.input_size[2] {
        for dy in 0..rule.input_size[1] {
            for dx in 0..rule.input_size[0] {
                let new = rule.output[di];
                if new != KEEP && rule.input[di] >> new & 1 == 0 {
                    let i = x + dx + (y + dy) * mx + (z + dz) * mx * my;
                    let new_potential = potentials[new as usize][i];
                    if new_potential == -1 {
                        return None;
                    }
                    let old = grid.state[i];
                    let old_potential = potentials[old as usize][i];
                    sum += new_potential - old_potential;
                    if fields[old as usize].as_ref().is_some_and(|f| f.inversed) {
                        sum += 2 * old_potential;
                    }
                    if fields[new as usize].as_ref().is_some_and(|f| f.inversed) {
                        sum -= 2 * new_potential;
                    }
                }
                di += 1;
            }
        }
    }
    Some(sum)
}

struct RuleNode {
    kind: RuleKind,
    grid: usize,
    line: usize,
    rules: Vec<Rule>,
    counter: usize,
    steps: usize,
    matches: Vec<(u32, u32, u32, u32)>,
    match_count: usize,
    last_matched_turn: i64,
    /// for each rule and cell, whether a match there is already listed; made when first needed
    mask: Vec<u64>,
    potentials: Vec<Vec<i32>>,
    fields: Vec<Option<Field>>,
    temperature: f64,
    /// what a `prl` node will write, held until all its matches are found
    newstate: Vec<u8>,
}

impl RuleNode {
    fn reset(&mut self) {
        self.last_matched_turn = -1;
        self.counter = 0;
        if self.kind == RuleKind::One && self.match_count != 0 {
            self.mask.iter_mut().for_each(|word| *word = 0);
            self.match_count = 0;
        }
    }

    fn masked(&self, r: usize, i: usize, cells: usize) -> bool {
        let bit = r * cells + i;
        self.mask.get(bit / 64).is_some_and(|word| word >> (bit % 64) & 1 == 1)
    }

    fn set_mask(&mut self, r: usize, i: usize, cells: usize, on: bool) {
        let bit = r * cells + i;
        if let Some(word) = self.mask.get_mut(bit / 64) {
            if on { *word |= 1 << (bit % 64) } else { *word &= !(1 << (bit % 64)) }
        }
    }

    /// Note a match found. A `prl` node writes it at once, to its own copy of the grid.
    fn add(&mut self, rules: &[Rule], r: usize, x: usize, y: usize, z: usize, grid: &Grid, ip: &mut Ip) {
        if self.kind == RuleKind::Prl {
            let rule = &rules[r];
            if ip.random.unit() > rule.p {
                return;
            }
            let [ox, oy, oz] = rule.output_size;
            for dz in 0..oz {
                for dy in 0..oy {
                    for dx in 0..ox {
                        let new = rule.output[dx + dy * ox + dz * ox * oy];
                        let i = grid.at(x + dx, y + dy, z + dz);
                        if new != KEEP && new != grid.state[i] {
                            self.newstate[i] = new;
                            ip.changes.push([(x + dx) as u32, (y + dy) as u32, (z + dz) as u32]);
                        }
                    }
                }
            }
            self.match_count += 1;
            return;
        }
        self.set_mask(r, grid.at(x, y, z), grid.state.len(), true);
        let found = (r as u32, x as u32, y as u32, z as u32);
        if self.match_count < self.matches.len() {
            self.matches[self.match_count] = found;
        } else {
            self.matches.push(found);
        }
        self.match_count += 1;
    }

    /// What every rule node does first each turn: find the matches, all of them the first time and after that only where the grid has changed.
    fn find(&mut self, grid: &Grid, ip: &mut Ip) -> Result<bool, Fault> {
        if self.steps > 0 && self.counter >= self.steps {
            return Ok(false);
        }
        let [mx, my, mz] = grid.size;
        let cells = grid.state.len();
        if self.kind != RuleKind::Prl && self.mask.is_empty() {
            let bits = (self.rules.len() as u64).saturating_mul(cells as u64);
            within(Thing::Matches, bits, ip.limits.matches, &ip.file, self.line)?;
            self.mask = vec![0; (bits as usize).div_ceil(64)];
        }
        let rules = std::mem::take(&mut self.rules);
        let fits = |rule: &Rule, x: usize, y: usize, z: usize, shift: &[usize; 3]| -> Option<[usize; 3]> {
            let (sx, sy, sz) = (x.checked_sub(shift[0])?, y.checked_sub(shift[1])?, z.checked_sub(shift[2])?);
            (sx + rule.input_size[0] <= mx && sy + rule.input_size[1] <= my && sz + rule.input_size[2] <= mz).then_some([sx, sy, sz])
        };
        if self.last_matched_turn >= 0 {
            for n in ip.first[self.last_matched_turn as usize]..ip.changes.len() {
                let [x, y, z] = ip.changes[n].map(|v| v as usize);
                // (a change noted on another grid, before this one was made, is no place of this one)
                if x >= mx || y >= my || z >= mz {
                    continue;
                }
                let value = grid.state[grid.at(x, y, z)] as usize;
                for (r, rule) in rules.iter().enumerate() {
                    for shift in &rule.shifts[value] {
                        if let Some([sx, sy, sz]) = fits(rule, x, y, z, shift) {
                            if !self.masked(r, grid.at(sx, sy, sz), cells) && grid.matches(rule, sx, sy, sz) {
                                self.add(&rules, r, sx, sy, sz, grid, ip);
                            }
                        }
                    }
                }
            }
        } else {
            self.match_count = 0;
            for (r, rule) in rules.iter().enumerate() {
                let [ix, iy, iz] = rule.input_size;
                // one cell in every box the size of the input: any match covers exactly one
                for z in (iz - 1..mz).step_by(iz) {
                    for y in (iy - 1..my).step_by(iy) {
                        for x in (ix - 1..mx).step_by(ix) {
                            for shift in &rule.shifts[grid.state[grid.at(x, y, z)] as usize] {
                                if let Some([sx, sy, sz]) = fits(rule, x, y, z, shift) {
                                    if grid.matches(rule, sx, sy, sz) {
                                        self.add(&rules, r, sx, sy, sz, grid, ip);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        self.rules = rules;
        if !self.fields.is_empty() {
            let (mut any_success, mut any_computation) = (false, false);
            for (c, field) in self.fields.iter().enumerate() {
                let Some(field) = field else { continue };
                if self.counter == 0 || field.recompute {
                    let success = field.compute(&mut self.potentials[c], grid);
                    if !success && field.essential {
                        return Ok(false);
                    }
                    any_success |= success;
                    any_computation = true;
                }
            }
            if any_computation && !any_success {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// The order of preference a field gives a match: the higher, the sooner.
    fn key(&self, h: i32, first: i32, u: f64) -> f64 {
        if self.temperature > 0.0 {
            // u to the power e^((h - first) / temperature)
            exp(exp((h - first) as f64 / self.temperature) * ln(u))
        } else {
            -(h as f64) + 0.001 * u
        }
    }

    fn go_one(&mut self, grid: &mut Grid, ip: &mut Ip) -> Result<bool, Fault> {
        if !self.find(grid, ip)? {
            return Ok(false);
        }
        self.last_matched_turn = ip.counter as i64;
        let cells = grid.state.len();
        let mut chosen: Option<(u32, u32, u32, u32)> = None;
        if !self.potentials.is_empty() {
            let (mut max, mut first) = (-1000.0, None);
            let mut k = 0;
            while k < self.match_count {
                let (r, x, y, z) = self.matches[k];
                let (x, y, z) = (x as usize, y as usize, z as usize);
                if !grid.matches(&self.rules[r as usize], x, y, z) {
                    self.set_mask(r as usize, grid.at(x, y, z), cells, false);
                    self.matches[k] = self.matches[self.match_count - 1];
                    self.match_count -= 1;
                    continue;
                }
                if let Some(h) = delta_pointwise(grid, &self.rules[r as usize], x, y, z, &self.fields, &self.potentials) {
                    let first = *first.get_or_insert(h);
                    let key = self.key(h, first, ip.random.unit());
                    if key > max {
                        max = key;
                        chosen = Some(self.matches[k]);
                    }
                }
                k += 1;
            }
        } else {
            while self.match_count > 0 {
                let index = ip.random.below(self.match_count);
                let (r, x, y, z) = self.matches[index];
                self.set_mask(r as usize, grid.at(x as usize, y as usize, z as usize), cells, false);
                self.matches[index] = self.matches[self.match_count - 1];
                self.match_count -= 1;
                if grid.matches(&self.rules[r as usize], x as usize, y as usize, z as usize) {
                    chosen = Some((r, x, y, z));
                    break;
                }
            }
        }
        let Some((r, x, y, z)) = chosen else { return Ok(false) };
        let rule = &self.rules[r as usize];
        let [ox, oy, oz] = rule.output_size;
        for dz in 0..oz {
            for dy in 0..oy {
                for dx in 0..ox {
                    let new = rule.output[dx + dy * ox + dz * ox * oy];
                    let (sx, sy, sz) = (x as usize + dx, y as usize + dy, z as usize + dz);
                    let i = grid.at(sx, sy, sz);
                    if new != KEEP && new != grid.state[i] {
                        grid.state[i] = new;
                        ip.changes.push([sx as u32, sy as u32, sz as u32]);
                    }
                }
            }
        }
        self.counter += 1;
        Ok(true)
    }

    fn go_all(&mut self, grid: &mut Grid, ip: &mut Ip) -> Result<bool, Fault> {
        if !self.find(grid, ip)? {
            return Ok(false);
        }
        self.last_matched_turn = ip.counter as i64;
        if self.match_count == 0 {
            return Ok(false);
        }
        let cells = grid.state.len();
        // the order the matches are tried in: by what a field prefers, or shuffled
        let order: Vec<usize> = if !self.potentials.is_empty() {
            let mut first = None;
            let mut keyed: Vec<(usize, f64)> = Vec::new();
            for m in 0..self.match_count {
                let (r, x, y, z) = self.matches[m];
                if let Some(h) = delta_pointwise(grid, &self.rules[r as usize], x as usize, y as usize, z as usize, &self.fields, &self.potentials) {
                    let first = *first.get_or_insert(h);
                    keyed.push((m, self.key(h, first, ip.random.unit())));
                }
            }
            // (a stable sort, as the original's is, so equal keys keep their order)
            keyed.sort_by(|a, b| b.1.total_cmp(&a.1));
            keyed.into_iter().map(|(m, _)| m).collect()
        } else {
            let mut shuffle = vec![0; self.match_count];
            for i in 0..shuffle.len() {
                let j = ip.random.below(i + 1);
                shuffle[i] = shuffle[j];
                shuffle[j] = i;
            }
            shuffle
        };
        for m in order {
            let (r, x, y, z) = self.matches[m];
            let (x, y, z) = (x as usize, y as usize, z as usize);
            self.set_mask(r as usize, grid.at(x, y, z), cells, false);
            let rule = &self.rules[r as usize];
            let [ox, oy, oz] = rule.output_size;
            let places = (0..oz).flat_map(|dz| (0..oy).flat_map(move |dy| (0..ox).map(move |dx| (dx, dy, dz))));
            // only where nothing it would write has been written this turn
            if places.clone().any(|(dx, dy, dz)| rule.output[dx + dy * ox + dz * ox * oy] != KEEP && grid.mask[grid.at(x + dx, y + dy, z + dz)]) {
                continue;
            }
            for (dx, dy, dz) in places {
                let new = rule.output[dx + dy * ox + dz * ox * oy];
                if new != KEEP {
                    let i = grid.at(x + dx, y + dy, z + dz);
                    grid.mask[i] = true;
                    grid.state[i] = new;
                    ip.changes.push([(x + dx) as u32, (y + dy) as u32, (z + dz) as u32]);
                }
            }
        }
        for n in ip.first[self.last_matched_turn as usize]..ip.changes.len() {
            let [x, y, z] = ip.changes[n].map(|v| v as usize);
            if let Some(mask) = grid.mask.get_mut(x + y * grid.size[0] + z * grid.size[0] * grid.size[1]) {
                *mask = false;
            }
        }
        self.counter += 1;
        self.match_count = 0;
        Ok(true)
    }

    fn go_prl(&mut self, grid: &mut Grid, ip: &mut Ip) -> Result<bool, Fault> {
        if !self.find(grid, ip)? {
            return Ok(false);
        }
        for n in ip.first[ip.counter]..ip.changes.len() {
            let [x, y, z] = ip.changes[n].map(|v| v as usize);
            let i = grid.at(x, y, z);
            grid.state[i] = self.newstate[i];
        }
        self.counter += 1;
        Ok(self.match_count > 0)
    }
}

struct PathNode {
    grid: usize,
    start: u32,
    finish: u32,
    substrate: u32,
    value: u8,
    inertia: bool,
    longest: bool,
    edges: bool,
    vertices: bool,
}

impl PathNode {
    /// The steps that may be taken from a cell: to its face neighbours, and across edges and corners if asked.
    fn directions(&self, x: i32, y: i32, z: i32, size: [usize; 3]) -> Vec<[i32; 3]> {
        let [mx, my, mz] = size.map(|s| s as i32);
        let mut out = Vec::new();
        let inside = |d: [i32; 3]| x + d[0] >= 0 && y + d[1] >= 0 && z + d[2] >= 0 && x + d[0] < mx && y + d[1] < my && z + d[2] < mz;
        const FACES: [[i32; 3]; 6] = [[-1, 0, 0], [1, 0, 0], [0, -1, 0], [0, 1, 0], [0, 0, -1], [0, 0, 1]];
        const EDGES: [[i32; 3]; 12] = [[-1, -1, 0], [-1, 1, 0], [1, -1, 0], [1, 1, 0], [-1, 0, -1], [-1, 0, 1], [1, 0, -1], [1, 0, 1], [0, -1, -1], [0, -1, 1], [0, 1, -1], [0, 1, 1]];
        const VERTICES: [[i32; 3]; 8] = [[-1, -1, -1], [-1, -1, 1], [-1, 1, -1], [-1, 1, 1], [1, -1, -1], [1, -1, 1], [1, 1, -1], [1, 1, 1]];
        let flat = mz == 1;
        out.extend(FACES.iter().take(if flat { 4 } else { 6 }).copied().filter(|d| inside(*d)));
        if self.edges {
            out.extend(EDGES.iter().take(if flat { 4 } else { 12 }).copied().filter(|d| inside(*d)));
        }
        if self.vertices && !flat {
            out.extend(VERTICES.iter().copied().filter(|d| inside(*d)));
        }
        out
    }

    /// The step from a cell towards the finish: one of those that lead a generation nearer, keeping straight on if `inertia` asks.
    fn direction(&self, at: [i32; 3], d: [i32; 3], generations: &[i32], size: [usize; 3], random: &mut Random) -> Option<[i32; 3]> {
        let [mx, my, mz] = size.map(|s| s as i32);
        let index = |p: [i32; 3]| (p[0] + p[1] * mx + p[2] * mx * my) as usize;
        let g = generations[index(at)];
        let nearer = |step: &[i32; 3]| generations[index([at[0] + step[0], at[1] + step[1], at[2] + step[2]])] == g - 1;
        let moving = d != [0, 0, 0];
        if !self.vertices && !self.edges {
            if moving && self.inertia {
                let c = [at[0] + d[0], at[1] + d[1], at[2] + d[2]];
                if c[0] >= 0 && c[1] >= 0 && c[2] >= 0 && c[0] < mx && c[1] < my && c[2] < mz && generations[index(c)] == g - 1 {
                    return Some(d);
                }
            }
            const FACES: [[i32; 3]; 6] = [[-1, 0, 0], [1, 0, 0], [0, -1, 0], [0, 1, 0], [0, 0, -1], [0, 0, 1]];
            let candidates: Vec<[i32; 3]> = FACES.iter().copied().filter(|s| at[0] + s[0] >= 0 && at[1] + s[1] >= 0 && at[2] + s[2] >= 0 && at[0] + s[0] < mx && at[1] + s[1] < my && at[2] + s[2] < mz && nearer(s)).collect();
            if candidates.is_empty() {
                return None;
            }
            return Some(candidates[random.below(candidates.len())]);
        }
        let candidates: Vec<[i32; 3]> = self.directions(at[0], at[1], at[2], size).into_iter().filter(nearer).collect();
        if candidates.is_empty() {
            return None;
        }
        if self.inertia && moving {
            let (mut best, mut result) = (-4.0, None);
            for c in candidates {
                let noise = 0.1 * random.unit();
                let cos = (c[0] * d[0] + c[1] * d[1] + c[2] * d[2]) as f64 / (((c[0] * c[0] + c[1] * c[1] + c[2] * c[2]) * (d[0] * d[0] + d[1] * d[1] + d[2] * d[2])) as f64).sqrt();
                if cos + noise > best {
                    best = cos + noise;
                    result = Some(c);
                }
            }
            return result;
        }
        Some(candidates[random.below(candidates.len())])
    }

    fn go(&mut self, grid: &mut Grid, ip: &mut Ip) -> bool {
        let size = grid.size;
        let [mx, my, _] = size;
        let mut frontier: VecDeque<(i32, i32, i32, i32)> = VecDeque::new();
        let mut starts: Vec<[i32; 3]> = Vec::new();
        let mut generations = vec![-1; grid.state.len()];
        for (i, s) in grid.state.iter().enumerate() {
            let p = [(i % mx) as i32, (i / mx % my) as i32, (i / (mx * my)) as i32];
            if self.start >> s & 1 == 1 {
                starts.push(p);
            }
            if self.finish >> s & 1 == 1 {
                generations[i] = 0;
                frontier.push_back((0, p[0], p[1], p[2]));
            }
        }
        if starts.is_empty() || frontier.is_empty() {
            return false;
        }
        let index = |p: [i32; 3]| p[0] as usize + p[1] as usize * mx + p[2] as usize * mx * my;
        while let Some((t, x, y, z)) = frontier.pop_front() {
            for d in self.directions(x, y, z, size) {
                let p = [x + d[0], y + d[1], z + d[2]];
                let i = index(p);
                let v = grid.state[i];
                if generations[i] == -1 && (self.substrate >> v & 1 == 1 || self.start >> v & 1 == 1) {
                    if self.substrate >> v & 1 == 1 {
                        frontier.push_back((t + 1, p[0], p[1], p[2]));
                    }
                    generations[i] = t + 1;
                }
            }
        }
        if !starts.iter().any(|p| generations[index(*p)] > 0) {
            return false;
        }
        let mut local = Random::new(ip.random.seed());
        let (mut min, mut max) = (grid.state.len() as f64, -2.0);
        let (mut argmin, mut argmax) = (None, None);
        for p in &starts {
            let g = generations[index(*p)];
            if g == -1 {
                continue;
            }
            let noise = 0.1 * local.unit();
            if g as f64 + noise < min {
                min = g as f64 + noise;
                argmin = Some(*p);
            }
            if g as f64 + noise > max {
                max = g as f64 + noise;
                argmax = Some(*p);
            }
        }
        let Some(mut pen) = (if self.longest { argmax } else { argmin }) else { return false };
        let Some(mut dir) = self.direction(pen, [0, 0, 0], &generations, size, &mut local) else { return false };
        pen = [pen[0] + dir[0], pen[1] + dir[1], pen[2] + dir[2]];
        while generations[index(pen)] != 0 {
            grid.state[index(pen)] = self.value;
            ip.changes.push(pen.map(|v| v as u32));
            let Some(next) = self.direction(pen, dir, &generations, size, &mut local) else { break };
            dir = next;
            pen = [pen[0] + dir[0], pen[1] + dir[1], pen[2] + dir[2]];
        }
        true
    }
}

struct ConvolutionRule {
    input: u8,
    output: u8,
    values: Vec<u8>,
    /// for each sum from 0 to 27, whether the rule applies; empty for "whatever the sum"
    sums: Vec<bool>,
    p: f64,
}

struct ConvolutionNode {
    grid: usize,
    rules: Vec<ConvolutionRule>,
    kernel: Vec<i32>,
    periodic: bool,
    counter: usize,
    steps: i64,
}

impl ConvolutionNode {
    fn go(&mut self, grid: &mut Grid, ip: &mut Ip) -> bool {
        if self.steps > 0 && self.counter as i64 >= self.steps {
            return false;
        }
        let size = grid.size.map(|s| s as i64);
        let values = grid.characters.len();
        let flat = size[2] == 1;
        let mut sumfield = vec![0i32; grid.state.len() * values];
        for z in 0..size[2] {
            for y in 0..size[1] {
                for x in 0..size[0] {
                    let at = (x + y * size[0] + z * size[0] * size[1]) as usize * values;
                    for dz in if flat { 0..=0 } else { -1..=1 } {
                        for dy in -1..=1 {
                            for dx in -1..=1 {
                                let mut s = [x + dx, y + dy, z + dz];
                                if self.periodic {
                                    for k in 0..3 {
                                        s[k] = s[k].rem_euclid(size[k]);
                                    }
                                } else if (0..3).any(|k| s[k] < 0 || s[k] >= size[k]) {
                                    continue;
                                }
                                let weight = self.kernel[(dx + 1 + (dy + 1) * 3 + if flat { 0 } else { (dz + 1) * 9 }) as usize];
                                sumfield[at + grid.state[(s[0] + s[1] * size[0] + s[2] * size[0] * size[1]) as usize] as usize] += weight;
                            }
                        }
                    }
                }
            }
        }
        let mut change = false;
        for i in 0..grid.state.len() {
            let input = grid.state[i];
            for rule in &self.rules {
                if input == rule.input && rule.output != grid.state[i] && (rule.p == 1.0 || ip.random.unit() < rule.p) {
                    let mut success = true;
                    if !rule.sums.is_empty() {
                        let sum: i32 = rule.values.iter().map(|v| sumfield[i * values + *v as usize]).sum();
                        success = rule.sums.get(sum as usize).copied().unwrap_or(false);
                    }
                    if success {
                        grid.state[i] = rule.output;
                        change = true;
                        break;
                    }
                }
            }
        }
        self.counter += 1;
        change
    }
}

struct MapNode {
    /// the grid it reads; the grid it makes is its branch's
    from: usize,
    rules: Vec<Rule>,
    /// for each axis, the new grid's cells to the old grid's: a numerator and a denominator
    scale: [(usize, usize); 3],
}

impl MapNode {
    fn go(&self, old: &Grid, new: &mut Grid) {
        new.state.iter_mut().for_each(|v| *v = 0);
        for rule in &self.rules {
            let ([ix, iy, iz], [ox, oy, oz]) = (rule.input_size, rule.output_size);
            for z in 0..old.size[2] {
                for y in 0..old.size[1] {
                    for x in 0..old.size[0] {
                        // (a pattern that runs off one side is read on from the other)
                        let found = (0..iz).all(|dz| (0..iy).all(|dy| (0..ix).all(|dx| rule.input[dx + dy * ix + dz * ix * iy] >> old.state[old.at((x + dx) % old.size[0], (y + dy) % old.size[1], (z + dz) % old.size[2])] & 1 == 1)));
                        if !found {
                            continue;
                        }
                        let at = [x * self.scale[0].0 / self.scale[0].1, y * self.scale[1].0 / self.scale[1].1, z * self.scale[2].0 / self.scale[2].1];
                        for dz in 0..oz {
                            for dy in 0..oy {
                                for dx in 0..ox {
                                    let output = rule.output[dx + dy * ox + dz * ox * oy];
                                    if output != KEEP {
                                        let i = new.at((at[0] + dx) % new.size[0], (at[1] + dy) % new.size[1], (at[2] + dz) % new.size[2]);
                                        new.state[i] = output;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

enum BranchKind {
    Sequence,
    Markov,
    Map(Box<MapNode>),
    Wfc(Box<TileNode>),
}

struct Branch {
    kind: BranchKind,
    nodes: Vec<usize>,
    /// which child runs next; -1 for a `map` or `wfc` that has not yet made its grid
    n: i64,
    grid: usize,
}

enum Node {
    Branch(Branch),
    Rule(Box<RuleNode>),
    Path(PathNode),
    Convolution(ConvolutionNode),
}

/// A program loaded and ready to run, or part way through a run.
pub struct Machine {
    nodes: Vec<Option<Node>>,
    /// for each node, the branch that takes over when it is done, if any
    parents: Vec<Option<usize>>,
    grids: Vec<Grid>,
    root: usize,
    origin: bool,
    ip: Ip,
    /// for each node, the line of the program it was written on: for saying what a run is doing
    lines: Vec<usize>,
}

/// What loading a program needs to hand down through its nodes.
struct Loader<'a> {
    nodes: Vec<Option<Node>>,
    parents: Vec<Option<usize>>,
    grids: Vec<Grid>,
    resources: &'a Resources,
    limits: &'a Limits,
    file: &'a str,
    root_flat: bool,
    lines: Vec<usize>,
}

impl Loader<'_> {
    fn push(&mut self, node: Node, line: usize) -> Result<usize, Fault> {
        within(Thing::Nodes, self.nodes.len() as u64 + 1, self.limits.nodes, self.file, line)?;
        self.nodes.push(Some(node));
        self.parents.push(None);
        self.lines.push(line);
        Ok(self.nodes.len() - 1)
    }

    fn rules(&self, element: &Element, grid: usize, parent_symmetry: &[bool]) -> Result<Vec<Rule>, Fault> {
        let g = &self.grids[grid];
        let flat = g.size[2] == 1;
        let own = symmetry(flat, element.get("symmetry"), parent_symmetry, self.file, element.line)?;
        let written: Vec<&Element> = element.elements(&["rule"]).collect();
        let written = if written.is_empty() { vec![element] } else { written };
        let mut rules = Vec::new();
        for xrule in written {
            let rule = Rule::load(xrule, g, g, true, self.resources, self.file, self.limits)?;
            let of_rule = symmetry(flat, xrule.get("symmetry"), &own, self.file, xrule.line)?;
            rules.extend(rule.symmetries(&of_rule, flat));
            within(Thing::Rules, rules.len() as u64, self.limits.rules, self.file, xrule.line)?;
        }
        Ok(rules)
    }

    /// The node an element describes, and the nodes inside it.
    fn node(&mut self, element: &Element, parent_symmetry: &[bool], grid: usize, depth: u64) -> Result<usize, Fault> {
        let (file, line) = (self.file.to_string(), element.line);
        within(Thing::Depth, depth, self.limits.depth, self.file, line)?;
        let unbuilt = |what: &str| Fault::NotImplemented { file: file.clone(), line, node: what.to_string() };
        let missing = |attribute| Fault::Missing { file: file.clone(), line, node: element.name.clone(), attribute };
        match element.name.as_str() {
            "one" | "all" | "prl" => {
                if let Some(found) = element.elements(&["observe"]).next() {
                    return Err(Fault::NotImplemented { file, line: found.line, node: "observe".into() });
                }
                let kind = match element.name.as_str() {
                    "one" => RuleKind::One,
                    "all" => RuleKind::All,
                    _ => RuleKind::Prl,
                };
                let rules = self.rules(element, grid, parent_symmetry)?;
                let g = &self.grids[grid];
                let mut fields: Vec<Option<Field>> = Vec::new();
                for xfield in element.elements(&["field"]) {
                    let lacks = |attribute| Fault::Missing { file: file.clone(), line: xfield.line, node: "field".into(), attribute };
                    fields.resize_with(g.characters.len(), || None);
                    let value = g.value_at(xfield.get("for").and_then(|s| s.chars().next()).ok_or_else(|| lacks("for"))?, &file, xfield.line)?;
                    let (inversed, zero) = match (xfield.get("from"), xfield.get("to")) {
                        (Some(from), _) => (true, from),
                        (None, Some(to)) => (false, to),
                        (None, None) => return Err(lacks("to")),
                    };
                    fields[value as usize] = Some(Field { recompute: flag(xfield, "recompute", &file)?, essential: flag(xfield, "essential", &file)?, inversed, zero: g.wave(zero, &file, xfield.line)?, substrate: g.wave(xfield.get("on").ok_or_else(|| lacks("on"))?, &file, xfield.line)? });
                }
                let cells = g.state.len();
                let potentials = if fields.is_empty() { Vec::new() } else { vec![vec![0; cells]; g.characters.len()] };
                let node = RuleNode { kind, grid, line, rules, counter: 0, steps: number(element, "steps", 0i64, &file)?.max(0) as usize, matches: Vec::new(), match_count: 0, last_matched_turn: -1, mask: Vec::new(), potentials, fields, temperature: number(element, "temperature", 0.0, &file)?, newstate: if kind == RuleKind::Prl { vec![0; cells] } else { Vec::new() } };
                self.push(Node::Rule(Box::new(node)), line)
            }
            "path" => {
                let g = &self.grids[grid];
                let from = element.get("from").filter(|s| !s.is_empty()).ok_or_else(|| missing("from"))?;
                let colour = match element.get("color") {
                    Some(c) => c.chars().next().ok_or_else(|| missing("color"))?,
                    None => from.chars().next().ok_or_else(|| missing("from"))?,
                };
                let node = PathNode { grid, start: g.wave(from, &file, line)?, finish: g.wave(element.get("to").ok_or_else(|| missing("to"))?, &file, line)?, substrate: g.wave(element.get("on").ok_or_else(|| missing("on"))?, &file, line)?, value: g.value_at(colour, &file, line)?, inertia: flag(element, "inertia", &file)?, longest: flag(element, "longest", &file)?, edges: flag(element, "edges", &file)?, vertices: flag(element, "vertices", &file)? };
                self.push(Node::Path(node), line)
            }
            "convolution" => {
                let g = &self.grids[grid];
                let flat = g.size[2] == 1;
                let neighborhood = element.get("neighborhood").ok_or_else(|| missing("neighborhood"))?;
                let kernel: Vec<i32> = match (flat, neighborhood) {
                    (true, "VonNeumann") => vec![0, 1, 0, 1, 0, 1, 0, 1, 0],
                    (true, "Moore") => vec![1, 1, 1, 1, 0, 1, 1, 1, 1],
                    (false, "VonNeumann") => vec![0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 1, 0, 1, 0, 1, 0, 1, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0],
                    (false, "NoCorners") => vec![0, 1, 0, 1, 1, 1, 0, 1, 0, 1, 1, 1, 1, 0, 1, 1, 1, 1, 0, 1, 0, 1, 1, 1, 0, 1, 0],
                    _ => return Err(Fault::BadValue { file, line, attribute: "neighborhood", value: neighborhood.to_string() }),
                };
                let written: Vec<&Element> = element.elements(&["rule"]).collect();
                let written = if written.is_empty() { vec![element] } else { written };
                let mut rules = Vec::new();
                for xrule in written {
                    let lacks = |attribute| Fault::Missing { file: file.clone(), line: xrule.line, node: "rule".into(), attribute };
                    let letter = |attribute: &'static str| xrule.get(attribute).and_then(|s| s.chars().next()).ok_or_else(|| lacks(attribute)).and_then(|c| g.value_at(c, &file, xrule.line));
                    let (mut values, mut sums) = (Vec::new(), Vec::new());
                    match (xrule.get("values"), xrule.get("sum")) {
                        (Some(v), Some(s)) => {
                            values = v.chars().map(|c| g.value_at(c, &file, xrule.line)).collect::<Result<_, _>>()?;
                            sums = vec![false; 28];
                            let bad = || Fault::BadValue { file: file.clone(), line: xrule.line, attribute: "sum", value: s.to_string() };
                            for interval in s.split(',') {
                                let (low, high) = interval.split_once("..").unwrap_or((interval, interval));
                                let (low, high) = (low.trim().parse::<usize>().map_err(|_| bad())?, high.trim().parse::<usize>().map_err(|_| bad())?);
                                if low > high || high >= sums.len() {
                                    return Err(bad());
                                }
                                sums[low..=high].iter_mut().for_each(|on| *on = true);
                            }
                        }
                        (Some(_), None) => return Err(lacks("sum")),
                        (None, Some(_)) => return Err(lacks("values")),
                        (None, None) => {}
                    }
                    rules.push(ConvolutionRule { input: letter("in")?, output: letter("out")?, values, sums, p: number(xrule, "p", 1.0, &file)? });
                }
                let node = ConvolutionNode { grid, rules, kernel, periodic: flag(element, "periodic", &file)?, counter: 0, steps: number(element, "steps", -1, &file)? };
                self.push(Node::Convolution(node), line)
            }
            "convchain" => Err(unbuilt("convchain")),
            "wfc" if element.get("sample").is_some() => Err(unbuilt("wfc with a sample")),
            "sequence" | "markov" | "map" | "wfc" => {
                // a `map` or `wfc` makes the grid its children work on
                let (kind, inner) = match element.name.as_str() {
                    "sequence" => (BranchKind::Sequence, grid),
                    "markov" => (BranchKind::Markov, grid),
                    "map" => {
                        let text = element.get("scale").ok_or_else(|| missing("scale"))?;
                        let bad = || Fault::BadValue { file: file.clone(), line, attribute: "scale", value: text.to_string() };
                        let parts: Vec<(usize, usize)> = text.split(' ').map(|part| {
                            let (n, d) = part.split_once('/').unwrap_or((part, "1"));
                            Ok((n.parse::<usize>().map_err(|_| bad())?, d.parse::<usize>().ok().filter(|d| *d > 0).ok_or_else(bad)?))
                        }).collect::<Result<_, Fault>>()?;
                        let scale: [(usize, usize); 3] = parts.try_into().map_err(|_| bad())?;
                        let old = self.grids[grid].size;
                        let size = [0, 1, 2].map(|k| old[k].saturating_mul(scale[k].0) / scale[k].1);
                        let new = Grid::load(element, size, &file, self.limits)?;
                        self.grids.push(new);
                        (BranchKind::Map(Box::new(MapNode { from: grid, rules: Vec::new(), scale })), self.grids.len() - 1)
                    }
                    _ => {
                        let (tiles, new) = TileNode::load(element, &self.grids[grid], grid, self.resources, &file, self.limits)?;
                        self.grids.push(new);
                        (BranchKind::Wfc(Box::new(tiles)), self.grids.len() - 1)
                    }
                };
                let own = symmetry(self.root_flat, element.get("symmetry"), parent_symmetry, &file, line)?;
                let mut children = Vec::new();
                for child in element.elements(&NODES) {
                    children.push(self.node(child, &own, inner, depth + 1)?);
                }
                let mut kind = kind;
                if let BranchKind::Map(map) = &mut kind {
                    let (old, new) = (&self.grids[grid], &self.grids[inner]);
                    let flat = old.size[2] == 1;
                    let of_map = symmetry(flat, element.get("symmetry"), parent_symmetry, &file, line)?;
                    for xrule in element.elements(&["rule"]) {
                        map.rules.extend(Rule::load(xrule, old, new, false, self.resources, &file, self.limits)?.symmetries(&of_map, flat));
                        within(Thing::Rules, map.rules.len() as u64, self.limits.rules, &file, xrule.line)?;
                    }
                }
                let starts = if matches!(kind, BranchKind::Map(_) | BranchKind::Wfc(_)) { -1 } else { 0 };
                let ends_program = starts == -1;
                let id = self.push(Node::Branch(Branch { kind, nodes: children.clone(), n: starts, grid: inner }), line)?;
                for child in children {
                    // (a branch hands back to the branch it is in; a `map` or `wfc` to none, which ends the program)
                    if matches!(self.nodes[child], Some(Node::Branch(_))) {
                        let makes_grid = matches!(&self.nodes[child], Some(Node::Branch(Branch { kind: BranchKind::Map(_) | BranchKind::Wfc(_), .. })));
                        self.parents[child] = if makes_grid { None } else { Some(id) };
                    }
                }
                let _ = ends_program;
                Ok(id)
            }
            other => Err(Fault::UnknownNode { file, line, node: other.to_string() }),
        }
    }
}

impl Machine {
    /// Load a program for a grid of `size` cells. `file` is the name it is called in messages.
    pub fn load(file: &str, text: &[u8], size: [usize; 3], resources: &Resources, limits: &Limits) -> Result<Machine, Fault> {
        let root = super::xml::read(file, text, limits)?;
        let grid = Grid::load(&root, size, file, limits)?;
        let flat = size[2] == 1;
        let all = symmetry(flat, root.get("symmetry"), &vec![true; if flat { 8 } else { 48 }], file, root.line)?;
        let mut loader = Loader { nodes: Vec::new(), parents: Vec::new(), grids: vec![grid], resources, limits, file, root_flat: flat, lines: Vec::new() };
        let mut top = loader.node(&root, &all, 0, 1)?;
        if !matches!(loader.nodes[top], Some(Node::Branch(_))) {
            // a program that is one node runs as a markov node holding it: over and over until it has nothing to do
            top = loader.push(Node::Branch(Branch { kind: BranchKind::Markov, nodes: vec![top], n: 0, grid: 0 }), root.line)?;
        }
        let origin = flag(&root, "origin", file)?;
        let ip = Ip { changes: Vec::new(), first: vec![0], counter: 0, random: Random::new(0), current: None, grid: 0, limits: *limits, file: file.to_string() };
        let mut machine = Machine { nodes: loader.nodes, parents: loader.parents, grids: loader.grids, root: top, origin, ip, lines: loader.lines };
        machine.start(0);
        Ok(machine)
    }

    /// Begin a run afresh from a seed.
    pub fn start(&mut self, seed: u64) {
        self.ip.random = Random::new(seed);
        self.ip.grid = 0;
        let grid = &mut self.grids[0];
        grid.state.iter_mut().for_each(|v| *v = 0);
        if self.origin {
            let middle = grid.at(grid.size[0] / 2, grid.size[1] / 2, grid.size[2] / 2);
            grid.state[middle] = 1.min(grid.characters.len() as u8 - 1);
        }
        self.ip.changes.clear();
        self.ip.first.clear();
        self.ip.first.push(0);
        self.ip.counter = 0;
        self.reset(self.root);
        self.ip.current = Some(self.root);
    }

    /// Whether the program has more to do.
    pub fn running(&self) -> bool {
        self.ip.current.is_some()
    }

    /// Turns taken since the run began.
    pub fn steps(&self) -> usize {
        self.ip.counter
    }

    /// Take one turn. Returns whether the program has more to do.
    pub fn step(&mut self) -> Result<bool, Fault> {
        let Some(current) = self.ip.current else { return Ok(false) };
        self.go(current)?;
        self.ip.counter += 1;
        self.ip.first.push(self.ip.changes.len());
        Ok(self.ip.current.is_some())
    }

    /// Take up to `turns` turns, stopping when the program is done. Returns whether it has more to do.
    pub fn advance(&mut self, turns: usize) -> Result<bool, Fault> {
        for _ in 0..turns {
            if !self.step()? {
                return Ok(false);
            }
        }
        Ok(self.running())
    }

    /// Run to the end, or to the limit on steps, which is a fault that leaves what was made in place.
    pub fn finish(&mut self) -> Result<(), Fault> {
        while self.running() {
            if self.ip.counter >= self.ip.limits.steps {
                return Err(Fault::StepLimit { steps: self.ip.counter });
            }
            self.step()?;
        }
        Ok(())
    }

    /// The grid as it stands: its size, the value in each cell (x fastest, then y, then z) and the letter of each value.
    pub fn view(&mut self) -> View<'_> {
        // a wfc node part way through shows, in each cell, what most of its remaining tiles have there
        for id in 0..self.nodes.len() {
            if let Some(Node::Branch(Branch { kind: BranchKind::Wfc(tiles), n: -1, grid, .. })) = &mut self.nodes[id] {
                // (one that has finished stands at -1 again, ready for another run, and has nothing more to show)
                if *grid == self.ip.grid && !tiles.first_go {
                    let (old, new) = two(&mut self.grids, tiles.from, *grid);
                    tiles.show(old, new);
                }
            }
        }
        let grid = &self.grids[self.ip.grid];
        View { size: grid.size, cells: &grid.state, letters: &grid.characters }
    }

    /// Every letter any of the program's grids has, each once: the
    /// letters of the grid made last first, since that is the grid a
    /// finished run is seen in, then those of the grids before it. A
    /// form's colours are numbered by this list, so that a cell keeps its
    /// colour when a `map` or `wfc` node moves the run to another grid
    /// with other letters.
    pub fn letters(&self) -> Vec<char> {
        let mut all: Vec<char> = Vec::new();
        for grid in self.grids.iter().rev() {
            for letter in &grid.characters {
                if !all.contains(letter) {
                    all.push(*letter);
                }
            }
        }
        all
    }

    /// The size of the grid made last, which is the one a run ends in.
    pub fn last_size(&self) -> [usize; 3] {
        self.grids.last().map_or([0; 3], |grid| grid.size)
    }

    /// What runs next turn: the node's name and the line it is written on. None once the program is done.
    pub fn doing(&self) -> Option<(&'static str, usize)> {
        let mut id = self.ip.current?;
        // (down from the branch whose turn it is to the node it will ask, which may be a branch that has yet to be given the turn)
        for _ in 0..self.nodes.len() {
            let Some(Node::Branch(branch)) = &self.nodes[id] else { break };
            match &branch.kind {
                BranchKind::Wfc(_) if branch.n < 0 => return Some(("wfc", self.lines[id])),
                BranchKind::Map(_) if branch.n < 0 => return Some(("map", self.lines[id])),
                _ => {}
            }
            let at = if matches!(branch.kind, BranchKind::Markov) { 0 } else { branch.n.max(0) as usize };
            match branch.nodes.get(at) {
                Some(child) => id = *child,
                None => break,
            }
        }
        let name = match &self.nodes[id] {
            Some(Node::Rule(rule)) => match rule.kind {
                RuleKind::One => "one",
                RuleKind::All => "all",
                RuleKind::Prl => "prl",
            },
            Some(Node::Path(_)) => "path",
            Some(Node::Convolution(_)) => "convolution",
            Some(Node::Branch(branch)) => match branch.kind {
                BranchKind::Sequence => "sequence",
                BranchKind::Markov => "markov",
                BranchKind::Map(_) => "map",
                BranchKind::Wfc(_) => "wfc",
            },
            None => return None,
        };
        Some((name, self.lines[id]))
    }

    fn reset(&mut self, id: usize) {
        let Some(mut node) = self.nodes[id].take() else { return };
        match &mut node {
            Node::Branch(branch) => self.reset_branch(branch),
            Node::Rule(rule) => rule.reset(),
            Node::Path(_) => {}
            Node::Convolution(convolution) => convolution.counter = 0,
        }
        self.nodes[id] = Some(node);
    }

    fn reset_branch(&mut self, branch: &mut Branch) {
        for child in &branch.nodes {
            self.reset(*child);
        }
        branch.n = 0;
        match &mut branch.kind {
            BranchKind::Map(_) => branch.n = -1,
            BranchKind::Wfc(tiles) => {
                branch.n = -1;
                tiles.first_go = true;
            }
            _ => {}
        }
    }

    fn go(&mut self, id: usize) -> Result<bool, Fault> {
        let Some(mut node) = self.nodes[id].take() else { return Ok(false) };
        let result = match &mut node {
            Node::Branch(branch) => self.go_branch(branch),
            Node::Rule(rule) => {
                let grid = &mut self.grids[rule.grid];
                match rule.kind {
                    RuleKind::One => rule.go_one(grid, &mut self.ip),
                    RuleKind::All => rule.go_all(grid, &mut self.ip),
                    RuleKind::Prl => rule.go_prl(grid, &mut self.ip),
                }
            }
            Node::Path(path) => Ok(path.go(&mut self.grids[path.grid], &mut self.ip)),
            Node::Convolution(convolution) => Ok(convolution.go(&mut self.grids[convolution.grid], &mut self.ip)),
        };
        self.nodes[id] = Some(node);
        result
    }

    fn go_branch(&mut self, branch: &mut Branch) -> Result<bool, Fault> {
        match &mut branch.kind {
            BranchKind::Markov => branch.n = 0,
            BranchKind::Sequence => {}
            BranchKind::Map(map) if branch.n < 0 => {
                let (old, new) = two(&mut self.grids, map.from, branch.grid);
                map.go(old, new);
                self.ip.grid = branch.grid;
                branch.n += 1;
                return Ok(true);
            }
            BranchKind::Wfc(tiles) if branch.n < 0 => {
                let (old, new) = two(&mut self.grids, tiles.from, branch.grid);
                let went = tiles.go(&mut branch.n, old, new, &mut self.ip)?;
                if went {
                    self.ip.grid = branch.grid;
                }
                return Ok(went);
            }
            _ => {}
        }
        while (branch.n as usize) < branch.nodes.len() {
            let child = branch.nodes[branch.n as usize];
            if matches!(self.nodes[child], Some(Node::Branch(_))) {
                self.ip.current = Some(child);
            }
            if self.go(child)? {
                return Ok(true);
            }
            branch.n += 1;
        }
        self.ip.current = self.ip.current.and_then(|current| self.parents[current]);
        self.reset_branch(branch);
        Ok(false)
    }
}

/// Two grids of the list at once, one to read and one to write. The one written was made later, so it is further along.
fn two(grids: &mut [Grid], read: usize, write: usize) -> (&Grid, &mut Grid) {
    let (before, after) = grids.split_at_mut(write);
    (&before[read.min(write.saturating_sub(1))], &mut after[0])
}

/// A grid as it stands.
pub struct View<'a> {
    pub size: [usize; 3],
    pub cells: &'a [u8],
    pub letters: &'a [char],
}
