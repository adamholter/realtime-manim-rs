use std::collections::HashMap;
use std::mem;
use std::path::Path;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use image::{ColorType, ImageFormat};
use realtime_manim_scene_core::{EvaluatedFrameView, ImageResampling, NodeKind, Scene};
use realtime_manim_text_engine::TextEngine;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowAttributes, WindowId};

use crate::cli::Args;
use crate::geometry::{
    DrawCommand, Geometry, IMAGE_VERTEX_ATTRIBUTES, ImageVertex, MESH_TEXTURE_VERTEX_ATTRIBUTES,
    MeshTextureVertex, VERTEX_ATTRIBUTES, Vertex, build_geometry,
};

const TARGET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

struct FilterPipelines {
    sample: wgpu::RenderPipeline,
    box_filter: wgpu::RenderPipeline,
    hamming: wgpu::RenderPipeline,
    bicubic: wgpu::RenderPipeline,
    lanczos: wgpu::RenderPipeline,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DepthMode {
    Overlay,
    Opaque,
    Transparent,
}

fn depth_state(mode: DepthMode) -> wgpu::DepthStencilState {
    wgpu::DepthStencilState {
        format: DEPTH_FORMAT,
        depth_write_enabled: Some(mode == DepthMode::Opaque),
        depth_compare: Some(match mode {
            DepthMode::Overlay => wgpu::CompareFunction::Always,
            DepthMode::Opaque | DepthMode::Transparent => wgpu::CompareFunction::LessEqual,
        }),
        stencil: wgpu::StencilState::default(),
        bias: wgpu::DepthBiasState::default(),
    }
}

struct FrameBuffers {
    vertex: wgpu::Buffer,
    index: wgpu::Buffer,
    image_vertex: wgpu::Buffer,
    mesh_texture_vertex: wgpu::Buffer,
    vertex_capacity: usize,
    index_capacity: usize,
    image_vertex_capacity: usize,
    mesh_texture_vertex_capacity: usize,
}

#[allow(clippy::too_many_arguments)]
fn create_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
    label: &str,
    vertex_entry: &str,
    fragment_entry: &str,
    stride: u64,
    attributes: &[wgpu::VertexAttribute],
    depth_mode: DepthMode,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(vertex_entry),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: stride,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes,
            })],
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(fragment_entry),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: Some(depth_state(depth_mode)),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

#[allow(clippy::too_many_arguments)]
fn create_filter_pipelines(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
    vertex_entry: &str,
    stride: u64,
    attributes: &[wgpu::VertexAttribute],
    depth_mode: DepthMode,
    label_prefix: &str,
) -> FilterPipelines {
    let fragment_prefix = if vertex_entry == "vs_image" {
        "fs_image"
    } else {
        "fs_mesh_texture"
    };
    let make = |suffix: &str| {
        create_pipeline(
            device,
            shader,
            layout,
            format,
            &format!("realtime-manim native {label_prefix} {suffix} pipeline"),
            vertex_entry,
            &format!("{fragment_prefix}_{suffix}"),
            stride,
            attributes,
            depth_mode,
        )
    };
    FilterPipelines {
        sample: make("sample"),
        box_filter: make("box"),
        hamming: make("hamming"),
        bicubic: make("bicubic"),
        lanczos: make("lanczos"),
    }
}

fn create_depth_target(
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("realtime-manim native depth target"),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

fn decode_base64(source: &str) -> Result<Vec<u8>, &'static str> {
    let compact = source
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect::<Vec<_>>();
    if compact.len() % 4 != 0 {
        return Err("encoded length is not divisible by four");
    }
    let padding = compact
        .iter()
        .rev()
        .take_while(|byte| **byte == b'=')
        .count();
    if padding > 2 || compact[..compact.len().saturating_sub(padding)].contains(&b'=') {
        return Err("padding is malformed");
    }
    let mut output = Vec::with_capacity(compact.len() / 4 * 3 - padding);
    let mut accumulator = 0_u32;
    let mut bits = 0_u8;
    for byte in compact.into_iter().take_while(|byte| *byte != b'=') {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return Err("encoded data contains a non-base64 byte"),
        };
        accumulator = (accumulator << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push(((accumulator >> bits) & 0xff) as u8);
        }
    }
    Ok(output)
}

impl FilterPipelines {
    fn get(&self, resampling: ImageResampling) -> &wgpu::RenderPipeline {
        match resampling {
            ImageResampling::Nearest | ImageResampling::Bilinear => &self.sample,
            ImageResampling::Box => &self.box_filter,
            ImageResampling::Hamming => &self.hamming,
            ImageResampling::Bicubic => &self.bicubic,
            ImageResampling::Lanczos => &self.lanczos,
        }
    }
}

struct GpuImage {
    _texture: wgpu::Texture,
    _dark_texture: wgpu::Texture,
    source: ImageSourceSignature,
    nearest_bind_group: wgpu::BindGroup,
    linear_bind_group: wgpu::BindGroup,
    reconstruction_bind_group: wgpu::BindGroup,
    opaque: bool,
}

#[derive(Debug, PartialEq, Eq)]
struct ImageSourceSignature {
    pixels: String,
    width: u32,
    height: u32,
    dark_pixels: String,
    dark_width: u32,
    dark_height: u32,
}

impl ImageSourceSignature {
    fn new(
        pixels: &str,
        width: u32,
        height: u32,
        dark_pixels: &str,
        dark_width: u32,
        dark_height: u32,
    ) -> Self {
        Self {
            pixels: pixels.to_owned(),
            width,
            height,
            dark_pixels: dark_pixels.to_owned(),
            dark_width,
            dark_height,
        }
    }

