//! Moving the view: the camera is flown. Right-drag turns it where it
//! stands, a middle click turns it to face whatever the cursor is on,
//! W A S D move it the way it looks and sideways, Space or E raise it,
//! Q lowers it, and Shift with the scroll wheel sets how fast it goes.
//!
//! It can be tilted right over backwards. Its direction is kept as a
//! heading about the upright and a tilt from level, and its own "up" is
//! worked out from those two instead of being fixed at the world's: with a
//! fixed up, a view cannot pass straight up or down, where it would have no
//! way to tell left from right.
//!
//! It once turned about a point in front of it and was brought nearer and
//! further with the wheel, which made its speed depend on how far off that
//! point happened to be, and left the point behind when the forms were
//! moved. Now there is no such point: the camera turns about itself and is
//! brought nearer by moving it. The wheel, with nothing to zoom, sets the
//! speed instead, while Shift is held: one speed is too slow across a large build and too fast
//! beside a small one, and a key held to go faster gave only two. (With
//! Ctrl the wheel still resizes the form being edited.)
//!
//! three-d's own `FlyControl` turns on a plain left-drag, which is the
//! button the handles are dragged with, and has no keys. This uses the
//! camera's own functions:
//! https://docs.rs/three-d-asset/latest/three_d_asset/camera/struct.Camera.html

use three_d::*;

pub struct Controls {
    /// W, A, S, D, up (Space or E), down (Q): whether each is held down
    held: [bool; 7],
    /// microbricks a second
    speed: f32,
    /// which way it looks: round the upright from +x, and up from level.
    /// The tilt is not kept within a quarter turn: past it, the camera is
    /// on its back.
    heading: f32,
    tilt: f32,
}

/// The speed to start with, and the slowest and fastest the wheel can set.
const SPEED: f32 = 80.0;
const SLOWEST: f32 = 5.0;
const FASTEST: f32 = 2000.0;
/// How much faster one notch of the wheel is: a notch is about 100 of the
/// wheel's units on a mouse, and far fewer at a time on a touchpad, so the
/// change is by how far it turned and not by how many times it was heard.
const PER_UNIT: f32 = 0.0025;
/// Radians turned for each point the mouse moves.
const TURN: f32 = 0.005;

/// The direction looked in at a heading and a tilt, and which way is up
/// for a camera looking that way: a quarter turn of tilt further on.
fn facing(heading: f32, tilt: f32) -> (Vec3, Vec3) {
    let at = |tilt: f32| vec3(heading.cos() * tilt.cos(), heading.sin() * tilt.cos(), tilt.sin());
    (at(tilt), at(tilt + std::f32::consts::FRAC_PI_2))
}

impl Controls {
    /// For a camera that is the right way up.
    pub fn new(camera: &Camera) -> Controls {
        let view = camera.view_direction();
        Controls { held: [false; 7], speed: SPEED, heading: view.y.atan2(view.x), tilt: view.z.clamp(-1.0, 1.0).asin() }
    }

    /// Turn the camera, where it stands, to look along `towards`: the way
    /// the cursor points. It comes out the right way up.
    pub fn face(&mut self, camera: &mut Camera, towards: Vec3) {
        if towards.magnitude() < 1e-6 {
            return;
        }
        let towards = towards.normalize();
        (self.heading, self.tilt) = (towards.y.atan2(towards.x), towards.z.clamp(-1.0, 1.0).asin());
        self.aim(camera);
    }

    /// Put the camera at `position`, looking along `towards`, the right
    /// way up: where a flight it has been sent on has it this frame.
    pub fn place(&mut self, camera: &mut Camera, position: Vec3, towards: Vec3) {
        let (view, up) = facing(self.heading, self.tilt);
        camera.set_view(position, position + view, up);
        self.face(camera, towards);
    }

    fn aim(&self, camera: &mut Camera) {
        let (view, up) = facing(self.heading, self.tilt);
        let at = camera.position();
        camera.set_view(at, at + view, up);
    }

