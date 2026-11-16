//! The Brickloft editor: a view of a form and its bricks, handles to reshape it,
//! and a copy of the bricks to the clipboard for the game to paste.
//!
//! All geometry comes from `brickloft-core`; this crate draws it and edits the
//! spec. It builds as a desktop program and, as an optional extra, as a
//! page for a browser (see docs/web-page.md).

#[cfg(not(target_arch = "wasm32"))]
pub mod brz;
pub mod camera;
#[cfg(not(target_arch = "wasm32"))]
pub mod crash;
pub mod icon;
pub mod material;
#[cfg(not(target_arch = "wasm32"))]
pub mod mcp;
pub mod mesh;
pub mod paint;
pub mod panel;
#[cfg(not(target_arch = "wasm32"))]
pub mod places;
pub mod ruler;
pub mod send;
pub mod state;
pub mod store;

use brickloft_core::shapes::all_masks;

/// Where the editor listens for Claude: `mcp::address()`, said here as well
/// so that the panel, which the browser build has too, can show it.
pub const CLAUDE_ADDRESS: &str = "http://127.0.0.1:7878/mcp";
use brickloft_core::DVec3;
use state::{Handle, State};
use three_d::*;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

/// The browser calls this when the page loads the module.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn start() {
    run();
}

/// The window, with the application's icon where the platform has one.
///
/// three-d's `WindowSettings` has no icon, so on the desktop the window is
/// made here with winit, the windowing library three-d itself uses, and
/// handed over; `Window::new` does exactly this minus the icon. In the
/// browser the page's own icon (index.html) serves, and three-d makes the
/// window from the page's canvas as before.
#[cfg(not(target_arch = "wasm32"))]
fn open_window() -> Result<Window, WindowError> {
    use winit::window::{Icon, WindowBuilder};
    let event_loop = winit::event_loop::EventLoop::new();
    // 256 pixels: Windows scales it down for the title bar and up for Alt+Tab
    let icon = |size: u32| Icon::from_rgba(icon::rgba(size), size, size).ok();
    let builder = WindowBuilder::new().with_title("Brickloft").with_maximized(true).with_window_icon(icon(64));
    // the taskbar and Alt+Tab take their own, larger icon on Windows
    // https://docs.rs/winit/0.28/winit/platform/windows/trait.WindowBuilderExtWindows.html#tymethod.with_taskbar_icon
    #[cfg(target_os = "windows")]
    let builder = {
        use winit::platform::windows::WindowBuilderExtWindows;
        builder.with_taskbar_icon(icon(256))
    };
    let winit_window = builder.build(&event_loop)?;
    winit_window.focus_window();
    Window::from_winit_window(winit_window, event_loop, SurfaceSettings::default(), true)
}

#[cfg(target_arch = "wasm32")]
fn open_window() -> Result<Window, WindowError> {
    Window::new(WindowSettings { title: "Brickloft".into(), ..Default::default() })
}

/// Milliseconds from some fixed moment. `std::time::Instant` is not
/// available in the browser, where it panics.
fn now_ms() -> f64 {
    #[cfg(target_arch = "wasm32")]
    return js_sys::Date::now();
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::sync::OnceLock;
        static START: OnceLock<std::time::Instant> = OnceLock::new();
        START.get_or_init(std::time::Instant::now).elapsed().as_secs_f64() * 1000.0
    }
}

fn v(p: DVec3) -> Vec3 {
    vec3(p.x as f32, p.y as f32, p.z as f32)
}

fn d(p: Vec3) -> DVec3 {
    DVec3::new(p.x as f64, p.y as f64, p.z as f64)
}

pub fn run() {
    // (on a cube: the plainest form there is, and the one new forms are added as)
    run_form(state::CUBE, false);
}

