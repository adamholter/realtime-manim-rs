//! Experimental, reproducible renderer architecture shootout.
//!
//! This tool intentionally does not select the production renderer. It renders
//! identical retained vector workloads with Vello and the project's current
//! lyon + wgpu architecture, measures warmed frame latency, and writes raw
//! images plus schema-valid benchmark receipts for ticket P-10 / decision D-20.

use std::{
    env, fs,
    num::NonZeroUsize,
    path::{Path, PathBuf},
    sync::mpsc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use bytemuck::{Pod, Zeroable};
use image::{ColorType, ImageFormat};
use lyon::{
    math::point,
    path::Path as LyonPath,
    tessellation::{
        BuffersBuilder, FillOptions, FillRule, FillTessellator, FillVertex, StrokeOptions,
        StrokeTessellator, StrokeVertex, VertexBuffers,
    },
};
use realtime_manim_benchmark_schema::{
    BenchmarkResult, EnvironmentSummary, MetricSample, Provenance, SCHEMA_VERSION,
    Workload as ReceiptWorkload,
};
use serde::Serialize;
use vello::{
    AaConfig, AaSupport, RenderParams, Renderer, RendererOptions, Scene,
    kurbo::{Affine, BezPath},
    peniko::{Color, Fill},
};
use wgpu::util::DeviceExt;

const DEFAULT_WIDTH: u32 = 1280;
const DEFAULT_HEIGHT: u32 = 720;
const DEFAULT_WARMUP: usize = 12;
const DEFAULT_ITERATIONS: usize = 60;
const READBACK_TIMEOUT: Duration = Duration::from_secs(30);
const BACKGROUND: [u8; 4] = [18, 20, 28, 255];

fn main() {
    if let Err(error) = run() {
        eprintln!("renderer shootout error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = Args::parse(env::args().skip(1))?;
    fs::create_dir_all(&args.output)
        .map_err(|error| format!("Could not create {}: {error}", args.output.display()))?;

    let workloads = workload_suite(args.width, args.height);
    let mut report = ShootoutReport {
        schema_version: 1,
        experimental: true,
        width: args.width,
        height: args.height,
        warmup_iterations: args.warmup,
        measured_iterations: args.iterations,
        workloads: Vec::new(),
        recommendation: String::new(),
    };

    for workload in workloads {
        println!(
            "shootout workload={} shapes={}",
            workload.name,
            workload.shapes.len()
        );
        let vello = pollster::block_on(run_vello(&workload, &args))?;
        let lyon = pollster::block_on(run_lyon(&workload, &args))?;
        let comparison = compare_images(&vello.pixels, &lyon.pixels)?;
        save_png(
            &args.output.join(format!("{}-vello.png", workload.name)),
            &vello.pixels,
            args.width,
            args.height,
        )?;
        save_png(
            &args.output.join(format!("{}-lyon-wgpu.png", workload.name)),
            &lyon.pixels,
            args.width,
            args.height,
        )?;

        write_receipt(&args, &workload, &vello, "vello-0.9")?;
        write_receipt(&args, &workload, &lyon, "lyon-wgpu")?;

        report.workloads.push(WorkloadReport {
            name: workload.name.to_owned(),
            shape_count: workload.shapes.len(),
            vello: vello.summary(),
            lyon_wgpu: lyon.summary(),
            comparison,
        });
    }

    report.recommendation = recommendation(&report.workloads);
    let report_path = args.output.join("report.json");
    fs::write(
        &report_path,
        serde_json::to_vec_pretty(&report)
            .map_err(|error| format!("Could not encode report: {error}"))?,
    )
    .map_err(|error| format!("Could not write {}: {error}", report_path.display()))?;

    println!("recommendation={}", report.recommendation);
    println!("report={}", report_path.display());
    Ok(())
}

#[derive(Debug)]
struct Args {
    output: PathBuf,
    width: u32,
    height: u32,
    warmup: usize,
    iterations: usize,
    background_load_profile: String,
}

impl Args {
    fn parse(arguments: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut args = Self {
            output: PathBuf::from("benchmarks/renderer-shootout/latest"),
            width: DEFAULT_WIDTH,
            height: DEFAULT_HEIGHT,
            warmup: DEFAULT_WARMUP,
            iterations: DEFAULT_ITERATIONS,
            background_load_profile: "interactive-apps-open".to_owned(),
        };
        let values: Vec<String> = arguments.collect();
        let mut index = 0;
        while index < values.len() {
            let flag = values[index].as_str();
            let value = values.get(index + 1);
            match flag {
                "--output" => args.output = PathBuf::from(require_value(flag, value)?),
                "--width" => args.width = parse_positive(flag, value)?,
                "--height" => args.height = parse_positive(flag, value)?,
                "--warmup" => args.warmup = parse_positive(flag, value)?,
                "--iterations" => args.iterations = parse_positive(flag, value)?,
                "--background-load" => {
                    args.background_load_profile = require_value(flag, value)?.to_owned();
                    if args.background_load_profile.trim().is_empty() {
                        return Err("--background-load must not be empty".to_owned());
                    }
                }
                "--help" | "-h" => {
                    println!(
                        "Usage: realtime-manim-renderer-shootout [--output DIR] [--width PX] [--height PX] [--warmup N] [--iterations N] [--background-load LABEL]"
                    );
                    std::process::exit(0);
                }
                _ => return Err(format!("Unknown argument `{flag}`")),
            }
            index += 2;
        }
        if args.width > 8192 || args.height > 8192 {
            return Err("Render dimensions must not exceed 8192 pixels".to_owned());
        }
        if args.iterations > 10_000 || args.warmup > 10_000 {
            return Err("Iteration counts must not exceed 10000".to_owned());
        }
        Ok(args)
    }
}

fn require_value<'a>(flag: &str, value: Option<&'a String>) -> Result<&'a str, String> {
    value
        .map(String::as_str)
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn parse_positive<T>(flag: &str, value: Option<&String>) -> Result<T, String>
where
    T: std::str::FromStr + PartialOrd + From<u8>,
{
    let value = require_value(flag, value)?;
    let parsed = value
        .parse::<T>()
        .map_err(|_| format!("{flag} requires a positive integer"))?;
    if parsed <= T::from(0) {
        return Err(format!("{flag} requires a positive integer"));
    }
    Ok(parsed)
}

#[derive(Clone, Copy, Debug)]
enum Command {
    Move(f32, f32),
    Line(f32, f32),
    Quad(f32, f32, f32, f32),
    Cubic(f32, f32, f32, f32, f32, f32),
    Close,
}

#[derive(Clone, Debug)]
struct ShapeSpec {
    commands: Vec<Command>,
    fill: Option<[f32; 4]>,
    stroke: Option<([f32; 4], f32)>,
}

#[derive(Debug)]
struct VectorWorkload {
    name: &'static str,
    notes: &'static str,
    shapes: Vec<ShapeSpec>,
}

fn workload_suite(width: u32, height: u32) -> Vec<VectorWorkload> {
    vec![
        primitive_grid(width, height),
        bezier_strokes(width, height),
        dense_field(width, height),
        translucent_layers(width, height),
        outline_glyphs(width, height),
    ]
}

fn primitive_grid(width: u32, height: u32) -> VectorWorkload {
    let mut shapes = Vec::new();
    let columns = 48;
    let rows = 26;
    for row in 0..rows {
        for column in 0..columns {
            let x = (column as f32 + 0.5) * width as f32 / columns as f32;
            let y = (row as f32 + 0.5) * height as f32 / rows as f32;
            let radius = 5.0 + ((row * 7 + column * 3) % 7) as f32;
            let color = palette_color(row * columns + column, 1.0);
            if (row + column) % 2 == 0 {
                shapes.push(circle(x, y, radius, color));
            } else {
                shapes.push(rounded_diamond(x, y, radius, color));
            }
        }
    }
    VectorWorkload {
        name: "primitive-grid",
        notes: "1,248 independent filled primitives; typical dense diagram scene",
        shapes,
    }
}

fn bezier_strokes(width: u32, height: u32) -> VectorWorkload {
    let mut shapes = Vec::new();
    let paths = 420;
    for index in 0..paths {
        let band = index % 42;
        let row = index / 42;
        let x0 = -40.0 + band as f32 * (width as f32 + 80.0) / 41.0;
        let y0 = 30.0 + row as f32 * (height as f32 - 60.0) / 9.0;
        let phase = index as f32 * 0.37;
        let x1 = x0 + 70.0;
        let y1 = y0 + phase.sin() * 55.0;
        shapes.push(ShapeSpec {
            commands: vec![
                Command::Move(x0, y0),
                Command::Cubic(x0 + 18.0, y0 - 48.0, x1 - 24.0, y1 + 52.0, x1, y1),
                Command::Cubic(x1 + 18.0, y1 - 42.0, x1 + 55.0, y0 + 38.0, x1 + 74.0, y0),
            ],
            fill: None,
            stroke: Some((palette_color(index, 0.82), 1.25 + (index % 5) as f32)),
        });
    }
    VectorWorkload {
        name: "bezier-strokes",
        notes: "420 multi-cubic antialiased strokes with varied widths and alpha",
        shapes,
    }
}

fn dense_field(width: u32, height: u32) -> VectorWorkload {
    let mut shapes = Vec::new();
    let columns = 100;
    let rows = 55;
    for row in 0..rows {
        for column in 0..columns {
            let x = (column as f32 + 0.5) * width as f32 / columns as f32;
            let y = (row as f32 + 0.5) * height as f32 / rows as f32;
            let dx = ((y / height as f32) * std::f32::consts::TAU).cos() * 5.0;
            let dy = ((x / width as f32) * std::f32::consts::TAU).sin() * 5.0;
            shapes.push(ShapeSpec {
                commands: vec![Command::Move(x - dx, y - dy), Command::Line(x + dx, y + dy)],
                fill: None,
                stroke: Some((palette_color(row + column, 0.9), 1.1)),
            });
        }
    }
    VectorWorkload {
        name: "dense-vector-field",
        notes: "5,500 independent line strokes; data-visualization command pressure",
        shapes,
    }
}

fn translucent_layers(width: u32, height: u32) -> VectorWorkload {
    let mut shapes = Vec::new();
    for index in 0..900 {
        let angle = index as f32 * 0.127;
        let radius = 10.0 + index as f32 * 0.36;
        let x = width as f32 * 0.5 + angle.cos() * radius;
        let y = height as f32 * 0.5 + angle.sin() * radius * 0.62;
        shapes.push(circle(
            x,
            y,
            8.0 + (index % 23) as f32,
            palette_color(index, 0.075),
        ));
    }
    VectorWorkload {
        name: "translucent-layers",
        notes: "900 ordered translucent paths; blend and overdraw pressure",
        shapes,
    }
}

fn outline_glyphs(width: u32, height: u32) -> VectorWorkload {
    let mut shapes = Vec::new();
    let columns = 38;
    let rows = 12;
    for row in 0..rows {
        for column in 0..columns {
            let x = 12.0 + column as f32 * (width as f32 - 24.0) / columns as f32;
            let y = 18.0 + row as f32 * (height as f32 - 36.0) / rows as f32;
            let scale = 8.0 + ((row + column) % 5) as f32 * 1.5;
            shapes.push(pseudo_glyph(
                x,
                y,
                scale,
                palette_color(row * columns + column, 1.0),
            ));
        }
    }
    VectorWorkload {
        name: "outline-glyphs",
        notes: "456 compound cubic outlines with holes; text/MathTex-like geometry",
        shapes,
    }
}

fn palette_color(index: usize, alpha: f32) -> [f32; 4] {
    const COLORS: [[f32; 3]; 6] = [
        [0.31, 0.78, 0.98],
        [0.98, 0.42, 0.55],
        [0.58, 0.91, 0.55],
        [0.96, 0.76, 0.31],
        [0.69, 0.49, 0.96],
        [0.30, 0.91, 0.81],
    ];
    let color = COLORS[index % COLORS.len()];
    [color[0], color[1], color[2], alpha]
}

fn circle(x: f32, y: f32, radius: f32, color: [f32; 4]) -> ShapeSpec {
    let control = radius * 0.552_284_8;
    ShapeSpec {
        commands: vec![
            Command::Move(x + radius, y),
            Command::Cubic(
                x + radius,
                y + control,
                x + control,
                y + radius,
                x,
                y + radius,
            ),
            Command::Cubic(
                x - control,
                y + radius,
                x - radius,
                y + control,
                x - radius,
                y,
            ),
            Command::Cubic(
                x - radius,
                y - control,
                x - control,
                y - radius,
                x,
                y - radius,
            ),
            Command::Cubic(
                x + control,
                y - radius,
                x + radius,
                y - control,
                x + radius,
                y,
            ),
            Command::Close,
        ],
        fill: Some(color),
        stroke: None,
    }
}

fn rounded_diamond(x: f32, y: f32, radius: f32, color: [f32; 4]) -> ShapeSpec {
    let inset = radius * 0.28;
    ShapeSpec {
        commands: vec![
            Command::Move(x, y - radius),
            Command::Quad(x + inset, y - inset, x + radius, y),
            Command::Quad(x + inset, y + inset, x, y + radius),
            Command::Quad(x - inset, y + inset, x - radius, y),
            Command::Quad(x - inset, y - inset, x, y - radius),
            Command::Close,
        ],
        fill: Some(color),
        stroke: None,
    }
}

fn pseudo_glyph(x: f32, y: f32, scale: f32, color: [f32; 4]) -> ShapeSpec {
    ShapeSpec {
        commands: vec![
            Command::Move(x, y + scale),
            Command::Cubic(
                x,
                y - scale,
                x + scale * 1.45,
                y - scale,
                x + scale * 1.45,
                y,
            ),
            Command::Cubic(x + scale * 1.45, y + scale, x, y + scale, x, y + scale),
            Command::Close,
            Command::Move(x + scale * 0.42, y + scale * 0.45),
            Command::Cubic(
                x + scale * 0.42,
                y - scale * 0.32,
                x + scale,
                y - scale * 0.32,
                x + scale,
                y + scale * 0.1,
            ),
            Command::Cubic(
                x + scale,
                y + scale * 0.5,
                x + scale * 0.42,
                y + scale * 0.5,
                x + scale * 0.42,
                y + scale * 0.45,
            ),
            Command::Close,
        ],
        fill: Some(color),
        stroke: None,
    }
}

#[derive(Debug)]
struct BackendResult {
    adapter: String,
    prepare_ms: f64,
    frame_ms: Vec<f64>,
    pixels: Vec<u8>,
    retained_bytes: u64,
}

impl BackendResult {
    fn summary(&self) -> BackendSummary {
        BackendSummary {
            adapter: self.adapter.clone(),
            prepare_ms: self.prepare_ms,
            frame_p50_ms: percentile(&self.frame_ms, 0.50),
            frame_p95_ms: percentile(&self.frame_ms, 0.95),
            frame_p99_ms: percentile(&self.frame_ms, 0.99),
            frame_mean_ms: mean(&self.frame_ms),
            retained_bytes: self.retained_bytes,
        }
    }
}

#[derive(Debug, Serialize)]
struct ShootoutReport {
    schema_version: u32,
    experimental: bool,
    width: u32,
    height: u32,
    warmup_iterations: usize,
    measured_iterations: usize,
    workloads: Vec<WorkloadReport>,
    recommendation: String,
}

#[derive(Debug, Serialize)]
struct WorkloadReport {
    name: String,
    shape_count: usize,
    vello: BackendSummary,
    lyon_wgpu: BackendSummary,
    comparison: ImageComparison,
}

#[derive(Debug, Serialize)]
struct BackendSummary {
    adapter: String,
    prepare_ms: f64,
    frame_p50_ms: f64,
    frame_p95_ms: f64,
    frame_p99_ms: f64,
    frame_mean_ms: f64,
    retained_bytes: u64,
}

#[derive(Debug, Serialize)]
struct ImageComparison {
    rgb_rmse: f64,
    alpha_rmse: f64,
    changed_pixels: usize,
    vello_content_pixels: usize,
    lyon_content_pixels: usize,
}

async fn run_vello(workload: &VectorWorkload, args: &Args) -> Result<BackendResult, String> {
    let start = Instant::now();
    let scene = build_vello_scene(workload);
    let prepare_ms = start.elapsed().as_secs_f64() * 1000.0;
    let mut instance_descriptor = vello::wgpu::InstanceDescriptor::new_without_display_handle();
    instance_descriptor.backends = vello::wgpu::Backends::METAL;
    let instance = vello::wgpu::Instance::new(instance_descriptor);
    let adapter = instance
        .request_adapter(&vello::wgpu::RequestAdapterOptions {
            power_preference: vello::wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        })
        .await
        .map_err(|error| format!("Vello adapter request failed: {error}"))?;
    let adapter_name = adapter.get_info().name;
    let (device, queue) = adapter
        .request_device(&vello::wgpu::DeviceDescriptor {
            label: Some("realtime-manim Vello shootout device"),
            required_features: vello::wgpu::Features::empty(),
            required_limits: vello::wgpu::Limits::default(),
            memory_hints: vello::wgpu::MemoryHints::Performance,
            trace: vello::wgpu::Trace::Off,
            experimental_features: vello::wgpu::ExperimentalFeatures::disabled(),
        })
        .await
        .map_err(|error| format!("Vello device request failed: {error}"))?;
    let texture = device.create_texture(&vello::wgpu::TextureDescriptor {
        label: Some("realtime-manim Vello shootout target"),
        size: vello::wgpu::Extent3d {
            width: args.width,
            height: args.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: vello::wgpu::TextureDimension::D2,
        format: vello::wgpu::TextureFormat::Rgba8Unorm,
        usage: vello::wgpu::TextureUsages::STORAGE_BINDING | vello::wgpu::TextureUsages::COPY_SRC,
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
    .map_err(|error| format!("Vello renderer initialization failed: {error}"))?;
    let params = RenderParams {
        base_color: Color::from_rgba8(BACKGROUND[0], BACKGROUND[1], BACKGROUND[2], BACKGROUND[3]),
        width: args.width,
        height: args.height,
        antialiasing_method: AaConfig::Area,
    };

    for _ in 0..args.warmup {
        renderer
            .render_to_texture(&device, &queue, &scene, &view, &params)
            .map_err(|error| format!("Vello warmup render failed: {error}"))?;
        device
            .poll(vello::wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(READBACK_TIMEOUT),
            })
            .map_err(|error| format!("Vello warmup poll failed: {error}"))?;
    }

    let mut frame_ms = Vec::with_capacity(args.iterations);
    for _ in 0..args.iterations {
        let frame_start = Instant::now();
        renderer
            .render_to_texture(&device, &queue, &scene, &view, &params)
            .map_err(|error| format!("Vello render failed: {error}"))?;
        device
            .poll(vello::wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(READBACK_TIMEOUT),
            })
            .map_err(|error| format!("Vello frame poll failed: {error}"))?;
        frame_ms.push(frame_start.elapsed().as_secs_f64() * 1000.0);
    }
    let pixels = read_vello_texture(&device, &queue, &texture, args.width, args.height)?;
    Ok(BackendResult {
        adapter: adapter_name,
        prepare_ms,
        frame_ms,
        pixels,
        retained_bytes: vello_retained_bytes(&scene),
    })
}

fn build_vello_scene(workload: &VectorWorkload) -> Scene {
    let mut scene = Scene::new();
    for shape in &workload.shapes {
        let path = to_bez_path(&shape.commands);
        if let Some(color) = shape.fill {
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                vello_color(color),
                None,
                &path,
            );
        }
        if let Some((color, width)) = shape.stroke {
            scene.stroke(
                &vello::kurbo::Stroke::new(width as f64),
                Affine::IDENTITY,
                vello_color(color),
                None,
                &path,
            );
        }
    }
    scene
}

fn to_bez_path(commands: &[Command]) -> BezPath {
    let mut path = BezPath::new();
    for command in commands {
        match *command {
            Command::Move(x, y) => path.move_to((f64::from(x), f64::from(y))),
            Command::Line(x, y) => path.line_to((f64::from(x), f64::from(y))),
            Command::Quad(x1, y1, x, y) => {
                path.quad_to((f64::from(x1), f64::from(y1)), (f64::from(x), f64::from(y)))
            }
            Command::Cubic(x1, y1, x2, y2, x, y) => path.curve_to(
                (f64::from(x1), f64::from(y1)),
                (f64::from(x2), f64::from(y2)),
                (f64::from(x), f64::from(y)),
            ),
            Command::Close => path.close_path(),
        }
    }
    path
}

fn vello_color(color: [f32; 4]) -> Color {
    Color::new([color[0], color[1], color[2], color[3]])
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct LyonVertex {
    position: [f32; 2],
    color: [f32; 4],
}

async fn run_lyon(workload: &VectorWorkload, args: &Args) -> Result<BackendResult, String> {
    let prepare_start = Instant::now();
    let geometry = tessellate_lyon(workload, args.width, args.height)?;
    let prepare_ms = prepare_start.elapsed().as_secs_f64() * 1000.0;
    let retained_bytes = (geometry.vertices.len() * std::mem::size_of::<LyonVertex>()
        + geometry.indices.len() * std::mem::size_of::<u32>()) as u64;

    let mut instance_descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    instance_descriptor.backends = wgpu::Backends::METAL;
    let instance = wgpu::Instance::new(instance_descriptor);
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        })
        .await
        .map_err(|error| format!("lyon/wgpu adapter request failed: {error}"))?;
    let adapter_name = adapter.get_info().name;
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("realtime-manim lyon shootout device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
            trace: wgpu::Trace::Off,
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
        })
        .await
        .map_err(|error| format!("lyon/wgpu device request failed: {error}"))?;
    let shader = device.create_shader_module(wgpu::include_wgsl!("shootout.wgsl"));
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("realtime-manim shootout layout"),
        bind_group_layouts: &[],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("realtime-manim lyon shootout pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<LyonVertex>() as u64,
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
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("realtime-manim lyon shootout target"),
        size: wgpu::Extent3d {
            width: args.width,
            height: args.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let msaa = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("realtime-manim lyon shootout MSAA"),
        size: wgpu::Extent3d {
            width: args.width,
            height: args.height,
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
    let vertex = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("realtime-manim lyon shootout vertices"),
        contents: bytemuck::cast_slice(&geometry.vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let index = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("realtime-manim lyon shootout indices"),
        contents: bytemuck::cast_slice(&geometry.indices),
        usage: wgpu::BufferUsages::INDEX,
    });

    let encode = || {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("realtime-manim lyon shootout frame"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("realtime-manim lyon shootout pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &msaa_view,
                    depth_slice: None,
                    resolve_target: Some(&view),
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
            pass.set_vertex_buffer(0, vertex.slice(..));
            pass.set_index_buffer(index.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..geometry.indices.len() as u32, 0, 0..1);
        }
        encoder.finish()
    };

    for _ in 0..args.warmup {
        let submission = queue.submit([encode()]);
        device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(READBACK_TIMEOUT),
            })
            .map_err(|error| format!("lyon/wgpu warmup poll failed: {error}"))?;
    }
    let mut frame_ms = Vec::with_capacity(args.iterations);
    for _ in 0..args.iterations {
        let frame_start = Instant::now();
        let submission = queue.submit([encode()]);
        device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(READBACK_TIMEOUT),
            })
            .map_err(|error| format!("lyon/wgpu frame poll failed: {error}"))?;
        frame_ms.push(frame_start.elapsed().as_secs_f64() * 1000.0);
    }
    let pixels = read_lyon_texture(&device, &queue, &texture, args.width, args.height)?;
    Ok(BackendResult {
        adapter: adapter_name,
        prepare_ms,
        frame_ms,
        pixels,
        retained_bytes,
    })
}

