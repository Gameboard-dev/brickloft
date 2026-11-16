//! The side panel: every setting of the form that is not a handle in the view.

use crate::paint::{Mode, MODES};
use crate::state::{Cut, Lock, State, ARCH, ARCHES, ARCH_POINTS, BEZIER, CIRCLE, FORMS, PYRAMID, REVOLVE, SPHERE, SPLINE};
use arcs_core::shapes::{shape_info, Kind};
use three_d::egui::{self, Color32, ComboBox, Slider, Ui};

/// A small "?" that explains the control beside it when the pointer rests
/// on it.
///
/// Every control also explains itself when the pointer rests on the control
/// itself, which egui does for any widget. That alone left nothing on the
/// panel to say that there was anything to read, so each control has a mark
/// beside it as well.
/// https://docs.rs/egui/latest/egui/struct.Response.html#method.on_hover_text
fn hint(ui: &mut Ui, text: &str) {
    ui.add(egui::Label::new(egui::RichText::new("?").weak()).sense(egui::Sense::hover())).on_hover_text(text);
}

/// A control with its explanation: said when the pointer rests on the
/// control, and on the "?" set after it (see [`hint`]).
fn helped(ui: &mut Ui, text: &str, add: impl FnOnce(&mut Ui) -> egui::Response) -> egui::Response {
    ui.horizontal(|ui| {
        let response = add(ui).on_hover_text(text);
        hint(ui, text);
        response
    })
    .inner
}

/// Saved forms: a folder to choose, its forms to open, and a name to save under.
fn saved_forms(ui: &mut Ui, state: &mut State) {
    ui.label("Saved forms").on_hover_text("A save holds the whole workspace: every form, with its colours and the grid it is on.");
    let before = state.folder.clone();
    helped(ui, "The folder of saved forms shown below. Add folder makes a new one.", |ui| {
        ComboBox::from_label("Folder").selected_text(state.folder.as_str()).show_ui(ui, |ui| {
            for folder in state.folders.clone() {
                ui.selectable_value(&mut state.folder, folder.clone(), folder);
            }
        })
            .response
    });
    if state.folder != before {
        state.list_saved();
    }

    // The folder's forms. Each can be opened in place of the form being
    // edited, added to the workspace beside it, or dragged onto the view,
    // which adds it too.
    let mut open: Option<(String, bool)> = None;
    let mut delete: Option<String> = None;
    if state.forms.is_empty() {
        ui.label("Nothing saved here yet.");
    }
    for name in &state.forms {
        ui.horizontal(|ui| {
            let id = egui::Id::new(("arcs saved form", &state.folder, name));
            ui.dnd_drag_source(id, Dragged { folder: state.folder.clone(), name: name.clone() }, |ui| {
                ui.label(format!(":: {name}")).on_hover_text("Drag onto the view to add it to the workspace.");
            });
            if ui.small_button("Open").on_hover_text("Open in place of everything in the workspace. Ctrl+Z brings the workspace back.").clicked() {
                open = Some((name.clone(), false));
            }
            if ui.small_button("Add").on_hover_text("Add its forms to the workspace, beside the form being edited and on its grid.").clicked() {
                open = Some((name.clone(), true));
            }
            if ui.small_button("Delete").on_hover_text("Delete this saved form. You are asked first.").clicked() {
                delete = Some(name.clone());
            }
        });
    }
    if let Some(name) = delete {
        state.deleting = Some((state.folder.clone(), name));
    }
    if let Some((name, add)) = open {
        let folder = state.folder.clone();
        open_saved(state, &folder, &name, add);
    }

    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(&mut state.save_name).hint_text("name").desired_width(110.0));
        if helped(ui, "Saves the whole workspace under this name in the chosen folder: every form, with its colours and the grid it is on. A save of the same name is replaced.", |ui| ui.button("Save")).clicked() {
            state.store_note = crate::store::save(&state.folder, &state.save_name, &state.painted(), &state.forms()).unwrap_or_else(|e| format!("Not saved: {e}"));
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
    /// delete every saved report
    pub clear_reports: bool,
    /// save a picture of the window
    pub screenshot: bool,
    /// put the workspace on the clipboard as a .brz file
    pub copy: bool,
    /// search for the best fit near the current settings
    pub best: bool,
    /// turn the form a quarter turn about this axis
    pub turn: Option<usize>,
    /// put the form back on the world's grid
    pub level: bool,
}

