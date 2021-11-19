use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

pub struct WgpuVec<T: Pod + Zeroable> {
    inner: wgpu::Buffer,
    inner_cap: usize,
    usage: wgpu::BufferUsages,
    buffer_len: u32,
    buffer: Vec<T>,
}

const INIT_CAP: usize = 1024;

impl<T: Pod + Zeroable> WgpuVec<T> {
    pub fn new(device: &wgpu::Device, usage: wgpu::BufferUsages) -> Self {
        Self {
            inner: device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                mapped_at_creation: false,
                size: (std::mem::size_of::<T>() * INIT_CAP) as u64,
                usage: usage | wgpu::BufferUsages::COPY_DST,
            }),
            inner_cap: INIT_CAP,
            buffer_len: 0,
            buffer: Vec::new(),
            usage: usage | wgpu::BufferUsages::COPY_DST,
        }
    }

    pub fn slice(&self, bounds: impl std::ops::RangeBounds<u64>) -> wgpu::BufferSlice<'_> {
        self.inner.slice(bounds)
    }

    pub fn push(&mut self, item: T) {
        self.buffer.push(item);
    }

    pub fn buffer_len(&self) -> u32 {
        self.buffer_len
    }

    pub fn flush(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        if self.inner_cap < self.buffer.len() {
            self.inner_cap *= 2;
            self.inner = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::cast_slice(&self.buffer),
                usage: self.usage,
            });
        } else {
            queue.write_buffer(&self.inner, 0, bytemuck::cast_slice(&self.buffer));
        }

        self.buffer_len = self.buffer.len() as u32;
        self.buffer.clear();
    }
}
