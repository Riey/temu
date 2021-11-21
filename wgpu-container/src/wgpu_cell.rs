use bytemuck::{cast_slice, Pod};
use std::{ops::Deref, slice::from_ref};
use wgpu::util::DeviceExt;

/// Wrapper around `T`
pub struct WgpuCell<T> {
    value: T,
    inner: wgpu::Buffer,
    outdated: bool,
}

impl<T: Pod> WgpuCell<T> {
    /// Create new [`WgpuCell`] with usage and value it will automatically add [`wgpu::BufferUsages::COPY_DST`]
    pub fn new(device: &wgpu::Device, usage: wgpu::BufferUsages, value: T) -> Self {
        Self {
            inner: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                contents: cast_slice(from_ref(&value)),
                label: None,
                usage: usage | wgpu::BufferUsages::COPY_DST,
            }),
            value,
            outdated: false,
        }
    }

    /// Create new [`WgpuCell`] with usage and zeroed value it will automatically add [`wgpu::BufferUsages::COPY_DST`]
    pub fn zeroed(device: &wgpu::Device, usage: wgpu::BufferUsages) -> Self {
        Self::new(device, usage, T::zeroed())
    }

    /// Get underlying
    pub fn buffer(&self) -> &wgpu::Buffer {
        &self.inner
    }

    /// Get mutable reference underlying value
    ///
    /// Caller should call [`WgpuCell::write`] to update gpu-buffer
    pub fn as_mut(&mut self) -> &mut T {
        self.outdated = true;
        &mut self.value
    }

    /// Update inner value and write changes
    pub fn update<'a, 'b: 'a, R>(&'b mut self, f: impl FnOnce(&'a mut T) -> R) -> R {
        self.outdated = true;
        f(&mut self.value)
    }

    /// Write value to gpu-buffer
    ///
    /// If buffer is up to date, it won't do write
    pub fn flush(&mut self, queue: &wgpu::Queue) {
        if self.outdated {
            queue.write_buffer(&self.inner, 0, cast_slice(from_ref(&self.value)));
        }

        self.outdated = false;
    }
}

impl<T> Deref for WgpuCell<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}