const SLIDERS: &str = "How far every size slider reaches, in microbricks: radius, height, thickness, depth and the reach of added points. Lower it for a finer slider. Large forms take longer to fit.";
const LONGEST: &str = "The longest edge a single brick may have, in microbricks. 80 is what the game was seen to accept when this was first tried; the game's own limit is said to be 800 studs, which is 4000 microbricks. Raise it and paste to find out: a form then needs fewer, longer bricks.";
const QUARTER: &str = "Turn the form a quarter turn about an axis, within the brick grid. The form is the same bricks whichever way it faces. For any other angle, drag one of the red, green or blue rings round: that turns the form's whole grid.";
const MIRROR: &str = "Make the form its own mirror image along an axis: its low side along that axis is kept, and the high side becomes its mirror image, about the middle of the form's dots. An arch with one foot higher than the other gets two feet alike; a point added to one side of a circle appears on the other too.";
const FLIP: &str = "Mirror the finished form along an axis, about the middle of its points.";

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
    // A saved form about to be deleted. A file once deleted is not brought
    // back by Ctrl+Z, so it is asked about first, and only the button says
    // yes: Enter does not, so that a key pressed out of habit to close a
    // notice cannot delete anything. Escape says no.
    if let Some((folder, name)) = state.deleting.clone() {
        egui::Window::new("Delete saved form").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ui.ctx(), |ui| {
            ui.set_max_width(420.0);
            ui.label(format!("Delete \"{name}\" from the folder \"{folder}\"? It cannot be brought back."));
            ui.horizontal(|ui| {
                if ui.button("Delete").clicked() {
                    state.store_note = match crate::store::delete(&folder, &name) {
                        Ok(()) => format!("Deleted {name}"),
                        Err(e) => format!("Not deleted: {e}"),
                    };
                    state.deleting = None;
                    state.list_saved();
                }
                if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    state.deleting = None;
                }
            });
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
        helped(ui, "The kind of form to build. Choosing one starts it at its usual size.", |ui| {
            ComboBox::from_label("Form").selected_text(FORMS[state.form]).show_ui(ui, |ui| {
                for (i, name) in FORMS.iter().enumerate() {
                    ui.selectable_value(&mut state.form, i, *name);
                }
            })
            .response
        });
        if state.form != before {
            state.preset();
        }
        if helped(ui, "Put the dots, sizes, stretch, flips and other choices back to how this form starts out. How it is fitted and coloured is left alone.", |ui| ui.button("Reset this form")).clicked() {
            state.reset();
        }
        ui.horizontal(|ui| {
            ui.label("Sliders go up to");
            ui.add(egui::DragValue::new(&mut state.max_size).range(20.0..=arcs_core::spec::MAX_SPAN).speed(1.0)).on_hover_text(SLIDERS);
            hint(ui, SLIDERS);
        });
        ui.horizontal(|ui| {
            ui.label("Longest brick");
            ui.add(egui::DragValue::new(&mut state.spec.max_ext).range(1..=4000).speed(1.0)).on_hover_text(LONGEST);
            hint(ui, LONGEST);
        });
        // One limit for every slider that sets a size. Each had its own (a
        // depth to 64 beside a radius to 60), so the limit shown was not the
        // limit met. The sliders are logarithmic, since a slider that reaches
        // 1000 in even steps cannot be set to 3.
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
                helped(ui, "The shape of the arch between its two feet.", |ui| {
                    ComboBox::from_label("Arch").selected_text(ARCHES[state.arch].0).show_ui(ui, |ui| {
                        for (i, (name, _)) in ARCHES.iter().enumerate() {
                            ui.selectable_value(&mut state.arch, i, *name);
                        }
                    })
                        .response
                });
                let mut natural = s.rise.is_none();
                helped(ui, "Ticked, the arch takes the height its shape gives it: a round arch is a semicircle. Dragging the top dot sets your own height.", |ui| ui.checkbox(&mut natural, "Natural height (drag the top dot to set your own)"));
                if natural {
                    s.rise = None;
                }
            }
            ARCH => {
                if s.points.len() == 3 {
                    ui.label("The arc through its three points.");
                    helped(ui, "Builds the whole circle through the three points, where unticked it is the arc from the first point to the last.", |ui| ui.checkbox(&mut s.closed, "Whole circle"));
                } else {
                    ui.label(format!("The curve through its {} points.", s.points.len()));
                }
            }
            SPLINE => {
                helped(ui, "Joins the last point back to the first.", |ui| ui.checkbox(&mut s.closed, "Closed loop"));
            }
            CIRCLE => {
                helped(ui, "The axis the circle faces along. Z lays it flat.", |ui| {
                    ComboBox::from_label("Facing").selected_text(["X", "Y", "Z (flat)"][s.plane.min(2)]).show_ui(ui, |ui| {
                        for (i, name) in ["X", "Y", "Z (flat)"].iter().enumerate() {
                            ui.selectable_value(&mut s.plane, i, *name);
                        }
                    })
                        .response
                });
                // within its plane, into an ellipse; across it, the depth of a round tube
                ui.label("Stretch:");
                for (axis, name) in ["X", "Y", "Z"].iter().enumerate() {
                    let across = axis == s.plane.min(2);
                    helped(ui, "Stretches the form along this axis: 2 makes it twice as long that way, 0.5 half as long. Across the circle's plane it sets how deep a round tube is.", |ui| ui.add_enabled(!across || s.tube, Slider::new(&mut s.stretch[axis], 0.25..=1000.0).logarithmic(true).text(if across { format!("{name} (the tube's depth)") } else { name.to_string() })));
                }
            }
            BEZIER => {
                ui.label("Pull of each point (higher draws the curve closer):");
                s.weights.resize(s.points.len(), 1.0);
                for (i, w) in s.weights.iter_mut().enumerate() {
                    helped(ui, "How strongly this point draws the curve towards itself. 1 is neutral.", |ui| ui.add(Slider::new(w, 0.1..=8.0).logarithmic(true).text(format!("point {}", i + 1))));
                }
            }
            SPHERE => {
                ui.label("Stretch:");
                for (axis, name) in ["X", "Y", "Z"].iter().enumerate() {
                    ui.add(Slider::new(&mut s.stretch[axis], 0.25..=1000.0).logarithmic(true).text(*name));
                }
                helped(ui, "The whole sphere, a dome (its top half), or a segment between two heights.", |ui| {
                    ComboBox::from_label("Keep").selected_text(["Whole", "Dome", "Segment"][state.cut as usize]).show_ui(ui, |ui| {
                        ui.selectable_value(&mut state.cut, Cut::Whole, "Whole");
                        ui.selectable_value(&mut state.cut, Cut::Dome, "Dome");
                        ui.selectable_value(&mut state.cut, Cut::Segment, "Segment");
                    })
                        .response
                });
                if state.cut == Cut::Segment {
                    helped(ui, "Where the segment starts, measured up from the sphere's centre.", |ui| ui.add(Slider::new(&mut state.segment.0, -largest..=largest).step_by(0.5).text("from height")));
                    helped(ui, "Where the segment ends, measured up from the sphere's centre.", |ui| ui.add(Slider::new(&mut state.segment.1, -largest..=largest).step_by(0.5).text("to height")));
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
            helped(ui, "The pyramid's height. Left alone it is as tall as half its base is wide.", |ui| ui.add(Slider::new(&mut height, 1.0..=largest).logarithmic(true).step_by(1.0).text("Height")));
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
            helped(ui, "The form's size from its middle to its edge, in microbricks. Ctrl + scroll over the view changes it too.", |ui| ui.add(Slider::new(&mut s.radius, 0.5..=largest).logarithmic(true).step_by(0.5).clamping(egui::SliderClamping::Never).text(name)));
        }

        ui.separator();
        // Fill means different things to a curve and to a solid, so say which
        match state.form {
            SPHERE => {
                helped(ui, "Solid all through, or a hollow shell as thick as Wall thickness.", |ui| ui.checkbox(&mut s.fill, "Solid (untick for a hollow shell)"));
            }
            REVOLVE => {
                helped(ui, "Solid all through, or a shell along the profile as thick as Wall thickness: a vase or a bowl.", |ui| ui.checkbox(&mut s.fill, "Solid (untick for a shell along the profile)"));
            }
            PYRAMID => {
                helped(ui, "Solid all through, or a hollow tent open underneath.", |ui| ui.checkbox(&mut s.fill, "Solid (untick for a hollow tent)"));
            }
            _ => {
                helped(ui, "Fills everything the curve encloses: a ring becomes a disc, an arch a filled arch.", |ui| ui.checkbox(&mut s.fill, "Fill the area the curve encloses"));
                helped(ui, "Builds the curve as a round tube, which for a circle is a torus. Unticked it is a flat band with a thickness and a depth.", |ui| ui.checkbox(&mut s.tube, "Round tube instead of a flat band"));
            }
        }
        if is_curve && s.tube {
            // how a round section is built: see `arcs_core::fit_stack`
            ui.horizontal(|ui| {
                ui.radio_value(&mut s.protrude, false, "Rounded").on_hover_text("A pipe: the edges chamfered round with sloped pieces.");
                ui.radio_value(&mut s.protrude, true, "Protruded").on_hover_text("Flat bands side by side, each standing proud of the next.");
                hint(ui, "How a round tube's section is made. Rounded is a pipe with chamfered edges. Protruded is flat bands of different widths side by side: smooth along the curve and stepped across it.");
            });
        }
        let wall = match state.form {
            SPHERE | REVOLVE | PYRAMID => "Wall thickness",
            _ => "Thickness",
        };
        // (a pyramid's wall is measured straight up, and only a hollow one has any)
        if !(state.form == PYRAMID && s.fill) {
            helped(ui, "How thick the band, the tube or the wall is, in microbricks.", |ui| ui.add(Slider::new(&mut s.thick, 0.5..=largest).logarithmic(true).step_by(0.5).text(wall)));
        }
        if is_curve && !s.tube {
            helped(ui, "How deep a flat band is, measured across the plane its curve lies in.", |ui| ui.add(Slider::new(&mut s.depth, 1..=largest as i32).logarithmic(true).text("Depth")));
        }
        if !s.pulls.is_empty() {
            helped(ui, "How far from each added point the form goes with it: along a curve to either side, or all round on a sphere or pyramid.", |ui| ui.add(Slider::new(&mut s.pull_reach, 1.0..=largest).logarithmic(true).step_by(0.5).text("Reach of added points")));
            if ui.button(format!("Remove the last added point ({})", s.pulls.len())).clicked() {
                s.pulls.pop();
            }
            // said where it applies, since the difference is large and not obvious
            let across = s.plane.min(2);
            if state.form == CIRCLE && s.pulls.iter().any(|(_, by)| by[across] != 0.0) {
                ui.label("A point pulled out of the circle's plane is built of sloped pieces where it was pulled, and is less smooth there than one pulled within the plane, more so the steeper it climbs. To tilt the whole circle, drag one of its rings.");
            }
        }
        helped(ui, "How much of a dragged dot's movement the dots near it share: 0 moves it alone, 1 takes its nearest neighbours almost all the way with it. The share falls away with distance, out to three times the reach of added points.", |ui| ui.add(Slider::new(&mut state.follow, 0.0..=1.0).step_by(0.05).text("Neighbouring dots follow")));
        ui.label("Sizes are in microbricks. Half steps are single game units.");
        if is_curve && s.tube && s.thick < 3.0 {
            ui.label("A round tube this thin is, in bricks, a square bar: there is no room for a rounder cross-section under 3 microbricks.");
        }
        ui.horizontal(|ui| {
            ui.label("Quarter turn").on_hover_text(QUARTER);
            for (axis, name) in ["X", "Y", "Z"].iter().enumerate() {
                if ui.button(*name).clicked() {
                    asked.turn = Some(axis);
                }
            }
            hint(ui, QUARTER);
        });
        if crate::state::on_own_grid(s) {
            ui.horizontal(|ui| {
                ui.label("Its grid is at an angle.").on_hover_text("Bricks sit on a grid, and a grid other than the world's can stand at any angle. This grid has been turned by its rings, so everything on it is copied to the clipboard on a grid of its own. Grids pass through each other, so it is in no other grid's way.");
                if ui.button("Level the grid").clicked() {
                    asked.level = true;
                }
            });
        }
        ui.horizontal(|ui| {
            ui.label("Mirror").on_hover_text(MIRROR);
            for (axis, name) in ["X", "Y", "Z"].iter().enumerate() {
                ui.checkbox(&mut s.mirror[axis], *name);
            }
            hint(ui, MIRROR);
        });
        helped(ui, "Where the form is its own mirror image (side to side, front to back, top to bottom), make its bricks mirror images too: the low side of each such axis is kept and the high side rebuilt from it. Does nothing along an axis the form is not symmetric about.", |ui| ui.checkbox(&mut s.symmetric, "Tidy symmetric bricks"));
        ui.horizontal(|ui| {
            ui.label("Flip");
            for (axis, name) in ["X", "Y", "Z"].into_iter().enumerate() {
                ui.checkbox(&mut s.flip[axis], name).on_hover_text(FLIP);
            }
            hint(ui, FLIP);
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
                hint(ui, "Adds or removes one of the form's own dots. To pull a form out of shape at one place, Shift + click on it: that adds an orange point, and Shift + right-click takes it off.");
            });
        }

        ui.separator();
        let lock_name = |l: Lock| match l {
            Lock::Free => "Facing the camera",
            Lock::Axis(0) => "Keep X (move in Y and Z)",
            Lock::Axis(1) => "Keep Y (move in X and Z)",
            Lock::Axis(_) => "Keep Z (move in X and Y)",
        };
        helped(ui, "Which way a dragged dot moves: in the plane facing the camera, or with one of its coordinates kept as it is.", |ui| {
            ComboBox::from_label("Dots move").selected_text(lock_name(state.lock)).show_ui(ui, |ui| {
                for l in [Lock::Free, Lock::Axis(0), Lock::Axis(1), Lock::Axis(2)] {
                    ui.selectable_value(&mut state.lock, l, lock_name(l));
                }
            })
            .response
        });
        helped(ui, "Draws the form's size along each axis, in microbricks.", |ui| ui.checkbox(&mut state.show_ruler, "Show rulers (lengths in microbricks)"));
        helped(ui, "Lowers the floor in the view so you can look at a form from underneath. The floor is also see-through from below. It changes nothing that is built.", |ui| ui.add(Slider::new(&mut state.floor_gap, 0.0..=40.0).step_by(1.0).text("Floor below form")));
        helped(ui, "Dots snap to half microbricks, which are single game units. Unticked they snap to whole microbricks.", |ui| ui.checkbox(&mut state.fine_snap, "Snap dots to half microbricks"));
        asked.best = helped(ui, "A form fits cleanly only where its surfaces land on the brick grid. This tries each dot and size up to a microbrick either way and keeps whatever fits closest.", |ui| ui.button("Snap to best fit nearby"))
            .clicked();
        helped(ui, "Runs Snap to best fit nearby by itself each time a drag ends.", |ui| ui.checkbox(&mut state.auto_best, "Do that after every drag"));
        if !state.best_note.is_empty() {
            ui.label(state.best_note.as_str());
        }

        ui.separator();
        let s = &mut state.spec;
        ui.label("Detail");
        let size_name = |g: i32| if g == 0 { "Automatic".to_string() } else { format!("{g} microbrick{}", if g == 1 { "" } else { "s" }) };
        helped(ui, "Fits the form with bricks a multiple of this size. Automatic picks the finest size that keeps the build within the brick budget.", |ui| {
            ComboBox::from_label("Brick size").selected_text(size_name(s.grain)).show_ui(ui, |ui| {
                ui.selectable_value(&mut s.grain, 0, size_name(0));
                for g in arcs_core::spec::GRAINS {
                    ui.selectable_value(&mut s.grain, g, size_name(g));
                }
            })
            .response
        });
        if s.grain == 0 {
            helped(ui, "The smallest brick size that keeps the build within this many bricks is used.", |ui| ui.add(Slider::new(&mut s.budget, 100..=30000).logarithmic(true).text("Brick budget")));
        }
        if is_curve && !s.tube {
            helped(ui, "How much closer a ramp must bring the outline to be worth a brick, in microbricks of area. 0 is the closest fit whatever the count; higher gives fewer, longer pieces. Flat forms only: on a 3D form, use Brick size.", |ui| ui.add(Slider::new(&mut s.simplify, 0.0..=1.5).text("Simplify")));
        }
        helped(ui, "Unticked, the form is built of cubes alone, as a staircase.", |ui| ui.checkbox(&mut s.wedges, "Use ramps and corner pieces"));

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
        ui.horizontal(|ui| {
            asked.report = helped(ui, "Writes out this form's settings, its bricks, and a cell-by-cell comparison of the bricks with the form: where material is missing or extra. Useful for working out why a fit looks wrong. Reports go in the folder omegga-arcs in the system's temporary folder.", |ui| ui.button("Save report"))
                .clicked();
            // a page's reports go to the clipboard: there are no files to clear
            if cfg!(not(target_arch = "wasm32")) {
                asked.clear_reports = helped(ui, "Deletes every saved report file. Nothing else is touched.", |ui| ui.button("Clear reports")).clicked();
            }
        });
        // a web page cannot put a file on the clipboard (see `brz`), nor
        // write one to disk
        if cfg!(not(target_arch = "wasm32")) {
            asked.screenshot = helped(ui, "Saves a picture of the window in Pictures/omegga-arcs and puts it on the clipboard. F12 does the same, and Shift+F12 leaves this panel out of the picture.", |ui| ui.button("Save screenshot"))
                .clicked();
            asked.copy = helped(ui, "Writes everything in the workspace to arcs-build.brz, a Brickadia prefab file, and puts that file on the clipboard. Ctrl+C does the same, and Ctrl+V adds the forms of a copied file back to the workspace.", |ui| ui.add_enabled(state.problem.is_none() && !state.built.bricks.is_empty(), egui::Button::new("Copy to Clipboard")))
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
    ui.label("Workspace").on_hover_text("Forms stand on grids. Forms on one grid are kept clear of each other, and are turned and moved together by the rings and arrows. Grids pass through each other.");
    let tilted = |spec: &arcs_core::spec::Spec| if crate::state::on_own_grid(spec) { ", at an angle" } else { "" };
    ui.label(format!("> {} (editing) on grid {}{}", FORMS[state.form], state.spec.on_grid + 1, tilted(&state.spec)));
    let mut switch = None;
    for (k, item) in state.others.iter().enumerate() {
        let label = format!(" {} {} on grid {}{}", if item.picked { "+" } else { " " }, item.name(), item.spec.on_grid + 1, tilted(&item.spec));
        if ui.selectable_label(false, label).on_hover_text("Edit this form, and its grid. Clicking it in the view does the same.").clicked() {
            switch = Some(k);
        }
    }
    if let Some(k) = switch {
        // with Ctrl, picked to be copied along with the one being edited
        if ui.input(|i| i.modifiers.ctrl || i.modifiers.command) {
            state.toggle_pick(k);
        } else {
            state.activate(k);
        }
    }
    ui.label("Ctrl+C copies the form being edited and any marked +. Ctrl + click a form to mark it, Ctrl+A for all.");
    ui.horizontal(|ui| {
        if ui.button("Add to this grid").on_hover_text("Another of this form on the same grid, set down beside it. The two are kept clear of each other and turn together.").clicked() {
            state.add_copy();
        }
        if ui.button("New grid").on_hover_text("Another of this form on a grid of its own, which its rings turn to any angle and its arrows move, apart from every other grid.").clicked() {
            state.add_grid();
        }
        if ui.add_enabled(!state.others.is_empty(), egui::Button::new("Remove")).on_hover_text("Take the form being edited out of the workspace.").clicked() {
            state.remove_active();
        }
        hint(ui, "Add to this grid sets another of this form beside it on the same grid: the two are kept clear of each other and turn together. New grid gives the new form a grid of its own, which its rings turn to any angle. Remove takes the form being edited out of the workspace.");
    });
}

