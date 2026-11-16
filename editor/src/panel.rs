//! The side panel: every setting of the form that is not a handle in the view.

use crate::paint::{Mode, MODES};
use crate::state::{Cut, Lock, State, ARCH, ARCHES, ARCH_POINTS, BEZIER, CIRCLE, FORMS, PYRAMID, REVOLVE, SPHERE, SPLINE};
use arcs_core::shapes::{shape_info, Kind};
use three_d::egui::{self, Color32, ComboBox, Slider, Ui};

/// Saved forms: a folder to choose, its forms to open, and a name to save under.
fn saved_forms(ui: &mut Ui, state: &mut State) {
    ui.label("Saved forms");
    let before = state.folder.clone();
    ComboBox::from_label("Folder").selected_text(state.folder.as_str()).show_ui(ui, |ui| {
        for folder in state.folders.clone() {
            ui.selectable_value(&mut state.folder, folder.clone(), folder);
        }
    });
    if state.folder != before {
        state.list_saved();
    }

    // The folder's forms. Each can be opened in place of the form being
    // edited, added to the workspace beside it, or dragged onto the view,
    // which adds it too.
    let mut open: Option<(String, bool)> = None;
    if state.forms.is_empty() {
        ui.label("Nothing saved here yet.");
    }
    for name in &state.forms {
        ui.horizontal(|ui| {
            let id = egui::Id::new(("arcs saved form", &state.folder, name));
            ui.dnd_drag_source(id, Dragged { folder: state.folder.clone(), name: name.clone() }, |ui| {
                ui.label(format!(":: {name}")).on_hover_text("Drag onto the view to add it to the workspace.");
            });
            if ui.small_button("Open").clicked() {
                open = Some((name.clone(), false));
            }
            if ui.small_button("Add").on_hover_text("Add to the workspace beside the form being edited.").clicked() {
                open = Some((name.clone(), true));
            }
        });
    }
    if let Some((name, add)) = open {
        let folder = state.folder.clone();
        open_saved(state, &folder, &name, add);
    }

    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(&mut state.save_name).hint_text("name").desired_width(110.0));
        if ui.button("Save").clicked() {
            let text = arcs_core::report::spec_text(&state.full_spec());
            state.store_note = crate::store::save(&state.folder, &state.save_name, &text).unwrap_or_else(|e| format!("Not saved: {e}"));
            state.list_saved();
        }
    });
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(&mut state.new_folder).hint_text("new folder").desired_width(110.0));
        if ui.button("Add folder").clicked() {
            match crate::store::add_folder(&state.new_folder) {
                Ok(folder) => {
                    state.folder = folder;
                    state.new_folder.clear();
                    state.store_note.clear();
                }
                Err(e) => state.store_note = format!("No folder added: {e}"),
            }
            state.list_saved();
        }
    });
    if !state.store_note.is_empty() {
        ui.label(state.store_note.as_str());
    }
}

/// What the player asked for with a button this frame.
#[derive(Default)]
pub struct Asked {
    pub report: bool,
    /// put the workspace on the clipboard as a .brz file
    pub copy: bool,
    /// search for the best fit near the current settings
    pub best: bool,
    /// turn the form a quarter turn about this axis
    pub turn: Option<usize>,
    /// put the form back on the world's grid
    pub level: bool,
}

