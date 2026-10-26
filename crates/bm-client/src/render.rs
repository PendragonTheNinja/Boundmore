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

use crate::colour::{self, PALETTE_COLOURS};
use crate::sky::{MAX_SKY_PLANETS, SkyPlanet};
use crate::textures::{self, TextureSet};
use bm_core::{BlockPos, BlockRegistry, CHUNK_SIZE, ChunkPos, ColourPalette, NO_TINT, PlanetShape};
use bm_mesh::{ChunkMesh, ShapedVertex, Vertex};
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
    sky_rotation: [[f32; 4]; 4],
    planets: [SkyPlanetGpu; MAX_SKY_PLANETS],
    /// Held lights (`Renderer::set_point_lights`): xyz camera-relative, w
    /// the level; the count is in `screen.w`.
    point_lights: [[f32; 4]; MAX_POINT_LIGHTS],
}

/// Moving lights the terrain shader adds to block light (torches in hands).
pub const MAX_POINT_LIGHTS: usize = 16;

/// The nearest `MAX_POINT_LIGHTS` lights to the camera, camera-relative
/// (each at the lap nearest the camera, ADR-0002), as the shader takes them.
fn point_lights_gpu(shape: &PlanetShape, eye: DVec3, lights: &[(DVec3, u8)]) -> ([[f32; 4]; MAX_POINT_LIGHTS], usize) {
    let mut near: Vec<(f64, DVec3, u8)> = lights
        .iter()
        .filter(|(_, level)| *level > 0)
        .map(|&(p, level)| {
            let rel = shape.nearest_lap(eye, p) - eye;
            (rel.length_squared(), rel, level.min(15))
        })
        .collect();
    near.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out = [[0.0; 4]; MAX_POINT_LIGHTS];
    let n = near.len().min(MAX_POINT_LIGHTS);
    for (slot, (_, rel, level)) in out.iter_mut().zip(near) {
        *slot = [rel.x as f32, rel.y as f32, rel.z as f32, level as f32];
    }
    (out, n)
}

/// One planet in the sky. Must match `SkyPlanet` in shaders/common.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
struct SkyPlanetGpu {
    /// xyz: unit direction, w: apparent radius in radians.
    dir: [f32; 4],
    /// rgb: linear colour, w: 1 for a wandering world (drawn with a halo).
    color: [f32; 4],
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
struct OverlayVertex {
    pos: [f32; 3],
    uv: [f32; 2],
    layer: u32,
}

/// A line end, camera-relative, with its colour (linear RGBA).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LineVertex {
    pub pos: [f32; 3],
    pub color: [f32; 4],
}

const HIGHLIGHT: [f32; 4] = [0.05, 0.05, 0.07, 0.9];

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct BoxVertex {
    corner: [f32; 3],
    normal: [f32; 3],
}

/// One box to draw: camera-relative centre, half extents, colour (linear),
/// a turn about its centre and the light it stands in. Must match `BoxIn`
/// in shaders/simple.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct BoxInstance {
    pub center: [f32; 3],
    pub half_size: [f32; 3],
    pub color: [f32; 4],
    /// A unit quaternion (x, y, z, w).
    pub rotation: [f32; 4],
    /// Sky light and block light (0 to 1, as the light map has them where
    /// the box stands), brightness (1 normal), glow (0 lit like terrain, 1
    /// shining by itself: shots, a hit's flash).
    pub light: [f32; 4],
}

impl BoxInstance {
    /// Light for a box out in full daylight with nothing else.
    #[cfg(test)]
    pub const OPEN_SKY: [f32; 4] = [1.0, 0.0, 1.0, 0.0];

    /// An unturned box in `light`.
    pub fn new(center: Vec3, half_size: Vec3, color: [f32; 4], light: [f32; 4]) -> Self {
        Self {
            center: center.to_array(),
            half_size: half_size.to_array(),
            color,
            rotation: glam::Quat::IDENTITY.to_array(),
            light,
        }
    }