/// How the form being edited is coloured.
fn colours(ui: &mut Ui, state: &mut State) {
    let before = state.paint.clone();
    let paint = &mut state.paint;
    let name = MODES.iter().find(|m| m.0 == paint.mode).map_or("", |m| m.1);
    helped(ui, "How the form is coloured: one colour, a blend from one colour to another, stripes, or a speckled blend. Each brick is one colour.", |ui| {
        ComboBox::from_label("Colour").selected_text(name).show_ui(ui, |ui| {
            for (mode, name) in MODES {
                ui.selectable_value(&mut paint.mode, mode, name);
            }
        })
            .response
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
            hint(ui, "The colours used. Click one to change it. A blend, stripes or a speckle take up to four, added with + and taken away with -.");
        });
        if most > 1 {
            helped(ui, "The direction a blend or stripes run in across the form.", |ui| {
                ComboBox::from_label("Runs along").selected_text(["X", "Y", "Z (up)"][paint.axis.min(2)]).show_ui(ui, |ui| {
                    for (i, axis) in ["X", "Y", "Z (up)"].iter().enumerate() {
                        ui.selectable_value(&mut paint.axis, i, *axis);
                    }
                })
                    .response
            });
        }
        if paint.mode == Mode::Stripes {
            helped(ui, "How wide each stripe is, in microbricks.", |ui| ui.add(Slider::new(&mut paint.width, 1..=24).text("Stripe width")));
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

/// Open a save: its forms added to the workspace, as a paste adds them, or
/// put in place of the whole workspace.
fn open_saved(state: &mut State, folder: &str, name: &str, add: bool) {
    state.store_note = match crate::store::load(folder, name) {
        Ok(forms) if add => {
            state.record();
            state.paste(forms);
            format!("Added {name} to the workspace")
        }
        Ok(forms) => {
            state.open(forms);
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
