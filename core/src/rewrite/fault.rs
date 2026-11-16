//! Everything that can go wrong with a rule program, in one place.
//!
//! The reader, the interpreter and the brick writer all return [`Fault`],
//! and the words a person reads are written once, in its `Display`. The
//! rest of the core reports a problem as a `String` built where it is
//! found (`spec::build` returns `Result<_, String>`), which suits a form
//! whose few refusals are each said in one place. A rule program can fail
//! in thirty ways from four files a stranger wrote, and a string built at
//! each of them would be thirty wordings to keep alike and none that a
//! test could match on. So here a failure is data (which file, which line,
//! the figure and its limit) and its wording is a single `match`. At the
//! boundary with the rest of the core it becomes a `String` through
//! `From<Fault>`, and nowhere before.

use std::fmt;

/// What was too large: a size read from a file or asked for, and held to a limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Thing {
    /// a file, in bytes
    File,
    /// a grid, in cells
    Grid,
    /// the values a grid may hold
    Values,
    /// the rules of one node, counting the turned and mirrored copies
    Rules,
    /// a rule's pattern, in cells
    Pattern,
    /// nodes one inside another
    Depth,
    /// the nodes of a program
    Nodes,
    /// the tiles of a tileset, counting the turned and mirrored copies
    Tiles,
    /// a tile, in cells along a side
    Tile,
    /// what a wfc node must remember: cells times tiles
    Wave,
    /// what a rule node must remember: cells times rules
    Matches,
    /// the bricks of a finished grid
    Bricks,
    /// microbricks to a cell
    Scale,
}

/// What a file's text was expected to hold where it held something else.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Expected {
    Element,
    Name,
    Equals,
    Quote,
    CloseOf,
    EndOfComment,
    EndOfTag,
    OneRoot,
    Text,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Fault {
    /// A file that is not the XML these programs are written in.
    Syntax { file: String, line: usize, expected: Expected },
    /// A size over its limit.
    TooLarge { thing: Thing, file: String, line: usize, figure: u64, limit: u64 },
    /// An element that is not a node of the language, where a node must be.
    UnknownNode { file: String, line: usize, node: String },
    /// A node or an attribute of the language that this interpreter does not run.
    NotImplemented { file: String, line: usize, node: String },
    /// An attribute a node cannot do without.
    Missing { file: String, line: usize, node: String, attribute: &'static str },
    /// An attribute whose value cannot be read.
    BadValue { file: String, line: usize, attribute: &'static str, value: String },
    /// A letter in a rule that the grid has no value or union for.
    UnknownSymbol { file: String, line: usize, symbol: char },
    /// A value or union named twice.
    Repeated { file: String, line: usize, symbol: char },
    /// A rule whose rows are not all one length, or whose two sides differ in size.
    Pattern { file: String, line: usize, uneven: bool },
    /// A tile a tileset or a wfc rule names and does not have.
    UnknownTile { file: String, line: usize, tile: String },
    /// A tileset whose tiles are not the shape they must be.
    TileShape { file: String, size: [usize; 3], cube: bool },
    /// A file in a .vox tile or rule with more colours than there are letters for.
    Colours { file: String, line: usize, figure: usize, limit: usize },
    /// A file a program refers to that is in none of the folders it may be read from.
    NoResource { name: String },
    /// A name that would be read from outside those folders.
    Outside { name: String },
    /// A file that is there and could not be read.
    Unreadable { name: String, why: String },
    /// A .vox file that is not one.
    NotVox { file: String },
    /// A grid with no room for the program: a side of nothing.
    NoGrid { size: [usize; 3] },
    /// A wfc node that found no arrangement of its tiles.
    Contradiction { file: String, line: usize, tries: usize, at_start: bool },
    /// A run that used all the steps it was allowed. What it made is kept.
    StepLimit { steps: usize },
    /// A run stopped for the time it had taken. What it made is kept.
    TimeLimit { steps: usize, seconds: u64 },
}

impl Thing {
    /// The thing in words, the unit its size is counted in, and what to do about it.
    fn words(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Thing::File => ("The file is", "bytes", "it is not a rule program, or it holds something that is not needed"),
            Thing::Grid => ("The grid is", "cells", "make the size smaller"),
            Thing::Values => ("A grid has", "values", "use fewer letters in its values"),
            Thing::Rules => ("A node has", "rules once each is turned and mirrored", "split the node in two, or give it a smaller symmetry"),
            Thing::Pattern => ("A rule's pattern is", "cells", "use a smaller pattern"),
            Thing::Depth => ("Nodes are nested", "deep", "flatten the program"),
            Thing::Nodes => ("The program has", "nodes", "use a shorter program"),
            Thing::Tiles => ("The tileset has", "tiles once each is turned and mirrored", "use fewer tiles"),
            Thing::Tile => ("A tile is", "cells along a side", "use smaller tiles"),
            Thing::Wave => ("A wfc node would have to remember", "cells times tiles", "make the size smaller"),
            Thing::Matches => ("A rule node would have to remember", "cells times rules", "make the size smaller"),
            Thing::Bricks => ("The finished grid comes to", "bricks", "make the size smaller, or choose the plain finish"),
            Thing::Scale => ("A cell is", "microbricks across", "choose a smaller cell"),
        }
    }
}

impl Expected {
    fn words(self) -> &'static str {
        match self {
            Expected::Element => "an element, which starts with <",
            Expected::Name => "a name",
            Expected::Equals => "= after an attribute's name",
            Expected::Quote => "a value in quotation marks",
            Expected::CloseOf => "the end of the element opened last, which is unclosed or closed by another name",
            Expected::EndOfComment => "the end of a comment (-->)",
            Expected::EndOfTag => "> at the end of a tag",
            Expected::OneRoot => "one element holding the whole program",
            Expected::Text => "text in UTF-8",
        }
    }
}

