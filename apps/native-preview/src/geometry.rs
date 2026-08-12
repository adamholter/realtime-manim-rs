use std::f32::consts::TAU;

use bytemuck::{Pod, Zeroable};
use lyon::{
    algorithms::measure::{PathMeasurements, SampleType},
    math::{Point, point},
    path::{Event as PathEvent, Path, iterator::PathIterator},
    tessellation::{
        BuffersBuilder, FillOptions, FillTessellator, FillVertex, FillVertexConstructor,
        StrokeOptions, StrokeTessellator, StrokeVertex, StrokeVertexConstructor, VertexBuffers,
    },
};
use realtime_manim_scene_core::{
    Camera, EvaluatedFrameView, EvaluatedLinearGradient, EvaluatedNodeView, EvaluatedStyle,
    FontSlant, FontWeight, GradientSpace, GradientSpread, NodeKind, PathCommand, StrokeCap,
    StrokeJoin, TextAlign, parse_color,
};
use realtime_manim_text_engine::{
    FontSelection, FontVariant, TextAlign as ShapedTextAlign, TextEngine,
};

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 2],
    pub color: [f32; 4],
}

pub const VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 2] =
    wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4];

#[derive(Debug)]
pub struct Geometry {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub visible_nodes: usize,
    pub rendered_nodes: usize,
    pub unsupported: Vec<String>,
}

impl Geometry {
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }
}

pub fn build_geometry(
    frame: &EvaluatedFrameView<'_>,
    text_engine: &mut TextEngine,
) -> Result<Geometry, String> {
    let mut buffers = VertexBuffers::<Vertex, u32>::new();
    let mut unsupported = Vec::new();
    let mut rendered_nodes = 0;
    for node in &frame.nodes {
        if append_node(&mut buffers, frame, node, text_engine)? {
            rendered_nodes += 1;
        } else {
            unsupported.push(format!(
                "{}:{}",
                node.id,
                node_kind_name(node.kind.as_ref())
            ));
        }
    }
    Ok(Geometry {
        vertices: buffers.vertices,
        indices: buffers.indices,
        visible_nodes: frame.nodes.len(),
        rendered_nodes,
        unsupported,
    })
}