    fn matches(
        &self,
        pixels: &str,
        width: u32,
        height: u32,
        dark_pixels: &str,
        dark_width: u32,
        dark_height: u32,
    ) -> bool {
        self.width == width
            && self.height == height
            && self.dark_width == dark_width
            && self.dark_height == dark_height
            && self.pixels == pixels
            && self.dark_pixels == dark_pixels
    }
}

impl GpuImage {
    fn bind_group(&self, resampling: ImageResampling) -> &wgpu::BindGroup {
        match resampling {
            ImageResampling::Nearest => &self.nearest_bind_group,
            ImageResampling::Bilinear => &self.linear_bind_group,
            ImageResampling::Box
            | ImageResampling::Hamming
            | ImageResampling::Bicubic
            | ImageResampling::Lanczos => &self.reconstruction_bind_group,
        }
    }
}

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    vector_pipeline: wgpu::RenderPipeline,
    vector_depth_pipeline: wgpu::RenderPipeline,
    vector_transparent_depth_pipeline: wgpu::RenderPipeline,
    image_pipelines: FilterPipelines,
    mesh_texture_pipelines: FilterPipelines,
    mesh_texture_depth_pipelines: FilterPipelines,
    image_layout: wgpu::BindGroupLayout,
    nearest_sampler: wgpu::Sampler,
    linear_sampler: wgpu::Sampler,
    image_cache: HashMap<String, GpuImage>,
    image_uploads: usize,
    adapter_name: String,
    backend: wgpu::Backend,
}

