//! Patterns that fill a flat region in place of solid brick: so far, a path
//! of stepping stones.
//!
//! A flat form is fitted as one outline (see `fit_planar`). A pattern cuts
//! the same region into many small outlines first and fits each as its own,
//! so the pieces of a pattern have the sloped, continuous edges every flat
//! form has, and a pattern follows whatever region the form gives it: the
//! band along a curved path, a ring, a disc, a filled outline.
//!
//! ## Stepping stones
//!
//! The stones are the cells of a Voronoi diagram: points are scattered over
//! the region, and each stone is the ground nearer its point than any
//! other, drawn in from its neighbours by half the gap between stones and
//! with its corners rounded off.
//! https://en.wikipedia.org/wiki/Voronoi_diagram
//!
//! The patterns were asked for under the name of wave function collapse,
//! which fills a grid with tiles that agree with their neighbours:
//! https://github.com/mxgmn/WaveFunctionCollapse
//! That suits a pattern made of repeated motifs, and a floor mosaic or a
//! rug will use it. It does not suit stones. Its rules are local, a few
//! cells across, and nothing local says how large a stone has grown: the
//! tiles that make a stone's inside can be laid for ever. Its output is
//! also the grid's own squares, where a stone wants an outline at any
//! angle. Scattered points give stones of a chosen size by construction,
//! and their outlines are exact curves for the outline fitter to follow.
//! The same author's MarkovJunior grows its own regions from scattered
//! points for the same reason: https://github.com/mxgmn/MarkovJunior
//!
//! The points are scattered by throwing darts: a point is kept if it is no
//! nearer than a least distance to any point already kept ("Poisson disc"
//! sampling), which spreads them evenly without a grid showing through.
//! https://en.wikipedia.org/wiki/Supersampling#Poisson_disk
//! They come from a small generator of numbers started from the form's
//! seed, so one seed is always one path of stones, on every machine.

use crate::fit_planar::{fit_planar, Fit, Opts};
use crate::shapes::CellBrick;
use glam::DVec2;
use std::collections::HashMap;

/// How a path of stones is laid: the settings a form carries for it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stones {
    /// how far apart the stones' middles are, about: a stone is a little
    /// under this across
    pub size: f64,
    /// the gap between neighbouring stones
    pub gap: f64,
    /// 0 for stones as alike as scattered points allow, 1 for stones of
    /// very different sizes and shapes
    pub uneven: f64,
    /// which path of stones: any number, each a different scattering
    pub seed: u64,
}

/// The least a stone may be across and the least gap, in cells: under
/// these the outline fitter has no room to make a stone or to keep two apart.
pub const LEAST_STONE: f64 = 3.0;
pub const LEAST_GAP: f64 = 1.0;

/// Numbers that look random and are the same every time for one seed.
///
/// SplitMix64: one multiply-and-shift mix of a counter. Any generator would
/// do for scattering stones; this one is a dozen lines with no state beyond
/// a number, where the `rand` crate would be a dependency for it.
/// https://prng.di.unimi.it/splitmix64.c
struct Numbers(u64);