    /// The light-map light at a point as `light` takes it (brightness 1, no
    /// glow).
    pub fn light_of(l: bm_core::light::Light) -> [f32; 4] {
        [l.sky() as f32 / 15.0, l.block() as f32 / 15.0, 1.0, 0.0]
    }
}

/// A chunk's buffers: cube quads (with their count) and cut-block
/// triangles (with their vertex count).
struct GpuChunk {
    opaque: Option<(wgpu::Buffer, u32)>,
    translucent: Option<(wgpu::Buffer, u32)>,
    shaped_opaque: Option<(wgpu::Buffer, u32)>,
    shaped_translucent: Option<(wgpu::Buffer, u32)>,
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
    /// The other planets of the universe, where this sky shows them.
    pub planets: &'a [SkyPlanet],
    pub shape: PlanetShape,
    pub boxes: &'a [BoxInstance],
    pub highlight: Option<BlockPos>,
    /// The block being broken and how far along (0 to 1), for the cracks.
    pub breaking: Option<(BlockPos, f32)>,
    /// Line pairs drawn where they are not hidden (claim outlines).
    pub lines: &'a [LineVertex],
    /// Line pairs drawn through everything (claim outlines seen faintly
    /// through terrain).
    pub xray_lines: &'a [LineVertex],
    /// The menus and HUD, drawn last over everything.
    pub ui: Option<EguiFrame<'a>>,
    pub fog_end: f32,
    pub time: f32,
}

