//! A rule program as a form: which program, at what size, from which
//! seed, how far run, and the bricks that comes to.
//!
//! Every other form's bricks follow from its settings alone, and the
//! editor relies on that everywhere: a save keeps settings and no bricks,
//! undo keeps settings, a form is built again on another machine from the
//! same text. A rule program is a process, watched as it runs, and the
//! question was how a process fits that.
//!
//! It fits by making how far the run has got one of the settings.
//! [`Program::stop`] is the number of turns taken, or none for a run left
//! to reach its end. The interpreter draws its random numbers from the
//! seed and does its arithmetic the same on every machine (see
//! [`super::nodes::ln`]), so a program, a size, a seed and a number of
//! turns are one grid, wherever and whenever it is worked out. A run that
//! is playing in the editor is then nothing the rest of the editor need
//! know of: it is a form whose `stop` goes up each frame. Paused, it is a
//! form like any other, and what is copied, saved or joined is exactly
//! what a save of it builds again.
//!
//! The other way was to keep a running machine as the form and its
//! bricks as the machine's to give. Then a save must either write the
//! grid out (bricks in a save, which nothing else has, and a second way
//! for a form to be read back) or run the program again on opening
//! anyway; and undo, copy and paste and the workspace's other forms would
//! each need to learn of a form that cannot be rebuilt from its text.
//!
//! ## Not running a program twice
//!
//! Built from its settings, a paused run of nine thousand turns is nine
//! thousand turns taken again for every change of colour. So the grid a
//! run comes to is remembered ([`remember`], [`known`]) under exactly the
//! settings it follows from, and [`grown`] looks there first. This is a
//! memory of answers and changes none of them: with nothing remembered
//! the same grid is worked out again (`a_run_remembered_is_the_run_worked_out`).
//! The editor's own run, on its own thread, leaves each frame's grid
//! here, and the frame is then built by the same [`built`] as a form
//! opened from a save: one way from a grid to bricks, and no second copy
//! of it to disagree.
//!
//! ## A seed that comes to nothing
//!
//! Some seeds of some programs cannot be finished. A wfc node finds no
//! arrangement of its tiles: ModernHouse's does on about one seed in
//! three, and the machine says so with a [`Fault`]. Or the program fails
//! in a way of its own. SeaVilla, on about one seed in six, draws a
//! flight of stairs it cannot use, and its own rule for that (the node
//! its author marked "fail on wrong stair turn") floods the grid a cell
//! at a time; the run then ends tens of thousands of turns later with a
//! few columns standing in the sea, or does not end. No fault is raised,
//! since the program did what it says. That is told by the turns taken
//! ([`Entry::most`]): a run of SeaVilla that works ends within fourteen
//! hundred turns, and one that has failed is past thirty thousand, so a
//! run still going at six thousand is given up there. A count is a blunt
//! test, but the grid says nothing better of itself: the failed runs fill
//! as much of it as the good ones (see `how_the_bundled_programs_run`).
//!
//! Either way the run is begun again from the next seed, up to [`SEEDS`]
//! of them, and [`Outcome::passed`] says which were passed over and why.
//! A program brought in from outside has no such count, and what it ends
//! with is what is shown.

use super::bricks::{bricks_at, Finish, MOST_SCALE};
use super::fault::{within, Fault, Limits, Thing};
use super::nodes::{Machine, View};
use super::{bundled, Resources};
use crate::spec::{Built, Spec};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};

/// A program that comes with the editor.
pub struct Entry {
    pub name: &'static str,
    /// what it makes, and whose it is, in a line
    pub about: &'static str,
    /// the size it is run at, which is the size the original's `models.xml` gives it
    pub size: [i32; 3],
    /// microbricks to a cell as it starts: as many as make the finished thing some 120 to 200 across
    pub cell: i32,
    /// The most turns a run of it that works takes, with room to spare,
    /// or 0 where every run that ends has worked (see the module's notes,
    /// and `how_the_bundled_programs_run` in the tests for the counts
    /// this was set from).
    pub most: u32,
}