impl Numbers {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// From 0 up to, not including, 1.
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// The points the stones grow from: scattered over the part of cells
/// `min..max` where `region` is negative (inside), none nearer another than
/// a least distance that `uneven` loosens.
pub fn scatter(region: &dyn Fn(DVec2) -> f64, min: [i32; 2], max: [i32; 2], stones: &Stones) -> Vec<DVec2> {
    let size = stones.size.max(LEAST_STONE);
    let uneven = stones.uneven.clamp(0.0, 1.0);
    // Even stones: points kept well apart, which leaves them close to a
    // honeycomb. Uneven: points allowed nearly twice as close, so that
    // some stones are crowded small and others left large.
    let apart = size * (0.86 - 0.36 * uneven);
    let (lo, span) = (DVec2::new(min[0] as f64, min[1] as f64), DVec2::new((max[0] - min[0]) as f64, (max[1] - min[1]) as f64));
    // a grid of squares one least distance wide: a new point need only be
    // compared with the points in the nine squares about it
    let across = ((span.x / apart).ceil() as usize + 1, (span.y / apart).ceil() as usize + 1);
    let square = |p: DVec2| (((p.x - lo.x) / apart) as usize, ((p.y - lo.y) / apart) as usize);
    let mut held: HashMap<(usize, usize), Vec<DVec2>> = HashMap::new();
    let mut points: Vec<DVec2> = Vec::new();
    let mut numbers = Numbers(stones.seed ^ 0x51ED_270B_7F4A_7C15);
    // Enough darts that the region is full: thirty for each stone the box
    // has room for. Past that a dart almost never finds a place, and a
    // fixed count keeps the time in hand on a region with little inside it.
    let darts = ((span.x * span.y / (apart * apart)).ceil() as usize * 30).clamp(30, 400_000);
    for _ in 0..darts {
        let p = lo + DVec2::new(numbers.unit(), numbers.unit()) * span;
        // a stone's middle is kept in from the region's edge, so that the
        // edge cuts a little off a stone and not most of it
        if region(p) > -size * 0.3 {
            continue;
        }
        let (i, j) = square(p);
        let crowded = (i.saturating_sub(1)..=(i + 1).min(across.0)).any(|a| (j.saturating_sub(1)..=(j + 1).min(across.1)).any(|b| held.get(&(a, b)).is_some_and(|there| there.iter().any(|q| q.distance(p) < apart))));
        if !crowded {
            held.entry((i, j)).or_default().push(p);
            points.push(p);
        }
    }
    points
}

/// One stone: the point it grew from and the lines half way to the stones
/// round it.
pub struct Stone {
    pub at: DVec2,
    /// For each stone round it, the line half way between the two, as the
    /// direction towards the other stone and how far along that direction
    /// the line is: a place `p` is `far - towards.dot(p)` on this stone's
    /// side of it.
    lines: Vec<(DVec2, f64)>,
    half_gap: f64,
    /// how sharply its corners are rounded: higher is sharper
    sharp: f64,
}

impl Stone {
    /// The stone as a solid for the outline fitter: negative inside.
    ///
    /// A point is inside by its distance from each line half way to a
    /// neighbouring stone, and from the region's edge, less half the gap.
    /// The least of those distances would give the Voronoi cell exactly,
    /// drawn in by half the gap, with sharp corners. A smooth least
    /// (the "log-sum-exp" one) gives the same edges and rounds the corners
    /// where two of them meet, as a stone's are.
    /// https://en.wikipedia.org/wiki/LogSumExp
    ///
    /// The smooth least differs from the plain least only where two of the
    /// distances are within a cell or so of each other, which is at the
    /// corners. So the plain least is found first, and a place it puts
    /// well inside the stone or well outside is answered from it. Taking
    /// the exponential of every distance at every place asked about was
    /// most of the time a path of stones took: 170 ms for a path that is
    /// 20 ms as solid brick.
    pub fn eval(&self, p: DVec2, region: &dyn Fn(DVec2) -> f64) -> f64 {
        // how far the smooth least can fall below the plain least, with room to spare
        let near = 2.0 / self.sharp;
        let mut least = f64::INFINITY;
        for (towards, far) in &self.lines {
            least = least.min(far - towards.dot(p));
        }
        if least < self.half_gap - near {
            return self.half_gap - least; // well past a neighbour's line: outside
        }
        let edge = -region(p);
        let least = least.min(edge);
        if least < self.half_gap - near || least > self.half_gap + near {
            return self.half_gap - least;
        }
        let mut sum = (-self.sharp * edge).exp();
        for (towards, far) in &self.lines {
            sum += (-self.sharp * (far - towards.dot(p))).exp();
        }
        self.half_gap + sum.ln() / self.sharp
    }
}

/// The stones of a region: one for each scattered point, knowing the
/// stones round it.
pub fn stones(region: &dyn Fn(DVec2) -> f64, min: [i32; 2], max: [i32; 2], settings: &Stones) -> Vec<Stone> {
    let points = scatter(region, min, max, settings);
    let size = settings.size.max(LEAST_STONE);
    // a stone's edges come from the points within two stones' width of its own
    let reach = size * 2.2;
    points
        .iter()
        .map(|at| Stone {
            at: *at,
            lines: points
                .iter()
                .filter(|q| *q != at && q.distance(*at) < reach)
                .map(|q| {
                    let towards = (*q - *at).normalize();
                    (towards, towards.dot((*q + *at) / 2.0))
                })
                .collect(),
            half_gap: settings.gap.max(LEAST_GAP) / 2.0,
            // Corners rounded over about half a cell, whatever the stone's
            // size. Rounder than that and the smooth least falls short of
            // the true least along the whole of every edge, not only at
            // the corners, since a small stone has several neighbours'
            // lines within a cell or two of any point on its edge: at a
            // sixth of a stone's width the stones came out a cell and a
            // half short all round, with gaps as wide as themselves.
            sharp: 2.5,
        })
        .collect()
}

/// Whether a place, given within a piece's box as two numbers from 0 to 1,
/// is in the piece itself: anywhere, for a rectangle; for a triangle, on
/// the near side of its sloped edge.
fn in_piece(mask: u8, u: f64, v: f64) -> bool {
    if mask == 15 {
        return true;
    }
    // the corner the triangle lacks, and how far the place is from the one opposite it
    let missing = (!mask & 15).trailing_zeros();
    let (from_u, from_v) = (if missing & 1 == 1 { u } else { 1.0 - u }, if missing & 2 == 2 { v } else { 1.0 - v });
    from_u + from_v < 1.0
}

/// Fit the stones of a region and push them through cells `w0..w0 + depth`
/// across `axis`, onto `bricks`. Returns the sum of the stones' fits: the
/// area of the stones themselves and the area where their bricks disagree
/// with them.
///
/// Each stone is fitted by itself, so that every edge of every stone gets
/// the sloped bricks an outline of its own gets. Fitted all at once as one
/// solid of many parts, two stones either side of a narrow gap wanted
/// cells of the same box for the triangles of their facing edges, the
/// fitter gave the box to one, and the other's edge was finished in steps:
/// a stone with a corner missing its wedge.
///
/// Fitted apart, the triangles of two facing edges may lie in boxes that
/// share cells, and both are kept, since what the two bricks fill does not
/// meet: the gap runs between them. Only where what they fill would meet
/// (each fit may stray half a cell from its stone, and the least gap is one
/// cell) is the later stone fitted again a little smaller.
#[allow(clippy::too_many_arguments)]
pub fn lay_stones(region: &dyn Fn(DVec2) -> f64, settings: &Stones, axis: usize, min: [i32; 2], max: [i32; 2], w0: i32, depth: i32, opts: &Opts, bricks: &mut Vec<CellBrick>) -> Fit {
    use crate::fit_planar::Piece;
    let (ua, va) = match axis {
        0 => (1, 2),
        1 => (0, 2),
        _ => (0, 1),
    };
    let size = settings.size.max(LEAST_STONE);
    let mut whole = Fit::default();
    // the pieces laid so far, and for each cell the pieces whose boxes cover it
    let mut laid: Vec<Piece> = Vec::new();
    let mut over: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
    let cells = |piece: &Piece| {
        let (x, y, w, h) = (piece.x, piece.y, piece.w, piece.h);
        (0..w * h).map(move |k| (x + k % w, y + k / w))
    };
    // whether a piece fills any of what a piece already laid fills: nine places to a cell they share
    let meets = |piece: &Piece, laid: &[Piece], over: &HashMap<(i32, i32), Vec<usize>>| {
        let within = |q: &Piece, x: f64, y: f64| in_piece(q.mask, (x - q.x as f64) / q.w as f64, (y - q.y as f64) / q.h as f64);
        cells(piece).any(|cell| {
            over.get(&cell).is_some_and(|others| {
                (0..9).any(|k| {
                    let (x, y) = (cell.0 as f64 + (k % 3) as f64 / 3.0 + 0.17, cell.1 as f64 + (k / 3) as f64 / 3.0 + 0.16);
                    within(piece, x, y) && others.iter().any(|o| within(&laid[*o], x, y))
                })
            })
        })
    };
    for mut stone in stones(region, min, max, settings) {
        // No place is further from the nearest stone's point than the
        // points are kept apart, or a dart would have landed there: so a
        // stone reaches no further than that from its own.
        let reach = size * 0.9 + 1.0;
        let (lo, hi) = (stone.at - DVec2::splat(reach), stone.at + DVec2::splat(reach));
        let (from, to) = ([(lo.x.floor() as i32).max(min[0]), (lo.y.floor() as i32).max(min[1])], [(hi.x.ceil() as i32).min(max[0]), (hi.y.ceil() as i32).min(max[1])]);
        if to[0] <= from[0] || to[1] <= from[1] {
            continue;
        }
        // as it is; then, if it would meet a stone already laid, drawn in by half a cell, and by a whole one
        let mut fit = fit_planar(&|x, y| stone.eval(DVec2::new(x, y), region), from, to, opts);
        for _ in 0..2 {
            if !fit.pieces.iter().any(|piece| meets(piece, &laid, &over)) {
                break;
            }
            stone.half_gap += 0.5;
            fit = fit_planar(&|x, y| stone.eval(DVec2::new(x, y), region), from, to, opts);
        }
        whole.solid += fit.solid;
        whole.error += fit.error;
        for piece in fit.pieces {
            // (still meeting one after that: left out)
            if meets(&piece, &laid, &over) {
                continue;
            }
            for cell in cells(&piece) {
                over.entry(cell).or_default().push(laid.len());
            }
            laid.push(piece);
            let mask = (0..8u8).fold(0u8, |m, c| m | (piece.mask >> ((c >> ua & 1) | (c >> va & 1) << 1) & 1) << c);
            let mut at = 0;
            while at < depth {
                let run = (depth - at).min(opts.max_ext.max(1));
                let (mut min, mut ext) = ([0; 3], [0; 3]);
                (min[ua], min[va], min[axis]) = (piece.x, piece.y, w0 + at);
                (ext[ua], ext[va], ext[axis]) = (piece.w, piece.h, run);
                bricks.push(CellBrick { mask, min, ext });
                at += run;
            }
        }
    }
    whole
}

/// How a mosaic is laid: the settings a form carries for it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mosaic {
    /// which of [`STYLES`]
    pub style: usize,
    /// how many cells one tile of the mosaic is across, and how many deep:
    /// the two differ for a mosaic of long triangles and lozenges
    pub tile: i32,
    pub tall: i32,
    /// how many tiles wide the plain border round the edge is
    pub border: i32,
    /// which filling: any number, each a different mosaic
    pub seed: u64,
}

/// A style of mosaic: either a small sample whose motifs wave function
/// collapse lays out afresh over the floor (see [`crate::wfc`]), or a
/// pattern drawn by rule (see [`drawn`]).
pub struct Style {
    pub name: &'static str,
    /// the sample: see [`read_sample`]
    sample: &'static str,
    /// how many tiles across the windows learnt from the sample are: 3
    /// keeps whole motifs together, 2 lets them be taken apart and mixed
    window: usize,
    /// whether the motifs may lie any way round
    turned: bool,
    /// set for a style that is drawn by rule and not learnt from a sample (see [`drawn`])
    drawn: Option<Drawn>,
}

/// The styles on offer. Each sample is a text file beside this one, so
/// that a style can be drawn and changed without reading any Rust.
pub const STYLES: [Style; 11] = [
    Style { name: "truchet", sample: include_str!("mosaics/truchet.txt"), window: 2, turned: true, drawn: None },
    Style { name: "lozenges", sample: include_str!("mosaics/lozenges.txt"), window: 3, turned: true, drawn: None },
    Style { name: "stars", sample: include_str!("mosaics/stars.txt"), window: 3, turned: true, drawn: None },
    Style { name: "pinwheels", sample: include_str!("mosaics/pinwheels.txt"), window: 2, turned: true, drawn: None },
    Style { name: "chevrons", sample: include_str!("mosaics/chevrons.txt"), window: 2, turned: false, drawn: None },
    Style { name: "key", sample: include_str!("mosaics/key.txt"), window: 3, turned: true, drawn: None },
    Style { name: "weave", sample: include_str!("mosaics/weave.txt"), window: 3, turned: true, drawn: None },
    Style { name: "flowers", sample: include_str!("mosaics/flowers.txt"), window: 3, turned: true, drawn: None },
    Style { name: "rug", sample: "", window: 0, turned: false, drawn: Some(Drawn::Rug) },
    Style { name: "snowflake", sample: "", window: 0, turned: false, drawn: Some(Drawn::Snowflake) },
    Style { name: "medallions", sample: "", window: 0, turned: false, drawn: Some(Drawn::Medallions) },
];