/// Draw the panel.
pub fn show(ui: &mut Ui, state: &mut State) -> Asked {
    let mut asked = Asked::default();
    // something that went wrong and has to be read, over the middle of the view
    if let Some((title, alert)) = state.alert.clone() {
        egui::Window::new(title).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ui.ctx(), |ui| {
            ui.set_max_width(420.0);
            ui.label(alert);
            // Enter or Escape does as well as the button
            let keyed = ui.input(|i| i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Escape));
            if ui.button("OK").clicked() || keyed {
                state.alert = None;
            }
        });
    }
    // Something done that needs no answer, said at the top of the view for
    // a few seconds and then faded away. A notice that has to be closed
    // (above) is kept for what went wrong.
    if let Some((text, since)) = state.toast.as_mut() {
        const SHOWN: f64 = 2.5;
        const FADE: f64 = 0.8;
        let now = ui.input(|i| i.time);
        let age = now - *since.get_or_insert(now);
        if age > SHOWN + FADE {
            state.toast = None;
        } else {
            let text = text.clone();
            egui::Area::new(egui::Id::new("toast")).anchor(egui::Align2::CENTER_TOP, [0.0, 16.0]).interactable(false).show(ui.ctx(), |ui| {
                ui.set_opacity((1.0 - (age - SHOWN) / FADE).clamp(0.0, 1.0) as f32);
                egui::Frame::popup(ui.style()).show(ui, |ui| ui.label(text));
            });
        }
    }
    // how far a best-fit search has got, in the top right corner while it runs
    if let Some(done) = state.search_progress() {
        egui::Area::new(egui::Id::new("search progress")).anchor(egui::Align2::RIGHT_TOP, [-12.0, 12.0]).interactable(false).show(ui.ctx(), |ui| {
            ui.add(egui::ProgressBar::new(done).desired_width(180.0).text("Finding the best fit"));
        });
    }
    let panel = egui::Panel::left("settings").show_inside(ui, |ui| {
      egui::ScrollArea::vertical().show(ui, |ui| {
        ui.heading("Arcs");
        ui.label("Drag the coloured dots to reshape the form. Drag the form itself, or one of its arrows, to move it. Shift + click on a form adds a point there to pull it out of shape; on a circle it moves within the circle's plane, or out of it with Ctrl held.");
        ui.label("View: right-drag looks around (right over backwards if you like), middle-click turns to face what the cursor is on, W A S D fly, Space or E rises, Q sinks, Shift + scroll sets how fast.");
        ui.separator();

        let before = state.form;
        ComboBox::from_label("Form").selected_text(FORMS[state.form]).show_ui(ui, |ui| {
            for (i, name) in FORMS.iter().enumerate() {
                ui.selectable_value(&mut state.form, i, *name);
            }
        });
        if state.form != before {
            state.preset();
        }
        if ui.button("Reset this form").on_hover_text("Put the dots, sizes, stretch, flips and other choices back to how this form starts out. How it is fitted and coloured is left alone.").clicked() {
            state.reset();
        }
        ui.horizontal(|ui| {
            ui.label("Sliders go up to");
            ui.add(egui::DragValue::new(&mut state.max_size).range(20.0..=400.0).speed(1.0)).on_hover_text("How far the size sliders reach, in microbricks. Large forms take longer to fit.");
        });
        ui.horizontal(|ui| {
            ui.label("Longest brick");
            ui.add(egui::DragValue::new(&mut state.spec.max_ext).range(1..=4000).speed(1.0)).on_hover_text("The longest edge a single brick may have, in microbricks. 80 is what the game was seen to accept when this was first tried; the game's own limit is said to be 800 studs, which is 4000 microbricks. Raise it and paste to find out: a form then needs fewer, longer bricks.");
        });
        let largest = state.max_size;
        ui.separator();
        workspace(ui, state);
        ui.separator();
        colours(ui, state);
        ui.separator();

        let arch_feet = state.has_rise();
        let s = &mut state.spec;
        let is_curve = state.form <= BEZIER;
        match state.form {
            ARCH if arch_feet => {
                ComboBox::from_label("Arch").selected_text(ARCHES[state.arch].0).show_ui(ui, |ui| {
                    for (i, (name, _)) in ARCHES.iter().enumerate() {
                        ui.selectable_value(&mut state.arch, i, *name);
                    }
                });
                let mut natural = s.rise.is_none();
                ui.checkbox(&mut natural, "Natural height (drag the top dot to set your own)");
                if natural {
                    s.rise = None;
                }
            }
            ARCH => {
                if s.points.len() == 3 {
                    ui.label("The arc through its three points.");
                    ui.checkbox(&mut s.closed, "Whole circle");
                } else {
                    ui.label(format!("The curve through its {} points.", s.points.len()));
                }
            }
            SPLINE => {
                ui.checkbox(&mut s.closed, "Closed loop");
            }
            CIRCLE => {
                ComboBox::from_label("Facing").selected_text(["X", "Y", "Z (flat)"][s.plane.min(2)]).show_ui(ui, |ui| {
                    for (i, name) in ["X", "Y", "Z (flat)"].iter().enumerate() {
                        ui.selectable_value(&mut s.plane, i, *name);
                    }
                });
                // within its plane, into an ellipse; across it, the depth of a round tube
                ui.label("Stretch:");
                for (axis, name) in ["X", "Y", "Z"].iter().enumerate() {
                    let across = axis == s.plane.min(2);
                    ui.add_enabled(!across || s.tube, Slider::new(&mut s.stretch[axis], 0.25..=1000.0).logarithmic(true).text(if across { format!("{name} (the tube's depth)") } else { name.to_string() }));
                }
            }
            BEZIER => {
                ui.label("Pull of each point (higher draws the curve closer):");
                s.weights.resize(s.points.len(), 1.0);
                for (i, w) in s.weights.iter_mut().enumerate() {
                    ui.add(Slider::new(w, 0.1..=8.0).logarithmic(true).text(format!("point {}", i + 1)));
                }
            }
            SPHERE => {
                ui.label("Stretch:");
                for (axis, name) in ["X", "Y", "Z"].iter().enumerate() {
                    ui.add(Slider::new(&mut s.stretch[axis], 0.25..=1000.0).logarithmic(true).text(*name));
                }
                ComboBox::from_label("Keep").selected_text(["Whole", "Dome", "Segment"][state.cut as usize]).show_ui(ui, |ui| {
                    ui.selectable_value(&mut state.cut, Cut::Whole, "Whole");
                    ui.selectable_value(&mut state.cut, Cut::Dome, "Dome");
                    ui.selectable_value(&mut state.cut, Cut::Segment, "Segment");
                });
                if state.cut == Cut::Segment {
                    ui.add(Slider::new(&mut state.segment.0, -40.0..=40.0).step_by(0.5).text("from height"));
                    ui.add(Slider::new(&mut state.segment.1, -40.0..=40.0).step_by(0.5).text("to height"));
                }
            }
            _ => {}
        }
        if state.form == PYRAMID {
            ui.label("Stretch:");
            for (axis, name) in ["X", "Y"].iter().enumerate() {
                ui.add(Slider::new(&mut s.stretch[axis], 0.25..=1000.0).logarithmic(true).text(*name));
            }
            let mut height = s.rise.unwrap_or(s.radius);
            ui.add(Slider::new(&mut height, 1.0..=largest.max(80.0)).step_by(1.0).text("Height"));
            // left alone it is as tall as it is wide to one side
            if height != s.rise.unwrap_or(s.radius) {
                s.rise = Some(height);
            }
        }
        if matches!(state.form, CIRCLE | SPHERE | PYRAMID) {
            // A size beyond the slider's reach (set by Ctrl + scroll, or
            // before the reach was lowered) is left as it is: the slider
            // would otherwise put it back to its own top value every frame.
            let name = match state.form {
                PYRAMID => "Half its base",
                _ => "Radius",
            };
            ui.add(Slider::new(&mut s.radius, 0.5..=largest).step_by(0.5).clamping(egui::SliderClamping::Never).text(name));
        }

        ui.separator();
        // Fill means different things to a curve and to a solid, so say which
        match state.form {
            SPHERE => {
                ui.checkbox(&mut s.fill, "Solid (untick for a hollow shell)");
            }
            REVOLVE => {
                ui.checkbox(&mut s.fill, "Solid (untick for a shell along the profile)");
            }
            PYRAMID => {
                ui.checkbox(&mut s.fill, "Solid (untick for a hollow tent)");
            }
            _ => {
                ui.checkbox(&mut s.fill, "Fill the area the curve encloses");
                ui.checkbox(&mut s.tube, "Round tube instead of a flat band");
            }
        }
        if is_curve && s.tube {
            // how a round section is built: see `arcs_core::fit_stack`
            ui.horizontal(|ui| {
                ui.radio_value(&mut s.protrude, false, "Rounded").on_hover_text("A pipe: the edges chamfered round with sloped pieces.");
                ui.radio_value(&mut s.protrude, true, "Protruded").on_hover_text("Flat bands side by side, each standing proud of the next.");
            });
        }
        let wall = match state.form {
            SPHERE | REVOLVE | PYRAMID => "Wall thickness",
            _ => "Thickness",
        };
        // (a pyramid's wall is measured straight up, and only a hollow one has any)
        if !(state.form == PYRAMID && s.fill) {
            ui.add(Slider::new(&mut s.thick, 0.5..=16.0).step_by(0.5).text(wall));
        }
        if is_curve && !s.tube {
            ui.add(Slider::new(&mut s.depth, 1..=64).text("Depth"));
        }
        if !s.pulls.is_empty() {
            ui.add(Slider::new(&mut s.pull_reach, 1.0..=40.0).step_by(0.5).text("Reach of added points")).on_hover_text("How far from each added point the form goes with it: along a curve to either side, or all round on a sphere or pyramid.");
            if ui.button(format!("Remove the last added point ({})", s.pulls.len())).clicked() {
                s.pulls.pop();
            }
            // said where it applies, since the difference is large and not obvious
            let across = s.plane.min(2);
            if state.form == CIRCLE && s.pulls.iter().any(|(_, by)| by[across] != 0.0) {
                ui.label("A point pulled out of the circle's plane is rough where it was pulled: bricks cannot make a smooth round tube or band at a slant. Pulled within the plane it stays smooth. To tilt the whole circle, drag one of its rings.");
            }
        }
        ui.add(Slider::new(&mut state.follow, 0.0..=1.0).step_by(0.05).text("Neighbouring dots follow")).on_hover_text("How much of a dragged dot's movement the dots near it share: 0 moves it alone, 1 takes its nearest neighbours almost all the way with it. The share falls away with distance, out to three times the reach of added points.");
        ui.label("Sizes are in microbricks. Half steps are single game units.");
        if is_curve && s.tube && s.thick < 3.0 {
            ui.label("A round tube this thin is, in bricks, a square bar: there is no room for a rounder cross-section under 3 microbricks.");
        }
        ui.horizontal(|ui| {
            ui.label("Quarter turn").on_hover_text("Turn the form a quarter turn about an axis, within the brick grid. The form is the same bricks whichever way it faces. For any other angle, drag one of the red, green or blue rings round: that turns the form's whole grid.");
            for (axis, name) in ["X", "Y", "Z"].iter().enumerate() {
                if ui.button(*name).clicked() {
                    asked.turn = Some(axis);
                }
            }
        });
        if crate::state::on_own_grid(s) {
            ui.horizontal(|ui| {
                ui.label("On a grid of its own, at an angle.").on_hover_text("Bricks sit on a grid, and a grid other than the world's can stand at any angle. This form has been turned by its rings, so it is copied to the clipboard on a grid of its own. Grids pass through each other, so it is in no other form's way.");
                if ui.button("Level it").clicked() {
                    asked.level = true;
                }
            });
        }
        ui.horizontal(|ui| {
            ui.label("Mirror").on_hover_text("Make the form its own mirror image along an axis: its low side along that axis is kept, and the high side becomes its mirror image, about the middle of the form's dots. An arch with one foot higher than the other gets two feet alike; a point added to one side of a circle appears on the other too.");
            for (axis, name) in ["X", "Y", "Z"].iter().enumerate() {
                ui.checkbox(&mut s.mirror[axis], *name);
            }
        });
        ui.checkbox(&mut s.symmetric, "Tidy symmetric bricks").on_hover_text("Where the form is its own mirror image (side to side, front to back, top to bottom), make its bricks mirror images too: the low side of each such axis is kept and the high side rebuilt from it. Does nothing along an axis the form is not symmetric about.");
        ui.horizontal(|ui| {
            ui.label("Flip");
            for (axis, name) in ["X", "Y", "Z"].into_iter().enumerate() {
                ui.checkbox(&mut s.flip[axis], name).on_hover_text("Mirror the finished form along this axis, about the middle of its points.");
            }
        });

        if state.can_add_points() {
            ui.horizontal(|ui| {
                let full = state.form == ARCH && state.spec.points.len() >= ARCH_POINTS;
                if ui.add_enabled(!full, egui::Button::new("Add point")).clicked() {
                    state.add_point();
                }
                if ui.button("Remove point").clicked() {
                    state.remove_point();
                }
            });
        }

        ui.separator();
        let lock_name = |l: Lock| match l {
            Lock::Free => "Facing the camera",
            Lock::Axis(0) => "Keep X (move in Y and Z)",
            Lock::Axis(1) => "Keep Y (move in X and Z)",
            Lock::Axis(_) => "Keep Z (move in X and Y)",
        };
        ComboBox::from_label("Dots move").selected_text(lock_name(state.lock)).show_ui(ui, |ui| {
            for l in [Lock::Free, Lock::Axis(0), Lock::Axis(1), Lock::Axis(2)] {
                ui.selectable_value(&mut state.lock, l, lock_name(l));
            }
        });
        ui.checkbox(&mut state.show_ruler, "Show rulers (lengths in microbricks)");
        ui.add(Slider::new(&mut state.floor_gap, 0.0..=40.0).step_by(1.0).text("Floor below form"))
            .on_hover_text("Lowers the floor in the view so you can look at a form from underneath. The floor is also see-through from below. It changes nothing that is built.");
        ui.checkbox(&mut state.fine_snap, "Snap dots to half microbricks");
        asked.best = ui
            .button("Snap to best fit nearby")
            .on_hover_text("A form fits cleanly only where its surfaces land on the brick grid. This tries each dot and size up to a microbrick either way and keeps whatever fits closest.")
            .clicked();
        ui.checkbox(&mut state.auto_best, "Do that after every drag");
        if !state.best_note.is_empty() {
            ui.label(state.best_note.as_str());
        }

        ui.separator();
        let s = &mut state.spec;
        ui.label("Detail");
        let size_name = |g: i32| if g == 0 { "Automatic".to_string() } else { format!("{g} microbrick{}", if g == 1 { "" } else { "s" }) };
        ComboBox::from_label("Brick size").selected_text(size_name(s.grain)).show_ui(ui, |ui| {
            ui.selectable_value(&mut s.grain, 0, size_name(0));
            for g in arcs_core::spec::GRAINS {
                ui.selectable_value(&mut s.grain, g, size_name(g));
            }
        });
        if s.grain == 0 {
            ui.add(Slider::new(&mut s.budget, 100..=30000).logarithmic(true).text("Brick budget"))
                .on_hover_text("The smallest brick size that keeps the build within this many bricks is used.");
        }
        if is_curve && !s.tube {
            ui.add(Slider::new(&mut s.simplify, 0.0..=1.5).text("Simplify"))
                .on_hover_text("How much closer a ramp must bring the outline to be worth a brick, in microbricks of area. 0 is the closest fit whatever the count; higher gives fewer, longer pieces. Flat forms only: on a 3D form, use Brick size.");
        }
        ui.checkbox(&mut s.wedges, "Use ramps and corner pieces");

        ui.separator();
        let built = &state.built;
        let count = |kind: Kind| built.bricks.iter().filter(|b| shape_info(b.mask).is_some_and(|s| s.kind == kind)).count();
        ui.label(format!("{} bricks: {} cubes, {} ramps, {} tetrahedra, {} cut cubes, {} hips, {} inner corners, {} half corners", built.bricks.len(), count(Kind::Cube), count(Kind::Ramp), count(Kind::Tet), count(Kind::Cut), count(Kind::Hip), count(Kind::Inner), count(Kind::HalfOuter) + count(Kind::HalfInner) + count(Kind::HalfInnerInv)));
        if built.grain > 1 {
            ui.label(format!("Brick size {} microbricks", built.grain));
        }
        if state.spec.grain == 0 && built.bricks.len() > state.spec.budget {
            ui.label("Over budget: larger bricks would be too coarse for the form's thinnest part. Thicken it, shrink it, or raise Simplify.");
        }
        if built.solid > 0.0 {
            ui.label(format!("Discrepancy: {:.2}% of the form's volume ({:.1} of {:.0} microbricks)", 100.0 * built.error / built.solid, built.error, built.solid))
                .on_hover_text("The volume where the bricks and the true form disagree: missing material plus extra material.");
        }
        ui.label(format!("Fitted in {:.0} ms", state.build_ms));
        if let Some(problem) = &state.problem {
            ui.colored_label(Color32::from_rgb(255, 120, 100), format!("Not built: {problem}"));
        }

        ui.separator();
        ui.separator();
        saved_forms(ui, state);

        ui.separator();
        asked.report = ui
            .button("Save report")
            .on_hover_text("Writes out this form's settings, its bricks, and a cell-by-cell comparison of the bricks with the form: where material is missing or extra. Useful for working out why a fit looks wrong.")
            .clicked();
        // a web page cannot put a file on the clipboard: see `brz`
        if cfg!(not(target_arch = "wasm32")) {
            asked.copy = ui
                .add_enabled(state.problem.is_none() && !state.built.bricks.is_empty(), egui::Button::new("Copy to Clipboard"))
                .on_hover_text("Writes everything in the workspace to arcs-build.brz, a Brickadia prefab file, and puts that file on the clipboard. Ctrl+C does the same, and Ctrl+V adds the forms of a copied file back to the workspace.")
                .clicked();
        }
        if !state.report_note.is_empty() {
            ui.label(state.report_note.as_str());
        }
      });
    });
    state.panel_right = panel.response.rect.right();
    dropped_form(ui, state);
    asked
}

