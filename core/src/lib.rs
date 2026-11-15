//! Curves and solids fitted to Brickadia microbricks.
//!
//! Everything here works in *cell* space: one cell is the smallest microbrick,
//! a cube 2 game units on a side, and lattice points are the cells' corners.
//!
//! - [`curves`] turns control points into polylines
//! - [`sdf`] turns polylines into solids, as distance functions
//! - [`fit_planar`] and [`fit_volume`] turn solids into bricks; [`fit_round`]
//!   builds spheres and ellipsoids as one faceted solid, and [`fit_stack`]
//!   round tubes as stacked outlines joined by sloped pieces
//! - [`spec`] ties them together: a [`spec::Spec`] in, bricks out
//! - [`orient`] maps each brick to a game asset, orientation and size
//! - [`report`] writes a form and its fit out as text, to be read back or inspected
//! - [`surface`] measures a build's outside against the form's: steps and slopes

pub mod curves;
pub mod fit_planar;
pub mod fit_round;
pub mod fit_stack;
pub mod fit_volume;
pub mod orient;
pub mod report;
pub mod sdf;
pub mod shapes;
pub mod spec;
pub mod surface;

pub use glam::{DVec2, DVec3};