    /// Apply this frame's events. Events already used by the panel or by a
    /// handle are marked `handled` and left alone. `elapsed_ms` is the time
    /// since the last frame, so that speed does not depend on frame rate.
    /// `typing` is whether a text box has the keyboard. Returns the speed
    /// if the wheel changed it, to be shown.
    pub fn update(&mut self, camera: &mut Camera, events: &mut [Event], elapsed_ms: f64, typing: bool) -> Option<f32> {
        let mut changed = None;
        for event in events.iter_mut() {
            match event {
                Event::MouseMotion { delta, button: Some(MouseButton::Right), handled, .. } if !*handled => {
                    // Turned as a heading about the upright and a tilt from
                    // level, and never rolled: turning about the camera's
                    // own axes, as its `yaw` and `pitch` do, tips the
                    // horizon over a little with every turn. On its back,
                    // the camera sees the heading turn the other way, so the
                    // mouse's sideways travel is taken the other way too and
                    // the view still follows the hand.
                    let upright = if self.tilt.cos() < 0.0 { -1.0 } else { 1.0 };
                    self.heading -= TURN * delta.0 * upright;
                    self.tilt -= TURN * delta.1;
                    self.aim(camera);
                    *handled = true;
                }
                // Only with Shift held: the wheel alone scrolls the panel,
                // and changed the speed as it did. (With Shift, some systems
                // report the wheel as turning sideways.)
                Event::MouseWheel { delta, modifiers, handled, .. } if !*handled && modifiers.shift => {
                    self.speed = faster(self.speed, if delta.1 != 0.0 { delta.1 } else { delta.0 });
                    changed = Some(self.speed);
                    *handled = true;
                }
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
            return changed;
        }
        let [w, a, s, d, space, e, q] = self.held.map(|h| h as i32 as f32);
        let step = step(camera.view_direction(), camera.right_direction(), [w - s, d - a, (space + e).min(1.0) - q], self.speed, elapsed_ms);
        if step != Vec3::zero() {
            camera.translate(step);
        }
        changed
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

/// The speed after the wheel has turned by `wheel`: up for faster. By a
/// share of what it was, so that each notch feels the same at any speed.
fn faster(speed: f32, wheel: f32) -> f32 {
    (speed * (wheel * PER_UNIT).clamp(-1.0, 1.0).exp()).clamp(SLOWEST, FASTEST)
}

/// How far the camera moves in a frame: `ahead`, `aside` and `up` are each
/// -1, 0 or 1. Ahead is the way the camera looks, tilt and all, so that
/// looking at something and holding W goes to it; up is straight up.
fn step(view: Vec3, right: Vec3, [ahead, aside, up]: [f32; 3], speed: f32, elapsed_ms: f64) -> Vec3 {
    let way = view * ahead + vec3(right.x, right.y, 0.0) * aside + vec3(0.0, 0.0, 1.0) * up;
    if way.magnitude() < 1e-6 {
        return Vec3::zero();
    }
    // a long frame (the window dragged, a slow build) is not a long stride
    let seconds = (elapsed_ms as f32 / 1000.0).min(0.1);
    way.normalize() * speed * seconds
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wheel_sets_the_speed_and_nothing_held_goes_nowhere() {
        let (view, right) = (vec3(0.6, 0.0, -0.8), vec3(0.0, -1.0, 0.0));
        assert_eq!(step(view, right, [0.0; 3], SPEED, 16.0), Vec3::zero());
        // a second's worth, in ten frames of a tenth
        let gone = step(view, right, [1.0, 0.0, 0.0], SPEED, 100.0) * 10.0;
        assert!((gone.magnitude() - SPEED).abs() < 1e-3);
        // ahead is where it looks, down the slope of its view
        assert!((gone.normalize() - view).magnitude() < 1e-5);
        // two keys at once are no faster than one
        let both = step(view, right, [1.0, 1.0, 1.0], SPEED, 100.0) * 10.0;
        assert!((both.magnitude() - SPEED).abs() < 1e-3);
        // and a stalled frame is not a leap
        assert!(step(view, right, [1.0, 0.0, 0.0], SPEED, 5000.0).magnitude() <= SPEED * 0.1 + 1e-3);

        // a notch up is faster by the same share at any speed, a notch down undoes it
        let up = faster(SPEED, 100.0);
        assert!(up > SPEED * 1.2 && (faster(up, -100.0) - SPEED).abs() < 1e-3);
        assert!((faster(4.0 * SPEED, 100.0) / (4.0 * SPEED) - up / SPEED).abs() < 1e-4);
        // within bounds however far it is turned, and one wild reading is not a leap
        assert_eq!(faster(SLOWEST, -100.0), SLOWEST);
        assert!((0..200).fold(SPEED, |s, _| faster(s, 100.0)) == FASTEST && (0..200).fold(SPEED, |s, _| faster(s, -100.0)) == SLOWEST);
        assert!(faster(SPEED, 1.0e6) < SPEED * 3.0);

        // Looking any way at all, up is square to the view and of length
        // one, with no way of looking where it is not: straight up, on its
        // back, and all the way round to where it began.
        for step in 0..=48 {
            let tilt = step as f32 * std::f32::consts::TAU / 48.0;
            let (view, up) = facing(0.7, tilt);
            assert!(view.dot(up).abs() < 1e-5 && (up.magnitude() - 1.0).abs() < 1e-5 && (view.magnitude() - 1.0).abs() < 1e-5);
        }
        // level, up is the world's; a half turn over, it is the world's down
        assert!((facing(0.7, 0.0).1 - vec3(0.0, 0.0, 1.0)).magnitude() < 1e-5);
        assert!((facing(0.7, std::f32::consts::PI).1 - vec3(0.0, 0.0, -1.0)).magnitude() < 1e-5);
    }
}