/// One frame of egui output, tessellated.
pub struct EguiFrame<'a> {
    pub primitives: &'a [egui::ClippedPrimitive],
    pub textures: &'a egui::TexturesDelta,
    pub pixels_per_point: f32,
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
    /// The block colours as last written (`write_palette`), and each face's
    /// average texture luma (`set_textures`; None where it has no texture),
    /// merged into the palette uniform by `upload_palette`.
    block_colours: Vec<[f32; 4]>,
    texture_luma: Vec<[Option<f32>; 3]>,
    /// The colour palette (ADR-0007), by colour id.
    colours: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    terrain_opaque: wgpu::RenderPipeline,
    terrain_translucent: wgpu::RenderPipeline,
    shaped_opaque: wgpu::RenderPipeline,
    shaped_translucent: wgpu::RenderPipeline,
    box_pipeline: wgpu::RenderPipeline,
    line_pipeline: wgpu::RenderPipeline,
    xray_line_pipeline: wgpu::RenderPipeline,
    cross_pipeline: wgpu::RenderPipeline,
    sky_pipeline: wgpu::RenderPipeline,
    overlay_pipeline: wgpu::RenderPipeline,
    overlay: wgpu::Buffer,
    egui: egui_wgpu::Renderer,
    crack_base: Option<u32>,
    bind_layout: wgpu::BindGroupLayout,
    faces: wgpu::Buffer,
    /// The face table as last uploaded, and each block's tint (sRGB), which
    /// rides in the table's spare column.
    face_table: Vec<[u32; 4]>,
    tints: Vec<[u8; 3]>,
    sampler: wgpu::Sampler,
    quad_indices: wgpu::Buffer,
    quad_capacity: u32,
    chunks: HashMap<ChunkPos, GpuChunk>,
    /// Chunks with a near mesh uploaded, empty ones too: their far mesh is
    /// not drawn.
    near_meshed: std::collections::HashSet<ChunkPos>,
    /// Far meshes (milestone 20), drawn where a chunk has no near mesh.
    far_chunks: HashMap<ChunkPos, GpuChunk>,
    /// Held lights in canonical world positions, with their levels.
    point_lights: Vec<(DVec3, u8)>,
    instances: wgpu::Buffer,
    instance_capacity: usize,
    box_vertices: wgpu::Buffer,
    box_indices: wgpu::Buffer,
    box_instances: wgpu::Buffer,
    box_capacity: usize,
    lines: wgpu::Buffer,
    line_capacity: usize,
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
        let colours = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("colours"),
            size: (PALETTE_COLOURS * 16) as u64,
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
            entries: &[
                uniform(0),
                uniform(1),
                uniform(2),
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                uniform(5),
            ],
        });
        let faces = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("texture faces"),
            size: (PALETTE_BLOCKS * 16) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // Crisp texels up close (the painted look), smooth at a distance.
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("blocks"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let empty = TextureSet {
            layers: vec![vec![255; (textures::SIZE * textures::SIZE * 4) as usize]],
            faces: Vec::new(),
            crack_base: None,
            missing: Vec::new(),
        };
        let block_view = make_block_texture(&device, &queue, &empty);
        let bind_group = make_bind_group(
            &device,
            &layout,
            [&globals, &palette, &faces, &colours],
            &block_view,
            &sampler,
        );
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
        // Cut blocks (ADR-0008): their own vertex, the same instances.
        let shaped_buffers = [
            Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<ShapedVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Uint32, 1 => Uint32, 2 => Uint32, 3 => Uint32],
            }),
            Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<ChunkInstance>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &wgpu::vertex_attr_array![4 => Float32x3, 5 => Sint32x3],
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
        let shaped_opaque = pipeline(
            "shaped opaque",
            &terrain,
            "vs_shaped",
            "fs_cutout",
            &shaped_buffers,
            TriangleList,
            Some(wgpu::Face::Back),
            Some(depth_state(true, Greater)),
            &opaque_target,
        );
        let shaped_translucent = pipeline(
            "shaped translucent",
            &terrain,
            "vs_shaped",
            "fs_translucent",
            &shaped_buffers,
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
                attributes: &wgpu::vertex_attr_array![2 => Float32x3, 3 => Float32x3, 4 => Float32x4, 5 => Float32x4, 6 => Float32x4],
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
            array_stride: std::mem::size_of::<LineVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4],
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
        let xray_line_pipeline = pipeline(
            "lines through terrain",
            &simple,
            "vs_line",
            "fs_line",
            &line_buffers,
            LineList,
            None,
            Some(depth_state(false, Always)),
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

        let overlay_buffers = [Some(wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<OverlayVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Uint32],
        })];
        let overlay_pipeline = pipeline(
            "crack overlay",
            &terrain,
            "vs_overlay",
            "fs_overlay",
            &overlay_buffers,
            TriangleList,
            None,
            Some(depth_state(false, GreaterEqual)),
            &blend_target,
        );

        // Menus and HUD: egui, drawn in its own pass over the finished frame.
        let egui = egui_wgpu::Renderer::new(&device, format, egui_wgpu::RendererOptions::default());

        let (box_vertices, box_indices) = cube(&device);
        let quad_capacity = 1 << 16;
        let overlay = make_vertex_buffer(&device, "overlay", 36 * std::mem::size_of::<OverlayVertex>());
        let mut r = Self {
            quad_indices: make_quad_indices(&device, quad_capacity),
            quad_capacity,
            instances: make_vertex_buffer(&device, "chunk instances", 4096 * std::mem::size_of::<ChunkInstance>()),
            instance_capacity: 4096,
            box_instances: make_vertex_buffer(&device, "box instances", 256 * std::mem::size_of::<BoxInstance>()),
            box_capacity: 256,
            lines: make_vertex_buffer(&device, "lines", 1024 * std::mem::size_of::<LineVertex>()),
            line_capacity: 1024,
            box_vertices,
            box_indices,
            surface,
            device,
            queue,
            config,
            depth,
            globals,
            palette,
            block_colours: vec![[0.0; 4]; PALETTE_BLOCKS * 4],
            texture_luma: Vec::new(),
            colours,
            bind_group,
            terrain_opaque,
            terrain_translucent,
            shaped_opaque,
            shaped_translucent,
            box_pipeline,
            line_pipeline,
            xray_line_pipeline,
            cross_pipeline,
            sky_pipeline,
            overlay_pipeline,
            overlay,
            egui,
            crack_base: None,
            bind_layout: layout,
            faces,
            face_table: vec![[textures::NO_TEXTURE, textures::NO_TEXTURE, textures::NO_TEXTURE, 0]; PALETTE_BLOCKS],
            tints: Vec::new(),
            sampler,
            chunks: HashMap::new(),
            near_meshed: Default::default(),
            far_chunks: HashMap::new(),
            point_lights: Vec::new(),
            stats: FrameStats::default(),
        };
        r.write_palette(&BlockRegistry::builtin());
        r.set_colours(&ColourPalette::builtin());
        r.write_faces();
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
        self.block_colours = data;
        self.upload_palette();
    }

    fn upload_palette(&mut self) {
        let data = with_face_luma(&self.block_colours, &self.texture_luma);
        self.queue.write_buffer(&self.palette, 0, bytemuck::cast_slice(&data));
    }

    /// Uploads the colour palette the server sent (`Welcome.colours`).
    pub fn set_colours(&mut self, palette: &ColourPalette) {
        let data = colour::palette_uniform(palette);
        self.queue.write_buffer(&self.colours, 0, bytemuck::cast_slice(&data));
    }

    /// Uploads the block textures and the per-block face table.
    pub fn set_textures(&mut self, set: &TextureSet) {
        let mut table = vec![[textures::NO_TEXTURE, textures::NO_TEXTURE, textures::NO_TEXTURE, 0u32]; PALETTE_BLOCKS];
        for (i, f) in set.faces.iter().take(PALETTE_BLOCKS).enumerate() {
            table[i] = *f;
        }
        self.face_table = table;
        self.write_faces();
        self.texture_luma = set
            .faces
            .iter()
            .take(PALETTE_BLOCKS)
            .map(|f| [f[1], f[0], f[2]].map(|e| colour::face_luma(e, &set.layers)))
            .collect();
        self.upload_palette();
        let view = make_block_texture(&self.device, &self.queue, set);
        self.bind_group = make_bind_group(
            &self.device,
            &self.bind_layout,
            [&self.globals, &self.palette, &self.faces, &self.colours],
            &view,
            &self.sampler,
        );
        self.crack_base = set.crack_base;
        tracing::info!("{} texture layers uploaded", set.layers.len());
    }

    /// This planet's block tints: one sRGB colour per block id multiplying
    /// its texture or colour (`PlanetInfo::tints`; white or missing is none).
    /// Applied in the shader, so changing planets re-meshes nothing.
    pub fn set_tints(&mut self, tints: &[[u8; 3]]) {
        self.tints = tints.to_vec();
        self.write_faces();
    }

    fn write_faces(&mut self) {
        let table = with_tints(&self.face_table, &self.tints);
        self.queue.write_buffer(&self.faces, 0, bytemuck::cast_slice(&table));
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
        self.near_meshed.insert(pos);
        match self.gpu_chunk(mesh) {
            Some(gpu) => self.chunks.insert(pos, gpu),
            None => self.chunks.remove(&pos),
        };
    }

    /// A chunk's far mesh (milestone 20).
    pub fn upload_far(&mut self, pos: ChunkPos, mesh: &ChunkMesh) {
        match self.gpu_chunk(mesh) {
            Some(gpu) => self.far_chunks.insert(pos, gpu),
            None => self.far_chunks.remove(&pos),
        };
    }

    pub fn remove_far(&mut self, pos: ChunkPos) {
        self.far_chunks.remove(&pos);
    }

    pub fn far_meshes(&self) -> usize {
        self.far_chunks.len()
    }

    fn gpu_chunk(&mut self, mesh: &ChunkMesh) -> Option<GpuChunk> {
        if mesh.is_empty() {
            return None;
        }
        let quads = (mesh.opaque.len().max(mesh.translucent.len()) / 4) as u32;
        if quads > self.quad_capacity {
            self.quad_capacity = quads.next_power_of_two();
            self.quad_indices = make_quad_indices(&self.device, self.quad_capacity);
        }
        let make = |bytes: &[u8], count: usize, label: &str| {
            (count > 0).then(|| {
                let buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(label),
                    contents: bytes,
                    usage: wgpu::BufferUsages::VERTEX,
                });
                (buf, count as u32)
            })
        };
        let quads = |v: &[Vertex], label: &str| make(bytemuck::cast_slice(v), v.len() / 4, label);
        let tris = |v: &[ShapedVertex], label: &str| make(bytemuck::cast_slice(v), v.len(), label);
        Some(GpuChunk {
            opaque: quads(&mesh.opaque, "chunk opaque"),
            translucent: quads(&mesh.translucent, "chunk translucent"),
            shaped_opaque: tris(&mesh.shaped_opaque, "chunk shaped opaque"),
            shaped_translucent: tris(&mesh.shaped_translucent, "chunk shaped translucent"),
        })
    }

    pub fn remove_chunk(&mut self, pos: ChunkPos) {
        self.chunks.remove(&pos);
        self.near_meshed.remove(&pos);
    }

    /// Drops every chunk mesh, near and far (leaving one session or planet
    /// for another).
    pub fn clear_chunks(&mut self) {
        self.chunks.clear();
        self.near_meshed.clear();
        self.far_chunks.clear();
    }

    pub fn chunk_meshes(&self) -> usize {
        self.chunks.len()
    }

    /// Moving lights for the next frames: each a canonical world position
    /// (a hand holding a torch) and a light level, falling off one level per
    /// block as block light does. Drawn in the shader on top of the meshed
    /// block light (the brighter wins), so nothing re-meshes and `LightMap`
    /// never sees them. The nearest [`MAX_POINT_LIGHTS`] are used.
    pub fn set_point_lights(&mut self, lights: &[(DVec3, u8)]) {
        self.point_lights.clear();
        self.point_lights.extend_from_slice(lights);
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
        let (point_lights, point_count) = point_lights_gpu(&scene.shape, cam.eye, &self.point_lights);
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
                screen: [
                    self.config.width as f32,
                    self.config.height as f32,
                    scene.planets.len().min(MAX_SKY_PLANETS) as f32,
                    point_count as f32,
                ],
                sky_rotation: Mat4::from_mat3(sky.sky_rotation).to_cols_array_2d(),
                planets: sky_planets(scene.planets),
                point_lights,
            }),
        );

        // Visible chunks: frustum and distance culled, sorted front to back.
        let planes = frustum_planes(view_proj);
        let size = CHUNK_SIZE as f64;
        let cull_dist = scene.fog_end as f64 + size * 1.8;
        let mut visible: Vec<(f64, &GpuChunk, ChunkInstance)> = Vec::new();
        let near = self.chunks.iter();
        let far = self.far_chunks.iter().filter(|(p, _)| !self.near_meshed.contains(p));
        for (pos, gpu) in near.chain(far) {
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
                gpu,
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

        // One line buffer: the highlight, then the outlines, then the x-ray.
        let mut lines: Vec<LineVertex> = Vec::new();
        if let Some(b) = scene.highlight {
            let corner = DVec3::new(b.x as f64, b.y as f64, b.z as f64);
            let rel = (scene.shape.nearest_lap(cam.eye, corner) - cam.eye).as_vec3();
            let edges = cube_edges(rel - Vec3::splat(0.002), Vec3::splat(1.004));
            lines.extend(edges.iter().map(|p| LineVertex {
                pos: *p,
                color: HIGHLIGHT,
            }));
        }
        let shown = lines.len() as u32 + scene.lines.len() as u32;
        lines.extend_from_slice(scene.lines);
        lines.extend_from_slice(scene.xray_lines);
        if lines.len() > self.line_capacity {
            self.line_capacity = lines.len().next_power_of_two();
            self.lines = make_vertex_buffer(
                &self.device,
                "lines",
                self.line_capacity * std::mem::size_of::<LineVertex>(),
            );
        }
        self.queue.write_buffer(&self.lines, 0, bytemuck::cast_slice(&lines));

        let cracks = match (scene.breaking, self.crack_base) {
            (Some((b, progress)), Some(base)) => {
                let stage = ((progress * textures::CRACK_STAGES as f32) as u32).min(textures::CRACK_STAGES - 1);
                let corner = DVec3::new(b.x as f64, b.y as f64, b.z as f64);
                let rel = (scene.shape.nearest_lap(cam.eye, corner) - cam.eye).as_vec3();
                let verts = crack_cube(rel - Vec3::splat(0.003), 1.006, base + stage);
                self.queue.write_buffer(&self.overlay, 0, bytemuck::cast_slice(&verts));
                true
            }
            _ => false,
        };

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame") });
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.config.width, self.config.height],
            pixels_per_point: scene.ui.as_ref().map(|u| u.pixels_per_point).unwrap_or(1.0),
        };
        let mut egui_commands = Vec::new();
        if let Some(ui) = &scene.ui {
            for (id, deltas) in &ui.textures.set {
                for delta in deltas {
                    self.egui.update_texture(&self.device, &self.queue, *id, delta);
                }
            }
            egui_commands = self
                .egui
                .update_buffers(&self.device, &self.queue, &mut encoder, ui.primitives, &screen);
        }
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
            for (i, (_, gpu, _)) in visible.iter().enumerate() {
                if let Some((buf, quads)) = &gpu.opaque {
                    pass.set_vertex_buffer(0, buf.slice(..));
                    pass.draw_indexed(0..quads * 6, 0, i as u32..i as u32 + 1);
                    stats.chunks_drawn += 1;
                    stats.quads_drawn += *quads as u64;
                }
            }
            pass.set_pipeline(&self.shaped_opaque);
            for (i, (_, gpu, _)) in visible.iter().enumerate() {
                if let Some((buf, verts)) = &gpu.shaped_opaque {
                    pass.set_vertex_buffer(0, buf.slice(..));
                    pass.draw(0..*verts, i as u32..i as u32 + 1);
                    stats.quads_drawn += *verts as u64 / 6;
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
            for (i, (_, chunk, _)) in visible.iter().enumerate().rev() {
                if let Some((buf, quads)) = &chunk.translucent {
                    pass.set_pipeline(&self.terrain_translucent);
                    pass.set_vertex_buffer(0, buf.slice(..));
                    pass.draw_indexed(0..quads * 6, 0, i as u32..i as u32 + 1);
                    stats.quads_drawn += *quads as u64;
                }
                if let Some((buf, verts)) = &chunk.shaped_translucent {
                    pass.set_pipeline(&self.shaped_translucent);
                    pass.set_vertex_buffer(0, buf.slice(..));
                    pass.draw(0..*verts, i as u32..i as u32 + 1);
                    stats.quads_drawn += *verts as u64 / 6;
                }
            }

            if cracks {
                pass.set_pipeline(&self.overlay_pipeline);
                pass.set_vertex_buffer(0, self.overlay.slice(..));
                pass.draw(0..36, 0..1);
            }
            if shown > 0 {
                pass.set_pipeline(&self.line_pipeline);
                pass.set_vertex_buffer(0, self.lines.slice(..));
                pass.draw(0..shown, 0..1);
            }
            if lines.len() as u32 > shown {
                pass.set_pipeline(&self.xray_line_pipeline);
                pass.set_vertex_buffer(0, self.lines.slice(..));
                pass.draw(shown..lines.len() as u32, 0..1);
            }
            pass.set_pipeline(&self.cross_pipeline);
            pass.draw(0..12, 0..1);
        }
        if let Some(ui) = &scene.ui {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            self.egui.render(&mut pass.forget_lifetime(), ui.primitives, &screen);
        }
        egui_commands.push(encoder.finish());
        self.queue.submit(egui_commands);
        if let Some(ui) = &scene.ui {
            for id in &ui.textures.free {
                self.egui.free_texture(id);
            }
        }
        self.queue.present(frame);
        self.stats = stats;
    }
}