/// `at line 12 of Castle.xml`, or less where less is known.
fn place(file: &str, line: usize) -> String {
    match (file.is_empty(), line) {
        (true, 0) => String::new(),
        (true, n) => format!(" at line {n}"),
        (false, 0) => format!(" in {file}"),
        (false, n) => format!(" at line {n} of {file}"),
    }
}

impl fmt::Display for Fault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Fault::Syntax { file, line, expected } => write!(f, "The program could not be read{}: {} was expected there. Check that line.", place(file, *line), expected.words()),
            Fault::TooLarge { thing, file, line, figure, limit } => {
                let (what, unit, advice) = thing.words();
                write!(f, "{what} {figure} {unit}{} and the most allowed is {limit}: {advice}.", place(file, *line))
            }
            Fault::UnknownNode { file, line, node } => write!(f, "\"{node}\"{} is not a node of the language. A program starts with one of: one, all, prl, markov, sequence, path, map, convolution, convchain, wfc.", place(file, *line)),
            Fault::NotImplemented { file, line, node } => write!(f, "The program uses \"{node}\"{}, which this interpreter does not run yet. It runs one, all, prl, markov, sequence, path, map, convolution, wfc with a tileset, and fields.", place(file, *line)),
            Fault::Missing { file, line, node, attribute } => write!(f, "The \"{node}\" element{} needs a \"{attribute}\" attribute and has none.", place(file, *line)),
            Fault::BadValue { file, line, attribute, value } => write!(f, "The attribute {attribute}=\"{value}\"{} could not be understood. Check that line.", place(file, *line)),
            Fault::UnknownSymbol { file, line, symbol } => write!(f, "The letter \"{symbol}\"{} is neither one of the grid's values nor a union. Add it to the values, or correct the rule.", place(file, *line)),
            Fault::Repeated { file, line, symbol } => write!(f, "The letter \"{symbol}\" is given twice{}. Each value and union needs a letter of its own.", place(file, *line)),
            Fault::Pattern { file, line, uneven: true } => write!(f, "A rule's pattern{} is not a box: its rows or layers are not all one length.", place(file, *line)),
            Fault::Pattern { file, line, uneven: false } => write!(f, "A rule{} has an input and an output of different sizes, which only a map node allows.", place(file, *line)),
            Fault::UnknownTile { file, line, tile } => write!(f, "The tile \"{tile}\" is named{} and the tileset has no such tile, or not turned that way.", place(file, *line)),
            Fault::TileShape { file, size, cube: false } => write!(f, "The tiles of {file} are {} by {} cells and must be square.", size[0], size[1]),
            Fault::TileShape { file, size, cube: true } => write!(f, "The tiles of {file} are {} by {} by {} cells and must be cubes, since the tileset asks for full symmetry.", size[0], size[1], size[2]),
            Fault::Colours { file, line, figure, limit } => write!(f, "{file} has {figure} colours{} and there are letters for {limit}. Give more letters, or use fewer colours in the file.", place("", *line)),
            Fault::NoResource { name } => write!(f, "The program needs the file {name}, which was not found beside it or among the programs that come with the editor. Put the file in a \"rules\" or \"tilesets\" folder beside the program."),
            Fault::Outside { name } => write!(f, "The program asks for \"{name}\", which is not a plain file name. Files are read only from the \"rules\" and \"tilesets\" folders beside the program, by name."),
            Fault::Unreadable { name, why } => write!(f, "The file {name} could not be read: {why}. Check that it is there and may be opened."),
            Fault::NotVox { file } => write!(f, "{file} is not a MagicaVoxel .vox file, or is damaged. Save it again from MagicaVoxel."),
            Fault::NoGrid { size } => write!(f, "A grid {} by {} by {} cells has no room for a program. Give each side at least one cell.", size[0], size[1], size[2]),
            Fault::Contradiction { file, line, at_start: true, .. } => write!(f, "The wfc node{} cannot be satisfied at this size: what the program fixed beforehand leaves some cell no tile. Try another size.", place(file, *line)),
            Fault::Contradiction { file, line, tries, at_start: false } => write!(f, "The wfc node{} found no arrangement of its tiles in {tries} tries. Try another seed or another size.", place(file, *line)),
            Fault::StepLimit { steps } => write!(f, "Stopped early: the program was still running after {steps} steps, which is the most allowed. What it had made is shown. Raise the step limit to let it go on, or make the size smaller."),
            Fault::TimeLimit { steps, seconds } => write!(f, "Stopped early: the program had run for {seconds} seconds ({steps} steps). What it had made is shown. Make the size smaller, or start it again to let it go on."),
        }
    }
}

