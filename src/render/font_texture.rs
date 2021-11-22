use ahash::AHashMap;
use swash::{
    scale::{image::Image, Render, ScaleContext, Source, StrikeWith},
    FontRef,
};

use crate::render::atlas::ArrayAllocator;

use super::{TEXTURE_SIZE, TEXTURE_WIDTH};

pub struct FontTexture {
    pub font: FontRef<'static>,
    pub data: Vec<u8>,
    pub glyph_cache: AHashMap<u16, GlyphCacheInfo>,
    pub layer_count: u32,
}

impl FontTexture {
    pub fn new(font: FontRef<'static>, font_size: f32) -> Self {
        let mut allocator = ArrayAllocator::new(TEXTURE_WIDTH, TEXTURE_WIDTH);

        let mut glyph_cache = AHashMap::new();
        let mut scale_ctx = ScaleContext::new();
        let mut image = Image::new();
        let mut data = vec![0; TEXTURE_SIZE * 2];
        let mut layer_count = 1;

        let mut scaler = scale_ctx.builder(font).hint(true).size(font_size).build();

        {
            profiling::scope!("Create font texture");

            font.charmap().enumerate(|_c, id| {
                image.clear();
                if Render::new(&[
                    Source::ColorBitmap(StrikeWith::BestFit),
                    Source::ColorOutline(0),
                    Source::Bitmap(StrikeWith::BestFit),
                    Source::Outline,
                ])
                .render_into(&mut scaler, id, &mut image)
                {
                    if image.placement.width == 0 || image.placement.height == 0 {
                    } else {
                        let alloc = allocator.alloc(image.placement.width, image.placement.height);
                        if let Some(new_page) = alloc.layer.checked_sub(layer_count) {
                            data.extend(
                                std::iter::repeat(0).take(TEXTURE_SIZE * new_page as usize),
                            );
                            layer_count += new_page;
                        }
                        let page = &mut data[TEXTURE_SIZE * alloc.layer as usize..][..TEXTURE_SIZE];
                        let left_top = (alloc.y * TEXTURE_WIDTH + alloc.x) as usize;

                        for (row_index, row) in image
                            .data
                            .chunks_exact(image.placement.width as usize)
                            .enumerate()
                        {
                            let begin = left_top + row_index * TEXTURE_WIDTH as usize;
                            let end = begin + row.len();
                            page[begin..end].copy_from_slice(row);
                        }
                        glyph_cache.insert(
                            id,
                            GlyphCacheInfo {
                                tex_position: [alloc.x as _, alloc.y as _],
                                tex_size: [image.placement.width as _, image.placement.height as _],
                                glyph_position: [
                                    image.placement.left as _,
                                    image.placement.top as _,
                                ],
                                layer: alloc.layer as _,
                            },
                        );
                    }
                }
            });
        }

        // use std::io::Write;
        // let mut out = std::fs::OpenOptions::new()
        //     .write(true)
        //     .create(true)
        //     .open("foo.pgm")
        //     .unwrap();
        // write!(out, "P5\n{} {}\n255\n", TEXTURE_WIDTH, TEXTURE_WIDTH).unwrap();
        // out.write_all(&data[..TEXTURE_SIZE]).unwrap();
        // out.flush().unwrap();

        Self {
            font,
            data,
            glyph_cache,
            layer_count: allocator.layer_count(),
        }
    }
}

pub struct GlyphCacheInfo {
    pub tex_position: [f32; 2],
    pub glyph_position: [f32; 2],
    pub tex_size: [f32; 2],
    pub layer: i32,
}
