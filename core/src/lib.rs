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
//! - [`pattern`] fills a flat region with stepping stones in place of solid brick
//! - [`tree`] grows a pine, a palm or an oak as one solid of many parts
//! - [`furniture`] lays out a bookcase board by board and book by book, with no fitting
//! - [`grass`] lays out a lawn from the height of its ground, cell by cell, with no fitting
//! - [`maze`] lays out a maze and builds its walls
//! - [`wfc`] fills a grid with tiles that agree with their neighbours (wave function collapse)
//! - [`spec`] ties them together: a [`spec::Spec`] in, bricks out
//! - [`par`] shares a build's work out over the processor's cores
//! - [`orient`] maps each brick to a game asset, orientation and size
//! - [`report`] writes a form and its fit out as text, to be read back or inspected
//! - [`surface`] measures a build's outside against the form's: steps and slopes

pub mod blend;
pub mod curves;
pub mod fit_lathe;
pub mod fit_planar;
pub mod fit_pyramid;
pub mod fit_ribbon;
pub mod fit_round;
pub mod fit_stack;
pub mod fit_volume;
pub mod furniture;
pub mod grass;
pub mod maze;
pub mod orient;
pub mod par;
pub mod pattern;
pub mod report;
pub mod sdf;
pub mod shapes;
pub mod spec;
pub mod surface;
pub mod tree;
pub mod wfc;

pub use glam::{DVec2, DVec3};
