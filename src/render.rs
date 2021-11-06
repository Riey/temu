use crate::event::TemuEvent;
use wayland_client::{protocol::wl_surface, Display, EventQueue};
use wgpu_glyph::{
    ab_glyph::{Font, FontRef},
    GlyphBrush, GlyphBrushBuilder, Section, Text,
};

const FONT: &[u8] = include_bytes!("/nix/store/imnk1n6llkh089xgzqyqpr6yw9qz9b3z-d2codingfont-1.3.2/share/fonts/truetype/D2Coding-Ver1.3.2-20180524-all.ttc");
const SHADER: &str = include_str!("../shaders/shader.wgsl");

pub struct ViewportDesc {
    surface: wgpu::Surface,
    background: wgpu::Color,
    foreground: wgpu::Color,
}

impl ViewportDesc {
    pub fn new(instance: &wgpu::Instance, handle: &WindowHandle) -> Self {
        unsafe {
            Self {
                surface: instance.create_surface(handle),
                background: wgpu::Color::BLACK,
                foreground: wgpu::Color::WHITE,
            }
        }
    }

    pub fn build(
        self,
        width: u32,
        height: u32,
        adapter: &wgpu::Adapter,
        device: &wgpu::Device,
    ) -> Viewport {
        let render_format = self
            .surface
            .get_preferred_format(adapter)
            .unwrap_or(wgpu::TextureFormat::Bgra8UnormSrgb);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: render_format,
            width: width.max(300),
            height: height.max(200),
            present_mode: wgpu::PresentMode::Fifo,
        };

        self.surface.configure(device, &config);

        Viewport {
            glyph: GlyphBrushBuilder::using_font(FontRef::try_from_slice(FONT).unwrap())
                .build(device, render_format),
            desc: self,
            config,
        }
    }
}

pub struct Viewport {
    desc: ViewportDesc,
    glyph: GlyphBrush<(), FontRef<'static>>,
    config: wgpu::SurfaceConfiguration,
}

impl Viewport {
    fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.config.width = width.max(300);
        self.config.height = height.max(200);
        self.desc.surface.configure(device, &self.config);
    }

    fn get_current_texture(&mut self) -> wgpu::SurfaceTexture {
        self.desc
            .surface
            .get_current_texture()
            .expect("Failed to acquire next swap chain texture")
    }
}

pub struct WindowHandle {
    handle: raw_window_handle::unix::WaylandHandle,
}

impl WindowHandle {
    pub fn new(surface: &wl_surface::WlSurface, display: &Display) -> Self {
        let mut handle = raw_window_handle::unix::WaylandHandle::empty();
        handle.surface = surface.as_ref().c_ptr().cast();
        handle.display = display.get_display_ptr().cast();

        Self { handle }
    }
}

unsafe impl raw_window_handle::HasRawWindowHandle for WindowHandle {
    fn raw_window_handle(&self) -> raw_window_handle::RawWindowHandle {
        raw_window_handle::RawWindowHandle::Wayland(self.handle)
    }
}

pub struct WgpuContext {
    viewport: Viewport,
    device: wgpu::Device,
    queue: wgpu::Queue,
    staging_belt: wgpu::util::StagingBelt,
    shader: wgpu::ShaderModule,
    render_pipeline: wgpu::RenderPipeline,
}

impl WgpuContext {
    pub fn new(viewport: Viewport, device: wgpu::Device, queue: wgpu::Queue) -> Self {
        let shader = device.create_shader_module(&wgpu::ShaderModuleDescriptor {
            label: Some("shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });
        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("text"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[viewport.config.format.into()],
            }),
            primitive: wgpu::PrimitiveState {
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                ..Default::default()
            },
        });

        Self {
            viewport,
            staging_belt: wgpu::util::StagingBelt::new(1024),
            device,
            queue,
            shader,
            render_pipeline,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if self.viewport.config.width != width || self.viewport.config.height != height {
            self.viewport.resize(&self.device, width, height);
            self.redraw();
        }
    }

    pub fn redraw(&mut self) {
        eprintln!("Redraw");
        let frame = self.viewport.get_current_texture();
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(wgpu::TextureFormat::Bgra8Unorm),
            ..Default::default()
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("background"),
                color_attachments: &[wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.viewport.desc.background),
                        store: true,
                    },
                }],
                depth_stencil_attachment: None,
            });

            // rpass.set_pipeline(&self.render_pipeline);
            // rpass.draw(0..3, 0..1);
        }

        {
            let wgpu::Color { a, r, g, b } = self.viewport.desc.foreground;
            let foreground = [a as f32, r as f32, g as f32, b as f32];

            self.viewport.glyph.queue(Section {
                text: vec![Text::new("가나다").with_color(foreground)],
                ..Default::default()
            });
            self.viewport.glyph.draw_queued(
                &self.device,
                &mut self.staging_belt,
                &mut encoder,
                &view,
                self.viewport.config.width,
                self.viewport.config.height,
            );
        }

        self.staging_belt.finish();
        self.queue.submit(Some(encoder.finish()));
        frame.present();
    }
}

pub async fn run(
    handle: WindowHandle,
    mut event_queue: EventQueue,
    event_rx: flume::Receiver<TemuEvent>,
) {
    let instance = wgpu::Instance::new(wgpu::Backends::all());
    let viewport = ViewportDesc::new(&instance, &handle);
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&viewport.surface),
            ..Default::default()
        })
        .await
        .expect("Failed to find an appropriate adapter");

    let (device, queue) = adapter
        .request_device(
            &wgpu::DeviceDescriptor {
                label: None,
                features: wgpu::Features::empty(),
                limits: wgpu::Limits::downlevel_defaults(),
            },
            None,
        )
        .await
        .expect("Failed to create device");

    let mut ctx = WgpuContext::new(viewport.build(300, 200, &adapter, &device), device, queue);

    ctx.redraw();

    loop {
        match event_rx.try_recv() {
            Ok(event) => match event {
                TemuEvent::Close => {
                    break;
                }
                TemuEvent::Resize { width, height } => {
                    ctx.resize(width, height);
                }
            },
            Err(flume::TryRecvError::Disconnected) => {
                break;
            }
            Err(flume::TryRecvError::Empty) => {
                event_queue
                    .dispatch(&mut (), |_, _, _| { /* we ignore unfiltered messages */ })
                    .unwrap();
            }
        }
    }
}
