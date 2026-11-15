//! Write the trial file for brick grids at an angle (see
//! `arcs_editor::brz::tilted_trial`):
//!
//!   cargo run -p arcs-editor --example grid_trial

fn main() {
    let path = std::env::current_dir().unwrap_or_default().join("arcs-grid-trial.brz");
    match arcs_editor::brz::tilted_trial().and_then(|bytes| std::fs::write(&path, bytes).map_err(|e| e.to_string())) {
        Ok(()) => println!("wrote {}", path.display()),
        Err(e) => eprintln!("could not write the trial file: {e}"),
    }
}
