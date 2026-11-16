//! The desktop window, with one thing three-d's own does not give: the
//! files dropped on it.
//!
//! three-d's `Window::render_loop` owns winit's event loop and hands each
//! frame the events it knows (keys, the mouse, the wheel). A file dropped
//! on the window is an event winit reports and three-d passes over:
//! <https://docs.rs/winit/0.28/winit/event/enum.WindowEvent.html#variant.DroppedFile>.
//! Nothing in three-d's API reaches it, so this is `render_loop` written
//! out again from three-d 0.19's `window/winit_window.rs` with that one
//! event added, using the two parts three-d offers for exactly this
//! (`WindowedContext` and `FrameInputGenerator`: "if this is not desired,
//! use a WindowedContext instead").
//!
//! The alternatives were a file dialog, which is a crate for each of
//! three systems' dialogs (`rfd`) to do what dropping a file does, and a
//! hook on Windows' own messages, which needs `unsafe` and is one system
//! only. Typing a path in the panel is the way that needs nothing, and it
//! stays.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use three_d::{Context, FrameInput, FrameInputGenerator, FrameOutput, SurfaceSettings, Viewport, WindowError, WindowedContext};
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};

/// A window that is drawn in and reports the files dropped on it.
pub struct Desk {
    window: winit::window::Window,
    event_loop: EventLoop<()>,
    gl: WindowedContext,
    dropped: Rc<RefCell<Vec<PathBuf>>>,
}

impl Desk {
    /// As `three_d::Window::from_winit_window`: with no multisampling if
    /// the system will not give a surface with it.
    pub fn from_winit_window(window: winit::window::Window, event_loop: EventLoop<()>, mut settings: SurfaceSettings) -> Result<Desk, WindowError> {
        let mut gl = WindowedContext::from_winit_window(&window, settings);
        if gl.is_err() {
            settings.multisamples = 0;
            gl = WindowedContext::from_winit_window(&window, settings);
        }
        Ok(Desk { window, event_loop, gl: gl?, dropped: Rc::default() })
    }

    pub fn gl(&self) -> Context {
        (*self.gl).clone()
    }

    pub fn viewport(&self) -> Viewport {
        let (width, height): (u32, u32) = self.window.inner_size().into();
        Viewport::new_at_origo(width, height)
    }

    /// Where the paths of dropped files are put, to be taken by whoever
    /// draws the frames: asked for before [`Desk::render_loop`], which
    /// does not return.
    pub fn dropped(&self) -> Rc<RefCell<Vec<PathBuf>>> {
        self.dropped.clone()
    }

    /// Call `frame` for every frame, for as long as the window is open.
    pub fn render_loop<F: 'static + FnMut(FrameInput) -> FrameOutput>(self, mut frame: F) {
        let Desk { window, event_loop, gl, dropped } = self;
        let mut input = FrameInputGenerator::from_winit_window(&window);
        event_loop.run(move |event, _, control_flow| match event {
            Event::MainEventsCleared => window.request_redraw(),
            Event::RedrawRequested(_) => {
                let output = frame(input.generate(&gl));
                if output.exit {
                    *control_flow = ControlFlow::Exit;
                    return;
                }
                if output.swap_buffers {
                    // (a frame that could not be shown is tried again with the next: the window may be between sizes)
                    let _ = gl.swap_buffers();
                }
                if output.wait_next_event {
                    *control_flow = ControlFlow::Wait;
                } else {
                    *control_flow = ControlFlow::Poll;
                    window.request_redraw();
                }
            }
            Event::WindowEvent { ref event, .. } => {
                input.handle_winit_window_event(event);
                match event {
                    WindowEvent::Resized(size) => gl.resize(*size),
                    WindowEvent::ScaleFactorChanged { new_inner_size, .. } => gl.resize(**new_inner_size),
                    WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
                    // (several files dropped together come one event each)
                    WindowEvent::DroppedFile(path) => dropped.borrow_mut().push(path.clone()),
                    _ => {}
                }
            }
            _ => {}
        });
    }
}
