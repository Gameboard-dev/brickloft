//! What the editor is editing: a form's spec, the handles that can be
//! dragged to change it, and the bricks last built from it.

use crate::paint::Paint;
use arcs_core::curves::{ArchType, Circle};
use arcs_core::shapes::CellBrick;
use arcs_core::spec::{build, snap, Built, Form, Spec};
use arcs_core::DVec3;
use std::cell::RefCell;
use std::rc::Rc;

/// The forms on offer, in menu order. `Form` itself carries the arch type,
/// so the menu works on this index and the arch type is kept beside it.
pub const FORMS: [&str; 8] = ["Arch", "Arc through 3 points", "Circle", "Spline", "Bézier", "Sphere", "Torus", "Revolve"];
pub const ARCHES: [(&str, ArchType); 6] = [
    ("Round", ArchType::Round),
    ("Pointed", ArchType::Pointed),
    ("Triangular", ArchType::Triangular),
    ("Elliptical", ArchType::Elliptical),
    ("Parabolic", ArchType::Parabolic),
    ("Catenary", ArchType::Catenary),
];

/// The furthest a handle may be dragged from the origin, and the largest a
/// radius or rise may be dragged to, in microbricks. Well inside what the
/// core will build (`arcs_core::spec::MAX_SPAN`), so that no drag can ask
/// for a form it would refuse.
pub const HANDLE_LIMIT: f64 = 400.0;

/// Which coordinate stays fixed while a handle is dragged. `Free` moves it
/// in the plane facing the camera.
#[derive(Clone, Copy, PartialEq)]
pub enum Lock {
    Free,
    Axis(usize),
}

#[derive(Clone, Copy, PartialEq)]
pub enum Cut {
    Whole,
    Dome,
    Segment,
}

/// One number the best-fit search may change.
#[derive(Clone, Copy)]
enum Tune {
    /// a point (by index) along an axis
    Point(usize, usize),
    Radius,
    Rise,
}

/// The cells a build occupies: its low corner and its high one.
pub type Bounds = ([i32; 3], [i32; 3]);

fn bounds_of(built: &Built) -> Option<Bounds> {
    let lo = |i: usize| built.bricks.iter().map(|b| b.min[i]).min();
    let hi = |i: usize| built.bricks.iter().map(|b| b.min[i] + b.ext[i]).max();
    (!built.bricks.is_empty()).then(|| ([0, 1, 2].map(|i| lo(i).unwrap()), [0, 1, 2].map(|i| hi(i).unwrap())))
}

/// A form in the workspace that is not the one being edited: what it is,
/// how it is coloured, and the bricks last built from it.
pub struct Item {
    pub spec: Spec,
    pub paint: Paint,
    pub built: Built,
    pub bounds: Option<Bounds>,
}

impl Item {
    /// What to call it in the list.
    pub fn name(&self) -> &'static str {
        FORMS[form_index(self.spec.form)]
    }
}

/// Where a form is in the menu.
fn form_index(form: Form) -> usize {
    match form {
        Form::Arch(_) => 0,
        Form::Arc => 1,
        Form::Circle => 2,
        Form::Spline => 3,
        Form::Bezier => 4,
        Form::Sphere => 5,
        Form::Torus => 6,
        Form::Revolve => 7,
    }
}

/// A brick of the workspace: the brick, its colour (`None` for the player's
/// own in the game), and which form it belongs to (0 is the one being
/// edited, then the others in order).
pub type Painted = (CellBrick, Option<[u8; 3]>, usize);

/// Something in the view that can be dragged.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Handle {
    /// moves the whole form
    Move,
    /// a control point
    Point(usize),
    /// pulls a circle, sphere or torus wider or narrower
    Radius,
    /// pulls an arch's crown up or down
    Rise,
}

