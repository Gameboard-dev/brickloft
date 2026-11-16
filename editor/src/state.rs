//! What the editor is editing: a form's spec, the handles that can be
//! dragged to change it, and the bricks last built from it.

use crate::paint::Paint;
use brickloft_core::curves::{ArchType, Circle};
use brickloft_core::blend::{Member, Piece};
use brickloft_core::shapes::CellBrick;
use brickloft_core::spec::{build, snap, Built, Form, Spec};
use brickloft_core::DVec3;

/// The forms on offer, in menu order. `Form` itself carries the arch type,
/// so the menu works on this index and the arch type is kept beside it.
/// The forms in the menu. A torus is the Circle as a round tube, and an arc
/// through three points is an Arch with three, so neither has an entry.
pub const FORMS: [&str; 13] = ["Arch", "Circle", "Spline", "Bézier", "Sphere", "Revolve", "Pyramid", "Cube", "Plant", "Furniture", "Maze", "Imported", "Rule program"];

// Where each is in that list. Named, because the places have changed as
// forms were merged, and a bare number left behind in a `match` then meant
// a different form without any complaint from the compiler.
pub const ARCH: usize = 0;
pub const CIRCLE: usize = 1;
pub const SPLINE: usize = 2;
pub const BEZIER: usize = 3;
pub const SPHERE: usize = 4;
pub const REVOLVE: usize = 5;
pub const PYRAMID: usize = 6;
pub const CUBE: usize = 7;
pub const TREE: usize = 8;
// (After Plant, as it was asked for, with the maze after it. A save names
// its forms in words and not by these places (see
// `brickloft_core::report::spec_text`), so no save changes meaning when a
// place does.)
pub const FURNITURE: usize = 9;
pub const MAZE: usize = 10;
// (Last, as the newest. Its place may change like any other's: a save says `imported`.)
pub const IMPORTED: usize = 11;
// (And after it, as the newest of all. A rule program is a building grown by rewrite
// rules: see `brickloft_core::rewrite`, and `grow` for how one is watched.)
pub const PROGRAM: usize = 12;

pub mod grow;

/// Most points an arch may have: two feet and up to six between.
pub const ARCH_POINTS: usize = 8;
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
/// core will build (`brickloft_core::spec::MAX_SPAN`), so that no drag can ask
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

/// What a brush does to grass's ground.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Action {
    Up,
    Down,
    Flatten,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Brush {
    pub range: f64,
    pub amount: f64,
    pub action: Action,
    /// whether a touch does all it does out to a narrow band at its edge (see `brickloft_core::tree::Sculpt`)
    pub sharp: bool,
}

/// What an added point of grass says of a flattening touch in the place
/// its range and amount are kept in: any number but 0 would do, and this
/// one is still not 0 after a form is shrunk for the coarsest brick size
/// and snapped to half cells (see `brickloft_core::spec::tree_lay`).
pub use brickloft_core::spec::FLATTENS;
/// And what it says of a touch with a sharp edge, added to that.
pub use brickloft_core::spec::SHARP;

impl Brush {
    /// What a touch made with this brush is kept as, in the place an
    /// added point keeps how far it was pulled: its range, what it does
    /// (see [`FLATTENS`]), and its amount, or for a flattening touch the
    /// `level` it flattens to.
    pub fn touch(&self, level: f64) -> DVec3 {
        let edge = if self.sharp { SHARP } else { 0.0 };
        match self.action {
            Action::Up => DVec3::new(self.range, edge, self.amount),
            Action::Down => DVec3::new(self.range, edge, -self.amount),
            Action::Flatten => DVec3::new(self.range, edge + FLATTENS, level),
        }
    }

    /// Whether an added point of grass is a touch that flattens.
    pub fn flattens(by: DVec3) -> bool {
        by.y % SHARP != 0.0
    }
}

/// A best-fit search under way (see [`State::snap_best`]).
struct Search {
    /// the numbers it may change, and how many times over it goes through them
    tunables: Vec<Tune>,
    rounds: usize,
    /// where it has got to: the round, the number, and the value of it to try next
    round: usize,
    which: usize,
    step: usize,
    /// the misfit it began with, once measured, and the least found so far
    start: Option<f64>,
    best: f64,
    /// the settings each value is tried from, and the whole spec they make
    at: Spec,
    full: Spec,
    /// the best settings found for the number being tried, if any beat `best`
    choice: Option<Spec>,
    /// whether this round has changed anything
    improved: bool,
    /// builds done, and the most there can be: it may well end sooner, when
    /// a round changes nothing
    done: usize,
    total: usize,
    /// whether a frame has been drawn since it was begun
    waited: bool,
}

/// One number the best-fit search may change.
#[derive(Clone, Copy)]
enum Tune {
    /// a point (by index) along an axis
    Point(usize, usize),
    Radius,
    Rise,
}

/// One leg of a flight of the camera: where it ends up, what it is looking
/// at when it gets there, and how long it takes over it. No time at all is
/// a jump.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Leg {
    pub to: [f64; 3],
    pub look_at: [f64; 3],
    pub seconds: f64,
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
    /// where it stands, in units
    pub place: [i32; 3],
    pub built: Built,
    pub bounds: Option<Bounds>,
    /// whether it is picked, with the form being edited, to be copied
    pub picked: bool,
}

impl Item {
    /// What to call it in the list.
    pub fn name(&self) -> &'static str {
        FORMS[form_index(self.spec.form)]
    }
}

/// Where a form is in the menu.
pub(crate) fn form_index(form: Form) -> usize {
    match form {
        // an arc through three points is an arch with three
        Form::Arch(_) | Form::Arc => ARCH,
        Form::Circle | Form::Torus => CIRCLE,
        Form::Spline => SPLINE,
        Form::Bezier => BEZIER,
        Form::Sphere => SPHERE,
        Form::Revolve => REVOLVE,
        Form::Pyramid => PYRAMID,
        Form::Cube => CUBE,
        Form::Tree => TREE,
        Form::Furniture => FURNITURE,
        Form::Maze => MAZE,
        Form::Imported => IMPORTED,
        Form::Program => PROGRAM,
    }
}

/// The points a form of kind `form` (one of `ARCH` to `TREE`) starts
/// from, and a Bezier curve's weights: where the editor sets one down, and
/// what a form added from outside gets if it is given none of its own.
/// For furniture `radius` is half its height (see [`half_height`]).
pub fn usual_points(form: usize, radius: f64) -> (Vec<DVec3>, Vec<f64>) {
    match form {
        ARCH => (vec![cell(-8.0, 0.0, 0.0), cell(8.0, 0.0, 0.0)], Vec::new()),
        // on the floor (a mesh's point is its foot: see `brickloft_core::mesh::Stood::new`)
        CIRCLE | PYRAMID | TREE | MAZE | IMPORTED | PROGRAM => (vec![cell(0.0, 0.0, 0.0)], Vec::new()),
        // standing on the floor
        CUBE => (vec![cell(0.0, 0.0, radius.floor())], Vec::new()),
        // Standing on the floor too. Its point is its middle, as a
        // cube's is, so half its height up: exactly, and not in the
        // cell below as a cube's is, since its height is whatever its
        // kind starts at and may be even.
        FURNITURE => (vec![DVec3::new(0.5, 0.5, radius)], Vec::new()),
        SPLINE => (vec![cell(-10.0, 0.0, 0.0), cell(-4.0, 0.0, 6.0), cell(4.0, 0.0, 2.0), cell(10.0, 0.0, 8.0)], Vec::new()),
        BEZIER => (vec![cell(-8.0, 0.0, 0.0), cell(0.0, 0.0, 12.0), cell(8.0, 0.0, 0.0)], vec![1.0, 1.0, 1.0]),
        // resting on the floor
        SPHERE => (vec![cell(0.0, 0.0, radius.ceil())], Vec::new()),
        _ => (vec![cell(0.0, 0.0, 10.0), cell(5.0, 0.0, 8.0), cell(8.0, 0.0, 0.0)], Vec::new()),
    }
}

/// What [`usual_points`] is to be given for a form's radius: the radius,
/// or for furniture half its height, which is what stands it on the
/// floor. Its radius is half its depth (see `State::usual_furniture`).
pub fn half_height(form: usize, spec: &Spec) -> f64 {
    if form == FURNITURE { (2.0 * spec.radius * spec.stretch.z).round() / 2.0 } else { spec.radius }
}

/// A brick of the workspace: the brick, its colour (`None` for the player's
/// own in the game), and which form it belongs to (0 is the one being
/// edited, then the others in order).
pub type Painted = (CellBrick, Option<[u8; 3]>, usize, [i32; 3]);

/// A form with how it is coloured and where it stands in the workspace.
///
/// Where it stands is in game units, two to a cell, and is no part of the
/// form: a form is fitted on its own cells, and moving it moves those cells
/// with it. Moving its points instead, as was done at first, refitted it at
/// every step, could only go a whole cell at a time, and changed its bricks
/// whenever a point crossed from a cell's middle to its edge.
pub type Placed = (Spec, Paint, [i32; 3]);

/// Something in the view that can be dragged.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Handle {
    /// moves the whole form
    Move,
    /// moves the whole form along x, y or z alone: one of its three arrows
    Slide(usize),
    /// a point added to a curve with Alt + click, which pulls the curve
    /// out of shape where it was added
    Pull(usize),
    /// one of the three rings, which turn the form's own grid about x, y or z
    Spin(usize),
    /// a control point
    Point(usize),
    /// pulls a circle, sphere or torus wider or narrower
    Radius,
    /// pulls an arch's crown up or down
    Rise,
    /// One side of a cube, which it moves in or out: 0 and 1 are its low
    /// and its high side along x, 2 and 3 along y, 4 and 5 along z.
    Side(usize),
}

/// A build under way on another thread: what it is of, when it began, which
/// form it was begun for, and where its answer will come.
struct Behind {
    of: Spec,
    started: f64,
    loaded: u64,
    answer: std::sync::mpsc::Receiver<Result<Built, String>>,
}

/// A build that took longer than this, in milliseconds, has the next one of
/// its form made on another thread (see [`State::refresh_behind`]): three
/// frames or so, past which a wait shows as a stutter.
const SLOW_BUILD_MS: f64 = 50.0;

/// Half the longest side a mesh is opened at, in microbricks: 48 long, a
/// little over a player's height in the game, which shows most models'
/// shape and is quick to fit. It is made larger from there.
pub const USUAL_MESH_RADIUS: f64 = 24.0;

pub struct State {
    pub spec: Spec,
    pub form: usize,
    pub arch: usize,
    pub lock: Lock,
    /// where the form being edited stands in the workspace, in units
    pub place: [i32; 3],
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
    /// where the last report went, or why it could not be made
    pub report_note: String,
    /// A notice to be read, as its heading and its text: shown over the view
    /// until dismissed.
    pub alert: Option<(&'static str, String)>,
    /// a saved form the player has asked to delete, as its folder and its
    /// name: asked about over the view, and deleted only on a yes
    pub deleting: Option<(String, String)>,
    /// Whether Claude may drive the editor (see `mcp`): off until the player
    /// switches it on, and off again each time the program starts.
    pub claude_on: bool,
    /// where the editor is listening for Claude, or why it is not
    pub claude_note: String,
    /// where the camera is and which way it looks (a direction of length
    /// one), in microbricks of the world: kept current by the frame loop,
    /// which owns the camera, for whoever asks from outside it
    pub camera_at: ([f64; 3], [f64; 3]),
    /// The legs of a flight the camera has been sent on, flown by the frame
    /// loop one after another.
    pub flight: std::collections::VecDeque<Leg>,
    /// something done, said for a few seconds over the view: the words, and
    /// when they were first shown (by the panel's clock, once it has shown them)
    pub toast: Option<(String, Option<f64>)>,
    /// recording the window as a GIF: its settings, and the recording under way (see `gif`)
    #[cfg(not(target_arch = "wasm32"))]
    pub gif: crate::gif::Controls,
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
    /// whether the arrows that move a grid, and the rings that turn it,
    /// are drawn and can be taken hold of: each on its own, since either
    /// can be in the way of the form it stands in the middle of
    pub show_arrows: bool,
    pub show_rings: bool,
    /// whether the form's dots are drawn and can be dragged: off for a
    /// picture of the bricks alone
    pub show_dots: bool,
    /// How far every slider for a size goes: as far as the core will build
    /// (`brickloft_core::spec::MAX_SPAN`) unless the user lowers it. It began at
    /// 60 to keep an even slider fine enough for small forms; the sliders
    /// are logarithmic now, which does that at any reach.
    pub max_size: f64,
    /// the box of cells the bricks occupy, low corner and high, for the rulers
    pub bounds: Option<Bounds>,
    /// How far below the form's lowest brick the floor is drawn, in
    /// microbricks. Only a matter of the view: it changes nothing that is built.
    pub floor_gap: f64,
    /// after each drag, move what was dragged to the best fit nearby
    pub auto_best: bool,
    /// The brush grass's ground is shaped with (see
    /// `brickloft_core::tree::Sculpt`): how far it reaches, how much it
    /// pushes, and what it does. Alt + click touches the ground with it.
    pub brush: Brush,
    /// which touch of the brush the panel's sliders are changing, if any: the one last made or dragged
    pub touched: Option<usize>,
    /// what the last best-fit search did
    pub best_note: String,
    /// a best-fit search not yet finished
    search: Option<Search>,
    /// saving: the folder chosen, the name typed, a new folder's name typed,
    /// and what the last save or open came to
    pub folder: String,
    pub save_name: String,
    pub new_folder: String,
    pub store_note: String,
    /// the path typed for a mesh file to open (see [`State::open_mesh`])
    pub mesh_path: String,
    /// the folders and the chosen folder's forms, read when they may have
    /// changed and not on every frame
    pub folders: Vec<String>,
    pub forms: Vec<String>,
    built_from: Option<Spec>,
    /// whether a dot, a form or a slider is being dragged this frame
    pub busy: bool,
    /// whether Ctrl is held during a drag: a point added to a circle then
    /// moves straight out of the circle's plane instead of within it
    pub lift: bool,
    /// Whether Ctrl is held during a drag. A form moved then goes where
    /// it is dragged, into other forms if that is where, and is not set
    /// down clear of them: the two are to be one build.
    pub merge: bool,
    /// the blend of the form being edited, and whether it is taken away
    /// from the forms it joins, when the view was last told of them: a
    /// change of either builds nothing (see `refresh`)
    blend_shown: (Option<f64>, bool),
    /// The bricks of the workspace's joined forms as last put together,
    /// with what they were put together from (see `pieces`).
    joins: std::cell::RefCell<Option<(Vec<JoinKey>, std::rc::Rc<Vec<Piece>>)>>,
    /// Whether a form is being dragged about the view just now. Joined
    /// forms are then drawn each with its own bricks, uncut, and are put
    /// together when it is let go (see [`State::painted`]).
    pub moving: bool,
    /// a build of the form being edited under way on another thread (see [`State::refresh_behind`])
    behind: Option<Behind>,
    /// how many times another form has been made the one being edited: a
    /// build begun for one form is not an answer for the next
    loaded: u64,
    /// how much of a dragged dot's movement the dots near it share, 0 to 1
    pub follow: f64,
    /// a ring being dragged: the angle round it where it was taken hold of,
    /// and the grid's angle then
    spin_from: Option<(f64, [f64; 4], DVec3, DVec3)>,
    /// the workspace as it was before each change, oldest first, and after
    /// each change undone, for Ctrl+Z and Ctrl+Y
    undone: Vec<Snapshot>,
    redone: Vec<Snapshot>,
    /// the workspace as last noted by [`State::record`]
    noted: Snapshot,
    /// a rule program being watched as it runs, and its controls (see [`grow`])
    pub grow: grow::Grow,
}

/// Everything in the workspace that an edit can change: each form and how it
/// is coloured, the one being edited first. Bricks are not kept; they are
/// built again from the form, which keeps a long history small.
type Snapshot = Vec<Placed>;

/// What a join was put together from: each form, where it stands, and
/// enough of its bricks to tell a build from the one before it.
type JoinKey = (Spec, [i32; 3], usize, bool);
/// What a form's drawn bricks depend on (see [`State::drawn_from`]): how it
/// is described and coloured, where it stands, whether it is picked, and
/// how many bricks it was built of and how; and, while forms are joined,
/// the number of the workspace's last change.
pub type DrawnFrom = (Spec, Paint, [i32; 3], bool, usize, bool, Option<u64>);

/// How far along the line through `from` in direction `a` the ray from
/// `origin` along `dir` comes nearest it; `None` if the ray runs along the
/// line or meets it implausibly far off. Both directions are of length one.
fn along_line(from: DVec3, a: DVec3, origin: DVec3, dir: DVec3) -> Option<f64> {
    // the nearest points of two lines: https://en.wikipedia.org/wiki/Skew_lines#Nearest_points
    let (w, b) = (from - origin, a.dot(dir));
    let across = 1.0 - b * b;
    if across < 1e-4 {
        return None;
    }
    let t = (b * dir.dot(w) - a.dot(w)) / across;
    // as for a drag in a plane: never flung to the horizon
    (t.abs() <= 4.0 * w.length().max(15.0)).then_some(t)
}

/// A grid that is not turned: see `brickloft_core::spec::Spec::grid`.
pub const LEVEL: [f64; 4] = [0.0, 0.0, 0.0, 1.0];

/// The turn that is `first` and then `then`, as quaternions (x, y, z, w).
/// <https://en.wikipedia.org/wiki/Quaternion#Hamilton_product>
fn turns(first: [f64; 4], then: [f64; 4]) -> [f64; 4] {
    let (a, b) = (then, first);
    let q = [a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1], a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0], a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3], a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2]];
    let length = q.iter().map(|v| v * v).sum::<f64>().sqrt();
    q.map(|v| v / length)
}

/// A turn by `angle` radians about x, y or z.
fn about(axis: usize, angle: f64) -> [f64; 4] {
    let mut q = [0.0, 0.0, 0.0, (angle / 2.0).cos()];
    q[axis] = (angle / 2.0).sin();
    q
}

/// A direction turned by a quaternion; with `back`, turned the other way.
pub fn turned_way(q: [f64; 4], v: DVec3, back: bool) -> DVec3 {
    let u = DVec3::new(q[0], q[1], q[2]) * if back { -1.0 } else { 1.0 };
    // https://en.wikipedia.org/wiki/Quaternions_and_spatial_rotation#Using_quaternions_as_rotations
    v + 2.0 * u.cross(u.cross(v) + v * q[3])
}

/// Which way a plant grows, as its bricks are built: the axis its own up
/// lies along, and whether up is towards that axis's low end. `None` for
/// a form that is no plant.
///
/// A plant's blend or stripes run up it, the one way that suits a thing
/// that grows (see `panel::colours`). They were laid along z whatever the
/// plant, which is up only for a plant standing as it grew: a tree given a
/// quarter turn onto its side was banded across its trunk, and one flipped
/// top to bottom had its foot's colour at its crown.
///
/// A build is mirrored first, where the form is flipped, and then given
/// its quarter turns (`brickloft_core::spec::build_with`). So the plant's
/// up is z, the other way if it is flipped along z, and then wherever the
/// turn takes z.
pub fn grows_along(spec: &Spec) -> Option<(usize, bool)> {
    if spec.form != Form::Tree {
        return None;
    }
    let turn = brickloft_core::shapes::ORIENT[(spec.turned % 24) as usize];
    // (the turn takes the brick's axis `perm[i]` to the world's axis `i`, the other way round if bit `i` of `neg` is set)
    let axis = (0..3).find(|i| turn.perm[*i] == 2)?;
    Some((axis, (turn.neg >> axis & 1 == 1) != spec.flip[2]))
}

/// Whether a form's grid is at an angle, and so not the world's.
pub fn on_own_grid(spec: &Spec) -> bool {
    spec.grid != LEVEL
}

/// Where a point of a form's grid is in the world: turned by the grid's
/// angle, then moved to where the grid is.
pub fn out_of_grid(spec: &Spec, p: DVec3) -> DVec3 {
    DVec3::from_array(spec.grid_at) + turned_way(spec.grid, p, false)
}

/// The reverse: where a point of the world is in a form's grid.
pub fn into_grid(spec: &Spec, p: DVec3) -> DVec3 {
    turned_way(spec.grid, p - DVec3::from_array(spec.grid_at), true)
}

/// How long the arrows and how wide the rings are, as a share of the way
/// from the middle of a grid's forms to their furthest side: a tenth, so
/// that they mark the middle of what they move without covering it. It was
/// 0.6 and then a quarter, each found to bury the form, and then a slider,
/// which only asked the user to find the size that should have been chosen.
pub const GIZMO: f64 = 0.1;

/// A plane whose normal is this near square to the line of sight (as the
/// cosine of the angle between them) is seen too nearly edge on to drag in:
/// see `State::drag_alone`.
const EDGE_ON: f64 = 0.3;

/// How far from a dragged dot others go with it, in reaches of an added point.
const FOLLOW_REACH: f64 = 3.0;

/// Most points that may be added to one curve.
pub const PULLS: usize = 12;

/// The most touches of a brush a patch of grass's ground may have. Far
/// more than the points another form may be pulled by, because a touch
/// costs only a little arithmetic for each tuft of grass, where a pull
/// bends a whole curve or wraps a solid; and a stroke of the brush is
/// tens of touches.
pub const TOUCHES: usize = 240;