fn append_node(
    buffers: &mut VertexBuffers<Vertex, u32>,
    frame: &EvaluatedFrameView<'_>,
    node: &EvaluatedNodeView<'_>,
    text_engine: &mut TextEngine,
) -> Result<bool, String> {
    match node.kind.as_ref() {
        NodeKind::Circle { radius } => {
            append_path(buffers, frame, node, &circle_path(*radius), &node.style)?;
        }
        NodeKind::Rect {
            width,
            height,
            corner_radius,
        } => append_path(
            buffers,
            frame,
            node,
            &rect_path(*width, *height, *corner_radius),
            &node.style,
        )?,
        NodeKind::Line { from, to } => {
            append_path(
                buffers,
                frame,
                node,
                &polyline_path(&[*from, *to], false),
                &node.style,
            )?;
        }
        NodeKind::Arrow { from, to, tip_size } => {
            append_path(
                buffers,
                frame,
                node,
                &polyline_path(&[*from, *to], false),
                &node.style,
            )?;
            let dx = to[0] - from[0];
            let dy = to[1] - from[1];
            let length = dx.hypot(dy).max(f32::EPSILON);
            let direction = [dx / length, dy / length];
            let perpendicular = [-direction[1], direction[0]];
            let base = [
                to[0] - direction[0] * tip_size,
                to[1] - direction[1] * tip_size,
            ];
            let tip = [
                *to,
                [
                    base[0] + perpendicular[0] * tip_size * 0.55,
                    base[1] + perpendicular[1] * tip_size * 0.55,
                ],
                [
                    base[0] - perpendicular[0] * tip_size * 0.55,
                    base[1] - perpendicular[1] * tip_size * 0.55,
                ],
            ];
            let mut style = node.style.clone();
            style.fill = style.stroke.or(style.fill);
            style.fill_gradient = style.stroke_gradient.clone().or(style.fill_gradient);
            style.stroke = None;
            style.stroke_gradient = None;
            append_path(buffers, frame, node, &polyline_path(&tip, true), &style)?;
        }
        NodeKind::Polyline { points, closed } => {
            append_path(
                buffers,
                frame,
                node,
                &polyline_path(points, *closed),
                &node.style,
            )?;
        }
        NodeKind::Path { commands } => {
            append_path(buffers, frame, node, &command_path(commands)?, &node.style)?
        }
        NodeKind::PointCloud {
            points,
            radius,
            screen_space_radius,
        } => {
            let radius_scale = if *screen_space_radius {
                1.0 / frame.camera.zoom.max(0.001)
            } else {
                1.0
            };
            for mark in points {
                let mut point_node = node.clone();
                point_node.transform = translate_matrix(node.transform, mark.x, mark.y);
                let mut style = point_node.style.clone();
                if let Some(color) = mark.color.as_deref() {
                    let mut color = parse_color(color)?;
                    color[3] *= style.opacity;
                    style.fill = Some(color);
                    style.fill_gradient = None;
                } else if style.fill.is_none() {
                    style.fill = style.stroke;
                }
                append_path(
                    buffers,
                    frame,
                    &point_node,
                    &circle_path(mark.radius.unwrap_or(*radius) * radius_scale),
                    &style,
                )?;
            }
        }
        NodeKind::Text {
            text,
            font_size,
            font_family,
            align,
            weight,
            slant,
        } => append_text(
            buffers,
            frame,
            node,
            text,
            *font_size,
            font_family,
            *align,
            *weight,
            *slant,
            text_engine,
        )?,
        NodeKind::MarkupText {
            spans,
            font_size,
            font_family,
            align,
        } => {
            let mut layouts = Vec::with_capacity(spans.len());
            let mut total_width = 0.0;
            for span in spans {
                let layout = text_engine
                    .layout_family_variant(
                        &span.text,
                        *font_size,
                        ShapedTextAlign::Left,
                        1.25,
                        0.0,
                        FontSelection {
                            family: font_family,
                            variant: font_variant(span.weight, span.slant),
                        },
                    )
                    .map_err(|error| format!("Text shaping failed for {}: {error}", node.id))?;
                total_width += layout.width;
                layouts.push(layout);
            }
            let mut cursor = match align {
                TextAlign::Left => 0.0,
                TextAlign::Center => -total_width * 0.5,
                TextAlign::Right => -total_width,
            };
            for (span, layout) in spans.iter().zip(layouts) {
                let span_color = span.color.as_deref().map(parse_color).transpose()?;
                for glyph in layout.glyphs {
                    let Some(path) = text_engine
                        .glyph_outline_for_font(glyph.font_id, glyph.glyph_id, glyph.variant)
                        .map_err(|error| {
                            format!("Glyph outline failed for {}: {error}", node.id)
                        })?
                    else {
                        continue;
                    };
                    let mut glyph_node = node.clone();
                    glyph_node.transform = local_matrix(
                        node.transform,
                        cursor + glyph.x,
                        glyph.y,
                        glyph.scale,
                        glyph.scale,
                    );
                    let mut style = glyph_node.style.clone();
                    style.fill = span_color
                        .map(|mut color| {
                            color[3] *= node.style.opacity;
                            color
                        })
                        .or(style.fill)
                        .or(style.stroke);
                    style.fill_gradient = None;
                    style.stroke = None;
                    style.stroke_gradient = None;
                    append_path(buffers, frame, &glyph_node, path, &style)?;
                }
                cursor += layout.width;
            }
        }
        NodeKind::Group | NodeKind::Billboard { .. } => return Ok(true),
        NodeKind::PathRef { source } => {
            return Err(format!(
                "Unresolved retained path reference {} -> {source}.",
                node.id
            ));
        }
        NodeKind::TracePath { .. } => {
            return Err(format!("Unresolved retained trace path {}.", node.id));
        }
        NodeKind::Path3d { .. }
        | NodeKind::Svg { .. }
        | NodeKind::Image { .. }
        | NodeKind::Mesh { .. }
        | NodeKind::Surface { .. }
        | NodeKind::CustomShaderMesh { .. } => return Ok(false),
    }
    Ok(true)
}