pub struct State {
    pub spec: Spec,
    pub form: usize,
    pub arch: usize,
    pub lock: Lock,
    /// the point of the form being edited that a drag has hold of, while it
    /// is being moved
    pub grab: Option<DVec3>,
    pub cut: Cut,
    /// a segment's two heights, relative to the sphere's centre
    pub segment: (f64, f64),
    /// snap handles to half cells (single game units) instead of whole cells
    pub fine_snap: bool,
    pub built: Built,
    pub build_ms: f64,
    pub problem: Option<String>,
    /// what the last send came to; shared with the browser callback that learns it
    pub status: Rc<RefCell<String>>,
    /// where the last report went, or why it could not be made
    pub report_note: String,
    /// A notice to be read, as its heading and its text: shown over the view
    /// until dismissed.
    pub alert: Option<(&'static str, String)>,
    /// how the form being edited is coloured
    pub paint: Paint,
    /// the workspace's other forms; the one being edited is the fields above
    pub others: Vec<Item>,
    /// Counts changes to what is in view that are not a rebuild of the form
    /// being edited (colours, forms added, removed or swapped), so the view
    /// knows to redraw.
    pub scene: u64,
    /// whether opening a saved form adds it to the workspace or replaces
    /// the form being edited
    pub open_adds: bool,
    /// where the panel ends, in screen points: a saved form dragged past it
    /// is being dropped on the view
    pub panel_right: f32,
    /// whether the rulers are drawn
    pub show_ruler: bool,
    /// the box of cells the bricks occupy, low corner and high, for the rulers
    pub bounds: Option<Bounds>,
    /// How far below the form's lowest brick the floor is drawn, in
    /// microbricks. Only a matter of the view: it changes nothing that is built.
    pub floor_gap: f64,
    /// after each drag, move what was dragged to the best fit nearby
    pub auto_best: bool,
    /// what the last best-fit search did
    pub best_note: String,
    /// saving: the folder chosen, the name typed, a new folder's name typed,
    /// and what the last save or open came to
    pub folder: String,
    pub save_name: String,
    pub new_folder: String,
    pub store_note: String,
    /// the folders and the chosen folder's forms, read when they may have
    /// changed and not on every frame
    pub folders: Vec<String>,
    pub forms: Vec<String>,
    built_from: Option<Spec>,
    /// the workspace as it was before each change, oldest first, and after
    /// each change undone, for Ctrl+Z and Ctrl+Y
    undone: Vec<Snapshot>,
    redone: Vec<Snapshot>,
    /// the workspace as last noted by [`State::record`]
    noted: Snapshot,
}

/// Everything in the workspace that an edit can change: each form and how it
/// is coloured, the one being edited first. Bricks are not kept; they are
/// built again from the form, which keeps a long history small.
type Snapshot = Vec<(Spec, Paint)>;

/// How many changes can be undone. Each is a few hundred bytes.
const HISTORY: usize = 200;

/// Centre of the cell with these whole coordinates.
fn cell(x: f64, y: f64, z: f64) -> DVec3 {
    DVec3::new(x + 0.5, y + 0.5, z + 0.5)
}

impl State {
    pub fn new() -> State {
        let mut state = State {
            spec: Spec::default(),
            form: 0,
            arch: 0,
            lock: Lock::Free,
            grab: None,
            cut: Cut::Whole,
            segment: (2.0, 6.0),
            fine_snap: false,
            built: Built::default(),
            build_ms: 0.0,
            problem: None,
            status: Rc::default(),
            report_note: String::new(),
            alert: None,
            paint: Paint::default(),
            others: Vec::new(),
            scene: 0,
            open_adds: false,
            panel_right: 0.0,
            show_ruler: true,
            bounds: None,
            floor_gap: 0.0,
            auto_best: false,
            best_note: String::new(),
            folder: crate::store::STARTER_FOLDERS[0].to_string(),
            save_name: String::new(),
            new_folder: String::new(),
            store_note: String::new(),
            folders: Vec::new(),
            forms: Vec::new(),
            built_from: None,
            undone: Vec::new(),
            redone: Vec::new(),
            noted: Vec::new(),
        };
        // The editor's starting point balances fit against brick count: a
        // brick must bring the fit a quarter of a microbrick closer to be
        // used, and the brick size is chosen to keep within a budget. The
        // library's own defaults (in `Spec`) are the closest fit regardless.
        state.spec.simplify = 0.25;
        state.spec.grain = 0;
        state.spec.budget = 2500;
        state.preset();
        state.list_saved();
        state
    }

    /// Read the folders and the chosen folder's forms again.
    pub fn list_saved(&mut self) {
        self.folders = crate::store::folders();
        self.forms = crate::store::forms(&self.folder);
    }

    /// The form being edited, as an item to set aside.
    fn as_item(&self) -> Item {
        Item { spec: self.full_spec(), paint: self.paint.clone(), built: self.built.clone(), bounds: self.bounds }
    }

    /// Make `item` the form being edited, with the bricks it already has.
    fn take_item(&mut self, item: Item) {
        self.load_spec(item.spec);
        self.paint = item.paint;
        self.built = item.built;
        self.bounds = item.bounds;
        self.problem = None;
        // its bricks are current: nothing to rebuild until it is changed
        self.built_from = Some(self.full_spec());
        self.scene += 1;
    }