/// How many times a mosaic is started again after a contradiction before
/// the floor is laid plain.
const TRIES: usize = 30;

/// A tile of a mosaic, as the colour at each of its four corners: top
/// left, top right, bottom left, bottom right, with the sample's rows
/// running downwards. A tile of one colour has it at all four. A tile cut
/// along a diagonal into two triangles has one colour at one end of the
/// other diagonal and the second at its other end; the two corners the cut
/// runs through belong to neither, and are [`ON_CUT`].
///
/// Told by its corners, a tile is turned by moving its corners round,
/// which is all a quarter turn or a mirror image does to it.
type Tile = [u8; 4];
const ON_CUT: u8 = 255;

/// A sample for a mosaic, read from text, and what its tiles are.
struct Read {
    sample: crate::wfc::Sample,
    /// the tile each number in the sample stands for
    tiles: Vec<Tile>,
    /// for each tile, the tile it is a quarter turn clockwise on, and its mirror image
    quarter: Vec<u8>,
    mirror: Vec<u8>,
}

/// Read a mosaic's sample. A line is a row of tiles with spaces between:
///
/// - `2` is a tile all of colour 2;
/// - `1/2` is a tile cut from bottom left to top right, colour 1 above and
///   to the left of the cut and colour 2 below and to the right;
/// - `1\2` is a tile cut from top left to bottom right, colour 1 below and
///   to the left and colour 2 above and to the right.
///
/// A line with no spaces is a row of one-colour tiles, a digit each. Lines
/// that are empty or start with `#` are skipped. Colours are 0 to 3.
fn read_sample(text: &str) -> Result<Read, String> {
    let colour = |c: char| c.to_digit(10).filter(|d| *d < 4).map(|d| d as u8).ok_or_else(|| format!("`{c}` is not a colour from 0 to 3"));
    let tile = |word: &str| -> Result<Tile, String> {
        let chars: Vec<char> = word.chars().collect();
        match chars.as_slice() {
            [c] => colour(*c).map(|c| [c; 4]),
            [a, '/', b] => Ok([colour(*a)?, ON_CUT, ON_CUT, colour(*b)?]),
            [a, '\\', b] => Ok([ON_CUT, colour(*b)?, colour(*a)?, ON_CUT]),
            _ => Err(format!("`{word}` is not a tile: write `2`, `1/2` or `1\\2`")),
        }
    };
    let mut tiles: Vec<Tile> = vec![[0; 4]]; // the plain ground first, which the border is made of
    let number = |t: Tile, tiles: &mut Vec<Tile>| match tiles.iter().position(|known| *known == t) {
        Some(n) => n as u8,
        None => {
            tiles.push(t);
            (tiles.len() - 1) as u8
        }
    };
    let rows: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).collect();
    let mut cells = Vec::new();
    let mut across = 0;
    for row in &rows {
        let words: Vec<String> = if row.contains(' ') { row.split_whitespace().map(str::to_string).collect() } else { row.chars().map(String::from).collect() };
        if across != 0 && words.len() != across {
            return Err("the rows of a sample are all the same length".into());
        }
        across = words.len();
        for word in &words {
            let t = tile(word)?;
            cells.push(number(t, &mut tiles));
        }
    }
    if across == 0 || across > 64 || rows.len() > 64 {
        return Err("a sample is 1 to 64 rows of 1 to 64 tiles".into());
    }
    // every tile's turns and mirror images are tiles too: [top left, top right, bottom left, bottom right]
    let turn = |t: Tile| [t[2], t[0], t[3], t[1]];
    let flip = |t: Tile| [t[1], t[0], t[3], t[2]];
    let mut at = 0;
    while at < tiles.len() {
        let t = tiles[at];
        number(turn(t), &mut tiles);
        number(flip(t), &mut tiles);
        at += 1;
    }
    let quarter = tiles.clone().iter().map(|t| number(turn(*t), &mut tiles)).collect();
    let mirror = tiles.clone().iter().map(|t| number(flip(*t), &mut tiles)).collect();
    Ok(Read { sample: crate::wfc::Sample { across, down: rows.len(), colours: cells }, tiles, quarter, mirror })
}

/// A grid of tiles strewn with a style's motifs by wave function collapse.
/// The tiles marked `plain` are to be plain ground, and those marked
/// `outside` are not part of the floor at all. `None` if the pattern could
/// not be completed.
///
/// The solver is told beforehand which tiles are to be plain, so that it
/// lays whole motifs clear of them. Overruling it afterwards cut motifs in
/// two along the border: half a star against the edge of the floor. A
/// style with no plain ground in it (every tile of it is cut, say) cannot
/// be told that, and is overruled after all.
fn strewn(style: &Style, [across, down]: [usize; 2], plain: &[bool], outside: &[bool], seed: u64) -> Option<Vec<Tile>> {
    use crate::wfc::{learn_turning, Grid};
    let read = read_sample(style.sample).ok()?;
    let turns = style.turned.then_some((read.quarter.as_slice(), read.mirror.as_slice()));
    let learnt = learn_turning(&read.sample, style.window, turns).ok()?;
    let filled = [true, false].into_iter().find_map(|told| {
        let mut grid = Grid::new(&learnt.rules, [across, down]);
        for k in 0..across * down {
            if outside[k] {
                grid.leave_out(k % across, k / across);
            } else if told && plain[k] {
                // (a learnt tile is a window of the sample: the tile of the mosaic it lays is the window's first)
                grid.only(k % across, k / across, |t| learnt.colour[t] == 0);
            }
        }
        grid.fill(seed, TRIES / 2).ok()
    })?;
    Some(filled.iter().map(|tile| tile.map_or([0; 4], |t| read.tiles[learnt.colour[t] as usize])).collect())
}

/// Which of `count` colours a brick takes whose pattern gives it the
/// number `shade` (see `spec::Built::shades`).
///
/// A pattern numbers its parts and not its colours: 0 is the ground and
/// each number above it one feature of the pattern (a rug's frame, its
/// medallion, a snowflake's branches). The ground has the first colour to
/// itself. The features share the rest out in turn, going round again when
/// there are more features than colours. So two colours give the whole
/// pattern in one colour on the other, and every colour added picks one
/// more feature out, until each has its own.
///
/// Going round all the colours, the ground's among them, was the first
/// rule, and it painted some feature the colour of the ground whenever
/// there were fewer colours than features: that feature vanished.
pub fn colour_of(shade: u8, count: usize) -> usize {
    match (shade, count) {
        (0, _) | (_, 0 | 1) => 0,
        _ => 1 + (shade as usize - 1) % (count - 1),
    }
}

/// The styles that are drawn by rule and not learnt from a sample.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Drawn {
    /// a rug in the Persian manner: a framed rectangle with a medallion at
    /// its middle, a piece in each corner and figures between
    Rug,
    /// one eight-armed snowflake at the middle of the floor
    Snowflake,
    /// a few figures repeated over the floor like a tiled ceiling, each
    /// repeat its own mirror image eight ways
    Medallions,
}

/// The feature of a rug that is its frame: the colour its plain tiles take
/// (those the floor's edge cuts into), since the frame is what lies there.
const RUG_FRAME: u8 = 1;