#[allow(clippy::too_many_arguments)]
fn append_text(
    buffers: &mut VertexBuffers<Vertex, u32>,
    frame: &EvaluatedFrameView<'_>,
    node: &EvaluatedNodeView<'_>,
    text: &str,
    font_size: f32,
    font_family: &str,
    align: TextAlign,
    weight: FontWeight,
    slant: FontSlant,
    text_engine: &mut TextEngine,
) -> Result<(), String> {
    let align = match align {
        TextAlign::Left => ShapedTextAlign::Left,
        TextAlign::Center => ShapedTextAlign::Center,
        TextAlign::Right => ShapedTextAlign::Right,
    };
    let layout = text_engine
        .layout_family_variant(
            text,
            font_size,
            align,
            1.25,
            0.0,
            FontSelection {
                family: font_family,
                variant: font_variant(weight, slant),
            },
        )
        .map_err(|error| format!("Text shaping failed for {}: {error}", node.id))?;
    for glyph in layout.glyphs {
        let Some(path) = text_engine
            .glyph_outline_for_font(glyph.font_id, glyph.glyph_id, glyph.variant)
            .map_err(|error| format!("Glyph outline failed for {}: {error}", node.id))?
        else {
            continue;
        };
        let mut glyph_node = node.clone();
        glyph_node.transform =
            local_matrix(node.transform, glyph.x, glyph.y, glyph.scale, glyph.scale);
        let mut style = glyph_node.style.clone();
        if style.fill.is_none() && style.fill_gradient.is_none() {
            style.fill = style.stroke;
            style.fill_gradient = style.stroke_gradient.take();
            style.stroke = None;
        }
        append_path(buffers, frame, &glyph_node, path, &style)?;
    }
    Ok(())
}

fn font_variant(weight: FontWeight, slant: FontSlant) -> FontVariant {
    match (weight, slant) {
        (FontWeight::Normal, FontSlant::Normal) => FontVariant::Regular,
        (FontWeight::Bold, FontSlant::Normal) => FontVariant::Bold,
        (FontWeight::Normal, FontSlant::Italic) => FontVariant::Italic,
        (FontWeight::Bold, FontSlant::Italic) => FontVariant::BoldItalic,
    }
}

fn append_path(
    buffers: &mut VertexBuffers<Vertex, u32>,
    frame: &EvaluatedFrameView<'_>,
    node: &EvaluatedNodeView<'_>,
    path: &Path,
    style: &EvaluatedStyle,
) -> Result<(), String> {
    let full_path = style.draw_start <= 0.000_001 && style.draw_progress >= 0.999_999;
    if full_path && let Some(paint) = Paint::fill(style) {
        FillTessellator::new()
            .tessellate_path(
                path,
                &FillOptions::default().with_tolerance(curve_tolerance(frame, node.transform)),
                &mut BuffersBuilder::new(
                    buffers,
                    VertexConstructor {
                        frame,
                        matrix: node.transform,
                        paint,
                    },
                ),
            )
            .map_err(|error| format!("Fill tessellation failed for {}: {error}.", node.id))?;
    }

    if style.stroke_width > 0.0
        && let Some(paint) = Paint::stroke(style)
        && style.draw_progress > style.draw_start
    {
        let partial;
        let stroke_path = if full_path {
            path
        } else {
            partial = partial_path(path, style.draw_start, style.draw_progress);
            &partial
        };
        let dashed_path = build_dashed_path(
            stroke_path,
            &style.dash_array,
            style.dash_offset,
            curve_tolerance(frame, node.transform),
        )?;
        let stroke_path = dashed_path.as_ref().unwrap_or(stroke_path);
        StrokeTessellator::new()
            .tessellate_path(
                stroke_path,
                &StrokeOptions::default()
                    .with_line_width(style.stroke_width)
                    .with_line_cap(match style.stroke_cap {
                        StrokeCap::Butt => lyon::tessellation::LineCap::Butt,
                        StrokeCap::Square => lyon::tessellation::LineCap::Square,
                        StrokeCap::Round => lyon::tessellation::LineCap::Round,
                    })
                    .with_line_join(match style.stroke_join {
                        StrokeJoin::Miter => lyon::tessellation::LineJoin::Miter,
                        StrokeJoin::MiterClip => lyon::tessellation::LineJoin::MiterClip,
                        StrokeJoin::Round => lyon::tessellation::LineJoin::Round,
                        StrokeJoin::Bevel => lyon::tessellation::LineJoin::Bevel,
                    })
                    .with_tolerance(curve_tolerance(frame, node.transform)),
                &mut BuffersBuilder::new(
                    buffers,
                    VertexConstructor {
                        frame,
                        matrix: node.transform,
                        paint,
                    },
                ),
            )
            .map_err(|error| format!("Stroke tessellation failed for {}: {error}.", node.id))?;
    }
    Ok(())
}

