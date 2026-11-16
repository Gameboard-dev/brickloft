//! A rule program watched as it runs: Play, Pause, Step, Restart, and the
//! frames between.
//!
//! A form's bricks follow from its settings, and a rule program's
//! settings include how many turns it has run
//! (`brickloft_core::rewrite::form::Program::stop`). So a run being
//! watched is a form whose `stop` goes up, and that is all this does to
//! the form: each frame it writes the turn the run has reached into the
//! spec, and the editor builds the form as it builds any form whose
//! settings changed. Paused, the form says turn 312, and a save of it
//! opened anywhere is the same bricks. Nothing else in the editor knows a
//! run from a slider being dragged.
//!
//! The turns themselves are taken on another thread ([`Worker`]). One
//! turn of a wfc node is up to three seconds of searching for a seed that
//! fills the grid (PillarsOfEternity, measured), and a turn of
//! CarmaTower's largest node is half a second; taken in the frame, each
//! would stop the window for as long. The thread leaves the grid it has
//! reached where a build looks first (`form::remember`), then says which
//! turn that is, and the frame that hears it builds from the grid already
//! there. Until it hears, the window goes on drawing what it has and the
//! readout says the run is working.
//!
//! The alternative was to take turns in the frame within a budget of
//! milliseconds, as a best-fit search is (`State::search_for`). A turn
//! cannot be cut short, so the budget is exceeded by whatever one turn
//! costs, and the worst of them are the ones named above.
//!
//! A program that has not been run yet is never run in a frame either:
//! the form being edited is built behind the window
//! (`State::refresh_behind`), and the workspace's other forms, when a
//! save is opened, stand empty until a thread here has run them
//! ([`State::grow_others`]).

use super::{State, PROGRAM};
// (what only the builds behind the window use, which a page in a browser has none of)
#[cfg(not(target_arch = "wasm32"))]
use super::{bounds_of, SLOW_BUILD_MS};
use brickloft_core::rewrite::form::{self, Grower, Outcome, Program};
use brickloft_core::rewrite::Fault;
#[cfg(not(target_arch = "wasm32"))]
use brickloft_core::spec::{Built, Form};
use std::sync::Arc;

/// The slowest and the fastest a run may be watched at, in turns a second, and the speed it starts at.
pub const SPEEDS: (f64, f64) = (1.0, 20_000.0);
pub const USUAL_SPEED: f64 = 120.0;

/// What the panel shows of a run and its controls set.
pub struct Grow {
    /// whether the run goes on by itself, frame after frame
    pub playing: bool,
    /// turns a second
    pub speed: f64,
    /// whether a change of program, size or seed is watched from its first turn, or shown finished
    pub watch: bool,
    /// seeds passed over to reach the one shown, said under the controls: the words, and the run they are true of
    note: Option<(String, Program)>,
    /// A run that was stopped early and whose bricks stand: said where a form's problem is said.
    pub caution: Option<String>,
    /// the path typed for a program to bring in, and what came of the last one
    pub path: String,
    pub import_note: String,
    /// the programs brought in so far, by name
    pub imported: Vec<String>,
    worker: Option<Worker>,
    /// turns owed to the run by the time that has passed, and when that was last counted
    owed: f64,
    counted_at: Option<f64>,
    /// turns to take once, whether playing or not: a step
    once: u32,
    /// whether the run is to go on to its end without being shown on the way
    to_end: bool,
    /// when the run was last asked for turns it has not yet given, and whether that is longer ago than a moment: the panel then says it is working
    asked_at: Option<f64>,
    pub working: bool,
    /// whether the form's last build was sent behind the window because its program had not been run, and not because building it is slow
    forced: bool,
    /// a form of the workspace that is not the one being edited, being run on a thread of its own, and those that could not be
    #[cfg(not(target_arch = "wasm32"))]
    other: Option<(Program, std::thread::JoinHandle<()>)>,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    refused: Vec<Program>,
}

impl Default for Grow {
    fn default() -> Grow {
        Grow {
            playing: false,
            speed: USUAL_SPEED,
            watch: true,
            note: None,
            caution: None,
            path: String::new(),
            import_note: String::new(),
            imported: Vec::new(),
            worker: None,
            owed: 0.0,
            counted_at: None,
            once: 0,
            to_end: false,
            asked_at: None,
            working: false,
            forced: false,
            #[cfg(not(target_arch = "wasm32"))]
            other: None,
            refused: Vec::new(),
        }
    }
}

/// What a run says when it has taken the turns it was asked for: the
/// form's program as it now stands (its seed, and its stop) and the seeds
/// passed over on the way, or why it cannot go on.
type Said = Result<(Program, Vec<(u64, Fault)>), Fault>;

