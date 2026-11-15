//! Rulers along the form: one for each direction it has any size in, with a
//! tick per microbrick and the distance written at regular ticks.
//!
//! They are drawn by the panel's 2D painter over the finished view, from
//! points of the scene projected to the screen, and not as geometry in the
//! scene: numbers need text, which the 3D side has none of without a font
//! atlas, and a ruler should stay legible in front of the bricks instead of
//! being hidden behind them.

use three_d::egui::{self, Align2, Color32, FontId, Pos2, Stroke};
use three_d::{vec3, Camera, InnerSpace, Vec3};

const LINE: Color32 = Color32::from_rgb(236, 236, 242);
const SHADE: Color32 = Color32::from_rgb(20, 22, 26);

/// Where a point of the scene is on the screen, in egui's points (from the
/// top left), or `None` if it is behind the camera.
fn on_screen(camera: &Camera, pixels_per_point: f32, p: Vec3) -> Option<Pos2> {
    if (p - camera.position()).dot(camera.view_direction()) <= camera.z_near() {
        return None;
    }
    // three-d counts pixels from the bottom left
    let pixel = camera.pixel_at_position(p);
    Some(Pos2::new(pixel.x / pixels_per_point, (camera.viewport().height as f32 - pixel.y) / pixels_per_point))
}

/// Draw the rulers for a form occupying cells `lo..hi`.
///
/// The two level rulers lie on the floor of the form's box, one cell out
/// from its low-x and low-y sides, and the upright one stands at the corner
/// where they meet, so all three read from the same zero.
pub fn draw(ui: &egui::Ui, camera: &Camera, pixels_per_point: f32, lo: [i32; 3], hi: [i32; 3]) {
    // under the panel, over the view
    let painter = ui.ctx().layer_painter(egui::LayerId::background());
    let project = |p: Vec3| on_screen(camera, pixels_per_point, p);
    let corner = vec3(lo[0] as f32 - 1.0, lo[1] as f32 - 1.0, lo[2] as f32);
    let axes = [vec3(1.0, 0.0, 0.0), vec3(0.0, 1.0, 0.0), vec3(0.0, 0.0, 1.0)];
    // each ruler: where its zero is, and which way its ticks point
    let rulers = [
        (vec3(lo[0] as f32, corner.y, corner.z), -axes[1]),
        (vec3(corner.x, lo[1] as f32, corner.z), -axes[0]),
        (corner, -axes[0]),
    ];
    for (axis, (zero, tick)) in rulers.into_iter().enumerate() {
        let length = hi[axis] - lo[axis];
        if length <= 0 {
            continue;
        }
        let at = |cells: f32| zero + axes[axis] * cells;
        let (Some(a), Some(b)) = (project(at(0.0)), project(at(length as f32))) else { continue };
        // a dark line under a light one reads against bricks and floor alike
        painter.line_segment([a, b], Stroke::new(3.0_f32, SHADE));
        painter.line_segment([a, b], Stroke::new(1.2_f32, LINE));

        // how many microbricks between numbered ticks: enough that the
        // numbers do not run into each other at this zoom
        let per_cell = a.distance(b) / length as f32;
        let step = [1, 2, 5, 10, 20, 50, 100, 200, 500].into_iter().find(|s| *s as f32 * per_cell >= 30.0).unwrap_or(1000);
        for cells in 0..=length {
            let numbered = cells % step == 0 || cells == length;
            // unnumbered ticks are left out once they would merge into a bar
            if !numbered && per_cell < 5.0 {
                continue;
            }
            let (Some(from), Some(to)) = (project(at(cells as f32)), project(at(cells as f32) + tick * if numbered { 0.9 } else { 0.4 })) else { continue };
            painter.line_segment([from, to], Stroke::new(1.2_f32, LINE));
            // the last tick always carries the full length; a regular tick
            // right before it gives way so the two numbers do not overlap
            let crowded = cells != length && ((length - cells) as f32) * per_cell < 22.0;
            if numbered && !crowded {
                if let Some(label) = project(at(cells as f32) + tick * 1.9) {
                    let font = FontId::proportional(if cells == length { 13.5 } else { 11.5 });
                    painter.text(label + egui::vec2(1.0, 1.0), Align2::CENTER_CENTER, cells, font.clone(), SHADE);
                    painter.text(label, Align2::CENTER_CENTER, cells, font, LINE);
                }
            }
        }
    }
}
