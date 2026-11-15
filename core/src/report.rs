//! A form and its fit written out as text, for a person or a program to read.
//!
//! A report has the spec in a form that [`parse_spec`] reads back, so the
//! same form can be rebuilt anywhere from the report alone; the bricks; and a
//! cell-by-cell comparison of the bricks against the form itself, measured
//! here independently of the fitters: where material is missing, where there
//! is extra, and for the worst cells what single shape would have done better.
//!
//! Plain text rather than JSON: it is read by eye as often as by code, the
//! maps are pictures, and it needs no parser beyond `split`.

use crate::curves::{ArchType, ARCH_TYPES};
use crate::fit_volume::{patterns, K};
use crate::shapes::{inside_sampled, shape_info, CellBrick, Kind, CUBE};
use crate::spec::{build, plane_axes, Built, Form, Probe, Spec};
use glam::DVec3;
use std::fmt::Write;

const SAMPLES: u32 = (K * K * K) as u32;

fn form_name(form: Form) -> String {
    match form {
        Form::Arc => "arc".into(),
        Form::Circle => "circle".into(),
        Form::Arch(kind) => format!("arch {}", format!("{kind:?}").to_lowercase()),
        Form::Spline => "spline".into(),
        Form::Bezier => "bezier".into(),
        Form::Sphere => "sphere".into(),
        Form::Torus => "torus".into(),
        Form::Revolve => "revolve".into(),
        Form::Pyramid => "pyramid".into(),
        Form::Surface => "surface".into(),
    }
}

fn number(v: f64) -> String {
    if v == f64::INFINITY {
        "inf".into()
    } else if v == f64::NEG_INFINITY {
        "-inf".into()
    } else {
        format!("{v}")
    }
}

/// The spec as `key = value` lines.
pub fn spec_text(spec: &Spec) -> String {
    let vec3 = |p: &DVec3| format!("{},{},{}", number(p.x), number(p.y), number(p.z));
    let list = |items: Vec<String>| items.join("; ");
    let mut s = String::new();
    let mut line = |key: &str, value: String| writeln!(s, "{key} = {value}").unwrap();
    line("form", form_name(spec.form));
    line("points", list(spec.points.iter().map(vec3).collect()));
    line(
        "weights",
        list(spec.weights.iter().map(|w| number(*w)).collect()),
    );
    line("radius", number(spec.radius));
    line("stretch", vec3(&spec.stretch));
    line("rise", spec.rise.map_or("natural".into(), number));
    line("thick", number(spec.thick));
    line("depth", spec.depth.to_string());
    line("fill", spec.fill.to_string());
    line("closed", spec.closed.to_string());
    line("tube", spec.tube.to_string());
    line("protrude", spec.protrude.to_string());
    line("plane", spec.plane.to_string());
    line(
        "cut",
        spec.cut.map_or("none".into(), |(lo, hi)| {
            format!("{},{}", number(lo), number(hi))
        }),
    );
    line("wedges", spec.wedges.to_string());
    line("flip", spec.flip.map(|f| f.to_string()).join(","));
    line("symmetric", spec.symmetric.to_string());
    // each pull as where it takes hold, then how far it moves
    line("pulls", list(spec.pulls.iter().map(|(at, by)| format!("{} > {}", vec3(at), vec3(by))).collect()));
    line("pull_reach", number(spec.pull_reach));
    line("simplify", number(spec.simplify));
    line("grain", spec.grain.to_string());
    line("budget", spec.budget.to_string());
    line("max_ext", spec.max_ext.to_string());
    s
}