/// The programs on the list, in the order the list has them: the four
/// buildings first. All are Maxim Gumin's, from MarkovJunior
/// (https://github.com/mxgmn/MarkovJunior, MIT); CarmaTower is credited
/// there as below.
pub const BUNDLED: [Entry; 7] = [
    Entry { name: "Apartemazements", about: "A block of flats with stairs, columns and windows", size: [8, 8, 8], cell: 4, most: 0 },
    Entry { name: "CarmaTower", about: "A tower: a proceduralization of a voxel scene by Antoine Lendrevie", size: [12, 12, 18], cell: 2, most: 0 },
    Entry { name: "ModernHouse", about: "A flat-roofed house on a lawn", size: [9, 9, 4], cell: 3, most: 0 },
    Entry { name: "SeaVilla", about: "A villa on arches over the sea", size: [10, 10, 4], cell: 2, most: 6_000 },
    Entry { name: "PillarsOfEternity", about: "Paths on pillars", size: [9, 9, 9], cell: 4, most: 0 },
    Entry { name: "Escher", about: "Stairs and walkways that turn back on themselves", size: [8, 8, 8], cell: 4, most: 0 },
    Entry { name: "StrangeDungeon", about: "A dungeon of rooms and passages, in three dimensions", size: [29, 29, 29], cell: 4, most: 0 },
];

/// The most cells a grid may be asked to be along one side. (What a
/// program makes of it may be several times larger, which
/// [`Limits::cells`] holds.)
pub const MOST_SIDE: i32 = 128;
/// The size a program brought in from outside starts at, if nothing beside it says.
pub const USUAL_SIZE: [i32; 3] = [16, 16, 16];
/// The steps a run is allowed as it starts, and the most it may be allowed.
pub const USUAL_LIMIT: u32 = 50_000;
pub const MOST_LIMIT: u32 = 5_000_000;
/// How many seeds are tried, one after another, before a program is said not to work at a size.
pub const SEEDS: u64 = 16;
/// The most colours a form has, and so the most letters that have a
/// colour of their own (the editor's `paint::MOST_COLOURS`, which a test
/// there holds to this). A grid has no more values than this
/// ([`Limits::USUAL`]); a program whose several grids have more letters
/// between them than this has the last of them share the last colour.
pub const MOST_COLOURS: usize = 32;
/// How long a run may hold whoever asked for it, in seconds, before it
/// is given up ([`Fault::TimeLimit`]). The editor's own runs are on
/// another thread and have no such limit.
pub const MOST_SECONDS: u64 = 20;

/// Which rule program a form is, and how far it has run.
#[derive(Clone, Debug, PartialEq)]
pub struct Program {
    /// One of [`BUNDLED`] by its name, or, if `imported`, the name of a
    /// program in the folder [`set_library`] was given. Never a path: a
    /// form may come from anyone's save, and from Claude.
    pub name: String,
    pub imported: bool,
    /// the grid it starts on, in cells along x, y and z
    pub size: [i32; 3],
    /// microbricks to a cell
    pub cell: i32,
    pub seed: u64,
    pub finish: Finish,
    /// the most turns a run may take before it is stopped
    pub limit: u32,
    /// how many turns have been taken, or none for a run left to reach its end
    pub stop: Option<u32>,
}

impl Default for Program {
    fn default() -> Program {
        Program::of(BUNDLED[0].name, false)
    }
}

/// Whether a name may be a program's: letters, digits, spaces and `_ -`, sixty at most, and not all spaces.
pub fn plain(name: &str) -> bool {
    !name.trim().is_empty() && name.len() <= 60 && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | ' '))
}

impl Program {
    /// A program as it starts out: at its own size if it is one of [`BUNDLED`], seed 1, with wedges, run to its end.
    pub fn of(name: &str, imported: bool) -> Program {
        let entry = BUNDLED.iter().find(|entry| entry.name == name).filter(|_| !imported);
        Program { name: name.to_string(), imported, size: entry.map_or(USUAL_SIZE, |e| e.size), cell: entry.map_or(4, |e| e.cell), seed: 1, finish: Finish::Wedged, limit: USUAL_LIMIT, stop: None }
    }

    /// The same with every number within what it may be.
    pub fn clamped(&self) -> Program {
        let limit = self.limit.clamp(1, MOST_LIMIT);
        Program { size: self.size.map(|side| side.clamp(1, MOST_SIDE)), cell: self.cell.clamp(1, MOST_SCALE), limit, stop: self.stop.map(|stop| stop.min(limit)), ..self.clone() }
    }