impl Gpu {
    async fn new(
        instance: &wgpu::Instance,
        compatible_surface: Option<&wgpu::Surface<'_>>,
        format: wgpu::TextureFormat,
    ) -> Result<Self, String> {
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface,
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            })
            .await
            .map_err(|error| format!("No native GPU adapter is available: {error}"))?;
        Self::from_adapter(adapter, format).await
    }

    async fn from_adapter(
        adapter: wgpu::Adapter,
        format: wgpu::TextureFormat,
    ) -> Result<Self, String> {
        let info = adapter.get_info();
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("realtime-manim native device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::Off,
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
            })
            .await
            .map_err(|error| format!("Native GPU device creation failed: {error}"))?;
        let shader = device.create_shader_module(wgpu::include_wgsl!("native.wgsl"));
        let vector_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("realtime-manim native vector pipeline layout"),
            bind_group_layouts: &[],
            immediate_size: 0,
        });
        let image_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("realtime-manim native image bind group layout"),
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
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let image_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("realtime-manim native image pipeline layout"),
                bind_group_layouts: &[Some(&image_layout)],
                immediate_size: 0,
            });
        let vector_pipeline = create_pipeline(
            &device,
            &shader,
            &vector_layout,
            format,
            "realtime-manim native vector pipeline",
            "vs_main",
            "fs_main",
            mem::size_of::<Vertex>() as u64,
            &VERTEX_ATTRIBUTES,
            DepthMode::Overlay,
        );
        let vector_depth_pipeline = create_pipeline(
            &device,
            &shader,
            &vector_layout,
            format,
            "realtime-manim native depth vector pipeline",
            "vs_main",
            "fs_main",
            mem::size_of::<Vertex>() as u64,
            &VERTEX_ATTRIBUTES,
            DepthMode::Opaque,
        );
        let vector_transparent_depth_pipeline = create_pipeline(
            &device,
            &shader,
            &vector_layout,
            format,
            "realtime-manim native transparent depth vector pipeline",
            "vs_main",
            "fs_main",
            mem::size_of::<Vertex>() as u64,
            &VERTEX_ATTRIBUTES,
            DepthMode::Transparent,
        );
        let image_pipelines = create_filter_pipelines(
            &device,
            &shader,
            &image_pipeline_layout,
            format,
            "vs_image",
            mem::size_of::<ImageVertex>() as u64,
            &IMAGE_VERTEX_ATTRIBUTES,
            DepthMode::Overlay,
            "image",
        );
        let mesh_texture_pipelines = create_filter_pipelines(
            &device,
            &shader,
            &image_pipeline_layout,
            format,
            "vs_mesh_texture",
            mem::size_of::<MeshTextureVertex>() as u64,
            &MESH_TEXTURE_VERTEX_ATTRIBUTES,
            DepthMode::Transparent,
            "mesh texture",
        );
        let mesh_texture_depth_pipelines = create_filter_pipelines(
            &device,
            &shader,
            &image_pipeline_layout,
            format,
            "vs_mesh_texture",
            mem::size_of::<MeshTextureVertex>() as u64,
            &MESH_TEXTURE_VERTEX_ATTRIBUTES,
            DepthMode::Opaque,
            "depth mesh texture",
        );
        let nearest_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("realtime-manim native nearest sampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        let linear_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("realtime-manim native linear sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        Ok(Self {
            device,
            queue,
            vector_pipeline,
            vector_depth_pipeline,
            vector_transparent_depth_pipeline,
            image_pipelines,
            mesh_texture_pipelines,
            mesh_texture_depth_pipelines,
            image_layout,
            nearest_sampler,
            linear_sampler,
            image_cache: HashMap::new(),
            image_uploads: 0,
            adapter_name: info.name,
            backend: info.backend,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn encode_uploaded_frame(
        &self,
        view: &wgpu::TextureView,
        depth_view: &wgpu::TextureView,
        background: [f32; 4],
        vertex_buffer: &wgpu::Buffer,
        index_buffer: &wgpu::Buffer,
        image_vertex_buffer: &wgpu::Buffer,
        mesh_texture_vertex_buffer: &wgpu::Buffer,
        geometry: &Geometry,
    ) -> wgpu::CommandBuffer {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("realtime-manim native frame encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("realtime-manim native vector pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: f64::from(background[0]),
                            g: f64::from(background[1]),
                            b: f64::from(background[2]),
                            a: f64::from(background[3]),
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            // Populate depth with every opaque 3D primitive before blending anything.
            for command in &geometry.commands {
                match command {
                    DrawCommand::Vector {
                        indices,
                        depth_test,
                        ..
                    } => {
                        if !*depth_test {
                            continue;
                        }
                        pass.set_pipeline(&self.vector_depth_pipeline);
                        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                        pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                        pass.draw_indexed(indices.clone(), 0, 0..1);
                    }
                    DrawCommand::MeshTexture {
                        vertices,
                        key,
                        resampling,
                        opaque_candidate,
                    } => {
                        let Some(image) = self.image_cache.get(key) else {
                            continue;
                        };
                        if !(*opaque_candidate && image.opaque) {
                            continue;
                        }
                        pass.set_pipeline(self.mesh_texture_depth_pipelines.get(*resampling));
                        pass.set_bind_group(0, image.bind_group(*resampling), &[]);
                        pass.set_vertex_buffer(0, mesh_texture_vertex_buffer.slice(..));
                        pass.draw(vertices.clone(), 0..1);
                    }
                    DrawCommand::Image { .. } => {}
                }
            }

            // Geometry is pre-sorted globally, not merely within each retained node.
            for primitive in &geometry.transparent_primitives {
                let Some(command) = geometry.commands.get(primitive.command_index) else {
                    continue;
                };
                match command {
                    DrawCommand::Vector { transparent_3d, .. } => {
                        if !*transparent_3d {
                            continue;
                        }
                        pass.set_pipeline(&self.vector_transparent_depth_pipeline);
                        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                        pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                        pass.draw_indexed(primitive.range.clone(), 0, 0..1);
                    }
                    DrawCommand::MeshTexture {
                        key,
                        resampling,
                        opaque_candidate,
                        ..
                    } => {
                        let Some(image) = self.image_cache.get(key) else {
                            continue;
                        };
                        if *opaque_candidate && image.opaque {
                            continue;
                        }
                        pass.set_pipeline(self.mesh_texture_pipelines.get(*resampling));
                        pass.set_bind_group(0, image.bind_group(*resampling), &[]);
                        pass.set_vertex_buffer(0, mesh_texture_vertex_buffer.slice(..));
                        pass.draw(primitive.range.clone(), 0..1);
                    }
                    DrawCommand::Image { .. } => {}
                }
            }

            // Screen-space vectors and images retain their scene order as overlays.
            for command in &geometry.commands {
                match command {
                    DrawCommand::Vector {
                        indices,
                        depth_test,
                        transparent_3d,
                    } => {
                        if *depth_test || *transparent_3d {
                            continue;
                        }
                        pass.set_pipeline(&self.vector_pipeline);
                        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                        pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                        pass.draw_indexed(indices.clone(), 0, 0..1);
                    }
                    DrawCommand::Image {
                        vertices,
                        key,
                        resampling,
                    } => {
                        let Some(image) = self.image_cache.get(key) else {
                            continue;
                        };
                        pass.set_pipeline(self.image_pipelines.get(*resampling));
                        pass.set_bind_group(0, image.bind_group(*resampling), &[]);
                        pass.set_vertex_buffer(0, image_vertex_buffer.slice(..));
                        pass.draw(vertices.clone(), 0..1);
                    }
                    DrawCommand::MeshTexture { .. } => {}
                }
            }
        }
        encoder.finish()
    }

    fn prepare_images(&mut self, frame: &EvaluatedFrameView<'_>) -> Result<(), String> {
        for node in &frame.nodes {
            let source = match node.kind.as_ref() {
                NodeKind::Image {
                    pixels,
                    pixel_width,
                    pixel_height,
                    ..
                } => Some((pixels.as_str(), *pixel_width, *pixel_height, "", 0, 0)),
                NodeKind::Mesh {
                    texture_pixels,
                    texture_width,
                    texture_height,
                    dark_texture_pixels,
                    dark_texture_width,
                    dark_texture_height,
                    ..
                } if !texture_pixels.is_empty() => Some((
                    texture_pixels.as_str(),
                    *texture_width,
                    *texture_height,
                    dark_texture_pixels.as_str(),
                    *dark_texture_width,
                    *dark_texture_height,
                )),
                _ => None,
            };
            let Some((pixels, width, height, dark_pixels, dark_width, dark_height)) = source else {
                continue;
            };
            if self.image_cache.get(node.id).is_some_and(|image| {
                image
                    .source
                    .matches(pixels, width, height, dark_pixels, dark_width, dark_height)
            }) {
                continue;
            }
            let decoded = decode_base64(pixels)
                .map_err(|error| format!("Image {} contains invalid base64: {error}", node.id))?;
            let expected = width as usize * height as usize * 4;
            if decoded.len() != expected {
                return Err(format!(
                    "Image {} contains {} bytes; expected {expected}.",
                    node.id,
                    decoded.len()
                ));
            }
            let dark_decoded = if dark_pixels.is_empty() {
                None
            } else {
                let decoded = decode_base64(dark_pixels).map_err(|error| {
                    format!("Image {} contains invalid dark base64: {error}", node.id)
                })?;
                let expected = dark_width as usize * dark_height as usize * 4;
                if decoded.len() != expected {
                    return Err(format!(
                        "Image {} contains {} dark bytes; expected {expected}.",
                        node.id,
                        decoded.len()
                    ));
                }
                Some(decoded)
            };
            self.insert_rgba_image(
                node.id,
                ImageSourceSignature::new(
                    pixels,
                    width,
                    height,
                    dark_pixels,
                    dark_width,
                    dark_height,
                ),
                &decoded,
                width,
                height,
                dark_decoded.as_deref().unwrap_or(&decoded),
                if dark_pixels.is_empty() {
                    width
                } else {
                    dark_width
                },
                if dark_pixels.is_empty() {
                    height
                } else {
                    dark_height
                },
            );
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn insert_rgba_image(
        &mut self,
        key: &str,
        source: ImageSourceSignature,
        pixels: &[u8],
        width: u32,
        height: u32,
        dark_pixels: &[u8],
        dark_width: u32,
        dark_height: u32,
    ) {
        let upload = |label: &str, data: &[u8], width: u32, height: u32| {
            let size = wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            };
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(width * 4),
                    rows_per_image: Some(height),
                },
                size,
            );
            texture
        };
        let texture = upload("realtime-manim native image", pixels, width, height);
        let dark_texture = upload(
            "realtime-manim native dark image",
            dark_pixels,
            dark_width,
            dark_height,
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let dark_view = dark_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let create_bind_group = |sampler: &wgpu::Sampler, label: &str| {
            self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(label),
                layout: &self.image_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&dark_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(sampler),
                    },
                ],
            })
        };
        let nearest_bind_group = create_bind_group(
            &self.nearest_sampler,
            "realtime-manim native nearest image bind group",
        );
        let linear_bind_group = create_bind_group(
            &self.linear_sampler,
            "realtime-manim native linear image bind group",
        );
        let reconstruction_bind_group = create_bind_group(
            &self.nearest_sampler,
            "realtime-manim native reconstruction image bind group",
        );
        let opaque = pixels.chunks_exact(4).all(|pixel| pixel[3] == u8::MAX)
            && dark_pixels.chunks_exact(4).all(|pixel| pixel[3] == u8::MAX);
        self.image_cache.insert(
            key.to_owned(),
            GpuImage {
                _texture: texture,
                _dark_texture: dark_texture,
                source,
                nearest_bind_group,
                linear_bind_group,
                reconstruction_bind_group,
                opaque,
            },
        );
        self.image_uploads += 1;
    }
}

