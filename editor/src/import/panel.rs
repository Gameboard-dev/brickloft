//! The side panel's settings for an Imported form: the file, its size,
//! which way is up in it, and how it is turned into bricks.
//!
//! In a file of its own beside the readers, and not in `crate::panel` with
//! the other forms' settings, so that this form can be changed without
//! touching the file every form shares. It uses that file's `help` and
//! `helped`, and its explanations are in `help.json` with the rest (the
//! test that every explanation is used reads this file too).

use crate::panel::{help, helped, hint};
use crate::state::State;
use brickloft_core::mesh::form::{FITTERS, MOST_COLOURS, MOST_LOOSE, MOST_SMOOTH, UPS};
use brickloft_core::mesh::{Fitter, Up};
use three_d::egui::{self, Color32, ComboBox, Slider, Ui};

pub fn show(ui: &mut Ui, state: &mut State, largest: f64) {
    // the file: a path typed or pasted, opened by the button or by Enter
    let mut open = false;
    ui.horizontal(|ui| {
        let typed = ui.add(egui::TextEdit::singleline(&mut state.mesh_path).hint_text("Path of a .glb, .obj or .ply file").desired_width(170.0)).on_hover_text(help("import_file"));
        open = typed.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        open |= ui.button("Open").on_hover_text(help("import_file")).clicked();
        hint(ui, help("import_file"));
    });
    if open && !state.mesh_path.trim().is_empty() {
        let path = state.mesh_path.clone();
        state.open_mesh(&path, false);
    }
    match (&state.spec.import.mesh, state.spec.import.name.is_empty()) {
        (Some(mesh), _) => {
            let turned = if mesh.turned > 0 { format!(", {} of them turned over to face outwards", mesh.turned) } else { String::new() };
            ui.label(format!("{} triangles{turned}. {}", mesh.triangles(), if mesh.coloured() { "It has colours of its own." } else { "It has no colours of its own." }));
        }
        (None, true) => {
            ui.label("No file yet. Type or paste a file's path above, or drop the file on the window.");
        }
        (None, false) => {
            ui.colored_label(Color32::from_rgb(255, 120, 100), help("import_missing"));
        }
    }
    let before = state.spec.import.clone();
    let s = &mut state.spec;
    // Its size as it is built: the length of its longest side. The form
    // keeps half of that (see `brickloft_core::spec::Form::Imported`), so
    // that the dot in the view is a radius dot like any other.
    let mut size = 2.0 * s.radius;
    helped(ui, help("import_size"), |ui| ui.add(Slider::new(&mut size, 2.0..=(2.0 * largest).max(2.0)).logarithmic(true).step_by(1.0).clamping(egui::SliderClamping::Never).text("Size (longest side)")));
    if size != 2.0 * s.radius && size.is_finite() {
        s.radius = (size.round() / 2.0).clamp(1.0, crate::state::HANDLE_LIMIT);
    }
    let up_name = |up: Up| match up {
        Up::Y => "Y (glTF, most OBJ)",
        Up::Z => "Z",
        Up::X => "X",
    };
    helped(ui, help("import_up"), |ui| {
        ComboBox::from_label("Up in the file").selected_text(up_name(s.import.up)).show_ui(ui, |ui| {
            for (up, _) in UPS {
                ui.selectable_value(&mut s.import.up, up, up_name(up));
            }
        })
        .response
    });
    let fitter_name = |fitter: Fitter| match fitter {
        Fitter::Smoothed => "Smoothed (ramps and corners in its faces)",
        Fitter::Blocks => "Blocks (cubes only)",
    };
    helped(ui, help("import_fitter"), |ui| {
        ComboBox::from_label("Bricks").selected_text(fitter_name(s.import.fitter)).show_ui(ui, |ui| {
            for (fitter, _) in FITTERS {
                ui.selectable_value(&mut s.import.fitter, fitter, fitter_name(fitter));
            }
        })
        .response
    });
    helped(ui, help("import_fill"), |ui| ui.checkbox(&mut s.fill, "Fill the inside (untick for a shell)"));
    if !s.fill {
        helped(ui, help("import_shell"), |ui| ui.add(Slider::new(&mut s.thick, 1.0..=16.0).step_by(1.0).text("Shell thickness")));
    }
    helped(ui, help("import_loose"), |ui| ui.add(Slider::new(&mut s.import.loose, 0..=MOST_LOOSE).logarithmic(true).text("Drop loose pieces under")));
    helped(ui, help("import_smooth"), |ui| ui.add(Slider::new(&mut s.import.smooth, 0..=MOST_SMOOTH).text("Smooth the surface")));
    helped(ui, help("import_colours"), |ui| ui.add(Slider::new(&mut s.import.colours, 0..=MOST_COLOURS).text("Colours")));
    helped(ui, help("import_simplify"), |ui| ui.add(Slider::new(&mut s.simplify, 0.0..=1.5).text("Simplify")));
    ui.horizontal(|ui| {
        ui.label("Brick size, Brick budget and Longest brick apply too.");
        hint(ui, help("import_detail"));
    });
    if s.import != before {
        if s.import.up != before.up || s.import.smooth != before.smooth {
            // the same file stood another way up, or smoothed more or less, is another mesh to index: read again, here
            s.import.mesh = None;
            crate::import::attach(s);
        }
        if s.import.colours != before.colours {
            // its colours are the mesh's own, as many as are now asked for; with none, it is coloured as any form is
            state.paint = crate::import::paint_of(&state.spec).unwrap_or_default();
        }
        state.scene += 1;
    }
}