    /// Edit another of the workspace's forms; the one being edited takes its place.
    pub fn activate(&mut self, other: usize) {
        if other < self.others.len() {
            let mine = self.as_item();
            let item = std::mem::replace(&mut self.others[other], mine);
            self.take_item(item);
        }
    }

    /// Add a form to the workspace and edit it. It is set down clear of the
    /// forms already there, to their +x side, so that nothing is hidden.
    pub fn add_form(&mut self, mut spec: Spec) {
        let taken = self.others.iter().filter_map(|i| i.bounds).chain(self.bounds).map(|(_, hi)| hi[0]).max();
        if let (Some(right), Ok(built)) = (taken, build(&spec)) {
            if let Some((lo, _)) = bounds_of(&built) {
                // four microbricks of air between them
                let shift = (right + 4 - lo[0]) as f64;
                for p in &mut spec.points {
                    p.x += shift;
                }
            }
        }
        let mine = self.as_item();
        self.others.push(mine);
        self.load_spec(spec);
        self.paint = Paint::default();
        self.built_from = None;
        self.scene += 1;
    }

    /// Make the form being edited larger or smaller by `cells` of radius,
    /// if it is one with a single size all round: a circle, a sphere or a
    /// torus. Returns whether it is. By whole cells, so that a radius that
    /// ends on the lattice still does.
    pub fn resize(&mut self, cells: f64) -> bool {
        let round = matches!(self.form, 2 | 5 | 6);
        if round {
            self.spec.radius = (self.spec.radius + cells).clamp(0.5, HANDLE_LIMIT);
        }
        round
    }

    /// Every form in the workspace with its colouring, the one being edited first.
    pub fn forms(&self) -> Vec<(Spec, Paint)> {
        self.snapshot()
    }

    /// Add copies of forms to the workspace, each set down clear of what is there.
    pub fn paste(&mut self, forms: Vec<(Spec, Paint)>) {
        for (spec, paint) in forms {
            self.add_form(spec);
            self.paint = paint;
            // built now, so that the next one is set down clear of this one too
            self.refresh(|| 0.0);
        }
    }

    fn snapshot(&self) -> Snapshot {
        std::iter::once((self.full_spec(), self.paint.clone())).chain(self.others.iter().map(|i| (i.spec.clone(), i.paint.clone()))).collect()
    }

    /// Note the workspace if it has changed since it was last noted, as a
    /// step that can be undone. Called once a frame while nothing is being
    /// dragged, so that a whole drag is one step and not one per pixel.
    pub fn record(&mut self) {
        let now = self.snapshot();
        if now == self.noted {
            return;
        }
        // Choosing another form to edit only reorders the workspace: that is
        // not a change to undo.
        let same_forms = now.len() == self.noted.len() && now.iter().all(|f| self.noted.contains(f)) && self.noted.iter().all(|f| now.contains(f));
        if !same_forms && !self.noted.is_empty() {
            self.undone.push(std::mem::take(&mut self.noted));
            if self.undone.len() > HISTORY {
                self.undone.remove(0);
            }
            self.redone.clear();
        }
        self.noted = now;
    }

    /// Put the workspace back as a snapshot has it.
    fn restore(&mut self, snapshot: Snapshot) {
        let mut forms = snapshot.clone().into_iter();
        let Some((spec, paint)) = forms.next() else { return };
        self.others = forms
            .map(|(spec, paint)| {
                let built = build(&spec).unwrap_or_default();
                Item { bounds: bounds_of(&built), spec, paint, built }
            })
            .collect();
        self.load_spec(spec);
        self.paint = paint;
        self.built_from = None;
        self.scene += 1;
        self.noted = snapshot;
    }

    /// Take back the last change. Returns whether there was one.
    pub fn undo(&mut self) -> bool {
        self.record();
        let Some(before) = self.undone.pop() else { return false };
        self.redone.push(self.snapshot());
        self.restore(before);
        true
    }

    /// Make again the change last taken back.
    pub fn redo(&mut self) -> bool {
        let Some(after) = self.redone.pop() else { return false };
        self.undone.push(self.snapshot());
        self.restore(after);
        true
    }

    /// Take the form being edited out of the workspace. The last one stays.
    pub fn remove_active(&mut self) {
        if let Some(item) = self.others.pop() {
            self.take_item(item);
        }
    }

    /// Every brick in the workspace, with its colour and its form.
    pub fn painted(&self) -> Vec<Painted> {
        let mut all = Vec::new();
        let forms = std::iter::once((&self.built, &self.paint, self.bounds)).chain(self.others.iter().map(|i| (&i.built, &i.paint, i.bounds)));
        for (form, (built, paint, bounds)) in forms.enumerate() {
            let Some((lo, hi)) = bounds else { continue };
            all.extend(built.bricks.iter().map(|b| (*b, paint.colour(b, lo, hi), form)));
        }
        all
    }