impl FrameBuffers {
    fn new(gpu: &Gpu) -> Self {
        let vertex_capacity = mem::size_of::<Vertex>();
        let index_capacity = mem::size_of::<u32>();
        let image_vertex_capacity = mem::size_of::<ImageVertex>();
        let mesh_texture_vertex_capacity = mem::size_of::<MeshTextureVertex>();
        let create = |label: &str, size: usize, usage: wgpu::BufferUsages| {
            gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: size as u64,
                usage: usage | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        Self {
            vertex: create(
                "realtime-manim native retained vertex buffer",
                vertex_capacity,
                wgpu::BufferUsages::VERTEX,
            ),
            index: create(
                "realtime-manim native retained index buffer",
                index_capacity,
                wgpu::BufferUsages::INDEX,
            ),
            image_vertex: create(
                "realtime-manim native retained image vertex buffer",
                image_vertex_capacity,
                wgpu::BufferUsages::VERTEX,
            ),
            mesh_texture_vertex: create(
                "realtime-manim native retained mesh texture vertex buffer",
                mesh_texture_vertex_capacity,
                wgpu::BufferUsages::VERTEX,
            ),
            vertex_capacity,
            index_capacity,
            image_vertex_capacity,
            mesh_texture_vertex_capacity,
        }
    }

    fn upload(&mut self, gpu: &Gpu, geometry: &Geometry) -> usize {
        let mut reallocations = 0;
        reallocations += usize::from(upload_slice(
            gpu,
            &mut self.vertex,
            &mut self.vertex_capacity,
            &geometry.vertices,
            wgpu::BufferUsages::VERTEX,
            "realtime-manim native retained vertex buffer",
        ));
        reallocations += usize::from(upload_slice(
            gpu,
            &mut self.index,
            &mut self.index_capacity,
            &geometry.indices,
            wgpu::BufferUsages::INDEX,
            "realtime-manim native retained index buffer",
        ));
        reallocations += usize::from(upload_slice(
            gpu,
            &mut self.image_vertex,
            &mut self.image_vertex_capacity,
            &geometry.image_vertices,
            wgpu::BufferUsages::VERTEX,
            "realtime-manim native retained image vertex buffer",
        ));
        reallocations += usize::from(upload_slice(
            gpu,
            &mut self.mesh_texture_vertex,
            &mut self.mesh_texture_vertex_capacity,
            &geometry.mesh_texture_vertices,
            wgpu::BufferUsages::VERTEX,
            "realtime-manim native retained mesh texture vertex buffer",
        ));
        reallocations
    }
}

pub fn render_headless(scene: &Scene, args: &Args, output: Option<&Path>) -> Result<(), String> {
    pollster::block_on(render_headless_async(scene, args, output))
}

async fn render_headless_async(
    scene: &Scene,
    args: &Args,
    output: Option<&Path>,
) -> Result<(), String> {
    let (width, height) = render_dimensions(scene, args);
    let frame = scene.evaluate_view(args.time)?;
    let mut text_engine = TextEngine::new().map_err(|error| error.to_string())?;
    let geometry = build_geometry(&frame, &mut text_engine)?;
    validate_geometry(&geometry, args.strict)?;
    let background = frame.background;

    let instance = native_instance();
    let mut gpu = Gpu::new(&instance, None, TARGET_FORMAT).await?;
    gpu.prepare_images(&frame)?;
    let uploads_after_cold_frame = gpu.image_uploads;
    gpu.prepare_images(&frame)?;
    let warm_texture_uploads = gpu.image_uploads - uploads_after_cold_frame;
    if warm_texture_uploads != 0 {
        return Err(format!(
            "Warmed retained frame unexpectedly uploaded {warm_texture_uploads} texture set(s)."
        ));
    }
    drop(frame);
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("realtime-manim native headless target"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: TARGET_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let (_depth_texture, depth_view) = create_depth_target(&gpu.device, width, height);
    let mut buffers = FrameBuffers::new(&gpu);
    let cold_buffer_reallocations = buffers.upload(&gpu, &geometry);
    let warm_buffer_reallocations = buffers.upload(&gpu, &geometry);
    if warm_buffer_reallocations != 0 {
        return Err(format!(
            "Warmed retained frame unexpectedly reallocated {warm_buffer_reallocations} GPU buffer(s)."
        ));
    }
    let command = gpu.encode_uploaded_frame(
        &view,
        &depth_view,
        background,
        &buffers.vertex,
        &buffers.index,
        &buffers.image_vertex,
        &buffers.mesh_texture_vertex,
        &geometry,
    );

    let unpadded_bytes_per_row = width * 4;
    let padded_bytes_per_row = unpadded_bytes_per_row.div_ceil(256) * 256;
    let output_buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("realtime-manim native readback"),
        size: u64::from(padded_bytes_per_row) * u64::from(height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut copy_encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("realtime-manim native readback encoder"),
        });
    copy_encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &output_buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_bytes_per_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    let submission = gpu.queue.submit([command, copy_encoder.finish()]);
    let (sender, receiver) = mpsc::channel();
    output_buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
    gpu.device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(Duration::from_secs(30)),
        })
        .map_err(|error| format!("GPU readback failed: {error}"))?;
    receiver
        .recv_timeout(Duration::from_secs(30))
        .map_err(|error| format!("GPU readback callback timed out: {error}"))?
        .map_err(|error| format!("GPU output mapping failed: {error}"))?;
    let mapped = output_buffer
        .slice(..)
        .get_mapped_range()
        .map_err(|error| format!("Could not read mapped GPU output: {error}"))?;
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for row in mapped.chunks_exact(padded_bytes_per_row as usize) {
        pixels.extend_from_slice(&row[..unpadded_bytes_per_row as usize]);
    }
    drop(mapped);
    output_buffer.unmap();

    if let Some(path) = output {
        image::save_buffer_with_format(
            path,
            &pixels,
            width,
            height,
            ColorType::Rgba8,
            ImageFormat::Png,
        )
        .map_err(|error| format!("Could not write {}: {error}", path.display()))?;
    }
    let checksum = fnv1a64(&pixels);
    let content_pixels = changed_pixel_count(&pixels, background);
    if geometry.triangle_count() > 0 && content_pixels == 0 {
        return Err(
            "GPU frame contains geometry but no pixels differ from the background.".to_owned(),
        );
    }
    if !geometry.unsupported.is_empty() {
        eprintln!(
            "native preview warning: skipped {} unsupported node(s): {}",
            geometry.unsupported.len(),
            geometry.unsupported.join(", ")
        );
    }
    println!(
        "native {} ok backend={:?} adapter={:?} scene={:?} time={:.3}s size={}x{} rendered_nodes={}/{} triangles={} cached_images={} cold_buffer_reallocations={} warm_buffer_reallocations={} warm_texture_uploads={} content_pixels={} checksum={checksum:016x}{}",
        if output.is_some() { "render" } else { "smoke" },
        gpu.backend,
        gpu.adapter_name,
        scene.title,
        args.time.clamp(0.0, scene.duration),
        width,
        height,
        geometry.rendered_nodes,
        geometry.visible_nodes,
        geometry.triangle_count(),
        gpu.image_cache.len(),
        cold_buffer_reallocations,
        warm_buffer_reallocations,
        warm_texture_uploads,
        content_pixels,
        output.map_or_else(String::new, |path| format!(" output={}", path.display())),
    );
    Ok(())
}

