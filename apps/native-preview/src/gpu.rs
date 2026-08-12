use std::mem;
use std::path::Path;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use image::{ColorType, ImageFormat};
use realtime_manim_scene_core::Scene;
use realtime_manim_text_engine::TextEngine;
use wgpu::util::DeviceExt;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowAttributes, WindowId};

use crate::cli::Args;
use crate::geometry::{Geometry, VERTEX_ATTRIBUTES, Vertex, build_geometry};

const TARGET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
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
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("realtime-manim native vector pipeline layout"),
            bind_group_layouts: &[],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("realtime-manim native vector pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &VERTEX_ATTRIBUTES,
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
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
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        Ok(Self {
            device,
            queue,
            pipeline,
            adapter_name: info.name,
            backend: info.backend,
        })
    }

    fn encode_frame(
        &self,
        view: &wgpu::TextureView,
        background: [f32; 4],
        geometry: &Geometry,
    ) -> wgpu::CommandBuffer {
        let (vertex_buffer, index_buffer) = self.create_frame_buffers(geometry);
        self.encode_uploaded_frame(
            view,
            background,
            &vertex_buffer,
            &index_buffer,
            geometry.indices.len() as u32,
        )
    }

    fn create_frame_buffers(&self, geometry: &Geometry) -> (wgpu::Buffer, wgpu::Buffer) {
        let dummy_vertex = Vertex {
            position: [0.0; 2],
            color: [0.0; 4],
        };
        let vertices = if geometry.vertices.is_empty() {
            std::slice::from_ref(&dummy_vertex)
        } else {
            &geometry.vertices
        };
        let dummy_index = 0_u32;
        let indices = if geometry.indices.is_empty() {
            std::slice::from_ref(&dummy_index)
        } else {
            &geometry.indices
        };
        let vertex_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("realtime-manim native frame vertices"),
                contents: bytemuck::cast_slice(vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let index_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("realtime-manim native frame indices"),
                contents: bytemuck::cast_slice(indices),
                usage: wgpu::BufferUsages::INDEX,
            });
        (vertex_buffer, index_buffer)
    }

    fn encode_uploaded_frame(
        &self,
        view: &wgpu::TextureView,
        background: [f32; 4],
        vertex_buffer: &wgpu::Buffer,
        index_buffer: &wgpu::Buffer,
        index_count: u32,
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
                ..Default::default()
            });
            if index_count > 0 {
                pass.set_pipeline(&self.pipeline);
                pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..index_count, 0, 0..1);
            }
        }
        encoder.finish()
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
    drop(frame);

    let instance = native_instance();
    let gpu = Gpu::new(&instance, None, TARGET_FORMAT).await?;
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
    let command = gpu.encode_frame(&view, background, &geometry);

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
        "native {} ok backend={:?} adapter={:?} scene={:?} time={:.3}s size={}x{} rendered_nodes={}/{} triangles={} content_pixels={} checksum={checksum:016x}{}",
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
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    vertex_capacity: usize,
    index_capacity: usize,
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
        let vertex_capacity = mem::size_of::<Vertex>();
        let index_capacity = mem::size_of::<u32>();
        let vertex_buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("realtime-manim native retained vertex buffer"),
            size: vertex_capacity as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let index_buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("realtime-manim native retained index buffer"),
            size: index_capacity as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Ok(Self {
            _window: window,
            surface,
            gpu,
            config,
            scene,
            text_engine: TextEngine::new().map_err(|error| error.to_string())?,
            vertex_buffer,
            index_buffer,
            vertex_capacity,
            index_capacity,
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
        drop(frame);
        self.upload_geometry(&geometry);
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
            background,
            &self.vertex_buffer,
            &self.index_buffer,
            geometry.indices.len() as u32,
        )]);
        self.gpu.queue.present(texture);
        Ok(())
    }

    fn upload_geometry(&mut self, geometry: &Geometry) {
        let vertex_bytes = bytemuck::cast_slice(&geometry.vertices);
        if vertex_bytes.len() > self.vertex_capacity {
            self.vertex_capacity = vertex_bytes.len().next_power_of_two();
            self.vertex_buffer = self.gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("realtime-manim native retained vertex buffer"),
                size: self.vertex_capacity as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        if !vertex_bytes.is_empty() {
            self.gpu
                .queue
                .write_buffer(&self.vertex_buffer, 0, vertex_bytes);
        }

        let index_bytes = bytemuck::cast_slice(&geometry.indices);
        if index_bytes.len() > self.index_capacity {
            self.index_capacity = index_bytes.len().next_power_of_two();
            self.index_buffer = self.gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("realtime-manim native retained index buffer"),
                size: self.index_capacity as u64,
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        if !index_bytes.is_empty() {
            self.gpu
                .queue
                .write_buffer(&self.index_buffer, 0, index_bytes);
        }
    }
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

    use super::{Playback, changed_pixel_count, fnv1a64};

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
