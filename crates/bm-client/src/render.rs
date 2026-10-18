//! The renderer: wgpu, reversed-Z infinite projection, floating origin.
//!
//! - **Floating origin.** The camera sits at the origin; every chunk is drawn
//!   at its offset from the camera, computed in `f64` and only then narrowed
//!   to `f32`, at the lap of the planet nearest the camera (ADR-0002).
//! - **Reversed-Z, infinite far plane.** Depth is 1 at the near plane and 0 at
//!   infinity, which spreads `f32` depth precision evenly over distance: no
//!   z-fighting on far mountains. Clear to 0, test `Greater`.
//! - One vertex buffer per chunk and per pass, a shared quad index buffer, and
//!   one instance record per visible chunk carrying its offset and origin.

use bm_core::{BlockPos, BlockRegistry, CHUNK_SIZE, ChunkPos, PlanetShape};
use bm_mesh::{ChunkMesh, Vertex};
use glam::{DVec3, Mat4, Vec3, Vec4};
use std::collections::HashMap;
use std::sync::Arc;
use wgpu::util::DeviceExt;
use winit::window::Window;

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const NEAR: f32 = 0.05;
/// Blocks with colours in the palette uniform (4 × 16 bytes each, 16 KiB).
const PALETTE_BLOCKS: usize = 256;

/// Must match `Globals` in shaders/common.wgsl.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    inv_view_proj: [[f32; 4]; 4],
    sun_dir: [f32; 4],
    sky_zenith: [f32; 4],
    sky_horizon: [f32; 4],
    sun_color: [f32; 4],
    params: [f32; 4],
    screen: [f32; 4],
}

/// Every shader is `common.wgsl` followed by its own file.
macro_rules! shader_source {
    ($file:literal) => {
        concat!(
            include_str!("shaders/common.wgsl"),
            include_str!(concat!("shaders/", $file))
        )
    };
}

const TERRAIN_WGSL: &str = shader_source!("terrain.wgsl");
const SIMPLE_WGSL: &str = shader_source!("simple.wgsl");
const SKY_WGSL: &str = shader_source!("sky.wgsl");

fn shader(device: &wgpu::Device, label: &str, source: &'static str) -> wgpu::ShaderModule {
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    })
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ChunkInstance {
    offset: [f32; 3],
    origin: [i32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct BoxVertex {
    corner: [f32; 3],
    normal: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct BoxInstance {
    pub center: [f32; 3],
    pub half_size: [f32; 3],
    pub color: [f32; 4],
}

struct GpuChunk {
    opaque: Option<(wgpu::Buffer, u32)>,
    translucent: Option<(wgpu::Buffer, u32)>,
}

pub struct Camera {
    /// Eye position, canonical world coordinates.
    pub eye: DVec3,
    pub yaw: f64,
    pub pitch: f64,
    pub fov_y: f32,
}

impl Camera {
    pub fn forward(&self) -> DVec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        DVec3::new(sy * cp, sp, -cy * cp)
    }
}

pub struct Scene<'a> {
    pub camera: &'a Camera,
    pub sky: &'a crate::sky::Sky,
    pub shape: PlanetShape,
    pub boxes: &'a [BoxInstance],
    pub highlight: Option<BlockPos>,
    pub fog_end: f32,
    pub time: f32,
}

#[derive(Default, Clone, Copy)]
pub struct FrameStats {
    pub chunks_drawn: u32,
    pub quads_drawn: u64,
}

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    depth: wgpu::TextureView,
    globals: wgpu::Buffer,
    palette: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    terrain_opaque: wgpu::RenderPipeline,
    terrain_translucent: wgpu::RenderPipeline,
    box_pipeline: wgpu::RenderPipeline,
    line_pipeline: wgpu::RenderPipeline,
    cross_pipeline: wgpu::RenderPipeline,
    sky_pipeline: wgpu::RenderPipeline,
    quad_indices: wgpu::Buffer,
    quad_capacity: u32,
    chunks: HashMap<ChunkPos, GpuChunk>,
    instances: wgpu::Buffer,
    instance_capacity: usize,
    box_vertices: wgpu::Buffer,
    box_indices: wgpu::Buffer,
    box_instances: wgpu::Buffer,
    box_capacity: usize,
    lines: wgpu::Buffer,
    pub stats: FrameStats,
}

