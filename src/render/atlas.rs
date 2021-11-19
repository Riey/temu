use etagere::{BucketedAtlasAllocator, Size};

#[derive(Clone, Copy, Default)]
pub struct Allocation {
    pub x: u32,
    pub y: u32,
    pub layer: u32,
}

pub struct ArrayAllocator {
    inner: Vec<BucketedAtlasAllocator>,
    size: Size,
}

impl ArrayAllocator {
    pub fn new(width: u32, height: u32) -> Self {
        let size = Size::new(width as _, height as _);
        Self {
            inner: vec![BucketedAtlasAllocator::new(size); 2],
            size,
        }
    }

    pub fn layer_count(&self) -> u32 {
        self.inner.len() as u32
    }

    pub fn alloc(&mut self, width: u32, height: u32) -> Allocation {
        let alloc_size = Size::new(width as _, height as _);

        for (layer, allocator) in self.inner.iter_mut().enumerate() {
            if let Some(alloc) = allocator.allocate(alloc_size) {
                let [x, y] = alloc.rectangle.min.to_u32().to_array();
                return Allocation {
                    x,
                    y,
                    layer: layer as u32,
                };
            }
        }

        let layer = self.inner.len();

        let mut new_allocator = BucketedAtlasAllocator::new(self.size);
        let alloc = new_allocator
            .allocate(alloc_size)
            .expect("Texture is too small");
        let [x, y] = alloc.rectangle.min.to_u32().to_array();
        self.inner.push(new_allocator);
        Allocation {
            x,
            y,
            layer: layer as u32,
        }
    }
}
