//! The controls of a rule program: which program, its grid and seed, and
//! the run (Play, Pause, Step, Restart). In a file of its own beside the
//! panel's, which is long enough; the explanations are in `help.json`
//! with the rest, and the test there reads this file for their names too.

use super::{help, helped, hint};
use crate::state::grow::SPEEDS;
use crate::state::State;
use brickloft_core::rewrite::bricks::MOST_SCALE;
use brickloft_core::rewrite::form::{Program, BUNDLED, MOST_LIMIT, MOST_SIDE};
use brickloft_core::rewrite::Finish;
use three_d::egui::{self, Color32, ComboBox, Slider, Ui};

/// What a program is called in the list.
fn titled(name: &str, imported: bool) -> String {
    if imported { format!("{name} (imported)") } else { name.to_string() }
}

pub(super) fn show(ui: &mut Ui, state: &mut State) {
    let before = state.spec.program.clone();
    let mut chosen = (before.name.clone(), before.imported);
    helped(ui, help("program"), |ui| {
        ComboBox::from_label("Program").selected_text(titled(&before.name, before.imported)).show_ui(ui, |ui| {
            for entry in &BUNDLED {
                ui.selectable_value(&mut chosen, (entry.name.to_string(), false), entry.name).on_hover_text(entry.about);
            }
            for name in &state.grow.imported {
                ui.selectable_value(&mut chosen, (name.clone(), true), titled(name, true));
            }
        })
        .response
    });
    if chosen != (before.name.clone(), before.imported) {
        state.usual_program(&chosen.0, chosen.1);
    }
    // what it makes and whose it is: said under the list, where a credit is read, and not only when the pointer rests on a name
    if let Some(entry) = BUNDLED.iter().find(|entry| entry.name == state.spec.program.name && !state.spec.program.imported) {
        ui.label(egui::RichText::new(format!("{}. From MarkovJunior, by Maxim Gumin.", entry.about)).weak());
    }
    // a page in a browser has no files to bring one in from
    if cfg!(not(target_arch = "wasm32")) {
        ui.horizontal(|ui| {
            let typed = ui.add(egui::TextEdit::singleline(&mut state.grow.path).hint_text("path of an .xml file").desired_width(150.0)).on_hover_text(help("program_import"));
            let entered = typed.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (ui.button("Import...").on_hover_text(help("program_import")).clicked() || entered) && !state.grow.path.trim().is_empty() {
                // (a path copied from Explorer comes in quotation marks)
                let path = std::path::PathBuf::from(state.grow.path.trim().trim_matches('"'));
                if state.import_program(&path) {
                    state.grow.path.clear();
                }
            }
            hint(ui, help("program_import"));
        });
        if !state.grow.import_note.is_empty() {
            ui.label(state.grow.import_note.as_str());
        }
    }

    let usual = Program::of(&state.spec.program.name, state.spec.program.imported).size;
    let usual = if state.spec.program.imported { crate::programs::usual_size(&state.spec.program.name).unwrap_or(usual) } else { usual };
    let program = &mut state.spec.program;
    ui.horizontal(|ui| {
        ui.label("Grid").on_hover_text(help("program_size"));
        for side in &mut program.size {
            ui.add(egui::DragValue::new(side).range(1..=MOST_SIDE).speed(0.1)).on_hover_text(help("program_size"));
        }
        if ui.add_enabled(program.size != usual, egui::Button::new("Usual")).on_hover_text(help("program_size")).clicked() {
            program.size = usual;
        }
        hint(ui, help("program_size"));
    });
    helped(ui, help("program_cell"), |ui| ui.add(Slider::new(&mut program.cell, 1..=MOST_SCALE).text("Cell size")));
    let shown = brickloft_core::rewrite::form::known(program).and_then(Result::ok);
    ui.horizontal(|ui| {
        ui.label("Seed");
        ui.add(egui::DragValue::new(&mut program.seed).speed(1.0));
        if ui.button("Another").clicked() {
            // (the one after the seed that is shown, which is not the form's own if seeds were passed over to reach it)
            program.seed = shown.as_ref().map_or(program.seed, |outcome| outcome.seed.max(program.seed)).wrapping_add(1);
        }
        hint(ui, help("program_seed"));
    });
    helped(ui, help("program_finish"), |ui| {
        ComboBox::from_label("Finish").selected_text(if program.finish == Finish::Plain { "Plain cells" } else { "With wedges" }).show_ui(ui, |ui| {
            ui.selectable_value(&mut program.finish, Finish::Plain, "Plain cells");
            ui.selectable_value(&mut program.finish, Finish::Wedged, "With wedges");
        })
        .response
    });
    ui.horizontal(|ui| {
        ui.label("Step limit");
        ui.add(egui::DragValue::new(&mut program.limit).range(1..=MOST_LIMIT).speed(100.0)).on_hover_text(help("program_limit"));
        hint(ui, help("program_limit"));
    });
    if helped(ui, help("program_colours"), |ui| ui.button("The program's own colours")).clicked() {
        state.program_colours();
    }
    // the program, its grid, its seed or its limit changed: another run
    if !before.same_run(&state.spec.program) && (before.name.as_str(), before.imported) == (state.spec.program.name.as_str(), state.spec.program.imported) {
        state.grow_changed();
    }

    ui.separator();
    let outcome = state.grow_outcome();
    let ended = state.spec.program.stop.is_none();
    ui.horizontal(|ui| {
        if state.grow.playing {
            if ui.button("Pause").on_hover_text(help("program_play")).clicked() {
                state.grow_pause();
            }
        } else if ui.button("Play").on_hover_text(help("program_play")).clicked() {
            state.grow_play();
        }
        if ui.button("Step").on_hover_text(help("program_step")).clicked() {
            state.grow_step();
        }
        if ui.button("Restart").on_hover_text(help("program_restart")).clicked() {
            state.grow_restart();
        }
        if ui.add_enabled(!ended, egui::Button::new("To the end")).on_hover_text(help("program_to_end")).clicked() {
            state.grow_finish();
        }
        hint(ui, help("program_play"));
    });
    helped(ui, help("program_speed"), |ui| ui.add(Slider::new(&mut state.grow.speed, SPEEDS.0..=SPEEDS.1).logarithmic(true).integer().text("Turns a second")));
    helped(ui, help("program_watch"), |ui| ui.checkbox(&mut state.grow.watch, "Watch a change grow"));
    // the readout: the turn, and what the program is doing
    let said = match &outcome {
        Some(outcome) if outcome.running => {
            let doing = outcome.doing.map_or(String::new(), |(node, line)| format!(": {node}, line {line} of the program"));
            format!("Turn {}{doing}{}", outcome.turns, if state.growing() { "" } else { " (paused)" })
        }
        Some(outcome) if outcome.early.is_some() => format!("Stopped at its step limit after {} turns", outcome.turns),
        Some(outcome) => format!("Finished in {} turns", outcome.turns),
        None if state.problem.is_some() && !state.fitting() => "Not run".to_string(),
        None => "Running the program...".to_string(),
    };
    ui.label(said);
    if state.grow.working {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label("Working on a long turn...").on_hover_text(help("program_working"));
        });
    }
    if let Some(outcome) = &outcome {
        let [x, y, z] = outcome.size;
        ui.label(egui::RichText::new(format!("Grid {x} by {y} by {z} cells, seed {}", outcome.seed)).weak());
    }
    if let Some(note) = state.grow_note() {
        ui.colored_label(Color32::from_rgb(255, 200, 90), note);
    }
    if state.grow_waiting() {
        ui.label("Running the programs of the workspace's other forms...");
    }
    ui.separator();
}

/// A run that was stopped early and whose bricks stand, said where a form's problem is said.
pub(super) fn caution(ui: &mut Ui, state: &State) {
    if let Some(caution) = &state.grow.caution {
        ui.colored_label(Color32::from_rgb(255, 200, 90), caution.as_str());
    }
}