/// The most added points a form may have (see [`PULLS`] and [`TOUCHES`]).
pub fn most_pulls(spec: &Spec) -> usize {
    if spec.form == Form::Tree && spec.tree.kind == brickloft_core::tree::Kind::Grass { TOUCHES } else { PULLS }
}

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
            place: [0; 3],
            grab: None,
            cut: Cut::Whole,
            segment: (2.0, 6.0),
            fine_snap: false,
            built: Built::default(),
            build_ms: 0.0,
            problem: None,
            report_note: String::new(),
            alert: None,
            deleting: None,
            claude_on: false,
            claude_note: String::new(),
            camera_at: ([0.0; 3], [1.0, 0.0, 0.0]),
            flight: std::collections::VecDeque::new(),
            toast: None,
            #[cfg(not(target_arch = "wasm32"))]
            gif: crate::gif::Controls::default(),
            paint: Paint::default(),
            others: Vec::new(),
            scene: 0,
            open_adds: false,
            panel_right: 0.0,
            show_ruler: true,
            show_arrows: true,
            show_rings: true,
            show_dots: true,
            max_size: brickloft_core::spec::MAX_SPAN,
            bounds: None,
            floor_gap: 0.0,
            auto_best: false,
            brush: Brush { range: 24.0, amount: 8.0, action: Action::Up, sharp: false },
            touched: None,
            best_note: String::new(),
            search: None,
            folder: crate::store::STARTER_FOLDERS[0].to_string(),
            save_name: String::new(),
            new_folder: String::new(),
            store_note: String::new(),
            mesh_path: String::new(),
            folders: Vec::new(),
            forms: Vec::new(),
            built_from: None,
            busy: false,
            lift: false,
            merge: false,
            blend_shown: (None, false),
            joins: std::cell::RefCell::new(None),
            moving: false,
            behind: None,
            loaded: 0,
            follow: 0.5,
            spin_from: None,
            undone: Vec::new(),
            redone: Vec::new(),
            noted: Vec::new(),
            grow: grow::Grow::default(),
        };
        // Where rule programs brought in from outside are kept: told to
        // the core once, the first time there is an editor, and the list
        // of them read. (Once, so that a test may keep them elsewhere
        // than in the folder of whoever runs it.)
        #[cfg(not(target_arch = "wasm32"))]
        if brickloft_core::rewrite::form::library().is_none() {
            if let Ok(folder) = crate::places::programs() {
                brickloft_core::rewrite::form::set_library(folder);
            }
        }
        state.grow.imported = crate::programs::imported();
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
        Item { spec: self.full_spec(), paint: self.paint.clone(), place: self.place, built: self.built.clone(), bounds: self.bounds, picked: false }
    }

    /// Make `item` the form being edited, with the bricks it already has.
    fn take_item(&mut self, item: Item) {
        self.load_spec(item.spec);
        self.paint = item.paint;
        self.place = item.place;
        self.built = item.built;
        self.bounds = item.bounds;
        self.problem = None;
        // its bricks are current: nothing to rebuild until it is changed
        self.built_from = Some(Spec { grid: LEVEL, grid_at: [0.0; 3], on_grid: 0, blend: None, carve: false, ..self.full_spec() });
        self.scene += 1;
    }

    /// Edit another of the workspace's forms; the one being edited takes its place.
    pub fn activate(&mut self, other: usize) {
        if other < self.others.len() {
            self.catch_up();
            let mine = self.as_item();
            let item = std::mem::replace(&mut self.others[other], mine);
            self.take_item(item);
            // choosing one form to edit is choosing it alone
            self.others.iter_mut().for_each(|o| o.picked = false);
        }
    }

    /// Pick another form to be copied along with the one being edited, or
    /// let it go if it is picked already: Shift + click.
    pub fn toggle_pick(&mut self, other: usize) {
        if let Some(item) = self.others.get_mut(other) {
            item.picked = !item.picked;
            self.scene += 1;
        }
    }

    /// Pick every form: Ctrl + A.
    pub fn pick_all(&mut self) {
        self.others.iter_mut().for_each(|o| o.picked = true);
        self.scene += 1;
    }

    /// Which forms are picked, the one being edited first; it always is.
    pub fn picked(&self) -> Vec<bool> {
        std::iter::once(true).chain(self.others.iter().map(|o| o.picked)).collect()
    }

    /// What Ctrl + C copies: the bricks and forms that are picked, the form
    /// being edited among them, numbered among themselves.
    ///
    /// It was the whole workspace, which made copying one form out of
    /// several a matter of removing the others first.
    pub fn copied(&self) -> (Vec<Painted>, Vec<Placed>) {
        let picked = self.picked();
        // each picked form's number among the picked
        let numbers: Vec<Option<usize>> = picked.iter().scan(0, |next, p| Some(p.then(|| std::mem::replace(next, *next + 1)))).collect();
        let bricks = self.painted().into_iter().filter_map(|(cell, colour, form, place)| numbers[form].map(|n| (cell, colour, n, place))).collect();
        let forms = self.forms().into_iter().zip(&picked).filter(|(_, p)| **p).map(|(f, _)| f).collect();
        (bricks, forms)
    }

    /// Add a form to the workspace and edit it. It is set down clear of the
    /// forms already there, to their +x side, so that nothing is hidden.
    pub fn add_form(&mut self, spec: Spec) {
        self.add_form_at(spec, None);
    }

    /// [`State::add_form`], standing at `place` if one is given.
    fn add_form_at(&mut self, spec: Spec, place: Option<[i32; 3]>) {
        // the far side of everything there, in units
        let taken = self.others.iter().filter_map(|i| i.bounds.map(|b| 2 * b.1[0] + i.place[0])).chain(self.bounds.map(|b| 2 * b.1[0] + self.place[0])).max();
        // (built once, here: to find where it goes, and to be the bricks it has until its first build as the form being edited)
        //
        // But a mesh, which may take seconds: where it goes is known from
        // its box without a brick being laid, and it is built behind the
        // window once it is the form being edited (see `load_spec`).
        let mesh_low = spec.import.mesh.as_ref().filter(|_| spec.form == Form::Imported).zip(spec.points.first()).map(|(mesh, middle)| (middle.x - 2.0 * spec.radius * mesh.half.x).round() as i32);
        let fresh = if spec.form == Form::Imported { Built::default() } else { build(&spec).unwrap_or_default() };
        let beside = match (taken, bounds_of(&fresh).map(|(lo, _)| lo[0]).or(mesh_low)) {
            // four microbricks of air between them
            (Some(right), Some(low)) => [right + 8 - 2 * low, 0, 0],
            _ => [0; 3],
        };
        self.catch_up();
        let mine = self.as_item();
        self.others.push(mine);
        self.load_spec(spec);
        self.paint = Paint::default();
        self.place = place.unwrap_or(beside);
        self.built_from = None;
        // Its own bricks from the start: the bricks in hand were the last
        // form's, and were drawn as the new one until it was built.
        self.bounds = bounds_of(&fresh);
        self.built = fresh;
        self.scene += 1;
    }

    /// Make the form being edited larger or smaller by `cells` of radius,
    /// if it is one with a single size all round: a circle, a sphere, a
    /// pyramid or a cube (which keeps its proportions as nearly as whole
    /// microbricks allow). Returns whether it is. By whole cells, so that a
    /// radius that ends on the lattice still does.
    pub fn resize(&mut self, cells: f64) -> bool {
        let round = matches!(self.form, CIRCLE | SPHERE | PYRAMID | CUBE | TREE | FURNITURE | MAZE | IMPORTED);
        if round {
            self.spec.radius = (self.spec.radius + cells).clamp(0.5, HANDLE_LIMIT);
        }
        round
    }

    /// Where the form being edited ends up if moved by `by` units: there, if
    /// that is clear of the other forms, or else the nearest place straight
    /// up or straight out to one side that is. `None` if there is none.
    ///
    /// Two forms may not overlap: the game refuses a brick whose box
    /// overlaps another's, so forms pushed into each other would be sent
    /// with pieces missing. Clear is judged by each brick's box, as the game
    /// judges it. The way out is up or sideways and never down, so that a
    /// form dragged onto another comes to rest on top of it.
    fn clear_of_others(&self, by: [i32; 3]) -> Option<[i32; 3]> {
        // only what shares its grid: grids pass through each other
        let taken: Vec<Standing> = self.others.iter().filter(|o| self.in_my_way(&o.spec)).map(|o| Standing::new(&o.built.bricks, o.place)).collect();
        let mine: Vec<[i32; 3]> = cells_of(&self.built.bricks).collect();
        way_out(&mine, self.place, &taken, by)
    }

    /// Move the other forms out of the way of the one being edited, where
    /// reshaping it has grown it into them.
    ///
    /// Each form it now touches goes to the nearest place, up or to a side,
    /// that is clear of it and of every form already settled; the ones it
    /// does not touch are settled first, so that a form pushed aside goes
    /// round them and not into them.
    fn push_others(&mut self) {
        // (a form joined to others on purpose pushes none of them, and none of them is pushed out of it)
        if self.others.is_empty() || self.joined() {
            return;
        }
        let shares: Vec<bool> = self.others.iter().map(|o| o.spec.blend.is_none() && self.in_my_way(&o.spec)).collect();
        let mut settled = vec![Standing::new(&self.built.bricks, self.place)];
        // (most rebuilds touch nothing, and then there is nothing more to do)
        if !self.others.iter().zip(&shares).any(|(o, shares)| *shares && settled[0].meets(cells_of(&o.built.bricks), o.place)) {
            return;
        }
        let touched: Vec<bool> = self.others.iter().zip(&shares).map(|(o, shares)| *shares && settled[0].meets(cells_of(&o.built.bricks), o.place)).collect();
        for ((other, _), _) in self.others.iter().zip(&touched).zip(&shares).filter(|((_, t), shares)| !**t && **shares) {
            settled.push(Standing::new(&other.built.bricks, other.place));
        }
        for (other, _) in self.others.iter_mut().zip(&touched).filter(|(_, t)| **t) {
            let mine: Vec<[i32; 3]> = cells_of(&other.built.bricks).collect();
            if let Some(by) = way_out(&mine, other.place, &settled, [0; 3]) {
                other.place = [other.place[0] + by[0], other.place[1] + by[1], other.place[2] + by[2]];
            }
            settled.push(Standing::new(&other.built.bricks, other.place));
            self.scene += 1;
        }
    }

    /// Where the form being edited stands, in cells: what to add to a point
    /// of the form to find it in the workspace.
    pub fn standing(&self) -> DVec3 {
        DVec3::new(self.place[0] as f64, self.place[1] as f64, self.place[2] as f64) / 2.0
    }

    /// Every form in the workspace with its colouring, the one being edited first.
    pub fn forms(&self) -> Vec<Placed> {
        self.snapshot()
    }

    /// Add copies of forms to the workspace, standing as they stood with
    /// each other when they were copied, and as a group clear of what is
    /// already there.
    pub fn paste(&mut self, forms: Vec<Placed>) {
        let Some(first) = forms.first().map(|f| f.2) else { return };
        let mut origin = None;
        // The grid of the first form pasted becomes the grid being worked
        // on, so that a pasted form joins the forms beside it. (Pasting
        // onto a grid of its own made every paste a separate grid, to be
        // moved and turned apart from what it was pasted next to.) Any
        // further grids in what is pasted stay grids of their own, numbered
        // after the ones already here.
        let (joined, here) = (forms[0].0.on_grid, (self.spec.on_grid, self.spec.grid, self.spec.grid_at));
        let after = std::iter::once(self.spec.on_grid).chain(self.others.iter().map(|o| o.spec.on_grid)).max().unwrap_or(0) + 1;
        for (mut spec, paint, place) in forms {
            if spec.on_grid == joined {
                (spec.on_grid, spec.grid, spec.grid_at) = here;
            } else {
                spec.on_grid += after;
            }
            // the first is set down beside what is there; the rest keep
            // their distance from it
            let at = origin.map(|o: [i32; 3]| [0, 1, 2].map(|k| o[k] + place[k] - first[k]));
            self.add_form_at(spec, at);
            self.paint = paint;
            origin.get_or_insert(self.place);
            self.refresh(|| 0.0);
        }
    }

    /// Put `forms` in place of the whole workspace, as opening a save does:
    /// each with its own colours, place and grid, the first the one being
    /// edited. One step to undo.
    pub fn open(&mut self, forms: Vec<Placed>) {
        if forms.is_empty() {
            return;
        }
        self.record();
        self.undone.push(self.snapshot());
        self.redone.clear();
        self.restore(forms);
    }

    fn snapshot(&self) -> Snapshot {
        std::iter::once((self.full_spec(), self.paint.clone(), self.place)).chain(self.others.iter().map(|i| (i.spec.clone(), i.paint.clone(), i.place))).collect()
    }

    /// Note the workspace if it has changed since it was last noted, as a
    /// step that can be undone. Called once a frame while nothing is being
    /// dragged, so that a whole drag is one step and not one per pixel.
    pub fn record(&mut self) {
        // (nor while a rule program is being watched: its run is one step, however many turns it shows on the way)
        if self.growing() {
            return;
        }
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
        // The bricks the workspace has now are kept for whichever of the
        // snapshot's forms are the same forms: a form's bricks follow from
        // how it is described, wherever it stands and whatever its grid.
        // Every form was built again, always. Taking back the move of a
        // cube then built the lawn beside it afresh, a second or more for
        // a large one, and so did every change Claude made through MCP,
        // which puts the workspace back whole each time.
        let fitted = |spec: &Spec| Spec { grid: LEVEL, grid_at: [0.0; 3], on_grid: 0, blend: None, ..spec.clone() };
        let mut kept: Vec<(Spec, Built, Option<Bounds>)> = std::mem::take(&mut self.others).into_iter().map(|o| (fitted(&o.spec), o.built, o.bounds)).collect();
        let mine = fitted(&self.full_spec());
        if self.built_from.as_ref() == Some(&mine) && self.problem.is_none() && !self.built.unmeasured {
            kept.push((mine, std::mem::take(&mut self.built), self.bounds));
        }
        let mut again = |spec: &Spec| -> Option<(Built, Option<Bounds>)> {
            let wanted = fitted(spec);
            let at = kept.iter().position(|(from, ..)| *from == wanted)?;
            // (taken, not copied, unless another of the snapshot's forms is the same again)
            Some(kept.iter().skip(at + 1).any(|(from, ..)| *from == wanted).then(|| (kept[at].1.clone(), kept[at].2)).unwrap_or_else(|| { let (_, built, bounds) = kept.swap_remove(at); (built, bounds) }))
        };
        let mut forms = snapshot.clone().into_iter();
        let Some((spec, paint, place)) = forms.next() else { return };
        let own = again(&spec);
        self.others = forms
            .map(|(spec, paint, place)| {
                let (built, bounds) = again(&spec).unwrap_or_else(|| {
                    // (a rule program not yet run is not run here, holding the window for as long as it takes: it stands empty until `grow_others` has run it)
                    if cfg!(not(target_arch = "wasm32")) && spec.form == Form::Program && brickloft_core::rewrite::form::known(&spec.program).is_none() {
                        return (Built::default(), None);
                    }
                    let built = crate::import::built(&spec).unwrap_or_default();
                    let bounds = bounds_of(&built);
                    (built, bounds)
                });
                Item { bounds, spec, paint, place, built, picked: false }
            })
            .collect();
        self.load_spec(spec);
        self.paint = paint;
        self.place = place;
        self.built_from = None;
        if let Some((built, bounds)) = own {
            (self.built, self.bounds, self.problem) = (built, bounds, None);
            self.built_from = Some(fitted(&self.full_spec()));
        } else if self.form == IMPORTED {
            // (a mesh is built behind the window, and until it is, it is not drawn as the bricks of whatever was here before)
            (self.built, self.bounds) = (Built::default(), None);
        }
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

    /// Whether the form being edited was moved into other forms on
    /// purpose (`Spec::blend`). A joined form is not pushed clear of the
    /// others when one of them is reshaped, and is made one build with
    /// them (see `painted`). Moving it without Ctrl sets it clear again.
    pub fn joined(&self) -> bool {
        self.spec.blend.is_some()
    }

    /// Move every joined form that stands half a microbrick off the form it
    /// is joined to onto that form's cells, by half a microbrick along each
    /// axis it is off on (`brickloft_core::blend::settled`, which says why).
    /// `lean` is the way the form being edited should move, for a drag.
    /// Returns whether any form moved.
    ///
    /// Called when a drag joins a form and again on every `refresh`, so that
    /// a join made any other way (by Claude, from a save made before forms
    /// were settled, by pasting) is settled too, and there is one rule and
    /// not one for each way in.
    fn settle_joins(&mut self, lean: [i32; 3]) -> bool {
        let count = 1 + self.others.len();
        if (0..count).all(|n| if n == 0 { self.spec.blend.is_none() } else { self.others[n - 1].spec.blend.is_none() }) {
            return false;
        }
        let mut moved = false;
        // (one at a time, each from where the ones before it now stand)
        for n in 0..count {
            let to = {
                let grid = |spec: &Spec| if on_own_grid(spec) { spec.on_grid + 1 } else { 0 };
                let members: Vec<Member> = std::iter::once((&self.spec, &self.built, self.place)).chain(self.others.iter().map(|o| (&o.spec, &o.built, o.place))).map(|(spec, built, place)| Member { spec, built, place, grid: grid(spec) }).collect();
                brickloft_core::blend::settled(&members, n, if n == 0 { lean } else { [0; 3] })
            };
            let place = if n == 0 { &mut self.place } else { &mut self.others[n - 1].place };
            moved |= *place != to;
            *place = to;
        }
        if moved {
            self.scene += 1;
        }
        moved
    }

    /// Why the form being edited, if it is joined, is cut where it meets
    /// another form and not fitted afresh with it: said in the panel under
    /// the blend, which would otherwise seem to do nothing. `None` if it is
    /// not joined, or blends with every form on its grid.
    ///
    /// The reasons are `brickloft_core::blend`'s: a join is fitted on the
    /// cells the two forms share, and a pattern's bricks are not its form.
    pub fn unblended(&self) -> Option<&'static str> {
        if !self.joined() {
            return None;
        }
        if !self.built.shades.is_empty() {
            return Some("A form laid as a pattern is joined by cutting alone: the blend does nothing to it.");
        }
        let half_off = |place: [i32; 3]| (0..3).any(|k| (place[k] - self.place[k]).rem_euclid(2) == 1);
        let mut others = self.others.iter().filter(|o| self.in_my_way(&o.spec));
        if others.clone().any(|o| half_off(o.place)) {
            // (a joined form is settled onto the cells of the form it stands deepest in: this is one it could not also be lined up with)
            return Some("Two of the forms this one meets stand half a microbrick off one another, so it lines up with one of them only and is joined to the other by cutting alone. Move either of those by half a microbrick to blend all three.");
        }
        others.any(|o| !o.built.shades.is_empty()).then_some("A form laid as a pattern is joined by cutting alone: the blend does nothing where this form meets one.")
    }

    /// The bricks of every form that is taken away from the forms it is
    /// joined to (`Spec::carve`), with which form each belongs to and where
    /// it stands. None of them is built, so none is among `painted`: the
    /// view draws them as glass, so that the form can be seen and taken
    /// hold of, and nothing else is given them.
    pub fn ghosts(&self) -> Vec<(CellBrick, usize, [i32; 3])> {
        let forms = std::iter::once((&self.built, self.place, &self.spec)).chain(self.others.iter().map(|i| (&i.built, i.place, &i.spec)));
        forms.enumerate().filter(|(_, form)| brickloft_core::blend::carves(form.2)).flat_map(|(n, (built, place, _))| built.bricks.iter().map(move |b| (*b, n, place))).collect()
    }

    /// The workspace's bricks before they are coloured: each form's own,
    /// and where joined forms meet, the join fitted afresh
    /// (`brickloft_core::blend`).
    ///
    /// Kept from one call to the next, and put together again only when a
    /// form, its bricks, its place or its blend has changed. `painted` is
    /// called whenever anything in view changes, a colour or a pick
    /// included, and a join is a few milliseconds of fitting that none of
    /// those alter.
    fn pieces(&self, forms: &[(&Built, &Paint, Option<Bounds>, [i32; 3], &Spec)]) -> std::rc::Rc<Vec<Piece>> {
        // (forms on level grids share the world's grid, whatever their numbers)
        let grid = |spec: &Spec| if on_own_grid(spec) { spec.on_grid + 1 } else { 0 };
        let key: Vec<JoinKey> = forms.iter().map(|(built, _, _, place, spec)| (Spec { grid: LEVEL, grid_at: [0.0; 3], on_grid: grid(spec), ..(*spec).clone() }, *place, built.bricks.len(), built.unmeasured)).collect();
        if let Some((from, pieces)) = &*self.joins.borrow() {
            if *from == key {
                return pieces.clone();
            }
        }
        let members: Vec<Member> = forms.iter().map(|(built, _, _, place, spec)| Member { spec, built, place: *place, grid: grid(spec) }).collect();
        let pieces = std::rc::Rc::new(brickloft_core::blend::union(&members));
        *self.joins.borrow_mut() = Some((key, pieces.clone()));
        pieces
    }

    /// Every brick in the workspace, with its colour and its form.
    ///
    /// Forms joined on purpose (see `merge`) overlap, and the game refuses
    /// a brick whose box overlaps another's. They are made one build by
    /// `brickloft_core::blend::union`: fitted afresh where their surfaces
    /// meet, as a smooth union with the joined form's blend, and cut back
    /// to whole cells where blocks of them overlap. That is done there and
    /// not here because it needs the forms' distance functions, and tests
    /// and tools want it with no editor. What is left to do here is colour.
    ///
    /// A brick of a join takes the colour of the brick of its form that it
    /// is, was cut from or stands in place of. One the form had no brick
    /// for (the fillet itself) is coloured as a brick of the form standing
    /// there would be.
    /// For each form (the one being edited, then the others), the colour
    /// of its bricks that are drawn seen through and how solid they are
    /// drawn, of 255: the water of a patch of grass, and more solid its
    /// ice. `None` for a form with no such bricks. Told by colour and not
    /// by brick, since a brick of a join is not a brick of its form's
    /// build; a patch whose water has been given the very colour of its
    /// sand would have its sand seen through as well. It depends on a
    /// form's spec and paint alone, which are both in [`DrawnFrom`].
    pub fn waters(&self) -> Vec<Option<([u8; 3], u8)>> {
        let spec = self.full_spec();
        std::iter::once((&self.paint, &spec)).chain(self.others.iter().map(|i| (&i.paint, &i.spec))).map(|(paint, spec)| {
            let pond = spec.form == Form::Tree && spec.tree.kind == brickloft_core::tree::Kind::Grass && paint.mode == crate::paint::Mode::Pattern;
            paint.shade(brickloft_core::tree::WATER).filter(|_| pond).map(|colour| (colour, if spec.fill { 190 } else { 105 }))
        }).collect()
    }

    pub fn painted(&self) -> Vec<Painted> {
        let mut all = Vec::new();
        let spec = self.full_spec();
        let forms: Vec<(&Built, &Paint, Option<Bounds>, [i32; 3], &Spec)> = std::iter::once((&self.built, &self.paint, self.bounds, self.place, &spec)).chain(self.others.iter().map(|i| (&i.built, &i.paint, i.bounds, i.place, &i.spec))).collect();
        let colour_of = |form: usize, from: Option<usize>, brick: &CellBrick| {
            let (built, paint, bounds, _, spec) = forms[form];
            let patterned = paint.mode == crate::paint::Mode::Pattern && built.shades.len() == built.bricks.len();
            let (lo, hi) = bounds.unwrap_or(([0; 3], [1; 3]));
            // (a plant's blend or stripes run up it, from its foot, however it has been turned)
            let (axis, back) = grows_along(spec).unwrap_or((paint.axis.min(2), false));
            match from {
                Some(n) if patterned => paint.shade(built.shades[n]),
                Some(n) => paint.colour_along(&built.bricks[n], lo, hi, axis, back),
                None => paint.colour_along(brick, lo, hi, axis, back),
            }
        };
        // (nothing joined: every form's bricks are its own, and there is nothing to put together)
        //
        // Nor while a form is being dragged, unless one is cut out of
        // another, which is nothing without its cut. Putting joined forms
        // together is done for the whole of each, and every brick of both
        // was then given to the graphics card again: with a tree in a lawn,
        // a hundred thousand bricks for every step of the drag, and the
        // tree followed the hand in jerks. The forms overlap on screen
        // while they move, which is where they are, and are one build
        // again the moment the drag ends.
        if forms.iter().all(|form| form.4.blend.is_none()) || (self.moving && !forms.iter().any(|form| form.4.carve)) {
            for (form, (built, .., place, _)) in forms.iter().enumerate() {
                all.extend(built.bricks.iter().enumerate().map(|(n, brick)| (*brick, colour_of(form, Some(n), brick), form, *place)));
            }
            return all;
        }
        let pieces = self.pieces(&forms);
        for (form, (.., place, spec)) in forms.iter().enumerate() {
            // the single cells left of bricks that were cut, each with the number of its colour, to be joined up again
            let mut loose: Vec<(CellBrick, u8)> = Vec::new();
            let mut tones: Vec<Option<[u8; 3]>> = Vec::new();
            for piece in pieces.iter().filter(|piece| piece.member == form) {
                let colour = colour_of(form, piece.from, &piece.brick);
                if !piece.loose {
                    all.push((piece.brick, colour, form, *place));
                    continue;
                }
                let tone = tones.iter().position(|known| *known == colour).unwrap_or_else(|| {
                    tones.push(colour);
                    tones.len() - 1
                });
                loose.push((piece.brick, (tone % 256) as u8));
            }
            // What is left of the cut bricks, joined into the largest
            // blocks of one colour. (Left as the rows they were found
            // in, a cube moved into a cube was a stack of single rows.
            // A form of more colours than a byte can number is not
            // joined: two colours would share a number.)
            let loose = if tones.len() <= 256 { brickloft_core::pattern::joined(loose, spec.max_ext.max(1)) } else { loose };
            all.extend(loose.into_iter().map(|(cell, tone)| (cell, tones[tone as usize], form, *place)));
        }
        all
    }

    /// Take on a saved form: its spec, and the menu choices that go with it.
    pub fn load_spec(&mut self, mut spec: Spec) {
        // (another form: a build begun for the last one is no answer for it)
        self.loaded += 1;
        // And it is built here and now the first time, however slow the
        // last form was. Built behind the window because a lawn had been
        // slow, a cube just added was drawn for a moment as the lawn's
        // bricks, which were still the bricks in hand: a second lawn
        // appeared beside the first and was sent to the graphics card
        // before the cube took its place.
        self.build_ms = 0.0;
        // a torus saved by an earlier version: the same thing under its new name
        if spec.form == Form::Torus {
            (spec.form, spec.tube, spec.fill) = (Form::Circle, true, false);
        }
        // a cube is never a round tube (see `brickloft_core::spec::Form::Cube`),
        // whatever the form it was made from left behind
        if spec.form == Form::Cube {
            spec.tube = false;
        }
        self.form = form_index(spec.form);
        if let Form::Arch(kind) = spec.form {
            self.arch = ARCHES.iter().position(|a| a.1 == kind).unwrap_or(0);
        }
        (self.cut, self.segment) = match spec.cut {
            None => (Cut::Whole, self.segment),
            Some((_, hi)) if hi == f64::INFINITY => (Cut::Dome, self.segment),
            Some(ends) => (Cut::Segment, ends),
        };
        // A mesh is the exception to building the first time here and
        // now: it may take seconds, which is seconds of a window that does
        // not answer. Whoever loads one empties the bricks in hand where
        // they are another form's (see `restore` and `add_form_at`).
        if spec.form == Form::Imported {
            self.build_ms = 4.0 * SLOW_BUILD_MS;
        }
        self.spec = spec;
        // (a hedge saved before it had a line is given the line it was)
        self.lay_tree();
    }

    /// Open the mesh file at `path` (see `crate::import`). With `add`, or
    /// if the form being edited is not an Imported one, it is added to
    /// the workspace as a new form; otherwise it takes the place of the
    /// mesh of the form being edited, which keeps its size and settings.
    ///
    /// A file that cannot be read changes nothing, and why is said where
    /// a form's problem is said if the form is an Imported one, and in a
    /// notice otherwise.
    pub fn open_mesh(&mut self, path: &str, add: bool) {
        // (a path copied from a file manager comes in quotation marks)
        let path = path.trim().trim_matches('"');
        let opened = crate::import::adopt(std::path::Path::new(path)).and_then(|(name, up)| Ok((crate::import::open(&name, up)?, name, up)));
        let (mesh, name, up) = match opened {
            Ok(opened) => opened,
            Err(why) if self.form == IMPORTED && !add => return self.problem = Some(why.to_string()),
            Err(why) => return self.alert = Some(("Cannot open the mesh", why.to_string())),
        };
        self.toast = Some((format!("Opened a mesh of {} triangles{}", mesh.triangles(), if mesh.coloured() { ", with its colours" } else { "" }), None));
        let import = brickloft_core::mesh::Import { name, up, mesh: Some(mesh), ..self.spec.import.clone() };
        if add || self.form != IMPORTED {
            // As the editor starts a form, with a brick size chosen to keep within a budget. But with bricks priced at
            // nothing: a price buys a third fewer bricks for twice the discrepancy on a mesh (docs/algorithm.md, 7.13),
            // and closeness comes first.
            let mut spec = Spec { form: Form::Imported, radius: USUAL_MESH_RADIUS, fill: true, simplify: 0.0, grain: 0, import: brickloft_core::mesh::Import { fitter: Default::default(), loose: 0, colours: brickloft_core::mesh::form::USUAL_COLOURS, ..import }, ..Spec::default() };
            spec.points = usual_points(IMPORTED, half_height(IMPORTED, &spec)).0;
            self.add_form(spec);
        } else {
            self.spec.import = import;
            // (built behind the window, like any change to a mesh)
            self.build_ms = 4.0 * SLOW_BUILD_MS;
            self.built_from = None;
        }
        self.paint = crate::import::paint_of(&self.spec).unwrap_or_default();
        self.problem = None;
        self.scene += 1;
    }

    /// A file dropped on the window: opened as a mesh. It becomes the mesh
    /// of the form being edited if that is an Imported form still waiting
    /// for its file, and a new form otherwise.
    pub fn drop_mesh(&mut self, path: &std::path::Path) {
        let waiting = self.form == IMPORTED && self.spec.import.name.is_empty();
        self.open_mesh(&path.display().to_string(), !waiting);
    }

    /// Make the form an Imported one as it starts out: filled, at its
    /// usual size, with no bricks until a file is opened for it. A mesh
    /// already opened in this form before another form was chosen is kept.
    fn usual_import(&mut self) {
        let s = &mut self.spec;
        (s.fill, s.radius, s.stretch) = (true, USUAL_MESH_RADIUS, DVec3::ONE);
        // (bricks priced at nothing: see `open_mesh`)
        (s.stones, s.mosaic, s.simplify) = (None, None, 0.0);
        (self.built, self.bounds) = (Built::default(), None);
        self.build_ms = 4.0 * SLOW_BUILD_MS;
        self.built_from = None;
        if let Some(paint) = crate::import::paint_of(&self.spec) {
            self.paint = paint;
        }
    }

    /// A number the best-fit search may change: one coordinate of a point,
    /// the radius, or an arch's rise.
    fn tunable(&self, only: Option<Handle>) -> Vec<Tune> {
        // (not a mesh: the search builds the form dozens of times over, and a mesh is seconds a build)
        if self.form == IMPORTED {
            return Vec::new();
        }
        let mut all = Vec::new();
        // A tree has nothing to tune: it fits no better a cell to one
        // side, and each try would be a second's building. Nor has a maze.
        // (Nor has furniture, which is exact wherever it stands.)
        // (Nor a rule program, which is not fitted to anything.)
        if matches!(self.form, TREE | FURNITURE | MAZE | PROGRAM) {
            return all;
        }
        let wanted = |h: Handle| only.is_none_or(|o| o == h);
        for i in 0..self.spec.points.len() {
            for axis in 0..3 {
                // a coordinate the drag lock holds still stays still here too
                if wanted(Handle::Point(i)) && self.lock != Lock::Axis(axis) {
                    all.push(Tune::Point(i, axis));
                }
            }
        }
        if matches!(self.form, CIRCLE | SPHERE | PYRAMID) && wanted(Handle::Radius) {
            all.push(Tune::Radius);
        }
        if self.has_rise() && wanted(Handle::Rise) {
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

    /// Begin moving to the best fit within a cell of here.
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
    ///
    /// The search is only begun here. It builds the form once for every value
    /// tried, a few dozen times in all, and a large form takes seconds to
    /// build: done in one go, the window stopped answering until it was over.
    /// So it is carried on a little each frame by [`State::search_for`], with
    /// [`State::search_progress`] to show how far it has got. A second thread
    /// would keep the view moving even during a single slow build, but the
    /// browser build has no threads to give it
    /// (<https://doc.rust-lang.org/rustc/platform-support/wasm32-unknown-unknown.html>),
    /// and two ways of running one search is more to go wrong than one.
    pub fn snap_best(&mut self, only: Option<Handle>) {
        let tunables = self.tunable(only);
        // a slow form gets one round, to be done in reasonable time
        let rounds = if self.build_ms > 60.0 { 1 } else { 3 };
        let tries: usize = tunables.iter().map(|t| self.steps(*t).len()).sum();
        self.search = Some(Search { tunables, rounds, round: 0, which: 0, step: 0, start: None, best: 0.0, at: self.spec.clone(), full: self.full_spec(), choice: None, improved: false, done: 0, total: 1 + rounds * tries, waited: false });
    }

    /// The amounts the search tries adding to one of its numbers.
    fn steps(&self, what: Tune) -> &'static [f64] {
        match what {
            // points sit on cell centres unless half steps are asked for
            Tune::Point(..) if !self.fine_snap => &[-1.0, 1.0],
            _ => &[-1.0, -0.5, 0.5, 1.0],
        }
    }

    /// How far a search under way has got, from 0 to 1; `None` if there is none.
    pub fn search_progress(&self) -> Option<f32> {
        self.search.as_ref().map(|s| s.done as f32 / s.total as f32)
    }

    /// Carry a search on for about `budget_ms`, and by one build at least.
    ///
    /// The frame in which a search is begun does none of it, so that the
    /// progress bar is on the screen before the first build, which for a
    /// large form is the first long wait.
    pub fn search_for(&mut self, budget_ms: f64, now_ms: impl Fn() -> f64) {
        if let Some(search) = self.search.as_mut().filter(|s| !s.waited) {
            search.waited = true;
            return;
        }
        let start = now_ms();
        while self.search_step() && now_ms() - start < budget_ms {}
    }

    /// Try one value (one build of the form). Returns whether there is more to do.
    fn search_step(&mut self) -> bool {
        let Some(mut s) = self.search.take() else { return false };
        // Anything changed by hand since the last step ends the search: what
        // it has measured was for a form that is no longer the one on screen.
        if self.full_spec() != s.full {
            return false;
        }
        let Some(start) = s.start else {
            // the first build is of the form as it stands, to have something to beat
            let Some(fit) = self.misfit() else { return false };
            (s.start, s.best, s.done) = (Some(fit), fit, 1);
            let more = !s.tunables.is_empty();
            self.search = more.then_some(s);
            return more;
        };
        let what = s.tunables[s.which];
        let steps = self.steps(what);
        self.tune(what, steps[s.step]);
        // a clear gain only: never wander off for the sake of noise
        if let Some(fit) = self.misfit().filter(|f| *f < s.best * 0.97 - 1e-9) {
            s.best = fit;
            s.choice = Some(self.spec.clone());
        }
        self.spec = s.at.clone();
        s.done += 1;
        s.step += 1;
        if s.step == steps.len() {
            // every value of this number tried: keep the best, go on to the next
            (s.step, s.which) = (0, s.which + 1);
            if let Some(spec) = s.choice.take() {
                self.spec = spec.clone();
                s.at = spec;
                s.full = self.full_spec();
                s.improved = true;
            }
            if s.which == s.tunables.len() {
                (s.which, s.round) = (0, s.round + 1);
                if !s.improved || s.round == s.rounds {
                    self.best_note = if s.best < start {
                        format!("Moved to a closer fit: discrepancy {:.1}% -> {:.1}%", start * 100.0, s.best * 100.0)
                    } else {
                        "This is already the best fit within a microbrick of here.".into()
                    };
                    return false;
                }
                s.improved = false;
            }
        }
        self.search = Some(s);
        true
    }

    /// Whether a best-fit search is under way.
    pub fn searching(&self) -> bool {
        self.search.is_some()
    }

    /// Put the form back to how it starts out: its dots, and every size and
    /// choice that shapes it. How it is fitted and coloured is left alone.
    pub fn reset(&mut self) {
        let fresh = Spec::default();
        let s = &mut self.spec;
        (s.radius, s.thick, s.depth, s.stretch, s.plane) = (fresh.radius, fresh.thick, fresh.depth, fresh.stretch, fresh.plane);
        (s.protrude, s.flip, s.symmetric, s.pull_reach) = (fresh.protrude, fresh.flip, fresh.symmetric, fresh.pull_reach);
        (s.mirror, s.turned) = (fresh.mirror, fresh.turned);
        self.arch = 0;
        self.segment = (2.0, 6.0);
        self.preset();
    }

    /// Start the chosen form from a sensible example of itself.
    pub fn preset(&mut self) {
        self.preset_settings();
        let (points, weights) = usual_points(self.form, half_height(self.form, &self.spec));
        (self.spec.points, self.spec.weights) = (points, weights);
        self.lay_tree();
    }

    /// Give a plant that lies along a line (a hedge, reeds, grass) the
    /// points of the line it starts on, three in a row along x, and any
    /// other plant its one point. The point in the middle is there to be
    /// dragged aside, which bends it.
    ///
    /// A hedge had one point and a length worked out from its thickness,
    /// so that making it thicker made it longer (see
    /// `brickloft_core::spec::tree_lay`). Its ends are dots now, and its
    /// length is where they are.
    fn lay_tree(&mut self) {
        use brickloft_core::tree::{Kind, Tree};
        if self.form != TREE {
            return;
        }
        let kind = self.spec.tree.kind;
        let at = self.spec.points.first().copied().unwrap_or(cell(0.0, 0.0, 0.0));
        if Tree::lies_along(kind) {
            if self.spec.points.len() < 2 {
                // (as long as it was, if it was stretched; else three times its thickness, or for grass a patch a little longer than wide)
                let long = if self.spec.stretch.x > 1.0 { self.spec.radius * self.spec.stretch.x } else if kind == Kind::Grass { 30.0 } else { (self.spec.radius * brickloft_core::tree::HEDGE_LENGTH).round() };
                self.spec.points = vec![at - DVec3::X * long.round(), at, at + DVec3::X * long.round()];
            }
            // its dots stay on the ground
            self.lock = Lock::Axis(2);
        } else {
            self.spec.points.truncate(1);
            // (a cherry keeps it: it is whether its fallen petals lie under it)
            if kind != Kind::Cherry {
                self.spec.closed = false;
            }
            self.spec.pulls.clear();
        }
        self.spec.stretch = DVec3::ONE;
    }

    /// The point a plant's height and spread dots stand at: the middle one
    /// of a line's, or its only one. And the way across it there, level.
    fn tree_middle(&self) -> (DVec3, DVec3) {
        let p = &self.spec.points;
        let n = p.len() / 2;
        let along = if p.len() >= 2 { (p[(n + 1).min(p.len() - 1)] - p[n.saturating_sub(1)]).truncate().normalize_or_zero() } else { brickloft_core::DVec2::ZERO };
        let across = if along == brickloft_core::DVec2::ZERO { DVec3::X } else { DVec3::new(-along.y, along.x, 0.0) };
        (DVec3::new(p[n].x, p[n].y, p[0].z.floor()), across)
    }

    /// The part of [`State::preset`] that is not the form's points.
    fn preset_settings(&mut self) {
        let s = &mut self.spec;
        s.weights.clear();
        s.pulls.clear();
        s.rise = None;
        s.closed = false;
        s.tube = false;
        s.fill = false;
        self.cut = Cut::Whole;
        self.lock = Lock::Free;
        // Curves stand on the floor, as they will in the game. Solids start
        // lifted clear of it, so that the camera can get underneath and look
        // up into a dome or a hollow shell.
        self.floor_gap = if matches!(self.form, SPHERE | REVOLVE) { 8.0 } else { 0.0 };
        match self.form {
            ARCH => self.lock = Lock::Axis(1), // an arch stays in its upright plane
            SPHERE => s.fill = true,
            PYRAMID => {
                // half a cell more than a whole radius, so that its base
                // ends on the lattice about a cell's centre
                s.radius = s.radius.floor() + 0.5;
                s.fill = true;
            }
            CUBE => {
                // The same each way, an odd number of microbricks, so that
                // it is centred on the cell its middle dot is in; and
                // solid, since a floor or a wall is what it is mostly made
                // into.
                s.radius = s.radius.floor() + 0.5;
                s.stretch = DVec3::ONE;
                s.fill = true;
            }
            TREE => {
                let kind = s.tree.kind;
                self.usual_tree(kind);
            }
            FURNITURE => {
                let kind = s.furniture.kind;
                self.usual_furniture(kind);
            }
            MAZE => self.usual_maze(),
            IMPORTED => self.usual_import(),
            PROGRAM => {
                let (name, imported) = (s.program.name.clone(), s.program.imported);
                self.usual_program(&name, imported);
            }
            _ => {}
        }
        if matches!(self.form, SPLINE | BEZIER | REVOLVE) {
            self.lock = Lock::Axis(1);
        }
    }

    /// Make the form the tree of `kind` as it starts out: its settings, its
    /// usual size, and its own colours. The seed is kept, so that going
    /// from kind to kind and back finds the same tree.
    ///
    /// The colours come with it because a tree in the one colour a form
    /// starts in is a grey shape: its parts are told apart by colour.
    pub fn usual_tree(&mut self, kind: brickloft_core::tree::Kind) {
        use brickloft_core::tree::{Tree, KINDS};
        let (height, spread) = Tree::usual_size(kind);
        self.spec.tree = Tree { seed: self.spec.tree.seed, ..Tree::of(kind) };
        // (A new lawn has cliffs where its ground is made steep, at the
        // middle setting; the number is one a pine keeps its baubles in.
        // A lawn saved before there were cliffs has 0 there and none.)
        if kind == brickloft_core::tree::Kind::Grass {
            self.spec.tree.baubles = 2;
        }
        (self.spec.radius, self.spec.rise) = (spread, Some(height));
        // (reeds round a pond in grass: a few, until asked for more)
        self.spec.depth = 3;
        // (from one kind that lies along a line to another, the line is kept)
        self.spec.stretch = DVec3::ONE;
        // (A hedge's added points bend its line, and a bend that reaches
        // only 8 along it, as a curve's starts out, is a kink in a hedge
        // 12 thick: it starts at 40, a few thicknesses each way. The other
        // kinds go back to what every form starts with.)
        self.spec.pull_reach = if kind == brickloft_core::tree::Kind::Hedge { 40.0 } else { Spec::default().pull_reach };
        self.lay_tree();
        self.paint = Paint { mode: crate::paint::Mode::Pattern, colours: crate::paint::palette(KINDS[kind as usize].1).to_vec(), ..Paint::default() };
    }

    /// Make the form the furniture of `kind` as it starts out: its
    /// settings, its usual size, and its own colours, standing where it
    /// stood. The seed is kept, as a tree's is.
    ///
    /// The colours come with it for a tree's reason: a bookcase in one
    /// colour is a block, since its books are told apart by colour alone.
    ///
    /// Its size is kept as a cube's is, half a side and a stretch along
    /// each axis, so that everything that resizes a cube (the dots on its
    /// sides, Ctrl + scroll) resizes it with no code of its own (see
    /// `brickloft_core::furniture::Furniture::usual_form`).
    pub fn usual_furniture(&mut self, kind: brickloft_core::furniture::Kind) {
        use brickloft_core::furniture::Furniture;
        let tall = Furniture::usual_size(kind)[2];
        // (the floor it stands on now, to stand on it still at its new height)
        let snapped = self.spec.snapped();
        let floor = snapped.points.first().map(|middle| brickloft_core::spec::cube_sides(&snapped, *middle)[2].0);
        // (and the style with the seed: a chair made from a table of a set is a chair of that set)
        self.spec.furniture = Furniture { seed: self.spec.furniture.seed, style: self.spec.furniture.style, ..Furniture::of(kind) };
        (self.spec.radius, self.spec.stretch) = Furniture::usual_form(kind);
        if let (Some(floor), Some(middle)) = (floor, self.spec.points.first_mut()) {
            middle.z = floor + tall / 2.0;
        }
        self.paint = Paint { mode: crate::paint::Mode::Pattern, colours: crate::paint::palette(self.spec.furniture.colours()).to_vec(), ..Paint::default() };
    }

    /// Give every other piece of furniture in the workspace the style,
    /// the seed and the colours of the one being edited, so that they
    /// are a set: the same legs, the same weight, the same wood (see
    /// `brickloft_core::furniture::Set`). A bookcase is left as it is,
    /// having no style. Returns how many were changed. One step to undo.
    pub fn match_furniture(&mut self) -> usize {
        use brickloft_core::furniture::Kind;
        let (style, seed, paint) = (self.spec.furniture.style, self.spec.furniture.seed, self.paint.clone());
        let mut forms = self.forms();
        let mut changed = 0;
        for (spec, colours, _) in forms.iter_mut().skip(1).filter(|f| f.0.form == Form::Furniture && f.0.furniture.kind != Kind::Bookcase) {
            (spec.furniture.style, spec.furniture.seed, *colours) = (style, seed, paint.clone());
            changed += 1;
        }
        if changed > 0 {
            self.record();
            self.open(forms);
        }
        changed
    }

    /// Make the form a maze as one starts out: its usual size, plain
    /// walls, and a maze's own colours. The seed is kept, as a tree's is.
    ///
    /// The colours come with it because a maze is told its parts by
    /// colour: the strip that marks the way through is one of them.
    pub fn usual_maze(&mut self) {
        use brickloft_core::maze::{Maze, USUAL_HEIGHT, USUAL_SIZE};
        let s = &mut self.spec;
        s.maze = Maze { seed: s.maze.seed, ..Maze::default() };
        (s.radius, s.thick, s.rise) = (USUAL_SIZE.0, USUAL_SIZE.1, Some(USUAL_HEIGHT));
        (s.stones, s.mosaic, s.stretch) = (None, None, DVec3::ONE);
        self.paint = Paint { mode: crate::paint::Mode::Pattern, colours: crate::paint::maze_palettes("none")[0].clone(), ..Paint::default() };
    }

    /// How tall a maze's walls stand, and how thick the floor under them is.
    pub fn maze_height(&self) -> (f64, f64) {
        (brickloft_core::maze::walls::height_of(&self.spec.maze, self.spec.rise), self.spec.maze.clamped().floor as f64)
    }

    /// A tree's height.
    pub fn tree_height(&self) -> f64 {
        self.spec.rise.unwrap_or(brickloft_core::tree::Tree::usual_size(self.spec.tree.kind).0)
    }

    /// Whether the form is an arch on its two feet alone, which has a kind
    /// and a height; with more points it is a curve through them.
    pub fn has_rise(&self) -> bool {
        self.form == ARCH && self.spec.points.len() == 2
    }

    /// The spec with the menu's choices folded in.
    pub fn full_spec(&self) -> Spec {
        let mut s = self.spec.clone();
        s.form = match self.form {
            // An arch is its two feet; with a third point, the arc through
            // all three; with more, the smooth curve through them all.
            ARCH => match s.points.len() {
                0..=2 => Form::Arch(ARCHES[self.arch].1),
                3 => Form::Arc,
                _ => Form::Spline,
            },
            CIRCLE => Form::Circle,
            SPLINE => Form::Spline,
            BEZIER => Form::Bezier,
            SPHERE => Form::Sphere,
            PYRAMID => Form::Pyramid,
            CUBE => Form::Cube,
            TREE => Form::Tree,
            FURNITURE => Form::Furniture,
            MAZE => Form::Maze,
            IMPORTED => Form::Imported,
            PROGRAM => Form::Program,
            _ => Form::Revolve,
        };
        if self.form == CUBE {
            s.tube = false; // not offered for it, so never left on by the form before
        }
        // a curve through an arch's points is open, whatever a spline left behind
        if self.form == ARCH && s.points.len() != 3 {
            s.closed = false;
        }
        s.cut = match self.cut {
            Cut::Whole => None,
            // from the bottom face of the centre's cell, so the dome sits flat
            Cut::Dome => Some((-0.5, f64::INFINITY)),
            Cut::Segment => Some((self.segment.0.min(self.segment.1), self.segment.0.max(self.segment.1))),
        };
        s
    }

    /// Whether a build of the form being edited is under way on another
    /// thread: its bricks on screen are of what it was a moment ago.
    pub fn fitting(&self) -> bool {
        self.behind.is_some()
    }

    /// Bring the form's bricks up to what it is now, here and at once,
    /// whatever is being built elsewhere: before the form is set down among
    /// the others, which are never built again and would keep stale bricks.
    fn catch_up(&mut self) {
        // (only if a build is under way: otherwise the form is as the last frame left it, as it always was here)
        if self.behind.take().is_some() {
            self.rebuild(|| 0.0);
        }
    }

    /// [`State::refresh`] for the window: a form that is slow to build is
    /// built on another thread, and its new bricks are shown when they are
    /// ready. Returns whether new bricks were put in place this time.
    ///
    /// Built here, as every form was, a slow form stopped the window for
    /// as long as it took: a lawn of sixty thousand bricks took half a
    /// second when grass was fitted, so each touch of the brush on it was
    /// a freeze, and a stroke of touches a run of them, several seconds
    /// together. (A lawn is laid out now, in a few milliseconds, and is
    /// built here: see `brickloft_core::grass`. A tree is still slow.) Now the window
    /// goes on drawing the bricks it has, and takes the new ones when the
    /// other thread sends them. One build at a time: a change made while
    /// one is under way waits for it, and is built next, so a run of
    /// changes costs the builds there was time for and not one for each.
    ///
    /// A form quick to build is still built here and now, since for it an
    /// answer a frame late is a lag where there was none. Which a form is
    /// goes by how long its last build took.
    ///
    /// The alternative was to build a slow form in pieces and only the
    /// pieces a change touches. That is particular to each kind of form;
    /// this keeps the window answering for every form, the ones not yet
    /// thought of among them.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn refresh_behind(&mut self, now_ms: impl Fn() -> f64 + Copy) -> bool {
        let wanted = |state: &State| Spec { grid: LEVEL, grid_at: [0.0; 3], on_grid: 0, blend: None, carve: false, ..state.full_spec() };
        let mut taken = false;
        if let Some(behind) = self.behind.take() {
            match behind.answer.try_recv() {
                Ok(result) => {
                    let now = wanted(self);
                    // Not for another form than it was begun for; nor where
                    // the form has since been built here for what it now is.
                    // Otherwise it is put in place even if the form has
                    // moved on again: nearer to it than what was there.
                    if behind.loaded == self.loaded && (behind.of == now || self.built_from.as_ref() != Some(&now)) {
                        match result {
                            Ok(built) => {
                                self.bounds = bounds_of(&built);
                                self.built = built;
                                self.problem = None;
                                self.push_others();
                            }
                            Err(e) => self.problem = Some(e),
                        }
                        self.build_ms = now_ms() - behind.started;
                        self.built_from = Some(behind.of);
                        self.scene += 1;
                        taken = true;
                    }
                }
                // still at it: nothing new, though a change of blend is still to be drawn
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    self.behind = Some(behind);
                    if self.blend_shown != (self.spec.blend, self.spec.carve) {
                        self.blend_shown = (self.spec.blend, self.spec.carve);
                        self.scene += 1;
                    }
                    return false;
                }
                // (a build that came to grief says nothing: the form is built again below)
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {}
            }
        }
        let now = wanted(self);
        let owed = self.built.unmeasured && !self.busy;
        if (self.built_from.as_ref() == Some(&now) && !owed) || self.build_ms < SLOW_BUILD_MS {
            return self.refresh(now_ms) || taken;
        }
        if self.blend_shown != (self.spec.blend, self.spec.carve) {
            self.blend_shown = (self.spec.blend, self.spec.carve);
            self.scene += 1;
        }
        let (answer_to, answer) = std::sync::mpsc::channel();
        let (spec, measured) = (now.clone(), !self.busy);
        // (if the window has gone by the time it is done, there is nobody to tell)
        std::thread::spawn(move || {
            let _ = answer_to.send(brickloft_core::spec::build_as(&spec, measured));
        });
        self.behind = Some(Behind { of: now, started: now_ms(), loaded: self.loaded, answer });
        taken
    }

    /// Build again if anything changed since the last build. Returns whether it did.
    pub fn refresh(&mut self, now_ms: impl Fn() -> f64) -> bool {
        let rebuilt = self.rebuild(now_ms);
        // Joined forms are settled once the form being edited has been
        // built, since where a form lines up goes by where its bricks are:
        // before, a workspace just opened was settled against the bricks
        // of the form the editor starts with, and a form half a microbrick
        // off stayed there until the next frame.
        // (outside a drag, which settles the form it moves as it goes, and says nothing: see `move_by`)
        if !self.busy && self.built_from.is_some() && self.settle_joins([0; 3]) {
            self.toast = Some(("Moved a joined form a little, so that its bricks line up with those of the form it joins".to_string(), None));
        }
        rebuilt
    }

    /// [`State::refresh`], less the settling of joined forms.
    fn rebuild(&mut self, now_ms: impl Fn() -> f64) -> bool {
        // A change of blend changes no brick of the form itself, so nothing
        // is built for it, but the join is other bricks and the view must
        // draw them.
        if self.blend_shown != (self.spec.blend, self.spec.carve) {
            self.blend_shown = (self.spec.blend, self.spec.carve);
            self.scene += 1;
        }
        // (the angle of its grid changes nothing about how a form is fitted, nor does what it is joined to)
        let spec = Spec { grid: LEVEL, grid_at: [0.0; 3], on_grid: 0, blend: None, carve: false, ..self.full_spec() };
        // Built while something was being dragged, a form is left unmeasured
        // (see `brickloft_core::spec::build_as`): once the mouse is let go it is
        // built again, measured this time.
        let owed = self.built.unmeasured && !self.busy;
        if self.built_from.as_ref() == Some(&spec) && !owed {
            return false;
        }
        let start = now_ms();
        // (a mesh by way of the one place that remembers the last one built: see `import::built`)
        let result = if spec.form == Form::Imported { crate::import::built(&spec) } else { brickloft_core::spec::build_as(&spec, !self.busy) };
        match result {
            Ok(built) => {
                // worked out once per build, not once per frame
                self.bounds = bounds_of(&built);
                self.built = built;
                self.problem = None;
                self.push_others();
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
            CIRCLE => Circle::axis_aligned(DVec3::ZERO, 1.0, self.spec.plane.min(2)).u,
            // along a mesh's longest side, which is the side its size is the length of
            IMPORTED => self.spec.import.mesh.as_ref().map_or(DVec3::X, |mesh| [DVec3::X, DVec3::Y, DVec3::Z][(0..3).max_by(|a, b| mesh.half[*a].total_cmp(&mesh.half[*b]).then(b.cmp(a))).unwrap_or(0)]),
            _ => DVec3::X,
        }
    }

    /// Turn the form being edited a quarter turn about x, y or z, or with
    /// `back`, a quarter turn the other way.
    pub fn turn(&mut self, axis: usize, back: bool) {
        use brickloft_core::shapes::{compose, quarter};
        for _ in 0..if back { 3 } else { 1 } {
            self.spec.turned = compose(self.spec.turned, quarter(axis));
        }
    }

    /// The rings the form's grid is turned by: their middle and how wide
    /// they are, which is as long as the arrows that move it.
    ///
    /// They stand at the middle of everything on the grid, which is what
    /// they turn, wherever the arrows are: those stand at the middle of
    /// the forms they move.
    pub fn rings(&self) -> Option<(DVec3, f64)> {
        let (lo, hi) = self.grid_box()?;
        Some((out_of_grid(&self.spec, (lo + hi) / 2.0), self.gizmo()?.1[0]))
    }

    /// What the bricks drawn for the form being edited depend on, and what
    /// those of all the others do: the view keeps the two apart and makes
    /// again only the one whose answer here has changed (see `lib.rs`).
    ///
    /// The form being edited is drawn where it stands by one move of all
    /// its bricks together, so where it stands, and its grid's place and
    /// angle, are no part of what its bricks depend on: dragging it makes
    /// nothing again. Forms joined on purpose are bricks of each other's
    /// making, so while any is joined both answers change with every change
    /// to the workspace.
    pub fn drawn_from(&self) -> (DrawnFrom, Vec<DrawnFrom>) {
        // (not while a form is dragged, when joined forms are drawn each as its own: see `painted`)
        let apart = self.moving && !self.spec.carve && !self.others.iter().any(|o| o.spec.carve);
        let joined = (!apart && (self.spec.blend.is_some() || self.others.iter().any(|o| o.spec.blend.is_some()))).then_some(self.scene);
        let own = (Spec { grid: LEVEL, grid_at: [0.0; 3], ..self.full_spec() }, self.paint.clone(), [0; 3], false, self.built.bricks.len(), self.built.unmeasured, joined);
        let rest = self.others.iter().map(|o| (o.spec.clone(), o.paint.clone(), o.place, o.picked, o.built.bricks.len(), o.built.unmeasured, joined)).collect();
        (own, rest)
    }

    /// Each form's grid, the one being edited first: where it is, and its angle.
    pub fn grids(&self) -> Vec<([f64; 3], [f64; 4])> {
        std::iter::once((self.spec.grid_at, self.spec.grid)).chain(self.others.iter().map(|o| (o.spec.grid_at, o.spec.grid))).collect()
    }

    /// The angle of the cursor's ray round ring `axis`: where the ray meets
    /// the ring's plane, as an angle about the ring's middle. `None` if the
    /// ring is seen edge on.
    fn round_ring(&self, axis: usize, about: DVec3, origin: DVec3, dir: DVec3) -> Option<f64> {
        let normal = [DVec3::X, DVec3::Y, DVec3::Z][axis];
        let facing = dir.dot(normal);
        if facing.abs() < 0.05 {
            return None;
        }
        let out = origin + dir * ((about - origin).dot(normal) / facing) - about;
        let (a, b) = [(1, 2), (2, 0), (0, 1)][axis];
        Some(out[b].atan2(out[a]))
    }

    /// Take hold of ring `axis` where the cursor's ray passes it. Returns
    /// whether it could be.
    pub fn hold_ring(&mut self, axis: usize, origin: DVec3, dir: DVec3) -> bool {
        let Some((lo, hi)) = self.grid_box() else { return false };
        let middle = (lo + hi) / 2.0;
        let about = out_of_grid(&self.spec, middle);
        self.spin_from = self.round_ring(axis, about, origin, dir).map(|angle| (angle, self.spec.grid, about, middle));
        self.spin_from.is_some()
    }

    /// Whether the form being edited is an arch standing at an angle to its
    /// grid: its two ends differ along both x and y. Such an arch lies in no
    /// plane of the grid, so no flat outline can be fitted to it and it is
    /// fitted cell by cell, which comes out stepped.
    pub fn askew(&self) -> bool {
        let p = &self.spec.points;
        self.form == ARCH && p.len() >= 2 && p[0].x != p[p.len() - 1].x && p[0].y != p[p.len() - 1].y
    }

    /// Turn the grid of an arch that is [`State::askew`] so that the arch
    /// runs along the grid's x, where it stands: its first foot stays where
    /// it is in the world, the other as near as half a microbrick allows,
    /// and it is then fitted as a flat outline. Returns whether it was done.
    ///
    /// Bricks cannot be turned on a grid, so an arch across a grid's
    /// diagonal cannot be made smooth on it. A grid can stand at any
    /// angle, though, and the bricks on it are fitted as if it were level.
    /// So the arch is given a grid of its own, turned about the upright by
    /// the angle the arch stood at, and the arch's points are turned back
    /// by as much within it.
    pub fn line_up(&mut self) -> bool {
        // (a form given quarter turns has its points in another frame: left alone)
        if !self.askew() || self.spec.turned != brickloft_core::shapes::UPRIGHT as u8 {
            return false;
        }
        let (first, last) = (self.spec.points[0], self.spec.points[self.spec.points.len() - 1]);
        let angle = (last.y - first.y).atan2(last.x - first.x);
        let stands = self.standing();
        let held = out_of_grid(&self.spec, first + stands);
        // every point about the first foot, turned back in plan and set on the half-cell grid
        let back = |v: DVec3| DVec3::new(v.x * angle.cos() + v.y * angle.sin(), v.y * angle.cos() - v.x * angle.sin(), v.z);
        for p in self.spec.points.iter_mut() {
            let turned = first + back(*p - first);
            *p = DVec3::new(snap(turned.x), first.y, snap(turned.z));
        }
        for (at, by) in self.spec.pulls.iter_mut() {
            (*at, *by) = (first + back(*at - first), back(*by));
        }
        // a grid of its own, unless it has one to itself already
        if self.others.iter().any(|o| o.spec.on_grid == self.spec.on_grid) {
            self.spec.on_grid = std::iter::once(self.spec.on_grid).chain(self.others.iter().map(|o| o.spec.on_grid)).max().unwrap_or(0) + 1;
        }
        // about the upright within the grid first, then the grid's own angle
        self.spec.grid = turns(about(2, angle), self.spec.grid);
        self.spec.grid_at = (held - turned_way(self.spec.grid, first + stands, false)).to_array();
        self.lock = Lock::Axis(1); // it stays in its upright plane
        self.built_from = None;
        self.scene += 1;
        true
    }

    /// Put the grid being edited back on the world's grid, level, about
    /// where its middle is, to the nearest unit.
    pub fn level(&mut self) {
        let Some((lo, hi)) = self.grid_box() else { return };
        let middle = (lo + hi) / 2.0;
        let gone = ((out_of_grid(&self.spec, middle) - middle) * 2.0).round();
        let by = [gone.x as i32, gone.y as i32, gone.z as i32];
        let on = self.spec.on_grid;
        self.place = [0, 1, 2].map(|k| self.place[k] + by[k]);
        for other in self.others.iter_mut().filter(|o| o.spec.on_grid == on) {
            other.place = [0, 1, 2].map(|k| other.place[k] + by[k]);
        }
        self.set_grid(LEVEL, [0.0; 3]);
    }

    /// The dots as they are seen: where the form's dots are once it has
    /// been turned by quarter turns, and its grid by whatever angle.
    pub fn handles(&self) -> Vec<(Handle, DVec3)> {
        let (turn, standing) = (brickloft_core::spec::Turn::of(&self.spec.snapped()), self.standing());
        self.dots().into_iter().map(|(h, at)| (h, out_of_grid(&self.spec, turn.point(at - standing) + standing))).collect()
    }

    /// The dots of the form as it is fitted, before it is turned: where
    /// dragging works, since the form's points and sizes are kept that way.
    fn dots(&self) -> Vec<(Handle, DVec3)> {
        let p = &self.spec.points;
        let mut all: Vec<(Handle, DVec3)> = p.iter().enumerate().map(|(i, q)| (Handle::Point(i), *q)).collect();
        // an added point is shown where it has pulled the curve to
        // (a touch on grass's ground is shown on the ground as that touch left it at its middle: for a push, as high as it pushed)
        let grass = self.is_grass();
        all.extend(self.spec.pulls.iter().enumerate().map(|(i, (at, by))| (Handle::Pull(i), if grass { *at + DVec3::Z * by.z } else { *at + *by })));
        let standing = self.standing();
        match self.form {
            ARCH if p.len() == 2 => all.push((Handle::Rise, (p[0] + p[1]) / 2.0 + DVec3::Z * self.arch_rise())),
            CIRCLE | SPHERE | PYRAMID if !p.is_empty() => all.push((Handle::Radius, p[0] + self.radius_dir() * self.spec.radius)),
            // A mesh: one dot at the end of its longest side, for its size.
            // Its point is its foot, so the dot is the mesh's radius out
            // from the middle of its box, which is half its height up.
            IMPORTED if !p.is_empty() => {
                let up = self.spec.import.mesh.as_ref().map_or(0.0, |mesh| 2.0 * self.spec.radius * mesh.half.z);
                all.push((Handle::Radius, DVec3::new(p[0].x, p[0].y, p[0].z.floor() + up) + self.radius_dir() * self.spec.radius));
            }
            // A dot at the middle of each side, where the bricks end. No
            // radius dot: it would move all six sides at once, which
            // Ctrl + scroll does, and seven dots round a small cube leave
            // little of it to see.
            // (furniture fills a cube's box and is resized by the same dots)
            CUBE | FURNITURE if !p.is_empty() => {
                let snapped = self.spec.snapped();
                let sides = brickloft_core::spec::cube_sides(&snapped, snapped.points[0]);
                for side in 0..6 {
                    // the middle of the box as built, which is half a cell off its point when it could not be centred there
                    let mut at = DVec3::from_array(sides.map(|(low, high)| (low + high) / 2.0));
                    at[side / 2] = if side % 2 == 0 { sides[side / 2].0 } else { sides[side / 2].1 };
                    all.push((Handle::Side(side), at));
                }
            }
            // a tree: one dot at its top for its height, and one at its
            // side, half way up, for its spread
            TREE if !p.is_empty() => {
                // (of a hedge, at the middle of its line, the spread dot out to one side of it: along the line it was inside the hedge)
                let (foot, across) = self.tree_middle();
                all.push((Handle::Rise, foot + DVec3::Z * self.tree_height()));
                all.push((Handle::Radius, foot + across * self.spec.radius + DVec3::Z * (self.tree_height() / 2.0).round()));
            }
            // a maze: one dot over its middle at the top of its walls, for
            // their height, and one on its rim, half way up, for its size
            MAZE if !p.is_empty() => {
                let foot = DVec3::new(p[0].x, p[0].y, p[0].z.floor());
                let (height, floor) = self.maze_height();
                all.push((Handle::Rise, foot + DVec3::Z * (floor + height)));
                all.push((Handle::Radius, foot + DVec3::X * self.spec.radius + DVec3::Z * (floor + (height / 2.0).round())));
            }
            _ => {}
        }
        // where they are in the workspace, which is where they are drawn and dragged
        all.iter_mut().for_each(|h| h.1 += standing);
        all
    }

    /// Where the arrows that move the form being edited are drawn: the
    /// middle of its bricks, and how long each arrow is. They are short, and
    /// drawn over the bricks, so that a large form is not buried in them.
    ///
    /// Dragging a form by its bricks moves it in the plane facing the eye,
    /// which is two directions at once and neither of them chosen: setting
    /// one form against another that way meant turning the view between
    /// every move. An arrow moves it along one axis and no other, whatever
    /// the view.
    ///
    /// They stand at the middle of the forms they move: the one being
    /// edited and any on its grid picked with it (see `slide_grid`).
    pub fn gizmo(&self) -> Option<(DVec3, [f64; 3])> {
        let (lo, hi) = self.grid_box()?;
        // A quarter of the way out to the furthest side of what is on the
        // grid: it grows and shrinks with the grid, and leaves most of it
        // clear. (Six tenths of the way, as it was first made, covered the
        // forms it was there to move.)
        let reach = (GIZMO * (hi - lo).max_element() / 2.0).max(1.5);
        let (lo, hi) = self.box_of(true)?;
        Some((out_of_grid(&self.spec, (lo + hi) / 2.0), [reach; 3]))
    }

    /// The box, in the grid's own cells, of every form on the grid of the
    /// one being edited. The arrows and rings stand at its middle, and so
    /// move there as the forms on the grid are moved and reshaped.
    fn grid_box(&self) -> Option<(DVec3, DVec3)> {
        self.box_of(false)
    }

    /// [`State::grid_box`], or with `picked` the box of only the form being
    /// edited and those on its grid picked with it.
    fn box_of(&self, picked: bool) -> Option<(DVec3, DVec3)> {
        let corner = |c: [i32; 3], place: [i32; 3]| DVec3::new(c[0] as f64 + place[0] as f64 / 2.0, c[1] as f64 + place[1] as f64 / 2.0, c[2] as f64 + place[2] as f64 / 2.0);
        let mine = self.bounds.map(|b| (b, self.place));
        let theirs = self.others.iter().filter(|o| o.spec.on_grid == self.spec.on_grid && (o.picked || !picked)).filter_map(|o| o.bounds.map(|b| (b, o.place)));
        mine.into_iter().chain(theirs).map(|((lo, hi), place)| (corner(lo, place), corner(hi, place))).reduce(|a, b| (a.0.min(b.0), a.1.max(b.1)))
    }

    /// Set the angle and place of the grid being edited, for every form on it.
    fn set_grid(&mut self, grid: [f64; 4], at: [f64; 3]) {
        let on = self.spec.on_grid;
        (self.spec.grid, self.spec.grid_at) = (grid, at);
        for other in self.others.iter_mut().filter(|o| o.spec.on_grid == on) {
            (other.spec.grid, other.spec.grid_at) = (grid, at);
        }
        self.scene += 1;
    }

    /// Take hold of the form being edited at a place in the view, to move
    /// it within its grid.
    pub fn take_hold(&mut self, at: DVec3) {
        self.grab = Some(into_grid(&self.spec, at));
    }

    /// Move what is picked along arrow `axis` to where the cursor's ray
    /// now passes it: the form being edited, and any on its grid picked
    /// with it (Shift + click).
    ///
    /// The arrows moved the whole grid, always. A form could then be set
    /// along one axis only by dragging it by its bricks with that axis
    /// locked, and several forms could not be moved together at all
    /// without every other form on their grid coming too. The rings still
    /// turn the whole grid: forms on one grid share its angle, so there is
    /// nothing else for a ring to turn.
    ///
    /// With every form on the grid picked (or one form alone on it) the
    /// grid itself moves, as it did: a level grid is the world's, and what
    /// is on it moves a unit at a time; a grid at an angle moves freely.
    /// Fewer than all move within the grid, a unit at a time along the
    /// grid's own axis of that name, and stop short of the forms left
    /// behind unless Ctrl is held to join them. (On a grid at an angle
    /// that axis is not quite the way the arrow points, the arrows being
    /// drawn along the world's.)
    fn slide_grid(&mut self, axis: usize, origin: DVec3, dir: DVec3) {
        let (Some(held), Some(now)) = (self.grab, self.on_arrow(axis, origin, dir)) else { return };
        let gone = now[axis] - held[axis];
        let on = self.spec.on_grid;
        if self.others.iter().any(|o| o.spec.on_grid == on && !o.picked) {
            let mut units = (gone * 2.0).round() as i32;
            let goes = |o: &Item| o.spec.on_grid == on && o.picked;
            if self.merge {
                // (joined with the blend it had if it was joined already, and a sharp crease if not)
                self.spec.blend = Some(self.spec.blend.unwrap_or(0.0));
            } else {
                // as far as none of them meets a form that stays, a form joined on purpose aside
                let stays: Vec<Standing> = self.others.iter().filter(|o| !goes(o) && self.in_my_way(&o.spec) && o.spec.blend.is_none()).map(|o| Standing::new(&o.built.bricks, o.place)).collect();
                let moved: Vec<(&[CellBrick], [i32; 3])> = std::iter::once((&self.built.bricks[..], self.place, self.spec.blend)).chain(self.others.iter().filter(|o| goes(o)).map(|o| (&o.built.bricks[..], o.place, o.spec.blend))).filter(|m| m.2.is_none()).map(|m| (m.0, m.1)).collect();
                let meets = |units: i32| {
                    moved.iter().any(|(bricks, place)| {
                        let mut at = *place;
                        at[axis] += units;
                        stays.iter().any(|stay| stay.meets(cells_of(bricks), at))
                    })
                };
                while units != 0 && meets(units) {
                    units -= units.signum();
                }
            }
            if units == 0 {
                return;
            }
            let limit = (HANDLE_LIMIT * 2.0) as i32;
            let from = self.place[axis];
            self.place[axis] = (self.place[axis] + units).clamp(-limit, limit);
            for other in self.others.iter_mut().filter(|o| goes(o)) {
                other.place[axis] = (other.place[axis] + units).clamp(-limit, limit);
            }
            // (a whole microbrick at a time once joined, as in `move_by`)
            if self.merge {
                let mut lean = [0; 3];
                lean[axis] = -units;
                self.settle_joins(lean);
            }
            self.scene += 1;
            let mut held = held;
            held[axis] += (self.place[axis] - from) as f64 / 2.0;
            self.grab = Some(held);
            return;
        }
        if on_own_grid(&self.spec) {
            let mut at = self.spec.grid_at;
            at[axis] += gone;
            self.set_grid(self.spec.grid, at);
            self.grab = Some(now);
            return;
        }
        let units = (gone * 2.0).round() as i32;
        if units == 0 {
            return;
        }
        let limit = (HANDLE_LIMIT * 2.0) as i32;
        let on = self.spec.on_grid;
        self.place[axis] = (self.place[axis] + units).clamp(-limit, limit);
        for other in self.others.iter_mut().filter(|o| o.spec.on_grid == on) {
            other.place[axis] = (other.place[axis] + units).clamp(-limit, limit);
        }
        self.scene += 1;
        let mut held = held;
        held[axis] += units as f64 / 2.0;
        self.grab = Some(held);
    }

    /// A cube as the menu starts one, to be added to the workspace: every
    /// new form begins as this, in the plain colour, and is made into
    /// whatever is wanted from there.
    ///
    /// Adding a form used to add another of the one being edited. That
    /// was a copy when a copy was wanted and something to undo when it was
    /// not: a second rug, with its pattern and colours, where a wall was
    /// meant. A copy has a button and a key of its own now (see
    /// [`State::add_copy`]). How the form being edited is fitted (its
    /// longest brick, its brick size) is carried over, being a choice
    /// about the build more than about one form.
    fn fresh_cube(&self) -> Spec {
        let fresh = Spec::default();
        let radius = fresh.radius.floor() + 0.5; // as `preset_settings` starts one
        let (points, _) = usual_points(CUBE, radius);
        let s = &self.spec;
        Spec { form: Form::Cube, points, radius, fill: true, max_ext: s.max_ext, grain: s.grain, budget: s.budget, simplify: s.simplify, wedges: s.wedges, ..fresh }
    }

    /// Start another grid with a new cube on it (see [`State::fresh_cube`]),
    /// level until its rings are turned.
    pub fn add_grid(&mut self) {
        let next = std::iter::once(self.spec.on_grid).chain(self.others.iter().map(|o| o.spec.on_grid)).max().unwrap_or(0) + 1;
        self.add_form(Spec { on_grid: next, ..self.fresh_cube() });
        self.lock = Lock::Free;
    }

    /// Add a new cube (see [`State::fresh_cube`]) to the grid of the form
    /// being edited, set down beside what is there.
    pub fn add_cube(&mut self) {
        self.add_form(Spec { on_grid: self.spec.on_grid, grid: self.spec.grid, grid_at: self.spec.grid_at, ..self.fresh_cube() });
        self.lock = Lock::Free;
    }

    /// Add a copy of the form being edited to its grid: the same form,
    /// coloured the same, set down beside it. (A form added from the saved
    /// ones starts in the plain colour: a saved form has none of its own.)
    pub fn add_copy(&mut self) {
        let paint = self.paint.clone();
        self.add_form(self.full_spec());
        self.paint = paint;
    }

    /// Whether another form can be in the way of the one being edited:
    /// one on its own grid, or, the world's grid being one grid however
    /// many level groups share it, any level form when this one is level.
    fn in_my_way(&self, other: &Spec) -> bool {
        other.on_grid == self.spec.on_grid || (!on_own_grid(other) && !on_own_grid(&self.spec))
    }

    /// Where along arrow `axis` the ray from the eye through the cursor
    /// comes nearest it, as a point; `None` if the ray runs along the arrow
    /// or meets it implausibly far off.
    fn on_arrow(&self, axis: usize, origin: DVec3, dir: DVec3) -> Option<DVec3> {
        let (middle, _) = self.gizmo()?;
        let a = [DVec3::X, DVec3::Y, DVec3::Z][axis];
        along_line(middle, a, origin, dir).map(|t| middle + a * t)
    }

    /// Take hold of arrow `axis` where the cursor's ray passes it. Returns
    /// whether it could be.
    pub fn hold_arrow(&mut self, axis: usize, origin: DVec3, dir: DVec3) -> bool {
        self.grab = self.on_arrow(axis, origin, dir);
        self.grab.is_some()
    }

    /// Move the form being edited by so many units, short of any other form
    /// in the way; `held` is the point of it the drag has hold of.
    fn move_by(&mut self, by: [i32; 3], held: DVec3) {
        // not into another form: onto it or beside it, unless Ctrl says into it
        let Some(by) = (if self.merge { Some(by) } else { self.clear_of_others(by) }) else { return };
        if by == [0; 3] {
            return;
        }
        // (joined with the blend it had if it was joined already, and a sharp crease if not)
        self.spec.blend = self.merge.then(|| self.spec.blend.unwrap_or(0.0));
        let limit = (HANDLE_LIMIT * 2.0) as i32;
        let from = self.place;
        self.place = [0, 1, 2].map(|k| (self.place[k] + by[k]).clamp(-limit, limit));
        // A joined form goes a whole microbrick at a time, to stay on the
        // cells of the form it joins: an odd unit short of the pointer
        // here, and the rest when the pointer has gone another unit.
        if self.merge {
            self.settle_joins(by.map(|v| -v));
        }
        let by = [0, 1, 2].map(|k| self.place[k] - from[k]);
        self.scene += 1;
        // what is held moves with the form
        self.grab = Some(held + DVec3::new(by[0] as f64, by[1] as f64, by[2] as f64) / 2.0);
    }

    /// Move `handle` to where the ray from the eye through the cursor meets
    /// its drag plane.
    pub fn drag(&mut self, handle: Handle, origin: DVec3, dir: DVec3, view: DVec3) {
        let place = |state: &State| match handle {
            Handle::Point(i) => state.spec.points.get(i).copied(),
            Handle::Pull(i) => state.spec.pulls.get(i).map(|(at, by)| if state.is_grass() { *at + DVec3::Z * by.z } else { *at + *by }),
            _ => None,
        };
        if let Handle::Spin(axis) = handle {
            // Round its ring, by whole degrees: the grid as it was when the
            // ring was taken hold of, turned by as far as the cursor has
            // gone round since, about the world's axis, as the ring is
            // drawn. What was at the rings' middle stays there.
            let Some((from, was, about_world, middle)) = self.spin_from else { return };
            let Some(now) = self.round_ring(axis, about_world, origin, dir) else { return };
            let by = (now - from).to_degrees().round().to_radians();
            let mut grid = turns(was, about(axis, by));
            // a hair off level after a full turn back is level
            if grid[3].abs() > 1.0 - 1e-9 {
                grid = LEVEL;
            }
            if grid == LEVEL {
                self.set_grid(self.spec.grid, self.spec.grid_at);
                self.level();
            } else {
                let at = about_world - turned_way(grid, middle, false);
                self.set_grid(grid, at.to_array());
            }
            return;
        }
        if let Handle::Slide(axis) = handle {
            self.slide_grid(axis, origin, dir);
            return;
        }
        // Everything else is done within the form's grid: the cursor's ray
        // is taken into the grid first, and then moves the form, or a dot
        // of it, as it would on a level one.
        let (origin, dir, view) = if !on_own_grid(&self.spec) {
            (origin, dir, view)
        } else {
            (into_grid(&self.spec, origin), turned_way(self.spec.grid, dir, true), turned_way(self.spec.grid, view, true))
        };
        let before = place(self);
        // A form that has been turned is dragged as it was before the turn:
        // the cursor's ray is turned back, and then moves the dots of the
        // form as fitted. Not a move of the whole form, which is the same
        // whichever way it faces.
        let turn = brickloft_core::spec::Turn::of(&self.spec.snapped());
        let (origin, dir, view) = if turn.is_none() || handle == Handle::Move {
            (origin, dir, view)
        } else {
            let standing = self.standing();
            (turn.back(origin - standing) + standing, turn.back_way(dir), turn.back_way(view))
        };
        self.drag_alone(handle, origin, dir, view);
        // The dots near the one dragged go with it, by less the further off
        // they are, and by `follow` of its own movement at most: a run of
        // points is then moved as a shape, not one point at a time out of a
        // line the others still hold. Added points follow whichever dot is
        // dragged; the form's own dots follow only one another, since an
        // added point is there to change the form's outline and not where
        // the form is: a circle's middle stays put when its rim is pulled.
        let (Some(before), Some(after)) = (before, place(self)) else { return };
        let moved = after - before;
        if self.follow <= 0.0 || moved == DVec3::ZERO {
            return;
        }
        // (A plant's dots are the ends and bends of its line, set where they
        // are wanted, and its reach of added points starts large for a
        // hedge: scaled by that, a hedge's ends came along when its middle
        // dot was dragged. They follow as far as any form's do at the start.)
        let reach = FOLLOW_REACH * if self.form == TREE { Spec::default().pull_reach } else { self.spec.pull_reach.max(1.0) };
        let share = |at: DVec3| {
            let d = at.distance(before);
            if d < reach { self.follow * 0.5 * (1.0 + (std::f64::consts::PI * d / reach).cos()) } else { 0.0 }
        };
        let on_grid = |v: f64| if self.fine_snap { snap(v) } else { v.floor() + 0.5 };
        let points: Vec<DVec3> = self.spec.points.iter().enumerate().map(|(j, p)| if handle == Handle::Point(j) || matches!(handle, Handle::Pull(_)) { *p } else { (*p + moved * share(*p)).map(on_grid) }).collect();
        // (grass's are touches of a brush, each its own: none follows another)
        let grass = self.is_grass();
        let pulls: Vec<(DVec3, DVec3)> = self.spec.pulls.iter().enumerate().map(|(j, (at, by))| if handle == Handle::Pull(j) || grass { (*at, *by) } else { (*at, (*by + moved * share(*at + *by)).map(snap)) }).collect();
        (self.spec.points, self.spec.pulls) = (points, pulls);
    }

    /// [`State::drag`], for the one dot alone.
    fn drag_alone(&mut self, handle: Handle, origin: DVec3, dir: DVec3, view: DVec3) {
        // A point added to a circle is dragged within the circle's plane, or
        // with Ctrl held, straight out of it: never a little of both, since
        // a curve pulled even slightly out of its plane can no longer be
        // built as a flat band (see `brickloft_core::spec::pulled`), and a drag
        // in the plane facing the eye is nearly always a little of both.
        // Which of the two was once decided by how the plane was seen, face
        // on or from the side; but a ring lying flat is usually looked at
        // from somewhere between, and the point then went up when it was
        // meant to go out. Only seen almost exactly edge on, where the
        // cursor cannot pick a place in the plane, does it go out of it
        // unasked.
        if let (Handle::Pull(i), CIRCLE, Lock::Free) = (handle, self.form, self.lock) {
            let Some(&(from, by)) = self.spec.pulls.get(i) else { return };
            let axis = self.spec.plane.min(2);
            let normal = [DVec3::X, DVec3::Y, DVec3::Z][axis];
            if self.lift || view.dot(normal).abs() < 0.15 {
                let mut within = by;
                within[axis] = 0.0;
                let foot = from + within + self.standing();
                let Some(t) = along_line(foot, normal, origin, dir) else { return };
                self.spec.pulls[i].1 = within + normal * snap(t);
                return;
            }
            // face on: in the plane, as if the plane's axis were locked
            let (was, kept) = (self.lock, by[axis]);
            self.lock = Lock::Axis(axis);
            self.drag_alone(handle, origin, dir, view);
            self.lock = was;
            self.spec.pulls[i].1[axis] = kept;
            return;
        }
        // A touch on grass's ground is dragged straight up and down, which
        // changes how much it pushes, or the height it flattens to. Where
        // it is and how far it reaches stay as they are.
        if let (Handle::Pull(i), true) = (handle, self.is_grass()) {
            let Some(&(from, by)) = self.spec.pulls.get(i) else { return };
            // With Ctrl held it is a stroke: the brush is dragged over
            // the ground, and wherever the cursor has gone 0.4 of the
            // brush's range from the last touch, the same touch is made
            // again there. A flattening stroke keeps the level it began
            // at, as such a tool does. The cursor's place is taken on
            // the level the last touch left its own middle at.
            if self.lift {
                let Some(&(last, same)) = self.spec.pulls.last() else { return };
                let level = last.z.floor() + brickloft_core::spec::grass_ground(&self.spec.snapped(), last) + self.standing().z;
                if dir.z.abs() < 1e-6 || self.spec.pulls.len() >= TOUCHES {
                    return;
                }
                let along = (level - origin.z) / dir.z;
                let hit = origin + dir * along - self.standing();
                let here = DVec3::new(hit.x.round().clamp(-HANDLE_LIMIT, HANDLE_LIMIT), hit.y.round().clamp(-HANDLE_LIMIT, HANDLE_LIMIT), last.z);
                let apart = same.x.max(brickloft_core::tree::LEAST_RANGE) * 0.4;
                if along > 0.0 && along < 2000.0 && here.truncate().distance(last.truncate()) >= apart {
                    self.spec.pulls.push((here, same));
                }
                return;
            }
            let Some(up) = along_line(from + self.standing(), DVec3::Z, origin, dir) else { return };
            let floor = from.z.floor() - from.z;
            let amount = snap(up).clamp(-HANDLE_LIMIT, HANDLE_LIMIT);
            self.spec.pulls[i].1.z = if Brush::flattens(by) { amount.max(floor.min(0.0)).max(0.0) } else { amount };
            return;
        }
        let held = if handle == Handle::Move { self.grab } else { self.dots().iter().find(|(h, _)| *h == handle).map(|h| h.1) };
        let Some(at) = held else { return };
        // A side of a cube moves along the one axis it faces, to wherever
        // on that axis the cursor's ray comes nearest, as an arrow does. In
        // the plane facing the eye, as the radius dot is dragged, a side
        // that faces away from the eye hardly moved at all: the cursor
        // going up the screen is then mostly a movement upwards.
        //
        // The side opposite stays where it is, so the middle moves by half
        // as much. A circle's middle stays put when its rim is pulled, and
        // a cube could have done the same; but a box is laid against
        // things, a floor up to a wall, and with both sides moving at once
        // the one already in place would have to be put back after every
        // change to the other.
        if let Handle::Side(side) = handle {
            let snapped = self.spec.snapped();
            let Some(&middle) = snapped.points.first() else { return };
            let (axis, (low, high)) = (side / 2, brickloft_core::spec::cube_sides(&snapped, middle)[(side / 2).min(2)]);
            let Some(gone) = along_line(at, [DVec3::X, DVec3::Y, DVec3::Z][axis], origin, dir) else { return };
            // on a whole microbrick, never past the side opposite, and no further out than any dot goes
            let (low, high) = if side % 2 == 0 { ((low + gone).round().clamp(-HANDLE_LIMIT, high - 1.0), high) } else { (low, (high + gone).round().clamp(low + 1.0, HANDLE_LIMIT)) };
            // The same sides as a middle, half a side and a stretch, which
            // is how the form is kept (see `Form::Cube`): the radius and
            // the other four sides exactly as the build has them now.
            self.spec.radius = snapped.radius;
            self.spec.points[0] = middle;
            self.spec.points[0][axis] = (low + high) / 2.0;
            self.spec.stretch[axis] = (high - low) / 2.0 / snapped.radius;
            // A cube given quarter turns is turned about its middle, which
            // has just moved: left at that, the whole cube shifted as a
            // side was dragged, and the side opposite did not stay put.
            // So the cube is moved back by as far as a point of that side
            // has gone. (Whole cells: a turn is about a lattice point.)
            let (was, now) = (brickloft_core::spec::Turn::of(&snapped), brickloft_core::spec::Turn::of(&self.spec.snapped()));
            let mut kept = middle;
            kept[axis] = if side % 2 == 0 { high } else { low };
            let back = ((was.point(kept) - now.point(kept)) * 2.0).round();
            if back != DVec3::ZERO {
                self.place = [0, 1, 2].map(|k| self.place[k] + back[k] as i32);
                self.scene += 1;
            }
            return;
        }
        let normal = match (handle, self.lock) {
            (Handle::Point(_) | Handle::Move | Handle::Pull(_), Lock::Axis(a)) => [DVec3::X, DVec3::Y, DVec3::Z][a],
            _ => view,
        };
        // A plane seen nearly edge on is no good to drag in: the cursor's
        // ray runs almost along it, and a pixel of mouse travel is many
        // cells along the plane, so a dot kept to X = somewhere and seen
        // from the side shot off into the distance. Seen flatter than about
        // 17 degrees, the dot is dragged in the plane facing the eye
        // instead, which moves it as far as the cursor moves, and the kept
        // axis is put back afterwards (below), so it still does not leave
        // its plane.
        let normal = if view.dot(normal).abs() < EDGE_ON { view } else { normal };
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
        // the form's own points are where they would be if it stood at the origin
        let standing = self.standing();
        let (hit, at) = if handle == Handle::Move { (hit, at) } else { (hit - standing, at - standing) };
        let _ = at;
        match handle {
            Handle::Point(i) => {
                let mut to = DVec3::new(step(hit.x), step(hit.y), step(hit.z));
                if let Lock::Axis(a) = self.lock {
                    to[a] = self.spec.points[i][a]; // exactly, whatever rounding did
                }
                self.spec.points[i] = to;
            }
            Handle::Move => {
                // The whole form, a unit at a time: half a cell. Only where
                // it stands changes, so it is the same bricks wherever it is.
                let mut by = [0, 1, 2].map(|k| ((hit[k] - at[k]) * 2.0).round() as i32);
                if let Lock::Axis(a) = self.lock {
                    by[a] = 0;
                }
                self.move_by(by, at);
            }
            Handle::Slide(_) | Handle::Spin(_) | Handle::Side(_) => unreachable!("answered above"),
            Handle::Pull(i) => {
                let Some(&(from, _)) = self.spec.pulls.get(i) else { return };
                let mut by = DVec3::new(snap(hit.x - from.x), snap(hit.y - from.y), snap(hit.z - from.z));
                if let Lock::Axis(a) = self.lock {
                    by[a] = 0.0; // kept in the curve's own plane
                }
                self.spec.pulls[i].1 = by;
            }
            Handle::Radius if self.form == TREE => {
                let (foot, across) = self.tree_middle();
                self.spec.radius = snap((hit - foot).dot(across).abs()).clamp(brickloft_core::tree::LEAST_SPREAD, HANDLE_LIMIT);
            }
            Handle::Radius if self.form == IMPORTED => {
                // (from its foot: the dot is one radius out if its longest side lies level, and two up if that side is its height)
                let (foot, along) = (self.spec.points[0], self.radius_dir());
                let out = (hit - DVec3::new(foot.x, foot.y, foot.z.floor())).dot(along) / (1.0 + along.z);
                self.spec.radius = snap(out.abs()).clamp(1.0, HANDLE_LIMIT);
            }
            Handle::Radius => {
                let out = (hit - self.spec.points[0]).dot(self.radius_dir());
                self.spec.radius = snap(out.abs()).clamp(0.5, HANDLE_LIMIT);
            }
            Handle::Rise if self.form == MAZE => {
                let height = (hit.z - self.spec.points[0].z.floor() - self.maze_height().1).round();
                self.spec.rise = Some(height.clamp(1.0, HANDLE_LIMIT));
            }
            Handle::Rise if self.form == TREE => {
                let height = snap(hit.z - self.spec.points[0].z.floor()).round();
                self.spec.rise = Some(height.clamp(brickloft_core::tree::Tree::least_height(self.spec.tree.kind), HANDLE_LIMIT));
            }
            Handle::Rise => {
                let mid = (self.spec.points[0] + self.spec.points[1]) / 2.0;
                self.spec.rise = Some(snap(hit.z - mid.z).clamp(0.5, HANDLE_LIMIT));
            }
        }
    }

    pub fn can_add_points(&self) -> bool {
        matches!(self.form, ARCH | SPLINE | BEZIER | REVOLVE)
    }

    /// Add a point. To an arch, between its feet: the first at its crown,
    /// each one after half way between the last two, so that the feet stay
    /// its ends. To the other curves, a little beyond the last.
    /// Whether points can be added to the form to pull it out of shape.
    /// Every form on offer takes them.
    pub fn can_pull(&self) -> bool {
        // (but a tree, which is grown and not drawn; a hedge and reeds are bent by them as a curve is, and grass's ground is raised)
        // (nor a maze, whose walls are where its cells are)
        // (nor furniture, which is boards: there is no curve in it to pull)
        // (nor a mesh, which is what its file says it is)
        !matches!(self.form, MAZE | FURNITURE | IMPORTED | PROGRAM) && (self.form != TREE || brickloft_core::tree::Tree::lies_along(self.spec.tree.kind))
    }

    /// Add a point to the curve nearest `at`, a place in the view (where a
    /// brick was clicked), to be dragged from there. Returns its handle, or
    /// `None` if the form is not a curve or there are already enough.
    ///
    /// The point takes hold of the curve where it is added and moves that
    /// part of it as it is dragged; the form's own points and sizes stay as
    /// they were. A bump on a circle could be had by turning the circle into
    /// a curve through many points, but then it would be a circle no longer:
    /// its radius could not be changed, nor the bump taken off again.
    pub fn add_pull(&mut self, at: DVec3) -> Option<Handle> {
        if !self.can_pull() || self.spec.pulls.len() >= most_pulls(&self.full_spec()) {
            return None;
        }
        // (where it was clicked, as it is in the form's own grid)
        let at = into_grid(&self.spec, at) - self.standing();
        // On a curve, or a profile spun about an axis, the point takes hold
        // of the curve as last built where it comes nearest the click. A
        // sphere or a pyramid has no curve: the point takes hold of its
        // surface where it was clicked. Either way the place is kept as it
        // is in the form before any turn, as all the form's points are.
        let on = match self.form {
            SPHERE | PYRAMID => at,
            // grass: where it was clicked, on the ground
            TREE if self.spec.tree.kind == brickloft_core::tree::Kind::Grass => DVec3::new(at.x.round(), at.y.round(), self.spec.points.first().map_or(0.0, |p| p.z)),
            _ => self.built.path.iter().copied().min_by(|a, b| a.distance_squared(at).total_cmp(&b.distance_squared(at)))?,
        };
        let on = brickloft_core::spec::Turn::of(&self.spec.snapped()).back(on);
        if self.is_grass() {
            // A touch of the brush as the panel has it. Flattening is to
            // the height the ground has where it was touched, as it is
            // in the games that have such a tool: touch a hilltop to make
            // a terrace at its height, or the flat beside a hill to level
            // the hill's foot.
            let level = if self.brush.action == Action::Flatten { snap(brickloft_core::spec::grass_ground(&self.spec.snapped(), on)) } else { 0.0 };
            self.spec.pulls.push((on, self.brush.touch(level)));
            // The brush stays the brush: the panel's sliders set what the
            // next touch will be, and change no touch already made. A
            // touch just made, or one dragged, used to become the one the
            // sliders changed, so widening the range for the next stroke
            // widened the last one, which is not what a brush does. A
            // touch is changed by the panel only when asked (see
            // `panel::brush`).
            self.touched = None;
            return Some(Handle::Pull(self.spec.pulls.len() - 1));
        }
        self.spec.pulls.push((on, DVec3::ZERO));
        Some(Handle::Pull(self.spec.pulls.len() - 1))
    }

    /// Whether the form being edited is a patch of grass.
    pub fn is_grass(&self) -> bool {
        self.form == TREE && self.spec.tree.kind == brickloft_core::tree::Kind::Grass
    }

    /// Take off the point added last.
    pub fn remove_pull(&mut self) {
        self.spec.pulls.pop();
    }

    /// Take off one added point, by its number: Shift + right click on it.
    pub fn remove_pull_at(&mut self, which: usize) {
        if which < self.spec.pulls.len() {
            self.spec.pulls.remove(which);
        }
    }

    pub fn add_point(&mut self) {
        let grid = |q: DVec3| DVec3::new(q.x.floor() + 0.5, q.y.floor() + 0.5, q.z.floor() + 0.5);
        if self.form == ARCH {
            let n = self.spec.points.len();
            if n >= ARCH_POINTS || n < 2 {
                return;
            }
            let p = &self.spec.points;
            let between = if n == 2 { (p[0] + p[1]) / 2.0 + DVec3::Z * self.arch_rise() } else { (p[n - 2] + p[n - 1]) / 2.0 };
            self.spec.points.insert(n - 1, grid(between));
            return;
        }
        let p = &mut self.spec.points;
        let n = p.len();
        let step = if n >= 2 { p[n - 1] - p[n - 2] } else { DVec3::new(4.0, 0.0, 0.0) };
        let next = p[n - 1] + step.clamp_length(2.0, 6.0);
        p.push(grid(next));
        if self.form == BEZIER {
            self.spec.weights.resize(self.spec.points.len(), 1.0);
        }
    }

    /// Take a point away: from an arch the last one between its feet, from
    /// the other curves the last. Two always stay.
    pub fn remove_point(&mut self) {
        let n = self.spec.points.len();
        if n > 2 {
            self.spec.points.remove(if self.form == ARCH { n - 2 } else { n - 1 });
            self.spec.weights.truncate(self.spec.points.len());
        }
    }
}

