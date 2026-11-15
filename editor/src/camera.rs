//! Moving the view: the camera is flown. Right-drag turns it where it
//! stands, W A S D move it the way it looks and sideways, Space or E raise
//! it, Q lowers it, and Shift doubles the speed.
//!
//! It once turned about a point in front of it and was brought nearer and
//! further with the wheel, which made its speed depend on how far off that
//! point happened to be, and left the point behind when the forms were
//! moved. Now there is no such point: the camera turns about itself, moves
//! at one speed, and is brought nearer by moving it. The wheel does nothing
//! to the view (with Ctrl it still resizes the form being edited).
//!
//! three-d's own `FlyControl` turns on a plain left-drag, which is the
//! button the handles are dragged with, and has no keys. This uses the
//! camera's own functions:
//! https://docs.rs/three-d-asset/latest/three_d_asset/camera/struct.Camera.html

use three_d::*;

pub struct Controls {
    /// W, A, S, D, up (Space or E), down (Q): whether each is held down
    held: [bool; 7],
    shift: bool,
}

/// Microbricks a second, and how many times that with Shift held.
const SPEED: f32 = 60.0;
const FAST: f32 = 2.0;
/// Radians turned for each point the mouse moves.
const TURN: f32 = 0.005;
/// How near straight up or down the view may be tilted, in radians: at
/// either it would have no way to tell left from right.
const STEEPEST: f32 = 1.5;

impl Controls {
    pub fn new() -> Controls {
        Controls { held: [false; 7], shift: false }
    }

    /// Apply this frame's events. Events already used by the panel or by a
    /// handle are marked `handled` and left alone. `elapsed_ms` is the time
    /// since the last frame, so that speed does not depend on frame rate.
    /// `typing` is whether a text box has the keyboard.
    pub fn update(&mut self, camera: &mut Camera, events: &mut [Event], elapsed_ms: f64, typing: bool) {
        for event in events.iter_mut() {
            match event {
                Event::MouseMotion { delta, button: Some(MouseButton::Right), handled, .. } if !*handled => {
                    // Turned as a heading about the upright and a tilt from
                    // level, and never rolled: turning about the camera's
                    // own axes, as its `yaw` and `pitch` do, tips the
                    // horizon over a little with every turn.
                    let view = camera.view_direction();
                    let heading = view.y.atan2(view.x) - TURN * delta.0;
                    let tilt = (view.z.clamp(-1.0, 1.0).asin() - TURN * delta.1).clamp(-STEEPEST, STEEPEST);
                    let view = vec3(heading.cos() * tilt.cos(), heading.sin() * tilt.cos(), tilt.sin());
                    let at = camera.position();
                    camera.set_view(at, at + view, vec3(0.0, 0.0, 1.0));
                    *handled = true;
                }
                Event::ModifiersChange { modifiers } => self.shift = modifiers.shift,
                Event::KeyPress { kind, handled, .. } if !*handled => self.key(*kind, true),
                // a release always counts, or a key let go while the panel
                // had the keyboard would stay "held" and the view drift away
                Event::KeyRelease { kind, .. } => self.key(*kind, false),
                _ => {}
            }
        }

        if typing {
            // a key released into a text box must not stay held
            self.held = [false; 7];
            return;
        }
        let [w, a, s, d, space, e, q] = self.held.map(|h| h as i32 as f32);
        let step = step(camera.view_direction(), camera.right_direction(), [w - s, d - a, (space + e).min(1.0) - q], self.shift, elapsed_ms);
        if step != Vec3::zero() {
            camera.translate(step);
        }
    }

    /// Whether a key is held, so the view needs redrawing every frame.
    pub fn moving(&self) -> bool {
        self.held.iter().any(|h| *h)
    }

    fn key(&mut self, key: Key, down: bool) {
        let slot = match key {
            Key::W => 0,
            Key::A => 1,
            Key::S => 2,
            Key::D => 3,
            Key::Space => 4,
            Key::E => 5,
            Key::Q => 6,
            _ => return,
        };
        self.held[slot] = down;
    }
}

/// How far the camera moves in a frame: `ahead`, `aside` and `up` are each
/// -1, 0 or 1. Ahead is the way the camera looks, tilt and all, so that
/// looking at something and holding W goes to it; up is straight up.
fn step(view: Vec3, right: Vec3, [ahead, aside, up]: [f32; 3], fast: bool, elapsed_ms: f64) -> Vec3 {
    let way = view * ahead + vec3(right.x, right.y, 0.0) * aside + vec3(0.0, 0.0, 1.0) * up;
    if way.magnitude() < 1e-6 {
        return Vec3::zero();
    }
    // a long frame (the window dragged, a slow build) is not a long stride
    let seconds = (elapsed_ms as f32 / 1000.0).min(0.1);
    way.normalize() * SPEED * if fast { FAST } else { 1.0 } * seconds
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shift_doubles_the_speed_and_nothing_held_goes_nowhere() {
        let (view, right) = (vec3(0.6, 0.0, -0.8), vec3(0.0, -1.0, 0.0));
        assert_eq!(step(view, right, [0.0; 3], true, 16.0), Vec3::zero());
        // a second's worth, in ten frames of a tenth
        let slow = step(view, right, [1.0, 0.0, 0.0], false, 100.0) * 10.0;
        let fast = step(view, right, [1.0, 0.0, 0.0], true, 100.0) * 10.0;
        assert!((slow.magnitude() - SPEED).abs() < 1e-3 && (fast - slow * 2.0).magnitude() < 1e-3);
        // ahead is where it looks, down the slope of its view
        assert!((slow.normalize() - view).magnitude() < 1e-5);
        // two keys at once are no faster than one
        let both = step(view, right, [1.0, 1.0, 1.0], false, 100.0) * 10.0;
        assert!((both.magnitude() - SPEED).abs() < 1e-3);
        // and a stalled frame is not a leap
        assert!(step(view, right, [1.0, 0.0, 0.0], false, 5000.0).magnitude() <= SPEED * 0.1 + 1e-3);
    }
}