/// The workspace's forms: which is being edited, and adding and removing them.
fn workspace(ui: &mut Ui, state: &mut State) {
    ui.label("Workspace");
    ui.label(format!("> {} (editing)", FORMS[state.form]));
    let mut switch = None;
    for (k, item) in state.others.iter().enumerate() {
        if ui.selectable_label(false, format!("   {}", item.name())).on_hover_text("Edit this form. Clicking it in the view does the same.").clicked() {
            switch = Some(k);
        }
    }
    if let Some(k) = switch {
        state.activate(k);
    }
    ui.horizontal(|ui| {
        if ui.button("Add a copy").on_hover_text("Another of this form, set down beside it.").clicked() {
            state.add_form(state.full_spec());
        }
        if ui.add_enabled(!state.others.is_empty(), egui::Button::new("Remove")).on_hover_text("Take the form being edited out of the workspace.").clicked() {
            state.remove_active();
        }
    });
}

/// How the form being edited is coloured.
fn colours(ui: &mut Ui, state: &mut State) {
    let before = state.paint.clone();
    let paint = &mut state.paint;
    let name = MODES.iter().find(|m| m.0 == paint.mode).map_or("", |m| m.1);
    ComboBox::from_label("Colour").selected_text(name).show_ui(ui, |ui| {
        for (mode, name) in MODES {
            ui.selectable_value(&mut paint.mode, mode, name);
        }
    });
    {
        // one colour for a solid; up to four to blend, stripe or speckle between
        let most = if paint.mode == Mode::Solid { 1 } else { 4 };
        ui.horizontal(|ui| {
            for colour in paint.colours.iter_mut().take(most) {
                ui.color_edit_button_srgb(colour);
            }
            if most > 1 && paint.colours.len() < most && ui.small_button("+").on_hover_text("Another colour").clicked() {
                paint.colours.push([230, 230, 230]);
            }
            if most > 1 && paint.colours.len() > 2 && ui.small_button("-").on_hover_text("One colour fewer").clicked() {
                paint.colours.pop();
            }
        });
        if most > 1 {
            ComboBox::from_label("Runs along").selected_text(["X", "Y", "Z (up)"][paint.axis.min(2)]).show_ui(ui, |ui| {
                for (i, axis) in ["X", "Y", "Z (up)"].iter().enumerate() {
                    ui.selectable_value(&mut paint.axis, i, *axis);
                }
            });
        }
        if paint.mode == Mode::Stripes {
            ui.add(Slider::new(&mut paint.width, 1..=24).text("Stripe width"));
        }
        ui.label("Each brick is one colour.");
}
    if state.paint != before {
        state.scene += 1;
    }
}