fn build_dashed_path(
    path: &Path,
    dash_array: &[f32],
    dash_offset: f32,
    tolerance: f32,
) -> Result<Option<Path>, String> {
    if dash_array.is_empty() {
        return Ok(None);
    }

    // SVG repeats odd-length dash lists to produce an even on/off cycle.
    let mut pattern = dash_array.to_vec();
    if pattern.len() % 2 == 1 {
        pattern.extend_from_slice(dash_array);
    }
    let cycle = pattern.iter().sum::<f32>();
    if !cycle.is_finite() || cycle <= f32::EPSILON {
        return Ok(None);
    }

    let mut output = Path::builder();
    let minimum_segment = pattern.iter().copied().fold(f32::INFINITY, f32::min);
    let mut source_builder = Path::builder();
    let mut source_active = false;
    let mut estimated_segments = 0.0f32;

    for event in path.iter() {
        match event {
            PathEvent::Begin { at } => {
                source_builder.begin(at);
                source_active = true;
            }
            PathEvent::Line { to, .. } => {
                source_builder.line_to(to);
            }
            PathEvent::Quadratic { ctrl, to, .. } => {
                source_builder.quadratic_bezier_to(ctrl, to);
            }
            PathEvent::Cubic {
                ctrl1, ctrl2, to, ..
            } => {
                source_builder.cubic_bezier_to(ctrl1, ctrl2, to);
            }
            PathEvent::End { close, .. } => {
                source_builder.end(close);
                let subpath = std::mem::replace(&mut source_builder, Path::builder()).build();
                source_active = false;
                let measurements = PathMeasurements::from_path(&subpath, tolerance);
                let length = measurements.length();
                estimated_segments += length / minimum_segment;
                if estimated_segments > 200_000.0 {
                    return Err("Dash pattern produces more than 200,000 path segments.".to_owned());
                }

                // SVG restarts the dash pattern at the beginning of every subpath.
                let mut pattern_index = 0usize;
                let mut phase = dash_offset.rem_euclid(cycle);
                while phase >= pattern[pattern_index] {
                    phase -= pattern[pattern_index];
                    pattern_index = (pattern_index + 1) % pattern.len();
                }
                let mut remaining = pattern[pattern_index] - phase;
                let mut position = 0.0f32;
                let mut sampler = measurements.create_sampler(&subpath, SampleType::Distance);
                while position < length {
                    let end = (position + remaining).min(length);
                    if pattern_index % 2 == 0 && end > position {
                        sampler.split_range(position..end, &mut output);
                    }
                    position = end;
                    pattern_index = (pattern_index + 1) % pattern.len();
                    remaining = pattern[pattern_index];
                }
            }
        }
    }
    if source_active {
        return Err("Cannot dash an unterminated path.".to_owned());
    }
    Ok(Some(output.build()))
}

#[derive(Clone, Copy)]
enum Paint<'a> {
    Solid([f32; 4]),
    Gradient(&'a EvaluatedLinearGradient),
}

impl<'a> Paint<'a> {
    fn fill(style: &'a EvaluatedStyle) -> Option<Self> {
        style
            .fill_gradient
            .as_ref()
            .map(Self::Gradient)
            .or(style.fill.map(Self::Solid))
    }

    fn stroke(style: &'a EvaluatedStyle) -> Option<Self> {
        style
            .stroke_gradient
            .as_ref()
            .map(Self::Gradient)
            .or(style.stroke.map(Self::Solid))
    }