pub fn play(scene: Scene, args: &Args) -> Result<(), String> {
    let event_loop =
        EventLoop::new().map_err(|error| format!("Window event loop failed: {error}"))?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let (width, height) = render_dimensions(&scene, args);
    let mut app = PreviewApp {
        scene,
        width,
        height,
        strict: args.strict,
        window: None,
        renderer: None,
        playback: Playback::new(args.time, args.paused),
        error: None,
        last_title_update: Instant::now(),
    };
    event_loop
        .run_app(&mut app)
        .map_err(|error| format!("Native window failed: {error}"))?;
    app.error.map_or(Ok(()), Err)
}

struct PreviewApp {
    scene: Scene,
    width: u32,
    height: u32,
    strict: bool,
    window: Option<Arc<Window>>,
    renderer: Option<SurfaceRenderer>,
    playback: Playback,
    error: Option<String>,
    last_title_update: Instant,
}

impl ApplicationHandler for PreviewApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = WindowAttributes::default()
            .with_title(format!("{} — realtime-manim native", self.scene.title))
            .with_inner_size(PhysicalSize::new(self.width, self.height));
        let result = event_loop
            .create_window(attributes)
            .map(Arc::new)
            .map_err(|error| format!("Could not create native preview window: {error}"))
            .and_then(|window| {
                pollster::block_on(SurfaceRenderer::new(
                    window.clone(),
                    self.scene.clone(),
                    self.strict,
                ))
                .map(|renderer| (window, renderer))
            });
        match result {
            Ok((window, renderer)) => {
                eprintln!(
                    "native play backend={:?} adapter={:?} controls=Space/Left/Right/Home/R/Esc",
                    renderer.gpu.backend, renderer.gpu.adapter_name
                );
                window.request_redraw();
                self.renderer = Some(renderer);
                self.window = Some(window);
            }
            Err(error) => {
                self.error = Some(error);
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if self.window.as_ref().map(|window| window.id()) != Some(window_id) {
            return;
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.resize(size.width, size.height);
                }
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                match event.physical_key {
                    PhysicalKey::Code(KeyCode::Escape) => event_loop.exit(),
                    PhysicalKey::Code(KeyCode::Space) => self.playback.toggle(),
                    PhysicalKey::Code(KeyCode::ArrowLeft) => {
                        self.playback.seek(-0.25, self.scene.duration)
                    }
                    PhysicalKey::Code(KeyCode::ArrowRight) => {
                        self.playback.seek(0.25, self.scene.duration)
                    }
                    PhysicalKey::Code(KeyCode::Home | KeyCode::KeyR) => self.playback.restart(),
                    _ => {}
                }
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                let time = self.playback.time(self.scene.duration);
                if let Some(renderer) = self.renderer.as_mut()
                    && let Err(error) = renderer.render(time)
                {
                    self.error = Some(error);
                    event_loop.exit();
                    return;
                }
                if self.last_title_update.elapsed() >= Duration::from_millis(200) {
                    if let Some(window) = &self.window {
                        window.set_title(&format!(
                            "{} — {:.2}/{:.2}s{} — realtime-manim native",
                            self.scene.title,
                            time,
                            self.scene.duration,
                            if self.playback.paused { " paused" } else { "" }
                        ));
                    }
                    self.last_title_update = Instant::now();
                }
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

struct Playback {
    base_time: f32,
    anchor: Instant,
    paused: bool,
}

impl Playback {
    fn new(time: f32, paused: bool) -> Self {
        Self {
            base_time: time,
            anchor: Instant::now(),
            paused,
        }
    }

    fn time(&self, duration: f32) -> f32 {
        if duration <= f32::EPSILON {
            return 0.0;
        }
        let elapsed = if self.paused {
            0.0
        } else {
            self.anchor.elapsed().as_secs_f32()
        };
        (self.base_time + elapsed).rem_euclid(duration)
    }

    fn toggle(&mut self) {
        if self.paused {
            self.anchor = Instant::now();
            self.paused = false;
        } else {
            self.base_time += self.anchor.elapsed().as_secs_f32();
            self.anchor = Instant::now();
            self.paused = true;
        }
    }

    fn seek(&mut self, delta: f32, duration: f32) {
        self.base_time = (self.time(duration) + delta).clamp(0.0, duration);
        self.anchor = Instant::now();
    }

    fn restart(&mut self) {
        self.base_time = 0.0;
        self.anchor = Instant::now();
    }
}

struct SurfaceRenderer {
    _window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    gpu: Gpu,
    config: wgpu::SurfaceConfiguration,
    scene: Scene,
    text_engine: TextEngine,
    frame_buffers: FrameBuffers,
    _depth_texture: wgpu::Texture,
    depth_view: wgpu::TextureView,
    strict: bool,
    warned_unsupported: bool,
}

impl SurfaceRenderer {
    async fn new(window: Arc<Window>, scene: Scene, strict: bool) -> Result<Self, String> {
        let size = window.inner_size();
        let instance = native_instance();
        let surface = instance
            .create_surface(window.clone())
            .map_err(|error| format!("Could not create Metal surface: {error}"))?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            })
            .await
            .map_err(|error| format!("No surface-compatible GPU adapter is available: {error}"))?;
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .ok_or_else(|| "The native surface has no compatible texture format.".to_owned())?;
        let formats = surface.get_capabilities(&adapter).formats;
        let linear_format = match config.format {
            wgpu::TextureFormat::Rgba8UnormSrgb => Some(wgpu::TextureFormat::Rgba8Unorm),
            wgpu::TextureFormat::Bgra8UnormSrgb => Some(wgpu::TextureFormat::Bgra8Unorm),
            _ => None,
        };
        if let Some(format) = linear_format.filter(|format| formats.contains(format)) {
            config.format = format;
        }
        config.present_mode = wgpu::PresentMode::Fifo;
        let gpu = Gpu::from_adapter(adapter, config.format).await?;
        surface.configure(&gpu.device, &config);
        let frame_buffers = FrameBuffers::new(&gpu);
        let (depth_texture, depth_view) =
            create_depth_target(&gpu.device, config.width, config.height);
        Ok(Self {
            _window: window,
            surface,
            gpu,
            config,
            scene,
            text_engine: TextEngine::new().map_err(|error| error.to_string())?,
            frame_buffers,
            _depth_texture: depth_texture,
            depth_view,
            strict,
            warned_unsupported: false,
        })
    }

    fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.gpu.device, &self.config);
        let (texture, view) = create_depth_target(&self.gpu.device, width, height);
        self._depth_texture = texture;
        self.depth_view = view;
    }

    fn render(&mut self, time: f32) -> Result<(), String> {
        let frame = self.scene.evaluate_view(time)?;
        let geometry = build_geometry(&frame, &mut self.text_engine)?;
        validate_geometry(&geometry, self.strict)?;
        if !self.warned_unsupported && !geometry.unsupported.is_empty() {
            eprintln!(
                "native preview warning: skipped {} unsupported node(s): {}",
                geometry.unsupported.len(),
                geometry.unsupported.join(", ")
            );
            self.warned_unsupported = true;
        }
        let background = frame.background;
        self.gpu.prepare_images(&frame)?;
        drop(frame);
        self.frame_buffers.upload(&self.gpu, &geometry);
        let texture = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture)
            | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.gpu.device, &self.config);
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err("Native surface validation failed.".to_owned());
            }
        };
        let view = texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.gpu.queue.submit([self.gpu.encode_uploaded_frame(
            &view,
            &self.depth_view,
            background,
            &self.frame_buffers.vertex,
            &self.frame_buffers.index,
            &self.frame_buffers.image_vertex,
            &self.frame_buffers.mesh_texture_vertex,
            &geometry,
        )]);
        self.gpu.queue.present(texture);
        Ok(())
    }
}

