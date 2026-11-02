//! Рендер на wgpu.
//!
//! Рисует чанки всех сеток (каждая сетка — своя матрица позы), линии
//! (рамка выделения) и интерфейс (`vx_ui`). Текстуры — массив слоёв, номер слоя
//! хранится в вершине, поэтому новая текстура = новый PNG без правки кода.

mod textures;
mod ui;

use std::{collections::HashMap, path::Path, sync::Arc};

use anyhow::{Context, Result};
use bytemuck::{Pod, Zeroable};
use glam::{IVec3, Mat4, Vec3};
use vx_core::GridId;
use vx_mesh::{Mesh, Vertex};
use wgpu::util::DeviceExt;
use winit::window::Window;

/// Вершина линии. `pos.w = 1` — точка мира, `pos.w = 0` — экранные координаты.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct LineVertex {
    pub pos: [f32; 4],
    pub color: [f32; 4],
}

/// Всё, что нужно для одного кадра.
pub struct Frame<'a> {
    pub view_proj: Mat4,
    pub camera: Vec3,
    /// Поза каждой сетки.
    pub grids: &'a [(GridId, Mat4)],
    pub lines: &'a [LineVertex],
    pub sky: [f32; 3],
    pub fog_distance: f32,
    /// Интерфейс поверх мира (уже триангулированный `vx_ui::tessellate`).
    pub ui: Option<&'a vx_ui::UiMesh>,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct CameraUniform {
    view_proj: [[f32; 4]; 4],
    position: [f32; 4],
    fog: [f32; 4],
}

struct ChunkGpu {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    count: u32,
}

struct GridGpu {
    model: wgpu::Buffer,
    bind: wgpu::BindGroup,
    chunks: HashMap<IVec3, ChunkGpu>,
}

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    depth: wgpu::TextureView,
    camera_buf: wgpu::Buffer,
    camera_bind: wgpu::BindGroup,
    model_layout: wgpu::BindGroupLayout,
    texture_bind: wgpu::BindGroup,
    block_pipeline: wgpu::RenderPipeline,
    line_pipeline: wgpu::RenderPipeline,
    ui: ui::UiPass,
    grids: HashMap<GridId, GridGpu>,
}

const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

fn uniform_layout(device: &wgpu::Device, label: &str) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(label),
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
    })
}