/// The face table with each block's tint packed into its spare column as
/// `r | g << 8 | b << 16 | 0xff << 24` (sRGB). The top byte marks it set, so
/// a zero column (no tint written) still reads as white in the shader.
fn with_tints(table: &[[u32; 4]], tints: &[[u8; 3]]) -> Vec<[u32; 4]> {
    table
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let [r, g, b] = tints.get(i).copied().unwrap_or(NO_TINT);
            [
                f[0],
                f[1],
                f[2],
                r as u32 | (g as u32) << 8 | (b as u32) << 16 | 0xff << 24,
            ]
        })
        .collect()
}

/// The block colours with each face's average luma in w: its texture's
/// where it has one, else its own colour's. The shader divides by it to find
/// a texel's light and shade when recolouring (ADR-0007).
fn with_face_luma(block_colours: &[[f32; 4]], texture_luma: &[[Option<f32>; 3]]) -> Vec<[f32; 4]> {
    let mut out = block_colours.to_vec();
    for (i, entry) in out.iter_mut().enumerate() {
        let (block, slot) = (i / 4, i % 4);
        if slot < 3 {
            let own = colour::luma([entry[0], entry[1], entry[2]]);
            entry[3] = texture_luma.get(block).and_then(|l| l[slot]).unwrap_or(own);
        }
    }
    out
}