/// A form's bricks where they stand: its place in units, the box that
/// holds them all, and the cells of their boxes.
///
/// The cells are listed only when something is asked about that comes
/// inside the box. Listing them is the costly part (a sphere of radius 40
/// has a quarter of a million), and it was done for every form on the grid
/// each time the form being edited was rebuilt or moved, whether or not
/// anything was near: a cube being resized beside a large sphere listed
/// the sphere's cells many times a second, to find each time that the two
/// were apart.
struct Standing {
    bricks: Vec<CellBrick>,
    cells: std::cell::OnceCell<std::collections::HashSet<[i32; 3]>>,
    /// the box of all its bricks, in cells
    low: [i32; 3],
    high: [i32; 3],
    place: [i32; 3],
}

impl Standing {
    fn new(bricks: &[CellBrick], place: [i32; 3]) -> Standing {
        let low = [0, 1, 2].map(|k| bricks.iter().map(|b| b.min[k]).min().unwrap_or(0));
        let high = [0, 1, 2].map(|k| bricks.iter().map(|b| b.min[k] + b.ext[k]).max().unwrap_or(0));
        Standing { bricks: bricks.to_vec(), cells: std::cell::OnceCell::new(), low, high, place }
    }

    /// Whether any of `cells`, of a form standing at `place`, overlaps this one.
    ///
    /// The two forms' cells need not line up: one may stand an odd unit off
    /// the other along any axis. A cell of the one then lies across two of
    /// the other's along that axis, so up to eight are looked at for each.
    /// Laying every cell out as eight unit cubes would line everything up
    /// and take eight times the memory to say the same thing.
    fn meets(&self, mut cells: impl Iterator<Item = [i32; 3]>, place: [i32; 3]) -> bool {
        // in this form's units; a cell is two units along each axis
        let off = [0, 1, 2].map(|k| place[k] - self.place[k]);
        cells.any(|c| {
            let low = [0, 1, 2].map(|k| 2 * c[k] + off[k]);
            // (outside the box of this form: no need to know its cells)
            if (0..3).any(|k| low[k] + 2 <= 2 * self.low[k] || low[k] >= 2 * self.high[k]) {
                return false;
            }
            let mine = self.cells.get_or_init(|| cells_of(&self.bricks).collect());
            let first = low.map(|u| u.div_euclid(2));
            let more = low.map(|u| u.rem_euclid(2));
            (0..=more[2]).any(|z| (0..=more[1]).any(|y| (0..=more[0]).any(|x| mine.contains(&[first[0] + x, first[1] + y, first[2] + z]))))
        })
    }
}