/// Read a spec from `key = value` lines: the output of [`spec_text`], or a
/// whole report, of which only the `[spec]` section is read. Keys left out
/// keep their defaults, and lines starting with `#` are ignored.
pub fn parse_spec(text: &str) -> Result<Spec, String> {
    let mut spec = Spec::default();
    let mut section = "spec";
    for (n, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[') {
            section = name.trim_end_matches(']');
            continue;
        }
        if section != "spec" {
            continue;
        }
        let bad = |what: &str| format!("line {}: {what}: `{line}`", n + 1);
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| bad("expected `key = value`"))?;
        let (key, value) = (key.trim(), value.trim());
        let num = |v: &str| -> Result<f64, String> {
            match v.trim() {
                "inf" => Ok(f64::INFINITY),
                "-inf" => Ok(f64::NEG_INFINITY),
                v => v.parse().map_err(|_| bad("not a number")),
            }
        };
        let vec3 = |v: &str| -> Result<DVec3, String> {
            let n: Vec<f64> = v.split(',').map(num).collect::<Result<_, _>>()?;
            if n.len() != 3 {
                return Err(bad("a point needs three numbers"));
            }
            Ok(DVec3::new(n[0], n[1], n[2]))
        };
        let items = |v: &'_ str| {
            v.split(';')
                .map(str::trim)
                .filter(|i| !i.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        };
        let flag = |v: &str| v.parse::<bool>().map_err(|_| bad("expected true or false"));
        let whole = |v: &str| v.parse::<i64>().map_err(|_| bad("expected a whole number"));
        match key {
            "form" => {
                let mut words = value.split_whitespace();
                spec.form = match (words.next().unwrap_or(""), words.next()) {
                    ("arc", _) => Form::Arc,
                    ("circle", _) => Form::Circle,
                    ("spline", _) => Form::Spline,
                    ("bezier", _) => Form::Bezier,
                    ("sphere", _) => Form::Sphere,
                    ("torus", _) => Form::Torus,
                    ("revolve", _) => Form::Revolve,
                    ("pyramid", _) => Form::Pyramid,
                    ("surface", _) => Form::Surface,
                    ("arch", kind) => {
                        let kind = kind.unwrap_or("round");
                        let found: Option<ArchType> = ARCH_TYPES
                            .into_iter()
                            .find(|k| format!("{k:?}").to_lowercase() == kind);
                        Form::Arch(found.ok_or_else(|| bad("unknown arch type"))?)
                    }
                    _ => return Err(bad("unknown form")),
                };
            }
            "points" => {
                spec.points = items(value)
                    .iter()
                    .map(|p| vec3(p))
                    .collect::<Result<_, _>>()?
            }
            "weights" => {
                spec.weights = items(value)
                    .iter()
                    .map(|w| num(w))
                    .collect::<Result<_, _>>()?
            }
            "radius" => spec.radius = num(value)?,
            "stretch" => spec.stretch = vec3(value)?,
            "rise" => {
                spec.rise = if value == "natural" {
                    None
                } else {
                    Some(num(value)?)
                }
            }
            "thick" => spec.thick = num(value)?,
            "depth" => spec.depth = whole(value)? as i32,
            "fill" => spec.fill = flag(value)?,
            "closed" => spec.closed = flag(value)?,
            "tube" => spec.tube = flag(value)?,
            "protrude" => spec.protrude = flag(value)?,
            "plane" => spec.plane = whole(value)?.clamp(0, 2) as usize,
            "cut" => {
                spec.cut = if value == "none" {
                    None
                } else {
                    let (lo, hi) = value
                        .split_once(',')
                        .ok_or_else(|| bad("expected `low,high` or `none`"))?;
                    Some((num(lo)?, num(hi)?))
                }
            }
            "wedges" => spec.wedges = flag(value)?,
            "symmetric" => spec.symmetric = flag(value)?,
            "pulls" => {
                spec.pulls = items(value)
                    .iter()
                    .map(|pull| {
                        let (at, by) = pull.split_once('>').ok_or_else(|| bad("expected a point, then >, then how far it moves"))?;
                        Ok((vec3(at.trim())?, vec3(by.trim())?))
                    })
                    .collect::<Result<_, String>>()?
            }
            "pull_reach" => spec.pull_reach = num(value)?,
            "flip" => {
                let flags: Vec<bool> = value.split(',').map(|f| flag(f.trim())).collect::<Result<_, _>>()?;
                spec.flip = <[bool; 3]>::try_from(flags).map_err(|_| bad("expected three of true or false"))?;
            }
            "simplify" => spec.simplify = num(value)?,
            "grain" => spec.grain = whole(value)? as i32,
            "budget" => spec.budget = whole(value)?.max(0) as usize,
            "max_ext" => spec.max_ext = whole(value)? as i32,
            _ => return Err(bad("unknown key")),
        }
    }
    Ok(spec)
}

/// One cell where the bricks and the form disagree.
#[derive(Clone, Copy, Debug)]
pub struct CellNote {
    pub at: [i32; 3],
    /// how many of the cell's 64 samples are inside the form
    pub form: u32,
    /// how many are inside a brick
    pub bricks: u32,
    /// how many are in one and not the other
    pub wrong: u32,
    /// the brick covering the cell, as an index into `Built::bricks`
    pub holds: Option<usize>,
    /// the single unit shape that fits this cell best (0 for empty), and how
    /// many samples it would still get wrong
    pub best: u8,
    pub best_wrong: u32,
    /// Whether another shape would do exactly as well as `best`. That is
    /// nearly always two mirror images, such as the two ramps that could cap
    /// a cell cut by a straight edge; neither is an improvement the fitter
    /// passed over, since it takes the symmetric choice on purpose.
    pub tied: bool,
}

/// The bricks measured against the form.
pub struct Analysis {
    /// volume of the form no brick covers, in cells cubed
    pub missing: f64,
    /// volume of brick outside the form
    pub extra: f64,
    /// volume of the form itself, within the cells looked at
    pub form_volume: f64,
    /// every cell with any disagreement, worst first
    pub cells: Vec<CellNote>,
    /// the box of cells looked at, and each cell's samples in it
    min: [i32; 3],
    dims: [usize; 3],
    form_bits: Vec<u64>,
    brick_bits: Vec<u64>,
}

/// Most cells an analysis will hold samples for (16 bytes each).
const MAX_ANALYSIS_CELLS: usize = 4_000_000;

pub fn analyse(spec: &Spec, built: &Built) -> Result<Analysis, String> {
    analyse_probe(&Probe::new(spec, built.grain)?, built)
}

/// [`analyse`] against a form already in hand.
pub fn analyse_probe(probe: &Probe, built: &Built) -> Result<Analysis, String> {
    scan(probe, built, true, &mut Seen::new())
}

/// Most cells [`measure`] will look at: 4 bytes each.
const MAX_MEASURE_CELLS: usize = 32_000_000;

