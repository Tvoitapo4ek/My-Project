//! Бэкенд интерфейса: рисует `vx_ui::UiMesh` поверх кадра.

use bytemuck::{Pod, Zeroable};
use vx_ui::{UiMesh, UiVertex};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ScreenUniform {
    size: [f32; 2],
    srgb_target: f32,
    _pad: f32,
}

pub struct UiPass {
    pipeline: wgpu::RenderPipeline,
    screen_buf: wgpu::Buffer,
    screen_bind: wgpu::BindGroup,
    atlas_layout: wgpu::BindGroupLayout,
    atlas_bind: Option<wgpu::BindGroup>,
}

impl UiPass {
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        depth: wgpu::TextureFormat,
        blocks_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let screen_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui screen"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let screen_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ui screen"),
            size: size_of::<ScreenUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let screen_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ui screen"),
            layout: &screen_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: screen_buf.as_entire_binding() }],
        });
        let atlas_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui atlas"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ui"),
            bind_group_layouts: &[Some(&screen_layout), Some(&atlas_layout), Some(blocks_layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("ui.wgsl"));
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ui"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_ui"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: size_of::<UiVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4, 3 => Sint32],
                })],
            },
            primitive: Default::default(), // без отсечения: изометрические грани иконок
            // Рисуется в том же проходе, что и мир, поэтому формат глубины нужен, но тест отключён.
            depth_stencil: Some(wgpu::DepthStencilState {
                format: depth,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_ui"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        UiPass { pipeline, screen_buf, screen_bind, atlas_layout, atlas_bind: None }
    }

    /// Загрузить атлас интерфейса (RGBA8).
    pub fn set_atlas(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, width: u32, height: u32, rgba: &[u8]) {
        let size = wgpu::Extent3d { width, height, depth_or_array_layers: 1 };
        let texture = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("ui atlas"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            Default::default(),
            rgba,
        );
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let view = texture.create_view(&Default::default());
        self.atlas_bind = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ui atlas"),
            layout: &self.atlas_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        }));
    }

    /// Подготовить буферы кадра (до начала прохода).
    pub fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mesh: &UiMesh,
        size: (u32, u32),
        srgb: bool,
    ) -> Option<(wgpu::Buffer, wgpu::Buffer, u32)> {
        if mesh.indices.is_empty() || self.atlas_bind.is_none() {
            return None;
        }
        let uniform =
            ScreenUniform { size: [size.0 as f32, size.1 as f32], srgb_target: srgb as u32 as f32, _pad: 0.0 };
        queue.write_buffer(&self.screen_buf, 0, bytemuck::bytes_of(&uniform));
        let buffer = |data: &[u8], usage| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("ui"), contents: data, usage })
        };
        Some((
            buffer(bytemuck::cast_slice(&mesh.vertices), wgpu::BufferUsages::VERTEX),
            buffer(bytemuck::cast_slice(&mesh.indices), wgpu::BufferUsages::INDEX),
            mesh.indices.len() as u32,
        ))
    }

    pub fn draw(
        &self,
        pass: &mut wgpu::RenderPass,
        blocks: &wgpu::BindGroup,
        buffers: &(wgpu::Buffer, wgpu::Buffer, u32),
    ) {
        let Some(atlas) = &self.atlas_bind else { return };
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.screen_bind, &[]);
        pass.set_bind_group(1, atlas, &[]);
        pass.set_bind_group(2, blocks, &[]);
        pass.set_vertex_buffer(0, buffers.0.slice(..));
        pass.set_index_buffer(buffers.1.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..buffers.2, 0, 0..1);
    }
}
