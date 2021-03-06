pub struct Viewport {
    surface: wgpu::Surface,
    config: wgpu::SurfaceConfiguration,
}

impl Viewport {
    pub fn new(
        width: u32,
        height: u32,
        adapter: &wgpu::Adapter,
        device: &wgpu::Device,
        surface: wgpu::Surface,
    ) -> Self {
        let render_format = surface
            .get_preferred_format(adapter)
            .unwrap_or(wgpu::TextureFormat::Bgra8UnormSrgb);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: render_format,
            width,
            height,
            present_mode: wgpu::PresentMode::Mailbox,
        };

        surface.configure(device, &config);

        Viewport { surface, config }
    }

    pub fn width(&self) -> u32 {
        self.config.width
    }

    pub fn height(&self) -> u32 {
        self.config.height
    }

    pub fn format(&self) -> wgpu::TextureFormat {
        self.config.format
    }

    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.config.width = width.max(300);
        self.config.height = height.max(200);
        self.surface.configure(device, &self.config);
    }

    pub fn get_current_texture(&mut self) -> Option<wgpu::SurfaceTexture> {
        match self.surface.get_current_texture() {
            Ok(t) => Some(t),
            Err(wgpu::SurfaceError::Outdated) => None,
            Err(err) => {
                panic!("Surface error: {}", err);
            }
        }
    }
}