impl std::error::Error for Fault {}

/// For the boundary with the rest of the core, which reports a problem as text.
impl From<Fault> for String {
    fn from(fault: Fault) -> String {
        fault.to_string()
    }
}

/// The limits everything read from a file or asked for is held to. One
/// set for bundled and imported programs alike: a bundled program is
/// within them, and an imported one is a stranger's file.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Limits {
    /// bytes in a program or tileset file, and in a .vox file
    pub text: u64,
    pub vox: u64,
    /// cells in any one grid
    pub cells: u64,
    /// rules in one node, counting turned and mirrored copies
    pub rules: u64,
    /// cells in one rule's pattern
    pub pattern: u64,
    /// nodes one inside another, and nodes in all
    pub depth: u64,
    pub nodes: u64,
    /// tiles in a tileset, counting turned and mirrored copies, and cells along a tile's side
    pub tiles: u64,
    pub tile: u64,
    /// cells times tiles in one wfc node, and cells times rules in one rule node
    pub wave: u64,
    pub matches: u64,
    /// steps in a run
    pub steps: usize,
    /// bricks written
    pub bricks: u64,
}

impl Limits {
    /// The usual limits. The largest program that comes with the original
    /// (ModernHouse, 31 KB of text, 32 KB tiles, a final grid of 150,000
    /// cells, 400 rules in a node) is several times inside each.
    pub const USUAL: Limits = Limits {
        text: 1 << 20,
        vox: 4 << 20,
        cells: 4_000_000,
        rules: 4_096,
        pattern: 4_096,
        depth: 64,
        nodes: 4_000,
        tiles: 512,
        tile: 32,
        wave: 40_000_000,
        matches: 2_000_000_000,
        steps: 50_000,
        bricks: 400_000,
    };
}

/// A size held to its limit.
pub fn within(thing: Thing, figure: u64, limit: u64, file: &str, line: usize) -> Result<(), Fault> {
    if figure > limit {
        return Err(Fault::TooLarge { thing, file: file.to_string(), line, figure, limit });
    }
    Ok(())
}