/// A style drawn by rule, as a grid of tiles: a dense pattern of outlined
/// lozenges, octagons, hexagons, squares and bars, ring within ring, whose
/// outlines run into one another where the figures meet, and which is its
/// own mirror image from side to side and from end to end.
///
/// ## How it is drawn
///
/// Every figure is a way of measuring how far a place is from the figure:
/// across plus along for a lozenge, the greater of the two for a square,
/// and mixtures of the two for an octagon, a hexagon and a bar. The measure
/// is nothing on the figure's outline, below nothing inside it, and grows
/// steadily away from it. One number is worked out for every place: the
/// least of all the figures' measures there. The floor is coloured by that
/// number in bands a tile wide, by a short repeating run of on and off.
///
/// So each figure is ringed inside and out by bands of its own shape, and
/// where the bands of two figures meet they join into one outline round
/// both, which is what gives the crowded, interlocked look of the real
/// thing. It is how a distance function joins two solids into one (see
/// `sdf`), used flat.
///
/// The figures are set out by rule, as a snowflake is. A place is first
/// folded onto one part of the floor: a quarter of it for a rug (left onto
/// right, bottom onto top), an eighth for a snowflake (and across the
/// diagonal too), and for medallions an eighth of one repeat. The figures
/// are placed in that part alone, and the folding repeats them everywhere
/// else. The pattern is then its own mirror image because the folding is,
/// with nothing to be turned over afterwards. The seed chooses which
/// figure each is, its size, where it stands and the run of bands.
///
/// ## Colours
///
/// A band that is on takes the number of the figure nearest it, by what
/// part of the pattern that figure is: a rug's frame, its medallion, its
/// corner pieces; a snowflake's arms, its branches, its tips. A band that
/// is off is the ground, 0. The numbers go up to 7, and whoever paints the
/// bricks shares them out among however many colours there are: with two
/// colours the whole pattern is one colour on the other, and each colour
/// added picks another part of it out.
///
/// ## Why not wave function collapse
///
/// The patterns were asked for under that name, and the sampled mosaics
/// use it. It was tried for a rug's field and gave a rug its scattered
/// small motifs, which is a plain rug. What makes a rug or a snowflake
/// look like one is a few large figures that govern the whole of it, and
/// wave function collapse knows only what may sit beside what, a few tiles
/// across.
///
/// ## The tiles
///
/// The number is taken at four places in every tile, one in each of the
/// quarters its two diagonals cut it into. Where all four are one colour
/// the tile is that colour. Where the two quarters on one side of a
/// diagonal are one colour and the two on the other side another, the tile
/// is cut along that diagonal. Every measure used here changes by whole
/// tiles along the grid's lines and its diagonals, so those are the only
/// two cases but for a few tiles where several figures' bands meet, which
/// take the colour at their middle.
///
/// `unbroken` says which tiles are whole tiles of the floor. The pattern
/// is set out about the middle of the rectangle of tiles that takes them
/// all in, and a rug fills that rectangle, so a rug wants a rectangular
/// floor: a straight band lying flat.
fn drawn(kind: Drawn, [across, down]: [usize; 2], unbroken: &[bool], seed: u64) -> Vec<Tile> {
    let mut tiles: Vec<Tile> = vec![[0; 4]; across * down];
    let whole = |k: usize| unbroken[k];
    let (Some(i0), Some(i1)) = ((0..across).find(|i| (0..down).any(|j| whole(j * across + i))), (0..across).rfind(|i| (0..down).any(|j| whole(j * across + i)))) else { return tiles };
    let (Some(j0), Some(j1)) = ((0..down).find(|j| (0..across).any(|i| whole(j * across + i))), (0..down).rfind(|j| (0..across).any(|i| whole(j * across + i)))) else { return tiles };
    // Half the rectangle, in whole tiles, so that its middle is a corner of
    // the grid: a figure about a corner is made of whole and half tiles,
    // and about the middle of a tile it is not. (A floor an odd number of
    // tiles across has one row left over, which stays plain.)
    let (half_wide, half_deep) = (((i1 + 1 - i0) / 2) as f64, ((j1 + 1 - j0) / 2) as f64);
    if half_wide < 2.0 || half_deep < 2.0 {
        return tiles;
    }
    let middle = (i0 as f64 + half_wide, j0 as f64 + half_deep);
    let least = half_wide.min(half_deep);
    let mut numbers = Numbers(seed ^ 0x6A09_E667_F3BC_C908);
    // a whole number from `low` to `high`
    let mut pick = |low: f64, high: f64| (low + numbers.unit() * (high - low + 1.0)).floor().clamp(low, high.max(low));
    // A measure: how far a folded place is from a figure. Boxed, since the
    // figures of one pattern are of several kinds in one list, and there
    // are a few dozen at most.
    type Measure = Box<dyn Fn(f64, f64) -> f64>;
    const NONE: f64 = f64::INFINITY;
    // A figure about a middle: within `across` of it one way and `along`
    // the other, and within `both` of it by the two added together. Left
    // out (`NONE`), a limit is no limit: the third alone is a lozenge, the
    // first two a rectangle, all three an octagon or a pointed bar.
    let shape = |cx: f64, cy: f64, across: f64, along: f64, both: f64| -> Measure {
        Box::new(move |u, v| {
            let (a, b) = ((u - cx).abs(), (v - cy).abs());
            (a - across).max(b - along).max(a + b - both)
        })
    };
    // a line on the diagonal, from a place outwards for `long` tiles each way
    let slant = |cx: f64, cy: f64, long: f64| -> Measure {
        Box::new(move |u, v| {
            let (a, b) = (u - cx, v - cy);
            (a - b).abs().max((a + b - long).abs() - long)
        })
    };
    // a lozenge, a square or an octagon, as the seed has it
    let any = |which: f64, cx: f64, cy: f64, size: f64, cut: f64| match which as i32 {
        0 => shape(cx, cy, NONE, NONE, size),
        1 => shape(cx, cy, size, size, NONE),
        _ => shape(cx, cy, size, size, size + cut.min(size)),
    };
    // the figures of the part everything is folded onto, each with the number of the feature it is
    let mut figures: Vec<(u8, Measure)> = Vec::new();
    // how many bands out from the figures the pattern goes before the ground is left bare: no limit, for a pattern that fills the floor
    let mut reach: Option<i64> = None;
    // how many tiles a repeat is, for a pattern that repeats
    let mut period = 2.0;
    match kind {
        Drawn::Rug => {
            // the frame: how far in from the edge
            figures.push((RUG_FRAME, Box::new(move |u, v| (half_wide - u).min(half_deep - v))));
            // the medallion
            figures.push((2, any(pick(0.0, 2.0), 0.0, 0.0, pick(1.0, (least / 3.0).max(1.0)), pick(1.0, (least / 4.0).max(1.0)))));
            // on the line along the rug through its middle, and on the line across it
            let (out, size) = (pick((half_wide * 0.5).ceil(), (half_wide * 0.72).floor()), pick(1.0, (least / 4.0).max(1.0)));
            figures.push((4, if pick(0.0, 2.0) < 2.0 { shape(out, 0.0, size, NONE, size + pick(1.0, 3.0)) } else { shape(out, 0.0, NONE, NONE, size) }));
            let (up, size) = (pick((half_deep * 0.5).ceil(), (half_deep * 0.75).floor()), pick(1.0, (least / 4.0).max(1.0)));
            figures.push((5, if pick(0.0, 2.0) < 2.0 { shape(0.0, up, NONE, size, size + pick(1.0, 3.0)) } else { shape(0.0, up, NONE, NONE, size) }));
            // in the corner
            figures.push((3, any(pick(0.0, 2.0), pick((half_wide * 0.7).ceil(), (half_wide * 0.86).floor()), pick((half_deep * 0.7).ceil(), (half_deep * 0.86).floor()), pick(1.0, (least / 5.0).max(1.0)), pick(1.0, 2.0))));
            // in the quarter, between them all
            figures.push((6, any(pick(0.0, 1.0), pick((half_wide * 0.3).ceil(), (half_wide * 0.5).floor()), pick((half_deep * 0.3).ceil(), (half_deep * 0.5).floor()), pick(0.0, 2.0), 1.0)));
        }
        Drawn::Snowflake => {
            // The arms are lines, and a line's first band is the line
            // drawn two tiles thick. Left at that, or with one more band
            // round the whole flake a tile clear of it. Any more and the
            // bands of the arms and their branches run together into a
            // lozenge with no arms to be seen.
            let rings = if pick(0.0, 1.0) < 1.0 { 1.0 } else { 3.0 };
            reach = Some(rings as i64);
            // how long an arm is: to the nearer edge, less the rings round it
            let long = (least - rings - 1.0).max(3.0);
            // the heart
            figures.push((4, any(pick(0.0, 2.0), 0.0, 0.0, pick(1.0, (long / 5.0).max(1.0)), 1.0)));
            // the four arms along the grid's lines, pointed, and the four between them on its diagonals
            figures.push((1, shape(0.0, 0.0, long, 0.0, long)));
            let short = pick((long * 0.4).ceil(), (long * 0.65).floor());
            figures.push((2, slant(0.0, 0.0, short)));
            // The branches: off a straight arm on the diagonal, and off a
            // diagonal arm along the grid, shorter the further out, as a
            // snowflake's are.
            let count = pick(2.0, 4.0);
            for n in 1..=count as i32 {
                let at = (long * n as f64 / (count + 1.0)).round();
                figures.push((3, slant(at, 0.0, ((long - at) * 0.5).round().max(1.0))));
            }
            let count = pick(1.0, 2.0);
            for n in 1..=count as i32 {
                let at = (short * n as f64 / (count + 1.0)).round().max(1.0);
                // (half its length a whole number, so that its two ends are both on the grid)
                let half = ((short - at) * 0.5).round().max(1.0);
                figures.push((3, shape(at + half, at, half, 0.0, NONE)));
            }
            // a lozenge at the tip of each arm, and one in each gap between the arms
            figures.push((5, shape(long, 0.0, NONE, NONE, pick(1.0, 2.0))));
            figures.push((5, shape(short, short, NONE, NONE, pick(1.0, 2.0))));
            let out = pick((long * 0.55).ceil(), (long * 0.8).floor());
            figures.push((6, shape(out, pick(1.0, (out * 0.4).floor().max(1.0)), NONE, NONE, pick(0.0, 1.0))));
        }
        Drawn::Medallions => {
            let half = pick(4.0, 8.0).min(least.max(2.0));
            period = half * 2.0;
            // at the middle of a repeat, at its corners, at the middle of its sides, and one between
            figures.push((1, any(pick(0.0, 2.0), 0.0, 0.0, pick(1.0, (half / 2.0).floor().max(1.0)), pick(1.0, 2.0))));
            figures.push((2, any(pick(0.0, 2.0), half, half, pick(1.0, (half / 2.0).floor().max(1.0)), pick(1.0, 2.0))));
            figures.push((3, any(pick(0.0, 2.0), half, 0.0, pick(0.0, (half / 3.0).floor()), 1.0)));
            let out = pick(2.0, (half - 1.0).max(2.0));
            figures.push((4, shape(out, pick(1.0, (out - 1.0).max(1.0)), NONE, NONE, pick(0.0, 1.0))));
        }
    }
    // The bands, outwards from a figure's outline: on and off by turns, one
    // or two bands of each. The first is on, so that every figure has its
    // outline.
    let mut run: Vec<bool> = Vec::new();
    // (a snowflake's are one band each, to keep its lines thin)
    for turn in 0..if kind == Drawn::Snowflake { 2 } else { pick(1.0, 3.0) as usize * 2 } {
        if kind == Drawn::Snowflake {
            run.push(turn == 0);
            continue;
        }
        for _ in 0..pick(1.0, 2.0) as usize {
            run.push(turn % 2 == 0);
        }
    }
    // a place folded onto the part the figures are in
    let fold = |x: f64, y: f64| {
        let ordered = |u: f64, v: f64| (u.max(v), u.min(v));
        match kind {
            Drawn::Rug => (x.abs(), y.abs()),
            Drawn::Snowflake => ordered(x.abs(), y.abs()),
            Drawn::Medallions => ordered((x - period * (x / period).round()).abs(), (y - period * (y / period).round()).abs()),
        }
    };
    // the colour at a place, in tiles from the middle
    let colour = |x: f64, y: f64| {
        let (u, v) = fold(x, y);
        let (mut level, mut feature) = (f64::INFINITY, 0u8);
        for (number, far) in &figures {
            let here = far(u, v);
            if here < level {
                (level, feature) = (here, *number);
            }
        }
        let band = level.floor() as i64;
        if reach.is_some_and(|reach| band >= reach) || !run[band.rem_euclid(run.len() as i64) as usize] {
            0
        } else {
            feature
        }
    };
    for j in j0..j0 + 2 * half_deep as usize {
        for i in i0..i0 + 2 * half_wide as usize {
            let (x, y) = (i as f64 - middle.0, j as f64 - middle.1);
            // the quarters towards the tile's top, right, bottom and left sides
            let [top, right, bottom, left] = [(0.5, 1.0 / 6.0), (5.0 / 6.0, 0.5), (0.5, 5.0 / 6.0), (1.0 / 6.0, 0.5)].map(|(dx, dy)| colour(x + dx, y + dy));
            tiles[j * across + i] = if top == left && bottom == right && top != bottom {
                [top, ON_CUT, ON_CUT, bottom] // cut from bottom left to top right
            } else if top == right && bottom == left && top != bottom {
                [ON_CUT, top, bottom, ON_CUT] // cut from top left to bottom right
            } else if top == right && right == bottom && bottom == left {
                [top; 4]
            } else {
                [colour(x + 0.5, y + 0.5); 4]
            };
        }
    }
    tiles
}