/// A saved form being dragged from the list.
#[derive(Clone)]
struct Dragged {
    folder: String,
    name: String,
}

/// Open a saved form: into the workspace as one more form, or in place of
/// the one being edited.
fn open_saved(state: &mut State, folder: &str, name: &str, add: bool) {
    let loaded = crate::store::load(folder, name).and_then(|text| arcs_core::report::parse_spec(&text));
    state.store_note = match loaded {
        Ok(spec) if add => {
            state.add_form(spec);
            format!("Added {name} to the workspace")
        }
        Ok(spec) => {
            state.load_spec(spec);
            state.save_name = name.to_string();
            format!("Opened {name}")
        }
        Err(e) => format!("Could not open {name}: {e}"),
    };
}

/// If a saved form was let go of over the view this frame, add it to the workspace.
fn dropped_form(ui: &mut Ui, state: &mut State) {
    let (released, pointer) = ui.input(|i| (i.pointer.any_released(), i.pointer.interact_pos()));
    if !released {
        return;
    }
    // taken whether or not it lands on the view, so that it is not left
    // hanging on the pointer
    if let Some(dragged) = egui::DragAndDrop::take_payload::<Dragged>(ui.ctx()) {
        if pointer.is_some_and(|p| p.x > state.panel_right) {
            open_saved(state, &dragged.folder, &dragged.name, true);
        }
    }
}