struct LyonGeometry {
    vertices: Vec<LyonVertex>,
    indices: Vec<u32>,
}

fn tessellate_lyon(
    workload: &VectorWorkload,
    width: u32,
    height: u32,
) -> Result<LyonGeometry, String> {
    let mut buffers: VertexBuffers<LyonVertex, u32> = VertexBuffers::new();
    let mut fill_tessellator = FillTessellator::new();
    let mut stroke_tessellator = StrokeTessellator::new();
    for shape in &workload.shapes {
        let path = to_lyon_path(&shape.commands);
        if let Some(color) = shape.fill {
            fill_tessellator
                .tessellate_path(
                    &path,
                    &FillOptions::default()
                        .with_fill_rule(FillRule::NonZero)
                        .with_tolerance(0.05),
                    &mut BuffersBuilder::new(&mut buffers, |vertex: FillVertex<'_>| LyonVertex {
                        position: to_ndc(vertex.position(), width, height),
                        color,
                    }),
                )
                .map_err(|error| format!("lyon fill tessellation failed: {error}"))?;
        }
        if let Some((color, line_width)) = shape.stroke {
            stroke_tessellator
                .tessellate_path(
                    &path,
                    &StrokeOptions::default()
                        .with_line_width(line_width)
                        .with_tolerance(0.05),
                    &mut BuffersBuilder::new(&mut buffers, |vertex: StrokeVertex<'_, '_>| {
                        LyonVertex {
                            position: to_ndc(vertex.position(), width, height),
                            color,
                        }
                    }),
                )
                .map_err(|error| format!("lyon stroke tessellation failed: {error}"))?;
        }
    }
    Ok(LyonGeometry {
        vertices: buffers.vertices,
        indices: buffers.indices,
    })
}

