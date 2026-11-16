//! The rule-program interpreter's tests, run while the interpreter is
//! still on trial and not yet part of the library (see
//! `editor/examples/rewrite_pictures.rs` for why it is brought in by
//! path). When the form goes into the editor, `rewrite` becomes a module
//! of the core, its tests run with the core's, and this file goes.

mod shapes {
    pub use brickloft_core::shapes::*;
}
mod pattern {
    pub use brickloft_core::pattern::*;
}
#[allow(dead_code, unused_imports)]
#[path = "../src/rewrite.rs"]
mod rewrite;
