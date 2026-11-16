//! Grids, rewrite rules and their symmetries.
//!
//! A grid's cells each hold one of up to 32 values, named by letters. A
//! rule is an input pattern and an output pattern: where the input is
//! found, the output is written. An input cell is a set of values (a
//! letter, a union of letters, or `*` for any), kept as one bit a value;
//! an output cell is a value, or `*` for "leave as it is".
//!
//! Everything here follows the original's `Grid.cs`, `Rule.cs` and
//! `SymmetryHelper.cs` closely enough that a program means the same
//! thing: the order of a pattern's layers, the order symmetries are
//! tried in, and which copies count as the same.

use super::fault::{within, Fault, Limits, Thing};
use super::xml::Element;
use super::Resources;

/// The output value that leaves a cell as it is.
pub const KEEP: u8 = 0xff;

#[derive(Clone, Debug)]
pub struct Grid {
    pub state: Vec<u8>,
    /// cells written this turn by an `all` node, so that two rules do not both write one
    pub mask: Vec<bool>,
    pub size: [usize; 3],
    /// the letter of each value, in order
    pub characters: Vec<char>,
    /// each letter that may stand in an input (values, unions and `*`) and the values it stands for
    waves: Vec<(char, u32)>,
    pub folder: Option<String>,
}

impl Grid {
    /// The grid an element declares with its `values`.
    pub fn load(element: &Element, size: [usize; 3], file: &str, limits: &Limits) -> Result<Grid, Fault> {
        let here = |attribute| Fault::Missing { file: file.to_string(), line: element.line, node: element.name.clone(), attribute };
        let values: Vec<char> = element.get("values").ok_or_else(|| here("values"))?.chars().filter(|c| *c != ' ').collect();
        if values.is_empty() {
            return Err(here("values"));
        }
        within(Thing::Values, values.len() as u64, 32, file, element.line)?;
        if size.iter().any(|side| *side == 0) {
            return Err(Fault::NoGrid { size });
        }
        let cells = size.iter().try_fold(1u64, |all, side| all.checked_mul(*side as u64)).unwrap_or(u64::MAX);
        within(Thing::Grid, cells, limits.cells, file, element.line)?;
        let mut grid = Grid { state: vec![0; cells as usize], mask: vec![false; cells as usize], size, characters: Vec::new(), waves: Vec::new(), folder: element.get("folder").map(str::to_string) };
        for (i, symbol) in values.iter().enumerate() {
            if grid.characters.contains(symbol) {
                return Err(Fault::Repeated { file: file.to_string(), line: element.line, symbol: *symbol });
            }
            grid.characters.push(*symbol);
            grid.waves.push((*symbol, 1 << i));
        }
        grid.waves.push(('*', if values.len() == 32 { u32::MAX } else { (1 << values.len()) - 1 }));
        // Unions are looked for through sequence and markov nodes only, breadth first, as the original looks.
        let mut queue: std::collections::VecDeque<&Element> = element.elements(&["markov", "sequence", "union"]).collect();
        while let Some(found) = queue.pop_front() {
            queue.extend(found.elements(&["markov", "sequence", "union"]));
            if found.name != "union" {
                continue;
            }
            let missing = |attribute| Fault::Missing { file: file.to_string(), line: found.line, node: "union".into(), attribute };
            let symbol = found.get("symbol").and_then(|s| s.chars().next()).ok_or_else(|| missing("symbol"))?;
            if grid.waves.iter().any(|(known, _)| *known == symbol) {
                return Err(Fault::Repeated { file: file.to_string(), line: found.line, symbol });
            }
            let wave = grid.wave(found.get("values").ok_or_else(|| missing("values"))?, file, found.line)?;
            grid.waves.push((symbol, wave));
        }
        Ok(grid)
    }

    pub fn value(&self, symbol: char) -> Option<u8> {
        self.characters.iter().position(|c| *c == symbol).map(|i| i as u8)
    }

    pub fn value_at(&self, symbol: char, file: &str, line: usize) -> Result<u8, Fault> {
        self.value(symbol).ok_or(Fault::UnknownSymbol { file: file.to_string(), line, symbol })
    }

    /// The set of values some letters name, a bit each.
    pub fn wave(&self, symbols: &str, file: &str, line: usize) -> Result<u32, Fault> {
        symbols.chars().try_fold(0, |sum, symbol| Ok(sum | 1 << self.value_at(symbol, file, line)?))
    }

    fn input(&self, symbol: char) -> Option<u32> {
        self.waves.iter().find(|(known, _)| *known == symbol).map(|(_, wave)| *wave)
    }

    pub fn at(&self, x: usize, y: usize, z: usize) -> usize {
        x + y * self.size[0] + z * self.size[0] * self.size[1]
    }