/// Open the editor on one of `state::FORMS`, by its place in that list.
/// `claude` opens it already listening for Claude (see `mcp`), which is
/// otherwise switched on in the panel.
pub fn run_form(form: usize, claude: bool) {
    let window = match open_window() {
        Ok(window) => window,
        Err(e) => panic!("brickloft editor: could not open a WebGL2 window: {e}. A browser with WebGL2 (or a desktop with OpenGL 3.3) is needed."),
    };
    let context = window.gl();

    // Brickadia's up is Z, so the camera's is too
    let mut camera = Camera::new_perspective(window.viewport(), vec3(22.0, -42.0, 26.0), vec3(0.0, 0.0, 6.0), vec3(0.0, 0.0, 1.0), degrees(45.0), 0.5, 4000.0);
    let mut controls = camera::Controls::new(&camera);
    let mut gui = GUI::new(&context);
    // A sun that casts shadows, a weak light from the other side so that
    // faces turned away from the sun still differ from one another, and a
    // little ambient light so that nothing is black.
    let ambient = AmbientLight::new(&context, 0.32, Srgba::WHITE);
    let mut sun = DirectionalLight::new(&context, 2.6, Srgba::new_opaque(255, 247, 235), vec3(-0.45, 0.6, -1.0));
    let fill_light = DirectionalLight::new(&context, 0.55, Srgba::new_opaque(200, 215, 255), vec3(0.7, -0.5, -0.35));

    // One instanced mesh per pose of each brick shape (125 in all): the
    // whole build is drawn in at most that many draw calls, one for each
    // pose in use, however many bricks it has. A mesh per brick would be a
    // draw call per brick, which is what limits WebGL long before triangles do.
    //
    // Two sets of them: one for the form being edited and one for all the
    // others, the first after the second in `brick_meshes`. With one set,
    // every change to the workspace gave every brick of every form to the
    // graphics card again: measured with a lawn of 86,000 bricks standing
    // by, 35 to 200 ms for each step of dragging a single cube. A set is
    // now made again only when what it shows has changed (see
    // `State::drawn_from`), and the form being edited is moved by moving
    // its set, which gives the card one matrix. A set for every form would
    // spare a little more, when one of the others changes while not being
    // edited, at 125 more draw calls a form.
    let masks = all_masks();
    let shape_of = |mask: u8| {
        let shape = mesh::shape_mesh(mask);
        CpuMesh {
            positions: Positions::F32(shape.positions.iter().map(|p| vec3(p[0], p[1], p[2])).collect()),
            normals: Some(shape.normals.iter().map(|n| vec3(n[0], n[1], n[2])).collect()),
            // the first texture coordinate carries how near the rim of
            // its face a vertex is; the second is unused
            uvs: Some(shape.rim.iter().map(|r| vec2(*r, 0.0)).collect()),
            ..Default::default()
        }
    };
    let mut brick_meshes: Vec<Gm<InstancedMesh, material::BrickMaterial>> = (0..2).flat_map(|_| masks.iter()).map(|&mask| Gm::new(InstancedMesh::new(&context, &Instances::default(), &shape_of(mask)), material::BrickMaterial::default())).collect();
    // what each set was last made from
    let mut shown_from: [Option<Vec<state::DrawnFrom>>; 2] = [None, None];
    // The bricks of a form that is taken away from others (`State::ghosts`):
    // none of them is built, and with nothing drawn there would be nothing
    // to see of the form, or to take hold of, but its dots. They are drawn
    // as tinted glass, unlit, so the hole shows through its own form. The
    // meshes are a second set and not more instances of the first: a mesh
    // is drawn with one material, and glass is drawn after everything solid.
    let mut ghost_meshes: Vec<Gm<InstancedMesh, ColorMaterial>> = masks.iter().map(|&mask| Gm::new(InstancedMesh::new(&context, &Instances::default(), &shape_of(mask)), ColorMaterial::new_transparent(&context, &CpuMaterial { albedo: Srgba::new(255, 255, 255, 255), ..Default::default() }))).collect();
    let mut handle_mesh = Gm::new(
        InstancedMesh::new(&context, &Instances::default(), &CpuMesh::sphere(16)),
        ColorMaterial::new_opaque(&context, &CpuMaterial { albedo: Srgba::WHITE, ..Default::default() }),
    );
    // drawn over the bricks whatever is in front: a sphere's centre dot would
    // otherwise be buried inside it, out of sight and out of reach
    handle_mesh.material.render_states.depth_test = DepthTest::Always;
    // The three rings that turn the grid of the form being edited, by any
    // angle: red about x, green about y, blue about z. One mesh, lying
    // about z, turned to face each axis. Drawn over the bricks, as the
    // arrows are, since they are no larger and start inside the form.
    let (ring_corners, ring_triangles) = mesh::ring_mesh(0.035);
    let ring = CpuMesh { positions: Positions::F32(ring_corners.iter().map(|c| vec3(c[0], c[1], c[2])).collect()), indices: Indices::U32(ring_triangles), ..Default::default() };
    let mut rings: Vec<Gm<Mesh, ColorMaterial>> = [Srgba::new_opaque(230, 60, 60), Srgba::new_opaque(70, 200, 80), Srgba::new_opaque(70, 120, 240)]
        .into_iter()
        .map(|albedo| {
            let mut ring = Gm::new(Mesh::new(&context, &ring), ColorMaterial::new_opaque(&context, &CpuMaterial { albedo, ..Default::default() }));
            ring.material.render_states.depth_test = DepthTest::Always;
            ring
        })
        .collect();
    // The three arrows that move the form being edited: red along x, green
    // along y, blue along z. Drawn over the bricks, like the dots, since
    // they start inside the form. They also show which way the axes run,
    // which a second set of arrows fixed at the origin once did.
    let mut arrows: Vec<Gm<Mesh, ColorMaterial>> = [Srgba::new_opaque(230, 60, 60), Srgba::new_opaque(70, 200, 80), Srgba::new_opaque(70, 120, 240)]
        .into_iter()
        .map(|albedo| {
            let mut arrow = Gm::new(Mesh::new(&context, &CpuMesh::arrow(0.9, 0.45, 16)), ColorMaterial::new_opaque(&context, &CpuMaterial { albedo, ..Default::default() }));
            arrow.material.render_states.depth_test = DepthTest::Always;
            arrow
        })
        .collect();
    // the floor the build stands on, to catch its shadow
    let mut ground = Gm::new(
        Mesh::new(&context, &CpuMesh::square()),
        PhysicalMaterial::new_opaque(&context, &CpuMaterial { albedo: Srgba::new_opaque(92, 96, 104), roughness: 0.95, metallic: 0.0, ..Default::default() }),
    );
    // Drawn from above only. From underneath the floor is not there at all,
    // so a camera lowered through it looks straight up into the form
    // instead of at the floor's underside.
    ground.material.render_states.cull = Cull::Back;
    let mut shown_gap = f64::NAN;

    let mut state = State::new();
    state.claude_on = claude;
    // the leg of a flight the camera is on: where it set out from and
    // which way it was looking, and how long ago in seconds
    let mut leg: Option<(Vec3, Vec3, f64)> = None;
    // the listener Claude reaches the editor through, while the player has it switched on
    #[cfg(not(target_arch = "wasm32"))]
    let mut claude: Option<mcp::Server> = None;
    if form < state::FORMS.len() {
        state.form = form;
        state.preset();
    }
    let mut dragging: Option<Handle> = None;
    let mut shown_handles: Vec<(Handle, DVec3)> = Vec::new();
    let mut first = true;
    let mut shown_scene = state.scene;
    // for each shape's mesh, which of the workspace's forms each of its
    // bricks belongs to, so that a click on a brick says which form was hit
    let mut owners: Vec<Vec<usize>> = vec![Vec::new(); 2 * masks.len()];
    // and the same for the bricks drawn as glass
    let mut ghost_owners: Vec<Vec<usize>> = vec![Vec::new(); masks.len()];

    window.render_loop(move |mut frame| {
        camera.set_viewport(frame.viewport);
        let mut asked = panel::Asked::default();
        gui.update(&mut frame.events, frame.accumulated_time, frame.viewport, frame.device_pixel_ratio, |ui| {
            asked = panel::show(ui, &mut state);
            // Drawn here because only the panel's painter has text. The camera
            // is where the last frame left it, so while the view is turning
            // the rulers trail it by one frame.
            if let (true, Some((lo, hi))) = (state.show_ruler, state.bounds) {
                // where the form stands, to the cell below
                let stands = state.place.map(|u| u.div_euclid(2));
                ruler::draw(ui, &camera, frame.device_pixel_ratio, state.panel_right, [0, 1, 2].map(|k| lo[k] + stands[k]), [0, 1, 2].map(|k| hi[k] + stands[k]));
            }
        });

        // handles take the mouse before the camera does
        for event in frame.events.iter_mut() {
            match event {
                // Shift + right click on a point that was added takes it off again
                Event::MousePress { button: MouseButton::Right, position, modifiers, handled, .. } if !*handled && modifiers.alt => {
                    if let Ok(Some(hit)) = pick(&context, &camera, *position, &handle_mesh, Cull::Back) {
                        if let Some((Handle::Pull(which), _)) = shown_handles.get(hit.instance_id as usize) {
                            state.remove_pull_at(*which);
                            *handled = true;
                        }
                    }
                }
                // a middle click turns the camera to face what the cursor is on
                Event::MousePress { button: MouseButton::Middle, position, handled, .. } if !*handled => {
                    let towards = camera.view_direction_at_pixel(*position);
                    controls.face(&mut camera, towards);
                    *handled = true;
                }
                Event::MousePress { button: MouseButton::Left, position, modifiers, handled, .. } if !*handled => {
                    if let Ok(Some(hit)) = pick(&context, &camera, *position, &handle_mesh, Cull::Back) {
                        dragging = shown_handles.get(hit.instance_id as usize).map(|h| h.0);
                        *handled = dragging.is_some();
                    }
                    // then the arrows, which move the form along one axis
                    if dragging.is_none() && state.show_arrows && state.gizmo().is_some() {
                        if let Ok(Some(hit)) = pick(&context, &camera, *position, arrows.iter().map(|a| &a.geometry), Cull::Back) {
                            let axis = hit.geometry_id as usize;
                            let (origin, dir) = (camera.position_at_pixel(*position), camera.view_direction_at_pixel(*position));
                            if axis < 3 && state.hold_arrow(axis, d(origin), d(dir)) {
                                dragging = Some(Handle::Slide(axis));
                                *handled = true;
                            }
                        }
                    }
                    // then the rings: dragging one round turns the form's grid about its axis
                    if dragging.is_none() && !*handled && state.show_rings && state.rings().is_some() {
                        if let Ok(Some(hit)) = pick(&context, &camera, *position, rings.iter().map(|r| &r.geometry), Cull::None) {
                            let axis = hit.geometry_id as usize;
                            let (origin, dir) = (camera.position_at_pixel(*position), camera.view_direction_at_pixel(*position));
                            if axis < 3 && state.hold_ring(axis, d(origin), d(dir)) {
                                dragging = Some(Handle::Spin(axis));
                                *handled = true;
                            }
                        }
                    }
                    // Otherwise a press on a brick takes hold of its form: a
                    // form other than the one being edited becomes the one
                    // being edited, and either way dragging from there moves
                    // it. Where the press landed is what the drag holds on to.
                    if dragging.is_none() && !*handled {
                        // the meshes that have bricks, in the order they are given to `pick`: the solid ones, then the glass ones
                        // (a form that is taken away is taken hold of by its glass, whichever of it or the bricks round its hole is nearer)
                        let drawn: Vec<(&InstancedMesh, &Vec<usize>)> = brick_meshes.iter().map(|m| &m.geometry).zip(&owners).chain(ghost_meshes.iter().map(|m| &m.geometry).zip(&ghost_owners)).filter(|(m, _)| m.instance_count() > 0).collect();
                        let hit = pick(&context, &camera, *position, drawn.iter().map(|(m, _)| *m), Cull::Back);
                        if let Ok(Some(hit)) = hit {
                            let form = drawn.get(hit.geometry_id as usize).and_then(|(_, owners)| owners.get(hit.instance_id as usize));
                            if let Some(&form) = form {
                                // With Alt, on the form being edited: a new
                                // point is added there, and dragged from there.
                                // (Alt, and Shift for marking forms: Shift is what
                                // every other program picks several things with,
                                // and it was taken here by the adding of points)
                                let added = if modifiers.alt && form == 0 { state.add_pull(d(hit.position)) } else { None };
                                if let Some(pull) = added {
                                    dragging = Some(pull);
                                } else if modifiers.shift && form > 0 {
                                    // with Shift, another form is picked as well as the one being edited, or let go
                                    state.toggle_pick(form - 1);
                                } else {
                                    if form > 0 {
                                        state.activate(form - 1);
                                    }
                                    state.take_hold(d(hit.position));
                                    dragging = Some(Handle::Move);
                                }
                                *handled = true;
                            }
                        }
                    }
                }
                Event::MouseMotion { position, modifiers, handled, .. } if dragging.is_some() => {
                    state.lift = modifiers.ctrl || modifiers.command;
                    // Ctrl and not Shift: a press with Shift on the form being
                    // edited adds a point to pull it by, so a drag begun with
                    // Shift held never moved the form at all.
                    state.merge = modifiers.ctrl || modifiers.command;
                    let (origin, dir) = (camera.position_at_pixel(*position), camera.view_direction_at_pixel(*position));
                    state.drag(dragging.unwrap(), d(origin), d(dir), d(camera.view_direction()));
                    *handled = true;
                }
                Event::MouseRelease { button: MouseButton::Left, .. } => {
                    // a form only moved is the same form: there is no better fit to look for
                    if let (Some(handle), true) = (dragging.take().filter(|h| !matches!(h, Handle::Move | Handle::Slide(_) | Handle::Spin(_))), state.auto_best) {
                        state.snap_best(Some(handle));
                    }
                }
                _ => {}
            }
        }
        // while a name is being typed, its letters are not camera keys
        let typing = gui.context().egui_wants_keyboard_input();
        // Keys that act on the workspace. Not while typing, when Delete and
        // Ctrl+C, Z and Y belong to the text box.
        #[cfg_attr(target_arch = "wasm32", allow(unused_mut, unused_variables))]
        let mut copy_keyed = false;
        // a picture of the window was asked for with F12: with the panel,
        // or with Shift held the view alone
        #[cfg_attr(target_arch = "wasm32", allow(unused_mut))]
        let mut shot_keyed: Option<bool> = None;
        for event in frame.events.iter_mut() {
            // Ctrl + the wheel resizes a round form, a cell of radius a
            // notch; without Ctrl, or on any other form, the wheel is the
            // camera's.
            if let Event::MouseWheel { delta, modifiers, handled, .. } = event {
                if !*handled && (modifiers.ctrl || modifiers.command) && delta.1 != 0.0 {
                    state.resize(if delta.1 > 0.0 { 1.0 } else { -1.0 });
                    // with Ctrl the wheel is never the camera's, resized or not
                    *handled = true;
                }
            }
            if let Event::KeyPress { kind, modifiers, handled } = event {
                if typing || *handled {
                    continue;
                }
                let held = modifiers.ctrl || modifiers.command;
                *handled = match kind {
                    Key::Delete => {
                        state.record();
                        state.remove_active();
                        true
                    }
                    Key::A if held => {
                        state.pick_all();
                        true
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    Key::F12 => {
                        shot_keyed = Some(!modifiers.shift);
                        true
                    }
                    // (D alone is a camera key, as A is: with Ctrl it is taken here first)
                    Key::D if held => {
                        state.record();
                        state.add_copy();
                        state.toast = Some(("Duplicated the form being edited".to_string(), None));
                        true
                    }
                    Key::Z if held => state.undo() || true,
                    Key::Y if held => state.redo() || true,
                    #[cfg(not(target_arch = "wasm32"))]
                    Key::C if held => {
                        copy_keyed = true;
                        true
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    Key::V if held => {
                        match brz::paste() {
                            Ok(Some(forms)) => {
                                state.record();
                                let count = forms.len();
                                state.paste(forms);
                                state.toast = Some((format!("Pasted {count} form{} from the clipboard", if count == 1 { "" } else { "s" }), None));
                            }
                            // not a .brz file on the clipboard: nothing to paste
                            Ok(None) => {}
                            Err(why) => state.alert = Some(("Cannot paste", why)),
                        }
                        true
                    }
                    _ => false,
                };
            }
        }
        // A change is noted as one step to undo once the mouse is let go: a
        // drag of a dot or of a slider is then one step, not one per frame.
        // nor in the middle of a best-fit search, which is one step however
        // many of the form's numbers it ends up changing.
        if dragging.is_none() && !gui.context().input(|i| i.pointer.any_down()) && !state.searching() {
            state.record();
        }
        // Claude's changes are made here, between the player's own and the
        // fitting, so that they are fitted and drawn in this same frame.
        #[cfg(not(target_arch = "wasm32"))]
        {
            if state.claude_on && claude.is_none() {
                match mcp::Server::start() {
                    Ok(server) => {
                        claude = Some(server);
                        state.claude_note = format!("Listening at {}", mcp::address());
                    }
                    Err(why) => {
                        state.claude_on = false;
                        state.claude_note = format!("Not switched on: {why}");
                    }
                }
            } else if !state.claude_on && claude.is_some() {
                claude = None;
                state.claude_note.clear();
            }
            if let Some(server) = &mut claude {
                // said over the view each time, so that a change the player
                // did not make is never unexplained
                let called = server.serve(&mut state, now_ms);
                if called.iter().any(|name| name == "take_screenshot") {
                    // a picture is of the editor, not of the notice that one was asked for
                    state.toast = None;
                } else if let Some(last) = called.iter().rev().find(|name| !name.starts_with("get_") && *name != "measure_between") {
                    // (asking about the workspace changes nothing, and is not announced)
                    state.toast = Some((format!("Claude: {}", last.replace('_', " ")), None));
                }
            }
        }
        if let Some(speed) = controls.update(&mut camera, &mut frame.events, frame.elapsed_time, typing) {
            state.toast = Some((format!("Camera speed: {speed:.0} microbricks a second"), None));
        }
        // A flight the camera has been sent on (see `mcp`), a leg at a time.
        // It sets out and arrives gently: its place along the leg follows
        // the smoothstep curve, which starts and ends with no speed.
        // https://en.wikipedia.org/wiki/Smoothstep
        if let Some(next) = state.flight.front().copied() {
            let (from, facing, flown) = leg.get_or_insert((camera.position(), camera.view_direction(), 0.0));
            *flown += frame.elapsed_time / 1000.0;
            let share = if next.seconds > 0.0 { (*flown / next.seconds).min(1.0) } else { 1.0 };
            let eased = (share * share * (3.0 - 2.0 * share)) as f32;
            let to = v(DVec3::from_array(next.to));
            let towards = v(DVec3::from_array(next.look_at)) - to;
            // (arriving on top of what it is to look at, it keeps looking the way it was)
            let towards = if towards.magnitude() > 1e-4 { towards.normalize() } else { *facing };
            controls.place(&mut camera, *from + (to - *from) * eased, *facing + (towards - *facing) * eased);
            if share >= 1.0 {
                state.flight.pop_front();
                leg = None;
            }
        }
        state.camera_at = (d(camera.position()).to_array(), d(camera.view_direction()).to_array());
        if asked.line_up {
            state.record();
            state.line_up();
        }
        if asked.best {
            state.snap_best(None);
        }
        if let Some(axis) = asked.turn {
            state.turn(axis, false);
        }
        if asked.level {
            state.level();
        }
        // A search under way is given half of a frame at 60 a second, so
        // that the view still turns and the progress bar still moves while
        // it runs. A form that takes longer than that to build once holds
        // each frame for one build.
        state.search_for(8.0, now_ms);

        // rebuild only when the spec changed; a frame with nothing new costs nothing
        // dragging a dot or a form in the view, or anything in the panel
        state.busy = dragging.is_some() || gui.context().input(|i| i.pointer.any_down());
        let rebuilt = state.refresh(now_ms);
        if rebuilt || first || shown_gap != state.floor_gap || shown_scene != state.scene {
            shown_gap = state.floor_gap;
            // the floor sits under the lowest brick of any form, less the gap asked for
            let lowest = state.others.iter().filter_map(|i| i.bounds.map(|b| 2 * b.0[2] + i.place[2])).chain(state.bounds.map(|b| 2 * b.0[2] + state.place[2])).min().unwrap_or(0) as f32 / 2.0;
            ground.set_transformation(Mat4::from_translation(vec3(0.0, 0.0, lowest - state.floor_gap as f32 - 0.02)) * Mat4::from_scale(600.0));
        }
        if rebuilt || first || shown_scene != state.scene {
            shown_scene = state.scene;
            let bricks = state.painted();
            let picked = state.picked();
            // each form's grid, where it is at an angle: turned, then moved to where the grid is
            let grids: Vec<Option<Mat4>> = state
                .grids()
                .into_iter()
                .map(|(at, q)| (q != state::LEVEL).then(|| Mat4::from_translation(vec3(at[0] as f32, at[1] as f32, at[2] as f32)) * Mat4::from(Quat::new(q[3] as f32, q[0] as f32, q[1] as f32, q[2] as f32))))
                .collect();
            let ghosts = state.ghosts();
            for ((object, &mask), owner) in ghost_meshes.iter_mut().zip(&masks).zip(ghost_owners.iter_mut()) {
                let mut instances = Instances::default();
                let mut colours = Vec::new();
                owner.clear();
                for (b, form, place) in ghosts.iter().filter(|b| b.0.mask == mask) {
                    let at = vec3(b.min[0] as f32 + place[0] as f32 / 2.0, b.min[1] as f32 + place[1] as f32 / 2.0, b.min[2] as f32 + place[2] as f32 / 2.0);
                    let on_lattice = Mat4::from_translation(at) * Mat4::from_nonuniform_scale(b.ext[0] as f32, b.ext[1] as f32, b.ext[2] as f32);
                    instances.transformations.push(match grids.get(*form).copied().flatten() {
                        Some(turn) => turn * on_lattice,
                        None => on_lattice,
                    });
                    // red, as what is taken away; fainter for a form that is not the one being edited
                    colours.push(Srgba::new(235, 90, 70, if *form == 0 { 90 } else { 45 }));
                    owner.push(*form);
                }
                instances.colors = Some(colours);
                object.geometry.set_instances(&instances);
            }
            // The form being edited is drawn from where it stands: its
            // bricks are placed as if it stood at the origin of a level
            // grid, and its whole set is then moved there and turned.
            let stands = vec3(state.place[0] as f32 / 2.0, state.place[1] as f32 / 2.0, state.place[2] as f32 / 2.0);
            let own_way = grids.first().copied().flatten().unwrap_or(Mat4::identity()) * Mat4::from_translation(stands);
            let (own_from, rest_from) = state.drawn_from();
            let now_from = [rest_from, vec![own_from]];
            // which mask is which place in a set
            let place_of: std::collections::HashMap<u8, usize> = masks.iter().enumerate().map(|(n, &mask)| (mask, n)).collect();
            for (set, now) in now_from.into_iter().enumerate() {
                let own = set == 1;
                let meshes = &mut brick_meshes[set * masks.len()..(set + 1) * masks.len()];
                if own {
                    meshes.iter_mut().for_each(|object| object.geometry.set_transformation(own_way));
                }
                if shown_from[set].as_ref() == Some(&now) {
                    continue;
                }
                shown_from[set] = Some(now);
                // (each brick is put with the others of its shape in one pass over them all)
                let mut sorted: Vec<(Instances, Vec<Srgba>, Vec<usize>)> = (0..masks.len()).map(|_| (Instances::default(), Vec::new(), Vec::new())).collect();
                for (b, colour, form, place) in bricks.iter().filter(|b| (b.2 == 0) == own) {
                    let Some((instances, colours, owner)) = place_of.get(&b.mask).map(|&n| &mut sorted[n]) else { continue };
                    // the mesh is a unit box at the origin: scale it to the
                    // brick's extent, then move it to the brick's corner
                    // (its form may stand an odd unit, half a cell, off the cells)
                    let place = if own { [0, 1, 2].map(|k| place[k] - state.place[k]) } else { *place };
                    let at = vec3(b.min[0] as f32 + place[0] as f32 / 2.0, b.min[1] as f32 + place[1] as f32 / 2.0, b.min[2] as f32 + place[2] as f32 / 2.0);
                    let on_lattice = Mat4::from_translation(at) * Mat4::from_nonuniform_scale(b.ext[0] as f32, b.ext[1] as f32, b.ext[2] as f32);
                    // then turned with its form's grid, if that is at an angle
                    instances.transformations.push(match grids.get(*form).copied().flatten().filter(|_| !own) {
                        Some(turn) => turn * on_lattice,
                        None => on_lattice,
                    });
                    // a brick left to the player's colour in the game is
                    // shown in a neutral one; the forms not being edited
                    // are shown a little darker, to tell them apart
                    let [r, g, b] = colour.unwrap_or([214, 208, 196]);
                    // (and those picked with it, to be copied, only a little)
                    let dim = if *form == 0 { 1.0 } else if picked.get(*form).copied().unwrap_or(false) { 0.9 } else { 0.72 };
                    let shade = |c: u8| (c as f32 * dim) as u8;
                    colours.push(Srgba::new_opaque(shade(r), shade(g), shade(b)));
                    owner.push(*form);
                }
                for ((object, (mut instances, colours, owner)), owned) in meshes.iter_mut().zip(sorted).zip(&mut owners[set * masks.len()..]) {
                    instances.colors = Some(colours);
                    object.geometry.set_instances(&instances);
                    *owned = owner;
                }
            }
            // The shadow map is redrawn only here, when the bricks change, not
            // every frame: turning the camera does not move a shadow.
            let casters = brick_meshes.iter().filter(|m| m.geometry.instance_count() > 0).map(|m| &m.geometry);
            if let Err(e) = sun.generate_shadow_map(2048, casters) {
                eprintln!("brickloft editor: no shadows this time: {e}");
            }
        }
        let handles = if state.show_dots { state.handles() } else { Vec::new() };
        if handles != shown_handles || first {
            let mut instances = Instances::default();
            let mut colours = Vec::new();
            for (handle, at) in &handles {
                instances.transformations.push(Mat4::from_translation(v(*at)) * Mat4::from_scale(0.7));
                colours.push(match handle {
                    Handle::Move | Handle::Slide(_) | Handle::Spin(_) => Srgba::new_opaque(250, 250, 250),
                    Handle::Point(_) => Srgba::new_opaque(255, 220, 60),
                    Handle::Pull(_) => Srgba::new_opaque(255, 140, 40),
                    Handle::Radius | Handle::Rise | Handle::Side(_) => Srgba::new_opaque(70, 230, 240),
                });
            }
            instances.colors = Some(colours);
            handle_mesh.geometry.set_instances(&instances);
            shown_handles = handles;
        }
        first = false;

        #[cfg(not(target_arch = "wasm32"))]
        if asked.copy || copy_keyed {
            // Said over the view as well as in the panel: the keys give no
            // other sign that anything happened. A copy that worked is said
            // in passing; one that did not has to be read and closed.
            // what is picked: the form being edited, and any picked with it
            let (bricks, forms) = state.copied();
            let said = brz::copy(&bricks, &forms);
            if said.starts_with("Copied") {
                state.toast = Some((said.clone(), None));
            } else {
                state.alert = Some(("Not copied", said.clone()));
            }
            state.report_note = said;
        }
        if asked.clear_reports {
            state.report_note = send::clear_reports();
        }
        if asked.report {
            state.report_note = match brickloft_core::report::report(&state.full_spec()) {
                Ok(text) => send::save_report(&text),
                Err(e) => format!("No report: {e}"),
            };
        }

        let lights: [&dyn Light; 3] = [&ambient, &sun, &fill_light];
        let screen = frame.screen();
        screen.clear(ClearState::color_and_depth(0.17, 0.19, 0.23, 1.0, 1.0));
        screen.render(&camera, &ground, &lights);
        screen.render(&camera, brick_meshes.iter().filter(|m| m.geometry.instance_count() > 0), &lights);
        screen.render(&camera, ghost_meshes.iter().filter(|m| m.geometry.instance_count() > 0), &[]);
        if let Some((middle, wide)) = state.rings().filter(|_| state.show_rings) {
            // the ring's mesh lies about z, one wide: turned to face its axis and grown to clear the form
            let faces = [Mat4::from_angle_y(degrees(90.0)), Mat4::from_angle_x(degrees(90.0)), Mat4::identity()];
            for (ring, face) in rings.iter_mut().zip(faces) {
                ring.set_transformation(Mat4::from_translation(v(middle)) * face * Mat4::from_scale(wide as f32));
            }
            screen.render(&camera, &rings, &lights);
        }
        if let Some((middle, reach)) = state.gizmo().filter(|_| state.show_arrows) {
            // the arrow's mesh lies along x, one long: turned to its axis and stretched to its length
            let turns = [Mat4::identity(), Mat4::from_angle_z(degrees(90.0)), Mat4::from_angle_y(degrees(-90.0))];
            for ((arrow, turn), length) in arrows.iter_mut().zip(turns).zip(reach) {
                // (as thick as suits its length, so that a small one is not a blob)
                let thick = (length as f32 / 10.0).clamp(0.12, 0.6);
                arrow.set_transformation(Mat4::from_translation(v(middle)) * turn * Mat4::from_nonuniform_scale(length as f32, thick, thick));
            }
            screen.render(&camera, &arrows, &lights);
        }
        screen.render(&camera, &handle_mesh, &lights);
        // the button is in the panel, so its picture has the panel in it
        let shot = shot_keyed.or(asked.screenshot.then_some(true));
        // Read back from the frame as drawn so far, before the window shows
        // it. The alternative, asking Windows for a picture of the window,
        // is one more system interface and cannot leave the panel out.
        // https://docs.rs/three-d/latest/three_d/core/render_target/struct.RenderTarget.html#method.read_color
        // (and to Claude, if it asked for the view without the panel: see below)
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(server) = claude.as_mut().filter(|server| server.wants_picture(false) && state.flight.is_empty()) {
            server.give_picture(false, frame.viewport.width, frame.viewport.height, &screen.read_color());
        }
        #[cfg(not(target_arch = "wasm32"))]
        if shot == Some(false) {
            state.toast = Some((send::save_screenshot(frame.viewport.width, frame.viewport.height, &screen.read_color()), None));
        }
        if let Err(e) = screen.write(|| gui.render()) {
            // the panel failing to draw should not take the view down with it
            eprintln!("brickloft editor: could not draw the panel: {e}");
        }
        #[cfg(not(target_arch = "wasm32"))]
        if shot == Some(true) {
            state.toast = Some((send::save_screenshot(frame.viewport.width, frame.viewport.height, &screen.read_color()), None));
        }
        // and to Claude, if it has asked to see the window: once the camera
        // has got to where it was sent, not from somewhere on the way
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(server) = claude.as_mut().filter(|server| server.wants_picture(true) && state.flight.is_empty()) {
            server.give_picture(true, frame.viewport.width, frame.viewport.height, &screen.read_color());
        }
        #[cfg(target_arch = "wasm32")]
        let _ = shot;
        FrameOutput::default()
    });
}
