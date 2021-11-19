use bytemuck::{cast_slice, Pod, Zeroable};
use std::{ops::Deref, slice::from_ref};
use wgpu::util::DeviceExt;

pub struct WgpuCell<T> {
    value: T,
    inner: wgpu::Buffer,
}

impl<T: Pod + Zeroable> WgpuCell<T> {
    pub fn new(device: &wgpu::Device, usage: wgpu::BufferUsages, value: T) -> Self {
        Self {
            inner: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                contents: cast_slice(from_ref(&value)),
                label: None,
                usage: usage | wgpu::BufferUsages::COPY_DST,
            }),
            value,
        }
    }

    pub fn buffer(&self) -> &wgpu::Buffer {
        &self.inner
    }

    pub fn update<R>(&mut self, queue: &wgpu::Queue, f: impl FnOnce(&mut T) -> R) -> R {
        let ret = f(&mut self.value);
        queue.write_buffer(&self.inner, 0, cast_slice(from_ref(&self.value)));
        ret
    }
}

impl<T> Deref for WgpuCell<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}
