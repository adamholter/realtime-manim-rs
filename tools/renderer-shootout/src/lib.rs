//! Browser half of the experimental renderer architecture shootout.

#[cfg(all(target_arch = "wasm32", feature = "browser-vello"))]
mod browser {
    use std::num::NonZeroUsize;

    use futures_intrusive::channel::shared::oneshot_channel;
    use serde::Serialize;
    use vello::{
        AaConfig, AaSupport, RenderParams, Renderer, RendererOptions, Scene,
        kurbo::{Affine, BezPath},
        peniko::{Color, Fill},
    };
    use wasm_bindgen::prelude::*;

    const BACKGROUND: [u8; 4] = [18, 20, 28, 255];

    #[derive(Serialize)]
    struct BrowserProbe {
        backend: &'static str,
        adapter_available: bool,
        width: u32,
        height: u32,
        shapes: usize,
        iterations: usize,
        frame_p50_ms: f64,
        frame_p95_ms: f64,
        frame_p99_ms: f64,
        frame_mean_ms: f64,
        content_pixels: usize,
        content_bounds: [u32; 4],
        occupancy_64x36: Vec<u32>,
        checksum: String,
        retained_encoding_bytes: u64,
    }

    /// Run a real Vello/WebGPU offscreen render and return measured evidence.
    #[wasm_bindgen]
    pub async fn run_vello_browser_probe(
        width: u32,
        height: u32,
        iterations: u32,
    ) -> Result<String, JsValue> {
        if !(64..=4096).contains(&width) || !(64..=4096).contains(&height) {
            return Err(JsValue::from_str("width and height must be 64–4096"));
        }
        if !(1..=1_000).contains(&iterations) {
            return Err(JsValue::from_str("iterations must be 1–1000"));
        }

        let mut descriptor = vello::wgpu::InstanceDescriptor::new_without_display_handle();
        descriptor.backends = vello::wgpu::Backends::BROWSER_WEBGPU;
        let instance = vello::wgpu::Instance::new(descriptor);
        let adapter = instance
            .request_adapter(&vello::wgpu::RequestAdapterOptions {
                power_preference: vello::wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .map_err(|error| js_error("WebGPU adapter request failed", error))?;
        let adapter_available = true;
        let (device, queue) = adapter
            .request_device(&vello::wgpu::DeviceDescriptor {
                label: Some("realtime-manim Vello browser probe"),
                required_features: vello::wgpu::Features::empty(),
                required_limits: vello::wgpu::Limits::downlevel_webgl2_defaults(),
                memory_hints: vello::wgpu::MemoryHints::Performance,
                trace: vello::wgpu::Trace::Off,
                experimental_features: vello::wgpu::ExperimentalFeatures::disabled(),
            })
            .await
            .map_err(|error| js_error("WebGPU device request failed", error))?;
        let scene = primitive_scene(width, height);
        let retained_encoding_bytes = retained_bytes(&scene);
        let texture = device.create_texture(&vello::wgpu::TextureDescriptor {
            label: Some("realtime-manim Vello browser target"),
            size: vello::wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: vello::wgpu::TextureDimension::D2,
            format: vello::wgpu::TextureFormat::Rgba8Unorm,
            usage: vello::wgpu::TextureUsages::STORAGE_BINDING
                | vello::wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&vello::wgpu::TextureViewDescriptor::default());
        let mut renderer = Renderer::new(
            &device,
            RendererOptions {
                use_cpu: false,
                antialiasing_support: AaSupport::area_only(),
                num_init_threads: NonZeroUsize::new(1),
                pipeline_cache: None,
            },
        )
        .map_err(|error| js_error("Vello renderer initialization failed", error))?;
        let params = RenderParams {
            base_color: Color::from_rgba8(
                BACKGROUND[0],
                BACKGROUND[1],
                BACKGROUND[2],
                BACKGROUND[3],
            ),
            width,
            height,
            antialiasing_method: AaConfig::Area,
        };

        for _ in 0..4 {
            renderer
                .render_to_texture(&device, &queue, &scene, &view, &params)
                .map_err(|error| js_error("Vello warmup render failed", error))?;
            wait_for_queue(&queue).await?;
        }

        let performance = web_sys::window()
            .and_then(|window| window.performance())
            .ok_or_else(|| JsValue::from_str("Performance API is unavailable"))?;
        let mut samples = Vec::with_capacity(iterations as usize);
        for _ in 0..iterations {
            let start = performance.now();
            renderer
                .render_to_texture(&device, &queue, &scene, &view, &params)
                .map_err(|error| js_error("Vello browser render failed", error))?;
            wait_for_queue(&queue).await?;
            samples.push(performance.now() - start);
        }
        let pixels = read_texture(&device, &queue, &texture, width, height).await?;
        let (content_pixels, content_bounds, occupancy_64x36) =
            content_signature(&pixels, width, height);
        let checksum = format!("{:016x}", fnv1a64(&pixels));
        samples.sort_by(f64::total_cmp);
        let result = BrowserProbe {
            backend: "vello-0.9/WebGPU",
            adapter_available,
            width,
            height,
            shapes: 1_248,
            iterations: iterations as usize,
            frame_p50_ms: percentile(&samples, 0.50),
            frame_p95_ms: percentile(&samples, 0.95),
            frame_p99_ms: percentile(&samples, 0.99),
            frame_mean_ms: samples.iter().sum::<f64>() / samples.len() as f64,
            content_pixels,
            content_bounds,
            occupancy_64x36,
            checksum,
            retained_encoding_bytes,
        };
        serde_json::to_string_pretty(&result)
            .map_err(|error| js_error("Could not encode browser evidence", error))
    }

    fn primitive_scene(width: u32, height: u32) -> Scene {
        let mut scene = Scene::new();
        let columns = 48;
        let rows = 26;
        for row in 0..rows {
            for column in 0..columns {
                let index = row * columns + column;
                let x = (column as f64 + 0.5) * f64::from(width) / f64::from(columns);
                let y = (row as f64 + 0.5) * f64::from(height) / f64::from(rows);
                let radius = 5.0 + f64::from((row * 7 + column * 3) % 7);
                let color = palette(index as usize);
                let path = if (row + column) % 2 == 0 {
                    circle_path(x, y, radius)
                } else {
                    diamond_path(x, y, radius)
                };
                scene.fill(Fill::NonZero, Affine::IDENTITY, color, None, &path);
            }
        }
        scene
    }

    fn content_signature(pixels: &[u8], width: u32, height: u32) -> (usize, [u32; 4], Vec<u32>) {
        let mut count = 0;
        let mut bounds = [width, height, 0, 0];
        let mut occupancy = vec![0_u32; 64 * 36];
        for (index, pixel) in pixels.chunks_exact(4).enumerate() {
            if pixel == BACKGROUND {
                continue;
            }
            count += 1;
            let x = index as u32 % width;
            let y = index as u32 / width;
            bounds[0] = bounds[0].min(x);
            bounds[1] = bounds[1].min(y);
            bounds[2] = bounds[2].max(x);
            bounds[3] = bounds[3].max(y);
            let cell_x = (x * 64 / width).min(63);
            let cell_y = (y * 36 / height).min(35);
            occupancy[(cell_y * 64 + cell_x) as usize] += 1;
        }
        if count == 0 {
            bounds = [0; 4];
        }
        (count, bounds, occupancy)
    }

    fn circle_path(x: f64, y: f64, radius: f64) -> BezPath {
        let control = radius * 0.552_284_8;
        let mut path = BezPath::new();
        path.move_to((x + radius, y));
        path.curve_to(
            (x + radius, y + control),
            (x + control, y + radius),
            (x, y + radius),
        );
        path.curve_to(
            (x - control, y + radius),
            (x - radius, y + control),
            (x - radius, y),
        );
        path.curve_to(
            (x - radius, y - control),
            (x - control, y - radius),
            (x, y - radius),
        );
        path.curve_to(
            (x + control, y - radius),
            (x + radius, y - control),
            (x + radius, y),
        );
        path.close_path();
        path
    }

    fn diamond_path(x: f64, y: f64, radius: f64) -> BezPath {
        let inset = radius * 0.28;
        let mut path = BezPath::new();
        path.move_to((x, y - radius));
        path.quad_to((x + inset, y - inset), (x + radius, y));
        path.quad_to((x + inset, y + inset), (x, y + radius));
        path.quad_to((x - inset, y + inset), (x - radius, y));
        path.quad_to((x - inset, y - inset), (x, y - radius));
        path.close_path();
        path
    }

    fn palette(index: usize) -> Color {
        const COLORS: [[u8; 3]; 6] = [
            [79, 199, 250],
            [250, 107, 140],
            [148, 232, 140],
            [245, 194, 79],
            [176, 125, 245],
            [77, 232, 207],
        ];
        let color = COLORS[index % COLORS.len()];
        Color::from_rgb8(color[0], color[1], color[2])
    }

    async fn wait_for_queue(queue: &vello::wgpu::Queue) -> Result<(), JsValue> {
        let (sender, receiver) = oneshot_channel();
        queue.on_submitted_work_done(move || {
            sender.send(()).ok();
        });
        receiver
            .receive()
            .await
            .ok_or_else(|| JsValue::from_str("WebGPU completion callback was dropped"))
    }

    async fn read_texture(
        device: &vello::wgpu::Device,
        queue: &vello::wgpu::Queue,
        texture: &vello::wgpu::Texture,
        width: u32,
        height: u32,
    ) -> Result<Vec<u8>, JsValue> {
        let padded = (width * 4).div_ceil(256) * 256;
        let buffer = device.create_buffer(&vello::wgpu::BufferDescriptor {
            label: Some("realtime-manim Vello browser readback"),
            size: u64::from(padded) * u64::from(height),
            usage: vello::wgpu::BufferUsages::COPY_DST | vello::wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&vello::wgpu::CommandEncoderDescriptor {
            label: Some("realtime-manim Vello browser readback encoder"),
        });
        encoder.copy_texture_to_buffer(
            vello::wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: vello::wgpu::Origin3d::ZERO,
                aspect: vello::wgpu::TextureAspect::All,
            },
            vello::wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: vello::wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(height),
                },
            },
            vello::wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        queue.submit([encoder.finish()]);
        let (sender, receiver) = oneshot_channel();
        buffer
            .slice(..)
            .map_async(vello::wgpu::MapMode::Read, move |result| {
                sender.send(result).ok();
            });
        receiver
            .receive()
            .await
            .ok_or_else(|| JsValue::from_str("WebGPU map callback was dropped"))?
            .map_err(|error| js_error("WebGPU readback mapping failed", error))?;
        let mapped = buffer.slice(..).get_mapped_range();
        let unpadded = width as usize * 4;
        let mut pixels = Vec::with_capacity(unpadded * height as usize);
        for row in mapped.chunks_exact(padded as usize) {
            pixels.extend_from_slice(&row[..unpadded]);
        }
        drop(mapped);
        buffer.unmap();
        Ok(pixels)
    }

    fn retained_bytes(scene: &Scene) -> u64 {
        let encoding = scene.encoding();
        [
            std::mem::size_of_val(encoding.path_tags.as_slice()),
            std::mem::size_of_val(encoding.path_data.as_slice()),
            std::mem::size_of_val(encoding.draw_tags.as_slice()),
            std::mem::size_of_val(encoding.draw_data.as_slice()),
            std::mem::size_of_val(encoding.transforms.as_slice()),
            std::mem::size_of_val(encoding.styles.as_slice()),
        ]
        .into_iter()
        .sum::<usize>() as u64
    }

    fn percentile(values: &[f64], quantile: f64) -> f64 {
        let index = ((values.len() - 1) as f64 * quantile).ceil() as usize;
        values[index]
    }

    fn fnv1a64(bytes: &[u8]) -> u64 {
        bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100_0000_01b3)
        })
    }

    fn js_error(context: &str, error: impl std::fmt::Display) -> JsValue {
        JsValue::from_str(&format!("{context}: {error}"))
    }
}

