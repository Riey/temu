use crossbeam_channel::Sender;
use raw_window_handle::HasRawWindowHandle;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, Event, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::{Window, WindowBuilder};

use crate::TemuEvent;

pub struct WinitWindow {
    inner: Window,
    event_loop: EventLoop<()>,
    event_tx: Sender<TemuEvent>,
}

pub struct WinitHandle {
    handle: raw_window_handle::RawWindowHandle,
}

unsafe impl Send for WinitHandle {}

unsafe impl HasRawWindowHandle for WinitHandle {
    fn raw_window_handle(&self) -> raw_window_handle::RawWindowHandle {
        self.handle
    }
}

impl crate::TemuWindow for WinitWindow {
    type Handle = WinitHandle;

    fn get_raw_event_handle(&self) -> Self::Handle {
        WinitHandle {
            handle: self.inner.raw_window_handle(),
        }
    }

    fn init(event_tx: Sender<TemuEvent>) -> Self {
        let event_loop = EventLoop::new();
        let inner = WindowBuilder::new()
            .with_inner_size(LogicalSize::new(720u32, 400u32))
            .with_title("Temu")
            .with_transparent(true)
            // // for debug purpose
            // .with_always_on_top(true)
            .build(&event_loop)
            .unwrap();

        Self {
            inner,
            event_loop,
            event_tx,
        }
    }

    fn size(&self) -> (u32, u32) {
        let size = self.inner.inner_size();
        (size.width, size.height)
    }

    fn scale_factor(&self) -> f32 {
        self.inner.scale_factor() as f32
    }

    #[profiling::function]
    fn run(self) {
        let Self {
            inner: _,
            event_loop,
            event_tx,
        } = self;

        event_loop.run(move |e, _target, flow| match e {
            Event::DeviceEvent { .. } => *flow = ControlFlow::Wait,
            Event::RedrawRequested(_) => {
                event_tx.send(TemuEvent::Redraw).ok();
            }
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => {
                    event_tx.send(TemuEvent::Close).ok();
                    *flow = ControlFlow::Exit;
                }
                WindowEvent::Resized(size) => {
                    event_tx
                        .send(TemuEvent::Resize {
                            width: size.width,
                            height: size.height,
                        })
                        .ok();
                }
                WindowEvent::ReceivedCharacter(c) => {
                    event_tx.send(TemuEvent::Char(c)).ok();
                }
                WindowEvent::MouseInput {
                    button: MouseButton::Left,
                    state,
                    ..
                } => {
                    event_tx
                        .send(TemuEvent::Left(state == ElementState::Pressed))
                        .ok();
                }
                WindowEvent::CursorMoved { position, .. } => {
                    event_tx
                        .send(TemuEvent::CursorMove {
                            x: position.x as f32,
                            y: position.y as f32,
                        })
                        .ok();
                }
                WindowEvent::MouseWheel { delta, .. } => match delta {
                    MouseScrollDelta::LineDelta(_, y) => {
                        if y > 0.0 {
                            event_tx.send(TemuEvent::ScrollUp).ok();
                        } else if y < 0.0 {
                            event_tx.send(TemuEvent::ScrollDown).ok();
                        }
                    }
                    MouseScrollDelta::PixelDelta(p) => {
                        log::info!("{:?}", p);
                    }
                },
                _ => {}
            },
            _ => {}
        });
    }
}
