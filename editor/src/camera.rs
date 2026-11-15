//! Moving the view: right-drag turns it about its focus, Shift + left-drag
//! slides it sideways, the wheel zooms, W A S D walk the camera over the
//! ground, and Space and Shift raise and lower it.
//!
//! three-d's own `OrbitControl` turns the view on a plain left-drag, which
//! is the button the handles are dragged with, and has no panning or keys;
//! this is the same turning and zooming with the buttons moved and the rest
//! added. It uses the camera's own functions throughout:
//! https://docs.rs/three-d-asset/latest/three_d_asset/camera/struct.Camera.html

use three_d::*;

pub struct Controls {
    /// the point the view turns about and zooms towards
    pub focus: Vec3,
    /// W, A, S, D, Space: whether each is held down
    held: [bool; 5],
    shift: bool,
    /// Whether the left mouse button is down. Shift also slides the view
    /// when dragged with it, and must not sink the camera at the same time.
    dragging: bool,
}

const NEAREST: f32 = 3.0;
const FURTHEST: f32 = 2500.0;

impl Controls {
    pub fn new(focus: Vec3) -> Controls {
        Controls { focus, held: [false; 5], shift: false, dragging: false }
    }

    /// Apply this frame's events. Events already used by the panel or by a
    /// handle are marked `handled` and left alone. `elapsed_ms` is the time
    /// since the last frame, so that walking speed does not depend on frame
    /// rate. `typing` is whether a text box has the keyboard.
    pub fn update(&mut self, camera: &mut Camera, events: &mut [Event], elapsed_ms: f64, typing: bool) {
        for event in events.iter_mut() {
            match event {
                Event::MouseMotion { delta, button: Some(MouseButton::Right), handled, .. } if !*handled => {
                    camera.rotate_around_with_fixed_up(self.focus, 0.01 * delta.0, 0.01 * delta.1);
                    *handled = true;
                }
                Event::MouseMotion { delta, button: Some(MouseButton::Left), modifiers, handled, .. } if !*handled && modifiers.shift => {
                    // slide so that what is under the cursor roughly stays
                    // there: further away, a pixel covers more ground
                    let reach = self.focus.distance(camera.position());
                    let slide = (camera.up() * delta.1 - camera.right_direction() * delta.0) * (reach * 0.0015);
                    self.shift(camera, slide);
                    *handled = true;
                }
                Event::MouseWheel { delta, handled, .. } if !*handled => {
                    let reach = self.focus.distance(camera.position());
                    camera.zoom_towards(self.focus, reach * (1.0 - (-delta.1 * 0.01).exp()), NEAREST, FURTHEST);
                    *handled = true;
                }
                Event::MousePress { button: MouseButton::Left, .. } => self.dragging = true,
                Event::MouseRelease { button: MouseButton::Left, .. } => self.dragging = false,
                Event::ModifiersChange { modifiers } => self.shift = modifiers.shift,
                Event::KeyPress { kind, handled, .. } if !*handled => self.key(*kind, true),
                // a release always counts, or a key let go while the panel
                // had the keyboard would stay "held" and the view drift away
                Event::KeyRelease { kind, .. } => self.key(*kind, false),
                _ => {}
            }
        }

        if typing {
            // Shift for a capital letter must not sink the camera, and a key
            // released into a text box must not stay held
            self.held = [false; 5];
            return;
        }
        let [w, a, s, d, up] = self.held.map(|h| h as i32 as f32);
        let down = (self.shift && !self.dragging) as i32 as f32;
        if w != s || a != d || up != down {
            // over the ground, whichever way the camera is tilted: Z is up
            let view = camera.view_direction();
            let ahead = vec3(view.x, view.y, 0.0);
            let ahead = if ahead.magnitude() > 1e-3 { ahead.normalize() } else { vec3(camera.up().x, camera.up().y, 0.0).normalize() };
            let right = camera.right_direction();
            let reach = self.focus.distance(camera.position()).max(8.0);
            let step = (ahead * (w - s) + vec3(right.x, right.y, 0.0) * (d - a) + vec3(0.0, 0.0, 1.0) * (up - down)) * (reach * 0.9 * (elapsed_ms as f32 / 1000.0).min(0.1));
            self.shift(camera, step);
        }
    }

    /// Whether a key is held, so the view needs redrawing every frame.
    pub fn moving(&self) -> bool {
        self.held.iter().any(|h| *h) || self.shift
    }

    fn key(&mut self, key: Key, down: bool) {
        let slot = match key {
            Key::W => 0,
            Key::A => 1,
            Key::S => 2,
            Key::D => 3,
            Key::Space => 4,
            _ => return,
        };
        self.held[slot] = down;
    }

    /// Move the camera and its focus together.
    fn shift(&mut self, camera: &mut Camera, by: Vec3) {
        camera.translate(by);
        self.focus += by;
    }
}