/// How well bricks fit a form, as the form's volume and the volume where the
/// two disagree, without the cell-by-cell record [`analyse_probe`] keeps.
///
/// That record is 16 bytes and more a cell and is only wanted for a report.
/// Building needs just the two numbers, and needs them for forms far larger
/// than a report would be read for: with the record's limit in the way, a
/// sphere over 78 cells in radius could not be measured, and so was fitted
/// cell by cell after all, as fifty thousand bricks instead of five hundred.
pub(crate) fn measure(probe: &Probe, built: &Built) -> Result<(f64, f64), String> {
    measure_with(probe, built, &mut Seen::new())
}

/// What the form holds of each cell its surface passes through, as found by
/// one measurement and good for the next of the same form.
pub(crate) type Seen = std::collections::HashMap<[i32; 3], (u64, u64)>;

/// [`measure`], for several builds of one form in turn.
///
/// Nearly all the time goes on asking the form about the 64 samples of each
/// cell its surface passes through, and the answers do not depend on the
/// bricks. A turned form is built two ways and both measured; with the
/// answers kept in `seen`, the second measurement asks almost nothing. A
/// lookup in a map for each such cell is nothing beside the 64 questions it
/// saves, and a map holds only the cells at the surface, where an array over
/// the whole box would hold every cell of two boxes that need not coincide.
pub(crate) fn measure_with(probe: &Probe, built: &Built, seen: &mut Seen) -> Result<(f64, f64), String> {
    let found = scan(probe, built, false, seen)?;
    Ok((found.form_volume, found.missing + found.extra))
}