    /// Take on a saved form: its spec, and the menu choices that go with it.
    pub fn load_spec(&mut self, spec: Spec) {
        self.form = form_index(spec.form);
        if let Form::Arch(kind) = spec.form {
            self.arch = ARCHES.iter().position(|a| a.1 == kind).unwrap_or(0);
        }
        (self.cut, self.segment) = match spec.cut {
            None => (Cut::Whole, self.segment),
            Some((_, hi)) if hi == f64::INFINITY => (Cut::Dome, self.segment),
            Some(ends) => (Cut::Segment, ends),
        };
        self.spec = spec;
    }

    /// A number the best-fit search may change: one coordinate of a point,
    /// the radius, or an arch's rise.
    fn tunable(&self, only: Option<Handle>) -> Vec<Tune> {
        let mut all = Vec::new();
        let wanted = |h: Handle| only.is_none_or(|o| o == h);
        for i in 0..self.spec.points.len() {
            for axis in 0..3 {
                // a coordinate the drag lock holds still stays still here too
                if wanted(Handle::Point(i)) && self.lock != Lock::Axis(axis) {
                    all.push(Tune::Point(i, axis));
                }
            }
        }
        if matches!(self.form, 2 | 5 | 6) && wanted(Handle::Radius) {
            all.push(Tune::Radius);
        }
        if self.form == 0 && wanted(Handle::Rise) {
            all.push(Tune::Rise);
        }
        all
    }

    fn tune(&mut self, what: Tune, by: f64) {
        match what {
            Tune::Point(i, axis) => self.spec.points[i][axis] += by,
            Tune::Radius => self.spec.radius = (self.spec.radius + by).clamp(0.5, HANDLE_LIMIT),
            Tune::Rise => self.spec.rise = Some((self.arch_rise() + by).clamp(0.5, HANDLE_LIMIT)),
        }
    }

    /// How badly the current settings fit: the discrepancy as a share of the
    /// form's own volume. `None` if they do not build at all.
    fn misfit(&self) -> Option<f64> {
        let built = build(&self.full_spec()).ok()?;
        (built.solid > 0.0 && !built.bricks.is_empty()).then(|| built.error / built.solid)
    }

    /// Move to the best fit within a cell of here.
    ///
    /// A form fits cleanly only where its surfaces fall on the grid: an arch
    /// whose radius comes to 8.3 has walls a third of a cell off every brick
    /// edge all the way round, and at radius 8 they sit exactly on them. Which
    /// values those are depends on the form's thickness and proportions, so
    /// they are searched for instead of worked out. Each tunable number in
    /// turn is tried a half and a whole cell either side and left at whichever
    /// fits best, and the round is repeated until nothing improves: a search
    /// of the neighbourhood one number at a time, which finds the nearest
    /// good fit and not some far better one elsewhere.
    ///
    /// `only` limits it to one handle's numbers (the one just dragged).
    pub fn snap_best(&mut self, only: Option<Handle>) {
        let Some(start) = self.misfit() else { return };
        let mut best = start;
        // a slow form gets one round, so that the editor does not stall
        let rounds = if self.build_ms > 60.0 { 1 } else { 3 };
        for _ in 0..rounds {
            let mut improved = false;
            for what in self.tunable(only) {
                let steps: &[f64] = match what {
                    // points sit on cell centres unless half steps are asked for
                    Tune::Point(..) if !self.fine_snap => &[-1.0, 1.0],
                    _ => &[-1.0, -0.5, 0.5, 1.0],
                };
                let before = self.spec.clone();
                let mut choice = None;
                for &by in steps {
                    self.tune(what, by);
                    // a clear gain only: never wander off for the sake of noise
                    if let Some(fit) = self.misfit().filter(|f| *f < best * 0.97 - 1e-9) {
                        best = fit;
                        choice = Some(self.spec.clone());
                    }
                    self.spec = before.clone();
                }
                if let Some(spec) = choice {
                    self.spec = spec;
                    improved = true;
                }
            }
            if !improved {
                break;
            }
        }
        self.best_note = if best < start {
            format!("Moved to a closer fit: discrepancy {:.1}% -> {:.1}%", start * 100.0, best * 100.0)
        } else {
            "This is already the best fit within a microbrick of here.".into()
        };
    }