/// Take `turns` turns of a run, leave the grid it comes to where a build
/// will find it, and say where the run stands. `quit` is asked between
/// turns whether anyone is still waiting.
fn taken(grower: &mut Grower, base: &Program, turns: u32, quit: impl Fn() -> bool) -> Said {
    let seed_before = grower.seed();
    let mut stepped = Ok(form::Stepped::More);
    for _ in 0..turns {
        stepped = grower.step();
        // (a run begun again from another seed is shown beginning again, and not some turns into it)
        if !matches!(stepped, Ok(form::Stepped::More)) || quit() {
            break;
        }
    }
    let now = |grower: &Grower, stop: Option<u32>| Program { seed: grower.seed(), stop, ..base.clone() };
    match stepped {
        Ok(_) => {
            let mut outcome = grower.outcome();
            // Remembered as the run of the seed it is, which passed over
            // nothing: the form is given that seed below, and the same
            // form built from nothing is this grid and says the same.
            let passed = std::mem::take(&mut outcome.passed);
            outcome.seed = grower.seed();
            let program = now(grower, outcome.running.then_some(outcome.turns));
            form::remember(&program, Ok(Arc::new(outcome)));
            Ok((program, if grower.seed() == seed_before { Vec::new() } else { passed }))
        }
        Err(fault) => {
            // (said by the build, which finds it here and does not run the program again to learn it)
            form::remember(&now(grower, None), Err(fault.clone()));
            Err(fault)
        }
    }
}

/// A run on its own thread, and what the frame knows of it.
struct Worker {
    /// the form's program as the run last said it stands: what the form must still be for the run to be its run
    at: Program,
    /// which form it was begun for (see `State::loaded`)
    loaded: u64,
    /// whether it has been asked for turns it has not yet given
    waiting: bool,
    #[cfg(not(target_arch = "wasm32"))]
    ask: std::sync::mpsc::Sender<u32>,
    #[cfg(not(target_arch = "wasm32"))]
    said: std::sync::mpsc::Receiver<Said>,
    #[cfg(not(target_arch = "wasm32"))]
    quit: Arc<std::sync::atomic::AtomicBool>,
    /// (in a browser there is no other thread: the turns are taken when they are asked for, and said at the next frame)
    #[cfg(target_arch = "wasm32")]
    grower: Result<Grower, Fault>,
    #[cfg(target_arch = "wasm32")]
    said: Option<Said>,
}

impl Worker {
    /// Begin the run of `program` at the turn it stands at.
    #[cfg(not(target_arch = "wasm32"))]
    fn new(program: &Program, loaded: u64) -> Worker {
        let (ask, asked) = std::sync::mpsc::channel::<u32>();
        let (say, said) = std::sync::mpsc::channel::<Said>();
        let quit = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (base, gone) = (program.clone(), quit.clone());
        std::thread::spawn(move || {
            let gone = || gone.load(std::sync::atomic::Ordering::Relaxed);
            // (the program is loaded here too: ModernHouse's four hundred patterns take a tenth of a second to read)
            let mut grower = Grower::new(&base).and_then(|mut grower| {
                // a form opened part way through its run is brought to where it stands before it goes on
                while grower.running() && base.stop.is_some_and(|stop| grower.turns() < stop) && !gone() {
                    grower.step()?;
                }
                Ok(grower)
            });
            // (until whoever asks has gone: then there is nobody to tell)
            while let Ok(turns) = asked.recv() {
                let said = match &mut grower {
                    Ok(grower) => taken(grower, &base, turns, gone),
                    Err(fault) => {
                        form::remember(&Program { stop: None, ..base.clone() }, Err(fault.clone()));
                        Err(fault.clone())
                    }
                };
                if say.send(said).is_err() {
                    break;
                }
            }
        });
        Worker { at: program.clone(), loaded, waiting: false, ask, said, quit }
    }

    #[cfg(target_arch = "wasm32")]
    fn new(program: &Program, loaded: u64) -> Worker {
        let grower = Grower::new(program).and_then(|mut grower| {
            grower.run_to(program.stop, None)?;
            Ok(grower)
        });
        Worker { at: program.clone(), loaded, waiting: false, grower, said: None }
    }

    fn ask(&mut self, turns: u32) {
        self.waiting = true;
        #[cfg(not(target_arch = "wasm32"))]
        let _ = self.ask.send(turns);
        #[cfg(target_arch = "wasm32")]
        {
            let base = self.at.clone();
            self.said = Some(match &mut self.grower {
                // (no more than a few turns at once, where a turn holds the page)
                Ok(grower) => taken(grower, &base, turns.min(64), || false),
                Err(fault) => Err(fault.clone()),
            });
        }
    }