fn srgb_to_linear(c: u8) -> f32 {
    let c = c as f32 / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

impl Renderer {
    pub async fn new(window: Arc<Window>, vsync: bool) -> anyhow::Result<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let surface = instance.create_surface(window.clone())?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            })
            .await?;
        let info = adapter.get_info();
        tracing::info!("GPU: {} ({:?}, {:?})", info.name, info.backend, info.device_type);
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("device"),
                required_limits: wgpu::Limits::default().using_resolution(adapter.limits()),
                memory_hints: wgpu::MemoryHints::Performance,
                ..Default::default()
            })
            .await?;

        let size = window.inner_size();
        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .ok_or_else(|| anyhow::anyhow!("the window surface is not supported by this GPU"))?;
        config.format = format;
        config.present_mode = if vsync {
            wgpu::PresentMode::AutoVsync
        } else {
            wgpu::PresentMode::AutoNoVsync
        };
        config.desired_maximum_frame_latency = 2;
        surface.configure(&device, &config);
        let depth = make_depth(&device, &config);

        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let palette = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("palette"),
            size: (PALETTE_BLOCKS * 4 * 16) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let uniform = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals"),
            entries: &[uniform(0), uniform(1)],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: globals.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: palette.as_entire_binding(),
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("main"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });

        let terrain = shader(&device, "terrain", TERRAIN_WGSL);
        let simple = shader(&device, "simple", SIMPLE_WGSL);
        let sky_module = shader(&device, "sky", SKY_WGSL);

        let chunk_buffers = [
            Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Vertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Uint32, 1 => Uint32],
            }),
            Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<ChunkInstance>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &wgpu::vertex_attr_array![2 => Float32x3, 3 => Sint32x3],
            }),
        ];
        let depth_state = |write: bool, compare: wgpu::CompareFunction| wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(write),
            depth_compare: Some(compare),
            stencil: Default::default(),
            bias: Default::default(),
        };
        let target = |blend: Option<wgpu::BlendState>| {
            [Some(wgpu::ColorTargetState {
                format,
                blend,
                write_mask: wgpu::ColorWrites::ALL,
            })]
        };
        let opaque_target = target(None);
        let blend_target = target(Some(wgpu::BlendState::ALPHA_BLENDING));
        let pipeline = |label: &str,
                        module: &wgpu::ShaderModule,
                        vs: &str,
                        fs: &str,
                        buffers: &[Option<wgpu::VertexBufferLayout>],
                        topology: wgpu::PrimitiveTopology,
                        cull: Option<wgpu::Face>,
                        depth: Option<wgpu::DepthStencilState>,
                        targets: &[Option<wgpu::ColorTargetState>]| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module,
                    entry_point: Some(vs),
                    compilation_options: Default::default(),
                    buffers,
                },
                primitive: wgpu::PrimitiveState {
                    topology,
                    cull_mode: cull,
                    front_face: wgpu::FrontFace::Ccw,
                    ..Default::default()
                },
                depth_stencil: depth,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets,
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        use wgpu::CompareFunction::{Always, Greater, GreaterEqual};
        use wgpu::PrimitiveTopology::{LineList, TriangleList};
        let terrain_opaque = pipeline(
            "terrain opaque",
            &terrain,
            "vs_main",
            "fs_opaque",
            &chunk_buffers,
            TriangleList,
            Some(wgpu::Face::Back),
            Some(depth_state(true, Greater)),
            &opaque_target,
        );
        // Water is seen from both sides (from above and from under it).
        let terrain_translucent = pipeline(
            "terrain translucent",
            &terrain,
            "vs_main",
            "fs_translucent",
            &chunk_buffers,
            TriangleList,
            None,
            Some(depth_state(false, Greater)),
            &blend_target,
        );
        let box_buffers = [
            Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<BoxVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
            }),
            Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<BoxInstance>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &wgpu::vertex_attr_array![2 => Float32x3, 3 => Float32x3, 4 => Float32x4],
            }),
        ];
        let box_pipeline = pipeline(
            "boxes",
            &simple,
            "vs_box",
            "fs_box",
            &box_buffers,
            TriangleList,
            Some(wgpu::Face::Back),
            Some(depth_state(true, Greater)),
            &opaque_target,
        );
        let line_buffers = [Some(wgpu::VertexBufferLayout {
            array_stride: 12,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3],
        })];
        let line_pipeline = pipeline(
            "lines",
            &simple,
            "vs_line",
            "fs_line",
            &line_buffers,
            LineList,
            None,
            Some(depth_state(false, GreaterEqual)),
            &blend_target,
        );
        let cross_pipeline = pipeline(
            "crosshair",
            &simple,
            "vs_cross",
            "fs_cross",
            &[],
            TriangleList,
            None,
            Some(depth_state(false, Always)),
            &blend_target,
        );

        // Fills only pixels no geometry reached (depth still the cleared 0).
        let sky_pipeline = pipeline(
            "sky",
            &sky_module,
            "vs_sky",
            "fs_sky",
            &[],
            TriangleList,
            None,
            Some(depth_state(false, GreaterEqual)),
            &opaque_target,
        );

        let (box_vertices, box_indices) = cube(&device);
        let quad_capacity = 1 << 16;
        let mut r = Self {
            quad_indices: make_quad_indices(&device, quad_capacity),
            quad_capacity,
            instances: make_vertex_buffer(&device, "chunk instances", 4096 * std::mem::size_of::<ChunkInstance>()),
            instance_capacity: 4096,
            box_instances: make_vertex_buffer(&device, "box instances", 256 * std::mem::size_of::<BoxInstance>()),
            box_capacity: 256,
            lines: make_vertex_buffer(&device, "lines", 24 * 12),
            box_vertices,
            box_indices,
            surface,
            device,
            queue,
            config,
            depth,
            globals,
            palette,
            bind_group,
            terrain_opaque,
            terrain_translucent,
            box_pipeline,
            line_pipeline,
            cross_pipeline,
            sky_pipeline,
            chunks: HashMap::new(),
            stats: FrameStats::default(),
        };
        r.write_palette(&BlockRegistry::builtin());
        Ok(r)
    }

    /// Uploads block colours. Called with the catalogue the server sent.
    pub fn write_palette(&mut self, registry: &BlockRegistry) {
        // Four entries per block: side, top, bottom colours, then parameters
        // (opacity, fringe). Must match `palette` in terrain.wgsl.
        let mut data = vec![[0f32; 4]; PALETTE_BLOCKS * 4];
        let lin = |[r, g, b]: [u8; 3]| [srgb_to_linear(r), srgb_to_linear(g), srgb_to_linear(b), 1.0];
        for (id, def) in registry.iter().take(PALETTE_BLOCKS) {
            let i = id.0 as usize * 4;
            data[i] = lin(def.color);
            data[i + 1] = lin(def.top_color.unwrap_or(def.color));
            data[i + 2] = lin(def.bottom_color.unwrap_or(def.color));
            data[i + 3] = [def.opacity, def.fringe as u8 as f32, 0.0, 0.0];
        }
        if registry.len() > PALETTE_BLOCKS {
            tracing::warn!("only the first {PALETTE_BLOCKS} blocks have colours");
        }
        self.queue.write_buffer(&self.palette, 0, bytemuck::cast_slice(&data));
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        self.depth = make_depth(&self.device, &self.config);
    }

    pub fn upload_chunk(&mut self, pos: ChunkPos, mesh: &ChunkMesh) {
        if mesh.is_empty() {
            self.chunks.remove(&pos);
            return;
        }
        let quads = (mesh.opaque.len().max(mesh.translucent.len()) / 4) as u32;
        if quads > self.quad_capacity {
            self.quad_capacity = quads.next_power_of_two();
            self.quad_indices = make_quad_indices(&self.device, self.quad_capacity);
        }
        let make = |verts: &[Vertex], label: &str| {
            (!verts.is_empty()).then(|| {
                let buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(label),
                    contents: bytemuck::cast_slice(verts),
                    usage: wgpu::BufferUsages::VERTEX,
                });
                (buf, (verts.len() / 4) as u32)
            })
        };
        let gpu = GpuChunk {
            opaque: make(&mesh.opaque, "chunk opaque"),
            translucent: make(&mesh.translucent, "chunk translucent"),
        };
        self.chunks.insert(pos, gpu);
    }

    pub fn remove_chunk(&mut self, pos: ChunkPos) {
        self.chunks.remove(&pos);
    }

    pub fn chunk_meshes(&self) -> usize {
        self.chunks.len()
    }

    pub fn render(&mut self, scene: &Scene) {
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
            _ => return,
        };
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let cam = scene.camera;
        let aspect = self.config.width as f32 / self.config.height as f32;
        let view_proj = projection(cam.fov_y, aspect) * view_rotation(cam.forward().as_vec3());
        let sky = scene.sky;
        let fog_start = scene.fog_end * 0.55;
        let v4 = |c: [f32; 3], w: f32| [c[0], c[1], c[2], w];
        self.queue.write_buffer(
            &self.globals,
            0,
            bytemuck::bytes_of(&Globals {
                view_proj: view_proj.to_cols_array_2d(),
                inv_view_proj: view_proj.inverse().to_cols_array_2d(),
                sun_dir: v4(sky.sun_dir, sky.daylight),
                sky_zenith: v4(sky.zenith, 1.0),
                sky_horizon: v4(sky.horizon, 1.0),
                sun_color: v4(sky.sun_color, sky.sky_light),
                params: [fog_start, scene.fog_end, scene.time, sky.stars],
                screen: [self.config.width as f32, self.config.height as f32, 0.0, 0.0],
            }),
        );

        // Visible chunks: frustum and distance culled, sorted front to back.
        let planes = frustum_planes(view_proj);
        let size = CHUNK_SIZE as f64;
        let cull_dist = scene.fog_end as f64 + size * 1.8;
        let mut visible: Vec<(f64, ChunkPos, ChunkInstance)> = Vec::new();
        for pos in self.chunks.keys() {
            let o = pos.origin();
            let origin = DVec3::new(o.x as f64, o.y as f64, o.z as f64);
            let centre = scene.shape.nearest_lap(cam.eye, origin + DVec3::splat(size / 2.0));
            let rel_centre = centre - cam.eye;
            let dist = rel_centre.length();
            if dist > cull_dist {
                continue;
            }
            let rel_min = (rel_centre - DVec3::splat(size / 2.0)).as_vec3();
            if !aabb_in_frustum(&planes, rel_min, rel_min + Vec3::splat(size as f32)) {
                continue;
            }
            visible.push((
                dist,
                *pos,
                ChunkInstance {
                    offset: rel_min.to_array(),
                    origin: [o.x as i32, o.y as i32, o.z as i32],
                },
            ));
        }
        visible.sort_by(|a, b| a.0.total_cmp(&b.0));
        if visible.len() > self.instance_capacity {
            self.instance_capacity = visible.len().next_power_of_two();
            self.instances = make_vertex_buffer(
                &self.device,
                "chunk instances",
                self.instance_capacity * std::mem::size_of::<ChunkInstance>(),
            );
        }
        let inst: Vec<ChunkInstance> = visible.iter().map(|v| v.2).collect();
        self.queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&inst));

        if scene.boxes.len() > self.box_capacity {
            self.box_capacity = scene.boxes.len().next_power_of_two();
            self.box_instances = make_vertex_buffer(
                &self.device,
                "box instances",
                self.box_capacity * std::mem::size_of::<BoxInstance>(),
            );
        }
        self.queue
            .write_buffer(&self.box_instances, 0, bytemuck::cast_slice(scene.boxes));

        let highlight = scene.highlight.map(|b| {
            let corner = DVec3::new(b.x as f64, b.y as f64, b.z as f64);
            let rel = (scene.shape.nearest_lap(cam.eye, corner) - cam.eye).as_vec3();
            let verts = cube_edges(rel - Vec3::splat(0.002), Vec3::splat(1.004));
            self.queue.write_buffer(&self.lines, 0, bytemuck::cast_slice(&verts));
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame") });
        let mut stats = FrameStats::default();
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("world"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: sky.horizon[0] as f64,
                            g: sky.horizon[1] as f64,
                            b: sky.horizon[2] as f64,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_index_buffer(self.quad_indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.set_vertex_buffer(1, self.instances.slice(..));

            pass.set_pipeline(&self.terrain_opaque);
            for (i, (_, pos, _)) in visible.iter().enumerate() {
                if let Some((buf, quads)) = &self.chunks[pos].opaque {
                    pass.set_vertex_buffer(0, buf.slice(..));
                    pass.draw_indexed(0..quads * 6, 0, i as u32..i as u32 + 1);
                    stats.chunks_drawn += 1;
                    stats.quads_drawn += *quads as u64;
                }
            }

            if !scene.boxes.is_empty() {
                pass.set_pipeline(&self.box_pipeline);
                pass.set_index_buffer(self.box_indices.slice(..), wgpu::IndexFormat::Uint16);
                pass.set_vertex_buffer(0, self.box_vertices.slice(..));
                pass.set_vertex_buffer(1, self.box_instances.slice(..));
                pass.draw_indexed(0..36, 0, 0..scene.boxes.len() as u32);
                pass.set_index_buffer(self.quad_indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.set_vertex_buffer(1, self.instances.slice(..));
            }

            pass.set_pipeline(&self.sky_pipeline);
            pass.draw(0..3, 0..1);

            // Translucent last, far to near, without writing depth.
            pass.set_pipeline(&self.terrain_translucent);
            for (i, (_, pos, _)) in visible.iter().enumerate().rev() {
                if let Some((buf, quads)) = &self.chunks[pos].translucent {
                    pass.set_vertex_buffer(0, buf.slice(..));
                    pass.draw_indexed(0..quads * 6, 0, i as u32..i as u32 + 1);
                    stats.quads_drawn += *quads as u64;
                }
            }

            if highlight.is_some() {
                pass.set_pipeline(&self.line_pipeline);
                pass.set_vertex_buffer(0, self.lines.slice(..));
                pass.draw(0..24, 0..1);
            }
            pass.set_pipeline(&self.cross_pipeline);
            pass.draw(0..12, 0..1);
        }
        self.queue.submit([encoder.finish()]);
        self.queue.present(frame);
        self.stats = stats;
    }
}

/// Right-handed perspective with reversed-Z and an infinite far plane: view
/// depth `z` maps to `NEAR / -z`, so the near plane is 1 and infinity is 0.
pub fn projection(fov_y: f32, aspect: f32) -> Mat4 {
    let f = 1.0 / (fov_y * 0.5).tan();
    Mat4::from_cols(
        Vec4::new(f / aspect, 0.0, 0.0, 0.0),
        Vec4::new(0.0, f, 0.0, 0.0),
        Vec4::new(0.0, 0.0, 0.0, -1.0),
        Vec4::new(0.0, 0.0, NEAR, 0.0),
    )
}

/// View matrix for a camera at the origin looking along `forward` (never
/// straight up or down: pitch is limited short of the poles).
pub fn view_rotation(forward: Vec3) -> Mat4 {
    let f = forward.normalize();
    let s = f.cross(Vec3::Y).normalize();
    let u = s.cross(f);
    Mat4::from_cols(
        Vec4::new(s.x, u.x, -f.x, 0.0),
        Vec4::new(s.y, u.y, -f.y, 0.0),
        Vec4::new(s.z, u.z, -f.z, 0.0),
        Vec4::W,
    )
}

fn make_depth(device: &wgpu::Device, config: &wgpu::SurfaceConfiguration) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d {
                width: config.width,
                height: config.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

fn make_vertex_buffer(device: &wgpu::Device, label: &str, bytes: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: bytes as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn make_quad_indices(device: &wgpu::Device, quads: u32) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("quad indices"),
        contents: bytemuck::cast_slice(&bm_mesh::quad_indices(quads as usize)),
        usage: wgpu::BufferUsages::INDEX,
    })
}

/// A unit cube centred on the origin (corners at ±1), with face normals.
fn cube(device: &wgpu::Device) -> (wgpu::Buffer, wgpu::Buffer) {
    let faces: [([f32; 3], [f32; 3], [f32; 3]); 6] = [
        ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
        ([-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
        ([0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
        ([0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
        ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        ([0.0, 0.0, -1.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]),
    ];
    let mut verts = Vec::new();
    let mut idx: Vec<u16> = Vec::new();
    for (n, u, v) in faces {
        let (n, u, v) = (Vec3::from(n), Vec3::from(u), Vec3::from(v));
        let base = verts.len() as u16;
        for (su, sv) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            verts.push(BoxVertex {
                corner: (n + u * su + v * sv).to_array(),
                normal: n.to_array(),
            });
        }
        idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    let vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("cube"),
        contents: bytemuck::cast_slice(&verts),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let ib = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("cube indices"),
        contents: bytemuck::cast_slice(&idx),
        usage: wgpu::BufferUsages::INDEX,
    });
    (vb, ib)
}

/// The 12 edges of a box as a line list.
fn cube_edges(min: Vec3, size: Vec3) -> [[f32; 3]; 24] {
    let c = |x: f32, y: f32, z: f32| (min + size * Vec3::new(x, y, z)).to_array();
    let p = [
        c(0.0, 0.0, 0.0),
        c(1.0, 0.0, 0.0),
        c(1.0, 0.0, 1.0),
        c(0.0, 0.0, 1.0),
        c(0.0, 1.0, 0.0),
        c(1.0, 1.0, 0.0),
        c(1.0, 1.0, 1.0),
        c(0.0, 1.0, 1.0),
    ];
    let e = [
        (0, 1),
        (1, 2),
        (2, 3),
        (3, 0),
        (4, 5),
        (5, 6),
        (6, 7),
        (7, 4),
        (0, 4),
        (1, 5),
        (2, 6),
        (3, 7),
    ];
    let mut out = [[0.0; 3]; 24];
    for (i, (a, b)) in e.iter().enumerate() {
        out[i * 2] = p[*a];
        out[i * 2 + 1] = p[*b];
    }
    out
}

/// Left, right, bottom, top and near planes (the far plane is at infinity).
fn frustum_planes(m: Mat4) -> [Vec4; 5] {
    let r = |i: usize| m.row(i);
    let n = |p: Vec4| p / p.truncate().length();
    // Reversed-Z: near is z = w, i.e. w - z >= 0.
    [
        n(r(3) + r(0)),
        n(r(3) - r(0)),
        n(r(3) + r(1)),
        n(r(3) - r(1)),
        n(r(3) - r(2)),
    ]
}

fn aabb_in_frustum(planes: &[Vec4; 5], min: Vec3, max: Vec3) -> bool {
    planes.iter().all(|p| {
        // The box corner furthest along the plane normal.
        let v = Vec3::new(
            if p.x >= 0.0 { max.x } else { min.x },
            if p.y >= 0.0 { max.y } else { min.y },
            if p.z >= 0.0 { max.z } else { min.z },
        );
        p.truncate().dot(v) + p.w >= 0.0
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frustum_keeps_what_is_ahead_and_drops_what_is_behind() {
        let planes = frustum_planes(projection(1.2, 1.6) * view_rotation(Vec3::NEG_Z));
        let ahead = Vec3::new(-1.0, -1.0, -40.0);
        assert!(aabb_in_frustum(&planes, ahead, ahead + Vec3::splat(2.0)));
        let behind = Vec3::new(-1.0, -1.0, 10.0);
        assert!(!aabb_in_frustum(&planes, behind, behind + Vec3::splat(2.0)));
        let far_left = Vec3::new(-500.0, 0.0, -10.0);
        assert!(!aabb_in_frustum(&planes, far_left, far_left + Vec3::splat(2.0)));
        // Very far ahead is never clipped: the far plane is at infinity.
        let distant = Vec3::new(0.0, 0.0, -1.0e6);
        assert!(aabb_in_frustum(&planes, distant, distant + Vec3::splat(32.0)));
    }

    #[test]
    fn reversed_z_puts_near_at_one_and_far_towards_zero() {
        let proj = projection(1.2, 1.0);
        let ndc = |z: f32| {
            let c = proj * Vec4::new(0.0, 0.0, z, 1.0);
            c.z / c.w
        };
        assert!((ndc(-NEAR) - 1.0).abs() < 1e-6);
        assert!(ndc(-1000.0) > 0.0 && ndc(-1000.0) < 1e-3);
        assert!(ndc(-10.0) > ndc(-20.0), "nearer is greater");
    }

    #[test]
    fn view_looks_down_negative_z_by_default() {
        let v = view_rotation(Vec3::NEG_Z);
        assert!((v - Mat4::IDENTITY).abs_diff_eq(Mat4::ZERO, 1e-6));
        // Looking along +X, a point at +X is straight ahead (view -Z).
        let p = view_rotation(Vec3::X) * Vec4::new(5.0, 0.0, 0.0, 1.0);
        assert!((p.truncate() - Vec3::new(0.0, 0.0, -5.0)).length() < 1e-5);
    }

    #[test]
    fn shaders_validate() {
        for (name, src) in [("terrain", TERRAIN_WGSL), ("simple", SIMPLE_WGSL), ("sky", SKY_WGSL)] {
            let module =
                naga::front::wgsl::parse_str(src).unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(src)));
            naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all())
                .validate(&module)
                .unwrap_or_else(|e| panic!("{name}: {e:?}"));
        }
    }

    #[test]
    fn layouts_match_the_shaders() {
        assert_eq!(std::mem::size_of::<Globals>(), 64 * 2 + 16 * 6);
        assert_eq!(std::mem::size_of::<ChunkInstance>(), 24);
        assert_eq!(std::mem::size_of::<Vertex>(), 8);
    }
}