fn upload_slice<T: bytemuck::Pod>(
    gpu: &Gpu,
    buffer: &mut wgpu::Buffer,
    capacity: &mut usize,
    values: &[T],
    usage: wgpu::BufferUsages,
    label: &str,
) -> bool {
    let bytes = bytemuck::cast_slice(values);
    let grown = grown_buffer_capacity(*capacity, bytes.len());
    if let Some(grown) = grown {
        *capacity = grown;
        *buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: *capacity as u64,
            usage: usage | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
    }
    if !bytes.is_empty() {
        gpu.queue.write_buffer(buffer, 0, bytes);
    }
    grown.is_some()
}

fn grown_buffer_capacity(current: usize, required: usize) -> Option<usize> {
    (required > current).then(|| required.next_power_of_two())
}

fn native_instance() -> wgpu::Instance {
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    descriptor.backends = if cfg!(target_os = "macos") {
        wgpu::Backends::METAL
    } else {
        wgpu::Backends::PRIMARY
    };
    wgpu::Instance::new(descriptor)
}

fn render_dimensions(scene: &Scene, args: &Args) -> (u32, u32) {
    (
        args.width.unwrap_or(scene.pixel_width).max(1),
        args.height.unwrap_or(scene.pixel_height).max(1),
    )
}