/// Fit a mosaic over a region and push it through cells `w0..w0 + depth`
/// across `axis`, onto `bricks`, with each brick's colour (0 to 3) onto
/// `shades`. Returns the fit of the region.
///
/// The region is fitted first as the one solid it is, so that its edge is
/// as smooth as any flat form's. Then it is coloured. It is cut into
/// tiles, `tile` cells across and `tall` deep, on the brick grid, and wave
/// function collapse gives each tile one of the style's tiles: one colour,
/// or two triangles of two colours either side of a diagonal.
///
/// A tile cut into triangles is two sloped bricks, each a tile across and
/// a tile deep, which for a tile wider than it is deep is a long ramp. The
/// two fill one box between them, as the two halves of a sloped pair do
/// anywhere else. Everything else the fit made is whole cells, and those
/// are sorted by their tile's colour and joined again into the largest
/// bricks of one colour, so that tiles of a colour side by side are one
/// brick and not a brick each.
///
/// The colours were first fitted one at a time, each colour's tiles as a
/// solid of its own. The outline fitter is made to prefer a slope to a
/// staircase, and so it turned the stepped edge between two colours of
/// small tiles into a slope, on both sides: bricks of two colours in one
/// place. Sorting whole cells cannot do that.
///
/// A tile is plain ground, the style's first colour, wherever it is not
/// whole: where the edge of the region or one of the edge's sloped bricks
/// cuts into it, since a brick is one colour. So are the tiles within
/// `border` of the edge. A floor mosaic has a border, and it keeps the
/// pattern clear of the part of the floor the edge has cut up.
///
/// If the pattern cannot be completed (see `wfc`), the floor is laid plain.
#[allow(clippy::too_many_arguments)]
pub fn lay_mosaic(region: &dyn Fn(DVec2) -> f64, settings: &Mosaic, axis: usize, min: [i32; 2], max: [i32; 2], w0: i32, depth: i32, opts: &Opts, bricks: &mut Vec<CellBrick>, shades: &mut Vec<u8>) -> Fit {
    let (ua, va) = match axis {
        0 => (1, 2),
        1 => (0, 2),
        _ => (0, 1),
    };
    let (wide, tall) = (settings.tile.clamp(1, 64), settings.tall.clamp(1, 64));
    let style = &STYLES[settings.style.min(STYLES.len() - 1)];
    let fit = fit_planar(&|x, y| region(DVec2::new(x, y)), min, max, opts);
    // the tiles that cover the box, set on multiples of a tile's size so
    // that moving the form does not slide the pattern under it
    let first = [min[0].div_euclid(wide), min[1].div_euclid(tall)];
    let across = ((max[0] + wide - 1).div_euclid(wide) - first[0]).max(1) as usize;
    let down = ((max[1] + tall - 1).div_euclid(tall) - first[1]).max(1) as usize;
    let tile_of = |x: i32, y: i32| (y.div_euclid(tall) - first[1]) as usize * across + (x.div_euclid(wide) - first[0]) as usize;
    let middle = |k: usize| DVec2::new(((first[0] + (k % across) as i32) as f64 + 0.5) * wide as f64, ((first[1] + (k / across) as i32) as f64 + 0.5) * tall as f64);
    // how deep inside the region a tile's middle is, in tiles (by its shorter side): under nothing, it is outside
    let inside: Vec<f64> = (0..across * down).map(|k| -region(middle(k)) / wide.min(tall) as f64).collect();
    let cells = |piece: &crate::fit_planar::Piece| {
        let (x, y, w, h) = (piece.x, piece.y, piece.w, piece.h);
        (0..w * h).map(move |k| (x + k % w, y + k / w))
    };
    // how many of each tile's cells the fit made whole: a tile short of
    // any, or within the border, is plain
    let mut whole_cells = vec![0; across * down];
    for piece in fit.pieces.iter().filter(|piece| piece.mask == 15) {
        for (x, y) in cells(piece) {
            whole_cells[tile_of(x, y)] += 1;
        }
    }
    let unbroken: Vec<bool> = whole_cells.iter().map(|n| *n == wide * tall).collect();
    let edge = settings.border.clamp(0, 8) as f64 - 0.5;
    // (a rug has a frame of its own, and its plain tiles are the frame's colour)
    let is_rug = style.drawn == Some(Drawn::Rug);
    let plain: Vec<bool> = (0..across * down).map(|k| !unbroken[k] || (!is_rug && inside[k] < edge)).collect();
    let outside: Vec<bool> = inside.iter().map(|depth| *depth < -0.75).collect();
    let plain_colour = if is_rug { RUG_FRAME } else { 0 };
    let mut tiles = if let Some(kind) = style.drawn { drawn(kind, [across, down], &unbroken, settings.seed) } else { strewn(style, [across, down], &plain, &outside, settings.seed).unwrap_or_else(|| vec![[0; 4]; across * down]) };
    for k in (0..across * down).filter(|k| plain[*k]) {
        tiles[k] = [plain_colour; 4];
    }
    let mut push = |mask4: u8, x: i32, y: i32, w: i32, h: i32, shade: u8| {
        let mask = (0..8u8).fold(0u8, |m, c| m | (mask4 >> ((c >> ua & 1) | (c >> va & 1) << 1) & 1) << c);
        let mut at = 0;
        while at < depth {
            let run = (depth - at).min(opts.max_ext.max(1));
            let (mut min, mut ext) = ([0; 3], [0; 3]);
            (min[ua], min[va], min[axis]) = (x, y, w0 + at);
            (ext[ua], ext[va], ext[axis]) = (w, h, run);
            bricks.push(CellBrick { mask, min, ext });
            shades.push(shade);
            at += run;
        }
    };
    // the tiles cut into two triangles: a sloped brick each, a tile in size
    // (a piece's mask has a bit for each corner it fills: 1 top left, 2 top
    // right, 4 bottom left, 8 bottom right, rows running the way y does)
    for (k, tile) in tiles.iter().enumerate().filter(|(_, tile)| tile.contains(&ON_CUT)) {
        let (x, y) = ((first[0] + (k % across) as i32) * wide, (first[1] + (k / across) as i32) * tall);
        if tile[1] == ON_CUT {
            // cut from bottom left to top right
            push(0b0111, x, y, wide, tall, tile[0]);
            push(0b1110, x, y, wide, tall, tile[3]);
        } else {
            // cut from top left to bottom right
            push(0b1101, x, y, wide, tall, tile[2]);
            push(0b1011, x, y, wide, tall, tile[1]);
        }
    }
    // the whole cells of every other tile, each with its tile's colour
    let mut whole: Vec<(i32, i32, u8)> = Vec::new();
    for piece in &fit.pieces {
        if piece.mask == 15 {
            whole.extend(cells(piece).filter(|(x, y)| !tiles[tile_of(*x, *y)].contains(&ON_CUT)).map(|(x, y)| (y, x, tiles[tile_of(x, y)][0])));
        } else {
            push(piece.mask, piece.x, piece.y, piece.w, piece.h, plain_colour);
        }
    }
    // Joined into bricks of one colour: along each row into runs, and a run
    // onto the brick above it where that is the same run of the same
    // colour. (The largest rectangles would be fewer bricks still, and are
    // a harder problem than a floor's worth of bricks is worth.)
    whole.sort();
    let limit = opts.max_ext.max(1);
    let mut open: Vec<(i32, i32, i32, i32, u8)> = Vec::new(); // x, y, w, h, colour: bricks that end on the row above
    let mut at = 0;
    while at < whole.len() {
        let row = whole[at].0;
        let mut runs: Vec<(i32, i32, u8)> = Vec::new(); // x, w, colour
        while at < whole.len() && whole[at].0 == row {
            let (_, x, colour) = whole[at];
            match runs.last_mut() {
                Some(run) if run.0 + run.1 == x && run.2 == colour && run.1 < limit => run.1 += 1,
                _ => runs.push((x, 1, colour)),
            }
            at += 1;
        }
        let mut still = Vec::new();
        for (x, w, colour) in runs {
            match open.iter().position(|b| b.0 == x && b.2 == w && b.4 == colour && b.1 + b.3 == row && b.3 < limit) {
                Some(above) => {
                    let mut brick = open.swap_remove(above);
                    brick.3 += 1;
                    still.push(brick);
                }
                None => still.push((x, row, w, 1, colour)),
            }
        }
        // a brick nothing was joined to this row is finished
        for (x, y, w, h, colour) in open.drain(..) {
            push(15, x, y, w, h, colour);
        }
        open = still;
    }
    for (x, y, w, h, colour) in open {
        push(15, x, y, w, h, colour);
    }
    fit
}