#[cfg(all(target_arch = "wasm32", feature = "browser-vello"))]
pub use browser::run_vello_browser_probe;

#[cfg(all(target_arch = "wasm32", feature = "browser-lyon"))]
mod browser_lyon {
    use futures_intrusive::channel::shared::oneshot_channel;
    use lyon::{
        math::point,
        path::Path,
        tessellation::{BuffersBuilder, FillOptions, FillTessellator, FillVertex, VertexBuffers},
    };
    use serde::Serialize;
    use wasm_bindgen::prelude::*;

    const BACKGROUND: [u8; 4] = [18, 20, 28, 255];

    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct Vertex {
        position: [f32; 2],
        color: [f32; 4],
    }

    #[derive(Serialize)]
    struct BrowserProbe {
        backend: &'static str,
        adapter_available: bool,
        width: u32,
        height: u32,
        shapes: usize,
        iterations: usize,
        frame_p50_ms: f64,
        frame_p95_ms: f64,
        frame_p99_ms: f64,
        frame_mean_ms: f64,
        content_pixels: usize,
        content_bounds: [u32; 4],
        occupancy_64x36: Vec<u32>,
        checksum: String,
        retained_geometry_bytes: u64,
    }

    /// Run the current lyon + wgpu architecture on the browser WebGPU backend.
    #[wasm_bindgen]
    pub async fn run_lyon_browser_probe(
        width: u32,
        height: u32,
        iterations: u32,
    ) -> Result<String, JsValue> {
        if !(64..=4096).contains(&width) || !(64..=4096).contains(&height) {
            return Err(JsValue::from_str("width and height must be 64–4096"));
        }
        if !(1..=1_000).contains(&iterations) {
            return Err(JsValue::from_str("iterations must be 1–1000"));
        }

        let geometry = primitive_geometry(width, height)?;
        let retained_geometry_bytes = (geometry.vertices.len() * std::mem::size_of::<Vertex>()
            + geometry.indices.len() * std::mem::size_of::<u32>())
            as u64;
        let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
        descriptor.backends = wgpu::Backends::BROWSER_WEBGPU;
        let instance = wgpu::Instance::new(descriptor);
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            })
            .await
            .map_err(|error| js_error("WebGPU adapter request failed", error))?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("realtime-manim lyon browser probe"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::Off,
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
            })
            .await
            .map_err(|error| js_error("WebGPU device request failed", error))?;
        let shader = device.create_shader_module(wgpu::include_wgsl!("shootout.wgsl"));
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("realtime-manim lyon browser layout"),
            bind_group_layouts: &[],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("realtime-manim lyon browser pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 0,
                            shader_location: 0,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: 8,
                            shader_location: 1,
                        },
                    ],
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 4,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        });
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("realtime-manim lyon browser target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let msaa = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("realtime-manim lyon browser MSAA"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 4,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let msaa_view = msaa.create_view(&wgpu::TextureViewDescriptor::default());
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("realtime-manim lyon browser vertices"),
            size: (geometry.vertices.len() * std::mem::size_of::<Vertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&vertex_buffer, 0, bytemuck::cast_slice(&geometry.vertices));
        let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("realtime-manim lyon browser indices"),
            size: (geometry.indices.len() * std::mem::size_of::<u32>()) as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&index_buffer, 0, bytemuck::cast_slice(&geometry.indices));

        let render = || {
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("realtime-manim lyon browser frame"),
            });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("realtime-manim lyon browser pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &msaa_view,
                        depth_slice: None,
                        resolve_target: Some(&target_view),
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: f64::from(BACKGROUND[0]) / 255.0,
                                g: f64::from(BACKGROUND[1]) / 255.0,
                                b: f64::from(BACKGROUND[2]) / 255.0,
                                a: 1.0,
                            }),
                            store: wgpu::StoreOp::Discard,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(&pipeline);
                pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..geometry.indices.len() as u32, 0, 0..1);
            }
            queue.submit([encoder.finish()]);
        };

        for _ in 0..4 {
            render();
            wait_for_queue(&queue).await?;
        }
        let performance = web_sys::window()
            .and_then(|window| window.performance())
            .ok_or_else(|| JsValue::from_str("Performance API is unavailable"))?;
        let mut samples = Vec::with_capacity(iterations as usize);
        for _ in 0..iterations {
            let start = performance.now();
            render();
            wait_for_queue(&queue).await?;
            samples.push(performance.now() - start);
        }
        let pixels = read_texture(&device, &queue, &target, width, height).await?;
        let (content_pixels, content_bounds, occupancy_64x36) =
            content_signature(&pixels, width, height);
        samples.sort_by(f64::total_cmp);
        let result = BrowserProbe {
            backend: "lyon-wgpu-30/WebGPU",
            adapter_available: true,
            width,
            height,
            shapes: 1_248,
            iterations: iterations as usize,
            frame_p50_ms: percentile(&samples, 0.50),
            frame_p95_ms: percentile(&samples, 0.95),
            frame_p99_ms: percentile(&samples, 0.99),
            frame_mean_ms: samples.iter().sum::<f64>() / samples.len() as f64,
            content_pixels,
            content_bounds,
            occupancy_64x36,
            checksum: format!("{:016x}", fnv1a64(&pixels)),
            retained_geometry_bytes,
        };
        serde_json::to_string_pretty(&result)
            .map_err(|error| js_error("Could not encode browser evidence", error))
    }

    struct Geometry {
        vertices: Vec<Vertex>,
        indices: Vec<u32>,
    }

    fn content_signature(pixels: &[u8], width: u32, height: u32) -> (usize, [u32; 4], Vec<u32>) {
        let mut count = 0;
        let mut bounds = [width, height, 0, 0];
        let mut occupancy = vec![0_u32; 64 * 36];
        for (index, pixel) in pixels.chunks_exact(4).enumerate() {
            if pixel == BACKGROUND {
                continue;
            }
            count += 1;
            let x = index as u32 % width;
            let y = index as u32 / width;
            bounds[0] = bounds[0].min(x);
            bounds[1] = bounds[1].min(y);
            bounds[2] = bounds[2].max(x);
            bounds[3] = bounds[3].max(y);
            let cell_x = (x * 64 / width).min(63);
            let cell_y = (y * 36 / height).min(35);
            occupancy[(cell_y * 64 + cell_x) as usize] += 1;
        }
        if count == 0 {
            bounds = [0; 4];
        }
        (count, bounds, occupancy)
    }

    fn primitive_geometry(width: u32, height: u32) -> Result<Geometry, JsValue> {
        let mut buffers: VertexBuffers<Vertex, u32> = VertexBuffers::new();
        let mut tessellator = FillTessellator::new();
        let columns = 48;
        let rows = 26;
        for row in 0..rows {
            for column in 0..columns {
                let index = (row * columns + column) as usize;
                let x = (column as f32 + 0.5) * width as f32 / columns as f32;
                let y = (row as f32 + 0.5) * height as f32 / rows as f32;
                let radius = 5.0 + ((row * 7 + column * 3) % 7) as f32;
                let color = palette(index);
                let path = if (row + column) % 2 == 0 {
                    circle(x, y, radius)
                } else {
                    diamond(x, y, radius)
                };
                tessellator
                    .tessellate_path(
                        &path,
                        &FillOptions::default().with_tolerance(0.05),
                        &mut BuffersBuilder::new(&mut buffers, |vertex: FillVertex<'_>| Vertex {
                            position: [
                                vertex.position().x * 2.0 / width as f32 - 1.0,
                                1.0 - vertex.position().y * 2.0 / height as f32,
                            ],
                            color,
                        }),
                    )
                    .map_err(|error| js_error("lyon browser tessellation failed", error))?;
            }
        }
        Ok(Geometry {
            vertices: buffers.vertices,
            indices: buffers.indices,
        })
    }

    fn circle(x: f32, y: f32, radius: f32) -> Path {
        let control = radius * 0.552_284_8;
        let mut builder = Path::builder().with_svg();
        builder.move_to(point(x + radius, y));
        builder.cubic_bezier_to(
            point(x + radius, y + control),
            point(x + control, y + radius),
            point(x, y + radius),
        );
        builder.cubic_bezier_to(
            point(x - control, y + radius),
            point(x - radius, y + control),
            point(x - radius, y),
        );
        builder.cubic_bezier_to(
            point(x - radius, y - control),
            point(x - control, y - radius),
            point(x, y - radius),
        );
        builder.cubic_bezier_to(
            point(x + control, y - radius),
            point(x + radius, y - control),
            point(x + radius, y),
        );
        builder.close();
        builder.build()
    }

    fn diamond(x: f32, y: f32, radius: f32) -> Path {
        let inset = radius * 0.28;
        let mut builder = Path::builder().with_svg();
        builder.move_to(point(x, y - radius));
        builder.quadratic_bezier_to(point(x + inset, y - inset), point(x + radius, y));
        builder.quadratic_bezier_to(point(x + inset, y + inset), point(x, y + radius));
        builder.quadratic_bezier_to(point(x - inset, y + inset), point(x - radius, y));
        builder.quadratic_bezier_to(point(x - inset, y - inset), point(x, y - radius));
        builder.close();
        builder.build()
    }

    fn palette(index: usize) -> [f32; 4] {
        const COLORS: [[f32; 3]; 6] = [
            [0.31, 0.78, 0.98],
            [0.98, 0.42, 0.55],
            [0.58, 0.91, 0.55],
            [0.96, 0.76, 0.31],
            [0.69, 0.49, 0.96],
            [0.30, 0.91, 0.81],
        ];
        let color = COLORS[index % COLORS.len()];
        [color[0], color[1], color[2], 1.0]
    }

    async fn wait_for_queue(queue: &wgpu::Queue) -> Result<(), JsValue> {
        let (sender, receiver) = oneshot_channel();
        queue.on_submitted_work_done(move || {
            sender.send(()).ok();
        });
        receiver
            .receive()
            .await
            .ok_or_else(|| JsValue::from_str("WebGPU completion callback was dropped"))
    }

    async fn read_texture(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture: &wgpu::Texture,
        width: u32,
        height: u32,
    ) -> Result<Vec<u8>, JsValue> {
        let padded = (width * 4).div_ceil(256) * 256;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("realtime-manim lyon browser readback"),
            size: u64::from(padded) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("realtime-manim lyon browser readback encoder"),
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        queue.submit([encoder.finish()]);
        let (sender, receiver) = oneshot_channel();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                sender.send(result).ok();
            });
        receiver
            .receive()
            .await
            .ok_or_else(|| JsValue::from_str("WebGPU map callback was dropped"))?
            .map_err(|error| js_error("WebGPU readback mapping failed", error))?;
        let mapped = buffer
            .slice(..)
            .get_mapped_range()
            .map_err(|error| js_error("Could not read mapped WebGPU bytes", error))?;
        let unpadded = width as usize * 4;
        let mut pixels = Vec::with_capacity(unpadded * height as usize);
        for row in mapped.chunks_exact(padded as usize) {
            pixels.extend_from_slice(&row[..unpadded]);
        }
        drop(mapped);
        buffer.unmap();
        Ok(pixels)
    }

    fn percentile(values: &[f64], quantile: f64) -> f64 {
        let index = ((values.len() - 1) as f64 * quantile).ceil() as usize;
        values[index]
    }

    fn fnv1a64(bytes: &[u8]) -> u64 {
        bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100_0000_01b3)
        })
    }

    fn js_error(context: &str, error: impl std::fmt::Display) -> JsValue {
        JsValue::from_str(&format!("{context}: {error}"))
    }
}

#[cfg(all(target_arch = "wasm32", feature = "browser-lyon"))]
pub use browser_lyon::run_lyon_browser_probe;