/// The bricks against the form, cell by cell. `keep` says whether to keep
/// what was found in each cell, for a report, or only the totals.
fn scan(probe: &Probe, built: &Built, keep: bool, seen: &mut Seen) -> Result<Analysis, String> {
    if built.bricks.is_empty() {
        return Err("there are no bricks to analyse".into());
    }
    // every brick, and a margin around them for parts of the form left unbuilt
    let pad = built.grain.max(1) + 1;
    let min = [0, 1, 2].map(|i| built.bricks.iter().map(|b| b.min[i]).min().unwrap() - pad);
    let max = [0, 1, 2].map(|i| {
        built
            .bricks
            .iter()
            .map(|b| b.min[i] + b.ext[i])
            .max()
            .unwrap()
            + pad
    });
    let dims = [0, 1, 2].map(|i| (max[i] - min[i]) as usize);
    let count = dims[0] * dims[1] * dims[2];
    let limit = if keep { MAX_ANALYSIS_CELLS } else { MAX_MEASURE_CELLS };
    if count > limit {
        return Err(format!("the build spans {count} cells, too many to analyse cell by cell (the limit is {limit})"));
    }
    let index = |c: [i32; 3]| {
        (c[0] - min[0]) as usize
            + dims[0] * ((c[1] - min[1]) as usize + dims[1] * (c[2] - min[2]) as usize)
    };

    let mut owner = vec![0u32; count];
    let mut shared: std::collections::HashMap<usize, Vec<usize>> = std::collections::HashMap::new();
    for (i, b) in built.bricks.iter().enumerate() {
        for z in 0..b.ext[2] {
            for y in 0..b.ext[1] {
                for x in 0..b.ext[0] {
                    let cell = index([b.min[0] + x, b.min[1] + y, b.min[2] + z]);
                    if owner[cell] == 0 {
                        owner[cell] = i as u32 + 1;
                    } else {
                        // Two bricks in one box: half bricks either side of
                        // a diagonal, as a surface is built of. Few cells
                        // have a second, so they are kept apart from the
                        // one number every cell has.
                        shared.entry(cell).or_default().push(i);
                    }
                }
            }
        }
    }

    let sample = |s: usize| {
        DVec3::new(
            (s % K) as f64 + 0.5,
            (s / K % K) as f64 + 0.5,
            (s / (K * K)) as f64 + 0.5,
        ) / K as f64
    };
    let kept = if keep { count } else { 0 };
    let (mut form_bits, mut brick_bits) = (vec![0u64; kept], vec![0u64; kept]);
    let mut inside = 0u64;
    let (mut missing, mut extra) = (0u64, 0u64);
    let mut cells = Vec::new();
    // The form is asked about once for each block of cells before any cell
    // in it: where the block's middle is further from the surface than its
    // own corners are, every cell in it is wholly in or wholly out, and
    // none of them is asked about at all. Most of a solid's box is such
    // blocks; asking about every cell's middle was four fifths of the time
    // a large sphere took to build, the fitting itself a fiftieth.
    const BLOCK: i32 = 8;
    let blocks = [0, 1, 2].map(|k| (dims[k] as i32 + BLOCK - 1) / BLOCK);
    let mut far = vec![0i8; (blocks[0] * blocks[1] * blocks[2]) as usize];
    for bz in 0..blocks[2] {
        for by in 0..blocks[1] {
            for bx in 0..blocks[0] {
                let middle = DVec3::new((min[0] + bx * BLOCK) as f64, (min[1] + by * BLOCK) as f64, (min[2] + bz * BLOCK) as f64) + BLOCK as f64 / 2.0;
                let d = probe.distance(middle);
                // half the block's diagonal, and a cell's own half diagonal beyond it
                let reach = BLOCK as f64 * 0.867 + 0.87;
                far[(bx + blocks[0] * (by + blocks[1] * bz)) as usize] = if d > reach { 1 } else if d < -reach { -1 } else { 0 };
            }
        }
    }
    for z in min[2]..max[2] {
        for y in min[1]..max[1] {
            for x in min[0]..max[0] {
                let at = [x, y, z];
                let i = index(at);
                let corner = DVec3::new(x as f64, y as f64, z as f64);
                // a cell whose centre is further from the surface than its
                // corners are from its centre is wholly in or out
                let block = far[((x - min[0]) / BLOCK + blocks[0] * ((y - min[1]) / BLOCK + blocks[1] * ((z - min[2]) / BLOCK))) as usize];
                let centre = match block {
                    1 => f64::INFINITY,
                    -1 => f64::NEG_INFINITY,
                    _ => probe.distance(corner + 0.5),
                };
                // `edge` marks the samples that lie on the form's surface itself
                let (form, edge) = if centre > 0.87 {
                    (0, 0)
                } else if centre < -0.87 {
                    (u64::MAX, 0)
                } else {
                    *seen.entry(at).or_insert_with(|| {
                        (0..K * K * K).fold((0u64, 0u64), |(bits, edge), s| {
                            let d = probe.distance(corner + sample(s));
                            (bits | ((d < 0.0) as u64) << s, edge | ((d.abs() < 1e-9) as u64) << s)
                        })
                    })
                };
                let holds = (owner[i] > 0).then(|| owner[i] as usize - 1);
                let samples_of = |b: CellBrick| -> Result<u64, String> {
                    if b.mask == CUBE {
                        return Ok(u64::MAX);
                    }
                    let info = shape_info(b.mask).ok_or("a brick has a mask that is not a shape")?;
                    Ok((0..K * K * K).fold(0u64, |bits, s| {
                        let p = corner + sample(s);
                        let u = [0, 1, 2].map(|k| (p[k] - b.min[k] as f64) / b.ext[k] as f64);
                        bits | (inside_sampled(info, u[0], u[1], u[2]) as u64) << s
                    }))
                };
                let mut bricks = match holds {
                    None => 0,
                    Some(h) => samples_of(built.bricks[h])?,
                };
                if let Some(others) = shared.get(&i) {
                    for &other in others {
                        bricks |= samples_of(built.bricks[other])?;
                    }
                }
                // A sample exactly on the form's surface is on neither side of
                // it, so it agrees with whatever the bricks do there. Counted
                // as outside, as a strict test has it, every sloped brick
                // lying exactly along a sloped face of the form was charged
                // for the samples on that face, and a pyramid built exactly
                // measured 5% wrong.
                let form = (form & !edge) | (bricks & edge);
                inside += form.count_ones() as u64;
                if keep {
                    form_bits[i] = form;
                    brick_bits[i] = bricks;
                }
                let wrong = (form ^ bricks).count_ones();
                if wrong == 0 {
                    continue;
                }
                missing += (form & !bricks).count_ones() as u64;
                extra += (bricks & !form).count_ones() as u64;
                if !keep {
                    continue;
                }
                let &(best, pattern) = patterns()
                    .iter()
                    .min_by_key(|(_, p)| (form ^ p).count_ones())
                    .unwrap();
                let best_wrong = (form ^ pattern).count_ones();
                let tied = patterns()
                    .iter()
                    .filter(|(_, p)| (form ^ p).count_ones() == best_wrong)
                    .count()
                    > 1;
                cells.push(CellNote {
                    at,
                    form: form.count_ones(),
                    bricks: bricks.count_ones(),
                    wrong,
                    holds,
                    best,
                    best_wrong,
                    tied,
                });
            }
        }
    }
    cells.sort_by_key(|c| (std::cmp::Reverse(c.wrong), c.at));
    let per = SAMPLES as f64;
    let form_volume = inside as f64 / per;
    Ok(Analysis {
        missing: missing as f64 / per,
        extra: extra as f64 / per,
        form_volume,
        cells,
        min,
        dims,
        form_bits,
        brick_bits,
    })
}

fn shape_name(mask: u8) -> String {
    match shape_info(mask) {
        None => "nothing".into(),
        Some(s) => format!("{} {mask:#04x}", format!("{:?}", s.kind).to_lowercase()),
    }
}

impl Analysis {
    /// Cells that hold nothing, or a single unit brick, where one particular
    /// unit shape would get at least an eighth of the cell more right: places
    /// a brick could simply be added or swapped.
    pub fn improvable<'a>(&'a self, built: &'a Built) -> impl Iterator<Item = &'a CellNote> {
        self.cells.iter().filter(|c| {
            let unit = c
                .holds
                .is_none_or(|h| built.bricks[h].ext == [built.grain.max(1); 3]);
            unit && !c.tied && c.best_wrong + SAMPLES / 8 <= c.wrong
        })
    }

