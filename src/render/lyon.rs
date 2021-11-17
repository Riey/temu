use std::time::Instant;

use ahash::AHashMap;
use bytemuck::{Pod, Zeroable};
use lyon::{
    geom::{Point, Transform},
    lyon_tessellation::{BuffersBuilder, FillOptions, FillTessellator, FillVertex, VertexBuffers},
    path::{path::Builder, Path},
};
use rustybuzz::{Face, UnicodeBuffer};
use ttf_parser::GlyphId;
use wgpu::util::{DeviceExt, RenderEncoder};

use crate::term::Terminal;

use super::Viewport;

const SAMPLE_COUNT: u32 = 4;

pub struct LyonContext {
    vertex_buf: wgpu::Buffer,
    index_buf: wgpu::Buffer,
    index_count: usize,
    pipeline: wgpu::RenderPipeline,
    face: Face<'static>,
    font_width: f32,
    font_height: f32,
    face_descender: f32,
    buzz_buf: Option<UnicodeBuffer>,
    path_cache: AHashMap<GlyphId, Option<Path>>,
}

impl LyonContext {
    pub fn new(
        device: &wgpu::Device,
        shader: &wgpu::ShaderModule,
        pipeline_layout: &wgpu::PipelineLayout,
        viewport: &Viewport,
        font_height: f32,
    ) -> Self {
        let face = Face::from_slice(super::FONT, 0).unwrap();
        let m = face.glyph_index('M').unwrap();
        let h_advance = face.glyph_hor_advance(m).unwrap();
        let face_width = h_advance as f32;
        let font_width = face_width / face.units_per_em() as f32 * font_height;
        let face_descender = face.descender() as f32;

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("lyon_pipeline"),
            layout: Some(pipeline_layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: "lyon_vs",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<LyonVertex>() as _,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x2,
                        1 => Float32,
                        2 => Float32x2,
                        3 => Float32x2,
                        4 => Float32x2,
                    ],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: "simple_fs",
                targets: &[wgpu::ColorTargetState {
                    format: viewport.format(),
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                }],
            }),
            primitive: wgpu::PrimitiveState {
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: SAMPLE_COUNT,
                alpha_to_coverage_enabled: true,
                ..Default::default()
            },
        });

        let vertex_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Lyon vertex buffer"),
            size: 0,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let index_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Lyon vertex buffer"),
            size: 0,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            face,
            index_buf,
            vertex_buf,
            index_count: 0,
            pipeline,
            font_height,
            font_width,
            face_descender,
            path_cache: AHashMap::with_capacity(500),
            buzz_buf: Some(UnicodeBuffer::new()),
        }
    }

    pub fn font_height(&self) -> f32 {
        self.font_height
    }

    pub fn font_width(&self) -> f32 {
        self.font_width
    }

    pub fn set_draw(&mut self, device: &wgpu::Device, term: &Terminal) {
        let start = Instant::now();

        let mut buzz_buf = self.buzz_buf.take().unwrap();

        let scale = 1.0 / self.face.units_per_em() as f32;

        let mut tess = FillTessellator::new();
        let mut mesh = VertexBuffers::<LyonVertex, u32>::new();
        let mut line_str = String::new();

        for (line_no, line) in term.rows().enumerate() {
            let line_no = line_no as f32;
            line.write_text(&mut line_str);
            buzz_buf.push_str(&line_str);

            let glyph_buf = rustybuzz::shape(&self.face, &[], buzz_buf);

            let positions = glyph_buf.glyph_positions();
            let infos = glyph_buf.glyph_infos();

            let mut x = 0.0;
            let mut y = 0.0;

            for (pos, info) in positions.iter().zip(infos.iter()) {
                let face = &self.face;
                let path = self
                    .path_cache
                    .entry(GlyphId(info.glyph_id as _))
                    .or_insert_with_key(|id| {
                        let mut builder = LyonBuilder {
                            builder: Builder::new(),
                        };
                        face.outline_glyph(*id, &mut builder)?;
                        Some(builder.builder.build())
                    });

                if let Some(path) = path {
                    let transform = Transform::translation(
                        x + pos.x_offset as f32,
                        y + pos.y_offset as f32 - self.face_descender,
                    )
                    .then_scale(scale, scale);

                    tess.tessellate_path(
                        &*path,
                        &FillOptions::default().with_tolerance(0.008),
                        &mut BuffersBuilder::new(&mut mesh, |v: FillVertex| LyonVertex {
                            position: v.position().to_array(),
                            line_no,
                            transform: transform.to_arrays(),
                        }),
                    )
                    .unwrap();
                }

                x += pos.x_advance as f32;
                y += pos.y_advance as f32;
            }

            buzz_buf = glyph_buf.clear();
            line_str.clear();
        }

        self.vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            contents: bytemuck::cast_slice(&mesh.vertices),
            label: Some("lyon vertex"),
            usage: wgpu::BufferUsages::VERTEX,
        });

        self.index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            contents: bytemuck::cast_slice(&mesh.indices),
            label: Some("lyon index"),
            usage: wgpu::BufferUsages::INDEX,
        });

        self.index_count = mesh.indices.len();

        self.buzz_buf = Some(buzz_buf);

        let elapsed = start.elapsed();

        log::info!(
            "Tessellation complete in {}us, vertex: {}, index: {}",
            elapsed.as_micros(),
            mesh.vertices.len(),
            mesh.indices.len()
        );
    }

    pub fn draw<'a>(&'a self, rpass: &mut impl RenderEncoder<'a>) {
        if self.index_count == 0 {
            return;
        }

        rpass.set_pipeline(&self.pipeline);
        rpass.set_vertex_buffer(0, self.vertex_buf.slice(..));
        rpass.set_index_buffer(self.index_buf.slice(..), wgpu::IndexFormat::Uint32);
        rpass.draw_indexed(0..self.index_count as u32, 0, 0..1);
    }
}

struct LyonBuilder {
    builder: Builder,
}

impl ttf_parser::OutlineBuilder for LyonBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        self.builder.begin(Point::new(x, y));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.builder.line_to(Point::new(x, y));
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.builder
            .quadratic_bezier_to(Point::new(x1, y1), Point::new(x, y));
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.builder
            .cubic_bezier_to(Point::new(x1, y1), Point::new(x2, y2), Point::new(x, y));
    }

    fn close(&mut self) {
        self.builder.close();
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct LyonVertex {
    position: [f32; 2],
    line_no: f32,
    transform: [[f32; 2]; 3],
}