impl Renderer {
    /// `textures` — имена из реестра блоков, файлы ищутся в `texture_dir`.
    pub async fn new(
        window: Arc<Window>,
        display: winit::event_loop::OwnedDisplayHandle,
        textures: &[String],
        texture_dir: &Path,
    ) -> Result<Self> {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle_from_env(Box::new(display)));
        let surface = instance.create_surface(window.clone())?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions { compatible_surface: Some(&surface), ..Default::default() })
            .await
            .context("не найден GPU-адаптер")?;
        log::info!("GPU: {:?}", adapter.get_info());
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                required_limits: wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits()),
                ..Default::default()
            })
            .await?;

        let size = window.inner_size();
        let config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .context("поверхность не поддерживается")?;
        surface.configure(&device, &config);
        let depth = create_depth(&device, config.width, config.height);

        // --- группа 0: камера ---
        let camera_layout = uniform_layout(&device, "camera");
        let camera_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("camera"),
            size: size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let camera_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera"),
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: camera_buf.as_entire_binding() }],
        });

        // --- группа 1: поза сетки ---
        let model_layout = uniform_layout(&device, "model");

        // --- группа 2: массив текстур ---
        let (tex_size, layers) = textures::load_layers(texture_dir, textures);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("blocks"),
            size: wgpu::Extent3d { width: tex_size, height: tex_size, depth_or_array_layers: layers.len() as u32 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for (i, data) in layers.iter().enumerate() {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x: 0, y: 0, z: i as u32 },
                    aspect: wgpu::TextureAspect::All,
                },
                data,
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4 * tex_size), rows_per_image: None },
                wgpu::Extent3d { width: tex_size, height: tex_size, depth_or_array_layers: 1 },
            );
        }
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            ..Default::default() // nearest — пиксельный стиль
        });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("textures"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
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
        let texture_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("textures"),
            layout: &texture_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&texture_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });

        // --- конвейеры ---
        let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));
        let pipeline = |label,
                        layouts: &[Option<&wgpu::BindGroupLayout>],
                        vs,
                        fs,
                        buffer: wgpu::VertexBufferLayout<'_>,
                        topology,
                        depth_write| {
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(label),
                bind_group_layouts: layouts,
                immediate_size: 0,
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vs),
                    compilation_options: Default::default(),
                    buffers: &[Some(buffer)],
                },
                primitive: wgpu::PrimitiveState {
                    topology,
                    cull_mode: (topology == wgpu::PrimitiveTopology::TriangleList).then_some(wgpu::Face::Back),
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH,
                    depth_write_enabled: Some(depth_write),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: config.format,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let block_pipeline = pipeline(
            "blocks",
            &[Some(&camera_layout), Some(&model_layout), Some(&texture_layout)],
            "vs_block",
            "fs_block",
            wgpu::VertexBufferLayout {
                array_stride: size_of::<Vertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Uint32, 3 => Float32],
            },
            wgpu::PrimitiveTopology::TriangleList,
            true,
        );
        let line_pipeline = pipeline(
            "lines",
            &[Some(&camera_layout)],
            "vs_line",
            "fs_line",
            wgpu::VertexBufferLayout {
                array_stride: size_of::<LineVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4],
            },
            wgpu::PrimitiveTopology::LineList,
            false,
        );

        let ui = ui::UiPass::new(&device, config.format, DEPTH, &texture_layout);

        Ok(Renderer {
            surface,
            device,
            queue,
            config,
            depth,
            camera_buf,
            camera_bind,
            model_layout,
            texture_bind,
            block_pipeline,
            line_pipeline,
            ui,
            grids: HashMap::new(),
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        self.depth = create_depth(&self.device, width, height);
    }

    /// Загрузить атлас интерфейса (`vx_ui::FontAtlas::pixels`).
    pub fn set_ui_atlas(&mut self, width: u32, height: u32, rgba: &[u8]) {
        self.ui.set_atlas(&self.device, &self.queue, width, height, rgba);
    }

    /// Размер области рисования в физических пикселях.
    pub fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    pub fn aspect(&self) -> f32 {
        self.config.width as f32 / self.config.height as f32
    }

    /// Загрузить меш чанка (пустой меш — удалить чанк).
    pub fn set_chunk(&mut self, grid: GridId, cpos: IVec3, mesh: &Mesh) {
        let device = &self.device;
        let g = self.grids.entry(grid).or_insert_with(|| {
            let model = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("model"),
                contents: bytemuck::bytes_of(&Mat4::IDENTITY.to_cols_array_2d()),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("model"),
                layout: &self.model_layout,
                entries: &[wgpu::BindGroupEntry { binding: 0, resource: model.as_entire_binding() }],
            });
            GridGpu { model, bind, chunks: HashMap::new() }
        });
        if mesh.is_empty() {
            g.chunks.remove(&cpos);
            return;
        }
        let buffer = |data: &[u8], usage| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: data, usage })
        };
        g.chunks.insert(
            cpos,
            ChunkGpu {
                vertices: buffer(bytemuck::cast_slice(&mesh.vertices), wgpu::BufferUsages::VERTEX),
                indices: buffer(bytemuck::cast_slice(&mesh.indices), wgpu::BufferUsages::INDEX),
                count: mesh.indices.len() as u32,
            },
        );
    }

    pub fn remove_grid(&mut self, grid: GridId) {
        self.grids.remove(&grid);
    }

    /// Нарисовать кадр в окно.
    pub fn render(&mut self, frame: &Frame) {
        let surface_texture = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
            _ => return,
        };
        let view = surface_texture.texture.create_view(&Default::default());
        self.draw(frame, &view);
        self.queue.present(surface_texture);
    }

    /// Нарисовать кадр в картинку (скриншот).
    pub fn screenshot(&mut self, frame: &Frame) -> Result<image::RgbaImage> {
        let (w, h) = (self.config.width, self.config.height);
        let target = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("screenshot"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        self.draw(frame, &target.create_view(&Default::default()));

        let row = (4 * w).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("screenshot"),
            size: (row * h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: None },
            },
            target.size(),
        );
        self.queue.submit([encoder.finish()]);
        buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        self.device.poll(wgpu::PollType::wait_indefinitely())?;

        let data = buffer.slice(..).get_mapped_range()?;
        let bgra = matches!(self.config.format, wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb);
        let mut img = image::RgbaImage::new(w, h);
        for (y, line) in data.chunks(row as usize).take(h as usize).enumerate() {
            for (x, px) in line.chunks(4).take(w as usize).enumerate() {
                let p = if bgra { [px[2], px[1], px[0], 255] } else { [px[0], px[1], px[2], 255] };
                img.put_pixel(x as u32, y as u32, image::Rgba(p));
            }
        }
        Ok(img)
    }

    fn draw(&mut self, frame: &Frame, view: &wgpu::TextureView) {
        let camera = CameraUniform {
            view_proj: frame.view_proj.to_cols_array_2d(),
            position: frame.camera.extend(1.0).to_array(),
            fog: [frame.sky[0], frame.sky[1], frame.sky[2], frame.fog_distance],
        };
        self.queue.write_buffer(&self.camera_buf, 0, bytemuck::bytes_of(&camera));
        for (id, model) in frame.grids {
            if let Some(g) = self.grids.get(id) {
                self.queue.write_buffer(&g.model, 0, bytemuck::bytes_of(&model.to_cols_array_2d()));
            }
        }
        let lines = (!frame.lines.is_empty()).then(|| {
            self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("lines"),
                contents: bytemuck::cast_slice(frame.lines),
                usage: wgpu::BufferUsages::VERTEX,
            })
        });

        let ui = frame
            .ui
            .and_then(|m| self.ui.prepare(&self.device, &self.queue, m, self.size(), self.config.format.is_srgb()));

        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let [r, g, b] = frame.sky.map(|c| c as f64);
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_pipeline(&self.block_pipeline);
            pass.set_bind_group(0, &self.camera_bind, &[]);
            pass.set_bind_group(2, &self.texture_bind, &[]);
            for g in self.grids.values() {
                pass.set_bind_group(1, &g.bind, &[]);
                for c in g.chunks.values() {
                    pass.set_vertex_buffer(0, c.vertices.slice(..));
                    pass.set_index_buffer(c.indices.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..c.count, 0, 0..1);
                }
            }
            if let Some(lines) = &lines {
                pass.set_pipeline(&self.line_pipeline);
                pass.set_vertex_buffer(0, lines.slice(..));
                pass.draw(0..frame.lines.len() as u32, 0..1);
            }
            if let Some(ui) = &ui {
                self.ui.draw(&mut pass, &self.texture_bind, ui);
            }
        }
        self.queue.submit([encoder.finish()]);
    }
}

fn create_depth(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&Default::default())
}
