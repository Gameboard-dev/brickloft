//! Moving the view: the camera is flown. Right-drag turns it where it
//! stands, a middle click turns it to face whatever the cursor is on,
//! W A S D move it the way it looks and sideways, Space or E raise it,
//! Q lowers it, and Shift held makes it go faster.
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
//! brought nearer by moving it.
//!
//! It has two speeds: its usual one, and a faster one while Shift is
//! held, as the game's own fly mode has. For a while the wheel set the
//! speed, with Shift held, to any of a range: finer, but a setting to be
//! kept in mind and put back, where a key held is fast for as long as it
//! is wanted and no longer. (With Ctrl the wheel still resizes the form
//! being edited.)
//!
//! three-d's own `FlyControl` turns on a plain left-drag, which is the
//! button the handles are dragged with, and has no keys. This uses the
//! camera's own functions:
//! https://docs.rs/three-d-asset/latest/three_d_asset/camera/struct.Camera.html

use three_d::*;

pub struct Controls {
    /// W, A, S, D, up (Space or E), down (Q): whether each is held down
    held: [bool; 7],
    /// whether Shift is held, which is the faster of its two speeds
    fast: bool,
    /// which way it looks: round the upright from +x, and up from level.
    /// The tilt is not kept within a quarter turn: past it, the camera is
    /// on its back.
    heading: f32,
    tilt: f32,
}

/// Its usual speed, in microbricks a second, and how many times faster it
/// goes with Shift held: across a large build in a few seconds.
const SPEED: f32 = 80.0;
const FAST: f32 = 4.0;
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
        Controls { held: [false; 7], fast: false, heading: view.y.atan2(view.x), tilt: view.z.clamp(-1.0, 1.0).asin() }
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
    /// `typing` is whether a text box has the keyboard.
    pub fn update(&mut self, camera: &mut Camera, events: &mut [Event], elapsed_ms: f64, typing: bool) {
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
                // (Shift, whenever it is pressed or let go, and as every key
                // pressed reports it: a window that loses the keyboard with
                // Shift down may never hear it come up)
                Event::ModifiersChange { modifiers } => self.fast = modifiers.shift,
                Event::KeyPress { kind, modifiers, handled, .. } if !*handled => {
                    self.fast = modifiers.shift;
                    self.key(*kind, true)
                }
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
        let step = step(camera.view_direction(), camera.right_direction(), [w - s, d - a, (space + e).min(1.0) - q], self.speed(), elapsed_ms);
        if step != Vec3::zero() {
            camera.translate(step);
        }
    }

    /// How fast it goes now, in microbricks a second.
    fn speed(&self) -> f32 {
        SPEED * if self.fast { FAST } else { 1.0 }
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
    fn shift_held_is_the_faster_of_two_speeds_and_nothing_held_goes_nowhere() {
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

        // Shift held is the faster speed, and let go the usual one again;
        // the wheel, with or without Shift, changes neither
        let mut controls = Controls { held: [false; 7], fast: false, heading: 0.0, tilt: 0.0 };
        assert_eq!(controls.speed(), SPEED);
        let shift = |down: bool| Event::ModifiersChange { modifiers: Modifiers { shift: down, ..Default::default() } };
        let mut camera = Camera::new_perspective(Viewport::new_at_origo(100, 100), vec3(0.0, 0.0, 0.0), vec3(1.0, 0.0, 0.0), vec3(0.0, 0.0, 1.0), degrees(45.0), 0.1, 100.0);
        controls.update(&mut camera, &mut [shift(true)], 16.0, false);
        assert_eq!(controls.speed(), SPEED * FAST);
        let mut wheel = [Event::MouseWheel { delta: (0.0, 300.0), position: PhysicalPoint { x: 0.0, y: 0.0 }, modifiers: Modifiers { shift: true, ..Default::default() }, handled: false }];
        controls.update(&mut camera, &mut wheel, 16.0, false);
        assert!(controls.speed() == SPEED * FAST && matches!(wheel[0], Event::MouseWheel { handled: false, .. }));
        controls.update(&mut camera, &mut [shift(false)], 16.0, false);
        assert_eq!(controls.speed(), SPEED);

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