/// Where a form's cells end up if moved by `by` units from `place`: there,
/// if that is clear of the forms in `taken`, or else the nearest place
/// straight up or straight out to one side that is. `None` if there is none
/// within reach.
///
/// Up or sideways and never down, so that a form brought down onto another
/// comes to rest on top of it.
fn way_out(mine: &[[i32; 3]], place: [i32; 3], taken: &[Standing], by: [i32; 3]) -> Option<[i32; 3]> {
    let hits = |shift: [i32; 3]| {
        let at = [0, 1, 2].map(|k| place[k] + shift[k]);
        taken.iter().any(|t| t.meets(mine.iter().copied(), at))
    };
    if !hits(by) {
        return Some(by);
    }
    // no further, in units, than would lift or slide it clear of everything
    let span = |k: usize| {
        let theirs = taken.iter().flat_map(|t| [2 * t.low[k] + t.place[k], 2 * (t.high[k] - 1) + t.place[k]]);
        let all = theirs.chain(mine.iter().map(|c| 2 * c[k] + place[k] + by[k]));
        let (lo, hi) = all.fold((i32::MAX, i32::MIN), |(lo, hi), v| (lo.min(v), hi.max(v)));
        hi - lo + 4
    };
    let furthest = span(0).max(span(1)).max(span(2));
    for d in 1..=furthest {
        // up first: of two ways out equally near, onto is the one meant
        for way in [[0, 0, 1], [1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0]] {
            let shift = [by[0] + way[0] * d, by[1] + way[1] * d, by[2] + way[2] * d];
            if !hits(shift) {
                return Some(shift);
            }
        }
    }
    None
}