    pub fn matches(&self, rule: &Rule, x: usize, y: usize, z: usize) -> bool {
        let [mx, my, _] = self.size;
        let mut di = 0;
        for dz in 0..rule.input_size[2] {
            for dy in 0..rule.input_size[1] {
                let row = x + (y + dy) * mx + (z + dz) * mx * my;
                for dx in 0..rule.input_size[0] {
                    if rule.input[di] & 1 << self.state[row + dx] == 0 {
                        return false;
                    }
                    di += 1;
                }
            }
        }
        true
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Rule {
    pub input_size: [usize; 3],
    pub output_size: [usize; 3],
    pub input: Vec<u32>,
    pub output: Vec<u8>,
    pub p: f64,
    /// for each value, the places in the input that accept it
    pub shifts: Vec<Vec<[usize; 3]>>,
}

/// A box of letters, as a rule is written or a .vox file holds it.
struct Letters {
    size: [usize; 3],
    cells: Vec<char>,
}

impl Rule {
    fn new(input: Vec<u32>, input_size: [usize; 3], output: Vec<u8>, output_size: [usize; 3], values: usize, p: f64) -> Rule {
        let mut shifts = vec![Vec::new(); values];
        for z in 0..input_size[2] {
            for y in 0..input_size[1] {
                for x in 0..input_size[0] {
                    let wave = input[x + y * input_size[0] + z * input_size[0] * input_size[1]];
                    for (value, list) in shifts.iter_mut().enumerate() {
                        if wave >> value & 1 == 1 {
                            list.push([x, y, z]);
                        }
                    }
                }
            }
        }
        Rule { input_size, output_size, input, output, p, shifts }
    }

    /// A box turned or mirrored: `from` gives, for a cell of the new box, the cell of the old.
    fn moved<T: Copy>(cells: &[T], old: [usize; 3], new: [usize; 3], from: impl Fn(usize, usize, usize) -> [usize; 3]) -> Vec<T> {
        let mut out = Vec::with_capacity(cells.len());
        for z in 0..new[2] {
            for y in 0..new[1] {
                for x in 0..new[0] {
                    let [ox, oy, oz] = from(x, y, z);
                    out.push(cells[ox + oy * old[0] + oz * old[0] * old[1]]);
                }
            }
        }
        out
    }

    fn both(&self, new: impl Fn([usize; 3]) -> [usize; 3], from: impl Fn([usize; 3], usize, usize, usize) -> [usize; 3]) -> Rule {
        let (i, o) = (self.input_size, self.output_size);
        Rule::new(Rule::moved(&self.input, i, new(i), |x, y, z| from(i, x, y, z)), new(i), Rule::moved(&self.output, o, new(o), |x, y, z| from(o, x, y, z)), new(o), self.shifts.len(), self.p)
    }

    /// A quarter turn about z.
    pub fn z_rotated(&self) -> Rule {
        self.both(|s| [s[1], s[0], s[2]], |s, x, y, z| [s[0] - 1 - y, x, z])
    }

    /// A quarter turn about y.
    pub fn y_rotated(&self) -> Rule {
        self.both(|s| [s[2], s[1], s[0]], |s, x, y, z| [s[0] - 1 - z, y, x])
    }

    /// The mirror image along x.
    pub fn reflected(&self) -> Rule {
        self.both(|s| s, |s, x, y, z| [s[0] - 1 - x, y, z])
    }

    fn same(a: &Rule, b: &Rule) -> bool {
        a.input_size == b.input_size && a.output_size == b.output_size && a.input == b.input && a.output == b.output
    }

    /// The rule and its turned and mirrored copies that `symmetry` allows, each once.
    pub fn symmetries(&self, symmetry: &[bool], flat: bool) -> Vec<Rule> {
        if flat {
            square_symmetries(self.clone(), Rule::z_rotated, Rule::reflected, Rule::same, Some(symmetry))
        } else {
            cube_symmetries(self.clone(), Rule::z_rotated, Rule::y_rotated, Rule::reflected, Rule::same, Some(symmetry))
        }
    }

    /// A pattern as it is written: rows parted by `/`, layers by a space, the top layer first.
    fn parse(text: &str, file: &str, line: usize, limits: &Limits) -> Result<Letters, Fault> {
        let uneven = || Fault::Pattern { file: file.to_string(), line, uneven: true };
        let layers: Vec<Vec<Vec<char>>> = text.split(' ').map(|layer| layer.split('/').map(|row| row.chars().collect()).collect()).collect();
        let size = [layers[0][0].len(), layers[0].len(), layers.len()];
        if size[0] == 0 {
            return Err(uneven());
        }
        within(Thing::Pattern, (size[0] * size[1] * size[2]) as u64, limits.pattern, file, line)?;
        let mut cells = Vec::with_capacity(size[0] * size[1] * size[2]);
        for layer in layers.iter().rev() {
            if layer.len() != size[1] || layer.iter().any(|row| row.len() != size[0]) {
                return Err(uneven());
            }
            cells.extend(layer.iter().flatten());
        }
        Ok(Letters { size, cells })
    }

    /// A pattern kept in a .vox file, its colours named by `legend` in the order they are first met.
    fn resource(name: &str, legend: Option<&str>, folder: &Option<String>, flat: bool, resources: &Resources, file: &str, line: usize, limits: &Limits) -> Result<Letters, Fault> {
        if flat {
            // (a flat grid's patterns are pictures, and the core reads no pictures)
            return Err(Fault::NotImplemented { file: file.to_string(), line, node: "a rule read from a .png picture".into() });
        }
        let legend: Vec<char> = legend.ok_or(Fault::Missing { file: file.to_string(), line, node: "rule".into(), attribute: "legend" })?.chars().collect();
        let mut parts = vec!["rules"];
        parts.extend(folder.as_deref());
        let leaf = format!("{name}.vox");
        parts.push(&leaf);
        let (shown, bytes) = resources.read(&parts, limits.vox)?;
        let vox = super::vox::read(&shown, &bytes, limits)?;
        within(Thing::Pattern, vox.cells.len() as u64, limits.pattern * 2, &shown, line)?;
        let mut seen: Vec<i32> = Vec::new();
        let mut cells = Vec::with_capacity(vox.cells.len());
        for colour in &vox.cells {
            let order = seen.iter().position(|known| known == colour).unwrap_or_else(|| {
                seen.push(*colour);
                seen.len() - 1
            });
            cells.push(*legend.get(order.min(legend.len())).unwrap_or(&' '));
        }
        if seen.len() > legend.len() {
            return Err(Fault::Colours { file: shown, line, figure: seen.len(), limit: legend.len() });
        }
        Ok(Letters { size: vox.size, cells })
    }

    /// The rule an element describes, its input read against `gin` and its output against `gout`.
    pub fn load(element: &Element, gin: &Grid, gout: &Grid, same_grid: bool, resources: &Resources, file: &str, limits: &Limits) -> Result<Rule, Fault> {
        let line = element.line;
        let flat = gin.size[2] == 1;
        let missing = |attribute| Fault::Missing { file: file.to_string(), line, node: element.name.clone(), attribute };
        let from_file = |name: &str| Rule::resource(name, element.get("legend"), &gout.folder, flat, resources, file, line, limits);
        let (input, output) = match element.get("file") {
            None => {
                let input = match (element.get("in"), element.get("fin")) {
                    (Some(text), _) => Rule::parse(text, file, line, limits)?,
                    (None, Some(name)) => from_file(name)?,
                    (None, None) => return Err(missing("in")),
                };
                let output = match (element.get("out"), element.get("fout")) {
                    (Some(text), _) => Rule::parse(text, file, line, limits)?,
                    (None, Some(name)) => from_file(name)?,
                    (None, None) => return Err(missing("out")),
                };
                if same_grid && input.size != output.size {
                    return Err(Fault::Pattern { file: file.to_string(), line, uneven: false });
                }
                (input, output)
            }
            Some(name) => {
                // one box holding the input in its low x half and the output in its high
                let whole = from_file(name)?;
                let [fx, fy, fz] = whole.size;
                if fx % 2 != 0 {
                    return Err(Fault::Pattern { file: file.to_string(), line, uneven: true });
                }
                let half = |shift: usize| Letters { size: [fx / 2, fy, fz], cells: Rule::moved(&whole.cells, whole.size, [fx / 2, fy, fz], |x, y, z| [x + shift, y, z]) };
                (half(0), half(fx / 2))
            }
        };
        let unknown = |symbol: char| Fault::UnknownSymbol { file: file.to_string(), line, symbol };
        let waves: Vec<u32> = input.cells.iter().map(|c| gin.input(*c).ok_or_else(|| unknown(*c))).collect::<Result<_, _>>()?;
        let values: Vec<u8> = output.cells.iter().map(|c| if *c == '*' { Ok(KEEP) } else { gout.value(*c).ok_or_else(|| unknown(*c)) }).collect::<Result<_, _>>()?;
        Ok(Rule::new(waves, input.size, values, output.size, gin.characters.len(), number(element, "p", 1.0, file)?))
    }
}

/// A number an attribute gives, or `usual` where there is no such attribute.
pub fn number<T: std::str::FromStr + Copy>(element: &Element, attribute: &'static str, usual: T, file: &str) -> Result<T, Fault> {
    match element.get(attribute) {
        None => Ok(usual),
        Some(text) => text.trim().parse().map_err(|_| Fault::BadValue { file: file.to_string(), line: element.line, attribute, value: text.to_string() }),
    }
}

/// Yes or no from an attribute, written `True` or `False` as the original's programs write it.
pub fn flag(element: &Element, attribute: &'static str, file: &str) -> Result<bool, Fault> {
    match element.get(attribute).map(|text| text.trim().to_ascii_lowercase()) {
        None => Ok(false),
        Some(text) if text == "true" => Ok(true),
        Some(text) if text == "false" => Ok(false),
        Some(text) => Err(Fault::BadValue { file: file.to_string(), line: element.line, attribute, value: text }),
    }
}

/// Which of a square's eight symmetries a name stands for, or of a cube's 48.
pub fn symmetry(flat: bool, name: Option<&str>, usual: &[bool], file: &str, line: usize) -> Result<Vec<bool>, Fault> {
    let Some(name) = name else { return Ok(usual.to_vec()) };
    let found: Option<Vec<bool>> = if flat {
        let of = |which: [usize; 8]| Some(which.iter().map(|w| *w == 1).collect());
        match name {
            "()" => of([1, 0, 0, 0, 0, 0, 0, 0]),
            "(x)" => of([1, 1, 0, 0, 0, 0, 0, 0]),
            "(y)" => of([1, 0, 0, 0, 0, 1, 0, 0]),
            "(x)(y)" => of([1, 1, 0, 0, 1, 1, 0, 0]),
            "(xy+)" => of([1, 0, 1, 0, 1, 0, 1, 0]),
            "(xy)" => of([1; 8]),
            _ => None,
        }
    } else {
        let of = |keep: fn(usize) -> bool| Some((0..48).map(keep).collect());
        match name {
            "()" => of(|l| l == 0),
            "(x)" => of(|l| l == 0 || l == 1),
            "(z)" => of(|l| l == 0 || l == 17),
            "(xy)" => of(|l| l < 8),
            "(xyz+)" => of(|l| l % 2 == 0),
            "(xyz)" => of(|_| true),
            _ => None,
        }
    };
    found.ok_or(Fault::BadValue { file: file.to_string(), line, attribute: "symmetry", value: name.to_string() })
}

fn distinct<T>(things: Vec<T>, same: impl Fn(&T, &T) -> bool, subgroup: Option<&[bool]>) -> Vec<T> {
    let mut result: Vec<T> = Vec::new();
    for (i, thing) in things.into_iter().enumerate() {
        if subgroup.is_none_or(|s| s.get(i).copied().unwrap_or(false)) && !result.iter().any(|kept| same(kept, &thing)) {
            result.push(thing);
        }
    }
    result
}

/// A thing and its seven turned and mirrored copies in a plane, in the original's order, each kept once.
pub fn square_symmetries<T>(thing: T, rotation: impl Fn(&T) -> T, reflection: impl Fn(&T) -> T, same: impl Fn(&T, &T) -> bool, subgroup: Option<&[bool]>) -> Vec<T> {
    let mut things = vec![thing];
    for i in 1..8 {
        let next = if i % 2 == 1 { reflection(&things[i - 1]) } else { rotation(&things[i - 2]) };
        things.push(next);
    }
    distinct(things, same, subgroup)
}

/// A thing and its 47 turned and mirrored copies in space, in the original's order, each kept once.
pub fn cube_symmetries<T>(thing: T, a: impl Fn(&T) -> T, b: impl Fn(&T) -> T, r: impl Fn(&T) -> T, same: impl Fn(&T, &T) -> bool, subgroup: Option<&[bool]>) -> Vec<T> {
    // For each even place, the place it is made from and by which turn (0 for a, 1 for b);
    // each odd place is the mirror image of the one before it. `SymmetryHelper.CubeSymmetries`.
    const FROM: [(usize, u8); 24] = [(0, 9), (0, 0), (2, 0), (4, 0), (0, 1), (2, 1), (4, 1), (6, 1), (8, 1), (10, 1), (12, 1), (14, 1), (16, 1), (18, 1), (20, 1), (22, 1), (8, 0), (10, 0), (12, 0), (14, 0), (24, 0), (26, 0), (28, 0), (30, 0)];
    let mut things: Vec<T> = Vec::with_capacity(48);
    things.push(thing);
    for i in 1..48 {
        let next = if i % 2 == 1 {
            r(&things[i - 1])
        } else {
            let (from, turn) = FROM[i / 2];
            if turn == 0 { a(&things[from]) } else { b(&things[from]) }
        };
        things.push(next);
    }
    distinct(things, same, subgroup)
}