    /// As a save keeps it: `name,x,y,z,cell,seed,finish,limit,stop`, the
    /// name with `imported:` before it for a program brought in from
    /// outside, the finish `plain` or `wedges`, and the stop a number of
    /// turns or `end`.
    pub fn text(&self) -> String {
        let [x, y, z] = self.size;
        format!("{}{},{x},{y},{z},{},{},{},{},{}", if self.imported { "imported:" } else { "" }, self.name, self.cell, self.seed, if self.finish == Finish::Plain { "plain" } else { "wedges" }, self.limit, self.stop.map_or("end".to_string(), |stop| stop.to_string()))
    }

    /// Read what [`Program::text`] writes, or the first few of its
    /// values: the name alone is that program as it starts out. The
    /// numbers are brought within what they may be. A name is taken as
    /// long as it is a plain one, whether or not there is such a program
    /// here: a save may have been made where there was, and its form then
    /// opens empty and says so ([`Fault::NoProgram`]).
    pub fn read(value: &str) -> Result<Program, Fault> {
        let parts: Vec<&str> = value.split(',').map(str::trim).collect();
        let bad = |attribute: &'static str, value: &str| Fault::BadValue { file: String::new(), line: 0, attribute, value: value.to_string() };
        let (name, imported) = parts[0].strip_prefix("imported:").map_or((parts[0], false), |name| (name.trim(), true));
        if !plain(name) {
            return Err(Fault::BadName { name: name.to_string() });
        }
        if parts.len() > 9 {
            return Err(bad("program", value));
        }
        let mut program = Program::of(name, imported);
        let whole = |attribute: &'static str, text: &str| text.parse::<i64>().map(|n| n.clamp(-1, i32::MAX as i64) as i32).map_err(|_| bad(attribute, text));
        for (n, part) in parts.iter().enumerate().skip(1) {
            match n {
                1..=3 => program.size[n - 1] = whole("size", part)?,
                4 => program.cell = whole("cell", part)?,
                5 => program.seed = part.parse().map_err(|_| bad("seed", part))?,
                6 => {
                    program.finish = match *part {
                        "plain" => Finish::Plain,
                        "wedges" => Finish::Wedged,
                        _ => return Err(bad("finish", part)),
                    }
                }
                7 => program.limit = whole("limit", part)?.max(0) as u32,
                _ => program.stop = if *part == "end" { None } else { Some(whole("stop", part)?.max(0) as u32) },
            }
        }
        Ok(program.clamped())
    }

    /// What a run of it follows from, less how far it has got.
    fn run(&self) -> Run {
        (self.name.clone(), self.imported, self.size, self.seed, self.limit)
    }

    /// Whether `other` is the same run as this, at whatever turn each stands.
    pub fn same_run(&self, other: &Program) -> bool {
        self.run() == other.run()
    }
}

/// The folder programs brought in from outside are kept in, a folder
/// each (`<name>/<name>.xml`, with the files it reads beside it). Set
/// once by whoever keeps such a folder, which is the editor; until then
/// there are no such programs.
///
/// Kept here, for the whole process, and not handed to every build: a
/// build is given a form's settings and nothing else, by a dozen callers,
/// and where one computer keeps its files is not a setting of a form.
static LIBRARY: RwLock<Option<PathBuf>> = RwLock::new(None);

pub fn set_library(folder: PathBuf) {
    *LIBRARY.write().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(folder);
}

pub fn library() -> Option<PathBuf> {
    LIBRARY.read().unwrap_or_else(|poisoned| poisoned.into_inner()).clone()
}

/// A program's text, the name to call it by in messages, and where the files it names are read from.
pub fn source(program: &Program) -> Result<(String, Vec<u8>, Resources), Fault> {
    let missing = || Fault::NoProgram { name: program.name.clone(), imported: program.imported };
    if !plain(&program.name) {
        return Err(Fault::BadName { name: program.name.clone() });
    }
    let file = format!("{}.xml", program.name);
    if !program.imported {
        let text = bundled::PROGRAMS.iter().find(|(name, _)| *name == program.name).filter(|_| BUNDLED.iter().any(|entry| entry.name == program.name)).ok_or_else(missing)?.1;
        return Ok((file, text.to_vec(), Resources { roots: Vec::new(), bundled: bundled::RESOURCES, seen: None }));
    }
    let folder = library().ok_or_else(missing)?.join(&program.name);
    let path = folder.join(&file);
    if !path.is_file() {
        return Err(missing());
    }
    let unreadable = |why: std::io::Error| Fault::Unreadable { name: file.clone(), why: why.to_string() };
    within(Thing::File, std::fs::metadata(&path).map_err(unreadable)?.len(), Limits::USUAL.text, &file, 0)?;
    let text = std::fs::read(&path).map_err(unreadable)?;
    Ok((file, text, Resources { roots: vec![folder], bundled: bundled::RESOURCES, seen: None }))
}