    fn color(self, local: [f32; 2], world: [f32; 2]) -> [f32; 4] {
        let Self::Gradient(gradient) = self else {
            let Self::Solid(color) = self else {
                unreachable!();
            };
            return color;
        };
        let position = match gradient.space {
            GradientSpace::Local => local,
            GradientSpace::World => world,
        };
        let delta = [
            gradient.to[0] - gradient.from[0],
            gradient.to[1] - gradient.from[1],
        ];
        let denominator = delta[0] * delta[0] + delta[1] * delta[1];
        let mut amount = if denominator <= f32::EPSILON {
            0.0
        } else {
            ((position[0] - gradient.from[0]) * delta[0]
                + (position[1] - gradient.from[1]) * delta[1])
                / denominator
        };
        amount = match gradient.spread {
            GradientSpread::Pad => amount.clamp(0.0, 1.0),
            GradientSpread::Repeat => amount.rem_euclid(1.0),
            GradientSpread::Reflect => {
                let reflected = amount.rem_euclid(2.0);
                if reflected <= 1.0 {
                    reflected
                } else {
                    2.0 - reflected
                }
            }
        };
        if let Some(first) = gradient.stops.first() {
            if amount <= first.offset {
                return first.color;
            }
        } else {
            return [0.0; 4];
        }
        for pair in gradient.stops.windows(2) {
            if amount <= pair[1].offset {
                let span = (pair[1].offset - pair[0].offset).max(f32::EPSILON);
                let mix = ((amount - pair[0].offset) / span).clamp(0.0, 1.0);
                return std::array::from_fn(|index| {
                    pair[0].color[index] + (pair[1].color[index] - pair[0].color[index]) * mix
                });
            }
        }
        gradient.stops.last().map_or([0.0; 4], |stop| stop.color)
    }
}

struct VertexConstructor<'a> {
    frame: &'a EvaluatedFrameView<'a>,
    matrix: [f32; 6],
    paint: Paint<'a>,
}

impl FillVertexConstructor<Vertex> for VertexConstructor<'_> {
    fn new_vertex(&mut self, vertex: FillVertex<'_>) -> Vertex {
        self.vertex(vertex.position())
    }
}

impl StrokeVertexConstructor<Vertex> for VertexConstructor<'_> {
    fn new_vertex(&mut self, vertex: StrokeVertex<'_, '_>) -> Vertex {
        self.vertex(vertex.position())
    }
}

impl VertexConstructor<'_> {
    fn vertex(&self, position: Point) -> Vertex {
        let local = [position.x, position.y];
        let world = apply_matrix(self.matrix, local);
        Vertex {
            position: to_clip(self.frame, world),
            color: self.paint.color(local, world),
        }
    }
}

fn circle_path(radius: f32) -> Path {
    let points = (0..96)
        .map(|index| {
            let angle = index as f32 / 96.0 * TAU;
            [angle.cos() * radius, angle.sin() * radius]
        })
        .collect::<Vec<_>>();
    polyline_path(&points, true)
}

fn rect_path(width: f32, height: f32, corner_radius: f32) -> Path {
    let half_width = width * 0.5;
    let half_height = height * 0.5;
    let radius = corner_radius.min(half_width).min(half_height).max(0.0);
    let mut builder = Path::builder().with_svg();
    if radius <= f32::EPSILON {
        builder.move_to(point(-half_width, -half_height));
        builder.line_to(point(half_width, -half_height));
        builder.line_to(point(half_width, half_height));
        builder.line_to(point(-half_width, half_height));
        builder.close();
        return builder.build();
    }
    builder.move_to(point(-half_width + radius, -half_height));
    builder.line_to(point(half_width - radius, -half_height));
    builder.quadratic_bezier_to(
        point(half_width, -half_height),
        point(half_width, -half_height + radius),
    );
    builder.line_to(point(half_width, half_height - radius));
    builder.quadratic_bezier_to(
        point(half_width, half_height),
        point(half_width - radius, half_height),
    );
    builder.line_to(point(-half_width + radius, half_height));
    builder.quadratic_bezier_to(
        point(-half_width, half_height),
        point(-half_width, half_height - radius),
    );
    builder.line_to(point(-half_width, -half_height + radius));
    builder.quadratic_bezier_to(
        point(-half_width, -half_height),
        point(-half_width + radius, -half_height),
    );
    builder.close();
    builder.build()
}

fn polyline_path(points: &[[f32; 2]], closed: bool) -> Path {
    let mut builder = Path::builder();
    if let Some(first) = points.first() {
        builder.begin(point(first[0], first[1]));
        for next in points.iter().skip(1) {
            builder.line_to(point(next[0], next[1]));
        }
        if closed {
            builder.close();
        } else {
            builder.end(false);
        }
    }
    builder.build()
}