fn sky_planets(list: &[SkyPlanet]) -> [SkyPlanetGpu; MAX_SKY_PLANETS] {
    let mut out = [SkyPlanetGpu::default(); MAX_SKY_PLANETS];
    for (o, p) in out.iter_mut().zip(list) {
        o.dir = [p.dir[0], p.dir[1], p.dir[2], p.radius];
        o.color = [p.color[0], p.color[1], p.color[2], if p.wandering { 1.0 } else { 0.0 }];
    }
    out
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

fn make_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    [globals, palette, faces, colours]: [&wgpu::Buffer; 4],
    view: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("globals"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: globals.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: palette.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: faces.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(view),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: colours.as_entire_binding(),
            },
        ],
    })
}

/// One 2D array texture holding every block texture layer, with mipmaps
/// made on the CPU in linear light.
fn make_block_texture(device: &wgpu::Device, queue: &wgpu::Queue, set: &TextureSet) -> wgpu::TextureView {
    let size = set.size();
    let mips = textures::mip_levels(size);
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("blocks"),
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: set.layers.len() as u32,
        },
        mip_level_count: mips,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (layer, pixels) in set.layers.iter().enumerate() {
        let mut level = pixels.clone();
        let mut s = size;
        for mip in 0..mips {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: mip,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: layer as u32,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &level,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * s),
                    rows_per_image: Some(s),
                },
                wgpu::Extent3d {
                    width: s,
                    height: s,
                    depth_or_array_layers: 1,
                },
            );
            if s > 1 {
                level = textures::downsample(&level, s);
                s /= 2;
            }
        }
    }
    texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    })
}