/// The limits a form's program is held to: the usual ones, with the steps the form allows.
fn limits(program: &Program) -> Limits {
    Limits { steps: program.limit as usize, ..Limits::USUAL }
}

/// The colour of each of a program's letters, in the order its form's
/// colours are numbered ([`Machine::letters`]): the program's own
/// palette. Read from `palette.xml`, which for a program from outside is
/// the one beside it if there is one and otherwise the original's, which
/// comes with the editor. A letter the palette has no colour for is grey.
pub fn colours(program: &Program) -> Result<Vec<(char, [u8; 3])>, Fault> {
    let program = program.clamped();
    let (file, text, resources) = source(&program)?;
    let limits = limits(&program);
    let machine = Machine::load(&file, &text, program.size.map(|side| side as usize), &resources, &limits)?;
    let (name, bytes) = resources.read(&["palette.xml"], limits.text)?;
    let palette = super::palette(&name, &bytes, &limits)?;
    Ok(machine.letters().into_iter().take(MOST_COLOURS).map(|letter| (letter, palette.iter().find(|(known, _)| *known == letter).map_or([128; 3], |(_, colour)| *colour))).collect())
}

/// The grid a run has come to, and what there is to say of the run.
#[derive(Clone, Debug, PartialEq)]
pub struct Outcome {
    /// the grid: its size, and the value in each cell (x fastest, then y, then z); 0 is nothing
    pub size: [usize; 3],
    pub cells: Vec<u8>,
    /// for each value of this grid, the number of its colour among the form's
    pub shade_of: Vec<u8>,
    /// the size of the grid the run ends in, which a `map` or `wfc` node may not have made yet
    pub full: [usize; 3],
    /// turns taken, and whether there are more to take
    pub turns: u32,
    pub running: bool,
    /// the seed this is the run of, which is the form's unless seeds were passed over
    pub seed: u64,
    /// the seeds passed over before it, and how each failed
    pub passed: Vec<(u64, Fault)>,
    /// why the run was stopped before its end, if it was: its step limit
    pub early: Option<Fault>,
    /// the node whose turn is next, and its line in the program
    pub doing: Option<(&'static str, usize)>,
}

/// What a step of a [`Grower`] came to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Stepped {
    /// a turn was taken and there are more
    More,
    /// the run is over: ended, or stopped at its limit
    Over,
    /// the seed came to nothing and the run has begun again from the next
    Passed,
}

/// A run of a form's program, taken a turn at a time: the machine, and
/// the passing over of seeds that come to nothing.
pub struct Grower {
    program: Program,
    machine: Machine,
    seed: u64,
    passed: Vec<(u64, Fault)>,
    early: Option<Fault>,
    /// the most turns a run that works takes, or 0 for no such test
    most: u32,
}

impl Grower {
    /// Load the form's program and stand at turn 0 of its seed.
    pub fn new(program: &Program) -> Result<Grower, Fault> {
        let program = program.clamped();
        let (file, text, resources) = source(&program)?;
        let mut machine = Machine::load(&file, &text, program.size.map(|side| side as usize), &resources, &limits(&program))?;
        machine.start(program.seed);
        let most = BUNDLED.iter().find(|entry| entry.name == program.name && !program.imported).map_or(0, |entry| entry.most);
        Ok(Grower { seed: program.seed, program, machine, passed: Vec::new(), early: None, most })
    }

    /// Back to turn 0 of the form's own seed.
    pub fn restart(&mut self) {
        self.seed = self.program.seed;
        self.passed.clear();
        self.early = None;
        self.machine.start(self.seed);
    }

    pub fn turns(&self) -> u32 {
        self.machine.steps() as u32
    }

    pub fn running(&self) -> bool {
        self.machine.running() && self.early.is_none()
    }

    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// The run is given up on this seed and begun on the next, unless that is one seed too many.
    fn pass(&mut self, fault: Fault) -> Result<Stepped, Fault> {
        self.passed.push((self.seed, fault));
        if self.passed.len() as u64 >= SEEDS {
            let (_, why) = self.passed.pop().expect("just pushed");
            return Err(Fault::NoSeed { from: self.program.seed, tries: SEEDS, why: Box::new(why) });
        }
        self.seed = self.seed.wrapping_add(1);
        self.machine.start(self.seed);
        Ok(Stepped::Passed)
    }