    /// Start the chosen form from a sensible example of itself.
    pub fn preset(&mut self) {
        let s = &mut self.spec;
        s.weights.clear();
        s.rise = None;
        s.closed = false;
        s.tube = false;
        s.fill = false;
        self.cut = Cut::Whole;
        self.lock = Lock::Free;
        // Curves stand on the floor, as they will in the game. Solids start
        // lifted clear of it, so that the camera can get underneath and look
        // up into a dome or a hollow shell.
        self.floor_gap = if self.form >= 5 { 8.0 } else { 0.0 };
        match self.form {
            0 => {
                s.points = vec![cell(-8.0, 0.0, 0.0), cell(8.0, 0.0, 0.0)];
                self.lock = Lock::Axis(1); // an arch stays in its upright plane
            }
            1 => {
                s.points = vec![cell(-8.0, 0.0, 0.0), cell(0.0, 0.0, 6.0), cell(8.0, 0.0, 0.0)];
                self.lock = Lock::Axis(1);
            }
            2 => s.points = vec![cell(0.0, 0.0, 0.0)],
            3 => s.points = vec![cell(-10.0, 0.0, 0.0), cell(-4.0, 0.0, 6.0), cell(4.0, 0.0, 2.0), cell(10.0, 0.0, 8.0)],
            4 => {
                s.points = vec![cell(-8.0, 0.0, 0.0), cell(0.0, 0.0, 12.0), cell(8.0, 0.0, 0.0)];
                s.weights = vec![1.0, 1.0, 1.0];
            }
            5 => {
                s.points = vec![cell(0.0, 0.0, s.radius.ceil())];
                s.fill = true;
            }
            6 => s.points = vec![cell(0.0, 0.0, 1.0)],
            _ => s.points = vec![cell(0.0, 0.0, 10.0), cell(5.0, 0.0, 8.0), cell(8.0, 0.0, 0.0)],
        }
        if matches!(self.form, 3 | 4 | 7) {
            self.lock = Lock::Axis(1);
        }
    }

    /// The spec with the menu's choices folded in.
    pub fn full_spec(&self) -> Spec {
        let mut s = self.spec.clone();
        s.form = match self.form {
            0 => Form::Arch(ARCHES[self.arch].1),
            1 => Form::Arc,
            2 => Form::Circle,
            3 => Form::Spline,
            4 => Form::Bezier,
            5 => Form::Sphere,
            6 => Form::Torus,
            _ => Form::Revolve,
        };
        s.cut = match self.cut {
            Cut::Whole => None,
            // from the bottom face of the centre's cell, so the dome sits flat
            Cut::Dome => Some((-0.5, f64::INFINITY)),
            Cut::Segment => Some((self.segment.0.min(self.segment.1), self.segment.0.max(self.segment.1))),
        };
        s
    }

    /// Build again if anything changed since the last build. Returns whether it did.
    pub fn refresh(&mut self, now_ms: impl Fn() -> f64) -> bool {
        let spec = self.full_spec();
        if self.built_from.as_ref() == Some(&spec) {
            return false;
        }
        let start = now_ms();
        match build(&spec) {
            Ok(built) => {
                // worked out once per build, not once per frame
                self.bounds = bounds_of(&built);
                self.built = built;
                self.problem = None;
            }
            // keep showing the last good build under the message
            Err(e) => self.problem = Some(e),
        }
        self.build_ms = now_ms() - start;
        self.built_from = Some(spec);
        true
    }

    pub fn arch_rise(&self) -> f64 {
        let p = &self.spec.points;
        self.spec.rise.unwrap_or_else(|| ARCHES[self.arch].1.default_rise(p[0].distance(p[1]) / 2.0))
    }

    /// The direction the radius handle sits in, from the centre.
    fn radius_dir(&self) -> DVec3 {
        match self.form {
            2 => Circle::axis_aligned(DVec3::ZERO, 1.0, self.spec.plane.min(2)).u,
            _ => DVec3::X,
        }
    }

    pub fn handles(&self) -> Vec<(Handle, DVec3)> {
        let p = &self.spec.points;
        let mut all: Vec<(Handle, DVec3)> = p.iter().enumerate().map(|(i, q)| (Handle::Point(i), *q)).collect();
        match self.form {
            0 if p.len() >= 2 => all.push((Handle::Rise, (p[0] + p[1]) / 2.0 + DVec3::Z * self.arch_rise())),
            2 | 5 | 6 if !p.is_empty() => all.push((Handle::Radius, p[0] + self.radius_dir() * self.spec.radius)),
            _ => {}
        }
        all
    }

