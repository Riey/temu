use wayland_client::{protocol::wl_surface, Display};

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

    pub fn surface(&self) -> &wgpu::Surface {
        &self.surface
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
            present_mode: wgpu::PresentMode::Mailbox,
        };

        self.surface.configure(device, &config);

        Viewport { desc: self, config }
    }
}

pub struct Viewport {
    desc: ViewportDesc,
    config: wgpu::SurfaceConfiguration,
}

impl Viewport {
    pub fn background(&self) -> wgpu::Color {
        self.desc.background
    }

    pub fn foreground(&self) -> wgpu::Color {
        self.desc.foreground
    }

    pub fn width(&self) -> u32 {
        self.config.width
    }

    pub fn height(&self) -> u32 {
        self.config.width
    }

    pub fn format(&self) -> wgpu::TextureFormat {
        self.config.format
    }

    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.config.width = width.max(300);
        self.config.height = height.max(200);
        self.desc.surface.configure(device, &self.config);
    }

    pub fn get_current_texture(&mut self) -> wgpu::SurfaceTexture {
        self.desc
            .surface
            .get_current_texture()
            .expect("Failed to acquire next swap chain texture")
    }
}

pub struct WindowHandle {
    handle: raw_window_handle::unix::WaylandHandle,
}

unsafe impl Send for WindowHandle {}

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