fn to_lyon_path(commands: &[Command]) -> LyonPath {
    let mut builder = LyonPath::builder().with_svg();
    for command in commands {
        match *command {
            Command::Move(x, y) => {
                builder.move_to(point(x, y));
            }
            Command::Line(x, y) => {
                builder.line_to(point(x, y));
            }
            Command::Quad(x1, y1, x, y) => {
                builder.quadratic_bezier_to(point(x1, y1), point(x, y));
            }
            Command::Cubic(x1, y1, x2, y2, x, y) => {
                builder.cubic_bezier_to(point(x1, y1), point(x2, y2), point(x, y));
            }
            Command::Close => {
                builder.close();
            }
        }
    }
    builder.build()
}

fn to_ndc(position: lyon::math::Point, width: u32, height: u32) -> [f32; 2] {
    [
        position.x * 2.0 / width as f32 - 1.0,
        1.0 - position.y * 2.0 / height as f32,
    ]
}

fn read_vello_texture(
    device: &vello::wgpu::Device,
    queue: &vello::wgpu::Queue,
    texture: &vello::wgpu::Texture,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, String> {
    let padded = (width * 4).div_ceil(256) * 256;
    let buffer = device.create_buffer(&vello::wgpu::BufferDescriptor {
        label: Some("realtime-manim Vello readback"),
        size: u64::from(padded) * u64::from(height),
        usage: vello::wgpu::BufferUsages::COPY_DST | vello::wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&vello::wgpu::CommandEncoderDescriptor {
        label: Some("realtime-manim Vello readback encoder"),
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
    let submission = queue.submit([encoder.finish()]);
    let (sender, receiver) = mpsc::channel();
    buffer
        .slice(..)
        .map_async(vello::wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
    device
        .poll(vello::wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(READBACK_TIMEOUT),
        })
        .map_err(|error| format!("Vello readback poll failed: {error}"))?;
    receiver
        .recv_timeout(READBACK_TIMEOUT)
        .map_err(|error| format!("Vello readback callback timed out: {error}"))?
        .map_err(|error| format!("Vello readback mapping failed: {error}"))?;
    let mapped = buffer.slice(..).get_mapped_range();
    let pixels = copy_rows(&mapped, width, height, padded);
    drop(mapped);
    buffer.unmap();
    Ok(pixels)
}

fn read_lyon_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, String> {
    let padded = (width * 4).div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("realtime-manim lyon readback"),
        size: u64::from(padded) * u64::from(height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("realtime-manim lyon readback encoder"),
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
    let submission = queue.submit([encoder.finish()]);
    let (sender, receiver) = mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(READBACK_TIMEOUT),
        })
        .map_err(|error| format!("lyon readback poll failed: {error}"))?;
    receiver
        .recv_timeout(READBACK_TIMEOUT)
        .map_err(|error| format!("lyon readback callback timed out: {error}"))?
        .map_err(|error| format!("lyon readback mapping failed: {error}"))?;
    let mapped = buffer
        .slice(..)
        .get_mapped_range()
        .map_err(|error| format!("Could not read lyon mapped bytes: {error}"))?;
    let pixels = copy_rows(&mapped, width, height, padded);
    drop(mapped);
    buffer.unmap();
    Ok(pixels)
}

fn copy_rows(mapped: &[u8], width: u32, height: u32, padded: u32) -> Vec<u8> {
    let unpadded = width as usize * 4;
    let mut pixels = Vec::with_capacity(unpadded * height as usize);
    for row in mapped.chunks_exact(padded as usize) {
        pixels.extend_from_slice(&row[..unpadded]);
    }
    pixels
}

fn vello_retained_bytes(scene: &Scene) -> u64 {
    let encoding = scene.encoding();
    [
        std::mem::size_of_val(encoding.path_tags.as_slice()),
        std::mem::size_of_val(encoding.path_data.as_slice()),
        std::mem::size_of_val(encoding.draw_tags.as_slice()),
        std::mem::size_of_val(encoding.draw_data.as_slice()),
        std::mem::size_of_val(encoding.transforms.as_slice()),
        std::mem::size_of_val(encoding.styles.as_slice()),
        std::mem::size_of_val(encoding.resources.patches.as_slice()),
        std::mem::size_of_val(encoding.resources.color_stops.as_slice()),
        std::mem::size_of_val(encoding.resources.glyphs.as_slice()),
        std::mem::size_of_val(encoding.resources.glyph_runs.as_slice()),
        std::mem::size_of_val(encoding.resources.normalized_coords.as_slice()),
    ]
    .into_iter()
    .sum::<usize>() as u64
}

fn compare_images(vello: &[u8], lyon: &[u8]) -> Result<ImageComparison, String> {
    if vello.len() != lyon.len() || vello.len() % 4 != 0 {
        return Err("Rendered image lengths differ".to_owned());
    }
    let mut rgb_error = 0.0;
    let mut alpha_error = 0.0;
    let mut changed_pixels = 0;
    let mut vello_content_pixels = 0;
    let mut lyon_content_pixels = 0;
    for (left, right) in vello.chunks_exact(4).zip(lyon.chunks_exact(4)) {
        let mut changed = false;
        for channel in 0..3 {
            let delta = f64::from(left[channel]) - f64::from(right[channel]);
            rgb_error += delta * delta;
            changed |= left[channel].abs_diff(right[channel]) > 4;
        }
        let alpha_delta = f64::from(left[3]) - f64::from(right[3]);
        alpha_error += alpha_delta * alpha_delta;
        changed |= left[3].abs_diff(right[3]) > 4;
        changed_pixels += usize::from(changed);
        vello_content_pixels += usize::from(left != BACKGROUND);
        lyon_content_pixels += usize::from(right != BACKGROUND);
    }
    let pixels = (vello.len() / 4) as f64;
    Ok(ImageComparison {
        rgb_rmse: (rgb_error / (pixels * 3.0)).sqrt() / 255.0,
        alpha_rmse: (alpha_error / pixels).sqrt() / 255.0,
        changed_pixels,
        vello_content_pixels,
        lyon_content_pixels,
    })
}

fn save_png(path: &Path, pixels: &[u8], width: u32, height: u32) -> Result<(), String> {
    image::save_buffer_with_format(
        path,
        pixels,
        width,
        height,
        ColorType::Rgba8,
        ImageFormat::Png,
    )
    .map_err(|error| format!("Could not save {}: {error}", path.display()))
}

fn write_receipt(
    args: &Args,
    workload: &VectorWorkload,
    result: &BackendResult,
    backend: &str,
) -> Result<(), String> {
    let summary = result.summary();
    let artifact_backend = if backend.starts_with("vello") {
        "vello"
    } else {
        backend
    };
    let receipt = BenchmarkResult {
        schema_version: SCHEMA_VERSION,
        benchmark_id: format!("P-10-{backend}-{}", workload.name),
        recorded_at_utc: rfc3339_now(),
        environment: EnvironmentSummary {
            os: "macOS".to_owned(),
            os_version: macos_version(),
            architecture: env::consts::ARCH.to_owned(),
            processor: processor_class(),
            memory_bytes: installed_memory(),
            display: None,
            background_load_profile: args.background_load_profile.clone(),
        },
        workload: ReceiptWorkload {
            name: format!("{backend} / {}", workload.name),
            scene: format!("tools/renderer-shootout/generated/{}", workload.name),
            resolution: [args.width, args.height],
            duration_seconds: result.frame_ms.iter().sum::<f64>() / 1000.0,
            notes: vec![
                workload.notes.to_owned(),
                "Experimental architecture evidence; not a renderer selection.".to_owned(),
                "Frame samples include CPU encoding, GPU submission, and completion wait."
                    .to_owned(),
            ],
        },
        samples: vec![
            MetricSample {
                name: "retained_prepare_time".to_owned(),
                unit: "ms".to_owned(),
                statistic: "single".to_owned(),
                value: summary.prepare_ms,
            },
            MetricSample {
                name: "frame_time".to_owned(),
                unit: "ms".to_owned(),
                statistic: "p50".to_owned(),
                value: summary.frame_p50_ms,
            },
            MetricSample {
                name: "frame_time".to_owned(),
                unit: "ms".to_owned(),
                statistic: "p95".to_owned(),
                value: summary.frame_p95_ms,
            },
            MetricSample {
                name: "frame_time".to_owned(),
                unit: "ms".to_owned(),
                statistic: "p99".to_owned(),
                value: summary.frame_p99_ms,
            },
            MetricSample {
                name: "retained_geometry".to_owned(),
                unit: "bytes".to_owned(),
                statistic: "single".to_owned(),
                value: summary.retained_bytes as f64,
            },
        ],
        provenance: Provenance {
            revision: source_revision(),
            dirty: git_dirty(),
            commands: vec![format!(
                "cargo run --release -p realtime-manim-renderer-shootout -- --output {} --width {} --height {} --warmup {} --iterations {} --background-load {}",
                args.output.display(),
                args.width,
                args.height,
                args.warmup,
                args.iterations,
                args.background_load_profile
            )],
            artifacts: vec![
                args.output
                    .join(format!("{}-{artifact_backend}.png", workload.name))
                    .display()
                    .to_string(),
                args.output.join("report.json").display().to_string(),
            ],
        },
    };
    receipt
        .validate()
        .map_err(|error| format!("Generated receipt is invalid: {error}"))?;
    let path = args
        .output
        .join(format!("{}-{backend}.receipt.json", workload.name));
    fs::write(
        &path,
        serde_json::to_vec_pretty(&receipt)
            .map_err(|error| format!("Could not encode receipt: {error}"))?,
    )
    .map_err(|error| format!("Could not write {}: {error}", path.display()))
}

fn percentile(values: &[f64], quantile: f64) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let index = ((sorted.len() - 1) as f64 * quantile).ceil() as usize;
    sorted[index]
}

fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}

fn recommendation(workloads: &[WorkloadReport]) -> String {
    let vello_wins = workloads
        .iter()
        .filter(|workload| workload.vello.frame_p95_ms < workload.lyon_wgpu.frame_p95_ms)
        .count();
    let lyon_wins = workloads.len() - vello_wins;
    if vello_wins == workloads.len() {
        "Vello wins every native p95 workload. Keep the current renderer until browser integration, feature-parity, and binary-size evidence complete D-20; prototype a Vello backend behind the renderer trait.".to_owned()
    } else if lyon_wins == workloads.len() {
        "lyon + wgpu wins every native p95 workload. Retain it as the current experimental baseline, but do not freeze D-20 until browser and full-feature correctness evidence is complete.".to_owned()
    } else {
        format!(
            "Mixed native result: Vello wins {vello_wins}/{} p95 workloads and lyon + wgpu wins {lyon_wins}/{}. Preserve a backend boundary and defer D-20 until browser, feature-parity, and binary-size evidence is complete.",
            workloads.len(),
            workloads.len()
        )
    }
}

fn rfc3339_now() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let output = std::process::Command::new("date")
        .args(["-u", "-r", &seconds.to_string(), "+%Y-%m-%dT%H:%M:%SZ"])
        .output();
    output
        .ok()
        .filter(|result| result.status.success())
        .and_then(|result| String::from_utf8(result.stdout).ok())
        .map(|value| value.trim().to_owned())
        .unwrap_or_else(|| format!("unix-{seconds}"))
}