    /// Move `handle` to where the ray from the eye through the cursor meets
    /// its drag plane.
    pub fn drag(&mut self, handle: Handle, origin: DVec3, dir: DVec3, view: DVec3) {
        // a form is moved by the point of it that was taken hold of; it has no dot
        let held = if handle == Handle::Move { self.grab } else { self.handles().iter().find(|(h, _)| *h == handle).map(|h| h.1) };
        let Some(at) = held else { return };
        let normal = match (handle, self.lock) {
            (Handle::Point(_) | Handle::Move, Lock::Axis(a)) => [DVec3::X, DVec3::Y, DVec3::Z][a],
            _ => view,
        };
        let facing = dir.dot(normal);
        if facing.abs() < 1e-6 {
            return; // looking along the plane: the cursor does not pick a point on it
        }
        let along = (at - origin).dot(normal) / facing;
        // Towards the horizon the cursor's ray runs almost along the drag
        // plane and meets it very far away (or, above the horizon, behind
        // the camera): one more pixel of mouse travel would fling the handle
        // thousands of cells. A handle may move at most a few times its own
        // distance from the eye in one go; beyond that the drag is ignored
        // and the handle stays where it was until the cursor comes back.
        let reach = at.distance(origin);
        if !(along > 0.0 && along <= 4.0 * reach.max(15.0)) {
            return;
        }
        let hit = origin + dir * along;
        // and never further from the origin than this, however it got there
        let limit = HANDLE_LIMIT;
        let hit = hit.clamp(DVec3::splat(-limit), DVec3::splat(limit));
        let step = |v: f64| if self.fine_snap { snap(v) } else { v.floor() + 0.5 };
        match handle {
            Handle::Point(i) => {
                let mut to = DVec3::new(step(hit.x), step(hit.y), step(hit.z));
                if let Lock::Axis(a) = self.lock {
                    to[a] = self.spec.points[i][a]; // exactly, whatever rounding did
                }
                self.spec.points[i] = to;
            }
            Handle::Move => {
                // the whole form, by whole microbricks so that it stays on
                // the grid it was fitted to
                let mut by = DVec3::new((hit.x - at.x).round(), (hit.y - at.y).round(), (hit.z - at.z).round());
                if let Lock::Axis(a) = self.lock {
                    by[a] = 0.0;
                }
                for p in &mut self.spec.points {
                    *p = (*p + by).clamp(DVec3::splat(-HANDLE_LIMIT), DVec3::splat(HANDLE_LIMIT));
                }
                // what is held moves with the form
                self.grab = Some(at + by);
            }
            Handle::Radius => {
                let out = (hit - self.spec.points[0]).dot(self.radius_dir());
                self.spec.radius = snap(out.abs()).clamp(0.5, HANDLE_LIMIT);
            }
            Handle::Rise => {
                let mid = (self.spec.points[0] + self.spec.points[1]) / 2.0;
                self.spec.rise = Some(snap(hit.z - mid.z).clamp(0.5, HANDLE_LIMIT));
            }
        }
    }

    pub fn can_add_points(&self) -> bool {
        matches!(self.form, 3 | 4 | 7)
    }

    /// Add a point a little beyond the last one.
    pub fn add_point(&mut self) {
        let p = &mut self.spec.points;
        let n = p.len();
        let step = if n >= 2 { p[n - 1] - p[n - 2] } else { DVec3::new(4.0, 0.0, 0.0) };
        let next = p[n - 1] + step.clamp_length(2.0, 6.0);
        p.push(DVec3::new(next.x.floor() + 0.5, next.y.floor() + 0.5, next.z.floor() + 0.5));
        if self.form == 4 {
            self.spec.weights.resize(self.spec.points.len(), 1.0);
        }
    }