    /// Three maps of one layer of cells across `axis` at `layer`: the form,
    /// the bricks, and their difference.
    fn maps(&self, axis: usize, layer: i32, out: &mut String) {
        let (ua, va) = plane_axes(axis);
        let (nu, nv) = (self.dims[ua], self.dims[va]);
        let names = ["x", "y", "z"];
        writeln!(
            out,
            "[layer {}={layer}]  {} runs left to right from {}, {} runs bottom to top from {}",
            names[axis], names[ua], self.min[ua], names[va], self.min[va]
        )
        .unwrap();
        let at = |u: usize, v: usize| {
            let mut c = [0usize; 3];
            (c[ua], c[va], c[axis]) = (u, v, (layer - self.min[axis]) as usize);
            c[0] + self.dims[0] * (c[1] + self.dims[1] * c[2])
        };
        let tenths = |n: u32| match (n * 10 + SAMPLES / 2) / SAMPLES {
            0 if n == 0 => '.',
            0 => '1',
            10 if n == SAMPLES => '#',
            10 => '9',
            t => char::from_digit(t, 10).unwrap(),
        };
        let level = |n: u32, base: u8| {
            (base + ((n * 10 + SAMPLES / 2) / SAMPLES).clamp(1, 10) as u8 - 1) as char
        };
        let kinds: [(&str, Box<dyn Fn(usize) -> char>); 3] = [
            ("form: how full each cell is, in tenths ('.' empty, '#' full)", Box::new(|i| tenths(self.form_bits[i].count_ones()))),
            ("bricks: the same, for what was built", Box::new(|i| tenths(self.brick_bits[i].count_ones()))),
            (
                "difference: a-j form missing from the bricks, A-J brick outside the form, in tenths of a cell",
                Box::new(|i| {
                    let (miss, over) = ((self.form_bits[i] & !self.brick_bits[i]).count_ones(), (self.brick_bits[i] & !self.form_bits[i]).count_ones());
                    match (miss, over) {
                        (0, 0) => '.',
                        (m, o) if m >= o => level(m, b'a'),
                        (_, o) => level(o, b'A'),
                    }
                }),
            ),
        ];
        for (title, cell) in kinds {
            writeln!(out, "{title}").unwrap();
            for v in (0..nv).rev() {
                let row: String = (0..nu).map(|u| cell(at(u, v))).collect();
                writeln!(out, "  {row}").unwrap();
            }
        }
    }
}