fn command_output(program: &str, arguments: &[&str], fallback: &str) -> String {
    std::process::Command::new(program)
        .args(arguments)
        .output()
        .ok()
        .filter(|result| result.status.success())
        .and_then(|result| String::from_utf8(result.stdout).ok())
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| fallback.to_owned())
}

fn macos_version() -> String {
    command_output("sw_vers", &["-productVersion"], "unknown")
}

fn processor_class() -> String {
    command_output(
        "sysctl",
        &["-n", "machdep.cpu.brand_string"],
        "Apple Silicon",
    )
}

fn installed_memory() -> u64 {
    command_output("sysctl", &["-n", "hw.memsize"], "1")
        .parse()
        .unwrap_or(1)
}

fn git_revision() -> String {
    command_output("git", &["rev-parse", "--short=12", "HEAD"], "uncommitted")
}

fn source_revision() -> String {
    format!("{}+tree-{:016x}", git_revision(), source_tree_hash())
}

fn source_tree_hash() -> u64 {
    // Stable FNV-1a: unlike `DefaultHasher`, this remains reproducible across
    // Rust/toolchain versions while the benchmark source is still uncommitted.
    let mut hash = 0xcbf29ce484222325_u64;
    let mut update = |bytes: &[u8]| {
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    };
    for relative in [
        "Cargo.toml",
        "Cargo.lock",
        "crates/benchmark-schema/Cargo.toml",
        "crates/benchmark-schema/src/lib.rs",
        "tools/renderer-shootout/Cargo.toml",
        "tools/renderer-shootout/src/lib.rs",
        "tools/renderer-shootout/src/main.rs",
        "tools/renderer-shootout/src/shootout.wgsl",
    ] {
        update(relative.as_bytes());
        update(&[0]);
        match fs::read(relative) {
            Ok(bytes) => update(&bytes),
            Err(error) => update(error.to_string().as_bytes()),
        }
        update(&[0xff]);
    }
    hash
}

