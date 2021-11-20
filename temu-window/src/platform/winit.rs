use crossbeam_channel::Sender;
use raw_window_handle::HasRawWindowHandle;
use winit::dpi::LogicalSize;
use winit::event::{Event, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::{Window, WindowBuilder};

use crate::TemuEvent;

pub struct WinitWindow {
    inner: Window,
    event_loop: EventLoop<()>,
    event_tx: Sender<TemuEvent>,
}

unsafe impl HasRawWindowHandle for WinitWindow {
    fn raw_window_handle(&self) -> raw_window_handle::RawWindowHandle {
        self.inner.raw_window_handle()
    }
}

impl crate::TemuWindow for WinitWindow {
    fn init(event_tx: Sender<TemuEvent>) -> Self {
        let event_loop = EventLoop::new();
        let inner = WindowBuilder::new()
            .with_inner_size(LogicalSize::new(720u32, 400u32))
            .with_title("Temu")
            .with_transparent(true)
            // for debug purpose
            .with_always_on_top(true)
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