/// For a flat form: its pieces as plain geometry, and a drawing of them.
///
/// The maps of how full each cell is cannot show where one piece ends and
/// the next begins, or which way a slope faces, and those are what a fit
/// that looks wrong is usually wrong about. So each piece of the outline is
/// listed as a rectangle or right triangle with its corners, and drawn with
/// a letter of its own, four characters to a cell across and two up, against
/// the true form.
fn pieces(built: &Built, probe: &Probe, axis: usize, layer: i32, out: &mut String) {
    let (ua, va) = plane_axes(axis);
    let names = ["x", "y", "z"];
    // the pieces in the first layer of cells; a flat form is the same in all
    let flat: Vec<&crate::shapes::CellBrick> = built
        .bricks
        .iter()
        .filter(|b| b.min[axis] <= layer && layer < b.min[axis] + b.ext[axis])
        .collect();
    let letter = |i: usize| (b'a' + (i % 26) as u8) as char;
    writeln!(
        out,
        "[pieces]  each piece of the outline, with its corners as ({}, {})",
        names[ua], names[va]
    )
    .unwrap();
    for (i, b) in flat.iter().enumerate() {
        // the box's corners the piece keeps, in order round it
        let corners: Vec<String> = [(0, 0), (1, 0), (1, 1), (0, 1)]
            .into_iter()
            .filter(|&(cu, cv)| b.mask >> ((cu << ua) | (cv << va)) & 1 == 1)
            .map(|(cu, cv)| {
                format!(
                    "({}, {})",
                    b.min[ua] + cu as i32 * b.ext[ua],
                    b.min[va] + cv as i32 * b.ext[va]
                )
            })
            .collect();
        let kind = if corners.len() == 4 {
            "rectangle"
        } else {
            "triangle"
        };
        writeln!(
            out,
            "{}: {kind} {} by {}: {}",
            letter(i),
            b.ext[ua],
            b.ext[va],
            corners.join(" ")
        )
        .unwrap();
    }

    let lo = [
        flat.iter().map(|b| b.min[ua]).min().unwrap_or(0) - 1,
        flat.iter().map(|b| b.min[va]).min().unwrap_or(0) - 1,
    ];
    let hi = [
        flat.iter()
            .map(|b| b.min[ua] + b.ext[ua])
            .max()
            .unwrap_or(0)
            + 1,
        flat.iter()
            .map(|b| b.min[va] + b.ext[va])
            .max()
            .unwrap_or(0)
            + 1,
    ];
    // four characters to a cell if that fits a line, else two
    let across = if (hi[0] - lo[0]) * 4 <= 200 { 4 } else { 2 };
    if (hi[0] - lo[0]) * across > 240 {
        writeln!(out, "[drawing] too wide to draw").unwrap();
        return;
    }
    writeln!(out, "
[drawing]  {} left to right from {}, {} bottom to top from {}; {across} characters to a cell across, 2 up", names[ua], lo[0], names[va], lo[1]).unwrap();
    writeln!(out, "a letter: that piece, where the form is too. A CAPITAL: that piece, where the form is not. '!': form with no piece. '.': neither.").unwrap();
    for row in (0..(hi[1] - lo[1]) * 2).rev() {
        let v = lo[1] as f64 + (row as f64 + 0.5) / 2.0;
        let line: String = (0..(hi[0] - lo[0]) * across)
            .map(|col| {
                let u = lo[0] as f64 + (col as f64 + 0.5) / across as f64;
                let mut p = DVec3::ZERO;
                (p[ua], p[va], p[axis]) = (u, v, layer as f64 + 0.5);
                let in_form = probe.distance(p) < 0.0;
                let holder = flat.iter().position(|b| {
                    let t = [0, 1, 2].map(|k| (p[k] - b.min[k] as f64) / b.ext[k] as f64);
                    t.iter().all(|c| (0.0..1.0).contains(c))
                        && shape_info(b.mask)
                            .is_some_and(|info| inside_sampled(info, t[0], t[1], t[2]))
                });
                match (holder, in_form) {
                    (Some(i), true) => letter(i),
                    (Some(i), false) => letter(i).to_ascii_uppercase(),
                    (None, true) => '!',
                    (None, false) => '.',
                }
            })
            .collect();
        writeln!(out, "  {line}").unwrap();
    }
    writeln!(out).unwrap();
}

/// Build `spec` and describe the result. See the module documentation.
pub fn report(spec: &Spec) -> Result<String, String> {
    let built = build(spec)?;
    let analysis = analyse(spec, &built)?;
    let mut out = String::new();
    let w = &mut out;
    writeln!(w, "# arcs report 1. Lengths are in cells (microbricks, 2 game units). Cell (x, y, z) spans x..x+1 and so on.").unwrap();
    writeln!(w, "[spec]\n{}", spec_text(spec)).unwrap();

    let count = |kind: Kind| {
        built
            .bricks
            .iter()
            .filter(|b| shape_info(b.mask).is_some_and(|s| s.kind == kind))
            .count()
    };
    writeln!(w, "[fit]").unwrap();
    writeln!(w, "brick size = {}", built.grain).unwrap();
    writeln!(
        w,
        "bricks = {} ({} cubes, {} ramps, {} tetrahedra, {} cut cubes, {} hips, {} inner corners, {} half corners)",
        built.bricks.len(),
        count(Kind::Cube),
        count(Kind::Ramp),
        count(Kind::Tet),
        count(Kind::Cut),
        count(Kind::Hip),
        count(Kind::Inner),
        count(Kind::HalfOuter) + count(Kind::HalfInner) + count(Kind::HalfInnerInv)
    )
    .unwrap();
    writeln!(w, "form volume = {:.2}", built.solid).unwrap();
    writeln!(
        w,
        "fitter's discrepancy = {:.2} ({:.2}%)",
        built.error,
        100.0 * built.error / built.solid
    )
    .unwrap();
    writeln!(
        w,
        "measured here: missing = {:.2}, extra = {:.2}, in {} cells",
        analysis.missing,
        analysis.extra,
        analysis.cells.len()
    )
    .unwrap();
    let improvable: Vec<&CellNote> = analysis.improvable(&built).collect();
    writeln!(
        w,
        "cells a single unit brick would improve = {}",
        improvable.len()
    )
    .unwrap();

    writeln!(
        w,
        "\n[improvable]  x y z | form/64 | bricks/64 | wrong | holds | better: shape (wrong)"
    )
    .unwrap();
    let describe = |c: &CellNote| {
        let holds = c.holds.map_or("nothing".to_string(), |h| {
            format!("brick {h} {}", shape_name(built.bricks[h].mask))
        });
        let tied = if c.tied { ", tied with another" } else { "" };
        format!(
            "{} {} {} | {} | {} | {} | {} | {} ({}{tied})",
            c.at[0],
            c.at[1],
            c.at[2],
            c.form,
            c.bricks,
            c.wrong,
            holds,
            shape_name(c.best),
            c.best_wrong
        )
    };
    for c in improvable.iter().take(60) {
        writeln!(w, "{}", describe(c)).unwrap();
    }
    writeln!(
        w,
        "\n[worst]  the same columns, for the cells furthest out whatever holds them"
    )
    .unwrap();
    for c in analysis.cells.iter().take(30) {
        writeln!(w, "{}", describe(c)).unwrap();
    }

    writeln!(w, "\n[bricks]  index: shape mask | min x y z | size x y z").unwrap();
    for (i, b) in built.bricks.iter().enumerate().take(500) {
        writeln!(
            w,
            "{i}: {} | {} {} {} | {} {} {}",
            shape_name(b.mask),
            b.min[0],
            b.min[1],
            b.min[2],
            b.ext[0],
            b.ext[1],
            b.ext[2]
        )
        .unwrap();
    }
    if built.bricks.len() > 500 {
        writeln!(w, "... and {} more", built.bricks.len() - 500).unwrap();
    }

    // How the outside looks: steps, and how far each face is off the form.
    writeln!(w).unwrap();
    let probe = Probe::new(spec, built.grain)?;
    let survey = crate::surface::survey(&probe, &built)?;
    survey.describe(&built, w);

    // Pictures. A flat form is the same in every layer, so one says it all.
    // A volume is cut through its middle each way, which shows its profile,
    // and at up to eight levels up z, which shows its outlines.
    writeln!(w).unwrap();
    let fits = |axis: usize| {
        let (ua, _) = plane_axes(axis);
        analysis.dims[ua] <= 160
    };
    match probe.flat() {
        Some((axis, w0, _)) if fits(axis) => {
            pieces(&built, &probe, axis, w0, w);
            analysis.maps(axis, w0, w)
        }
        Some(_) => writeln!(w, "[layers] too wide to draw").unwrap(),
        None => {
            let (min, dims) = survey.bounds();
            // through the middle of the cell at the centre: a cut exactly on
            // a lattice plane would run along brick faces and show neither side
            for axis in [1, 0] {
                survey.section(
                    &probe,
                    &built,
                    axis,
                    (min[axis] + dims[axis] as i32 / 2) as f64 + 0.5,
                    w,
                );
            }
            // just above each lattice plane, where a layer's outline is widest
            let levels = dims[2].saturating_sub(2).max(1);
            let step = levels.div_ceil(8);
            for k in (0..levels).step_by(step) {
                survey.section(&probe, &built, 2, (min[2] + 1 + k as i32) as f64 + 0.05, w);
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: f64, y: f64, z: f64) -> DVec3 {
        DVec3::new(x + 0.5, y + 0.5, z + 0.5)
    }

    #[test]
    fn a_spec_survives_being_written_and_read() {
        let specs = [
            Spec::default(),
            Spec {
                form: Form::Arch(ArchType::Catenary),
                points: vec![at(-8.0, 0.0, 0.0), at(8.0, 0.0, 2.5)],
                weights: vec![1.0, 2.5],
                rise: Some(6.5),
                stretch: DVec3::new(1.0, 0.5, 2.0),
                cut: Some((-0.5, f64::INFINITY)),
                fill: true,
                closed: true,
                tube: true,
                protrude: true,
                plane: 0,
                wedges: false,
                flip: [true, false, true],
                symmetric: true,
                pulls: vec![(DVec3::new(1.0, 2.5, -3.0), DVec3::new(0.0, 0.5, 4.0))],
                pull_reach: 6.5,
                simplify: 0.25,
                grain: 0,
                budget: 2500,
                thick: 3.5,
                depth: 7,
                radius: 12.5,
                max_ext: 40,
            },
        ];
        for spec in specs {
            assert_eq!(parse_spec(&spec_text(&spec)).unwrap(), spec);
        }
        for form in [
            Form::Arc,
            Form::Circle,
            Form::Spline,
            Form::Bezier,
            Form::Sphere,
            Form::Torus,
            Form::Revolve,
        ] {
            let spec = Spec {
                form,
                ..Spec::default()
            };
            assert_eq!(parse_spec(&spec_text(&spec)).unwrap().form, form);
        }
        assert!(parse_spec("form = teapot")
            .unwrap_err()
            .contains("unknown form"));
        assert!(parse_spec("radius = wide").unwrap_err().contains("line 1"));
        assert!(parse_spec("colour = red")
            .unwrap_err()
            .contains("unknown key"));
    }

    #[test]
    fn the_spec_can_be_read_back_out_of_a_whole_report() {
        let spec = Spec {
            form: Form::Sphere,
            points: vec![at(0.0, 0.0, 6.0)],
            radius: 6.0,
            fill: true,
            simplify: 0.25,
            ..Spec::default()
        };
        let text = report(&spec).unwrap();
        assert_eq!(parse_spec(&text).unwrap(), spec);
        // a volume has no flat outline to list
        assert!(!text.contains("[pieces]"));
        let arch = Spec {
            form: Form::Arch(ArchType::Round),
            points: vec![at(-4.0, 0.0, 0.0), at(4.0, 0.0, 0.0)],
            thick: 1.0,
            depth: 1,
            ..Spec::default()
        };
        let flat = report(&arch).unwrap();
        for part in [
            "[pieces]",
            "triangle",
            "rectangle",
            "[drawing]",
            "[layer y=",
        ] {
            assert!(flat.contains(part), "no {part} in a flat form's report");
        }
        // every piece listed is drawn, under its own letter
        let listed = flat
            .lines()
            .skip_while(|l| !l.starts_with("[pieces]"))
            .skip(1)
            .take_while(|l| !l.is_empty())
            .count();
        let drawing: String = flat
            .lines()
            .skip_while(|l| !l.starts_with("[drawing]"))
            .skip(2)
            .take_while(|l| !l.is_empty())
            .collect();
        for i in 0..listed.min(26) {
            let letter = (b'a' + i as u8) as char;
            assert!(
                drawing.to_lowercase().contains(letter),
                "piece {letter} is listed but not drawn"
            );
        }
        for part in [
            "[spec]",
            "[fit]",
            "[improvable]",
            "[worst]",
            "[bricks]",
            "[surface]",
            "stepped =",
            "[steps]",
            "[section y=",
            "[section x=",
            "[section z=",
        ] {
            assert!(text.contains(part), "no {part} in the report");
        }
    }

    #[test]
    fn the_analysis_agrees_with_the_fitter_and_finds_a_removed_brick() {
        let spec = Spec {
            form: Form::Sphere,
            points: vec![at(0.0, 0.0, 0.0)],
            radius: 8.0,
            fill: true,
            ..Spec::default()
        };
        let mut built = build(&spec).unwrap();
        let whole = analyse(&spec, &built).unwrap();
        assert!(built.bricks.len() > 100);
        // measured independently, the disagreement is what the fitter said it was
        assert!(
            (whole.missing + whole.extra - built.error).abs() < 0.02 * built.error,
            "{} + {} vs {}",
            whole.missing,
            whole.extra,
            built.error
        );
        // at the closest fit there is nothing left that one unit brick would fix
        assert_eq!(whole.improvable(&built).count(), 0);

        // take away a unit brick that is not a cube: its cell must be
        // reported, with that brick as the cure. Any will do except one in a
        // cell where a second shape would fit exactly as well, which is
        // reported as a tie.
        let unit_ramps: Vec<usize> = (0..built.bricks.len())
            .filter(|&i| built.bricks[i].ext == [1; 3] && built.bricks[i].mask != CUBE)
            .collect();
        assert!(
            unit_ramps.len() > 10,
            "only {} unit pieces to try",
            unit_ramps.len()
        );
        let mut checked = 0;
        for gone in unit_ramps {
            if checked >= 3 {
                break;
            }
            let mut holed_build = built.clone();
            let brick = holed_build.bricks.remove(gone);
            let holed = analyse(&spec, &holed_build).unwrap();
            let note = holed
                .cells
                .iter()
                .find(|c| c.at == brick.min)
                .expect("the emptied cell is not reported at all");
            assert_eq!(note.holds, None);
            assert!(holed.missing > whole.missing);
            if note.tied {
                continue;
            }
            let found: Vec<&CellNote> = holed.improvable(&holed_build).collect();
            assert_eq!(found.len(), 1, "{brick:?}");
            assert_eq!((found[0].at, found[0].best), (brick.min, brick.mask));
            checked += 1;
        }
        assert!(checked >= 3, "only {checked} removals could be checked");
        built.bricks.clear();
    }

    /// A cube on its own costs one brick, the same as any other single shape,
    /// so the brick price must never leave one where a ramp would fit better.
    /// (It once did: ramps were charged and cubes were not.)
    #[test]
    fn no_lone_cube_is_left_where_another_shape_fits_better() {
        use crate::curves::ARCH_TYPES;
        let mut lone = 0;
        for kind in ARCH_TYPES {
            for (rise, thick) in [
                (None, 1.0),
                (Some(6.0), 2.0),
                (Some(10.0), 2.0),
                (Some(12.0), 3.0),
            ] {
                let spec = Spec {
                    form: Form::Arch(kind),
                    points: vec![at(-8.0, 0.0, 0.0), at(8.0, 0.0, 0.0)],
                    rise,
                    thick,
                    depth: 1,
                    simplify: 0.25,
                    ..Spec::default()
                };
                let built = build(&spec).unwrap();
                let analysis = analyse(&spec, &built).unwrap();
                // every cell some cube covers: a unit cube beside another
                // cube is not on its own, only left single by the merge
                let cubes: std::collections::HashSet<[i32; 3]> = built
                    .bricks
                    .iter()
                    .filter(|b| b.mask == CUBE)
                    .flat_map(|b| {
                        (0..b.ext[0]).flat_map(move |x| {
                            (0..b.ext[2]).map(move |z| [b.min[0] + x, b.min[1], b.min[2] + z])
                        })
                    })
                    .collect();
                let beside = |c: [i32; 3]| {
                    [[1, 0], [-1, 0], [0, 1], [0, -1]]
                        .iter()
                        .any(|d| cubes.contains(&[c[0] + d[0], c[1], c[2] + d[1]]))
                };
                for c in analysis.improvable(&built) {
                    let brick = c.holds.map(|h| built.bricks[h]);
                    assert!(brick.is_none_or(|b| b.mask != CUBE) || beside(c.at), "{kind:?} rise {rise:?} thick {thick}: lone cube at {:?} is {}/64 full; {} would leave {} wrong, not {}", c.at, c.form, shape_name(c.best), c.best_wrong, c.wrong);
                }
                lone += built.bricks.iter().filter(|b| b.ext == [1; 3]).count();
            }
        }
        assert!(
            lone > 20,
            "only {lone} unit bricks in all: the test is not looking at much"
        );

        // and in a volume: a sphere, where lone cubes sit all over the surface
        let spec = Spec {
            form: Form::Sphere,
            points: vec![at(0.0, 0.0, 0.0)],
            radius: 9.0,
            fill: true,
            simplify: 0.25,
            ..Spec::default()
        };
        let built = build(&spec).unwrap();
        let analysis = analyse(&spec, &built).unwrap();
        let lumps = analysis
            .improvable(&built)
            .filter(|c| c.holds.is_some_and(|h| built.bricks[h].mask == CUBE))
            .count();
        assert_eq!(
            lumps, 0,
            "lone cubes on the sphere that another shape would fit better"
        );
    }

    #[test]
    fn a_flat_form_is_drawn_as_one_layer() {
        let spec = Spec {
            form: Form::Spline,
            points: vec![at(0.0, 0.0, 0.0), at(5.0, 0.0, 0.0)],
            thick: 1.0,
            depth: 1,
            ..Spec::default()
        };
        let text = report(&spec).unwrap();
        // a 6 x 1 bar: six full cells in a row, in both pictures, and no difference
        assert_eq!(text.matches("  ..######..\n").count(), 2, "{text}");
        assert!(text.contains("measured here: missing = 0.00, extra = 0.00"));
    }
}