/// Every cell in the boxes of some bricks.
fn cells_of(bricks: &[CellBrick]) -> impl Iterator<Item = [i32; 3]> + '_ {
    bricks.iter().flat_map(|b| {
        let (min, ext) = (b.min, b.ext);
        (0..ext[2]).flat_map(move |z| (0..ext[1]).flat_map(move |y| (0..ext[0]).map(move |x| [min[0] + x, min[1] + y, min[2] + z])))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_preset_builds() {
        let mut state = State::new();
        // (a rule program shown finished: watched, it starts from an empty grid, which is no bricks yet)
        state.grow.watch = false;
        // (but a mesh, which has nothing to build until a file is opened for it: see `import::tests`)
        for form in (0..FORMS.len()).filter(|form| *form != IMPORTED) {
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
    fn an_arch_takes_more_points_and_gives_them_back() {
        let mut state = State::new();
        state.form = ARCH;
        state.preset();
        let feet = state.spec.points.clone();
        assert!(state.has_rise() && matches!(state.full_spec().form, Form::Arch(_)));
        // a third point goes in at the crown, and the arch is the arc through all three
        state.add_point();
        assert_eq!(state.spec.points.len(), 3);
        assert_eq!((state.spec.points[0], state.spec.points[2]), (feet[0], feet[1]));
        assert!(state.spec.points[1].z > feet[0].z + 4.0 && state.spec.points[1].x.abs() < 1.0);
        assert!(!state.has_rise() && state.full_spec().form == Form::Arc);
        assert!(state.refresh(|| 0.0) && state.problem.is_none());
        // more make it the curve through them all, up to the limit, with the feet still its ends
        for _ in 0..10 {
            state.add_point();
        }
        assert_eq!(state.spec.points.len(), ARCH_POINTS);
        assert_eq!((state.spec.points[0], state.spec.points[ARCH_POINTS - 1]), (feet[0], feet[1]));
        assert_eq!(state.full_spec().form, Form::Spline);
        assert!(state.handles().iter().all(|(h, _)| *h != Handle::Rise));
        // and taken away again, down to the two feet
        for _ in 0..10 {
            state.remove_point();
        }
        assert_eq!(state.spec.points, feet);
        // a saved arc opens as an arch with three points
        let mut arc = state.full_spec();
        arc.form = Form::Arc;
        arc.points.insert(1, cell(0.0, 0.0, 6.0));
        state.load_spec(arc);
        assert_eq!((state.form, state.spec.points.len()), (ARCH, 3));
    }

    #[test]
    fn a_form_is_measured_once_the_mouse_is_let_go() {
        let mut state = State::new();
        state.form = SPHERE;
        state.preset();
        state.refresh(|| 0.0);
        assert!(state.built.solid > 0.0 && !state.built.unmeasured);
        // dragged: bricks, and no measurement
        state.busy = true;
        state.spec.radius += 2.0;
        assert!(state.refresh(|| 0.0));
        assert!(state.built.unmeasured && state.built.solid == 0.0 && !state.built.bricks.is_empty());
        let bricks = state.built.bricks.clone();
        assert!(!state.refresh(|| 0.0), "nothing changed, and it is still being dragged");
        // let go: the same bricks, measured
        state.busy = false;
        assert!(state.refresh(|| 0.0));
        assert!(!state.built.unmeasured && state.built.solid > 0.0 && state.built.error > 0.0);
        assert_eq!(state.built.bricks, bricks);
        assert!(!state.refresh(|| 0.0));
    }

    #[test]
    fn reshaping_a_form_pushes_its_neighbour_away() {
        let mut state = State::new();
        state.form = SPHERE;
        state.preset();
        state.spec.radius = 4.5;
        state.refresh(|| 0.0);
        let mut ball = State::new();
        ball.form = SPHERE;
        ball.preset();
        ball.spec.radius = 4.5;
        state.add_form(ball.full_spec());
        state.refresh(|| 0.0);
        let overlap = |state: &State| Standing::new(&state.others[0].built.bricks, state.others[0].place).meets(cells_of(&state.built.bricks), state.place);
        assert!(!overlap(&state));
        let (was, bricks) = (state.others[0].place, state.others[0].built.bricks.clone());
        // grown until it would swallow half of the other
        state.spec.radius = 12.5;
        assert!(state.refresh(|| 0.0));
        assert!(!overlap(&state), "the two overlap");
        let other = &state.others[0];
        assert_ne!(other.place, was, "the other form did not move");
        // moved whole: only where it stands has changed
        assert_eq!(other.built.bricks, bricks);
    }

    #[test]
    fn a_point_added_to_a_circle_pulls_it_out_of_shape_there() {
        let mut state = State::new();
        state.form = CIRCLE;
        state.preset();
        state.refresh(|| 0.0);
        let (radius, centre) = (state.spec.radius, state.spec.points[0]);
        // which way the circle faces, and a direction out from its middle within its plane
        let across = state.spec.plane.min(2);
        let (normal, out) = ([DVec3::X, DVec3::Y, DVec3::Z][across], state.radius_dir());
        let far = |state: &State, way: DVec3| {
            let (lo, hi) = state.bounds.unwrap();
            (0..3).map(|k| if way[k] > 0.5 { hi[k] as f64 } else if way[k] < -0.5 { -lo[k] as f64 } else { 0.0 }).sum::<f64>()
        };
        let (out_before, up_before) = (far(&state, out), far(&state, normal));
        // clicked a little off the circle, and off its plane: the point lands on the curve
        let handle = state.add_pull(centre + out * (radius + 0.8) + normal * 0.6).unwrap();
        assert_eq!(handle, Handle::Pull(0));
        let held = state.spec.pulls[0].0;
        assert!((held - (centre + out * radius)).length() < 1.0, "held at {held:?}");
        assert_eq!(state.handles().iter().find(|h| h.0 == handle).unwrap().1, held);
        // Seen face on and dragged five cells outwards, with the cursor's
        // ray a little askew: it moves in the plane and not out of it.
        let target = held + out * 5.0;
        let askew = (normal + out * 0.05).normalize();
        state.drag(handle, target - askew * 80.0, askew, normal);
        let by = state.spec.pulls[0].1;
        assert!((by.dot(out) - 5.0).abs() <= 0.5 && by[across] == 0.0, "pulled by {by:?}");
        assert!(state.refresh(|| 0.0));
        assert!(state.problem.is_none(), "{:?}", state.problem);
        assert!(far(&state, out) >= out_before + 4.0, "it did not grow outwards");
        assert_eq!(far(&state, normal), up_before, "it left its plane");
        // Seen from well to one side, it still moves in the plane: a ring
        // lying flat is mostly looked at from above and to one side.
        let slanted = (normal * 0.4 + out * 0.9).normalize();
        let further = held + out * 7.0;
        state.drag(handle, further - slanted * 80.0, slanted, slanted);
        assert!((state.spec.pulls[0].1.dot(out) - 7.0).abs() <= 0.5 && state.spec.pulls[0].1[across] == 0.0, "pulled by {:?}", state.spec.pulls[0].1);
        state.spec.pulls[0].1 = by;
        // With Ctrl held, the same point goes straight out of the plane
        // and keeps what it had within it.
        state.lift = true;
        let at = state.handles().iter().find(|h| h.0 == handle).unwrap().1;
        let side = normal.cross(out).normalize();
        let eye = at - side * 80.0;
        state.drag(handle, eye, (at + normal * 3.0 + out * 0.4 - eye).normalize(), side);
        let lifted = state.spec.pulls[0].1;
        assert!((lifted[across] - 3.0).abs() <= 0.5 && (lifted.dot(out) - by.dot(out)).abs() < 1e-9, "pulled by {lifted:?}");
        assert!(state.refresh(|| 0.0) && state.problem.is_none(), "{:?}", state.problem);
        assert!(far(&state, normal) >= up_before + 2.0, "it did not rise out of its plane");
        // Further than the circle is wide, both ways: an added point is not
        // held within the circle's own size.
        for lift in [4.0, 9.0, 20.0, 45.0] {
            let at = state.handles().iter().find(|h| h.0 == handle).unwrap().1;
            let eye = at - side * 80.0;
            let target = centre + out * radius + by + normal * lift;
            state.drag(handle, eye, (target - eye).normalize(), side);
            assert!((state.spec.pulls[0].1[across] - lift).abs() <= 0.5, "asked for {lift}, got {:?}", state.spec.pulls[0].1);
            assert!(state.refresh(|| 0.0) && state.problem.is_none(), "{:?}", state.problem);
            assert!(far(&state, normal) >= up_before + lift - 2.0, "lifted {lift}, the bricks reach {}", far(&state, normal));
        }
        state.spec.pulls[0].1 = by;
        state.lift = false;
        for reach in [9.0, 20.0, 45.0] {
            let target = held + out * reach;
            state.drag(handle, target - askew * 80.0, askew, normal);
            assert!((state.spec.pulls[0].1.dot(out) - reach).abs() <= 0.5, "asked for {reach}, got {:?}", state.spec.pulls[0].1);
            assert!(state.refresh(|| 0.0) && state.problem.is_none(), "{:?}", state.problem);
            assert!(far(&state, out) >= out_before + reach - 2.0, "pulled {reach}, the bricks reach {}", far(&state, out));
        }
        state.spec.pulls[0].1 = by;
        state.refresh(|| 0.0);
        let top_before = out_before;
        // the circle is still a circle: its radius and centre are as they were
        assert_eq!((state.spec.radius, state.spec.points[0]), (radius, centre));
        // one of several is taken off by its number, and the others are left as they were
        state.spec.pulls.push((held + DVec3::Z, DVec3::Z));
        let first = state.spec.pulls[0];
        state.remove_pull_at(1);
        state.remove_pull_at(7);
        assert_eq!(state.spec.pulls, vec![first]);
        // taken off again, it is round again; and a solid has no such points
        state.remove_pull();
        state.refresh(|| 0.0);
        assert_eq!(far(&state, out), top_before);
        // a sphere takes a point where its surface is clicked, and bulges there
        state.form = SPHERE;
        state.preset();
        state.refresh(|| 0.0);
        let wide = state.bounds.unwrap().1[0];
        let side = state.spec.points[0] + DVec3::X * state.spec.radius;
        let handle = state.add_pull(side).unwrap();
        assert_eq!(state.spec.pulls[0].0, side);
        state.drag(handle, side + DVec3::new(4.0, -80.0, 0.0), DVec3::Y, DVec3::Y);
        assert!((state.spec.pulls[0].1.x - 4.0).abs() <= 0.5);
        assert!(state.refresh(|| 0.0) && state.problem.is_none(), "{:?}", state.problem);
        assert!(state.bounds.unwrap().1[0] >= wide + 2, "the sphere did not bulge");
    }

    #[test]
    fn reset_puts_back_the_sizes_and_choices_as_well_as_the_dots() {
        let mut state = State::new();
        state.form = PYRAMID;
        state.preset();
        let dots = state.spec.points.clone();
        state.spec.points[0] += DVec3::new(3.0, 0.0, 2.0);
        // Reset puts back the sizes and choices too, not only the dots
        state.spec.stretch = DVec3::new(2.0, 0.5, 1.0);
        state.spec.symmetric = true;
        state.spec.mirror = [true, true, false];
        state.spec.flip = [true, false, true];
        state.spec.radius = 31.0;
        let fit = (state.spec.simplify, state.spec.budget);
        state.reset();
        let fresh = Spec::default();
        assert_eq!((state.spec.stretch, state.spec.symmetric, state.spec.flip), (fresh.stretch, fresh.symmetric, fresh.flip));
        assert_eq!(state.spec.mirror, fresh.mirror);
        assert_eq!(state.spec.radius, fresh.radius.floor() + 0.5, "a pyramid starts half a cell over a whole radius");
        assert_eq!(state.spec.points, dots);
        // and leaves how it is fitted alone
        assert_eq!((state.spec.simplify, state.spec.budget), fit);
    }

    #[test]
    fn the_dots_near_a_dragged_one_go_with_it() {
        let mut state = State::new();
        state.form = SPLINE; // four points in a row, held in their upright plane
        state.preset();
        let was = state.spec.points.clone();
        let drag_up = |state: &mut State, by: f64| {
            let at = state.handles()[1].1;
            state.drag(Handle::Point(1), at + DVec3::new(0.0, -90.0, by), DVec3::Y, DVec3::Y);
        };
        // on its own
        state.follow = 0.0;
        drag_up(&mut state, 6.0);
        assert_eq!(state.spec.points[1], was[1] + DVec3::Z * 6.0);
        assert!([0, 2, 3].iter().all(|k| state.spec.points[*k] == was[*k]));
        // with its neighbours following: the nearer by more, none by more than it, none off the grid
        state.spec.points = was.clone();
        state.follow = 0.8;
        drag_up(&mut state, 6.0);
        let rose: Vec<f64> = (0..4).map(|k| state.spec.points[k].z - was[k].z).collect();
        assert_eq!(rose[1], 6.0);
        assert!(rose[0] > 0.0 && rose[2] > 0.0 && rose[0] < 6.0 && rose[2] < 6.0, "{rose:?}");
        assert!(rose[3] <= rose[2], "{rose:?}");
        assert!(state.spec.points.iter().all(|p| (p.z - 0.5).fract() == 0.0 && p.y == was[0].y), "{:?}", state.spec.points);
        assert!(state.refresh(|| 0.0) && state.problem.is_none());
        // an added point follows too
        state.spec.points = was.clone();
        state.spec.pulls = vec![(was[1] + DVec3::new(1.0, 0.0, 1.0), DVec3::ZERO)];
        drag_up(&mut state, 6.0);
        assert!(state.spec.pulls[0].1.z > 3.0 && state.spec.pulls[0].1.z <= 6.0, "{:?}", state.spec.pulls);
    }

    #[test]
    fn a_turned_form_shows_its_dots_where_it_now_is_and_is_dragged_by_them() {
        let mut state = State::new(); // an arch, standing up along z across x
        state.refresh(|| 0.0);
        let tall = state.bounds.unwrap().1[2] - state.bounds.unwrap().0[2];
        let rise_before = state.arch_rise();
        // a quarter turn about x lays it down along y
        state.turn(0, false);
        assert!(state.refresh(|| 0.0) && state.problem.is_none());
        let (lo, hi) = state.bounds.unwrap();
        assert_eq!(hi[1] - lo[1], tall, "it is not lying along y");
        // its crown's dot is at the crown as it lies: out along y, not up
        let (_, crown) = *state.handles().iter().find(|h| h.0 == Handle::Rise).unwrap();
        let (_, foot) = state.handles()[0];
        assert!((crown.y - foot.y).abs() > rise_before - 1.0 && (crown.z - foot.z).abs() < 1.0, "crown {crown:?}, foot {foot:?}");
        // pulled three cells further out along y, it is three cells taller as an arch
        let further = crown + DVec3::Y * 3.0 * (crown.y - foot.y).signum();
        state.drag(Handle::Rise, further + DVec3::Z * 80.0, -DVec3::Z, -DVec3::Z);
        assert!((state.arch_rise() - (rise_before + 3.0)).abs() <= 0.5, "rise {} from {rise_before}", state.arch_rise());
        // four turns bring it back
        for _ in 0..3 {
            state.turn(0, false);
        }
        assert_eq!(state.spec.turned, brickloft_core::shapes::UPRIGHT);
        state.turn(2, false);
        state.turn(2, true);
        assert_eq!(state.spec.turned, brickloft_core::shapes::UPRIGHT);
    }

    #[test]
    fn a_maze_starts_at_its_usual_size_with_dots_for_its_radius_and_wall_height() {
        use brickloft_core::maze::{Maze, USUAL_HEIGHT, USUAL_SIZE};
        let mut state = State::new();
        state.spec.maze.seed = 5;
        state.form = MAZE;
        state.preset();
        assert_eq!((state.spec.radius, state.spec.thick, state.spec.rise), (USUAL_SIZE.0, USUAL_SIZE.1, Some(USUAL_HEIGHT)));
        assert_eq!(state.spec.maze, Maze { seed: 5, ..Maze::default() }, "the seed is not kept, or the rest is not as a maze starts");
        assert_eq!((state.paint.mode, &state.paint.colours), (crate::paint::Mode::Pattern, &crate::paint::maze_palettes("none")[0]));
        assert_eq!(state.full_spec().form, Form::Maze);
        assert!(state.refresh(|| 0.0) && state.problem.is_none(), "{:?}", state.problem);
        // a dot at its middle, one over it at the top of the walls and one on the rim, half way up
        let dots = state.handles();
        assert_eq!(dots.iter().map(|d| d.0).collect::<Vec<_>>(), [Handle::Point(0), Handle::Rise, Handle::Radius]);
        let (top, rim) = (dots[1].1, dots[2].1);
        assert_eq!((top.x, top.y, top.z), (0.5, 0.5, USUAL_HEIGHT));
        assert_eq!((rim.x, rim.y, rim.z), (0.5 + USUAL_SIZE.0, 0.5, USUAL_HEIGHT / 2.0));
        let high = |state: &State| state.built.bricks.iter().map(|b| b.min[2] + b.ext[2]).max().unwrap();
        let far = |state: &State| state.built.bricks.iter().map(|b| b.min[0] + b.ext[0]).max().unwrap();
        assert_eq!((high(&state), far(&state)), (USUAL_HEIGHT as i32, (USUAL_SIZE.0 + USUAL_SIZE.1 / 2.0) as i32), "the dots are not where the bricks end");
        // the top dot dragged down ten cells lowers the walls by ten, and the bricks follow
        let eye = DVec3::new(0.5, -400.0, top.z);
        state.drag(Handle::Rise, eye, (top - DVec3::Z * 10.0 - eye).normalize(), DVec3::Y);
        assert_eq!(state.spec.rise, Some(USUAL_HEIGHT - 10.0));
        // with a floor under them the walls are as tall as they were, and the dot is at their top
        state.spec.maze.floor = 2;
        assert!(state.refresh(|| 0.0) && state.problem.is_none(), "{:?}", state.problem);
        assert_eq!((high(&state), state.handles()[1].1.z), (USUAL_HEIGHT as i32 - 8, USUAL_HEIGHT - 8.0));
        let top = state.handles()[1].1;
        state.drag(Handle::Rise, DVec3::new(0.5, -400.0, top.z), (top + DVec3::Z * 3.0 - DVec3::new(0.5, -400.0, top.z)).normalize(), DVec3::Y);
        assert_eq!(state.spec.rise, Some(USUAL_HEIGHT - 7.0));
        // the rim dot dragged out thirty cells makes the maze thirty larger: wider corridors, the same rings
        let eye = DVec3::new(rim.x, -400.0, rim.z);
        state.drag(Handle::Radius, eye, (rim + DVec3::X * 30.0 - eye).normalize(), DVec3::Y);
        assert_eq!((state.spec.radius, state.spec.maze.rings), (USUAL_SIZE.0 + 30.0, Maze::default().rings));
        assert!(state.refresh(|| 0.0) && state.problem.is_none(), "{:?}", state.problem);
        assert_eq!(far(&state), (USUAL_SIZE.0 + 30.0 + USUAL_SIZE.1 / 2.0) as i32);
        // dragged in until its walls no longer fit, they are built thinner (see `brickloft_core::maze::Layout::new`)
        state.spec.radius = 30.0;
        assert!(state.refresh(|| 0.0) && state.problem.is_none(), "{:?}", state.problem);
        // and further, until no wall leaves a corridor, it is not built and says why; the last maze stands
        let bricks = state.built.bricks.len();
        state.spec.radius = 18.0;
        state.refresh(|| 0.0);
        assert!(state.problem.as_deref().is_some_and(|why| why.contains("corridors")), "{:?}", state.problem);
        assert_eq!(state.built.bricks.len(), bricks);
        // no points are added to a maze, and nothing about it is searched for
        assert!(!state.can_pull() && state.tunable(None).is_empty());
    }

    #[test]
    fn a_tree_starts_at_its_usual_size_with_dots_for_its_height_and_spread() {
        use brickloft_core::tree::{Kind, Tree};
        let mut state = State::new();
        state.form = TREE;
        state.preset();
        let (height, spread) = Tree::usual_size(Kind::Pine);
        assert_eq!((state.spec.rise, state.spec.radius, state.floor_gap), (Some(height), spread, 0.0));
        assert_eq!(state.paint.mode, crate::paint::Mode::Pattern);
        assert_eq!(state.full_spec().form, Form::Tree);
        // small, for the rest: a dot at the foot, one at the top and one at the side
        (state.spec.rise, state.spec.radius) = (Some(30.0), 10.0);
        assert!(state.refresh(|| 0.0) && state.problem.is_none(), "{:?}", state.problem);
        let dots = state.handles();
        assert_eq!(dots.iter().map(|d| d.0).collect::<Vec<_>>(), [Handle::Point(0), Handle::Rise, Handle::Radius]);
        let top = dots[1].1;
        assert_eq!((top.x, top.y, top.z), (0.5, 0.5, 30.0), "the top dot is not at the top of the tree");
        // the top dot dragged up six cells makes it six taller, and stops at the least a tree can be
        let eye = DVec3::new(0.5, -200.0, 30.0);
        state.drag(Handle::Rise, eye, (top + DVec3::Z * 6.0 - eye).normalize(), DVec3::Y);
        assert_eq!(state.spec.rise, Some(36.0));
        state.drag(Handle::Rise, eye, (top - DVec3::Z * 29.0 - eye).normalize(), DVec3::Y);
        assert_eq!(state.spec.rise, Some(brickloft_core::tree::LEAST_HEIGHT));
        // the side dot sets its spread
        let side = state.handles()[2].1;
        let eye = DVec3::new(side.x, -200.0, side.z);
        state.drag(Handle::Radius, eye, (side + DVec3::X * 4.0 - eye).normalize(), DVec3::Y);
        assert_eq!(state.spec.radius, 14.0);
        assert!(state.refresh(|| 0.0) && state.problem.is_none(), "{:?}", state.problem);
        // no points are added to a tree, and nothing about it is searched for
        assert!(!state.can_pull() && state.tunable(None).is_empty());
        // another kind comes at its own size and in its own colours, and is the same tree when come back to
        state.spec.tree.seed = 9;
        state.usual_tree(Kind::Oak);
        assert_eq!((state.spec.tree, state.spec.rise), (Tree { seed: 9, ..Tree::of(Kind::Oak) }, Some(Tree::usual_size(Kind::Oak).0)));
        assert_eq!(state.paint.colours, crate::paint::palette("oak"));
        // every set of colours has a colour for each of a tree's eight parts, and a name
        for kind in brickloft_core::tree::KINDS.map(|k| k.1) {
            // (grass has six more, for its cliffs and the white water under a fall)
            assert!(crate::paint::palettes(kind).iter().all(|set| set.len() == if kind == "grass" { 14 } else { 8 }), "{kind}");
            assert_eq!(crate::paint::palettes(kind).len(), crate::paint::tree_palette_names(kind).len(), "{kind}");
        }
    }

    #[test]
    fn a_hedge_is_bent_by_its_dots_and_narrowed_without_changing_its_length() {
        use brickloft_core::tree::Kind;
        let mut state = State::new();
        state.form = TREE;
        state.preset();
        state.usual_tree(Kind::Hedge);
        // three dots along its line, then its height and its thickness; the thickness dot out to one side of the line
        let dots = state.handles();
        assert_eq!(dots.iter().map(|d| d.0).collect::<Vec<_>>(), [Handle::Point(0), Handle::Point(1), Handle::Point(2), Handle::Rise, Handle::Radius]);
        let (middle, side) = (dots[1].1, dots[4].1);
        assert!((side.y - middle.y).abs() == state.spec.radius && side.x == middle.x, "the thickness dot is at {side}, the middle of the line at {middle}");
        assert!(state.refresh(|| 0.0) && state.problem.is_none(), "{:?}", state.problem);
        let long = |state: &State| { let b = state.bounds.unwrap(); ((b.1[0] - b.0[0]) as f64, (b.1[1] - b.0[1]) as f64) };
        let before = long(&state);
        assert!((before.0 - 72.0).abs() < 6.0 && (before.1 - 24.0).abs() < 4.0, "{before:?}");
        // widened, it is wider and no longer: the fault was that it grew longer with every step wider
        let eye = DVec3::new(side.x, side.y, 300.0);
        state.drag(Handle::Radius, eye, (side + DVec3::Y * 28.0 * (side.y - middle.y).signum() - eye).normalize(), -DVec3::Z);
        assert_eq!(state.spec.radius, 40.0);
        assert!(state.refresh(|| 0.0) && state.problem.is_none());
        let wide = long(&state);
        assert!((wide.0 - before.0).abs() <= 2.0 * 30.0 && wide.0 < 140.0 && wide.1 > 70.0, "wide it is {wide:?}, from {before:?}");
        // and narrowed again it is as it was
        let side = state.handles()[4].1;
        let eye = DVec3::new(side.x, side.y, 300.0);
        state.drag(Handle::Radius, eye, (side - DVec3::Y * 28.0 * (side.y - middle.y).signum() - eye).normalize(), -DVec3::Z);
        assert_eq!(state.spec.radius, 12.0);
        assert!(state.refresh(|| 0.0));
        assert_eq!(long(&state), before);
        // its middle dot dragged aside bends it: it now reaches where a straight hedge does not
        let eye = DVec3::new(middle.x, middle.y, 300.0);
        state.drag(Handle::Point(1), eye, (middle + DVec3::Y * 30.0 - eye).normalize(), -DVec3::Z);
        assert_eq!(state.spec.points[1].y - state.spec.points[0].y, 30.0);
        assert_eq!(state.spec.points[1].z, state.spec.points[0].z, "the dot left the ground");
        assert!(state.refresh(|| 0.0) && state.problem.is_none());
        assert!(long(&state).1 > before.1 + 20.0, "{:?}", long(&state));
        // a point added to it bends it further, as it does a curve
        let on = state.built.path[state.built.path.len() / 4];
        let added = state.add_pull(on + state.standing()).expect("no point could be added to a hedge");
        assert_eq!(added, Handle::Pull(0));
        // a hedge saved before it had a line: one point and a stretch. It opens as long as it was, with its ends as dots.
        let old = Spec { form: Form::Tree, points: vec![cell(0.0, 0.0, 0.0)], radius: 10.0, rise: Some(30.0), stretch: DVec3::new(6.0, 1.0, 1.0), tree: brickloft_core::tree::Tree::of(Kind::Hedge), ..Spec::default() };
        state.load_spec(old);
        assert_eq!((state.spec.points.len(), state.spec.points[2].x - state.spec.points[0].x, state.spec.stretch), (3, 120.0, DVec3::ONE));
        // grass: a point added where it is clicked, and lifted, raises the ground there
        state.usual_tree(Kind::Grass);
        assert!(state.refresh(|| 0.0) && state.problem.is_none(), "{:?}", state.problem);
        let flat = state.bounds.unwrap().1[2];
        // a touch of the brush where it is clicked: as wide and as strong as the brush is, and the ground is a hill there
        // (a new lawn has cliffs at their middle setting, in the number a pine keeps its baubles in)
        assert_eq!(state.spec.tree.baubles, 2);
        state.brush = Brush { range: 20.0, amount: 6.0, action: Action::Up, sharp: false };
        let handle = state.add_pull(DVec3::new(10.5, 3.5, 2.0) + state.standing()).expect("grass could not be touched");
        assert_eq!((state.spec.pulls[0].1, state.touched), (DVec3::new(20.0, 0.0, 6.0), None));
        // A sharp edge is the brush's, for touches still to be made: ticked, the touch already made is as it was, and the next has it (32 in its middle number, and 48 if it flattens).
        {
            let mut sharp = State::new();
            sharp.form = TREE;
            sharp.preset();
            sharp.usual_tree(Kind::Grass);
            sharp.refresh(|| 0.0);
            sharp.brush = Brush { range: 12.0, amount: 9.0, action: Action::Up, sharp: false };
            sharp.add_pull(DVec3::new(-10.5, 0.5, 2.0) + sharp.standing()).unwrap();
            sharp.brush.sharp = true;
            sharp.add_pull(DVec3::new(10.5, 0.5, 2.0) + sharp.standing()).unwrap();
            sharp.brush.action = Action::Flatten;
            sharp.add_pull(DVec3::new(10.5, 0.5, 2.0) + sharp.standing()).unwrap();
            assert_eq!(sharp.spec.pulls.iter().map(|touch| (touch.1.x, touch.1.y)).collect::<Vec<_>>(), [(12.0, 0.0), (12.0, SHARP), (12.0, SHARP + FLATTENS)]);
            assert_eq!(sharp.spec.pulls[2].1.z, 9.0, "flattened to the height of the table it was touched on");
            // the sharp one is a table: as high three quarters of the way out as at its middle, where the eased one has fallen most of the way
            sharp.spec.pulls.truncate(2);
            let high = |x: f64| brickloft_core::spec::grass_ground(&sharp.spec, DVec3::new(x, 0.5, 0.5) + sharp.standing() * 0.0);
            assert!(high(10.5 + 9.0) == 9.0 && high(-10.5 + 9.0) < 2.0, "{} and {}", high(19.5), high(-1.5));
        }
        // its dot is on the hilltop, and dragged up it pushes harder: where it is and how wide stay as they were
        let at = state.handles().iter().find(|h| h.0 == handle).unwrap().1;
        assert_eq!(at.z - state.spec.points[0].z, 6.0);
        let eye = DVec3::new(at.x, -300.0, at.z);
        state.drag(handle, eye, (at + DVec3::Z * 8.0 - eye).normalize(), DVec3::Y);
        assert_eq!(state.spec.pulls[0], (DVec3::new(11.0, 4.0, 0.5), DVec3::new(20.0, 0.0, 14.0)));
        assert!(state.refresh(|| 0.0) && state.problem.is_none());
        assert!(state.bounds.unwrap().1[2] >= flat + 10, "{} high raised, {flat} flat", state.bounds.unwrap().1[2]);
        // pushed down beside it, a hollow in the hill; and flattened on its flank, a terrace at the height the flank had there
        state.brush = Brush { range: 10.0, amount: 4.0, action: Action::Down, sharp: false };
        state.add_pull(DVec3::new(16.5, 3.5, 2.0) + state.standing()).unwrap();
        assert_eq!(state.spec.pulls[1].1, DVec3::new(10.0, 0.0, -4.0));
        state.brush.action = Action::Flatten;
        let flank = brickloft_core::spec::grass_ground(&state.spec, DVec3::new(2.0, 4.0, 0.5));
        state.add_pull(DVec3::new(1.5, 3.5, 2.0) + state.standing()).unwrap();
        assert!(flank > 2.0 && state.spec.pulls[2].1.y != 0.0 && (state.spec.pulls[2].1.z - flank).abs() <= 0.25, "{:?}, flank {flank}", state.spec.pulls[2]);
        assert!(state.refresh(|| 0.0) && state.problem.is_none());
        // a stroke: with Ctrl held, the cursor moved 30 cells along the ground leaves a row of the same touch, 0.4 of its range apart
        state.brush = Brush { range: 10.0, amount: 3.0, action: Action::Up, sharp: false };
        let begun = state.add_pull(DVec3::new(-20.5, -5.5, 2.0) + state.standing()).unwrap();
        let had = state.spec.pulls.len();
        state.lift = true;
        for step in 1..=60 {
            let over = DVec3::new(-20.5 + step as f64, -5.5, 0.5) + state.standing();
            state.drag(begun, over + DVec3::Z * 200.0, -DVec3::Z, -DVec3::Z);
        }
        state.lift = false;
        let stroke = &state.spec.pulls[had - 1..];
        assert!(stroke.len() >= 13 && stroke.len() <= 17, "{} touches in a stroke of 60", stroke.len());
        assert!(stroke.iter().all(|t| t.1 == DVec3::new(10.0, 0.0, 3.0)) && stroke.windows(2).all(|w| (w[1].0.x - w[0].0.x - 4.0).abs() <= 1.0 && w[1].0.y == w[0].0.y), "{stroke:?}");
        assert!(state.refresh(|| 0.0) && state.problem.is_none());
        // and a patch takes far more touches than a form takes added points, up to its own most
        assert!(state.spec.pulls.len() > PULLS && most_pulls(&state.full_spec()) == TOUCHES);
        // the line's dots moved, the touches stay as they are
        let before = state.spec.pulls.clone();
        let end = state.handles()[2].1;
        let eye = DVec3::new(end.x, end.y, 300.0);
        state.drag(Handle::Point(2), eye, (end + DVec3::X * 6.0 - eye).normalize(), -DVec3::Z);
        assert_eq!(state.spec.pulls, before);
        // a kind that does not lie along a line has one dot again, and no added points
        state.usual_tree(Kind::Oak);
        assert_eq!((state.spec.points.len(), state.spec.pulls.len()), (1, 0));
    }

    #[test]
    fn opening_a_save_replaces_the_workspace_and_can_be_undone() {
        let mut state = State::new();
        state.add_grid();
        state.spec.grid = [0.0, 0.0, 0.6, 0.8];
        state.paint.colours = vec![[9, 8, 7]];
        let saved = state.forms();
        let mut other = State::new();
        other.add_copy();
        other.add_copy();
        let before = other.forms();
        other.open(saved.clone());
        assert_eq!(other.forms(), saved);
        assert_eq!((other.spec.grid, other.paint.colours.clone()), ([0.0, 0.0, 0.6, 0.8], vec![[9, 8, 7]]));
        assert!(other.undo());
        assert_eq!(other.forms(), before);
    }

    #[test]
    fn a_dot_dragged_in_a_plane_seen_edge_on_moves_no_further_than_the_cursor() {
        let mut state = State::new(); // an arch across x, standing along z
        state.lock = Lock::Axis(0); // its dots move in y and z
        let at = state.dots().iter().find(|(h, _)| *h == Handle::Point(0)).unwrap().1;
        let before = state.spec.points[0];
        // looked at along y, from 50 away and a cell to one side: the plane
        // x = the dot's own is seen edge on. The cursor a hair to the side of
        // the dot met that plane 117 cells beyond it.
        let origin = at + DVec3::new(1.0, -50.0, 0.0);
        state.drag_alone(Handle::Point(0), origin, DVec3::new(-0.006, 1.0, 0.1).normalize(), DVec3::Y);
        let moved = state.spec.points[0] - before;
        assert_eq!(moved.x, 0.0, "the dot left its plane");
        assert!(moved.y.abs() <= 1.0 && (moved.z - 5.0).abs() <= 1.0, "the dot moved by {moved:?}");
    }

    #[test]
    fn an_arch_at_an_angle_is_lined_up_on_a_grid_of_its_own_where_it_stands() {
        let mut state = State::new(); // an arch across x
        state.add_copy(); // and another on the same grid
        let (from, to) = (cell(0.0, 0.0, 0.0), cell(6.0, 8.0, 0.0));
        state.spec.points = vec![from, to];
        state.refresh(|| 0.0);
        assert!(state.askew());
        let world = |state: &State, p: DVec3| out_of_grid(&state.spec, p + state.standing());
        let (was_from, was_to, cell_by_cell) = (world(&state, from), world(&state, to), state.built.bricks.len());
        assert!(state.line_up());
        state.refresh(|| 0.0);
        // along x within its grid, ten apart as before, and no longer askew
        let (a, b) = (state.spec.points[0], state.spec.points[1]);
        assert!(!state.askew() && a == from && b == from + DVec3::X * 10.0, "{a:?} {b:?}");
        // on a grid of its own, at the angle it stood at, with its feet where they were
        assert!(state.others.iter().all(|o| o.spec.on_grid != state.spec.on_grid));
        assert!((world(&state, a) - was_from).length() < 1e-9 && (world(&state, b) - was_to).length() < 1e-9, "{:?} {:?}", world(&state, a), world(&state, b));
        // and fitted as an outline: far fewer bricks than cell by cell
        assert!(state.problem.is_none() && state.built.bricks.len() * 2 < cell_by_cell, "{} bricks, {cell_by_cell} before", state.built.bricks.len());
        assert!(!state.line_up(), "an arch already lined up was turned again");
    }

    #[test]
    fn a_mosaic_is_coloured_as_its_pattern_has_it() {
        let mut state = State::new();
        state.spec = Spec { form: Form::Circle, points: vec![cell(0.0, 0.0, 0.0)], radius: 24.0, fill: true, depth: 1, mosaic: Some(brickloft_core::pattern::Mosaic { style: 1, size: 2, bevel: false, border: 1, seed: 3, mortar: true, stagger: -1, upright: false, uneven: [0; 2] }), ..Spec::default() };
        state.form = CIRCLE;
        state.paint = Paint { mode: crate::paint::Mode::Pattern, colours: crate::paint::MOSAIC.to_vec(), axis: 2, width: 2 };
        state.refresh(|| 0.0);
        assert!(state.problem.is_none(), "{:?}", state.problem);
        assert_eq!(state.built.shades.len(), state.built.bricks.len());
        let painted = state.painted();
        // each brick in the colour of its number, and more than one colour in use
        assert!(painted.iter().zip(&state.built.shades).all(|(brick, shade)| brick.1 == Some(crate::paint::MOSAIC[brickloft_core::pattern::colour_of(*shade, 4)])));
        let colours: std::collections::HashSet<[u8; 3]> = painted.iter().filter_map(|p| p.1).collect();
        assert!(colours.len() >= 3, "{} colours", colours.len());
        // a quarter turn and a flip leave each brick its colour
        let before: Vec<u8> = state.built.shades.clone();
        state.spec.flip = [true, false, false];
        state.refresh(|| 0.0);
        assert_eq!(state.built.shades, before);
    }

    #[test]
    fn a_form_moved_with_ctrl_goes_into_another_and_gives_up_the_room_they_share() {
        let ball = |radius: f64| Spec { form: Form::Sphere, points: vec![cell(0.0, 0.0, 0.0)], radius, ..Spec::default() };
        let mut state = State::new();
        state.load_spec(ball(8.0));
        state.refresh(|| 0.0);
        state.add_form(ball(6.0));
        state.refresh(|| 0.0);
        // (the form being edited is the smaller ball, set down clear of the larger)
        let apart = [0, 1, 2].map(|k| state.others[0].place[k] - state.place[k]);
        let overlap = |state: &State| Standing::new(&state.others[0].built.bricks, state.others[0].place).meets(cells_of(&state.built.bricks), state.place);
        let boxes_meet = |all: &[Painted]| all.iter().enumerate().any(|(n, a)| all[n + 1..].iter().any(|b| a.2 != b.2 && (0..3).all(|k| 2 * a.0.min[k] + a.3[k] < 2 * (b.0.min[k] + b.0.ext[k]) + b.3[k] && 2 * b.0.min[k] + b.3[k] < 2 * (a.0.min[k] + a.0.ext[k]) + a.3[k])));
        // without Ctrl it stops against the other
        state.move_by(apart, DVec3::ZERO);
        assert!(!overlap(&state) && !state.joined());
        let whole = state.painted().iter().filter(|b| b.2 == 0).count();
        // with Ctrl it goes all the way in, most of the way to the other's middle
        state.merge = true;
        let most = [0, 1, 2].map(|k| (state.others[0].place[k] - state.place[k]) * 2 / 3);
        state.move_by(most, DVec3::ZERO);
        assert!(overlap(&state) && state.joined(), "it did not go in");
        // no brick's box overlaps a brick of the other form, and the joined one has lost bricks but not all
        let all = state.painted();
        assert!(!boxes_meet(&all), "two forms' bricks share room");
        // The other form is its own bricks away from the seam, and fitted
        // afresh at it: it was whole before the join was a refit, which
        // left a notch all round the seam where the joined form's sloped
        // bricks were left out.
        let own = &state.others[0].built.bricks;
        let kept = all.iter().filter(|b| b.2 == 1 && own.contains(&b.0)).count();
        assert!(kept > own.len() / 4 && kept < own.len(), "{kept} of the other form's {} bricks are its own", own.len());
        let left: i32 = all.iter().filter(|b| b.2 == 0).map(|b| b.0.ext.iter().product::<i32>()).sum();
        let had: i32 = state.built.bricks.iter().map(|b| b.ext.iter().product::<i32>()).sum();
        assert!(left > had / 4 && left < had, "{left} of {had} cells left, from {whole} bricks");
        // reshaping the other form does not push the joined one out of it
        state.activate(0);
        state.spec.radius = 9.0;
        state.refresh(|| 0.0);
        assert!(state.others[0].spec.blend.is_some() && Standing::new(&state.others[0].built.bricks, state.others[0].place).meets(cells_of(&state.built.bricks), state.place), "the joined form was pushed out");
        // moved without Ctrl, it is set clear again
        state.activate(0);
        state.merge = false;
        state.move_by([2, 0, 0], DVec3::ZERO);
        assert!(!overlap(&state) && !state.joined());
        assert!(!boxes_meet(&state.painted()));
    }

    /// A cube with a sphere moved into its corner with Ctrl held, the
    /// sphere being edited, and the bricks of the two wherever they stand.
    fn a_sphere_in_a_cube() -> State {
        let mut state = a_cube();
        state.add_form(Spec { form: Form::Sphere, points: vec![cell(0.0, 0.0, 0.0)], radius: 10.0, fill: true, ..Spec::default() });
        state.refresh(|| 0.0);
        state.record();
        state.merge = true;
        let to = [0, 1, 2].map(|k| state.others[0].place[k] + [20, 8, 30][k] - state.place[k]);
        state.move_by(to, DVec3::ZERO);
        state.merge = false;
        state.record();
        state
    }

    fn bricks_where_they_stand(state: &State) -> Vec<(CellBrick, [i32; 3], Option<[u8; 3]>)> {
        let mut all: Vec<_> = state.painted().into_iter().map(|b| (b.0, b.3, b.1)).collect();
        all.sort_by_key(|(b, place, _)| (*place, b.min, b.ext, b.mask));
        all
    }

    #[test]
    fn a_join_and_its_blend_come_back_with_undo_and_with_a_save() {
        let mut state = a_sphere_in_a_cube();
        assert_eq!(state.spec.blend, Some(0.0));
        let sharp = bricks_where_they_stand(&state);
        // a blend rounds the join: more room is filled, and the bricks differ
        state.spec.blend = Some(4.0);
        state.record();
        let round = bricks_where_they_stand(&state);
        let room = |all: &[(CellBrick, [i32; 3], Option<[u8; 3]>)]| all.iter().map(|b| b.0.volume()).sum::<f64>();
        assert!(room(&round) > room(&sharp) + 20.0, "a blend of 4 added {} cells to the join", room(&round) - room(&sharp));
        // undo takes the blend back and leaves the join; again, and the sphere is out of the cube
        assert!(state.undo());
        state.refresh(|| 0.0);
        assert_eq!(state.spec.blend, Some(0.0));
        assert!(bricks_where_they_stand(&state) == sharp, "undo did not bring the sharp join back");
        assert!(state.undo());
        state.refresh(|| 0.0);
        assert!(!state.joined());
        assert!(state.redo() && state.redo());
        state.refresh(|| 0.0);
        assert_eq!(state.spec.blend, Some(4.0));
        assert!(bricks_where_they_stand(&state) == round, "redo did not bring the blend back");
        // a save holds the join and its blend, and opening it builds the same bricks
        let saved = crate::store::describe(&state.forms());
        let mut opened = State::new();
        opened.open(crate::store::forms_in(&saved).unwrap());
        opened.refresh(|| 0.0);
        assert_eq!(opened.spec.blend, Some(4.0));
        assert!(bricks_where_they_stand(&opened) == round, "a save opened again is not the same bricks");
    }

    #[test]
    fn a_form_cut_out_of_another_builds_none_of_its_own_bricks_and_comes_back_with_undo_a_save_and_a_copy() {
        let mut state = a_sphere_in_a_cube();
        let joined = bricks_where_they_stand(&state);
        let room = |all: &[(CellBrick, [i32; 3], Option<[u8; 3]>)]| all.iter().map(|b| b.0.volume()).sum::<f64>();
        let cube: f64 = state.others[0].built.bricks.iter().map(|b| b.volume()).sum();
        assert!(state.ghosts().is_empty());
        state.spec.carve = true;
        assert!(state.refresh(|| 0.0) || state.scene > 0);
        state.record();
        // the sphere is drawn as glass and none of it is built; the cube has a hollow where it stood
        let cut = bricks_where_they_stand(&state);
        assert!(state.painted().iter().all(|b| b.2 == 1), "a brick of the form cut out was built");
        assert_eq!(state.ghosts().len(), state.built.bricks.len());
        assert!(room(&cut) < cube - 200.0 && room(&cut) > cube / 2.0 && room(&joined) > cube + 200.0, "{} cells of a cube of {cube}, and {} joined", room(&cut), room(&joined));
        // undo and redo
        assert!(state.undo());
        state.refresh(|| 0.0);
        assert!(!state.spec.carve && bricks_where_they_stand(&state) == joined, "undo did not fill the hole");
        assert!(state.redo());
        state.refresh(|| 0.0);
        assert!(state.spec.carve && bricks_where_they_stand(&state) == cut, "redo did not cut it again");
        // a save, and a copy of both forms
        let mut opened = State::new();
        opened.open(crate::store::forms_in(&crate::store::describe(&state.forms())).unwrap());
        opened.refresh(|| 0.0);
        assert!(opened.spec.carve && bricks_where_they_stand(&opened) == cut, "a save opened again is not the same bricks");
        state.pick_all();
        let (bricks, forms) = state.copied();
        assert!(bricks.len() == cut.len() && forms.len() == 2 && forms[0].0.carve && forms[0].0.blend == Some(0.0));
        // moved without Ctrl it is set clear, and is a sphere of bricks again
        state.move_by([2, 0, 0], DVec3::ZERO);
        assert!(!state.joined() && state.ghosts().is_empty() && state.painted().iter().any(|b| b.2 == 0));
    }

    #[test]
    fn a_form_joined_half_a_microbrick_off_is_moved_onto_the_other_forms_cells() {
        let fresh = |state: &State| {
            let own: Vec<&[CellBrick]> = std::iter::once(&state.built).chain(state.others.iter().map(|o| &o.built)).map(|b| b.bricks.as_slice()).collect();
            state.painted().iter().filter(|b| b.0.mask != brickloft_core::shapes::CUBE && !own[b.2].contains(&b.0)).count()
        };
        let mut state = a_cube();
        state.add_form(Spec { form: Form::Sphere, points: vec![cell(0.0, 0.0, 0.0)], radius: 10.0, fill: true, ..Spec::default() });
        state.refresh(|| 0.0);
        let cube = state.others[0].place;
        let off = |state: &State| [0, 1, 2].map(|k| (state.place[k] - cube[k]).rem_euclid(2));
        // dragged with Ctrl to a place an odd unit off along x and z, it stops a unit short of the pointer on each
        state.merge = true;
        let to = [0, 1, 2].map(|k| cube[k] + [21, 8, 31][k] - state.place[k]);
        state.move_by(to, DVec3::ZERO);
        assert_eq!((off(&state), [0, 1, 2].map(|k| state.place[k] - cube[k])), ([0; 3], [21 - to[0].signum(), 8, 31 - to[2].signum()]));
        assert!(fresh(&state) > 20, "the join was not fitted afresh");
        // and goes on a whole microbrick at a time: one more unit moves nothing, two move two
        let at = state.place;
        state.move_by([1, 0, -1], DVec3::ZERO);
        assert_eq!(state.place, at);
        state.move_by([2, 0, -3], DVec3::ZERO);
        assert_eq!(state.place, [at[0] + 2, at[1], at[2] - 2]);
        state.merge = false;
        // set half off by other means (a save from before, or Claude), it is settled when next looked at, and that is said
        state.place[1] += 1;
        state.toast = None;
        state.refresh(|| 0.0);
        assert!(off(&state) == [0; 3] && state.toast.is_some() && fresh(&state) > 20);
        assert!(state.unblended().is_none());
    }

    #[test]
    fn a_workspace_opened_with_a_form_joined_half_a_microbrick_off_is_settled_the_first_time_it_is_built() {
        // the joined form is not the one being edited, and the one that is has not been built when the workspace is opened
        let cube = Spec { form: Form::Cube, points: vec![cell(0.0, 0.0, 8.0)], radius: 8.5, fill: true, ..Spec::default() };
        let ball = Spec { form: Form::Sphere, points: vec![cell(0.0, 0.0, 0.0)], radius: 10.0, fill: true, blend: Some(3.0), ..Spec::default() };
        let mut state = State::new();
        state.open(vec![(cube, Paint::default(), [0; 3]), (ball, Paint::default(), [9, 4, 31])]);
        state.refresh(|| 0.0);
        assert_eq!(state.others[0].place, [10, 4, 32], "the joined form was left half a microbrick off");
        let own = &state.others[0].built.bricks;
        assert!(state.painted().iter().filter(|b| b.2 == 1 && b.0.mask != brickloft_core::shapes::CUBE && !own.contains(&b.0)).count() > 20, "the join was not fitted afresh");
    }

    #[test]
    fn a_join_is_the_same_bricks_whichever_of_its_forms_is_being_edited() {
        let mut state = a_sphere_in_a_cube();
        state.spec.blend = Some(2.0);
        let before = bricks_where_they_stand(&state);
        state.activate(0);
        assert!(state.spec.form == Form::Cube && bricks_where_they_stand(&state) == before, "editing the cube changed the join");
        // and both of them joined, as when the cube is then moved with Ctrl too
        state.spec.blend = Some(0.0);
        let both = bricks_where_they_stand(&state);
        state.activate(0);
        assert!(bricks_where_they_stand(&state) == both, "with both forms joined, choosing the other to edit changed the join");
    }

    #[test]
    fn a_join_is_put_together_once_and_not_for_every_change_of_colour() {
        let mut state = a_sphere_in_a_cube();
        state.painted();
        let first = state.joins.borrow().as_ref().map(|j| j.1.clone()).unwrap();
        state.paint.colours = vec![[200, 30, 30]];
        state.others[0].picked = true;
        let recoloured = state.painted();
        assert!(std::rc::Rc::ptr_eq(&first, &state.joins.borrow().as_ref().unwrap().1), "a change of colour put the join together again");
        assert!(recoloured.iter().any(|b| b.1 == Some([200, 30, 30])));
        // moving a form, or changing its blend, does
        state.spec.blend = Some(1.0);
        state.painted();
        assert!(!std::rc::Rc::ptr_eq(&first, &state.joins.borrow().as_ref().unwrap().1));
    }

    #[test]
    fn stepping_stones_can_be_chosen_on_every_curve_as_it_starts() {
        // as the panel does it: the form as the editor starts it, then the pattern, then the colours
        for form in 0..4 {
            for (fill, busy) in [(false, false), (true, false), (false, true)] {
                let mut state = State::new();
                state.form = form;
                state.refresh(|| 0.0);
                state.spec.fill = fill;
                state.spec.stones = Some(brickloft_core::pattern::Stones { size: 10.0, gap: 1.5, uneven: 0.5, seed: 1 });
                state.paint = Paint { mode: crate::paint::Mode::Pattern, colours: crate::paint::palette("stepping stones").to_vec(), axis: 2, width: 2 };
                state.busy = busy;
                state.refresh(|| 0.0);
                let painted = state.painted();
                assert!(state.problem.is_some() || !painted.is_empty(), "{}: nothing laid", FORMS[form]);
            }
        }
    }

    #[test]
    fn a_cube_joined_to_a_cube_is_cut_into_a_few_blocks_and_not_into_cells() {
        let mut state = State::new();
        state.form = CUBE;
        state.preset();
        state.refresh(|| 0.0);
        state.add_cube();
        state.refresh(|| 0.0);
        // moved a third of the way into the other, off its lines by an odd unit each way
        let towards = [0, 1, 2].map(|k| state.others[0].place[k] - state.place[k]);
        state.merge = true;
        state.move_by([towards[0] * 2 / 3 + 1, towards[1] * 2 / 3 + 1, 11], DVec3::ZERO);
        assert!(state.joined());
        let all = state.painted();
        let mine: Vec<&Painted> = all.iter().filter(|b| b.2 == 0).collect();
        // what is left of a cube with a corner taken out is a handful of blocks, and most of the cube
        let cells: i32 = mine.iter().map(|b| b.0.ext.iter().product::<i32>()).sum();
        assert!(cells > 17 * 17 * 17 / 2 && cells < 17 * 17 * 17, "{cells} cells left");
        assert!(mine.len() <= 12, "{} bricks for a cube with a corner out", mine.len());
        assert!(mine.iter().any(|b| b.0.ext.iter().product::<i32>() > 500), "no large block is left");
        // no two bricks of the two cubes share room, and the other cube is whole
        let unit_box = |b: &Painted| ([0, 1, 2].map(|k| 2 * b.0.min[k] + b.3[k]), [0, 1, 2].map(|k| 2 * (b.0.min[k] + b.0.ext[k]) + b.3[k]));
        for (n, a) in all.iter().enumerate() {
            let a_box = unit_box(a);
            assert!(all[n + 1..].iter().all(|b| { let b_box = unit_box(b); (0..3).any(|k| a_box.1[k] <= b_box.0[k] || b_box.1[k] <= a_box.0[k]) }), "two bricks share room");
        }
        assert_eq!(all.iter().filter(|b| b.2 == 1).count(), 1);
    }

    #[test]
    fn a_form_far_from_another_is_not_compared_with_it_cell_by_cell() {
        // a cube beside a large sphere: the sphere's cells are never listed while the cube is resized clear of it
        let mut state = State::new();
        state.load_spec(Spec { form: Form::Sphere, points: vec![cell(0.0, 0.0, 30.0)], radius: 30.0, fill: true, ..Spec::default() });
        state.refresh(|| 0.0);
        let sphere = Standing::new(&state.built.bricks, state.place);
        let cube = [CellBrick { mask: 0xff, min: [0, 0, 0], ext: [17, 17, 17] }];
        assert!(!sphere.meets(cells_of(&cube), [200, 0, 0]) && sphere.cells.get().is_none(), "the sphere's cells were listed for a cube nowhere near it");
        // touching its box is enough to look, and inside it they meet
        assert!(sphere.meets(cells_of(&cube), [0, 0, 40]) && sphere.cells.get().is_some());
    }

    #[test]
    fn a_path_of_stones_is_coloured_a_stone_at_a_time() {
        let mut state = State::new();
        state.spec = Spec { form: Form::Circle, points: vec![cell(0.0, 0.0, 0.0)], radius: 30.0, thick: 16.0, depth: 2, stones: Some(brickloft_core::pattern::Stones { size: 7.0, gap: 1.5, uneven: 0.5, seed: 2 }), ..Spec::default() };
        state.form = CIRCLE;
        state.paint = Paint { mode: crate::paint::Mode::Pattern, colours: crate::paint::STONE.to_vec(), axis: 2, width: 2 };
        state.refresh(|| 0.0);
        assert!(state.problem.is_none(), "{:?}", state.problem);
        let painted = state.painted();
        let pieces = brickloft_core::pattern::pieces(&state.built.bricks);
        // many stones, and every one of the colours in use
        let stones: std::collections::HashSet<u32> = pieces.iter().copied().collect();
        let colours: std::collections::HashSet<[u8; 3]> = painted.iter().filter_map(|p| p.1).collect();
        assert!(stones.len() > 20 && colours.len() == 4, "{} stones in {} colours", stones.len(), colours.len());
        // the bricks of one stone are one colour
        let mut of_piece: std::collections::HashMap<u32, Option<[u8; 3]>> = std::collections::HashMap::new();
        for (brick, piece) in painted.iter().zip(&pieces) {
            assert_eq!(*of_piece.entry(*piece).or_insert(brick.1), brick.1, "a stone is in two colours");
        }
    }

    #[test]
    fn a_pasted_form_joins_the_grid_being_worked_on() {
        let mut state = State::new();
        state.add_grid();
        let here = (state.spec.on_grid, state.spec.grid, state.spec.grid_at);
        // copied from some other grid, standing at some other angle
        let mut copied = state.full_spec();
        (copied.on_grid, copied.grid, copied.grid_at) = (7, [0.0, 0.0, 0.6, 0.8], [40.0, 0.0, 0.0]);
        let mut apart = copied.clone();
        apart.on_grid = 9;
        state.paste(vec![(copied.clone(), Paint::default(), [0; 3]), (copied, Paint::default(), [8, 0, 0]), (apart, Paint::default(), [16, 0, 0])]);
        // the last pasted is the one being edited: a grid of its own
        assert!(state.spec.on_grid != here.0, "a second grid in what was pasted joined the first");
        let joined = state.others.iter().filter(|o| (o.spec.on_grid, o.spec.grid, o.spec.grid_at) == here).count();
        // the form that was there, and the two pasted from one grid
        assert_eq!(joined, 3);
    }

    #[test]
    fn the_rings_and_arrows_turn_and_move_a_whole_grid() {
        let mut state = State::new(); // an arch across x, standing along z
        state.refresh(|| 0.0);
        // a second form on the same grid, and a third on a grid of its own
        state.add_form(state.full_spec());
        state.refresh(|| 0.0);
        state.add_grid();
        state.refresh(|| 0.0);
        assert_eq!(state.others.iter().map(|o| o.spec.on_grid).collect::<Vec<_>>(), vec![0, 0]);
        assert_eq!(state.spec.on_grid, 1);
        // Only what is picked is copied: the form being edited, and any picked with it.
        assert_eq!(state.copied().1.len(), 1);
        assert!(state.copied().0.iter().all(|b| b.2 == 0) && state.copied().0.len() == state.built.bricks.len());
        state.toggle_pick(1);
        let (bricks, forms) = state.copied();
        assert_eq!((forms.len(), forms[1].0.on_grid), (2, state.others[1].spec.on_grid));
        assert_eq!(bricks.iter().filter(|b| b.2 == 1).count(), state.others[1].built.bricks.len());
        assert!(bricks.iter().all(|b| b.2 < 2));
        state.toggle_pick(1);
        assert_eq!(state.copied().1.len(), 1);
        state.pick_all();
        assert_eq!(state.copied().1.len(), 3);
        // choosing another form to edit lets the rest go
        state.activate(0);
        assert_eq!(state.picked(), vec![true, false, false]);
        state.activate(0);
        state.refresh(|| 0.0);
        // (a copy is coloured as what it was copied from)
        state.paint.colours = vec![[10, 200, 30]];
        state.add_copy();
        assert_eq!(state.paint.colours, vec![[10, 200, 30]]);
        assert_eq!(state.others.last().unwrap().paint.colours, vec![[10, 200, 30]]);
        state.remove_active();
        state.paint = Paint::default();
        state.refresh(|| 0.0);
        // back to the grid of two: its arrows and rings stand at the middle of both forms
        state.activate(0);
        state.refresh(|| 0.0);
        assert_eq!(state.spec.on_grid, 0);
        // (both picked: the arrows stand at the middle of what they move, and move the whole grid when that is all of it)
        state.others.iter_mut().filter(|o| o.spec.on_grid == 0).for_each(|o| o.picked = true);
        let (middle, reach) = state.gizmo().unwrap();
        let mate = state.others.iter().find(|o| o.spec.on_grid == 0).unwrap();
        let own = (state.bounds.unwrap(), state.place);
        let both_lo = (2 * own.0 .0[0] + own.1[0]).min(2 * mate.bounds.unwrap().0[0] + mate.place[0]) as f64 / 2.0;
        let both_hi = (2 * own.0 .1[0] + own.1[0]).max(2 * mate.bounds.unwrap().1[0] + mate.place[0]) as f64 / 2.0;
        assert!((middle.x - (both_lo + both_hi) / 2.0).abs() < 1e-9, "not at the middle of the two: {middle:?}");
        // a quarter of the way out to their furthest side, and the rings as large as the arrows
        assert!((reach[0] - GIZMO * (both_hi - both_lo) / 2.0).abs() < 1e-9 && state.rings().unwrap().1 == reach[0]);
        let bricks = state.built.bricks.clone();
        let foot = state.handles()[0].1;

        // the ring about y, seen along y: taken hold of and carried 37 degrees round
        let eye = middle - DVec3::Y * 200.0;
        let towards = |angle: f64| (middle + DVec3::new(angle.sin(), 0.0, angle.cos()) * 4.0 - eye).normalize();
        assert!(state.hold_ring(1, eye, towards(0.0)));
        state.drag(Handle::Spin(1), eye, towards(37f64.to_radians()), DVec3::Y);
        assert!(on_own_grid(&state.spec));
        let up = turned_way(state.spec.grid, DVec3::Z, false);
        assert!((up - DVec3::new(37f64.to_radians().sin(), 0.0, 37f64.to_radians().cos())).length() < 1e-9, "{up:?}");
        // both forms on the grid turned, and the one on the other grid did not
        let grids: Vec<(u32, bool)> = state.others.iter().map(|o| (o.spec.on_grid, on_own_grid(&o.spec))).collect();
        assert!(grids.contains(&(0, true)) && grids.contains(&(1, false)), "{grids:?}");
        // about the rings' middle, which has stayed where it was
        assert!((state.gizmo().unwrap().0 - middle).length() < 1e-9);
        // the same bricks, not fitted again, and the dots gone round with them
        assert!(!state.refresh(|| 0.0));
        assert_eq!(state.built.bricks, bricks);
        let now = state.handles()[0].1;
        assert!((now - (middle + turned_way(state.spec.grid, foot - middle, false))).length() < 1e-9);

        // a dot is dragged where it is seen: the crown, two cells further along the arch's own up
        let rise = state.arch_rise();
        let crown = state.handles().iter().find(|h| h.0 == Handle::Rise).unwrap().1;
        let side = turned_way(state.spec.grid, DVec3::Y, false);
        state.drag(Handle::Rise, crown + up * 2.0 - side * 80.0, side, side);
        assert!((state.arch_rise() - (rise + 2.0)).abs() <= 0.5, "rise {} from {rise}", state.arch_rise());
        state.spec.rise = Some(rise);
        state.refresh(|| 0.0);

        // an arrow moves the whole grid, freely, and nothing on it against anything else
        let (middle, _) = state.gizmo().unwrap();
        let places: Vec<[i32; 3]> = std::iter::once(state.place).chain(state.others.iter().map(|o| o.place)).collect();
        let far = middle + DVec3::new(-60.0, -200.0, 40.0);
        assert!(state.hold_arrow(2, far, (middle - far).normalize()));
        state.drag(Handle::Slide(2), far, (middle + DVec3::Z * 3.3 - far).normalize(), DVec3::Y);
        assert!((state.gizmo().unwrap().0 - (middle + DVec3::Z * 3.3)).length() < 1e-6, "{:?}", state.gizmo().unwrap().0 - middle);
        assert_eq!(std::iter::once(state.place).chain(state.others.iter().map(|o| o.place)).collect::<Vec<_>>(), places);
        // and a form is moved within its grid by its bricks: along the grid's own axes
        let at = state.handles()[0].1;
        state.take_hold(at);
        let along = turned_way(state.spec.grid, DVec3::X, false);
        state.lock = Lock::Axis(1);
        state.drag(Handle::Move, at + along * 2.0 - side * 80.0, side, side);
        assert_eq!([state.place[0] - places[0][0], state.place[1] - places[0][1], state.place[2] - places[0][2]], [4, 0, 0]);

        // levelled, the grid is the world's again, about where its middle was
        let middle = state.gizmo().unwrap().0;
        state.level();
        assert!(!on_own_grid(&state.spec) && state.others.iter().all(|o| !on_own_grid(&o.spec)));
        assert!((state.gizmo().unwrap().0 - middle).length() <= 0.9, "{:?}", state.gizmo().unwrap().0 - middle);
        // seen edge on, a ring cannot be taken hold of
        assert!(!state.hold_ring(1, middle - DVec3::X * 60.0, DVec3::X));
    }

    #[test]
    fn an_arrow_moves_the_forms_that_are_picked_and_leaves_the_rest_of_the_grid() {
        let mut state = State::new();
        state.form = CUBE;
        state.preset();
        state.refresh(|| 0.0);
        state.add_cube();
        state.refresh(|| 0.0);
        state.add_cube();
        state.refresh(|| 0.0);
        assert!(state.others.iter().all(|o| o.spec.on_grid == state.spec.on_grid));
        let places = |state: &State| -> Vec<[i32; 3]> { std::iter::once(state.place).chain(state.others.iter().map(|o| o.place)).collect() };
        let pull = |state: &mut State, axis: usize, by: f64| {
            let (middle, _) = state.gizmo().unwrap();
            let far = middle + DVec3::new(-60.0, -200.0, 40.0);
            assert!(state.hold_arrow(axis, far, (middle - far).normalize()));
            let mut to = middle;
            to[axis] += by;
            state.drag(Handle::Slide(axis), far, (to - far).normalize(), DVec3::Y);
        };
        // nothing else picked: the form being edited goes up alone, and the arrows with it
        let (before, rings) = (places(&state), state.rings().unwrap().0);
        let arrows = state.gizmo().unwrap().0;
        pull(&mut state, 2, 5.0);
        let after = places(&state);
        assert_eq!(after[0], [before[0][0], before[0][1], before[0][2] + 10]);
        assert_eq!(after[1..], before[1..]);
        assert!((state.gizmo().unwrap().0 - (arrows + DVec3::Z * 5.0)).length() < 1e-9);
        // (the rings are at the middle of the whole grid, which is not where the arrows are)
        assert!((state.rings().unwrap().0 - state.gizmo().unwrap().0).length() > 1.0 && (state.rings().unwrap().0 - rings).length() < 6.0);
        // one picked with it: the two go together, and the third stays
        state.others[0].picked = true;
        pull(&mut state, 2, 3.0);
        let two = places(&state);
        assert_eq!((two[0][2] - after[0][2], two[1][2] - after[1][2], two[2]), (6, 6, after[2]));
        // moved down again beside the one that stays, and across at it: it stops short of it
        state.others[0].picked = false;
        pull(&mut state, 2, -8.0);
        let towards = [0, 1].map(|k| state.others[1].place[k] - state.place[k]);
        let axis = if towards[0].abs() > towards[1].abs() { 0 } else { 1 };
        let was = state.place[axis];
        pull(&mut state, axis, towards[axis] as f64 / 2.0);
        assert!(state.place[axis] != was && state.place[axis] != state.others[1].place[axis], "did not move towards it, or went into it: {:?}", places(&state));
        let all = state.painted();
        let unit_box = |b: &Painted| ([0, 1, 2].map(|k| 2 * b.0.min[k] + b.3[k]), [0, 1, 2].map(|k| 2 * (b.0.min[k] + b.0.ext[k]) + b.3[k]));
        for (n, a) in all.iter().enumerate() {
            let a_box = unit_box(a);
            assert!(all[n + 1..].iter().all(|b| { let b_box = unit_box(b); (0..3).any(|k| a_box.1[k] <= b_box.0[k] || b_box.1[k] <= a_box.0[k]) }), "two forms share room");
        }
        // all picked: the whole grid, as it always was
        state.others.iter_mut().for_each(|o| o.picked = true);
        let before = places(&state);
        pull(&mut state, 0, 4.0);
        assert!(places(&state).iter().zip(&before).all(|(now, was)| now[0] - was[0] == 8));
    }

    #[test]
    fn a_hedge_starts_with_added_points_that_reach_forty_and_other_plants_with_the_usual() {
        let mut state = State::new();
        state.form = TREE;
        state.preset();
        state.usual_tree(brickloft_core::tree::Kind::Hedge);
        assert_eq!(state.spec.pull_reach, 40.0);
        state.usual_tree(brickloft_core::tree::Kind::Oak);
        assert_eq!(state.spec.pull_reach, Spec::default().pull_reach);
    }

    #[test]
    fn taking_a_change_back_builds_only_the_forms_it_changed() {
        let mut state = State::new();
        state.load_spec(Spec { form: Form::Sphere, points: vec![cell(0.0, 0.0, 12.0)], radius: 12.0, fill: true, ..Spec::default() });
        state.refresh(|| 0.0);
        state.record();
        state.add_cube();
        state.refresh(|| 0.0);
        state.record();
        // the cube is moved, and the move taken back: the sphere's bricks are the very ones it had
        let bricks = state.others[0].built.bricks.as_ptr();
        let was = state.place;
        state.merge = false;
        state.move_by([0, 0, 6], DVec3::ZERO);
        state.record();
        assert!(state.undo() && state.place == was);
        assert_eq!(state.others[0].built.bricks.as_ptr(), bricks, "the sphere was built again for a move of the cube");
        // and the cube's own, with nothing owed to be built
        assert!(!state.built.bricks.is_empty() && !state.refresh(|| 0.0));
        // a form that did change is built: the cube resized, and the resize taken back
        let size = state.built.bricks[0].ext;
        state.spec.radius += 4.0;
        state.refresh(|| 0.0);
        state.record();
        assert_ne!(state.built.bricks[0].ext, size);
        assert!(state.undo());
        state.refresh(|| 0.0);
        assert_eq!(state.built.bricks[0].ext, size);
        assert_eq!(state.others[0].built.bricks.as_ptr(), bricks);
    }

    #[test]
    fn a_form_just_added_has_its_own_bricks_and_not_the_last_forms() {
        let mut state = State::new();
        state.load_spec(Spec { form: Form::Sphere, points: vec![cell(0.0, 0.0, 12.0)], radius: 12.0, fill: true, ..Spec::default() });
        state.refresh(|| 0.0);
        let sphere = state.built.bricks.len();
        assert!(sphere > 50);
        // (as a form slow to build would leave it: the next build would be made behind the window)
        state.build_ms = 1000.0;
        state.add_cube();
        // before anything is built again: one brick, the cube's, and the sphere set down whole beside it
        assert_eq!((state.built.bricks.len(), state.others[0].built.bricks.len()), (1, sphere));
        // and its first build is made at once, not behind the window
        state.refresh_behind(|| 0.0);
        assert!(!state.fitting() && state.built.bricks.len() == 1 && state.form == CUBE);
    }

    #[test]
    fn joined_forms_are_drawn_apart_while_one_is_dragged_and_together_when_it_is_let_go() {
        let mut state = State::new();
        state.form = CUBE;
        state.preset();
        state.refresh(|| 0.0);
        state.add_cube();
        state.refresh(|| 0.0);
        let towards = [0, 1, 2].map(|k| state.others[0].place[k] - state.place[k]);
        state.merge = true;
        state.move_by([towards[0] * 2 / 3, towards[1] * 2 / 3, 10], DVec3::ZERO);
        assert!(state.joined());
        let together = state.painted();
        assert!(together.len() > 2, "the two cubes were not cut into one build");
        // dragged: each its own one brick, and what is drawn of the one being edited does not change as it goes
        state.moving = true;
        assert_eq!(state.painted().len(), 2);
        let (own, rest) = state.drawn_from();
        state.move_by([2, 0, 0], DVec3::ZERO);
        assert!(state.drawn_from().0 == own && state.drawn_from().1 == rest);
        // let go: one build again
        state.moving = false;
        assert!(state.painted().len() > 2 && state.drawn_from().0 != own);
    }

    #[test]
    fn a_form_slow_to_build_is_built_on_another_thread_and_shown_when_it_is_done() {
        let wait = |state: &mut State| {
            for _ in 0..4000 {
                if state.refresh_behind(|| 0.0) {
                    return true;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            false
        };
        let mut state = State::new();
        state.form = TREE;
        state.preset();
        state.usual_tree(brickloft_core::tree::Kind::Grass);
        state.refresh(|| 0.0);
        let flat = state.built.bricks.clone();
        // (as a lawn large enough to be slow would have it)
        state.build_ms = 1000.0;
        // a touch of the brush: nothing is built here, the lawn on screen is the one it was, and a build is under way
        state.spec.pulls.push((DVec3::new(4.0, 2.0, 2.0), DVec3::new(14.0, 0.0, 8.0)));
        assert!(!state.refresh_behind(|| 0.0) && state.fitting());
        assert_eq!(state.built.bricks, flat);
        // when it is done the lawn is the one a build here would have made
        let scene = state.scene;
        assert!(wait(&mut state) && !state.fitting() && state.scene > scene);
        let mut here = State::new();
        here.load_spec(state.full_spec());
        here.refresh(|| 0.0);
        assert!(state.built.bricks != flat && state.built.bricks == here.built.bricks);
        assert!(!state.refresh_behind(|| 0.0) && !state.fitting(), "built twice");
        // Another form chosen while a build is under way: the lawn is set
        // down among the others as it now is, not as it was, and the
        // build's answer is not taken for the new form's.
        state.build_ms = 1000.0;
        state.spec.pulls.push((DVec3::new(-10.0, 0.0, 2.0), DVec3::new(10.0, 0.0, 6.0)));
        assert!(!state.refresh_behind(|| 0.0) && state.fitting());
        let mut touched = State::new();
        touched.load_spec(state.full_spec());
        touched.refresh(|| 0.0);
        state.add_cube();
        assert_eq!(state.others[0].built.bricks, touched.built.bricks, "the lawn was left with the bricks it had before its last touch");
        state.refresh(|| 0.0);
        let cube = state.built.bricks.clone();
        std::thread::sleep(std::time::Duration::from_millis(1500));
        state.refresh_behind(|| 0.0);
        assert!(state.form == CUBE && state.built.bricks == cube);
    }

    #[test]
    fn a_plants_blend_and_stripes_run_up_it_however_it_is_turned_or_flipped() {
        for mode in [crate::paint::Mode::Blend, crate::paint::Mode::Stripes] {
            let plant = |turns: &[usize], flipped: bool| {
                let mut state = State::new();
                state.form = TREE;
                state.preset();
                state.usual_tree(brickloft_core::tree::Kind::Tulip);
                state.paint = Paint { mode, colours: vec![[200, 0, 0], [0, 200, 0], [0, 0, 200]], axis: 2, width: 3 };
                turns.iter().for_each(|axis| state.turn(*axis, false));
                state.spec.flip[2] = flipped;
                state.refresh(|| 0.0);
                assert!(state.problem.is_none(), "{:?}", state.problem);
                let colours: Vec<Option<[u8; 3]>> = state.painted().iter().map(|brick| brick.1).collect();
                (colours, state.built.bricks.clone())
            };
            // as it grew: the first colour at its foot, others above, and in a blend the last at its top (stripes come round again)
            let (upright, bricks) = plant(&[], false);
            let (foot, top) = (bricks.iter().enumerate().min_by_key(|(_, b)| b.min[2]).unwrap().0, bricks.iter().enumerate().max_by_key(|(_, b)| b.min[2] + b.ext[2]).unwrap().0);
            assert!(upright[foot] == Some([200, 0, 0]) && (mode == crate::paint::Mode::Stripes || upright[top] == Some([0, 0, 200])) && upright.iter().collect::<std::collections::HashSet<_>>().len() >= 3, "{mode:?}: {:?} at its foot, {:?} at its top", upright[foot], upright[top]);
            // Laid on its side about x, about y, both, upside down, and upside down on its side:
            // each brick the colour it was (a build turned or flipped is the same bricks in the same order).
            for (turns, flipped) in [(&[0usize][..], false), (&[1], false), (&[0, 1], false), (&[2, 0, 0], false), (&[], true), (&[1], true)] {
                let (colours, turned) = plant(turns, flipped);
                assert!(turned.len() == bricks.len() && (turns.is_empty() && !flipped || turned != bricks));
                let wrong = colours.iter().zip(&upright).filter(|(a, b)| a != b).count();
                assert_eq!(wrong, 0, "{mode:?}, turned {turns:?}, flipped {flipped}: {wrong} of {} bricks are another colour than they were upright", bricks.len());
            }
        }
    }

    /// Not a test: what a touch of the brush on a large lawn costs, and a tree joined into it.
    #[test]
    #[ignore]
    fn timing_sculpting_a_lawn() {
        use std::time::Instant;
        let mut state = State::new();
        state.form = TREE;
        state.preset();
        state.usual_tree(brickloft_core::tree::Kind::Grass);
        state.spec.radius = 60.0;
        state.refresh(|| 0.0);
        println!("lawn: {} bricks, box {:?}", state.built.bricks.len(), state.bounds);
        let mut seed = 7u64;
        let mut next = || { seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407); ((seed >> 33) % 1000) as f64 / 1000.0 };
        for n in 0..24 {
            let at = DVec3::new(next() * 100.0 - 50.0, next() * 80.0 - 40.0, 2.0);
            state.spec.pulls.push((at, DVec3::new(10.0 + next() * 15.0, 0.0, next() * 12.0 - 4.0)));
            if n % 6 == 5 {
                state.busy = true;
                let t = Instant::now();
                state.refresh(|| 0.0);
                let dragging = t.elapsed();
                state.busy = false;
                let t = Instant::now();
                let again = state.refresh(|| 0.0);
                println!("{} touches: {} bricks; built while dragging {:?}; built again on letting go ({again}) {:?}", n + 1, state.built.bricks.len(), dragging, t.elapsed());
            }
        }
        let t = Instant::now();
        std::hint::black_box(state.painted().len());
        println!("painted, nothing joined: {:?}", t.elapsed());
        // an oak moved into it
        state.add_cube();
        state.form = TREE;
        state.preset();
        state.usual_tree(brickloft_core::tree::Kind::Oak);
        state.refresh(|| 0.0);
        let towards = [0, 1, 2].map(|k| state.others[0].place[k] - state.place[k]);
        state.merge = true;
        state.move_by([towards[0], towards[1], towards[2] + 4], DVec3::ZERO);
        println!("oak: {} bricks, joined {}", state.built.bricks.len(), state.joined());
        for n in 0..4 {
            let t = Instant::now();
            state.move_by([if n % 2 == 0 { 2 } else { -2 }, 0, 0], DVec3::ZERO);
            let moved = t.elapsed();
            let t = Instant::now();
            let all = state.painted().len();
            println!("oak moved a cell: move {:?}, painted {:?} ({all} bricks)", moved, t.elapsed());
        }
    }

    /// Not a test: what a touch of the brush costs from the build to the
    /// tiles the graphics card is given, on lawns of three sizes, with the
    /// last build kept (`brickloft_core::grass`) and with nothing kept, as
    /// every touch was before. Run by name, with `--release -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn timing_a_touch_on_lawns_of_three_sizes() {
        use std::time::{Duration, Instant};
        for (name, long, wide) in [("180 by 120", 180.0, 120.0), ("300 by 240", 300.0, 240.0), ("600 by 480", 600.0, 480.0)] {
            let mut state = State::new();
            state.form = TREE;
            state.preset();
            state.usual_tree(brickloft_core::tree::Kind::Grass);
            let (spread, half): (f64, f64) = (wide / 2.0, long / 2.0 - wide / 2.0);
            state.spec.radius = spread;
            state.spec.points = vec![DVec3::new(0.5 - half, 0.5, 0.5), DVec3::new(0.5, 0.5, 0.5), DVec3::new(0.5 + half, 0.5, 0.5)];
            let mut seed = 7u64;
            let mut next = || { seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407); ((seed >> 33) % 1000) as f64 / 1000.0 };
            state.spec.pulls = (0..24).map(|_| (DVec3::new((next() - 0.5) * (long - 20.0), (next() - 0.5) * (wide - 20.0), 2.0), DVec3::new(10.0 + next() * 15.0, 0.0, next() * 12.0 - 4.0))).collect();
            state.refresh(|| 0.0);
            let mut tiles = crate::tiles::Tiles::default();
            let mut take = |state: &State| {
                let at = Instant::now();
                let bricks = state.painted();
                let painted = at.elapsed();
                let at = Instant::now();
                let changed = tiles.take(bricks.iter().filter(|b| b.2 == 0).map(|(b, colour, ..)| (*b, colour.unwrap_or([0; 3]))));
                let given: usize = changed.now.iter().map(|key| tiles.of(key).len()).sum();
                (painted, at.elapsed(), given, bricks.len())
            };
            take(&state);
            for kept in [true, false] {
                let mut least = [Duration::MAX; 3];
                let (mut given, mut all) = (0, 0);
                for n in 0..8 {
                    // a touch 20 in range made and taken back, turn about; with nothing kept, on a lawn of another seed each time, which is a lawn never built
                    if n % 2 == 0 { state.spec.pulls.push((DVec3::new(long / 4.0, wide / 5.0, 2.0), DVec3::new(20.0, 0.0, 7.0))); } else { state.spec.pulls.pop(); }
                    if !kept { state.spec.tree.seed += 1; }
                    state.busy = true;
                    let at = Instant::now();
                    state.refresh(|| 0.0);
                    let built = at.elapsed();
                    state.busy = false;
                    let (painted, taken, gave, of) = take(&state);
                    least = [least[0].min(built), least[1].min(painted), least[2].min(taken)];
                    // (with another seed every tile is other bricks: what is given then says nothing of a touch)
                    (given, all) = (gave, of);
                }
                println!("{name}, {}: build {:?}, bricks with their colours {:?}, finding the tiles that changed {:?}; {given} of {all} bricks to give the card", if kept { "the last build kept" } else { "nothing kept" }, least[0], least[1], least[2]);
            }
        }
    }

    /// Not a test: what moving and reshaping a form beside a large lawn
    /// costs, part by part. Run by name, with `--release -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn timing_a_cube_on_a_lawn() {
        use std::time::Instant;
        let mut state = State::new();
        state.form = TREE;
        state.preset();
        state.usual_tree(brickloft_core::tree::Kind::Grass);
        state.spec.radius = 60.0;
        state.refresh(|| 0.0);
        println!("lawn: {} bricks, box {:?}", state.built.bricks.len(), state.bounds);
        state.add_cube();
        state.refresh(|| 0.0);
        // onto the lawn
        let towards = [0, 1, 2].map(|k| state.others[0].place[k] - state.place[k]);
        state.move_by([towards[0], towards[1], 40], DVec3::ZERO);
        println!("cube at {:?}, lawn at {:?}", state.place, state.others[0].place);
        let time = |what: &str, mut run: Box<dyn FnMut() + '_>| {
            let at = Instant::now();
            for _ in 0..20 {
                run();
            }
            println!("{what}: {:.2} ms each", at.elapsed().as_secs_f64() * 1000.0 / 20.0);
        };
        let mut n = 0;
        time("move the cube a unit", Box::new(|| { n += 1; state.move_by([if n % 2 == 0 { 1 } else { -1 }, 0, 0], DVec3::ZERO); }));
        time("painted", Box::new(|| { std::hint::black_box(state.painted().len()); }));
        time("push_others", Box::new(|| state.push_others()));
        // down among the blades, inside the lawn's box
        state.merge = true;
        state.move_by([0, 0, -30], DVec3::ZERO);
        state.merge = false;
        state.spec.blend = None;
        println!("cube at {:?}", state.place);
        let mut n = 0;
        time("move the cube a unit, among the blades", Box::new(|| { n += 1; state.move_by([if n % 2 == 0 { 1 } else { -1 }, 0, 0], DVec3::ZERO); }));
        time("push_others, among the blades", Box::new(|| state.push_others()));
        let mut n = 0;
        time("resize the cube and refresh", Box::new(|| { n += 1; state.spec.radius += if n % 2 == 0 { 1.0 } else { -1.0 }; state.refresh(|| 0.0); }));
        time("record a step to undo", Box::new(|| state.record()));
        state.activate(0);
        state.refresh(|| 0.0);
        let mut n = 0;
        time("move the lawn a unit", Box::new(|| { n += 1; state.move_by([if n % 2 == 0 { 1 } else { -1 }, 0, 0], DVec3::ZERO); }));
        time("push_others from the lawn", Box::new(|| state.push_others()));
    }

    #[test]
    fn a_ring_is_on_the_grid_without_best_fit() {
        let misfit_at = |radius: f64| {
            let mut state = State::new();
            state.form = CIRCLE;
            state.preset();
            state.spec.radius = radius;
            state.spec.thick = 2.0;
            (state.misfit().unwrap(), state)
        };
        let (half, mut state) = misfit_at(12.5);
        let (whole, _) = misfit_at(13.0);
        // (off the grid it was 14% and more)
        assert!(half < 0.1 && whole < 0.14, "radius 12.5 fits {half}, radius 13 fits {whole}");
        best_now(&mut state, None);
        assert!(state.misfit().unwrap() <= half);
    }

    /// A best-fit search from start to finish.
    fn best_now(state: &mut State, only: Option<Handle>) {
        state.snap_best(only);
        while state.search_step() {}
    }

    #[test]
    fn best_fit_is_done_a_little_at_a_time_and_says_how_far_it_has_got() {
        let mut state = State::new(); // an arch
        state.spec.rise = Some(10.5);
        let mut whole = State::new();
        whole.spec.rise = Some(10.5);
        best_now(&mut whole, None);
        assert!(!whole.searching() && !whole.best_note.is_empty());

        // a clock that says a build takes 10 ms, and frames of 15
        let clock = std::cell::Cell::new(0.0);
        let now = || {
            clock.set(clock.get() + 5.0);
            clock.get()
        };
        state.snap_best(None);
        assert_eq!(state.search_progress(), Some(0.0));
        // the frame it is begun in only shows the bar
        state.search_for(15.0, now);
        assert_eq!(state.search_progress(), Some(0.0));
        let (mut frames, mut last) = (0, 0.0);
        while state.searching() {
            state.search_for(15.0, now);
            if let Some(p) = state.search_progress() {
                assert!(p > last && p <= 1.0, "progress went from {last} to {p}");
                last = p;
            }
            frames += 1;
        }
        assert!(frames > 3, "the whole search was done in {frames} frames");
        // and it comes to what it came to done all at once
        assert_eq!((state.spec.clone(), state.best_note.clone()), (whole.spec.clone(), whole.best_note.clone()));

        // a change made by hand part way through ends it, and is kept
        state.spec.rise = Some(10.5);
        state.snap_best(None);
        state.search_for(15.0, now);
        state.search_for(15.0, now);
        assert!(state.searching());
        state.spec.thick += 1.0;
        let mine = state.spec.clone();
        state.search_for(15.0, now);
        assert!(!state.searching());
        assert_eq!(state.spec, mine);
    }

    #[test]
    fn best_fit_respects_the_drag_lock_and_the_handle_asked_for() {
        let mut state = State::new(); // an arch, with y held
        state.spec.rise = Some(10.5);
        let feet = state.spec.points.clone();
        best_now(&mut state, Some(Handle::Rise));
        assert_eq!(state.spec.points, feet, "only the rise was to change");
        assert_ne!(state.spec.rise, Some(10.5));
        best_now(&mut state, None);
        assert!(state.spec.points.iter().all(|p| p.y == 0.5), "y is locked");
    }

    #[test]
    fn a_saved_form_comes_back_as_it_was() {
        let mut state = State::new();
        state.form = SPHERE;
        state.preset();
        state.cut = Cut::Dome;
        state.spec.radius = 11.0;
        let text = brickloft_core::report::spec_text(&state.full_spec());
        let mut other = State::new();
        other.load_spec(brickloft_core::report::parse_spec(&text).unwrap());
        assert_eq!((other.form, other.cut == Cut::Dome), (SPHERE, true));
        assert_eq!(other.full_spec(), state.full_spec());

        let mut arch = State::new();
        arch.arch = 3;
        arch.spec.rise = Some(5.0);
        other.load_spec(brickloft_core::report::parse_spec(&brickloft_core::report::spec_text(&arch.full_spec())).unwrap());
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
        sphere.form = SPHERE;
        sphere.preset();
        state.add_form(sphere.full_spec());
        assert!(state.refresh(|| 0.0));
        assert_eq!((state.form, state.others.len()), (SPHERE, 1));
        // (where it stands is in units, two to a cell)
        assert!(2 * state.bounds.unwrap().0[0] + state.place[0] >= 2 * (arch_right + 4), "the sphere overlaps the arch");
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

        // moving a form moves all of it, a unit (half a microbrick) at a time,
        // and changes nothing about it but where it stands
        assert!(state.handles().iter().all(|(h, _)| *h != Handle::Move), "a form has no dot to move it by");
        let bricks = state.built.bricks.clone();
        let at = state.spec.points[0] + DVec3::new(2.0, 0.0, 5.0);
        state.grab = Some(at);
        let eye = DVec3::new(0.0, -80.0, at.z);
        state.lock = Lock::Axis(1);
        state.drag(Handle::Move, eye, (at + DVec3::new(3.4, 0.0, 2.2) - eye).normalize(), DVec3::Y);
        assert_eq!(state.place, [7, 0, 4], "3.4 and 2.2 cells, to the nearest unit");
        assert_eq!(state.spec.points, arch.points);
        assert!(!state.refresh(|| 0.0), "a move is no reason to fit the form again");
        assert_eq!(state.built.bricks, bricks);
        // its dots, and its bricks as drawn and sent, are where it now stands
        assert_eq!(state.handles()[0].1, arch.points[0] + DVec3::new(3.5, 0.0, 2.0));
        assert!(state.painted().iter().filter(|b| b.2 == 0).all(|b| b.3 == [7, 0, 4]));
        // and a second, smaller pull carries on from where the first left it
        assert_eq!(state.grab, Some(at + DVec3::new(3.5, 0.0, 2.0)));

        // By an arrow it moves along that arrow's axis alone, whatever the
        // view and whatever is locked: here along y, which the lock holds.
        let (middle, reach) = state.gizmo().unwrap();
        assert!(reach.iter().all(|r| *r >= 1.5));
        let far = middle + DVec3::new(-60.0, -20.0, 40.0);
        let towards = |p: DVec3| (p - far).normalize();
        assert!(state.hold_arrow(1, far, towards(middle + DVec3::Y * 3.0)));
        let before = state.place;
        // the cursor wanders off the arrow as it goes: 2.3 cells along it, and well aside
        state.drag(Handle::Slide(1), far, towards(middle + DVec3::new(0.0, 5.3, 1.5)), DVec3::X);
        let went = [0, 1, 2].map(|k| state.place[k] - before[k]);
        assert!(went[0] == 0 && went[2] == 0 && (3..=6).contains(&went[1]), "went {went:?}");
        assert_eq!(state.gizmo().unwrap().0, middle + DVec3::Y * went[1] as f64 / 2.0);
        assert!(!state.refresh(|| 0.0) && state.built.bricks == bricks);
        // looking straight down an arrow, there is nowhere along it to take hold
        assert!(!state.hold_arrow(0, middle - DVec3::X * 50.0, DVec3::X));
        state.place = before;
        state.scene += 1;
        state.grab = Some(at + DVec3::new(3.5, 0.0, 2.0));

        // only a form with one size all round is resized by the wheel
        assert!(!state.resize(1.0), "an arch has no single size");
        // each of those changes can be taken back and made again
        state.record();
        let moved = state.place;
        state.spec.thick = 5.0;
        state.record();
        state.paint.colours = vec![[1, 2, 3]];
        state.record();
        assert!(state.undo());
        assert_eq!((state.spec.thick, state.paint.colours.clone()), (5.0, vec![[10, 20, 30]]));
        assert!(state.undo());
        assert_eq!((state.spec.thick, state.place, state.others.len()), (arch.thick, moved, 1));
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
        assert_eq!((state.spec.thick, state.place), (arch.thick, moved));
        // a move is a step of its own
        state.place[0] += 3;
        state.record();
        assert!(state.undo());
        assert_eq!(state.place, moved);
        state.refresh(|| 0.0);

        // dragged into the other form, it stops against it or comes to rest on it
        state.refresh(|| 0.0);
        let overlap = |state: &State| Standing::new(&state.others[0].built.bricks, state.others[0].place).meets(cells_of(&state.built.bricks), state.place);
        assert!(!overlap(&state));
        // where each begins and ends in units, and so how far apart their middles are
        let low = |b: Bounds, place: [i32; 3], k: usize| 2 * b.0[k] + place[k];
        let middle = |b: Bounds, place: [i32; 3], k: usize| b.0[k] + b.1[k] + place[k];
        let (other, theirs, mine, before) = (state.others[0].bounds.unwrap(), state.others[0].place, state.bounds.unwrap(), state.place);
        // straight at the middle of it (the drag is in the plane facing the eye, so not along y)
        let aim = [middle(other, theirs, 0) - middle(mine, before, 0), 0, low(other, theirs, 2) - low(mine, before, 2)];
        state.lock = Lock::Free;
        let at = state.handles()[0].1;
        state.grab = Some(at);
        let target = at + DVec3::new(aim[0] as f64, aim[1] as f64, aim[2] as f64) / 2.0;
        state.drag(Handle::Move, target + DVec3::new(0.0, -200.0, 0.0), DVec3::Y, DVec3::Y);
        assert_ne!(state.place, before, "it did not move at all");
        assert!(!overlap(&state), "the two forms overlap");
        // not where it was aimed, which is inside the other, but up or aside from there
        let went = [0, 1, 2].map(|k| state.place[k] - before[k]);
        let off: Vec<i32> = (0..3).map(|k| went[k] - aim[k]).filter(|d| *d != 0).collect();
        assert!(off.len() == 1 && (went[2] - aim[2]) >= 0, "aimed {aim:?}, went {went:?}");
        // resting on it or against it: one unit back the way it came would overlap
        let back = (0..3).find(|&k| went[k] != aim[k]).unwrap();
        state.place[back] -= (went[back] - aim[back]).signum();
        assert!(overlap(&state), "it stopped short of the other form");
        state.place = before;

        // removing the form being edited leaves the other; the last cannot go
        state.remove_active();
        assert_eq!((state.form, state.others.len()), (SPHERE, 0));
        let before = state.spec.radius;
        assert!(state.resize(-1.0) && state.spec.radius == before - 1.0);
        assert!(state.resize(-1000.0) && state.spec.radius == 0.5);
        state.remove_active();
        assert_eq!(state.form, SPHERE);
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
        state.form = SPHERE;
        state.preset();
        let (_, at) = state.handles()[1];
        let out = at + DVec3::X * 3.4;
        state.drag(Handle::Radius, eye, (out - eye).normalize(), DVec3::Y);
        assert_eq!(state.spec.radius, 11.5);
    }
    /// A cube as the menu starts one, built.
    fn a_cube() -> State {
        let mut state = State::new();
        state.form = CUBE;
        state.preset();
        state.refresh(|| 0.0);
        state
    }

    #[test]
    fn a_cube_starts_the_same_each_way_with_a_dot_on_every_side() {
        let state = a_cube();
        assert_eq!(state.full_spec().form, Form::Cube);
        // one brick, 17 each way, standing on the floor about the cell of its middle dot
        assert_eq!(state.bounds, Some(([-8, -8, 0], [9, 9, 17])));
        assert_eq!((state.built.bricks.len(), state.built.error), (1, 0.0));
        let dots = state.handles();
        assert_eq!(dots.iter().map(|d| d.0).collect::<Vec<_>>(), [Handle::Point(0)].into_iter().chain((0..6).map(Handle::Side)).collect::<Vec<_>>());
        // the middle dot in the middle, and each side's dot in the middle of that side of the brick
        assert_eq!(dots[0].1, DVec3::new(0.5, 0.5, 8.5));
        for side in 0..6 {
            let mut at = DVec3::new(0.5, 0.5, 8.5);
            at[side / 2] = [-8.0, 9.0, -8.0, 9.0, 0.0, 17.0][side];
            assert_eq!(dots[1 + side].1, at, "side {side}");
        }
    }

    #[test]
    fn dragging_a_side_of_a_cube_moves_that_side_and_no_other() {
        for side in 0..6 {
            let mut state = a_cube();
            state.place = [6, -4, 2]; // standing somewhere else, by whole and half microbricks
            let axis = side / 2;
            let outwards = if side % 2 == 0 { -1.0 } else { 1.0 };
            let (low, high) = state.bounds.unwrap();
            let at = state.handles().iter().find(|h| h.0 == Handle::Side(side)).unwrap().1;
            // looked at from square to the side's axis, the cursor 3.2 microbricks further out
            let mut way = DVec3::ZERO;
            way[axis] = outwards;
            let mut from = DVec3::ZERO;
            from[(axis + 1) % 3] = 1.0;
            state.drag(Handle::Side(side), at + way * 3.2 - from * 80.0, from, from);
            assert!(state.refresh(|| 0.0) && state.problem.is_none(), "side {side}: {:?}", state.problem);
            let (mut low_after, mut high_after) = (low, high);
            if side % 2 == 0 { low_after[axis] -= 3 } else { high_after[axis] += 3 };
            assert_eq!(state.bounds, Some((low_after, high_after)), "side {side}");
            assert_eq!((state.built.bricks.len(), state.built.error), (1, 0.0), "side {side}");
            // the dot is on the side it moved, and the next drag starts from there
            let now = state.handles().iter().find(|h| h.0 == Handle::Side(side)).unwrap().1;
            assert_eq!(now[axis], at[axis] + outwards * 3.0, "side {side}");
            // pushed in through the side opposite, it stops a microbrick short of it
            state.drag(Handle::Side(side), now - way * 60.0 - from * 80.0, from, from);
            state.refresh(|| 0.0);
            let (mut low_thin, mut high_thin) = (low, high);
            if side % 2 == 0 { low_thin[axis] = high[axis] - 1 } else { high_thin[axis] = low[axis] + 1 };
            assert_eq!(state.bounds, Some((low_thin, high_thin)), "side {side}");
        }
        // a cube turned a quarter turn is dragged as it is seen: the side that now faces up moves up
        // (where its bricks are in the workspace, in units: the cube is turned about its middle, which a side moves)
        let stands = |state: &State| {
            let (low, high) = state.bounds.unwrap();
            ([0, 1, 2].map(|k| 2 * low[k] + state.place[k]), [0, 1, 2].map(|k| 2 * high[k] + state.place[k]))
        };
        let mut state = a_cube();
        state.turn(0, false);
        state.refresh(|| 0.0);
        let (low, high) = stands(&state);
        let top = *state.handles().iter().filter(|h| matches!(h.0, Handle::Side(_))).max_by(|a, b| a.1.z.total_cmp(&b.1.z)).unwrap();
        state.drag(top.0, top.1 + DVec3::Z * 4.0 - DVec3::Y * 80.0, DVec3::Y, DVec3::Y);
        state.refresh(|| 0.0);
        assert_eq!(stands(&state), (low, [high[0], high[1], high[2] + 8]));
        // and its dot is still on the side it moved
        assert_eq!(state.handles().iter().find(|h| h.0 == top.0).unwrap().1, top.1 + DVec3::Z * 4.0);
    }

    #[test]
    fn a_cube_made_short_one_way_takes_a_pattern_on_its_broad_face() {
        // a wall: 2 through along x, by dragging its high x side in
        let mut state = a_cube();
        let at = state.handles().iter().find(|h| h.0 == Handle::Side(1)).unwrap().1;
        state.drag(Handle::Side(1), at - DVec3::X * 15.0 - DVec3::Y * 80.0, DVec3::Y, DVec3::Y);
        state.spec.mosaic = Some(brickloft_core::pattern::Mosaic { style: 0, size: 2, bevel: false, border: 1, seed: 1, mortar: true, stagger: -1, upright: false, uneven: [0; 2] });
        state.paint = Paint { mode: crate::paint::Mode::Pattern, colours: vec![[200, 200, 200], [150, 30, 30], [30, 30, 150]], ..Paint::default() };
        assert!(state.refresh(|| 0.0) && state.problem.is_none(), "{:?}", state.problem);
        assert_eq!(state.bounds, Some(([-8, -8, 0], [-6, 9, 17])));
        // tiles, each right through the wall, in more than one colour
        assert_eq!(state.built.shades.len(), state.built.bricks.len());
        assert!(state.built.bricks.len() > 20 && state.built.bricks.iter().all(|b| (b.min[0], b.ext[0]) == (-8, 2)));
        let colours: std::collections::HashSet<_> = state.painted().iter().map(|b| b.1).collect();
        assert!(colours.len() > 1, "the pattern is one colour");
    }

    #[test]
    fn a_form_added_is_a_plain_cube_and_a_duplicate_is_a_copy() {
        let mut state = State::new();
        state.form = SPHERE;
        state.preset();
        state.spec.max_ext = 40;
        state.paint = Paint { mode: crate::paint::Mode::Blend, colours: vec![[200, 30, 30], [20, 40, 220]], axis: 0, width: 3 };
        state.spec.on_grid = 3;
        state.spec.grid = [0.0, 0.0, 0.6, 0.8];
        state.refresh(|| 0.0);
        let (sphere, paint) = (state.full_spec(), state.paint.clone());
        // Duplicate: the same form in the same colours, on its grid
        state.add_copy();
        assert_eq!((state.full_spec(), state.paint.clone(), state.others.len()), (sphere.clone(), paint.clone(), 1));
        // Add to this grid: a cube as the menu starts one, plain, on the same grid at the same angle
        state.add_cube();
        state.refresh(|| 0.0);
        let cube = state.full_spec();
        assert_eq!((cube.form, cube.radius, cube.stretch, cube.fill, cube.points.clone()), (Form::Cube, 8.5, DVec3::ONE, true, vec![cell(0.0, 0.0, 8.0)]));
        assert_eq!((cube.on_grid, cube.grid), (3, [0.0, 0.0, 0.6, 0.8]));
        assert_eq!(cube.max_ext, 40, "how the build is fitted was not carried over");
        assert!(cube.cut.is_none() && state.paint == Paint::default() && state.form == CUBE);
        assert_eq!((state.others.len(), state.built.bricks.len()), (2, 1));
        // set down clear of the forms already there
        assert!(state.others.iter().all(|o| o.place[0] < state.place[0]));
        // New grid: the same cube on a level grid of its own
        state.paint = paint;
        state.record();
        state.add_grid();
        let apart = state.full_spec();
        assert_eq!((apart.form, apart.on_grid, apart.grid, state.paint.clone()), (Form::Cube, 4, LEVEL, Paint::default()));
        assert_eq!(state.others.len(), 3);
        // one step to undo
        assert!(state.undo() && state.others.len() == 2 && state.spec.on_grid == 3);
    }

    /// A bookcase as the menu starts one, built.
    fn a_bookcase() -> State {
        let mut state = State::new();
        state.form = FURNITURE;
        state.preset();
        state.refresh(|| 0.0);
        state
    }

    /// How many shelves, bays and books the bookcase being edited has.
    fn counted(state: &State) -> (usize, usize, usize) {
        let spec = state.full_spec().snapped();
        let count = brickloft_core::furniture::Model::new(&spec.furniture, brickloft_core::spec::cube_sides(&spec, spec.points[0]), spec.max_ext, spec.wedges).unwrap().count;
        (count.shelves, count.bays, count.books)
    }

    #[test]
    fn furniture_starts_as_a_bookcase_on_the_floor_in_its_own_colours() {
        use brickloft_core::furniture::{Furniture, Kind};
        let mut state = a_bookcase();
        assert_eq!((state.full_spec().form, state.spec.furniture), (Form::Furniture, Furniture::of(Kind::Bookcase)));
        // 36 by 10 by 60, standing on the floor
        assert_eq!(state.bounds, Some(([-18, -5, 0], [18, 5, 60])));
        assert!(state.problem.is_none() && state.built.error == 0.0 && state.built.shades.len() == state.built.bricks.len());
        assert_eq!((state.paint.mode, state.paint.colours.as_slice()), (crate::paint::Mode::Pattern, crate::paint::palette("bookcase")));
        // its bricks are in its colours: the wood, the back, and several of the books'
        let seen: std::collections::HashSet<[u8; 3]> = state.painted().iter().filter_map(|b| b.1).collect();
        assert!(seen.len() >= 6 && seen.iter().all(|c| state.paint.colours.contains(c)), "{} colours", seen.len());
        // a dot at its middle and one on each side, as a cube has; no points are added to it, and nothing about it is searched for
        assert_eq!(state.handles().iter().map(|d| d.0).collect::<Vec<_>>(), [Handle::Point(0)].into_iter().chain((0..6).map(Handle::Side)).collect::<Vec<_>>());
        assert!(!state.can_pull() && state.add_pull(DVec3::new(0.5, -5.0, 30.0)).is_none() && state.tunable(None).is_empty());
        // every set of colours has a colour for each of its eight numbers, and a name
        let sets = crate::paint::palettes("bookcase");
        assert!(sets.len() == crate::paint::furniture_palette_names("bookcase").len() && sets.iter().all(|set| set.len() == crate::paint::USUAL_COLOURS));
        // put back to how it starts, it keeps its seed and the floor it stands on
        state.spec.furniture = Furniture { seed: 9, board: 5, books: 10, ..state.spec.furniture };
        state.spec.points[0].z += 20.0;
        state.usual_furniture(Kind::Bookcase);
        state.refresh(|| 0.0);
        assert_eq!((state.spec.furniture, state.bounds), (Furniture { seed: 9, ..Furniture::of(Kind::Bookcase) }, Some(([-18, -5, 20], [18, 5, 80]))));
        // and a save of it is read back as it was
        let forms = state.forms();
        assert_eq!(crate::store::forms_in(&crate::store::describe(&forms)).unwrap(), forms);
    }

    #[test]
    fn dragging_a_side_of_a_bookcase_resizes_it_and_its_shelves_and_books_follow() {
        // the cursor `far` microbricks further out than the dot on `side`, seen from square to the side's axis
        let dragged = |state: &mut State, side: usize, far: f64| {
            let axis = side / 2;
            let at = state.handles().iter().find(|h| h.0 == Handle::Side(side)).unwrap().1;
            let (mut way, mut from) = (DVec3::ZERO, DVec3::ZERO);
            (way[axis], from[(axis + 1) % 3]) = (if side % 2 == 0 { -1.0 } else { 1.0 }, 1.0);
            state.drag(Handle::Side(side), at + way * far - from * 80.0, from, from);
            assert!(state.refresh(|| 0.0) && state.problem.is_none(), "side {side}: {:?}", state.problem);
            assert!(state.built.error == 0.0 && state.built.shades.len() == state.built.bricks.len(), "side {side}");
        };
        let mut state = a_bookcase();
        state.place = [6, -4, 2];
        let (shelves, bays, books) = counted(&state);
        assert_eq!((shelves, bays), (3, 1));
        // its top dragged up by 26: the floor stays, and there are two more shelves, with books on them
        dragged(&mut state, 5, 26.2);
        assert_eq!(state.bounds, Some(([-18, -5, 0], [18, 5, 86])));
        let taller = counted(&state);
        assert!(taller.0 == shelves + 2 && taller.1 == 1 && taller.2 > books, "{taller:?}");
        // its right side dragged out by 40: the left stays, and it is two bays (70 inside, and a bay is 34 at most)
        dragged(&mut state, 1, 40.4);
        assert_eq!(state.bounds, Some(([-18, -5, 0], [58, 5, 86])));
        let wider = counted(&state);
        assert!(wider.0 == taller.0 && wider.1 == 2 && wider.2 * 2 > 3 * taller.2, "{wider:?} after {taller:?}");
        // its front dragged in by 4: shallower, and the same shelves and bays
        dragged(&mut state, 2, -4.0);
        assert_eq!(state.bounds, Some(([-18, -1, 0], [58, 5, 86])));
        assert_eq!((counted(&state).0, counted(&state).1), (wider.0, wider.1));
        // each side pushed in through the one opposite stops a microbrick short of it, and something is still built
        for side in [5, 1, 2] {
            dragged(&mut state, side, -500.0);
        }
        assert_eq!(state.bounds, Some(([-18, 4, 0], [-17, 5, 1])));
        assert_eq!(state.built.bricks.len(), 1);
        // and dragged back out it is a bookcase again
        for (side, far) in [(5, 59.0), (1, 35.0), (2, 9.0)] {
            dragged(&mut state, side, far);
        }
        assert_eq!(state.bounds, Some(([-18, -5, 0], [18, 5, 60])));
        assert_eq!(counted(&state), (shelves, bays, books));
        // undone, it is as it was before the last drag
        state.record();
        dragged(&mut state, 5, 30.0);
        assert!(state.undo() && state.refresh(|| 0.0));
        assert_eq!(state.bounds, Some(([-18, -5, 0], [18, 5, 60])));
    }

    #[test]
    fn a_piece_made_into_another_kind_keeps_its_style_and_seed_and_the_rest_are_made_to_match() {
        use brickloft_core::furniture::{Furniture, Kind, Style};
        let mut state = a_bookcase();
        state.spec.furniture = Furniture { style: Style::Rustic, seed: 7, ..state.spec.furniture };
        // made a dining table: its usual size, on the floor it stood on, in the rustic colours, and still the seventh rustic piece
        state.usual_furniture(Kind::Dining);
        state.refresh(|| 0.0);
        assert_eq!(state.spec.furniture, Furniture { style: Style::Rustic, seed: 7, ..Furniture::of(Kind::Dining) });
        assert_eq!(state.bounds, Some(([-22, -12, 0], [22, 12, 19])));
        assert_eq!(state.paint.colours.as_slice(), crate::paint::palette("rustic furniture"));
        assert!(state.problem.is_none() && state.built.error == 0.0 && !state.can_pull());
        // its dots resize it as they do a cube, and it is a table at the new size
        let top = state.handles().iter().find(|h| h.0 == Handle::Side(5)).unwrap().1;
        state.drag(Handle::Side(5), top + DVec3::Z * 6.2 - DVec3::X * 80.0, DVec3::X, DVec3::X);
        state.refresh(|| 0.0);
        assert_eq!(state.bounds, Some(([-22, -12, 0], [22, 12, 25])));
        // two chairs of other styles beside it, and a bookcase: matched, the chairs are the table's style, seed and colours, and the bookcase is left alone
        for (kind, style) in [(Kind::Chair, Style::Modern), (Kind::Chair, Style::Classical), (Kind::Bookcase, Style::Modern)] {
            state.record();
            state.add_cube();
            state.form = FURNITURE;
            state.spec.furniture.style = style;
            state.usual_furniture(kind);
            state.refresh(|| 0.0);
        }
        let table = state.others.iter().position(|o| o.spec.furniture.kind == Kind::Dining).unwrap();
        state.activate(table);
        state.refresh(|| 0.0);
        assert_eq!(state.spec.furniture.kind, Kind::Dining);
        let before = state.forms();
        assert_eq!(state.match_furniture(), 2);
        let after = state.forms();
        for (n, ((spec, paint, _), (was, ..))) in after.iter().zip(&before).enumerate() {
            match spec.furniture.kind {
                Kind::Bookcase => assert_eq!(spec, was, "form {n}"),
                _ => assert_eq!((spec.furniture.style, spec.furniture.seed, paint.colours.as_slice()), (Style::Rustic, 7, crate::paint::palette("rustic furniture")), "form {n}"),
            }
        }
        // one step to undo, and nothing to do a second time
        assert_eq!(state.match_furniture(), 2);
        assert!(state.undo());
    }
}