/// The six faces of a cube as triangles, textured with one overlay layer.
fn crack_cube(min: Vec3, size: f32, layer: u32) -> Vec<OverlayVertex> {
    let mut out = Vec::with_capacity(36);
    let faces: [(Vec3, Vec3, Vec3); 6] = [
        (Vec3::X, Vec3::NEG_Z, Vec3::NEG_Y),
        (Vec3::ZERO, Vec3::Z, Vec3::NEG_Y),
        (Vec3::Y, Vec3::X, Vec3::Z),
        (Vec3::ZERO, Vec3::X, Vec3::Z),
        (Vec3::Z, Vec3::X, Vec3::NEG_Y),
        (Vec3::ZERO, Vec3::NEG_X, Vec3::NEG_Y),
    ];
    for (plane, u, v) in faces {
        // Corner of the face with u = v = 0, mapped into the unit cube.
        let origin = plane + u.min(Vec3::ZERO).abs() + v.min(Vec3::ZERO).abs();
        let corner = |a: f32, b: f32| OverlayVertex {
            pos: (min + (origin + u * a + v * b) * size).to_array(),
            uv: [a, b],
            layer,
        };
        let q = [corner(0.0, 0.0), corner(1.0, 0.0), corner(1.0, 1.0), corner(0.0, 1.0)];
        out.extend_from_slice(&[q[0], q[1], q[2], q[0], q[2], q[3]]);
    }
    out
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
pub fn cube_edges(min: Vec3, size: Vec3) -> [[f32; 3]; 24] {
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
    fn point_lights_are_camera_relative_nearest_first_across_the_seam() {
        let shape = PlanetShape::new(4, 1);
        let w = shape.width_blocks() as f64;
        let eye = DVec3::new(1.0, 10.0, 1.0);
        let mut lights = vec![
            (DVec3::new(w - 2.0, 10.0, 1.0), 10),
            (DVec3::new(30.0, 10.0, 30.0), 10),
            (eye, 0),
        ];
        for i in 0..20 {
            lights.push((DVec3::new(40.0 + i as f64, 10.0, 40.0), 12));
        }
        let (gpu, n) = point_lights_gpu(&shape, eye, &lights);
        assert_eq!(n, MAX_POINT_LIGHTS);
        // Across the seam, three blocks away; dark lights are dropped.
        assert_eq!(gpu[0], [-3.0, 0.0, 0.0, 10.0]);
        assert_eq!(gpu[1], [29.0, 0.0, 29.0, 10.0]);
        assert!(gpu.iter().all(|l| l[3] > 0.0));
    }

    #[test]
    fn layouts_match_the_shaders() {
        assert_eq!(
            std::mem::size_of::<Globals>(),
            64 * 3 + 16 * 6 + 32 * MAX_SKY_PLANETS + 16 * MAX_POINT_LIGHTS
        );
        assert_eq!(std::mem::size_of::<ChunkInstance>(), 24);
        assert_eq!(std::mem::size_of::<Vertex>(), 8);
        assert_eq!(std::mem::size_of::<ShapedVertex>(), 16);
        assert_eq!(std::mem::size_of::<LineVertex>(), 28);
        // centre 12, half size 12, colour 16, rotation 16, light 16, in the
        // order of `BoxIn`'s locations 2 to 6.
        assert_eq!(std::mem::size_of::<BoxInstance>(), 72);
        let b = BoxInstance::new(Vec3::ONE, Vec3::ONE, [0.0; 4], BoxInstance::OPEN_SKY);
        let base = &b as *const BoxInstance as usize;
        let offsets = [
            &b.center as *const _ as usize - base,
            &b.half_size as *const _ as usize - base,
            &b.color as *const _ as usize - base,
            &b.rotation as *const _ as usize - base,
            &b.light as *const _ as usize - base,
        ];
        assert_eq!(offsets, [0, 12, 24, 40, 56]);
        assert_eq!(b.rotation, [0.0, 0.0, 0.0, 1.0], "unturned");
    }

    #[test]
    fn faces_carry_their_texture_luma_or_their_colours() {
        let mut colours = vec![[0.0; 4]; 8];
        colours[0] = [0.5, 0.5, 0.5, 1.0]; // block 0 side
        colours[1] = [0.2, 0.2, 0.2, 1.0]; // block 0 top
        colours[3] = [0.9, 1.0, 0.0, 0.0]; // block 0 parameters
        colours[4] = [0.1, 0.1, 0.1, 1.0]; // block 1 side
        let out = with_face_luma(&colours, &[[Some(0.33), None, None]]);
        assert!((out[0][3] - 0.33).abs() < 1e-6, "textured side");
        assert!((out[1][3] - 0.2).abs() < 1e-6, "untextured top: its colour");
        assert_eq!(out[3], colours[3], "parameters untouched");
        assert!((out[4][3] - 0.1).abs() < 1e-6, "a block past the texture list");
        assert_eq!(out[0][..3], colours[0][..3]);
    }

    #[test]
    fn tints_ride_in_the_face_tables_spare_column() {
        let table = vec![[1, 2, 3, 0]; 3];
        let t = with_tints(&table, &[[255, 255, 255], [200, 120, 80]]);
        assert_eq!(t[0], [1, 2, 3, 0xffff_ffff], "white");
        assert_eq!(t[1][..3], [1, 2, 3], "layers untouched");
        assert_eq!(t[1][3], 200 | 120 << 8 | 80 << 16 | 0xff << 24);
        assert_eq!(t[2][3], 0xffff_ffff, "a block past the list is untinted");
    }
}
