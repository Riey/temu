use bytemuck::Pod;

/// Wrapper around `Vec<T>`
pub struct WgpuVec<T> {
    cpu_buffer: Vec<T>,
    inner: wgpu::Buffer,
    inner_cap: usize,
    usage: wgpu::BufferUsages,
}

impl<T: Pod> WgpuVec<T> {
    /// Create new [`WgpuVec`] with usage it will automatically add [`wgpu::BufferUsages::COPY_DST`]
    #[inline]
    pub fn new(device: &wgpu::Device, usage: wgpu::BufferUsages) -> Self {
        Self::with_capacity(device, usage, 256)
    }

    /// Create new [`WgpuVec`] with usage and capacity it will automatically add [`wgpu::BufferUsages::COPY_DST`]
    pub fn with_capacity(
        device: &wgpu::Device,
        usage: wgpu::BufferUsages,
        capacity: usize,
    ) -> Self {
        // capacity should be more than zero
        let capacity = capacity.max(1);

        Self {
            inner: device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                mapped_at_creation: false,
                size: (std::mem::size_of::<T>() * capacity) as u64,
                usage: usage | wgpu::BufferUsages::COPY_DST,
            }),
            inner_cap: capacity,
            cpu_buffer: Vec::with_capacity(capacity),
            usage: usage | wgpu::BufferUsages::COPY_DST,
        }
    }

    /// Returns the number of elements in the cpu buffer.
    #[inline]
    pub fn len(&self) -> usize {
        self.cpu_buffer.len()
    }

    /// Returns the capacity of gpu buffer.
    #[inline]
    pub fn gpu_capacity(&self) -> usize {
        self.inner_cap
    }

    /// Get inner [`wgpu::Buffer`]
    #[inline]
    pub fn gpu_buffer(&self) -> &wgpu::Buffer {
        &self.inner
    }

    /// Extracts a slice containing the entire cpu buffer.
    #[inline]
    pub fn cpu_buffer(&self) -> &[T] {
        self.cpu_buffer.as_slice()
    }

    /// Get mutable reference underlying cpu buffer.
    ///
    /// Caller should call [`WgpuVec::write`] later for update gpu buffer
    #[inline]
    pub fn cpu_buffer_mut(&mut self) -> &mut Vec<T> {
        &mut self.cpu_buffer
    }

    /// Write cpu-buffer to gpu-buffer
    ///
    /// It will reuse gpu-buffer when capacity is bigger than cpu-buffer
    pub fn write(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        if self.inner_cap < self.cpu_buffer.len() {
            while self.inner_cap < self.cpu_buffer.len() {
                self.inner_cap *= 2;
            }
            self.inner = device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                usage: self.usage,
                mapped_at_creation: false,
                size: (self.inner_cap * std::mem::size_of::<T>()) as u64,
            });
        }

        queue.write_buffer(&self.inner, 0, bytemuck::cast_slice(&self.cpu_buffer));
    }
}