    /// Take one turn.
    pub fn step(&mut self) -> Result<Stepped, Fault> {
        if !self.running() {
            return Ok(Stepped::Over);
        }
        // (a run that has failed in the program's own way, before it is one that has merely used its steps)
        if self.most > 0 && self.machine.steps() >= self.most as usize {
            return self.pass(Fault::Overrun { turns: self.machine.steps(), most: self.most as usize });
        }
        if self.machine.steps() >= self.program.limit as usize {
            self.early = Some(Fault::StepLimit { steps: self.machine.steps() });
            return Ok(Stepped::Over);
        }
        match self.machine.step() {
            Ok(true) => Ok(Stepped::More),
            Ok(false) => Ok(Stepped::Over),
            Err(fault @ Fault::Contradiction { .. }) => self.pass(fault),
            Err(fault) => Err(fault),
        }
    }

    /// Take up to `turns` turns, stopping short where the run ends or
    /// begins again from another seed (so that whoever is watching sees
    /// it begin again). Returns whether there are more to take.
    pub fn advance(&mut self, turns: u32) -> Result<bool, Fault> {
        for _ in 0..turns {
            if self.step()? != Stepped::More {
                break;
            }
        }
        Ok(self.running())
    }

    /// Run until `stop` turns have been taken of a seed, or to the end
    /// for none. With `seconds`, the run is given up if it takes longer.
    pub fn run_to(&mut self, stop: Option<u32>, seconds: Option<u64>) -> Result<(), Fault> {
        #[cfg(not(target_arch = "wasm32"))]
        let began = std::time::Instant::now();
        let mut since_looked = 0u32;
        while self.running() && stop.is_none_or(|stop| self.turns() < stop) {
            self.step()?;
            since_looked += 1;
            // (the clock is looked at every few turns, and not at all in a browser, which has none to look at here: https://doc.rust-lang.org/std/time/struct.Instant.html#platform-specific-behavior)
            #[cfg(not(target_arch = "wasm32"))]
            if since_looked >= 16 {
                since_looked = 0;
                if seconds.is_some_and(|seconds| began.elapsed().as_secs() >= seconds) {
                    return Err(Fault::TimeLimit { steps: self.machine.steps(), seconds: seconds.unwrap_or(0) });
                }
            }
        }
        let _ = (seconds, since_looked);
        Ok(())
    }

    /// The grid as it stands.
    pub fn outcome(&mut self) -> Outcome {
        let letters = self.machine.letters();
        let (full, turns, running, doing) = (self.machine.last_size(), self.turns(), self.running(), self.machine.doing());
        let view = self.machine.view();
        let shade_of = view.letters.iter().map(|letter| letters.iter().position(|known| known == letter).unwrap_or(0).min(MOST_COLOURS - 1) as u8).collect();
        Outcome { size: view.size, cells: view.cells.to_vec(), shade_of, full, turns, running, seed: self.seed, passed: self.passed.clone(), early: self.early.clone(), doing: doing.filter(|_| running) }
    }
}

/// What a run follows from, less how far it has got: the program, whether it is from outside, the size, the seed and the step limit.
type Run = (String, bool, [i32; 3], u64, u32);

struct Kept {
    run: Run,
    stop: Option<u32>,
    result: Result<Arc<Outcome>, Fault>,
}

/// How many grids are remembered: the run being watched and its end, and a few forms beside it. Each is a byte a cell.
const KEEP: usize = 8;

static KEPT: Mutex<Vec<Kept>> = Mutex::new(Vec::new());

fn kept() -> std::sync::MutexGuard<'static, Vec<Kept>> {
    KEPT.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Remember what a form's program comes to, so that [`grown`] need not