fn git_dirty() -> bool {
    std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .map_or(true, |output| !output.stdout.is_empty())
}

#[cfg(test)]
mod tests {
    use super::{
        Args, BACKGROUND, DEFAULT_HEIGHT, DEFAULT_ITERATIONS, DEFAULT_WARMUP, DEFAULT_WIDTH,
        compare_images, percentile, workload_suite,
    };

    #[test]
    fn suite_covers_five_distinct_vector_pressures() {
        let workloads = workload_suite(DEFAULT_WIDTH, DEFAULT_HEIGHT);
        assert_eq!(workloads.len(), 5);
        assert!(workloads.iter().all(|workload| !workload.shapes.is_empty()));
        assert!(workloads.iter().map(|workload| workload.name).all(|name| {
            workloads
                .iter()
                .filter(|workload| workload.name == name)
                .count()
                == 1
        }));
    }

    #[test]
    fn percentile_uses_nearest_rank_ceil() {
        let values = [1.0, 5.0, 2.0, 4.0, 3.0];
        assert_eq!(percentile(&values, 0.50), 3.0);
        assert_eq!(percentile(&values, 0.95), 5.0);
    }

    #[test]
    fn identical_images_have_zero_error() {
        let image = [BACKGROUND, BACKGROUND].concat();
        let comparison = compare_images(&image, &image).expect("comparison should work");
        assert_eq!(comparison.rgb_rmse, 0.0);
        assert_eq!(comparison.alpha_rmse, 0.0);
        assert_eq!(comparison.changed_pixels, 0);
    }

    #[test]
    fn defaults_are_bounded_and_nonzero() {
        let args = Args::parse(std::iter::empty()).expect("defaults should parse");
        assert_eq!(args.width, DEFAULT_WIDTH);
        assert_eq!(args.height, DEFAULT_HEIGHT);
        assert_eq!(args.warmup, DEFAULT_WARMUP);
        assert_eq!(args.iterations, DEFAULT_ITERATIONS);
    }

    #[test]
    fn source_revision_captures_benchmark_tree() {
        let revision = super::source_revision();
        assert!(revision.contains("+tree-"));
        assert!(revision.len() > 24);
    }
}
