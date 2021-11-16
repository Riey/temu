use crossbeam_channel::Sender;
use raw_window_handle::HasRawWindowHandle;
use winit::dpi::LogicalSize;
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop, EventLoopWindowTarget};
use winit::window::{Window, WindowBuilder};

use crate::{TemuEvent, TemuPtyEvent};

pub struct WinitWindow {
    inner: Window,
    event_loop: EventLoop<()>,
    event_tx: Sender<TemuEvent>,
    pty_event_tx: Sender<TemuPtyEvent>,
}

unsafe impl HasRawWindowHandle for WinitWindow {
    fn raw_window_handle(&self) -> raw_window_handle::RawWindowHandle {
        self.inner.raw_window_handle()
    }
}

impl crate::TemuWindow for WinitWindow {
    fn init(event_tx: Sender<TemuEvent>, pty_event_tx: Sender<TemuPtyEvent>) -> Self {
        let event_loop = EventLoop::new();
        let inner = WindowBuilder::new()
            .with_inner_size(LogicalSize::new(600, 400))
            .with_title("Temu")
            // // for debug purpose
            // .with_always_on_top(true)
            .build(&event_loop)
            .unwrap();

        let factor = inner.scale_factor();

        event_tx
            .send(TemuEvent::Resize {
                width: (600.0 * factor) as u32,
                height: (400.0 * factor) as u32,
            })
            .unwrap();

        Self {
            inner,
            event_loop,
            event_tx,
            pty_event_tx,
        }
    }

    fn run(self) {
        let Self {
            inner,
            event_loop,
            pty_event_tx,
            event_tx,
        } = self;

        event_loop.run(move |e, _target, flow| {
            match e {
                Event::RedrawRequested(_) => {
                    event_tx.send(TemuEvent::Redraw).ok();
                }
                Event::WindowEvent { event, .. } => match event {
                    WindowEvent::CloseRequested => {
                        event_tx.send(TemuEvent::Close).ok();
                        *flow = ControlFlow::Exit;
                        return;
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
                        pty_event_tx.send(TemuPtyEvent::Char(c)).ok();
                    }
                    _ => {}
                },
                _ => {}
            }
            *flow = ControlFlow::Wait;
        });
    }
}