/// work it out again. Of one run's stops part way, only the latest two
/// are kept: a run being watched leaves one each frame, and the frames
/// before are not asked for again. Two and not one, because the run is
/// on another thread and may leave the next frame's grid while this
/// frame's is still to be built.
pub fn remember(program: &Program, result: Result<Arc<Outcome>, Fault>) {
    let program = program.clamped();
    let (run, mut all) = (program.run(), kept());
    all.retain(|kept| !(kept.run == run && kept.stop == program.stop));
    if program.stop.is_some() {
        let part_way: Vec<usize> = all.iter().enumerate().filter(|(_, kept)| kept.run == run && kept.stop.is_some()).map(|(at, _)| at).collect();
        if part_way.len() >= 2 {
            all.remove(part_way[0]);
        }
    }
    all.push(Kept { run, stop: program.stop, result });
    if all.len() > KEEP {
        all.remove(0);
    }
}

/// What is remembered for a form's program as it stands, if anything.
pub fn known(program: &Program) -> Option<Result<Arc<Outcome>, Fault>> {
    let program = program.clamped();
    let run = program.run();
    kept().iter().find(|kept| kept.stop == program.stop && kept.run == run).map(|kept| kept.result.clone())
}

/// Forget every run of a program from outside: its file has been brought in again and may be another.
pub fn forget(name: &str) {
    kept().retain(|kept| !(kept.run.1 && kept.run.0 == name));
}

/// Forget everything, for a test that would see a run worked out.
pub fn forget_all() {
    kept().clear();
}

/// The grid a form's program comes to: remembered, or worked out here
/// and now, which holds whoever asks for as long as it takes, up to
/// [`MOST_SECONDS`]. (A failure is remembered as an answer is: a
/// program that cannot be read is not read again for every frame. Not a
/// run given up for its time, which another try may finish.)
pub fn grown(program: &Program) -> Result<Arc<Outcome>, Fault> {
    if let Some(result) = known(program) {
        return result;
    }
    let program = program.clamped();
    let result = Grower::new(&program).and_then(|mut grower| {
        grower.run_to(program.stop, Some(MOST_SECONDS))?;
        Ok(Arc::new(grower.outcome()))
    });
    if !matches!(result, Err(Fault::TimeLimit { .. })) {
        remember(&program, result.clone());
    }
    result
}

/// The bricks of a form that is a rule program, before the form is turned.
///
/// The grid stands on the form's point: the middle of its floor. A grid
/// that is not yet the run's last (the plan a `wfc` node will build
/// from, eight cells across where the building will be forty) is drawn
/// with each cell as many times larger as fits in the room the last grid
/// will take, so that the plan is seen at the size of what it is a plan
/// of and the view does not jump when the building appears.
pub fn built(spec: &Spec) -> Result<Built, String> {
    let program = spec.program.clamped();
    let outcome = grown(&program)?;
    let point = spec.points.first().copied().unwrap_or_default();
    let grow = (0..3).map(|k| (outcome.full[k] / outcome.size[k].max(1)).max(1)).min().unwrap_or(1) as i32;
    let view = View { size: outcome.size, cells: &outcome.cells, letters: &[] };
    // Wedges are read from a grid the program has finished with (or been
    // stopped in at its limit), and a run still under way is built plain.
    // A flight is told by its neighbours two steps off, so while a
    // staircase is being drawn a cell at a time its ramps come and go
    // with every cell; and a frame of a run being watched is then twice
    // the work or more for bricks that are gone at the next (CarmaTower's
    // grid part way through: 19 ms a frame with wedges and 10 plain,
    // measured before cells were handed to the joiner in rows).
    let finish = if outcome.running { Finish::Plain } else { program.finish };
    let made = bricks_at(&view, program.cell * grow, finish, spec.max_ext, &Limits::USUAL)?;
    // the low corner of the room the last grid takes, and of this grid within it
    let room = outcome.full.map(|side| side as i32 * program.cell);
    let mine = outcome.size.map(|side| side as i32 * program.cell * grow);
    let corner = [point.x.floor() as i32 - room[0] / 2 + (room[0] - mine[0]) / 2, point.y.floor() as i32 - room[1] / 2 + (room[1] - mine[1]) / 2, point.z.floor() as i32];
    let bricks = made
        .bricks
        .into_iter()
        .map(|mut brick| {
            brick.min = [0, 1, 2].map(|k| brick.min[k] + corner[k]);
            brick
        })
        .collect();
    let shades = made.shades.iter().map(|value| outcome.shade_of.get(*value as usize).copied().unwrap_or(0)).collect();
    // (no volume of a true form to compare the bricks with: they are the form)
    Ok(Built { bricks, solid: 0.0, error: 0.0, path: Vec::new(), grain: 1, unmeasured: false, shades })
}
