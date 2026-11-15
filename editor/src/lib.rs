//! The arcs editor: a view of a form and its bricks, handles to reshape it,
//! and a button that sends the bricks to the game.
//!
//! All geometry comes from `arcs-core`; this crate draws it and edits the
//! spec. It builds for the browser (the page the plugin serves) and, for
//! development, as a desktop window.

#[cfg(not(target_arch = "wasm32"))]
pub mod brz;
pub mod camera;
pub mod icon;
pub mod material;
pub mod mesh;
pub mod paint;
pub mod panel;
pub mod ruler;
pub mod send;
pub mod state;
pub mod store;

use arcs_core::shapes::all_masks;
use arcs_core::DVec3;
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
    let builder = WindowBuilder::new().with_title("Arcs").with_maximized(true).with_window_icon(icon(64));
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
    Window::new(WindowSettings { title: "Arcs".into(), ..Default::default() })
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
    run_form(0);
}

/// Open the editor on one of `state::FORMS`, by its place in that list.
pub fn run_form(form: usize) {
    let window = match open_window() {
        Ok(window) => window,
        Err(e) => panic!("arcs editor: could not open a WebGL2 window: {e}. A browser with WebGL2 (or a desktop with OpenGL 3.3) is needed."),
    };
    let context = window.gl();

    // Brickadia's up is Z, so the camera's is too
    let mut camera = Camera::new_perspective(window.viewport(), vec3(22.0, -42.0, 26.0), vec3(0.0, 0.0, 6.0), vec3(0.0, 0.0, 1.0), degrees(45.0), 0.5, 4000.0);
    let mut controls = camera::Controls::new(camera.target());
    let mut gui = GUI::new(&context);
    // A sun that casts shadows, a weak light from the other side so that
    // faces turned away from the sun still differ from one another, and a
    // little ambient light so that nothing is black.
    let ambient = AmbientLight::new(&context, 0.32, Srgba::WHITE);
    let mut sun = DirectionalLight::new(&context, 2.6, Srgba::new_opaque(255, 247, 235), vec3(-0.45, 0.6, -1.0));
    let fill_light = DirectionalLight::new(&context, 0.55, Srgba::new_opaque(200, 215, 255), vec3(0.7, -0.5, -0.35));

    // One instanced mesh per brick shape: the whole build is drawn in at most
    // 29 draw calls however many bricks it has. A mesh per brick would be a
    // draw call per brick, which is what limits WebGL long before triangles do.
    let masks = all_masks();
    let mut brick_meshes: Vec<Gm<InstancedMesh, material::BrickMaterial>> = masks
        .iter()
        .map(|&mask| {
            let shape = mesh::shape_mesh(mask);
            let cpu = CpuMesh {
                positions: Positions::F32(shape.positions.iter().map(|p| vec3(p[0], p[1], p[2])).collect()),
                normals: Some(shape.normals.iter().map(|n| vec3(n[0], n[1], n[2])).collect()),
                // the first texture coordinate carries how near the rim of
                // its face a vertex is; the second is unused
                uvs: Some(shape.rim.iter().map(|r| vec2(*r, 0.0)).collect()),
                ..Default::default()
            };
            Gm::new(InstancedMesh::new(&context, &Instances::default(), &cpu), material::BrickMaterial::default())
        })
        .collect();
    let mut handle_mesh = Gm::new(
        InstancedMesh::new(&context, &Instances::default(), &CpuMesh::sphere(16)),
        ColorMaterial::new_opaque(&context, &CpuMaterial { albedo: Srgba::WHITE, ..Default::default() }),
    );
    // drawn over the bricks whatever is in front: a sphere's centre dot would
    // otherwise be buried inside it, out of sight and out of reach
    handle_mesh.material.render_states.depth_test = DepthTest::Always;
    let axes = Axes::new(&context, 0.12, 5.0);
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
    let mut owners: Vec<Vec<usize>> = vec![Vec::new(); masks.len()];

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
                ruler::draw(ui, &camera, frame.device_pixel_ratio, [0, 1, 2].map(|k| lo[k] + stands[k]), [0, 1, 2].map(|k| hi[k] + stands[k]));
            }
        });

        // handles take the mouse before the camera does
        for event in frame.events.iter_mut() {
            match event {
                // Shift + left-drag slides the view, so it does not pick up a handle
                Event::MousePress { button: MouseButton::Left, position, modifiers, handled, .. } if !*handled && !modifiers.shift => {
                    if let Ok(Some(hit)) = pick(&context, &camera, *position, &handle_mesh, Cull::Back) {
                        dragging = shown_handles.get(hit.instance_id as usize).map(|h| h.0);
                        *handled = dragging.is_some();
                    }
                    // Otherwise a press on a brick takes hold of its form: a
                    // form other than the one being edited becomes the one
                    // being edited, and either way dragging from there moves
                    // it. Where the press landed is what the drag holds on to.
                    if dragging.is_none() {
                        // the meshes that have bricks, in the order they are given to `pick`
                        let drawn: Vec<usize> = (0..brick_meshes.len()).filter(|&m| brick_meshes[m].geometry.instance_count() > 0).collect();
                        let hit = pick(&context, &camera, *position, drawn.iter().map(|&m| &brick_meshes[m].geometry), Cull::Back);
                        if let Ok(Some(hit)) = hit {
                            let form = drawn.get(hit.geometry_id as usize).and_then(|&m| owners[m].get(hit.instance_id as usize));
                            if let Some(&form) = form {
                                if form > 0 {
                                    state.activate(form - 1);
                                }
                                state.grab = Some(d(hit.position));
                                dragging = Some(Handle::Move);
                                *handled = true;
                            }
                        }
                    }
                }
                Event::MouseMotion { position, handled, .. } if dragging.is_some() => {
                    let (origin, dir) = (camera.position_at_pixel(*position), camera.view_direction_at_pixel(*position));
                    state.drag(dragging.unwrap(), d(origin), d(dir), d(camera.view_direction()));
                    *handled = true;
                }
                Event::MouseRelease { button: MouseButton::Left, .. } => {
                    if let (Some(handle), true) = (dragging.take(), state.auto_best) {
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
        for event in frame.events.iter_mut() {
            // Ctrl + the wheel resizes a round form, a cell of radius a
            // notch; without Ctrl, or on any other form, the wheel is the
            // camera's.
            if let Event::MouseWheel { delta, modifiers, handled, .. } = event {
                if !*handled && (modifiers.ctrl || modifiers.command) && delta.1 != 0.0 {
                    *handled = state.resize(if delta.1 > 0.0 { 1.0 } else { -1.0 });
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
                                state.alert = Some(("Pasted", format!("Added {count} form{} from the clipboard to the workspace.", if count == 1 { "" } else { "s" })));
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
        if dragging.is_none() && !gui.context().input(|i| i.pointer.any_down()) {
            state.record();
        }
        controls.update(&mut camera, &mut frame.events, frame.elapsed_time, typing);
        if asked.best {
            state.snap_best(None);
        }

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
            for ((object, &mask), owner) in brick_meshes.iter_mut().zip(&masks).zip(owners.iter_mut()) {
                let mut instances = Instances::default();
                let mut colours = Vec::new();
                owner.clear();
                for (b, colour, form, place) in bricks.iter().filter(|b| b.0.mask == mask) {
                    // the mesh is a unit box at the origin: scale it to the
                    // brick's extent, then move it to the brick's corner
                    // (its form may stand an odd unit, half a cell, off the cells)
                    let at = vec3(b.min[0] as f32 + place[0] as f32 / 2.0, b.min[1] as f32 + place[1] as f32 / 2.0, b.min[2] as f32 + place[2] as f32 / 2.0);
                    instances.transformations.push(Mat4::from_translation(at) * Mat4::from_nonuniform_scale(b.ext[0] as f32, b.ext[1] as f32, b.ext[2] as f32));
                    // a brick left to the player's colour in the game is
                    // shown in a neutral one; the forms not being edited
                    // are shown a little darker, to tell them apart
                    let [r, g, b] = colour.unwrap_or([214, 208, 196]);
                    let shade = |c: u8| if *form == 0 { c } else { (c as f32 * 0.72) as u8 };
                    colours.push(Srgba::new_opaque(shade(r), shade(g), shade(b)));
                    owner.push(*form);
                }
                instances.colors = Some(colours);
                object.geometry.set_instances(&instances);
            }
            // The shadow map is redrawn only here, when the bricks change, not
            // every frame: turning the camera does not move a shadow.
            let casters = brick_meshes.iter().filter(|m| m.geometry.instance_count() > 0).map(|m| &m.geometry);
            if let Err(e) = sun.generate_shadow_map(2048, casters) {
                eprintln!("arcs editor: no shadows this time: {e}");
            }
        }
        let handles = state.handles();
        if handles != shown_handles || first {
            let mut instances = Instances::default();
            let mut colours = Vec::new();
            for (handle, at) in &handles {
                instances.transformations.push(Mat4::from_translation(v(*at)) * Mat4::from_scale(0.7));
                colours.push(match handle {
                    Handle::Move => Srgba::new_opaque(250, 250, 250),
                    Handle::Point(_) => Srgba::new_opaque(255, 220, 60),
                    Handle::Radius | Handle::Rise => Srgba::new_opaque(70, 230, 240),
                });
            }
            instances.colors = Some(colours);
            handle_mesh.geometry.set_instances(&instances);
            shown_handles = handles;
        }
        first = false;

        if asked.send {
            send::send(&state.painted(), state.status.clone());
        }
        #[cfg(not(target_arch = "wasm32"))]
        if asked.copy || copy_keyed {
            // said in a notice over the view as well as in the panel: the
            // keys give no other sign that anything happened
            let said = brz::copy(&state.painted(), &state.forms());
            state.alert = Some((if said.starts_with("Copied") { "Copied" } else { "Not copied" }, said.clone()));
            state.report_note = said;
        }
        if asked.report {
            state.report_note = match arcs_core::report::report(&state.full_spec()) {
                Ok(text) => send::save_report(&text),
                Err(e) => format!("No report: {e}"),
            };
        }

        let lights: [&dyn Light; 3] = [&ambient, &sun, &fill_light];
        let screen = frame.screen();
        screen.clear(ClearState::color_and_depth(0.17, 0.19, 0.23, 1.0, 1.0));
        screen.render(&camera, &ground, &lights);
        screen.render(&camera, brick_meshes.iter().filter(|m| m.geometry.instance_count() > 0), &lights);
        screen.render(&camera, &axes, &lights);
        screen.render(&camera, &handle_mesh, &lights);
        if let Err(e) = screen.write(|| gui.render()) {
            // the panel failing to draw should not take the view down with it
            eprintln!("arcs editor: could not draw the panel: {e}");
        }
        FrameOutput::default()
    });
}
