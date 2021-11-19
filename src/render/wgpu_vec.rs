use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

pub struct WgpuVec<T: Pod + Zeroable> {
    inner: wgpu::Buffer,
    inner_cap: usize,
    usage: wgpu::BufferUsages,
    vec: Vec<T>,
}

const INIT_CAP: usize = 1024;

impl<T: Pod + Zeroable> WgpuVec<T> {
    /// Create new [`WgpuVec`] it will automatically add wgpu::BufferUsages::COPY_DST
    pub fn new(device: &wgpu::Device, usage: wgpu::BufferUsages) -> Self {
        Self {
            inner: device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                mapped_at_creation: false,
                size: (std::mem::size_of::<T>() * INIT_CAP) as u64,
                usage: usage | wgpu::BufferUsages::COPY_DST,
            }),
            inner_cap: INIT_CAP,
            vec: Vec::with_capacity(INIT_CAP),
            usage: usage | wgpu::BufferUsages::COPY_DST,
        }
    }

    pub fn len(&self) -> u32 {
        self.vec.len() as u32
    }

    pub fn as_slice(&self) -> &[T] {
        self.vec.as_slice()
    }

    pub fn as_vec_mut(&mut self) -> &mut Vec<T> {
        &mut self.vec
    }

    pub fn slice(&self, bounds: impl std::ops::RangeBounds<u64>) -> wgpu::BufferSlice<'_> {
        self.inner.slice(bounds)
    }

    pub fn write(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        if self.inner_cap < self.vec.len() {
            self.inner_cap *= 2;
            self.inner = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::cast_slice(&self.vec),
                usage: self.usage,
            });
        } else {
            queue.write_buffer(&self.inner, 0, bytemuck::cast_slice(&self.vec));
        }
    }
}