    /// What the run has said since it was last asked, if it has; `Err(())` if it has gone without saying.
    fn hear(&mut self) -> Result<Option<Said>, ()> {
        #[cfg(not(target_arch = "wasm32"))]
        return match self.said.try_recv() {
            Ok(said) => Ok(Some(said)),
            Err(std::sync::mpsc::TryRecvError::Empty) => Ok(None),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => Err(()),
        };
        #[cfg(target_arch = "wasm32")]
        Ok(self.said.take())
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for Worker {
    fn drop(&mut self) {
        // (a run asked for more turns than anyone will now wait for stops at its next turn)
        self.quit.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Whether two programs are the same run at the same turn: all that a run decides of a form.
fn stands_at(a: &Program, b: &Program) -> bool {
    a.same_run(b) && a.stop == b.stop
}

/// The seeds passed over, in words.
pub(crate) fn passed_over(passed: &[(u64, Fault)], shown: u64) -> String {
    let (first, why) = &passed[0];
    let which = if passed.len() == 1 { format!("Seed {first} was passed over") } else { format!("Seeds {first} to {} were passed over", passed[passed.len() - 1].0) };
    format!("{which} and seed {shown} is shown. Seed {first} could not be finished: {why}")
}

impl State {
    /// Make the form the rule program `name` as it starts out: at its
    /// own size, standing on the floor, in the program's own colours.
    /// The seed is kept, as a tree's is. If changes are watched, it is
    /// run from its first turn.
    pub fn usual_program(&mut self, name: &str, imported: bool) {
        let mut program = Program { seed: self.spec.program.seed, ..Program::of(name, imported) };
        if imported {
            program.size = crate::programs::usual_size(name).unwrap_or(program.size);
        }
        self.spec.program = program;
        self.program_colours();
        self.grow_changed();
    }

    /// Give the form the colours its program's letters have in the program's own palette.
    pub fn program_colours(&mut self) {
        // (a program that cannot be read keeps the colours the form has, and the build says why it cannot)
        if let Ok(colours) = form::colours(&self.spec.program) {
            self.paint = crate::paint::Paint { mode: crate::paint::Mode::Pattern, colours: colours.into_iter().map(|(_, colour)| colour).collect(), ..crate::paint::Paint::default() };
        }
    }

    /// The program, its size, its seed or its step limit has been
    /// changed by hand: the run starts again, watched from its first turn
    /// or shown finished as the panel's tickbox says.
    pub fn grow_changed(&mut self) {
        self.grow.worker = None;
        (self.grow.once, self.grow.to_end, self.grow.owed) = (0, false, 0.0);
        self.grow.playing = self.grow.watch;
        self.spec.program.stop = if self.grow.watch { Some(0) } else { None };
    }

    /// Play: go on from where the run stands, or from the start if it has ended.
    pub fn grow_play(&mut self) {
        if self.spec.program.stop.is_none() {
            self.grow.worker = None;
            self.spec.program.stop = Some(0);
        }
        (self.grow.playing, self.grow.to_end) = (true, false);
    }

    pub fn grow_pause(&mut self) {
        (self.grow.playing, self.grow.to_end, self.grow.once) = (false, false, 0);
    }

    /// One turn, and stop there.
    pub fn grow_step(&mut self) {
        self.grow_play();
        (self.grow.playing, self.grow.once) = (false, 1);
    }

    /// From the first turn again, playing.
    pub fn grow_restart(&mut self) {
        self.grow.worker = None;
        self.spec.program.stop = Some(0);
        (self.grow.playing, self.grow.to_end, self.grow.once, self.grow.owed) = (true, false, 0, 0.0);
    }

    /// On to the end without showing the turns between.
    pub fn grow_finish(&mut self) {
        (self.grow.playing, self.grow.once) = (false, 0);
        if self.grow.worker.is_some() {
            self.grow.to_end = true;
        } else {
            // (no run in hand: the form is built to its end like any form whose settings changed, behind the window)
            self.spec.program.stop = None;
        }
    }

    /// Whether a run is going on by itself or has turns still to give: the form is then changing without anyone changing it.
    pub fn growing(&self) -> bool {
        self.form == PROGRAM && (self.grow.playing || self.grow.to_end || self.grow.once > 0 || self.grow.worker.as_ref().is_some_and(|worker| worker.waiting))
    }

    /// What the seeds passed over were, if the run shown passed over any.
    pub fn grow_note(&self) -> Option<&str> {
        self.grow.note.as_ref().filter(|(_, of)| of.same_run(&self.spec.program)).map(|(note, _)| note.as_str())
    }

    /// What is known of the run as the form stands: its turn, whether it has more to take, and what it is doing.
    pub fn grow_outcome(&self) -> Option<Arc<Outcome>> {
        (self.form == PROGRAM).then(|| form::known(&self.spec.program)).flatten().and_then(Result::ok)
    }

    /// Once a frame, before the form is built: hear what the run has
    /// reached and write it into the form, and ask the run for the turns
    /// the time since has earned.
    pub fn grow_frame(&mut self, now_ms: f64) {
        self.grow_others();
        let elapsed = self.grow.counted_at.replace(now_ms).map_or(0.0, |before| (now_ms - before).clamp(0.0, 250.0));
        if self.form != PROGRAM {
            self.grow.worker = None;
            (self.grow.playing, self.grow.to_end, self.grow.once, self.grow.caution, self.grow.working) = (false, false, 0, None, false);
            return;
        }
        self.spec.program = self.spec.program.clamped();
        // A run begun for another form, or for what this form no longer
        // is (it was changed from outside: an undo, a save opened,
        // Claude), is not this form's run.
        if self.grow.worker.as_ref().is_some_and(|worker| worker.loaded != self.loaded || !stands_at(&worker.at, &self.spec.program)) {
            self.grow.worker = None;
            (self.grow.playing, self.grow.to_end, self.grow.once) = (false, false, 0);
        }
        // What the run has reached. Not while the last frame's bricks are
        // still being made behind the window: the form would move on
        // again before they came, and they would never be shown.
        if !self.fitting() {
            let heard = self.grow.worker.as_mut().filter(|worker| worker.waiting).map(|worker| worker.hear());
            match heard {
                Some(Ok(Some(said))) => {
                    self.grow.asked_at = None;
                    match said {
                        Ok((now, passed)) => {
                            if !passed.is_empty() {
                                self.grow.note = Some((passed_over(&passed, now.seed), now.clone()));
                            }
                            (self.spec.program.seed, self.spec.program.stop) = (now.seed, now.stop);
                            if let Some(worker) = &mut self.grow.worker {
                                (worker.waiting, worker.at) = (false, self.spec.program.clone());
                            }
                            if now.stop.is_none() {
                                // ended, or stopped at its limit: there is no more of it to run
                                self.grow.worker = None;
                                (self.grow.playing, self.grow.to_end, self.grow.once) = (false, false, 0);
                            }
                        }
                        // (the build says why, where a form's problem is said: the run left it there to be found)
                        Err(_) => {
                            self.spec.program.stop = None;
                            self.grow.worker = None;
                            (self.grow.playing, self.grow.to_end, self.grow.once) = (false, false, 0);
                        }
                    }
                }
                // (a run that has gone without a word: the form is built from its settings, as ever)
                Some(Err(())) => {
                    self.grow.worker = None;
                    (self.grow.playing, self.grow.to_end, self.grow.once) = (false, false, 0);
                }
                _ => {}
            }
        }
        // The turns to ask for now.
        if self.grow.playing {
            // (no more owed than a second's worth, however long the run takes over its turns)
            self.grow.owed = (self.grow.owed + elapsed / 1000.0 * self.grow.speed.clamp(SPEEDS.0, SPEEDS.1)).min(self.grow.speed.max(1.0));
        } else {
            self.grow.owed = 0.0;
        }
        let wanted = if self.grow.to_end { u32::MAX } else { self.grow.once.max(self.grow.owed.floor() as u32) };
        // (not while a slider is being dragged: a run begun for each value on the way would be a thread and a program read for each)
        if wanted > 0 && !self.busy {
            let (program, loaded) = (self.spec.program.clone(), self.loaded);
            let worker = self.grow.worker.get_or_insert_with(|| Worker::new(&program, loaded));
            if !worker.waiting {
                worker.ask(wanted);
                self.grow.asked_at = Some(now_ms);
                self.grow.owed -= self.grow.owed.floor();
                self.grow.once = 0;
            }
        }
        self.grow.working = self.grow.worker.as_ref().is_some_and(|worker| worker.waiting) && self.grow.asked_at.is_some_and(|at| now_ms - at > 250.0);
        // What there is to say of the form as it stands, and whether it
        // has yet to be run at all: then it is run behind the window,
        // however quick the last build was (see `State::refresh_behind`).
        // (a form taken up from among the workspace's others comes with the bricks it had there, which for one not yet run, or not to be run, are none: `State::take_item`)
        let unbuilt = self.built.bricks.is_empty() && self.built_from.is_some() && self.problem.is_none() && !self.fitting();
        match form::known(&self.spec.program) {
            Some(known) => {
                // (run now, so building it is writing a grid out as bricks: done in the frame again, unless that proves slow in its own right)
                if std::mem::take(&mut self.grow.forced) && !self.fitting() {
                    self.build_ms = 0.0;
                }
                let outcome = known.as_ref().ok();
                self.grow.caution = outcome.and_then(|outcome| outcome.early.as_ref()).map(|early| early.to_string());
                if let Some(outcome) = outcome.filter(|outcome| !outcome.passed.is_empty()) {
                    self.grow.note = Some((passed_over(&outcome.passed, outcome.seed), self.spec.program.clone()));
                }
                match &known {
                    // run since, and not yet built from: built now
                    Ok(outcome) if unbuilt && outcome.cells.iter().any(|value| *value != 0) => self.built_from = None,
                    Err(fault) if unbuilt => self.problem = Some(fault.to_string()),
                    _ => {}
                }
            }
            None => {
                self.grow.caution = None;
                if unbuilt {
                    self.built_from = None;
                }
                #[cfg(not(target_arch = "wasm32"))]
                {
                    self.build_ms = self.build_ms.max(SLOW_BUILD_MS);
                    self.grow.forced = true;
                    // (and the bricks in hand, if they are another form's, are not shown as this one's while it runs)
                    if self.built_from.as_ref().is_none_or(|from| from.form != Form::Program) && !self.built.bricks.is_empty() {
                        (self.built, self.bounds) = (Built::default(), None);
                        self.scene += 1;
                    }
                }
            }
        }
    }

    /// The workspace's other forms that are rule programs not yet run
    /// (a save just opened): each is run on a thread, one at a time, and
    /// given its bricks when the run is done. Until then it stands empty.
    ///
    /// Run where the save is opened, each would hold the window for as
    /// long as its program takes, which for CarmaTower is two to ten
    /// seconds (see `State::restore`, which leaves them for this).
    fn grow_others(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            if self.grow.other.as_ref().is_some_and(|(_, thread)| thread.is_finished()) {
                let (program, _) = self.grow.other.take().expect("just seen");
                // (a run given up for its time leaves nothing: not tried again at every frame)
                if form::known(&program).is_none() {
                    self.grow.refused.push(program);
                }
            }
            let mut built = false;
            for item in self.others.iter_mut().filter(|item| item.spec.form == Form::Program && item.built.bricks.is_empty()) {
                let program = item.spec.program.clamped();
                if self.grow.refused.contains(&program) {
                    continue;
                }
                match form::known(&program) {
                    Some(_) => {
                        // (from the grid the run left: no turn is taken here)
                        match brickloft_core::spec::build(&item.spec) {
                            Ok(bricks) => {
                                item.bounds = bounds_of(&bricks);
                                item.built = bricks;
                                built = true;
                            }
                            Err(_) => self.grow.refused.push(program),
                        }
                    }
                    None if self.grow.other.is_none() => {
                        let run = program.clone();
                        self.grow.other = Some((
                            program,
                            std::thread::spawn(move || {
                                let _ = form::grown(&run);
                            }),
                        ));
                    }
                    None => {}
                }
            }
            if built {
                self.scene += 1;
            }
            if self.grow.refused.len() > 64 {
                self.grow.refused.remove(0);
            }
        }
    }

    /// Whether a form of the workspace is waiting for its program to be run: a save's bricks are not all there yet.
    pub fn grow_waiting(&self) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        return self.grow.other.is_some();
        #[cfg(target_arch = "wasm32")]
        false
    }

    /// Bring in a program from a file (see `crate::programs::import`) and make the form that program.
    pub fn import_program(&mut self, path: &std::path::Path) -> bool {
        match crate::programs::import(path) {
            Ok(name) => {
                self.grow.import_note = format!("Imported {name}");
                self.grow.imported = crate::programs::imported();
                self.record();
                if self.form != PROGRAM {
                    self.form = PROGRAM;
                    self.preset();
                }
                self.usual_program(&name, true);
                true
            }
            Err(why) => {
                self.grow.import_note = why;
                false
            }
        }
    }

    /// A file dropped on the window: taken, if it is a program's (`.xml`). Returns whether it was one.
    pub fn drop_file(&mut self, path: &std::path::Path) -> bool {
        let ours = path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("xml"));
        if ours && !self.import_program(path) {
            self.alert = Some(("Not imported", self.grow.import_note.clone()));
        }
        ours
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use brickloft_core::report::{parse_spec, spec_text};
    use brickloft_core::spec::{build, Spec};
    use brickloft_core::DVec3;

    /// Frames as the window takes them, sixty a second by the clock they
    /// are given, until `until` holds: the change is noted for undo, the
    /// run is heard and asked, then the form is built.
    fn frames(state: &mut State, clock: &mut f64, until: impl Fn(&State) -> bool) {
        for _ in 0..60_000 {
            *clock += 16.0;
            let now = *clock;
            // (and a change is noted as a step to undo, as the window notes one at every frame in which nothing is held)
            state.record();
            state.grow_frame(now);
            state.refresh_behind(move || now);
            if until(state) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        panic!("the run did not get there: {:?}, growing {}, fitting {}, problem {:?}", state.spec.program, state.growing(), state.fitting(), state.problem);
    }

    /// At rest: no run going, nothing being built, and the bricks those of the form as it stands.
    fn settled(state: &State) -> bool {
        !state.growing() && !state.fitting() && state.built_from.as_ref().is_some_and(|from| from.program == state.spec.program)
    }

    /// An editor on a rule program, small enough to run in a moment, watched from its first turn.
    fn watching(name: &str, size: [i32; 3], seed: u64) -> State {
        let mut state = State::new();
        state.form = PROGRAM;
        state.preset();
        state.usual_program(name, false);
        (state.spec.program.size, state.spec.program.seed) = (size, seed);
        state.grow_changed();
        // (slowly: the most turns one frame asks for is a quarter of a second's worth, however long a busy machine takes over the frame, and a run is to be caught part way)
        state.grow.speed = 60.0;
        state
    }

    #[test]
    fn a_run_watched_is_at_every_pause_the_form_a_save_of_it_builds() {
        let (mut state, mut clock) = (watching("Apartemazements", [6, 6, 6], 11), 0.0);
        // chosen from the list, it is in the program's own colours and playing from an empty grid
        assert_eq!(state.paint.mode, crate::paint::Mode::Pattern);
        assert_eq!(state.paint.colours.len(), form::colours(&state.spec.program).unwrap().len());
        assert!(state.growing() && state.spec.program.stop == Some(0));
        state.busy = false;
        let steps_before = state.undone.len();
        // some turns in, paused
        frames(&mut state, &mut clock, |state| state.spec.program.stop.is_some_and(|turn| turn >= 30));
        state.grow_pause();
        frames(&mut state, &mut clock, settled);
        let paused = state.full_spec();
        let turn = paused.program.stop.expect("the run ended before it was paused");
        assert!(turn >= 30 && !state.built.bricks.is_empty() && state.problem.is_none());
        // the same settings, with nothing remembered, are the same bricks: from the spec, and from a save's text of it
        form::forget_all();
        let again = build(&paused).unwrap();
        assert!(state.built.bricks == again.bricks && state.built.shades == again.shades, "{} bricks on screen, {} from the settings", state.built.bricks.len(), again.bricks.len());
        form::forget_all();
        assert_eq!(build(&parse_spec(&spec_text(&paused)).unwrap()).unwrap().bricks, again.bricks);
        // a step is one turn more
        state.grow_step();
        frames(&mut state, &mut clock, |state| settled(state) && state.spec.program.stop != Some(turn));
        assert_eq!(state.spec.program.stop, Some(turn + 1));
        // played on, it ends; and what stands is the form run to its end from nothing
        state.grow.speed = 2000.0;
        state.grow_play();
        frames(&mut state, &mut clock, |state| settled(state) && state.spec.program.stop.is_none());
        let ended = state.full_spec();
        form::forget_all();
        let whole = build(&ended).unwrap();
        assert!(!whole.bricks.is_empty() && whole.bricks != again.bricks);
        assert!(state.built.bricks == whole.bricks && state.built.shades == whole.shades, "{} bricks on screen, {} from the settings", state.built.bricks.len(), whole.bricks.len());
        assert!(state.grow_outcome().is_none_or(|outcome| !outcome.running));
        // while it ran nothing was noted to undo: the pause and the end are steps, and no frame between is
        state.record();
        assert!(state.undone.len() <= steps_before + 3, "{} steps to undo for one run", state.undone.len() - steps_before);
        // Play on a run that has ended begins it again; Restart does so at any time; To the end ends it without the frames between
        state.grow.speed = 60.0;
        state.grow_play();
        assert!(state.spec.program.stop == Some(0) && state.growing());
        frames(&mut state, &mut clock, |state| state.spec.program.stop.is_some_and(|turn| turn >= 5));
        state.grow_restart();
        assert_eq!(state.spec.program.stop, Some(0));
        frames(&mut state, &mut clock, |state| state.spec.program.stop.is_some_and(|turn| turn >= 5));
        state.grow_finish();
        frames(&mut state, &mut clock, |state| settled(state) && state.spec.program.stop.is_none());
        assert_eq!(state.built.bricks, whole.bricks);
    }

    #[test]
    fn a_change_made_from_outside_ends_the_run_and_the_form_is_what_it_was_changed_to() {
        let (mut state, mut clock) = (watching("Escher", [5, 5, 5], 2), 0.0);
        state.busy = false;
        // (some way in, or already ended: a run this small may be over in the turns one frame asks for)
        frames(&mut state, &mut clock, |state| state.spec.program.stop != Some(0));
        // as an undo, a save opened or Claude would: the form is another seed, run to its end
        let mut spec = state.full_spec();
        (spec.program.seed, spec.program.stop) = (9, None);
        state.load_spec(spec.clone());
        frames(&mut state, &mut clock, settled);
        assert!(!state.grow.playing && state.spec.program == spec.program.clamped());
        form::forget_all();
        assert_eq!(state.built.bricks, build(&spec).unwrap().bricks);
        // and another form chosen is no longer a run at all
        state.grow_restart();
        state.form = super::super::CUBE;
        state.preset();
        frames(&mut state, &mut clock, |state| !state.fitting());
        assert!(!state.growing() && state.built.bricks.len() == 1);
    }

    #[test]
    fn a_seed_passed_over_is_said_and_a_run_stopped_at_its_limit_keeps_its_bricks() {
        // ModernHouse's first four seeds cannot be finished: watched, the run begins again four times, and the form ends as seed 5
        let (mut state, mut clock) = (watching("ModernHouse", [9, 9, 4], 1), 0.0);
        state.grow.speed = 20_000.0;
        state.busy = false;
        frames(&mut state, &mut clock, |state| settled(state) && state.spec.program.stop.is_none());
        assert_eq!(state.spec.program.seed, 5);
        let note = state.grow_note().expect("nothing was said of the seeds passed over").to_string();
        assert!(note.contains("Seed 4 was passed over and seed 5 is shown") || note.contains("were passed over and seed 5 is shown"), "{note}");
        assert!(state.problem.is_none() && !state.built.bricks.is_empty());
        // (the note is of that run: another seed chosen, it is no longer said)
        state.spec.program.seed = 6;
        state.grow_changed();
        assert_eq!(state.grow_note(), None);
        // shown finished and not watched, the same seed 1 is the same building, and says which seeds it passed
        let mut unwatched = State::new();
        unwatched.grow.watch = false;
        unwatched.form = PROGRAM;
        unwatched.preset();
        unwatched.usual_program("ModernHouse", false);
        unwatched.spec.program.seed = 1;
        unwatched.grow_changed();
        assert_eq!(unwatched.spec.program.stop, None);
        let mut clock = 0.0;
        frames(&mut unwatched, &mut clock, |state| settled(state) && state.grow_note().is_some());
        assert!(unwatched.grow_note().unwrap().contains("Seeds 1 to 4 were passed over and seed 5 is shown"), "{:?}", unwatched.grow_note());
        let house = |seed| Spec { program: Program { seed, ..unwatched.spec.program.clone() }, ..unwatched.full_spec() };
        assert_eq!(unwatched.built.bricks, build(&house(5)).unwrap().bricks);

        // a run that reaches its step limit stops, keeps what it made, and says so where a problem is said
        let (mut limited, mut clock) = (watching("Apartemazements", [6, 6, 6], 4), 0.0);
        limited.spec.program.limit = 30;
        limited.grow_changed();
        limited.busy = false;
        frames(&mut limited, &mut clock, |state| settled(state) && state.spec.program.stop.is_none() && state.grow.caution.is_some());
        let caution = limited.grow.caution.clone().unwrap();
        assert!(caution.contains("Stopped early") && caution.contains("30 steps") && caution.contains("Raise the step limit"), "{caution}");
        assert!(limited.problem.is_none() && !limited.built.bricks.is_empty());
    }

    #[test]
    fn a_long_turn_is_taken_behind_the_window_and_no_frame_waits_for_it() {
        // PillarsOfEternity's wfc node searches for a seed that fills the grid in its first turn: a second or so of work
        let (mut state, mut clock) = (watching("PillarsOfEternity", [9, 9, 9], 18), 0.0);
        state.busy = false;
        let (began, mut longest, mut said_working) = (std::time::Instant::now(), std::time::Duration::ZERO, false);
        for _ in 0..200_000 {
            clock += 16.0;
            let (now, at) = (clock, std::time::Instant::now());
            state.grow_frame(now);
            state.refresh_behind(move || now);
            longest = longest.max(at.elapsed());
            said_working |= state.grow.working;
            if settled(&state) && state.spec.program.stop.is_none() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(state.spec.program.stop.is_none() && !state.built.bricks.is_empty(), "the run did not end: {:?}", state.problem);
        // (a frame builds the grid's bricks, which is milliseconds; a busy machine is allowed a tenth of a second)
        assert!(longest < std::time::Duration::from_millis(100), "a frame took {longest:?}");
        // where the run took long enough for anyone to wonder, the panel was told it was working
        assert!(said_working || began.elapsed() < std::time::Duration::from_millis(600), "a run of {:?} never said it was working", began.elapsed());
    }

    /// Not a test: what a frame costs while a program runs, for each
    /// program that comes with the editor at its usual size. The frame's
    /// share is hearing the run and building the grid's bricks; the turns
    /// are another thread's. Run by name with `--release -- --ignored --nocapture`.
    #[test]
    #[ignore = "a measurement, not a check"]
    fn how_long_a_frame_of_a_run_takes() {
        for entry in &form::BUNDLED {
            let mut state = watching(entry.name, entry.size, 5);
            (state.grow.speed, state.busy) = (240.0, false);
            let (began, mut frames, mut building, mut longest, mut built, mut tiles) = (std::time::Instant::now(), 0u32, std::time::Duration::ZERO, std::time::Duration::ZERO, 0u32, crate::tiles::Tiles::default());
            let (mut taking, mut most_bricks) = (std::time::Duration::ZERO, 0);
            loop {
                let now = began.elapsed().as_secs_f64() * 1000.0;
                let at = std::time::Instant::now();
                state.grow_frame(now);
                let rebuilt = state.refresh_behind(move || now);
                let took = at.elapsed();
                if rebuilt {
                    (built, building, longest) = (built + 1, building + took, longest.max(took));
                    // (and what the view then does with them: finds which tiles changed)
                    let at = std::time::Instant::now();
                    std::hint::black_box(tiles.take(state.built.bricks.iter().map(|brick| (*brick, [1, 2, 3]))));
                    taking += at.elapsed();
                    most_bricks = most_bricks.max(state.built.bricks.len());
                }
                frames += 1;
                if settled(&state) && state.spec.program.stop.is_none() {
                    break;
                }
                // (a frame at sixty a second)
                std::thread::sleep(std::time::Duration::from_millis(16).saturating_sub(at.elapsed()));
            }
            let turns = state.grow_outcome().map_or(0, |outcome| outcome.turns);
            println!("{}: {turns} turns in {:.1} s over {frames} frames, {built} of them with new bricks: {:.2} ms a frame to hear the run and build (at most {:.1}), {:.2} ms to find the tiles that changed; {} bricks at the most", entry.name, began.elapsed().as_secs_f64(), building.as_secs_f64() * 1e3 / built.max(1) as f64, longest.as_secs_f64() * 1e3, taking.as_secs_f64() * 1e3 / built.max(1) as f64, most_bricks);
        }
    }

    #[test]
    fn a_save_opened_runs_its_other_forms_behind_the_window_and_a_missing_program_opens_empty_and_says_so() {
        let cube = Spec { form: Form::Cube, points: vec![DVec3::new(0.5, 0.5, 4.5)], radius: 4.5, fill: true, ..Spec::default() };
        let flats = Spec { form: Form::Program, points: vec![DVec3::new(0.5, 0.5, 0.5)], program: Program { size: [5, 5, 5], seed: 31, ..Program::of("Apartemazements", false) }, ..Spec::default() };
        let gone = Spec { form: Form::Program, points: vec![DVec3::new(0.5, 0.5, 0.5)], program: Program::of("Never brought in", true), ..Spec::default() };
        let paint = crate::paint::Paint::default();
        let mut state = State::new();
        form::forget_all();
        state.open(vec![(cube, paint.clone(), [0; 3]), (flats.clone(), paint.clone(), [40, 0, 0]), (gone.clone(), paint.clone(), [200, 0, 0])]);
        // not run where the save was opened
        assert!(state.others[0].built.bricks.is_empty());
        let mut clock = 0.0;
        frames(&mut state, &mut clock, |state| !state.others[0].built.bricks.is_empty() && !state.grow_waiting());
        assert_eq!(state.others[0].built.bricks, build(&flats).unwrap().bricks);
        assert!(state.others[0].bounds.is_some() && state.others[1].built.bricks.is_empty());
        // the form whose program is not here: empty, and it says what to do about it
        state.activate(1);
        frames(&mut state, &mut clock, |state| !state.fitting() && state.problem.is_some());
        let problem = state.problem.clone().unwrap();
        assert!(state.built.bricks.is_empty() && problem.contains("\"Never brought in\"") && problem.contains("Import its .xml file again"), "{problem}");
    }
}