fn validate_geometry(geometry: &Geometry, strict: bool) -> Result<(), String> {
    if strict && !geometry.unsupported.is_empty() {
        return Err(format!(
            "Native renderer does not yet support: {}.",
            geometry.unsupported.join(", ")
        ));
    }
    if geometry.visible_nodes > 0 && geometry.rendered_nodes == 0 {
        return Err(format!(
            "No visible nodes are supported by the native renderer: {}.",
            geometry.unsupported.join(", ")
        ));
    }
    Ok(())
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn changed_pixel_count(pixels: &[u8], background: [f32; 4]) -> usize {
    let background = background.map(|channel| (channel.clamp(0.0, 1.0) * 255.0).round() as u8);
    pixels
        .chunks_exact(4)
        .filter(|pixel| *pixel != background)
        .count()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    #[cfg(target_os = "macos")]
    use crate::cli::{Args, Command};
    use crate::geometry::Geometry;
    #[cfg(target_os = "macos")]
    use realtime_manim_scene_core::Scene;

    use super::{
        DepthMode, ImageSourceSignature, Playback, changed_pixel_count, decode_base64, depth_state,
        fnv1a64, grown_buffer_capacity, validate_geometry,
    };
    #[cfg(target_os = "macos")]
    use super::{Gpu, TARGET_FORMAT, native_instance, render_headless};

    #[cfg(target_os = "macos")]
    fn render_metal_center_pixel(scene_json: &str, slug: &str) -> [u8; 4] {
        let scene = Scene::from_json(scene_json).unwrap();
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let output = std::env::temp_dir().join(format!(
            "realtime-manim-{slug}-{}-{unique}.png",
            std::process::id()
        ));
        let args = Args {
            command: Command::Render,
            scene_path: None,
            output_path: Some(output.clone()),
            time: 0.0,
            width: Some(64),
            height: Some(64),
            paused: false,
            strict: true,
        };
        render_headless(&scene, &args, Some(&output)).unwrap();
        let pixels = image::open(&output).unwrap().into_rgba8();
        let center = pixels.get_pixel(32, 32).0;
        std::fs::remove_file(&output).unwrap();
        center
    }

    #[test]
    fn transparent_depth_mode_tests_without_writing_depth() {
        let state = depth_state(DepthMode::Transparent);
        assert_eq!(state.depth_write_enabled, Some(false));
        assert_eq!(state.depth_compare, Some(wgpu::CompareFunction::LessEqual));
    }

    #[test]
    fn base64_decoder_handles_rgba_and_rejects_malformed_padding() {
        assert_eq!(decode_base64("/wAA/w==").unwrap(), [255, 0, 0, 255]);
        assert_eq!(decode_base64("AAEC").unwrap(), [0, 1, 2]);
        assert!(decode_base64("A===").is_err());
        assert!(decode_base64("AA=A").is_err());
    }

    #[test]
    fn strict_mode_names_every_unsupported_node_exactly() {
        let geometry = Geometry {
            vertices: Vec::new(),
            indices: Vec::new(),
            image_vertices: Vec::new(),
            mesh_texture_vertices: Vec::new(),
            commands: Vec::new(),
            transparent_primitives: Vec::new(),
            visible_nodes: 2,
            rendered_nodes: 1,
            unsupported: vec!["logo:svg".to_owned(), "effect:customShaderMesh".to_owned()],
        };
        assert_eq!(
            validate_geometry(&geometry, true).unwrap_err(),
            "Native renderer does not yet support: logo:svg, effect:customShaderMesh."
        );
    }

    #[test]
    fn retained_image_signature_reuses_exact_source_and_invalidates_same_id_changes() {
        let source = ImageSourceSignature::new("/wAA/w==", 1, 1, "", 0, 0);
        assert!(source.matches("/wAA/w==", 1, 1, "", 0, 0));
        assert!(!source.matches("AAD//w==", 1, 1, "", 0, 0));
        assert!(!source.matches("/wAA/w==", 2, 1, "", 0, 0));
        assert!(!source.matches("/wAA/w==", 1, 1, "AAAAAA==", 1, 1));
    }

    #[test]
    fn warmed_retained_buffers_only_grow_when_capacity_is_exceeded() {
        assert_eq!(grown_buffer_capacity(1024, 1024), None);
        assert_eq!(grown_buffer_capacity(1024, 1000), None);
        assert_eq!(grown_buffer_capacity(1024, 1025), Some(2048));
        assert_eq!(grown_buffer_capacity(2048, 1500), None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn metal_translucent_geometry_behind_opaque_depth_is_rejected() {
        let pixel = render_metal_center_pixel(
            r##"{
              "version":2,"title":"transparent behind opaque","width":4,"height":4,"pixelWidth":64,"pixelHeight":64,"duration":1,
              "background":"#000000",
              "camera3d":{"position":[0,0,0],"target":[0,0,1],"up":[0,1,0],"fovY":0.9,"near":1,"far":10,"ambient":1},
              "nodes":[
                {"id":"opaque-front","type":"mesh","vertices":[[-2,-2,2],[0,2,2],[2,-2,2]],"triangles":[[0,1,2]],"colors":["#ff0000ff","#ff0000ff","#ff0000ff"],"unlit":true,"doubleSided":true,"style":{"fill":"#ffffff","stroke":null}},
                {"id":"transparent-behind","type":"mesh","vertices":[[-4,-4,4],[0,4,4],[4,-4,4]],"triangles":[[0,1,2]],"colors":["#0000ffff","#0000ffff","#0000ffff"],"unlit":true,"doubleSided":true,"style":{"fill":"#ffffff","stroke":null,"opacity":0.5}}
              ]
            }"##,
            "transparent-depth",
        );
        assert!(pixel[0] > 245, "expected opaque red, got {pixel:?}");
        assert!(pixel[2] < 10, "behind blue leaked through depth: {pixel:?}");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn metal_transparent_nodes_blend_globally_far_to_near() {
        let pixel = render_metal_center_pixel(
            r##"{
              "version":2,"title":"global transparent order","width":4,"height":4,"pixelWidth":64,"pixelHeight":64,"duration":1,
              "background":"#000000",
              "camera3d":{"position":[0,0,0],"target":[0,0,1],"up":[0,1,0],"fovY":0.9,"near":1,"far":10,"ambient":1},
              "nodes":[
                {"id":"near-blue-first","type":"mesh","vertices":[[-2,-2,2],[0,2,2],[2,-2,2]],"triangles":[[0,1,2]],"colors":["#0000ffff","#0000ffff","#0000ffff"],"unlit":true,"doubleSided":true,"style":{"fill":"#ffffff","stroke":null,"opacity":0.5}},
                {"id":"far-red-second","type":"mesh","vertices":[[-4,-4,4],[0,4,4],[4,-4,4]],"triangles":[[0,1,2]],"colors":["#ff0000ff","#ff0000ff","#ff0000ff"],"unlit":true,"doubleSided":true,"style":{"fill":"#ffffff","stroke":null,"opacity":0.5}}
              ]
            }"##,
            "transparent-global-order",
        );
        assert!(
            pixel[2] > pixel[0],
            "near blue must blend after far red regardless of node order: {pixel:?}"
        );
        assert!(
            pixel[0] > 40 && pixel[2] > 110,
            "unexpected blend: {pixel:?}"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn metal_cache_survives_lifetime_gap_and_reuploads_changed_pixels_for_same_id() {
        pollster::block_on(async {
            let scene = Scene::from_json(
                r##"{
                  "version":2,"title":"red lifetime","width":4,"height":4,"duration":2,
                  "background":"#000000","nodes":[{
                    "id":"same-image","type":"image","pixels":"/wAA/w==",
                    "pixelWidth":1,"pixelHeight":1,
                    "corners":[[-1,1],[1,1],[-1,-1],[1,-1]],"disappearAt":1,
                    "style":{"stroke":null}
                  }]
                }"##,
            )
            .unwrap();
            let replacement = Scene::from_json(
                r##"{
                  "version":2,"title":"blue replacement","width":4,"height":4,"duration":2,
                  "background":"#000000","nodes":[{
                    "id":"same-image","type":"image","pixels":"AAD//w==",
                    "pixelWidth":1,"pixelHeight":1,
                    "corners":[[-1,1],[1,1],[-1,-1],[1,-1]],
                    "style":{"stroke":null}
                  }]
                }"##,
            )
            .unwrap();
            let instance = native_instance();
            let mut gpu = Gpu::new(&instance, None, TARGET_FORMAT).await.unwrap();

            let red = scene.evaluate_view(0.0).unwrap();
            gpu.prepare_images(&red).unwrap();
            assert_eq!(gpu.image_uploads, 1);
            assert_eq!(gpu.image_cache["same-image"].source.pixels, "/wAA/w==");

            let lifetime_gap = scene.evaluate_view(1.5).unwrap();
            assert!(lifetime_gap.nodes.is_empty());
            gpu.prepare_images(&lifetime_gap).unwrap();
            assert_eq!(gpu.image_uploads, 1);
            assert_eq!(gpu.image_cache.len(), 1);

            let blue = replacement.evaluate_view(0.0).unwrap();
            gpu.prepare_images(&blue).unwrap();
            assert_eq!(gpu.image_uploads, 2);
            assert_eq!(gpu.image_cache["same-image"].source.pixels, "AAD//w==");
            gpu.prepare_images(&blue).unwrap();
            assert_eq!(gpu.image_uploads, 2);
        });
    }

    #[test]
    fn readback_metrics_detect_content() {
        let pixels = [0, 0, 0, 255, 0, 0, 0, 255, 255, 0, 0, 255];
        assert_eq!(changed_pixel_count(&pixels, [0.0, 0.0, 0.0, 1.0]), 1);
        assert_ne!(fnv1a64(&pixels), 0);
    }

    #[test]
    fn playback_seek_and_pause_are_explicit() {
        let mut playback = Playback::new(1.0, true);
        assert_eq!(playback.time(5.0), 1.0);
        playback.seek(0.25, 5.0);
        assert_eq!(playback.time(5.0), 1.25);
        playback.toggle();
        std::thread::sleep(Duration::from_millis(2));
        assert!(playback.time(5.0) > 1.25);
    }
}