fn command_path(commands: &[PathCommand]) -> Result<Path, String> {
    let mut builder = Path::builder().with_svg();
    let mut started = false;
    for command in commands {
        match command {
            PathCommand::MoveTo { x, y } => {
                builder.move_to(point(*x, *y));
                started = true;
            }
            PathCommand::LineTo { x, y } => {
                if !started {
                    return Err("Path lineTo requires a preceding moveTo.".to_owned());
                }
                builder.line_to(point(*x, *y));
            }
            PathCommand::QuadTo { cx, cy, x, y } => {
                if !started {
                    return Err("Path quadTo requires a preceding moveTo.".to_owned());
                }
                builder.quadratic_bezier_to(point(*cx, *cy), point(*x, *y));
            }
            PathCommand::CubicTo {
                c1x,
                c1y,
                c2x,
                c2y,
                x,
                y,
            } => {
                if !started {
                    return Err("Path cubicTo requires a preceding moveTo.".to_owned());
                }
                builder.cubic_bezier_to(point(*c1x, *c1y), point(*c2x, *c2y), point(*x, *y));
            }
            PathCommand::Close => {
                if started {
                    builder.close();
                    started = false;
                }
            }
        }
    }
    Ok(builder.build())
}

fn partial_path(path: &Path, start: f32, end: f32) -> Path {
    let segments = path
        .iter()
        .flattened(0.002)
        .filter_map(|event| match event {
            PathEvent::Line { from, to } => Some((from, to)),
            PathEvent::End {
                last,
                first,
                close: true,
            } => Some((last, first)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let lengths = segments
        .iter()
        .map(|(from, to)| from.distance_to(*to))
        .collect::<Vec<_>>();
    let total = lengths.iter().sum::<f32>();
    let start = total * start.clamp(0.0, 1.0);
    let end = total * end.clamp(0.0, 1.0);
    let mut consumed = 0.0;
    let mut builder = Path::builder();
    for ((segment_from, segment_to), length) in segments.into_iter().zip(lengths) {
        let segment_start = consumed;
        let segment_end = consumed + length;
        consumed = segment_end;
        if segment_end < start || segment_start > end || length <= f32::EPSILON {
            continue;
        }
        let first = ((start - segment_start) / length).clamp(0.0, 1.0);
        let last = ((end - segment_start) / length).clamp(0.0, 1.0);
        let from = segment_from.lerp(segment_to, first);
        let to = segment_from.lerp(segment_to, last);
        builder.begin(from);
        builder.line_to(to);
        builder.end(false);
    }
    builder.build()
}

fn curve_tolerance(frame: &EvaluatedFrameView<'_>, matrix: [f32; 6]) -> f32 {
    let trace = matrix[0] * matrix[0]
        + matrix[1] * matrix[1]
        + matrix[2] * matrix[2]
        + matrix[3] * matrix[3];
    let determinant = matrix[0] * matrix[3] - matrix[1] * matrix[2];
    let discriminant = (trace * trace - 4.0 * determinant * determinant).max(0.0);
    let maximum_stretch = ((trace + discriminant.sqrt()) * 0.5).sqrt();
    let screen_scale = (maximum_stretch * frame.camera.zoom).max(0.001);
    (0.006 / screen_scale).clamp(0.0001, 0.1)
}

fn translate_matrix(matrix: [f32; 6], x: f32, y: f32) -> [f32; 6] {
    [
        matrix[0],
        matrix[1],
        matrix[2],
        matrix[3],
        matrix[0] * x + matrix[2] * y + matrix[4],
        matrix[1] * x + matrix[3] * y + matrix[5],
    ]
}

fn local_matrix(matrix: [f32; 6], x: f32, y: f32, scale_x: f32, scale_y: f32) -> [f32; 6] {
    [
        matrix[0] * scale_x,
        matrix[1] * scale_x,
        matrix[2] * scale_y,
        matrix[3] * scale_y,
        matrix[0] * x + matrix[2] * y + matrix[4],
        matrix[1] * x + matrix[3] * y + matrix[5],
    ]
}

fn apply_matrix(matrix: [f32; 6], point: [f32; 2]) -> [f32; 2] {
    [
        matrix[0] * point[0] + matrix[2] * point[1] + matrix[4],
        matrix[1] * point[0] + matrix[3] * point[1] + matrix[5],
    ]
}

fn to_clip(frame: &EvaluatedFrameView<'_>, point: [f32; 2]) -> [f32; 2] {
    let Camera {
        x,
        y,
        zoom,
        rotation,
    } = frame.camera;
    let translated = [point[0] - x, point[1] - y];
    let (sin, cos) = (-rotation).sin_cos();
    let rotated = [
        (translated[0] * cos - translated[1] * sin) * zoom,
        (translated[0] * sin + translated[1] * cos) * zoom,
    ];
    [
        rotated[0] / (frame.width * 0.5),
        rotated[1] / (frame.height * 0.5),
    ]
}

fn node_kind_name(kind: &NodeKind) -> &'static str {
    match kind {
        NodeKind::Group => "group",
        NodeKind::Billboard { .. } => "billboard",
        NodeKind::Circle { .. } => "circle",
        NodeKind::Rect { .. } => "rect",
        NodeKind::Line { .. } => "line",
        NodeKind::Arrow { .. } => "arrow",
        NodeKind::Polyline { .. } => "polyline",
        NodeKind::Path { .. } => "path",
        NodeKind::Path3d { .. } => "path3d",
        NodeKind::TracePath { .. } => "tracePath",
        NodeKind::PathRef { .. } => "pathRef",
        NodeKind::Text { .. } => "text",
        NodeKind::MarkupText { .. } => "markupText",
        NodeKind::Svg { .. } => "svg",
        NodeKind::Image { .. } => "image",
        NodeKind::PointCloud { .. } => "pointCloud",
        NodeKind::Mesh { .. } => "mesh",
        NodeKind::Surface { .. } => "surface",
        NodeKind::CustomShaderMesh { .. } => "customShaderMesh",
    }
}

#[cfg(test)]
mod tests {
    use lyon::math::point;
    use lyon::path::{Event as PathEvent, Path};
    use realtime_manim_scene_core::Scene;
    use realtime_manim_text_engine::TextEngine;

    use super::{build_dashed_path, build_geometry, partial_path, polyline_path};

    const TEST_SCENE: &str = r##"{
        "version":2,"title":"native geometry","width":8,"height":4,"duration":2,
        "background":"#000000","nodes":[
          {"id":"circle","type":"circle","radius":1,"style":{"fill":"#ff0000","stroke":"#ffffff","strokeWidth":0.05}},
          {"id":"label","type":"text","text":"GPU","fontSize":0.4,"transform":{"y":-1.5},"style":{"fill":"#ffffff","stroke":null}}
        ],"tracks":[{"target":"circle","property":"x","keyframes":[{"at":0,"value":-1},{"at":2,"value":1}]}]
    }"##;

    #[test]
    fn builds_animated_vector_and_text_geometry() {
        let scene = Scene::from_json(TEST_SCENE).unwrap();
        let frame = scene.evaluate_view(1.0).unwrap();
        let mut text = TextEngine::new().unwrap();
        let geometry = build_geometry(&frame, &mut text).unwrap();
        assert_eq!(geometry.visible_nodes, 2);
        assert_eq!(geometry.rendered_nodes, 2);
        assert!(geometry.triangle_count() > 100);
        assert!(geometry.unsupported.is_empty());
        assert!(
            geometry
                .vertices
                .iter()
                .all(|vertex| { vertex.position[0].is_finite() && vertex.position[1].is_finite() })
        );
    }

    #[test]
    fn draw_range_produces_a_shorter_path() {
        let full = polyline_path(&[[0.0, 0.0], [2.0, 0.0], [2.0, 2.0]], false);
        let partial = partial_path(&full, 0.25, 0.75);
        assert!(partial.iter().count() > 0);
        assert!(partial.iter().count() <= full.iter().count() * 2);
    }

    #[test]
    fn odd_dash_pattern_repeats_and_offset_changes_first_dash() {
        let line = polyline_path(&[[0.0, 0.0], [8.0, 0.0]], false);
        let plain = build_dashed_path(&line, &[1.0, 0.5, 0.25], 0.0, 0.001)
            .unwrap()
            .unwrap();
        let shifted = build_dashed_path(&line, &[1.0, 0.5, 0.25], -0.25, 0.001)
            .unwrap()
            .unwrap();
        let dash_count = |path: &Path| {
            path.iter()
                .filter(|event| matches!(event, PathEvent::Begin { .. }))
                .count()
        };
        assert!(dash_count(&plain) >= 4);
        assert!(dash_count(&shifted) >= 4);
        let first_end = |path: &Path| {
            path.iter().find_map(|event| match event {
                PathEvent::Line { to, .. } => Some(to.x),
                _ => None,
            })
        };
        assert_ne!(first_end(&plain), first_end(&shifted));
    }

    #[test]
    fn dash_pattern_restarts_for_every_subpath() {
        let mut source = Path::builder();
        source.begin(point(0.0, 0.0));
        source.line_to(point(1.5, 0.0));
        source.end(false);
        source.begin(point(10.0, 0.0));
        source.line_to(point(11.5, 0.0));
        source.end(false);
        let dashed = build_dashed_path(&source.build(), &[1.0, 1.0], 0.0, 0.001)
            .unwrap()
            .unwrap();
        let starts: Vec<f32> = dashed
            .iter()
            .filter_map(|event| match event {
                PathEvent::Begin { at } => Some(at.x),
                _ => None,
            })
            .collect();
        assert_eq!(starts, [0.0, 10.0]);
    }

    #[test]
    fn cap_join_and_dash_style_change_native_geometry() {
        let scene = Scene::from_json(
            r##"{
                "version":2,"title":"stroke semantics","width":8,"height":4,"duration":1,
                "background":"#000000","nodes":[
                  {"id":"line","type":"polyline","points":[[-3,0],[0,1],[3,0]],"closed":false,
                   "style":{"fill":null,"stroke":"#ffffff","strokeWidth":0.3,
                            "strokeCap":"round","strokeJoin":"bevel",
                            "dashArray":[0.8,0.35,0.2],"dashOffset":-0.15}}
                ]
            }"##,
        )
        .unwrap();
        let frame = scene.evaluate_view(0.0).unwrap();
        let mut text = TextEngine::new().unwrap();
        let dashed = build_geometry(&frame, &mut text).unwrap();

        let solid_scene = Scene::from_json(
            r##"{
                "version":2,"title":"stroke baseline","width":8,"height":4,"duration":1,
                "background":"#000000","nodes":[
                  {"id":"line","type":"polyline","points":[[-3,0],[0,1],[3,0]],"closed":false,
                   "style":{"fill":null,"stroke":"#ffffff","strokeWidth":0.3}}
                ]
            }"##,
        )
        .unwrap();
        let solid_frame = solid_scene.evaluate_view(0.0).unwrap();
        let solid = build_geometry(&solid_frame, &mut text).unwrap();
        assert!(dashed.triangle_count() > 0);
        assert_ne!(dashed.indices.len(), solid.indices.len());
    }

    #[test]
    fn round_caps_and_joins_are_tessellated_not_ignored() {
        fn triangle_count(points: &str, style: &str) -> usize {
            let scene = Scene::from_json(&format!(
                r##"{{
                    "version":2,"title":"stroke option","width":8,"height":4,"duration":1,
                    "background":"#000000","nodes":[
                      {{"id":"line","type":"polyline","points":{points},"closed":false,
                       "style":{{"fill":null,"stroke":"#ffffff","strokeWidth":0.4,{style}}}}}
                    ]
                }}"##
            ))
            .unwrap();
            let frame = scene.evaluate_view(0.0).unwrap();
            let mut text = TextEngine::new().unwrap();
            build_geometry(&frame, &mut text).unwrap().triangle_count()
        }

        let butt = triangle_count("[[-2,0],[2,0]]", r#""strokeCap":"butt""#);
        let round_cap = triangle_count("[[-2,0],[2,0]]", r#""strokeCap":"round""#);
        assert!(round_cap > butt);

        let bevel = triangle_count("[[-2,-1],[0,1],[2,-1]]", r#""strokeJoin":"bevel""#);
        let round_join = triangle_count("[[-2,-1],[0,1],[2,-1]]", r#""strokeJoin":"round""#);
        assert!(round_join > bevel);
    }
}
