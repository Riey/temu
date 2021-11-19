//! Basic wrapper container types for [`wgpu::Buffer`]

mod wgpu_cell;
mod wgpu_vec;

pub use crate::{wgpu_cell::WgpuCell, wgpu_vec::WgpuVec};