    pub fn remove_point(&mut self) {
        if self.spec.points.len() > 2 {
            self.spec.points.pop();
            self.spec.weights.truncate(self.spec.points.len());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_preset_builds() {
        let mut state = State::new();
        for form in 0..FORMS.len() {
            state.form = form;
            state.preset();
            assert!(state.refresh(|| 0.0));
            assert_eq!(state.problem, None, "{}", FORMS[form]);
            assert!(!state.built.bricks.is_empty(), "{}", FORMS[form]);
            let (lo, hi) = state.bounds.expect("a built form has bounds");
            assert!(state.built.bricks.iter().all(|b| (0..3).all(|i| lo[i] <= b.min[i] && b.min[i] + b.ext[i] <= hi[i])));
            assert!((0..3).all(|i| state.built.bricks.iter().any(|b| b.min[i] == lo[i]) && state.built.bricks.iter().any(|b| b.min[i] + b.ext[i] == hi[i])));
            assert!(!state.refresh(|| 0.0), "nothing changed, so nothing should rebuild");
        }
    }

    /// From saved reports: a 2-thick ring of radius 12.5 came out with its
    /// walls half a cell off the grid, and best fit had to move it to radius
    /// 13 or 12. A ring is now set on the grid whatever its radius.
    #[test]
    fn a_ring_is_on_the_grid_without_best_fit() {
        let misfit_at = |radius: f64| {
            let mut state = State::new();
            state.form = 2;
            state.preset();
            state.spec.radius = radius;
            state.spec.thick = 2.0;
            (state.misfit().unwrap(), state)
        };
        let (half, mut state) = misfit_at(12.5);
        let (whole, _) = misfit_at(13.0);
        // (off the grid it was 14% and more)
        assert!(half < 0.1 && whole < 0.14, "radius 12.5 fits {half}, radius 13 fits {whole}");
        state.snap_best(None);
        assert!(state.misfit().unwrap() <= half);
    }

    #[test]
    fn best_fit_respects_the_drag_lock_and_the_handle_asked_for() {
        let mut state = State::new(); // an arch, with y held
        state.spec.rise = Some(10.5);
        let feet = state.spec.points.clone();
        state.snap_best(Some(Handle::Rise));
        assert_eq!(state.spec.points, feet, "only the rise was to change");
        assert_ne!(state.spec.rise, Some(10.5));
        state.snap_best(None);
        assert!(state.spec.points.iter().all(|p| p.y == 0.5), "y is locked");
    }

    #[test]
    fn a_saved_form_comes_back_as_it_was() {
        let mut state = State::new();
        state.form = 5;
        state.preset();
        state.cut = Cut::Dome;
        state.spec.radius = 11.0;
        let text = arcs_core::report::spec_text(&state.full_spec());
        let mut other = State::new();
        other.load_spec(arcs_core::report::parse_spec(&text).unwrap());
        assert_eq!((other.form, other.cut == Cut::Dome), (5, true));
        assert_eq!(other.full_spec(), state.full_spec());

        let mut arch = State::new();
        arch.arch = 3;
        arch.spec.rise = Some(5.0);
        other.load_spec(arcs_core::report::parse_spec(&arcs_core::report::spec_text(&arch.full_spec())).unwrap());
        assert_eq!((other.form, other.arch), (0, 3));
        assert_eq!(other.full_spec(), arch.full_spec());
    }

    #[test]
    fn a_workspace_holds_several_forms_each_with_its_own_colours() {
        let mut state = State::new(); // an arch
        state.refresh(|| 0.0);
        let arch = state.full_spec();
        let arch_right = state.bounds.unwrap().1[0];
        state.paint.mode = crate::paint::Mode::Solid;
        state.paint.colours = vec![[10, 20, 30]];

        // add a sphere: it becomes the form being edited, clear of the arch
        let mut sphere = State::new();
        sphere.form = 5;
        sphere.preset();
        state.add_form(sphere.full_spec());
        assert!(state.refresh(|| 0.0));
        assert_eq!((state.form, state.others.len()), (5, 1));
        assert!(state.bounds.unwrap().0[0] >= arch_right + 4, "the sphere overlaps the arch");
        assert_eq!(state.paint, Paint::default(), "a new form starts with its own colouring");

        // every brick is there, each with its own form's colour
        let all = state.painted();
        assert_eq!(all.len(), state.built.bricks.len() + state.others[0].built.bricks.len());
        assert!(all.iter().filter(|b| b.2 == 0).all(|b| b.1 == Some(Paint::default().colours[0])));
        assert!(all.iter().filter(|b| b.2 == 1).all(|b| b.1 == Some([10, 20, 30])));

        // switch to the arch and back: nothing is rebuilt or lost
        let sphere_spec = state.full_spec();
        state.activate(0);
        assert_eq!(state.full_spec(), arch);
        assert_eq!(state.paint.colours, vec![[10, 20, 30]]);
        assert!(!state.refresh(|| 0.0), "its bricks came with it");
        assert_eq!(state.others[0].spec, sphere_spec);

        // moving a form moves all of it, by whole microbricks
        assert!(state.handles().iter().all(|(h, _)| *h != Handle::Move), "a form has no dot to move it by");
        let at = state.spec.points[0] + DVec3::new(2.0, 0.0, 5.0);
        state.grab = Some(at);
        let eye = DVec3::new(0.0, -80.0, at.z);
        state.lock = Lock::Axis(1);
        state.drag(Handle::Move, eye, (at + DVec3::new(3.4, 0.0, 2.2) - eye).normalize(), DVec3::Y);
        assert_eq!(state.spec.points[0] - arch.points[0], DVec3::new(3.0, 0.0, 2.0));
        assert_eq!(state.spec.points[1] - arch.points[1], DVec3::new(3.0, 0.0, 2.0));
        // and a second, smaller pull carries on from where the first left it
        assert_eq!(state.grab, Some(at + DVec3::new(3.0, 0.0, 2.0)));

        // only a form with one size all round is resized by the wheel
        assert!(!state.resize(1.0), "an arch has no single size");
        // each of those changes can be taken back and made again
        state.record();
        let moved = state.spec.points.clone();
        state.spec.thick = 5.0;
        state.record();
        state.paint.colours = vec![[1, 2, 3]];
        state.record();
        assert!(state.undo());
        assert_eq!((state.spec.thick, state.paint.colours.clone()), (5.0, vec![[10, 20, 30]]));
        assert!(state.undo());
        assert_eq!((state.spec.thick, state.spec.points.clone(), state.others.len()), (arch.thick, moved.clone(), 1));
        assert!(state.redo() && state.redo() && !state.redo());
        assert_eq!((state.spec.thick, state.paint.colours.clone()), (5.0, vec![[1, 2, 3]]));
        // choosing the other form to edit is not a step, and a new change forgets what was undone
        assert!(state.undo());
        state.activate(0);
        state.record();
        state.activate(0);
        state.record();
        assert!(state.redo(), "choosing a form forgot what could be redone");
        assert!(state.undo());
        state.spec.thick = 7.0;
        state.record();
        assert!(!state.redo());
        assert!(state.undo() && state.undo());
        assert_eq!((state.spec.thick, state.spec.points.clone()), (arch.thick, moved));
        state.refresh(|| 0.0);

        // removing the form being edited leaves the other; the last cannot go
        state.remove_active();
        assert_eq!((state.form, state.others.len()), (5, 0));
        let before = state.spec.radius;
        assert!(state.resize(-1.0) && state.spec.radius == before - 1.0);
        assert!(state.resize(-1000.0) && state.spec.radius == 0.5);
        state.remove_active();
        assert_eq!(state.form, 5);
    }

    #[test]
    fn dragging_moves_handles_on_the_grid() {
        let mut state = State::new(); // an arch, locked to its upright plane
        let eye = DVec3::new(0.0, -50.0, 5.0);
        // drag the right foot (at 8.5, 0.5, 0.5) to a point 3.2 cells further
        // out and 1.7 up: (11.7, 0.5, 2.2), which is in the cell centred (11.5, 0.5, 2.5)
        let target = state.spec.points[1] + DVec3::new(3.2, 0.0, 1.7);
        state.drag(Handle::Point(1), eye, (target - eye).normalize(), DVec3::Y);
        assert_eq!(state.spec.points[1], DVec3::new(11.5, 0.5, 2.5), "whole cells, and y untouched");

        // pull the crown up: the rise becomes explicit, in whole units
        let crown = state.handles().iter().find(|(h, _)| *h == Handle::Rise).unwrap().1;
        let up = crown + DVec3::new(0.0, 0.0, 2.3);
        state.drag(Handle::Rise, eye, (up - eye).normalize(), DVec3::Y);
        let rise = state.spec.rise.unwrap();
        assert_eq!(rise, snap(rise));
        assert!(state.refresh(|| 0.0) && state.problem.is_none());

        // towards the horizon, and above it, the handle stays put
        let before = state.spec.points[1];
        // the handle moves in the plane y = 0.5, so a ray with almost no y
        // in it runs almost along that plane and meets it millions of cells away
        let level = DVec3::new(1.0, 1e-5, 0.3).normalize();
        state.drag(Handle::Point(1), eye, level, DVec3::Y);
        let skyward = DVec3::new(0.3, -0.2, 1.0).normalize(); // heading away from the plane: it never meets it
        state.drag(Handle::Point(1), eye, skyward, DVec3::Y);
        assert_eq!(state.spec.points[1], before);
        // and the most extreme drag that is allowed still builds
        state.drag(Handle::Point(1), eye, (DVec3::new(190.0, 0.5, 60.0) - eye).normalize(), DVec3::Y);
        assert!(state.spec.points[1].x > 100.0, "{}", state.spec.points[1]);
        assert!(state.refresh(|| 0.0) && state.problem.is_none(), "{:?}", state.problem);
        state.preset();

        // a sphere's radius handle
        state.form = 5;
        state.preset();
        let (_, at) = state.handles()[1];
        let out = at + DVec3::X * 3.4;
        state.drag(Handle::Radius, eye, (out - eye).normalize(), DVec3::Y);
        assert_eq!(state.spec.radius, 11.5);
    }
}