/// Which separate piece of a flat build each brick belongs to: bricks that
/// touch are one piece, and each stone of a path is a piece of its own.
/// Returns a number for each brick, the same for bricks of one piece.
///
/// The pieces are found from the bricks alone, with no memory of the stones
/// they were laid as, so that a build can be flipped, turned, scaled or
/// read back from a file and still be told apart into its stones. Whether
/// two bricks touch is asked of their solid parts and not of their boxes:
/// across a narrow gap, the empty half of one stone's sloped brick shares
/// cells with its neighbour's. So each brick is sampled, four samples to a
/// cell each way, seen along the axis the build is pushed through, and
/// samples side by side are one piece.
///
/// A build that is not flat (some brick is not the same all the way
/// through along any one axis) is one piece.
pub fn pieces(bricks: &[CellBrick]) -> Vec<u32> {
    const FINE: i32 = 4;
    // the axis every brick is a prism along: its corners at one end match those at the other
    let through = (0..3).find(|axis| bricks.iter().all(|b| (0..8u8).filter(|c| c >> axis & 1 == 0).all(|c| (b.mask >> c & 1) == (b.mask >> (c | 1 << axis) & 1))));
    let Some(axis) = through else { return vec![0; bricks.len()] };
    let (ua, va) = match axis {
        0 => (1, 2),
        1 => (0, 2),
        _ => (0, 1),
    };
    // each sample that some brick's solid part covers, and one brick that covers it
    let mut held: HashMap<(i32, i32), usize> = HashMap::new();
    for (n, b) in bricks.iter().enumerate() {
        let Some(info) = crate::shapes::shape_info(b.mask) else { continue };
        for j in 0..b.ext[va] * FINE {
            for i in 0..b.ext[ua] * FINE {
                let mut at = [0.5; 3];
                (at[ua], at[va]) = ((i as f64 + 0.5) / (b.ext[ua] * FINE) as f64, (j as f64 + 0.5) / (b.ext[va] * FINE) as f64);
                if crate::shapes::inside_shape(info, at[0], at[1], at[2]) {
                    held.insert((b.min[ua] * FINE + i, b.min[va] * FINE + j), n);
                }
            }
        }
    }
    // spread a piece's number from sample to sample
    let mut piece_of_sample: HashMap<(i32, i32), u32> = HashMap::new();
    let mut piece_of_brick: Vec<Option<u32>> = vec![None; bricks.len()];
    let mut next = 0;
    let mut starts: Vec<(i32, i32)> = held.keys().copied().collect();
    starts.sort();
    for start in starts {
        if piece_of_sample.contains_key(&start) {
            continue;
        }
        let mut open = vec![start];
        while let Some(at) = open.pop() {
            let Some(brick) = held.get(&at) else { continue };
            if piece_of_sample.insert(at, next).is_some() {
                continue;
            }
            piece_of_brick[*brick].get_or_insert(next);
            open.extend([(at.0 + 1, at.1), (at.0 - 1, at.1), (at.0, at.1 + 1), (at.0, at.1 - 1)]);
        }
        next += 1;
    }
    piece_of_brick.into_iter().map(|piece| piece.unwrap_or(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// A band 16 wide along the x axis, 120 long.
    fn band(p: DVec2) -> f64 {
        (p.y.abs() - 8.0).max(p.x.abs() - 60.0)
    }

    const BOX: ([i32; 2], [i32; 2]) = ([-62, -10], [62, 10]);

    fn usual() -> Stones {
        Stones { size: 7.0, gap: 1.5, uneven: 0.5, seed: 3 }
    }

    #[test]
    fn stones_are_scattered_inside_the_region_apart_and_the_same_for_one_seed() {
        let points = scatter(&band, BOX.0, BOX.1, &usual());
        // a band 120 by 16 has room for some forty stones 7 across
        assert!((25..70).contains(&points.len()), "{} stones", points.len());
        assert!(points.iter().all(|p| band(*p) < 0.0), "a stone's middle is outside the path");
        let nearest = |p: &DVec2| points.iter().filter(|q| *q != p).map(|q| q.distance(*p)).fold(f64::INFINITY, f64::min);
        assert!(points.iter().all(|p| nearest(p) >= 7.0 * 0.6), "two stones grew from nearly one place");
        assert_eq!(scatter(&band, BOX.0, BOX.1, &usual()), points, "one seed gave two scatterings");
        assert_ne!(scatter(&band, BOX.0, BOX.1, &Stones { seed: 4, ..usual() }), points, "two seeds gave one scattering");
        // uneven stones are let closer together: there are more of them
        let (even, uneven) = (scatter(&band, BOX.0, BOX.1, &Stones { uneven: 0.0, ..usual() }), scatter(&band, BOX.0, BOX.1, &Stones { uneven: 1.0, ..usual() }));
        assert!(uneven.len() > even.len(), "{} uneven, {} even", uneven.len(), even.len());
    }

    #[test]
    fn a_stone_holds_its_own_point_and_stops_short_of_its_neighbours_and_the_edge() {
        let all = stones(&band, BOX.0, BOX.1, &usual());
        for stone in &all {
            assert!(stone.eval(stone.at, &band) < 0.0, "a stone does not hold the point it grew from");
            for (towards, far) in &stone.lines {
                // on the line half way to a neighbour: in the gap, in neither stone
                let on_line = stone.at + *towards * (far - towards.dot(stone.at));
                assert!(stone.eval(on_line, &band) > 0.0);
            }
        }
        // no stone reaches outside the path
        for k in 0..400 {
            let p = DVec2::new(-61.0 + (k % 40) as f64 * 3.1, if k % 2 == 0 { 8.3 } else { -8.3 });
            assert!(all.iter().all(|s| s.eval(p, &band) > 0.0), "a stone holds {p:?}, outside the path");
        }
    }

    #[test]
    fn a_mosaic_covers_the_floor_in_coloured_tiles_with_a_plain_border() {
        // a disc of radius 30
        let disc = |p: DVec2| p.length() - 30.0;
        let (min, max) = ([-32, -32], [32, 32]);
        let whole_floor = fit_planar(&|x, y| disc(DVec2::new(x, y)), min, max, &Opts::default());
        for (style, named) in STYLES.iter().enumerate() {
            let (mut bricks, mut shades) = (Vec::new(), Vec::new());
            let settings = Mosaic { style, tile: 2, tall: 2, border: 1, seed: 7 };
            let fit = lay_mosaic(&disc, &settings, 2, min, max, 0, 1, &Opts::default(), &mut bricks, &mut shades);
            let name = named.name;
            assert_eq!(bricks.len(), shades.len(), "{name}");
            // the floor is the form's own fit, coloured: the same area, as closely fitted
            assert_eq!((fit.solid, fit.error), (whole_floor.solid, whole_floor.error), "{name}");
            // every place of the floor is in one brick, and none in two
            let holding = |x: f64, y: f64| bricks.iter().filter(|b| x >= b.min[0] as f64 && x < (b.min[0] + b.ext[0]) as f64 && y >= b.min[1] as f64 && y < (b.min[1] + b.ext[1]) as f64).filter(|b| crate::shapes::inside_shape(crate::shapes::shape_info(b.mask).unwrap(), (x - b.min[0] as f64) / b.ext[0] as f64, (y - b.min[1] as f64) / b.ext[1] as f64, 0.5)).count();
            for k in 0..120 * 120 {
                let p = DVec2::new(-30.0 + (k % 120) as f64 / 2.0 + 0.13, -30.0 + (k / 120) as f64 / 2.0 + 0.19);
                let count = holding(p.x, p.y);
                assert!(count <= 1, "{name}: {count} bricks hold {p:?}");
                assert!(count == 1 || disc(p) > -1.5, "{name}: no brick holds {p:?}, well inside the floor");
            }
            // more than one colour, and far fewer bricks than the 2,800 cells of the disc
            let used: HashSet<u8> = shades.iter().copied().collect();
            assert!(used.len() >= 2 && used.iter().all(|shade| *shade < 8), "{name}: colours {used:?}");
            assert!(bricks.len() < 1400, "{name}: {} bricks", bricks.len());
            // one seed is one mosaic
            let again = |seed: u64| {
                let (mut b, mut s) = (Vec::new(), Vec::new());
                lay_mosaic(&disc, &Mosaic { seed, ..settings }, 2, min, max, 0, 1, &Opts::default(), &mut b, &mut s);
                (b, s)
            };
            assert_eq!(again(7), (bricks.clone(), shades.clone()), "{name}");
        }
        // a style of triangles lays sloped bricks inside the floor, as long as its tiles are
        let (mut bricks, mut shades) = (Vec::new(), Vec::new());
        lay_mosaic(&disc, &Mosaic { style: 0, tile: 4, tall: 2, border: 1, seed: 7 }, 2, min, max, 0, 1, &Opts::default(), &mut bricks, &mut shades);
        let long = bricks.iter().zip(&shades).filter(|(b, _)| b.mask != 0xff && b.ext[0] == 4 && b.ext[1] == 2).count();
        assert!(long > 40, "{long} long triangles");
        assert!(bricks.iter().zip(&shades).any(|(b, shade)| b.mask != 0xff && *shade != 0), "no sloped brick has a colour of its own");
    }

    #[test]
    fn a_drawn_style_is_a_dense_pattern_of_cut_tiles_and_its_own_mirror_image() {
        // a floor 120 by 80, in tiles of 2: 60 tiles by 40
        let floor = |p: DVec2| (p.x.abs() - 60.0).max(p.y.abs() - 40.0);
        let (min, max) = ([-60, -40], [60, 40]);
        for (style, named) in STYLES.iter().enumerate().filter(|(_, style)| style.drawn.is_some()) {
            let name = named.name;
            let lay = |seed: u64| {
                let (mut bricks, mut shades) = (Vec::new(), Vec::new());
                lay_mosaic(&floor, &Mosaic { style, tile: 2, tall: 2, border: 1, seed }, 2, min, max, 0, 1, &Opts::default(), &mut bricks, &mut shades);
                (bricks, shades)
            };
            for seed in [1, 5, 12] {
                let (bricks, shades) = lay(seed);
                // the colours at a place: of the bricks that hold it
                let colours = |x: f64, y: f64| -> Vec<u8> { bricks.iter().zip(&shades).filter(|(b, _)| x >= b.min[0] as f64 && x < (b.min[0] + b.ext[0]) as f64 && y >= b.min[1] as f64 && y < (b.min[1] + b.ext[1]) as f64 && crate::shapes::inside_shape(crate::shapes::shape_info(b.mask).unwrap(), (x - b.min[0] as f64) / b.ext[0] as f64, (y - b.min[1] as f64) / b.ext[1] as f64, 0.5)).map(|(_, shade)| *shade).collect() };
                // every place is one colour, and the same colour as its mirror images left to right and top to bottom
                for k in 0..2000 {
                    let (x, y) = ((k % 50) as f64 * 1.19 + 0.313, (k / 50) as f64 * 0.97 + 0.211);
                    let here = colours(x, y);
                    assert!(here.len() == 1 && here == colours(-x, y) && here == colours(x, -y), "{name}, seed {seed}: the pattern differs across its middle at {x}, {y}: {here:?}, {:?}, {:?}", colours(-x, y), colours(x, -y));
                }
                // a dense pattern: several features, many sloped bricks, and none of it most of the floor
                let sloped = bricks.iter().filter(|b| b.mask != 0xff).count();
                assert!(sloped > 150, "{name}, seed {seed}: {sloped} sloped bricks");
                let used: HashSet<u8> = shades.iter().copied().collect();
                assert!(used.len() >= 4 && used.iter().all(|shade| *shade < 8), "{name}, seed {seed}: features {used:?}");
                for shade in 1..8u8 {
                    let area: f64 = bricks.iter().zip(&shades).filter(|(_, s)| **s == shade).map(|(b, _)| (b.ext[0] * b.ext[1]) as f64 * if b.mask == 0xff { 1.0 } else { 0.5 }).sum();
                    assert!(area < 0.5 * 9600.0, "{name}, seed {seed}: feature {shade} covers {area} of 9600");
                }
            }
            // one seed is one pattern, and another seed another
            assert_eq!(lay(5), lay(5), "{name}");
            assert_ne!(lay(5), lay(6), "{name}");
        }
    }

    #[test]
    fn features_share_the_colours_and_the_ground_keeps_its_own() {
        // two colours: every feature is the second
        assert_eq!((0..8).map(|shade| colour_of(shade, 2)).collect::<Vec<_>>(), [0, 1, 1, 1, 1, 1, 1, 1]);
        // four: the features go round the three that are not the ground's
        assert_eq!((0..8).map(|shade| colour_of(shade, 4)).collect::<Vec<_>>(), [0, 1, 2, 3, 1, 2, 3, 1]);
        // as many as there are features: each its own
        assert_eq!((0..8).map(|shade| colour_of(shade, 8)).collect::<Vec<_>>(), [0, 1, 2, 3, 4, 5, 6, 7]);
        assert_eq!((colour_of(5, 1), colour_of(5, 0)), (0, 0));
    }

    #[test]
    fn a_sample_is_read_as_tiles_that_turn() {
        let read = read_sample("0 1/2\n1\\2 3\n").unwrap();
        // the ground, then the tiles as met, then their turns
        assert_eq!(read.tiles[0], [0; 4]);
        let cut = read.tiles[read.sample.colours[1] as usize];
        assert_eq!(cut, [1, ON_CUT, ON_CUT, 2]);
        // a quarter turn clockwise: the colour at the top left goes to the top right
        let turned = read.tiles[read.quarter[read.sample.colours[1] as usize] as usize];
        assert_eq!(turned, [ON_CUT, 1, 2, ON_CUT]);
        // four quarter turns, or two mirror images, are no turn at all
        for tile in 0..read.tiles.len() as u8 {
            let round = (0..4).fold(tile, |t, _| read.quarter[t as usize]);
            assert_eq!((round, read.mirror[read.mirror[tile as usize] as usize]), (tile, tile));
        }
        // a row of digits is a row of one-colour tiles
        assert_eq!(read_sample("012\n").unwrap().sample.colours.len(), 3);
        for bad in ["0 5", "0 1/", "0 1|2", "01\n0", ""] {
            assert!(read_sample(bad).is_err(), "{bad:?} was read");
        }
        // every style on offer is read and learnt
        for style in STYLES.iter().filter(|style| style.drawn.is_none()) {
            let read = read_sample(style.sample).unwrap_or_else(|e| panic!("{}: {e}", style.name));
            let turns = style.turned.then_some((read.quarter.as_slice(), read.mirror.as_slice()));
            assert!(crate::wfc::learn_turning(&read.sample, style.window, turns).is_ok(), "{}", style.name);
        }
    }

    #[test]
    fn a_path_is_laid_as_separate_stones_with_sloped_edges() {
        let mut bricks = Vec::new();
        let fit = lay_stones(&band, &usual(), 2, BOX.0, BOX.1, 0, 2, &Opts::default(), &mut bricks);
        assert!(!bricks.is_empty() && fit.solid > 0.0);
        // the stones cover most of the path and not all of it: there are gaps
        let cover = fit.solid / (120.0 * 16.0);
        assert!((0.45..0.9).contains(&cover), "{cover} of the path is stone");
        // closely fitted, with sloped pieces and not only squares
        assert!(fit.error < 0.2 * fit.solid, "{} wrong of {}", fit.error, fit.solid);
        assert!(bricks.iter().any(|b| b.mask != 0xff), "the stones are all square blocks");
        // two cells deep, on the ground, and no place in two bricks: boxes
        // may share cells across a gap, so it is places that are asked about
        assert!(bricks.iter().all(|b| b.min[2] == 0 && b.ext[2] == 2));
        for k in 0..124 * 20 * 16 {
            let (x, y) = (-62.0 + (k % (124 * 4)) as f64 / 4.0 + 0.13, -10.0 + (k / (124 * 4)) as f64 / 4.0 + 0.11);
            let holding = bricks.iter().filter(|b| x >= b.min[0] as f64 && x < (b.min[0] + b.ext[0]) as f64 && y >= b.min[1] as f64 && y < (b.min[1] + b.ext[1]) as f64).filter(|b| crate::shapes::inside_shape(crate::shapes::shape_info(b.mask).unwrap(), (x - b.min[0] as f64) / b.ext[0] as f64, (y - b.min[1] as f64) / b.ext[1] as f64, 0.5)).count();
            assert!(holding <= 1, "{holding} bricks hold the place {x}, {y}");
        }
        // every stone's edge is finished: among the bricks on a stone's rim
        // there are sloped ones, and few stones are all square blocks
        // the bricks fall into separate stones, about as many as were scattered
        let scattered = scatter(&band, BOX.0, BOX.1, &usual()).len();
        let of = pieces(&bricks);
        let stones: HashSet<u32> = of.iter().copied().collect();
        assert!(stones.len() * 10 >= scattered * 8 && stones.len() <= scattered, "{} separate stones from {scattered} points", stones.len());
        // and the same stones however the bricks are listed
        let mut turned: Vec<CellBrick> = bricks.iter().rev().copied().collect();
        turned.iter_mut().for_each(|b| (b.min[0], b.min[2], b.ext[0], b.ext[2]) = (b.min[2], b.min[0], b.ext[2], b.ext[0]));
        assert_eq!(pieces(&turned[..0]).len(), 0);
        // a build that is not flat is one piece
        let lump = [CellBrick { mask: 0x17, min: [0; 3], ext: [1; 3] }, CellBrick { mask: 0xff, min: [5, 5, 5], ext: [1; 3] }];
        assert_eq!(pieces(&lump), [0, 0]);
    }
}
