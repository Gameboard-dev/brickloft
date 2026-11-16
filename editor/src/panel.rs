//! The side panel: every setting of the form that is not a handle in the view.

use crate::paint::{Mode, MODES};
use crate::state::{Cut, Lock, State, ARCH, ARCHES, BEZIER, CIRCLE, CUBE, FORMS, PYRAMID, REVOLVE, SPHERE, SPLINE, TREE};
use brickloft_core::shapes::{shape_info, Kind};
use three_d::egui::{self, Color32, ComboBox, Slider, Ui};

/// What a control is for, by the name it has in `help.json`.
///
/// The explanations are kept in that file, apart from the code that lays
/// the panel out, so that they can be read through and reworded in one
/// place by someone who does not read Rust. The file is built into the
/// program (the program stays one file to hand to someone) and read once,
/// the first time an explanation is wanted. JSON, since the crate that
/// reads it is one the program's prefab writer already brings in; YAML
/// would be one more dependency for the same list of names and sentences.
/// https://docs.rs/serde_json
fn help(name: &str) -> &'static str {
    use std::collections::BTreeMap;
    use std::sync::OnceLock;
    static TEXTS: OnceLock<BTreeMap<String, String>> = OnceLock::new();
    // (a file that cannot be read is a mistake in the file, which the test
    // below catches before the program is ever built for anyone)
    let texts = TEXTS.get_or_init(|| serde_json::from_str(include_str!("help.json")).unwrap_or_default());
    texts.get(name).map_or("No explanation has been written for this control.", String::as_str)
}

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
    ui.label("Saved forms").on_hover_text(help("saved_forms"));
    let before = state.folder.clone();
    helped(ui, help("saved_folder"), |ui| {
        ComboBox::from_label("Folder").selected_text(state.folder.as_str()).show_ui(ui, |ui| {
            for folder in state.folders.clone() {
                ui.selectable_value(&mut state.folder, folder.clone(), folder);
            }
        })
            .response
    });
    // a page keeps its saves in the browser, where there is no folder to show
    #[cfg(not(target_arch = "wasm32"))]
    if helped(ui, help("go_to_folder"), |ui| ui.button("Go to folder")).clicked() {
        state.store_note = crate::store::show_folder(&state.folder).unwrap_or_else(|e| format!("Not opened: {e}"));
    }
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
                ui.label(format!(":: {name}")).on_hover_text(help("saved_drag"));
            });
            if ui.small_button("Open").on_hover_text(help("saved_open")).clicked() {
                open = Some((name.clone(), false));
            }
            if ui.small_button("Add").on_hover_text(help("saved_add")).clicked() {
                open = Some((name.clone(), true));
            }
            if ui.small_button("Delete").on_hover_text(help("saved_delete")).clicked() {
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
        if helped(ui, help("saved_save"), |ui| ui.button("Save")).clicked() {
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
    /// turn the grid of an arch standing at an angle to it, to line the arch up
    pub line_up: bool,
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
    // back by Ctrl+Z, so it is asked about first. Enter says yes and Escape
    // says no, as the two buttons do.
    if let Some((folder, name)) = state.deleting.clone() {
        egui::Window::new("Delete saved form").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ui.ctx(), |ui| {
            ui.set_max_width(420.0);
            ui.label(format!("Delete \"{name}\" from the folder \"{folder}\"? It cannot be brought back."));
            ui.horizontal(|ui| {
                if ui.button("Delete").clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
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
                egui::Frame::popup(ui.style()).show(ui, |ui| ui.add(egui::Label::new(text).wrap_mode(egui::TextWrapMode::Extend)));
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
        ui.heading("Brickloft");
        ui.label("Drag the coloured dots to reshape the form. Drag the form itself, or one of its arrows, to move it. Ctrl + drag a form moves it into another form and joins the two. Alt + click on a form adds a point there to pull it out of shape, and Alt + right-click takes one off; on a circle it moves within the circle's plane, or out of it with Ctrl held.");
        ui.label("View: right-drag looks around (right over backwards if you like), middle-click turns to face what the cursor is on, W A S D fly, Space or E rises, Q sinks, Shift + scroll sets how fast.");
        ui.separator();

        // settings of the view, not of any one form: above everything that changes with the form
        ui.horizontal(|ui| {
            ui.label("Show");
            ui.checkbox(&mut state.show_ruler, "Rulers");
            ui.checkbox(&mut state.show_rings, "Rings");
            ui.checkbox(&mut state.show_arrows, "Arrows");
            ui.checkbox(&mut state.show_dots, "Dots");
            hint(ui, help("show_parts"));
        });
        // a web page cannot listen for connections: see `mcp`
        if cfg!(not(target_arch = "wasm32")) {
            helped(ui, help("claude_on"), |ui| ui.checkbox(&mut state.claude_on, "Let Claude drive the editor"));
            if !state.claude_note.is_empty() {
                ui.label(state.claude_note.as_str());
            }
            if state.claude_on {
                // the command that tells Claude Code where the editor is, to copy
                let mut command = format!("claude mcp add --transport http brickloft {}", crate::CLAUDE_ADDRESS);
                helped(ui, help("claude_command"), |ui| ui.add(egui::TextEdit::singleline(&mut command).desired_width(190.0)));
            }
        }
        ui.separator();

        let before = state.form;
        helped(ui, help("form"), |ui| {
            ComboBox::from_label("Form").selected_text(FORMS[state.form]).show_ui(ui, |ui| {
                for (i, name) in FORMS.iter().enumerate() {
                    ui.selectable_value(&mut state.form, i, *name);
                }
            })
            .response
        });
        if state.form != before {
            // a tree's colours are for a tree's parts: they go when it does
            if before == TREE && state.paint.mode == Mode::Pattern {
                state.paint = crate::paint::Paint::default();
            }
            state.preset();
        }
        if helped(ui, help("reset_form"), |ui| ui.button("Reset this form")).clicked() {
            state.reset();
        }
        ui.horizontal(|ui| {
            ui.label("Sliders go up to");
            ui.add(egui::DragValue::new(&mut state.max_size).range(20.0..=brickloft_core::spec::MAX_SPAN).speed(1.0)).on_hover_text(help("sliders_go_up_to"));
            hint(ui, help("sliders_go_up_to"));
        });
        ui.horizontal(|ui| {
            ui.label("Longest brick");
            ui.add(egui::DragValue::new(&mut state.spec.max_ext).range(1..=4000).speed(1.0)).on_hover_text(help("longest_brick"));
            hint(ui, help("longest_brick"));
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
        if state.form == TREE {
            tree(ui, state, largest);
        }
        let s = &mut state.spec;
        let is_curve = state.form <= BEZIER;
        match state.form {
            ARCH if arch_feet => {
                // said where it applies: the cause is not to be guessed from the bricks
                let (a, b) = (s.points[0], s.points[s.points.len() - 1]);
                if a.x != b.x && a.y != b.y {
                    ui.colored_label(Color32::from_rgb(255, 200, 90), help("arch_askew"));
                    asked.line_up = helped(ui, help("line_up"), |ui| ui.button("Turn its grid to line it up")).clicked();
                }
                helped(ui, help("arch_kind"), |ui| {
                    ComboBox::from_label("Arch").selected_text(ARCHES[state.arch].0).show_ui(ui, |ui| {
                        for (i, (name, _)) in ARCHES.iter().enumerate() {
                            ui.selectable_value(&mut state.arch, i, *name);
                        }
                    })
                        .response
                });
                let mut natural = s.rise.is_none();
                helped(ui, help("natural_height"), |ui| ui.checkbox(&mut natural, "Natural height (drag the top dot to set your own)"));
                if natural {
                    s.rise = None;
                }
            }
            ARCH => {
                if s.points.len() == 3 {
                    ui.label("The arc through its three points.");
                    helped(ui, help("whole_circle"), |ui| ui.checkbox(&mut s.closed, "Whole circle"));
                } else {
                    ui.label(format!("The curve through its {} points.", s.points.len()));
                }
            }
            SPLINE => {
                helped(ui, help("closed_loop"), |ui| ui.checkbox(&mut s.closed, "Closed loop"));
            }
            CIRCLE => {
                helped(ui, help("facing"), |ui| {
                    ComboBox::from_label("Facing").selected_text(["X", "Y", "Z (flat)"][s.plane.min(2)]).show_ui(ui, |ui| {
                        for (i, name) in ["X", "Y", "Z (flat)"].iter().enumerate() {
                            ui.selectable_value(&mut s.plane, i, *name);
                        }
                    })
                        .response
                });
                // Within its plane, into an ellipse. There was a third
                // slider, across the plane, for the depth of a round
                // tube. It did what Thickness does and did it worse: the
                // tube it deepened kept the round ends of the thinner
                // tube and gained flat sides between them. A tube's size
                // is its Thickness, and one control says so.
                ui.label("Stretch:");
                for (axis, name) in ["X", "Y", "Z"].iter().enumerate().filter(|(axis, _)| *axis != s.plane.min(2)) {
                    helped(ui, help("stretch_circle"), |ui| ui.add(Slider::new(&mut s.stretch[axis], 0.25..=1000.0).logarithmic(true).text(*name)));
                }
                // (a circle from before, deepened across its plane, is as round as its Thickness makes it from the next change on)
                s.stretch[s.plane.min(2)] = 1.0;
            }
            CUBE => {
                // Its three lengths in microbricks, as it is built. The
                // form keeps half a side and a stretch along each axis
                // (see `brickloft_core::spec::Form::Cube`), and a slider
                // for each of those would be four sliders for three
                // lengths, none of them in the units a wall is measured in.
                let snapped = s.snapped();
                if let Some(&middle) = snapped.points.first() {
                    let sides = brickloft_core::spec::cube_sides(&snapped, middle);
                    for (axis, (low, high)) in sides.into_iter().enumerate() {
                        let before = high - low;
                        let mut size = before;
                        helped(ui, help("cube_size"), |ui| ui.add(Slider::new(&mut size, 1.0..=(2.0 * largest).max(1.0)).logarithmic(true).step_by(1.0).clamping(egui::SliderClamping::Never).text(format!("Size along {}", ["X", "Y", "Z"][axis]))));
                        if size != before && size.is_finite() {
                            // about its middle, which stays where it is
                            s.radius = snapped.radius;
                            s.stretch[axis] = size.round().clamp(1.0, 2.0 * brickloft_core::spec::MAX_SPAN) / 2.0 / snapped.radius;
                        }
                    }
                }
            }
            BEZIER => {
                ui.label("Pull of each point (higher draws the curve closer):");
                s.weights.resize(s.points.len(), 1.0);
                for (i, w) in s.weights.iter_mut().enumerate() {
                    helped(ui, help("bezier_pull"), |ui| ui.add(Slider::new(w, 0.1..=8.0).logarithmic(true).text(format!("point {}", i + 1))));
                }
            }
            SPHERE => {
                ui.label("Stretch:");
                for (axis, name) in ["X", "Y", "Z"].iter().enumerate() {
                    helped(ui, help("stretch"), |ui| ui.add(Slider::new(&mut s.stretch[axis], 0.25..=1000.0).logarithmic(true).text(*name)));
                }
                helped(ui, help("sphere_keep"), |ui| {
                    ComboBox::from_label("Keep").selected_text(["Whole", "Dome", "Segment"][state.cut as usize]).show_ui(ui, |ui| {
                        ui.selectable_value(&mut state.cut, Cut::Whole, "Whole");
                        ui.selectable_value(&mut state.cut, Cut::Dome, "Dome");
                        ui.selectable_value(&mut state.cut, Cut::Segment, "Segment");
                    })
                        .response
                });
                if state.cut == Cut::Segment {
                    helped(ui, help("segment_from"), |ui| ui.add(Slider::new(&mut state.segment.0, -largest..=largest).step_by(0.5).text("from height")));
                    helped(ui, help("segment_to"), |ui| ui.add(Slider::new(&mut state.segment.1, -largest..=largest).step_by(0.5).text("to height")));
                }
            }
            _ => {}
        }
        if state.form == PYRAMID {
            ui.label("Stretch:");
            for (axis, name) in ["X", "Y"].iter().enumerate() {
                helped(ui, help("stretch"), |ui| ui.add(Slider::new(&mut s.stretch[axis], 0.25..=1000.0).logarithmic(true).text(*name)));
            }
            let mut height = s.rise.unwrap_or(s.radius);
            helped(ui, help("pyramid_height"), |ui| ui.add(Slider::new(&mut height, 1.0..=largest).logarithmic(true).step_by(1.0).text("Height")));
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
            helped(ui, help("radius"), |ui| ui.add(Slider::new(&mut s.radius, 0.5..=largest).logarithmic(true).step_by(0.5).clamping(egui::SliderClamping::Never).text(name)));
        }

        ui.separator();
        // Fill means different things to a curve and to a solid, so say which
        match state.form {
            SPHERE => {
                helped(ui, help("solid_sphere"), |ui| ui.checkbox(&mut s.fill, "Solid (untick for a hollow shell)"));
            }
            REVOLVE => {
                helped(ui, help("solid_revolve"), |ui| ui.checkbox(&mut s.fill, "Solid (untick for a shell along the profile)"));
            }
            PYRAMID => {
                helped(ui, help("solid_pyramid"), |ui| ui.checkbox(&mut s.fill, "Solid (untick for a hollow tent)"));
            }
            // (no round tube: see `brickloft_core::spec::Form::Cube`)
            CUBE => {
                helped(ui, help("solid_cube"), |ui| ui.checkbox(&mut s.fill, "Solid (untick for a hollow box or a frame)"));
            }
            // a tree is neither filled nor hollow, and has no wall
            TREE => {}
            _ => {
                helped(ui, help("fill_curve"), |ui| ui.checkbox(&mut s.fill, "Fill the area the curve encloses"));
                helped(ui, help("round_tube"), |ui| ui.checkbox(&mut s.tube, "Round tube instead of a flat band"));
            }
        }
        if is_curve && s.tube {
            // how a round section is built: see `brickloft_core::fit_stack`
            ui.horizontal(|ui| {
                ui.radio_value(&mut s.protrude, false, "Rounded").on_hover_text(help("tube_rounded"));
                ui.radio_value(&mut s.protrude, true, "Protruded").on_hover_text(help("tube_protruded"));
                hint(ui, help("tube_section"));
            });
        }
        let wall = match state.form {
            SPHERE | REVOLVE | PYRAMID | CUBE => "Wall thickness",
            _ => "Thickness",
        };
        // (a pyramid's wall is measured straight up, and only a hollow one has any; nor has a solid cube, nor any tree)
        if !(matches!(state.form, PYRAMID | CUBE) && s.fill) && state.form != TREE {
            helped(ui, help("thickness"), |ui| ui.add(Slider::new(&mut s.thick, 0.5..=largest).logarithmic(true).step_by(0.5).text(wall)));
        }
        // (a round tube too: a pattern is laid on whatever of a form is flat, see `brickloft_core::spec::pattern_flat_faces`)
        // (and a cube, which is its largest face pushed through its shortest length)
        if is_curve || state.form == CUBE {
            // One pattern in place of solid brick, or none: see
            // `brickloft_core::pattern`. One list and not a tickbox for
            // each, since a form has one pattern at most. 0 is none, 1
            // the stepping stones, and 2 onwards the styles of mosaic.
            let styles = &brickloft_core::pattern::STYLES;
            let named = |choice: usize| match choice {
                0 => "none",
                1 => "stepping stones",
                _ => styles[(choice - 2).min(styles.len() - 1)].name,
            };
            let before = if s.stones.is_some() { 1 } else { s.mosaic.map_or(0, |laid| 2 + laid.style.min(styles.len() - 1)) };
            let mut choice = before;
            helped(ui, help("pattern"), |ui| {
                ComboBox::from_label("Pattern").selected_text(named(choice)).show_ui(ui, |ui| {
                    for n in 0..styles.len() + 2 {
                        ui.selectable_value(&mut choice, n, named(n));
                    }
                })
                .response
            });
            if choice != before {
                s.stones = (choice == 1).then_some(brickloft_core::pattern::Stones { size: 10.0, gap: 1.5, uneven: 0.5, seed: 1 });
                // (`then` and not `then_some`, which works its value out whether or not it is wanted: with the stepping stones chosen that was 1 - 2, and the editor stopped)
                // (cobbles start uneven: all alike, they are a grid of squares)
                s.mosaic = (choice >= 2).then(|| brickloft_core::pattern::Mosaic { style: choice - 2, size: 2, bevel: false, border: 0, seed: 1, mortar: true, stagger: -1, upright: false, uneven: if named(choice) == "cobblestone" { [60, 60] } else { [0; 2] } });
                // A pattern is its colours, and each has colours of its
                // own to start in (a rug's reds and golds, a floor of
                // planks' browns). They are brought in with the pattern
                // unless the player has colours of their own for one:
                // the pattern's colouring already chosen, in colours
                // that are none of the last pattern's sets.
                let own = state.paint.mode == Mode::Pattern && !crate::paint::palettes(named(before)).iter().any(|set| state.paint.colours == *set);
                if choice != 0 && !own {
                    (state.paint.mode, state.paint.colours) = (Mode::Pattern, crate::paint::palette(named(choice)).to_vec());
                }
            }
            // another pattern, in the next of its sets of colours
            let mut another = None;
            if let Some(laid) = &mut s.stones {
                helped(ui, help("stone_size"), |ui| ui.add(Slider::new(&mut laid.size, brickloft_core::pattern::LEAST_STONE..=largest.min(100.0)).logarithmic(true).step_by(0.5).text("Stone size")));
                helped(ui, help("stone_gap"), |ui| ui.add(Slider::new(&mut laid.gap, brickloft_core::pattern::LEAST_GAP..=8.0).step_by(0.5).text("Gap")));
                helped(ui, help("stone_uneven"), |ui| ui.add(Slider::new(&mut laid.uneven, 0.0..=1.0).step_by(0.05).text("Uneven")));
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut laid.seed).speed(1.0));
                    ui.label("Seed");
                    if ui.button("Another").clicked() {
                        laid.seed = laid.seed.wrapping_add(1);
                        another = Some(laid.seed);
                    }
                    hint(ui, help("pattern_seed"));
                });
            }
            if let Some(laid) = &mut s.mosaic {
                // (each style shows the settings it has a use for)
                let style = named(choice);
                let masonry = matches!(style, "bricks" | "cobblestone");
                if style == "planks" || masonry {
                    helped(ui, help("pattern_size"), |ui| ui.add(Slider::new(&mut laid.size, 1..=8).text("Size")));
                }
                if masonry {
                    // how unlike one another the pieces are, each way (see `brickloft_core::pattern::Masonry`)
                    helped(ui, help("pattern_uneven_along"), |ui| ui.add(Slider::new(&mut laid.uneven[0], 0..=100).suffix("%").text("Uneven along")));
                    helped(ui, help("pattern_uneven_across"), |ui| ui.add(Slider::new(&mut laid.uneven[1], 0..=100).suffix("%").text("Uneven across")));
                    helped(ui, help("pattern_mortar"), |ui| ui.checkbox(&mut laid.mortar, "Mortar between"));
                }
                if style == "bricks" {
                    helped(ui, help("pattern_bevel"), |ui| ui.checkbox(&mut laid.bevel, "Bevelled"));
                    // below nothing, every course is moved along by an amount of its own
                    let mut uneven = laid.stagger < 0;
                    if helped(ui, help("pattern_stagger_uneven"), |ui| ui.checkbox(&mut uneven, "Uneven stagger")).changed() {
                        laid.stagger = if uneven { -1 } else { 50 };
                    }
                    if !uneven {
                        helped(ui, help("pattern_stagger"), |ui| ui.add(Slider::new(&mut laid.stagger, 0..=50).suffix("%").text("Stagger")));
                    }
                    helped(ui, help("pattern_upright"), |ui| ui.checkbox(&mut laid.upright, "Courses upright"));
                }
                if matches!(style, "medallions" | "rug") {
                    helped(ui, help("pattern_border"), |ui| ui.add(Slider::new(&mut laid.border, 0..=6).text("Border")));
                }
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut laid.seed).speed(1.0));
                    ui.label("Seed");
                    if ui.button("Another").clicked() {
                        laid.seed = laid.seed.wrapping_add(1);
                        another = Some(laid.seed);
                    }
                    hint(ui, help("pattern_seed"));
                });
            }
            if let Some(seed) = another {
                let sets = crate::paint::palettes(named(choice));
                state.paint.colours = sets[seed as usize % sets.len()].to_vec();
            }
            // (a cube's depth is one of its three lengths)
            if state.form != CUBE {
                helped(ui, help("depth"), |ui| ui.add(Slider::new(&mut s.depth, 1..=largest as i32).logarithmic(true).text("Depth")));
            }
        }
        if !s.pulls.is_empty() {
            helped(ui, help("reach_of_added_points"), |ui| ui.add(Slider::new(&mut s.pull_reach, 1.0..=largest).logarithmic(true).step_by(0.5).text("Reach of added points")));
            if ui.button(format!("Remove the last added point ({})", s.pulls.len())).clicked() {
                s.pulls.pop();
            }
            // said where it applies, since the difference is large and not obvious
            let across = s.plane.min(2);
            if state.form == CIRCLE && s.pulls.iter().any(|(_, by)| by[across] != 0.0) {
                ui.label("A point pulled out of the circle's plane is built of sloped pieces where it was pulled, and is less smooth there than one pulled within the plane, more so the steeper it climbs. To tilt the whole circle, drag one of its rings.");
            }
        }
        helped(ui, help("neighbouring_dots_follow"), |ui| ui.add(Slider::new(&mut state.follow, 0.0..=1.0).step_by(0.05).text("Neighbouring dots follow")));
        ui.label("Sizes are in microbricks. Half steps are single game units.");
        if is_curve && s.tube && s.thick < 3.0 {
            ui.label("A round tube this thin is, in bricks, a square bar: there is no room for a rounder cross-section under 3 microbricks.");
        }
        ui.horizontal(|ui| {
            ui.label("Quarter turn").on_hover_text(help("quarter_turn"));
            for (axis, name) in ["X", "Y", "Z"].iter().enumerate() {
                if ui.button(*name).clicked() {
                    asked.turn = Some(axis);
                }
            }
            hint(ui, help("quarter_turn"));
        });
        if crate::state::on_own_grid(s) {
            ui.horizontal(|ui| {
                ui.label("Its grid is at an angle.").on_hover_text(help("grid_at_an_angle"));
                if ui.button("Level the grid").clicked() {
                    asked.level = true;
                }
            });
        }
        // (not a tree, which is no one's mirror image; it can still be flipped)
        if state.form != TREE {
            ui.horizontal(|ui| {
                ui.label("Mirror").on_hover_text(help("mirror"));
                for (axis, name) in ["X", "Y", "Z"].iter().enumerate() {
                    ui.checkbox(&mut s.mirror[axis], *name);
                }
                hint(ui, help("mirror"));
            });
        }
        // (There was a second tickbox here, to make the bricks of a form that
        // was already symmetric mirror images of each other. It did nothing
        // that could be seen on most forms and was not understood, and a
        // mirrored form has its bricks made mirror images in any case.)
        ui.horizontal(|ui| {
            ui.label("Flip");
            for (axis, name) in ["X", "Y", "Z"].into_iter().enumerate() {
                ui.checkbox(&mut s.flip[axis], name).on_hover_text(help("flip"));
            }
            hint(ui, help("flip"));
        });

        // (There were Add point and Remove point buttons here, for the dots a
        // form is drawn through. Alt + click and Alt + right-click on the
        // form add and remove a point on any form, and are the one way now.)

        ui.separator();
        let lock_name = |l: Lock| match l {
            Lock::Free => "Facing the camera",
            Lock::Axis(0) => "Keep X (move in Y and Z)",
            Lock::Axis(1) => "Keep Y (move in X and Z)",
            Lock::Axis(_) => "Keep Z (move in X and Y)",
        };
        helped(ui, help("dots_move"), |ui| {
            ComboBox::from_label("Dots move").selected_text(lock_name(state.lock)).show_ui(ui, |ui| {
                for l in [Lock::Free, Lock::Axis(0), Lock::Axis(1), Lock::Axis(2)] {
                    ui.selectable_value(&mut state.lock, l, lock_name(l));
                }
            })
            .response
        });
        helped(ui, help("floor_below_form"), |ui| ui.add(Slider::new(&mut state.floor_gap, 0.0..=40.0).step_by(1.0).text("Floor below form")));
        helped(ui, help("snap_half_microbricks"), |ui| ui.checkbox(&mut state.fine_snap, "Snap dots to half microbricks"));
        asked.best = helped(ui, help("snap_best_fit"), |ui| ui.button("Snap to best fit nearby"))
            .clicked();
        helped(ui, help("snap_after_every_drag"), |ui| ui.checkbox(&mut state.auto_best, "Do that after every drag"));
        if !state.best_note.is_empty() {
            ui.label(state.best_note.as_str());
        }

        ui.separator();
        let s = &mut state.spec;
        ui.label("Detail");
        let size_name = |g: i32| if g == 0 { "Automatic".to_string() } else { format!("{g} microbrick{}", if g == 1 { "" } else { "s" }) };
        helped(ui, help("brick_size"), |ui| {
            ComboBox::from_label("Brick size").selected_text(size_name(s.grain)).show_ui(ui, |ui| {
                ui.selectable_value(&mut s.grain, 0, size_name(0));
                for g in brickloft_core::spec::GRAINS {
                    ui.selectable_value(&mut s.grain, g, size_name(g));
                }
            })
            .response
        });
        if s.grain == 0 {
            helped(ui, help("brick_budget"), |ui| ui.add(Slider::new(&mut s.budget, 100..=30000).logarithmic(true).text("Brick budget")));
        }
        if is_curve && !s.tube {
            helped(ui, help("simplify"), |ui| ui.add(Slider::new(&mut s.simplify, 0.0..=1.5).text("Simplify")));
        }
        helped(ui, help("ramps_and_corners"), |ui| ui.checkbox(&mut s.wedges, "Use ramps and corner pieces"));

        ui.separator();
        let built = &state.built;
        let count = |kind: Kind| built.bricks.iter().filter(|b| shape_info(b.mask).is_some_and(|s| s.kind == kind)).count();
        ui.label(format!("{} bricks: {} cubes, {} ramps, {} tetrahedra, {} cut cubes, {} hips, {} inner corners, {} half corners", built.bricks.len(), count(Kind::Cube), count(Kind::Ramp), count(Kind::Tet), count(Kind::Cut), count(Kind::Hip), count(Kind::Inner), count(Kind::HalfOuter) + count(Kind::HalfInner) + count(Kind::HalfInnerInv)));
        if built.grain > 1 {
            ui.label(format!("Brick size {} microbricks", built.grain));
        }
        if state.spec.mosaic.is_some() && state.spec.grain == 0 {
            // (a pattern is not kept within the budget: see `brickloft_core::spec::build_as`)
            ui.label("A pattern is laid at single microbricks, whatever the brick budget: its detail is its bricks. Make the form larger for more detail, or set Brick size for less.");
        } else if state.spec.grain == 0 && built.bricks.len() > state.spec.budget {
            ui.label("Over budget: larger bricks would be too coarse for the form's thinnest part. Thicken it, shrink it, or raise Simplify.");
        }
        if built.solid > 0.0 {
            ui.label(format!("Discrepancy: {:.2}% of the form's volume ({:.1} of {:.0} microbricks)", 100.0 * built.error / built.solid, built.error, built.solid))
                .on_hover_text(help("discrepancy"));
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
            asked.report = helped(ui, help("save_report"), |ui| ui.button("Save report"))
                .clicked();
            // a page's reports go to the clipboard: there are no files to clear
            if cfg!(not(target_arch = "wasm32")) {
                asked.clear_reports = helped(ui, help("clear_reports"), |ui| ui.button("Clear reports")).clicked();
            }
        });
        // a web page cannot put a file on the clipboard (see `brz`), nor
        // write one to disk
        if cfg!(not(target_arch = "wasm32")) {
            asked.screenshot = helped(ui, help("save_screenshot"), |ui| ui.button("Save screenshot"))
                .clicked();
            asked.copy = helped(ui, help("copy_to_clipboard"), |ui| ui.add_enabled(state.problem.is_none() && !state.built.bricks.is_empty(), egui::Button::new("Copy to Clipboard")))
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
    ui.label("Workspace").on_hover_text(help("workspace"));
    let tilted = |spec: &brickloft_core::spec::Spec| if crate::state::on_own_grid(spec) { ", at an angle" } else { "" };
    ui.label(format!("> {} (editing) on grid {}{}", FORMS[state.form], state.spec.on_grid + 1, tilted(&state.spec)));
    let mut switch = None;
    for (k, item) in state.others.iter().enumerate() {
        let label = format!(" {} {} on grid {}{}", if item.picked { "+" } else { " " }, item.name(), item.spec.on_grid + 1, tilted(&item.spec));
        if ui.selectable_label(false, label).on_hover_text(help("workspace_form")).clicked() {
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
    ui.label("Shift + click a form to pick it as well as the one being edited (it is marked +), and again to let it go; Ctrl+A picks all. Ctrl+C copies the form being edited and any marked +. Ctrl+V pastes what was copied onto this grid. Ctrl+D duplicates the form being edited.");
    ui.horizontal(|ui| {
        if ui.button("Add to this grid").on_hover_text(help("add_to_this_grid")).clicked() {
            state.add_cube();
        }
        if ui.button("New grid").on_hover_text(help("new_grid")).clicked() {
            state.add_grid();
        }
        if ui.button("Duplicate").on_hover_text(help("duplicate_form")).clicked() {
            state.add_copy();
        }
        if ui.add_enabled(!state.others.is_empty(), egui::Button::new("Remove")).on_hover_text(help("remove_form")).clicked() {
            state.remove_active();
        }
        hint(ui, help("workspace_buttons"));
    });
}

/// How the form being edited is coloured.
/// The controls of a tree: its kind, its size, the three settings each
/// kind has under its own names, and which tree of the kind it is.
fn tree(ui: &mut Ui, state: &mut State, largest: f64) {
    use brickloft_core::tree::{Kind, Tree, KINDS, LEAST_HEIGHT, LEAST_SPREAD};
    let titled = |kind: Kind| ["Pine tree", "Palm tree", "Oak tree", "Beech tree", "Boxwood bush", "Hedge", "Peony", "Sunflower", "Tulip", "Hydrangea", "Petunias", "Reeds"][kind as usize];
    let before = state.spec.tree.kind;
    let mut kind = before;
    helped(ui, help("tree_kind"), |ui| {
        ComboBox::from_label("Kind").selected_text(titled(kind)).show_ui(ui, |ui| {
            for (k, _) in KINDS {
                ui.selectable_value(&mut kind, k, titled(k));
            }
        })
        .response
    });
    if kind != before {
        state.usual_tree(kind);
    }
    let named = KINDS[kind as usize].1;
    // which of the kind's sets of colours, if it is in one of them
    let (sets, names) = (crate::paint::palettes(named), crate::paint::tree_palette_names(named));
    let now = sets.iter().position(|set| state.paint.mode == Mode::Pattern && state.paint.colours == *set);
    let mut choice = now;
    helped(ui, help("tree_colours"), |ui| {
        ComboBox::from_label("Colours").selected_text(now.map_or("Your own", |n| names[n])).show_ui(ui, |ui| {
            for (n, name) in names.iter().enumerate() {
                ui.selectable_value(&mut choice, Some(n), *name);
            }
        })
        .response
    });
    if let Some(n) = choice.filter(|_| choice != now) {
        (state.paint.mode, state.paint.colours) = (Mode::Pattern, sets[n].to_vec());
        // (the view is drawn again: the colour controls further down look
        // for a change made in them, and this one was made before they
        // looked, so a tree stayed in its old colours until something
        // else changed)
        state.scene += 1;
    }
    let s = &mut state.spec;
    let mut height = s.rise.unwrap_or(Tree::usual_size(kind).0);
    if helped(ui, help("tree_height"), |ui| ui.add(Slider::new(&mut height, LEAST_HEIGHT..=largest.max(LEAST_HEIGHT)).logarithmic(true).step_by(1.0).clamping(egui::SliderClamping::Never).text("Height"))).changed() {
        s.rise = Some(height);
    }
    let spread = match kind {
        Kind::Palm => "Frond length",
        Kind::Sunflower => "Half the head's width",
        Kind::Peony => "Half the bloom's width",
        Kind::Hedge | Kind::Reeds => "Half its thickness",
        Kind::Tulip => "Half the cup's width",
        _ => "Spread",
    };
    helped(ui, help("tree_spread"), |ui| ui.add(Slider::new(&mut s.radius, LEAST_SPREAD..=largest.max(LEAST_SPREAD)).logarithmic(true).step_by(0.5).clamping(egui::SliderClamping::Never).text(spread)));
    if matches!(kind, Kind::Hedge | Kind::Reeds) {
        // its length, end to end: kept as how many times its thickness it is, so that a thicker hedge is a longer one
        let mut length = 2.0 * s.radius * s.stretch.x.max(1.0);
        if helped(ui, help("hedge_length"), |ui| ui.add(Slider::new(&mut length, 2.0 * s.radius..=largest.max(2.0 * s.radius)).logarithmic(true).step_by(1.0).clamping(egui::SliderClamping::Never).text("Length"))).changed() {
            s.stretch.x = (length / (2.0 * s.radius)).max(1.0);
        }
    }
    let (fullness, count, bend) = match kind {
        Kind::Pine => ("Fullness", "Tiers of boughs", "Droop"),
        Kind::Palm => ("Leaflet length", "Fronds", "Lean"),
        Kind::Oak => ("Foliage", "Main limbs", "Gnarl"),
        Kind::Boxwood => ("Density", "Stems", "Looseness"),
        Kind::Beech => ("Foliage", "Main limbs", "Crookedness"),
        Kind::Hedge => ("Density", "Plants along it", "Looseness"),
        Kind::Peony => ("Petals and leaves", "Stems in the clump", "How open the bloom is"),
        Kind::Hydrangea => ("Leaves", "Flower heads", "Looseness"),
        Kind::Petunia => ("Leaves", "Flowers", "How far it trails"),
        Kind::Reeds => ("Leaves", "Stems", "Sway"),
        Kind::Sunflower => ("Petal and leaf size", "Flowers in the clump", "Nod of the head"),
        Kind::Tulip => ("Leaf size", "Flowers in the clump", "How open the cup is"),
    };
    let (least, most) = Tree::count_range(kind);
    helped(ui, help("tree_count"), |ui| ui.add(Slider::new(&mut s.tree.count, least..=most).text(count)));
    helped(ui, help("tree_fullness"), |ui| ui.add(Slider::new(&mut s.tree.fullness, 0..=100).suffix("%").text(fullness)));
    helped(ui, help("tree_bend"), |ui| ui.add(Slider::new(&mut s.tree.bend, 0..=100).suffix("%").text(bend)));
    match kind {
        Kind::Pine => {
            helped(ui, help("tree_extras"), |ui| ui.checkbox(&mut s.tree.extras, "A star and baubles"));
        }
        Kind::Palm => {
            helped(ui, help("tree_extras"), |ui| ui.checkbox(&mut s.tree.extras, "Coconuts"));
        }
        Kind::Oak | Kind::Beech => {
            helped(ui, help("tree_extras"), |ui| ui.checkbox(&mut s.tree.extras, "Bare (no leaves)"));
        }
        Kind::Boxwood | Kind::Hedge => {
            helped(ui, help("tree_extras"), |ui| ui.checkbox(&mut s.tree.extras, "Berries"));
        }
        Kind::Reeds => {
            helped(ui, help("tree_extras"), |ui| ui.checkbox(&mut s.tree.extras, "Feathery plumes (common reed)"));
        }
        Kind::Peony | Kind::Sunflower | Kind::Tulip | Kind::Hydrangea | Kind::Petunia => {}
    }
    ui.horizontal(|ui| {
        ui.add(egui::DragValue::new(&mut s.tree.seed).speed(1.0));
        ui.label("Seed");
        if ui.button("Another").clicked() {
            s.tree.seed = s.tree.seed.wrapping_add(1);
        }
        hint(ui, help("tree_seed"));
    });
}

fn colours(ui: &mut Ui, state: &mut State) {
    let before = state.paint.clone();
    let paint = &mut state.paint;
    let name = MODES.iter().find(|m| m.0 == paint.mode).map_or("", |m| m.1);
    helped(ui, help("colour_mode"), |ui| {
        ComboBox::from_label("Colour").selected_text(name).show_ui(ui, |ui| {
            for (mode, name) in MODES {
                ui.selectable_value(&mut paint.mode, mode, name);
            }
        })
            .response
    });
    {
        // one colour for a solid; up to four to blend or stripe between; one for each feature of a pattern
        let most = match paint.mode {
            Mode::Solid => 1,
            Mode::Pattern => crate::paint::MOST_COLOURS,
            _ => 4,
        };
        // (A form that had a pattern's eight colours and is set to blend or stripes keeps the first four: the rest would be blended through and could not be seen or changed here.)
        // (One colour shows the first and keeps the rest, for when the form is set back.)
        if most > 1 {
            paint.colours.truncate(most);
        }
        // A colour is made the same as another by copying one and pasting
        // it on the other, from the menu a right click on either opens.
        // The copied colour is kept in the panel's own memory and not on
        // the system clipboard, which holds the last prefab copied.
        let copied = egui::Id::new("copied colour");
        ui.horizontal_wrapped(|ui| {
            for colour in paint.colours.iter_mut().take(most) {
                ui.color_edit_button_srgb(colour).context_menu(|ui| {
                    if ui.button("Copy colour").clicked() {
                        ui.data_mut(|data| data.insert_temp(copied, *colour));
                        ui.close();
                    }
                    let held: Option<[u8; 3]> = ui.data(|data| data.get_temp(copied));
                    if ui.add_enabled(held.is_some(), egui::Button::new("Paste colour")).clicked() {
                        *colour = held.unwrap_or(*colour);
                        ui.close();
                    }
                });
            }
            if most > 1 && paint.colours.len() < most && ui.small_button("+").on_hover_text(help("colour_more")).clicked() {
                paint.colours.push([230, 230, 230]);
            }
            if most > 1 && paint.colours.len() > 2 && ui.small_button("-").on_hover_text(help("colour_fewer")).clicked() {
                paint.colours.pop();
            }
            hint(ui, help("colours"));
        });
        if most > 1 {
            helped(ui, help("colour_runs_along"), |ui| {
                ComboBox::from_label("Runs along").selected_text(["X", "Y", "Z (up)"][paint.axis.min(2)]).show_ui(ui, |ui| {
                    for (i, axis) in ["X", "Y", "Z (up)"].iter().enumerate() {
                        ui.selectable_value(&mut paint.axis, i, *axis);
                    }
                })
                    .response
            });
        }
        if paint.mode == Mode::Stripes {
            helped(ui, help("stripe_width"), |ui| ui.add(Slider::new(&mut paint.width, 1..=24).text("Stripe width")));
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    // (the windowless way of running egui that this test uses has newer names in egui; the old ones are what the version three_d carries still has in common with them)
    #[allow(deprecated)]
    fn the_panel_draws_every_pattern_on_every_curve_without_panicking() {
        // the panel drawn with no window, as it is every frame, for each form with each pattern chosen
        let styles = brickloft_core::pattern::STYLES.len();
        for form in 0..4 {
            for choice in 0..styles + 2 {
                let mut state = State::new();
                state.form = form;
                state.preset();
                state.refresh(|| 0.0);
                state.spec.stones = (choice == 1).then_some(brickloft_core::pattern::Stones { size: 10.0, gap: 1.5, uneven: 0.5, seed: 1 });
                state.spec.mosaic = (choice >= 2).then(|| brickloft_core::pattern::Mosaic { style: choice - 2, size: 2, bevel: false, border: 0, seed: 1, mortar: true, stagger: -1, upright: false, uneven: [0; 2] });
                if choice != 0 {
                    state.paint.mode = Mode::Pattern;
                    state.paint.colours = crate::paint::palette(if choice == 1 { "stepping stones" } else { brickloft_core::pattern::STYLES[choice - 2].name }).to_vec();
                }
                let context = egui::Context::default();
                for _ in 0..3 {
                    state.refresh(|| 0.0);
                    let _ = context.run(egui::RawInput::default(), |context| {
                        egui::SidePanel::left("panel").show(context, |ui| {
                            show(ui, &mut state);
                        });
                    });
                    let _ = state.painted();
                }
            }
        }
    }

    use std::collections::{BTreeMap, BTreeSet};

    #[test]
    fn every_explanation_asked_for_is_written_and_every_one_written_is_used() {
        let written: BTreeMap<String, String> = serde_json::from_str(include_str!("help.json")).expect("help.json is not a list of names and sentences");
        // the names this file asks for: whatever follows `help("`, up to the next quote
        let asked: BTreeSet<&str> = include_str!("panel.rs").split("help(\"").skip(1).filter_map(|rest| rest.split('"').next()).filter(|name| !name.is_empty() && name.chars().all(|c| c.is_ascii_lowercase() || c == '_')).collect();
        assert!(asked.len() > 50, "only {} names were found: the search for them is broken", asked.len());
        for name in &asked {
            assert!(written.get(*name).is_some_and(|text| !text.trim().is_empty()), "help.json has no explanation named {name}");
        }
        for name in written.keys() {
            assert!(asked.contains(name.as_str()), "help.json has an explanation named {name} that no control asks for");
        }
        assert_eq!(super::help("form"), written["form"]);
    }
}
