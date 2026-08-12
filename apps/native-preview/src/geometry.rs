use std::{f32::consts::TAU, ops::Range};

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
    FontSlant, FontWeight, GradientSpace, GradientSpread, ImageResampling, NodeKind, PathCommand,
    PathCommand3d, StrokeCap, StrokeJoin, TextAlign, Transform, parse_color,
};
use realtime_manim_text_engine::{
    FontSelection, FontVariant, StyledTextSpan, TextAlign as ShapedTextAlign, TextEngine,
};

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub color: [f32; 4],
}

pub const VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 2] =
    wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4];

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct ImageVertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
    pub opacity: f32,
}

pub const IMAGE_VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 3] =
    wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32];

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct MeshTextureVertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
    pub opacity: f32,
    pub point: [f32; 3],
    pub normal: [f32; 3],
    pub light_position: [f32; 3],
    pub gloss: f32,
    pub shadow: f32,
    pub has_dark_texture: f32,
    pub unlit: f32,
}

pub const MESH_TEXTURE_VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 10] = wgpu::vertex_attr_array![
    0 => Float32x3,
    1 => Float32x2,
    2 => Float32,
    3 => Float32x3,
    4 => Float32x3,
    5 => Float32x3,
    6 => Float32,
    7 => Float32,
    8 => Float32,
    9 => Float32
];

#[derive(Debug)]
pub enum DrawCommand {
    Vector {
        indices: Range<u32>,
        depth_test: bool,
        transparent_3d: bool,
    },
    Image {
        vertices: Range<u32>,
        key: String,
        resampling: ImageResampling,
    },
    MeshTexture {
        vertices: Range<u32>,
        key: String,
        resampling: ImageResampling,
        opaque_candidate: bool,
    },
}

#[derive(Debug)]
pub struct TransparentPrimitive {
    pub depth: f32,
    pub command_index: usize,
    pub range: Range<u32>,
}

#[derive(Debug)]
pub struct Geometry {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub image_vertices: Vec<ImageVertex>,
    pub mesh_texture_vertices: Vec<MeshTextureVertex>,
    pub commands: Vec<DrawCommand>,
    pub transparent_primitives: Vec<TransparentPrimitive>,
    pub visible_nodes: usize,
    pub rendered_nodes: usize,
    pub unsupported: Vec<String>,
}

impl Geometry {
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
            + self.image_vertices.len() / 3
            + self.mesh_texture_vertices.len() / 3
    }
}

pub fn build_geometry(
    frame: &EvaluatedFrameView<'_>,
    text_engine: &mut TextEngine,
) -> Result<Geometry, String> {
    let mut buffers = VertexBuffers::<Vertex, u32>::new();
    let mut image_vertices = Vec::new();
    let mut mesh_texture_vertices = Vec::new();
    let mut commands = Vec::new();
    let mut transparent_primitives = Vec::new();
    let mut unsupported = Vec::new();
    let mut rendered_nodes = 0;
    for node in &frame.nodes {
        let supported = match node.kind.as_ref() {
            NodeKind::Image {
                corners,
                resampling,
                ..
            } => {
                let start = image_vertices.len() as u32;
                append_image_vertices(&mut image_vertices, frame, node, corners)?;
                commands.push(DrawCommand::Image {
                    vertices: start..image_vertices.len() as u32,
                    key: node.id.to_owned(),
                    resampling: *resampling,
                });
                true
            }
            NodeKind::Mesh {
                vertices,
                triangles,
                normals,
                uvs,
                texture_pixels,
                dark_texture_pixels,
                texture_resampling,
                gloss,
                shadow,
                light_position,
                unlit,
                double_sided,
                ..
            } if !texture_pixels.is_empty() => {
                let start = mesh_texture_vertices.len() as u32;
                let triangle_depths = append_textured_mesh_vertices(
                    &mut mesh_texture_vertices,
                    frame,
                    node,
                    vertices,
                    triangles,
                    uvs,
                    normals,
                    *gloss,
                    *shadow,
                    *light_position,
                    !dark_texture_pixels.is_empty(),
                    *unlit,
                    *double_sided,
                )?;
                let end = mesh_texture_vertices.len() as u32;
                if end > start {
                    let command_index = commands.len();
                    commands.push(DrawCommand::MeshTexture {
                        vertices: start..end,
                        key: node.id.to_owned(),
                        resampling: *texture_resampling,
                        opaque_candidate: node.style.opacity >= 0.999,
                    });
                    transparent_primitives.extend(triangle_depths.into_iter().enumerate().map(
                        |(triangle, depth)| TransparentPrimitive {
                            depth,
                            command_index,
                            range: start + triangle as u32 * 3..start + triangle as u32 * 3 + 3,
                        },
                    ));
                }
                true
            }
            _ => {
                let start = buffers.indices.len() as u32;
                let supported = append_node(&mut buffers, frame, node, text_engine)?;
                let end = buffers.indices.len() as u32;
                if end > start {
                    let depth_test = mesh_node_is_opaque(node)?;
                    let transparent_3d = matches!(
                        node.kind.as_ref(),
                        NodeKind::Mesh { .. } | NodeKind::Surface { .. }
                    ) && !depth_test;
                    let command_index = commands.len();
                    commands.push(DrawCommand::Vector {
                        indices: start..end,
                        depth_test,
                        transparent_3d,
                    });
                    if transparent_3d {
                        for triangle_start in (start..end).step_by(3) {
                            transparent_primitives.push(TransparentPrimitive {
                                depth: vector_triangle_view_depth(frame, &buffers, triangle_start),
                                command_index,
                                range: triangle_start..triangle_start + 3,
                            });
                        }
                    }
                }
                supported
            }
        };
        if supported {
            rendered_nodes += 1;
        } else {
            unsupported.push(format!(
                "{}:{}",
                node.id,
                node_kind_name(node.kind.as_ref())
            ));
        }
    }
    transparent_primitives.sort_by(|left, right| right.depth.total_cmp(&left.depth));
    Ok(Geometry {
        vertices: buffers.vertices,
        indices: buffers.indices,
        image_vertices,
        mesh_texture_vertices,
        commands,
        transparent_primitives,
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
        NodeKind::Path3d { commands } => {
            let commands = project_path_3d(frame, node, commands)?;
            if !commands.is_empty() {
                let mut projected_node = node.clone();
                projected_node.transform = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
                append_path(
                    buffers,
                    frame,
                    &projected_node,
                    &command_path(&commands)?,
                    &node.style,
                )?;
            }
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
            let shaped_spans = spans
                .iter()
                .map(|span| StyledTextSpan {
                    text: &span.text,
                    variant: font_variant(span.weight, span.slant),
                })
                .collect::<Vec<_>>();
            let align = match align {
                TextAlign::Left => ShapedTextAlign::Left,
                TextAlign::Center => ShapedTextAlign::Center,
                TextAlign::Right => ShapedTextAlign::Right,
            };
            let layout = text_engine
                .layout_family_chain_spans(
                    &shaped_spans,
                    *font_size,
                    align,
                    1.25,
                    0.0,
                    font_family,
                    &[],
                )
                .map_err(|error| format!("Text shaping failed for {}: {error}", node.id))?;
            let mut source_end = 0usize;
            let mut span_ends = Vec::with_capacity(spans.len());
            let mut span_colors = Vec::with_capacity(spans.len());
            for span in spans {
                source_end = source_end.saturating_add(span.text.len());
                span_ends.push(source_end);
                span_colors.push(span.color.as_deref().map(parse_color).transpose()?);
            }
            for glyph in layout.glyphs {
                let Some(path) = text_engine
                    .glyph_outline_for_font(glyph.font_id, glyph.glyph_id, glyph.variant)
                    .map_err(|error| format!("Glyph outline failed for {}: {error}", node.id))?
                else {
                    continue;
                };
                let span_index = span_ends.partition_point(|end| *end <= glyph.source_start);
                let span_color = span_colors.get(span_index).copied().flatten();
                let mut glyph_node = node.clone();
                glyph_node.transform =
                    local_matrix(node.transform, glyph.x, glyph.y, glyph.scale, glyph.scale);
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
        NodeKind::Mesh {
            vertices,
            triangles,
            colors,
            normals,
            gloss,
            shadow,
            light_position,
            unlit,
            double_sided,
            texture_pixels,
            ..
        } => {
            if !texture_pixels.is_empty() {
                return Err(format!(
                    "Textured mesh {} reached the untextured native geometry path.",
                    node.id
                ));
            }
            append_mesh(
                buffers,
                frame,
                node,
                vertices,
                triangles,
                colors,
                normals,
                *gloss,
                *shadow,
                *light_position,
                *unlit,
                *double_sided,
            )?;
        }
        NodeKind::Surface {
            vertices,
            patches,
            colors,
            stroke_colors,
            stroke_radii,
            unlit,
            double_sided,
        } => append_surface(
            buffers,
            frame,
            node,
            vertices,
            patches,
            colors,
            stroke_colors,
            stroke_radii,
            *unlit,
            *double_sided,
        )?,
        NodeKind::Svg { .. } | NodeKind::CustomShaderMesh { .. } => return Ok(false),
        NodeKind::Image { .. } => {
            return Err(format!(
                "Image {} reached the vector native geometry path.",
                node.id
            ));
        }
    }
    Ok(true)
}

fn append_image_vertices(
    output: &mut Vec<ImageVertex>,
    frame: &EvaluatedFrameView<'_>,
    node: &EvaluatedNodeView<'_>,
    corners: &[[f32; 2]],
) -> Result<(), String> {
    if corners.len() != 4 {
        return Err(format!("Image {} requires four corners.", node.id));
    }
    let position = |index: usize| {
        let point = to_clip(
            frame,
            apply_matrix(node.transform, [corners[index][0], corners[index][1]]),
        );
        [point[0], point[1], 0.0]
    };
    let opacity = node.style.opacity;
    output.extend([
        ImageVertex {
            position: position(0),
            uv: [0.0, 0.0],
            opacity,
        },
        ImageVertex {
            position: position(2),
            uv: [0.0, 1.0],
            opacity,
        },
        ImageVertex {
            position: position(1),
            uv: [1.0, 0.0],
            opacity,
        },
        ImageVertex {
            position: position(1),
            uv: [1.0, 0.0],
            opacity,
        },
        ImageVertex {
            position: position(2),
            uv: [0.0, 1.0],
            opacity,
        },
        ImageVertex {
            position: position(3),
            uv: [1.0, 1.0],
            opacity,
        },
    ]);
    Ok(())
}

#[derive(Clone, Copy)]
struct TexturedClipVertex {
    point: [f32; 3],
    uv: [f32; 2],
    normal: [f32; 3],
}

#[derive(Clone, Copy)]
struct ColoredClipVertex {
    point: [f32; 3],
    color: [f32; 4],
}

fn clip_polygon_to_view_depth<T: Copy>(
    mut polygon: Vec<T>,
    near: f32,
    far: f32,
    depth: impl Fn(&T) -> f32 + Copy,
    interpolate: impl Fn(&T, &T, f32) -> T + Copy,
) -> Vec<T> {
    polygon = clip_polygon_to_view_plane(polygon, near, true, depth, interpolate);
    clip_polygon_to_view_plane(polygon, far, false, depth, interpolate)
}

fn clip_polygon_to_view_plane<T: Copy>(
    polygon: Vec<T>,
    boundary: f32,
    keep_greater: bool,
    depth: impl Fn(&T) -> f32 + Copy,
    interpolate: impl Fn(&T, &T, f32) -> T + Copy,
) -> Vec<T> {
    if polygon.is_empty() {
        return polygon;
    }
    let inside = |value: f32| {
        if keep_greater {
            value >= boundary
        } else {
            value <= boundary
        }
    };
    let mut output = Vec::with_capacity(polygon.len() + 1);
    let mut previous = *polygon.last().expect("non-empty polygon");
    let mut previous_depth = depth(&previous);
    let mut previous_inside = inside(previous_depth);
    for current in polygon {
        let current_depth = depth(&current);
        let current_inside = inside(current_depth);
        if current_inside != previous_inside {
            let amount =
                ((boundary - previous_depth) / (current_depth - previous_depth)).clamp(0.0, 1.0);
            output.push(interpolate(&previous, &current, amount));
        }
        if current_inside {
            output.push(current);
        }
        previous = current;
        previous_depth = current_depth;
        previous_inside = current_inside;
    }
    output
}

fn lerp_2d(from: [f32; 2], to: [f32; 2], amount: f32) -> [f32; 2] {
    [
        from[0] + (to[0] - from[0]) * amount,
        from[1] + (to[1] - from[1]) * amount,
    ]
}

fn lerp_3d(from: [f32; 3], to: [f32; 3], amount: f32) -> [f32; 3] {
    [
        from[0] + (to[0] - from[0]) * amount,
        from[1] + (to[1] - from[1]) * amount,
        from[2] + (to[2] - from[2]) * amount,
    ]
}

fn lerp_4d(from: [f32; 4], to: [f32; 4], amount: f32) -> [f32; 4] {
    [
        from[0] + (to[0] - from[0]) * amount,
        from[1] + (to[1] - from[1]) * amount,
        from[2] + (to[2] - from[2]) * amount,
        from[3] + (to[3] - from[3]) * amount,
    ]
}

#[allow(clippy::too_many_arguments)]
fn append_textured_mesh_vertices(
    output: &mut Vec<MeshTextureVertex>,
    frame: &EvaluatedFrameView<'_>,
    node: &EvaluatedNodeView<'_>,
    vertices: &[[f32; 3]],
    triangles: &[[u32; 3]],
    uvs: &[[f32; 2]],
    normals: &[[f32; 3]],
    gloss: f32,
    shadow: f32,
    light_position: [f32; 3],
    has_dark_texture: bool,
    unlit: bool,
    double_sided: bool,
) -> Result<Vec<f32>, String> {
    if uvs.len() != vertices.len() {
        return Err(format!(
            "Textured mesh {} requires one UV per vertex.",
            node.id
        ));
    }
    let transformed = vertices
        .iter()
        .map(|vertex| transform_point_3d(*vertex, node.transform_3d))
        .collect::<Vec<_>>();
    let transformed_normals = if normals.len() == vertices.len() {
        normals
            .iter()
            .map(|normal| transform_normal_3d(*normal, node.transform_3d))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("Mesh {} normal transform failed: {error}", node.id))?
    } else {
        Vec::new()
    };
    let camera = frame.camera_3d;
    let forward = normalize_3d(sub_3d(camera.target, camera.position));
    let right = normalize_3d(cross_3d(forward, camera.up));
    let camera_up = normalize_3d(cross_3d(right, forward));
    let tan_half_fov = (camera.fov_y * 0.5).tan().max(0.0001);
    let aspect = (frame.width / frame.height).max(0.0001);
    let mut projected_triangles = Vec::with_capacity(triangles.len());
    for triangle in triangles {
        let points = triangle.map(|index| transformed[index as usize]);
        let face_normal = normalize_3d(cross_3d(
            sub_3d(points[1], points[0]),
            sub_3d(points[2], points[0]),
        ));
        let center = mul_3d(add_3d(add_3d(points[0], points[1]), points[2]), 1.0 / 3.0);
        let facing = dot_3d(face_normal, normalize_3d(sub_3d(camera.position, center)));
        if !double_sided && facing <= 0.0 {
            continue;
        }
        let triangle_normals = if transformed_normals.is_empty() {
            let normal = if double_sided && facing < 0.0 {
                mul_3d(face_normal, -1.0)
            } else {
                face_normal
            };
            [normal; 3]
        } else {
            let normals = triangle.map(|index| transformed_normals[index as usize]);
            if double_sided && facing < 0.0 {
                normals.map(|normal| mul_3d(normal, -1.0))
            } else {
                normals
            }
        };
        let polygon = clip_polygon_to_view_depth(
            (0..3)
                .map(|index| TexturedClipVertex {
                    point: points[index],
                    uv: uvs[triangle[index] as usize],
                    normal: triangle_normals[index],
                })
                .collect(),
            camera.near,
            camera.far,
            |vertex| dot_3d(sub_3d(vertex.point, camera.position), forward),
            |from, to, amount| TexturedClipVertex {
                point: lerp_3d(from.point, to.point, amount),
                uv: lerp_2d(from.uv, to.uv, amount),
                normal: lerp_3d(from.normal, to.normal, amount),
            },
        );
        for corner in 1..polygon.len().saturating_sub(1) {
            let clipped = [polygon[0], polygon[corner], polygon[corner + 1]];
            let mut projected = [[0.0; 3]; 3];
            let mut depth = 0.0;
            for (index, vertex) in clipped.iter().enumerate() {
                let relative = sub_3d(vertex.point, camera.position);
                let view_z = dot_3d(relative, forward);
                projected[index] = [
                    dot_3d(relative, right) / (view_z * tan_half_fov * aspect),
                    dot_3d(relative, camera_up) / (view_z * tan_half_fov),
                    view_depth_to_clip(camera.near, camera.far, view_z),
                ];
                depth += view_z;
            }
            projected_triangles.push((
                depth / 3.0,
                projected,
                clipped.map(|vertex| vertex.uv),
                clipped.map(|vertex| vertex.point),
                clipped.map(|vertex| vertex.normal),
            ));
        }
    }
    projected_triangles.sort_by(|left, right| right.0.total_cmp(&left.0));
    let mut triangle_depths = Vec::with_capacity(projected_triangles.len());
    for (depth, positions, triangle_uvs, points, triangle_normals) in projected_triangles {
        triangle_depths.push(depth);
        output.extend(
            positions
                .into_iter()
                .zip(triangle_uvs)
                .zip(points)
                .zip(triangle_normals)
                .map(|(((position, uv), point), normal)| MeshTextureVertex {
                    position,
                    uv,
                    opacity: node.style.opacity,
                    point,
                    normal,
                    light_position,
                    gloss,
                    shadow,
                    has_dark_texture: f32::from(has_dark_texture),
                    unlit: f32::from(unlit),
                }),
        );
    }
    Ok(triangle_depths)
}

#[allow(clippy::too_many_arguments)]
fn append_surface(
    buffers: &mut VertexBuffers<Vertex, u32>,
    frame: &EvaluatedFrameView<'_>,
    node: &EvaluatedNodeView<'_>,
    surface_vertices: &[[f32; 3]],
    patches: &[Vec<u32>],
    colors: &[String],
    stroke_colors: &[String],
    stroke_radii: &[f32],
    unlit: bool,
    double_sided: bool,
) -> Result<(), String> {
    let mut vertices = Vec::new();
    let mut triangles = Vec::new();
    let mut expanded_colors = Vec::new();
    let mut normals = Vec::new();
    let mut color_offset = 0usize;
    for (patch_index, patch) in patches.iter().enumerate() {
        let points = patch
            .iter()
            .map(|index| surface_vertices[*index as usize])
            .collect::<Vec<_>>();
        let mut normal = [0.0; 3];
        for index in 0..points.len() {
            normal = add_3d(
                normal,
                cross_3d(points[index], points[(index + 1) % points.len()]),
            );
        }
        normal = normalize_3d(normal);
        if normal == [0.0; 3] {
            normal = [0.0, 0.0, 1.0];
        }

        let base = vertices.len() as u32;
        vertices.extend(points.iter().copied());
        normals.extend(std::iter::repeat_n(normal, points.len()));
        expanded_colors.extend_from_slice(&colors[color_offset..color_offset + points.len()]);
        color_offset += points.len();
        for corner in 1..points.len() - 1 {
            triangles.push([base, base + corner as u32, base + corner as u32 + 1]);
        }

        let radius = stroke_radii[patch_index];
        if radius <= f32::EPSILON {
            continue;
        }
        for index in 0..points.len() {
            let edge_start = points[index];
            let edge_end = points[(index + 1) % points.len()];
            let direction = sub_3d(edge_end, edge_start);
            let mut perpendicular = cross_3d(normal, direction);
            let perpendicular_length = dot_3d(perpendicular, perpendicular).sqrt();
            if perpendicular_length <= 1e-12 {
                perpendicular = [1.0, 0.0, 0.0];
            } else {
                perpendicular = mul_3d(perpendicular, 1.0 / perpendicular_length);
            }
            let offset = mul_3d(perpendicular, radius);
            let wire_base = vertices.len() as u32;
            vertices.extend([
                sub_3d(edge_start, offset),
                add_3d(edge_start, offset),
                sub_3d(edge_end, offset),
                add_3d(edge_end, offset),
            ]);
            normals.extend([normal; 4]);
            expanded_colors.extend(std::iter::repeat_n(stroke_colors[patch_index].clone(), 4));
            triangles.extend([
                [wire_base, wire_base + 1, wire_base + 2],
                [wire_base + 2, wire_base + 1, wire_base + 3],
            ]);
        }
    }
    append_mesh(
        buffers,
        frame,
        node,
        &vertices,
        &triangles,
        &expanded_colors,
        &normals,
        0.0,
        0.0,
        [0.0, 0.0, 8.0],
        unlit,
        double_sided,
    )
}

#[allow(clippy::too_many_arguments)]
fn append_mesh(
    buffers: &mut VertexBuffers<Vertex, u32>,
    frame: &EvaluatedFrameView<'_>,
    node: &EvaluatedNodeView<'_>,
    vertices: &[[f32; 3]],
    triangles: &[[u32; 3]],
    colors: &[String],
    normals: &[[f32; 3]],
    gloss: f32,
    shadow: f32,
    light_position: [f32; 3],
    unlit: bool,
    double_sided: bool,
) -> Result<(), String> {
    let Some(base_color) = node.style.fill.or(node.style.stroke) else {
        return Ok(());
    };
    let transformed = vertices
        .iter()
        .map(|vertex| transform_point_3d(*vertex, node.transform_3d))
        .collect::<Vec<_>>();
    let vertex_colors = if colors.is_empty() {
        vec![base_color; vertices.len()]
    } else {
        colors
            .iter()
            .map(|color| parse_color(color))
            .collect::<Result<Vec<_>, _>>()?
    };
    let transformed_normals = if normals.len() == vertices.len() {
        normals
            .iter()
            .map(|normal| transform_normal_3d(*normal, node.transform_3d))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("Mesh {} normal transform failed: {error}", node.id))?
    } else {
        Vec::new()
    };
    let camera = frame.camera_3d;
    let forward = normalize_3d(sub_3d(camera.target, camera.position));
    let right = normalize_3d(cross_3d(forward, camera.up));
    let camera_up = normalize_3d(cross_3d(right, forward));
    let light = normalize_3d(camera.light_direction);
    let tan_half_fov = (camera.fov_y * 0.5).tan().max(0.0001);
    let aspect = (frame.width / frame.height).max(0.0001);
    let mut projected_triangles = Vec::with_capacity(triangles.len());
    for triangle in triangles {
        let points = triangle.map(|index| transformed[index as usize]);
        let normal = normalize_3d(cross_3d(
            sub_3d(points[1], points[0]),
            sub_3d(points[2], points[0]),
        ));
        let center = mul_3d(add_3d(add_3d(points[0], points[1]), points[2]), 1.0 / 3.0);
        let facing = dot_3d(normal, normalize_3d(sub_3d(camera.position, center)));
        if !double_sided && facing <= 0.0 {
            continue;
        }
        let diffuse = if double_sided {
            dot_3d(normal, light).abs()
        } else {
            dot_3d(normal, light).max(0.0)
        };
        let brightness = camera.ambient + (1.0 - camera.ambient) * diffuse;
        let triangle_colors = triangle.map(|index| {
            let color = vertex_colors[index as usize];
            let alpha = if colors.is_empty() {
                color[3]
            } else {
                color[3] * node.style.opacity
            };
            if unlit {
                [color[0], color[1], color[2], alpha]
            } else if transformed_normals.is_empty() {
                [
                    color[0] * brightness,
                    color[1] * brightness,
                    color[2] * brightness,
                    alpha,
                ]
            } else {
                let point = transformed[index as usize];
                let mut normal = normalize_3d(transformed_normals[index as usize]);
                if double_sided && facing < 0.0 {
                    normal = mul_3d(normal, -1.0);
                }
                let to_camera = sub_3d(camera.position, point);
                let to_light = sub_3d(light_position, point);
                let reflection = add_3d(
                    mul_3d(to_light, -1.0),
                    mul_3d(normal, 2.0 * dot_3d(to_light, normal)),
                );
                let dot_product = dot_3d(normalize_3d(reflection), normalize_3d(to_camera));
                let shine = gloss * (-3.0 * (1.0 - dot_product).powi(2)).exp();
                let lit = 1.0 + (dot_3d(normalize_3d(to_light), normal).max(0.0) - 1.0) * shadow;
                [
                    lit * (color[0] + (1.0 - color[0]) * shine),
                    lit * (color[1] + (1.0 - color[1]) * shine),
                    lit * (color[2] + (1.0 - color[2]) * shine),
                    alpha,
                ]
            }
        });
        let polygon = clip_polygon_to_view_depth(
            (0..3)
                .map(|index| ColoredClipVertex {
                    point: points[index],
                    color: triangle_colors[index],
                })
                .collect(),
            camera.near,
            camera.far,
            |vertex| dot_3d(sub_3d(vertex.point, camera.position), forward),
            |from, to, amount| ColoredClipVertex {
                point: lerp_3d(from.point, to.point, amount),
                color: lerp_4d(from.color, to.color, amount),
            },
        );
        for corner in 1..polygon.len().saturating_sub(1) {
            let clipped = [polygon[0], polygon[corner], polygon[corner + 1]];
            let mut projected = [[0.0; 3]; 3];
            let mut depth = 0.0;
            for (index, vertex) in clipped.iter().enumerate() {
                let relative = sub_3d(vertex.point, camera.position);
                let view_z = dot_3d(relative, forward);
                projected[index] = [
                    dot_3d(relative, right) / (view_z * tan_half_fov * aspect),
                    dot_3d(relative, camera_up) / (view_z * tan_half_fov),
                    view_depth_to_clip(camera.near, camera.far, view_z),
                ];
                depth += view_z;
            }
            projected_triangles.push((depth / 3.0, projected, clipped.map(|vertex| vertex.color)));
        }
    }
    if !mesh_node_is_opaque(node)? {
        projected_triangles.sort_by(|left, right| right.0.total_cmp(&left.0));
    }
    for (_, positions, colors) in projected_triangles {
        let base = buffers.vertices.len() as u32;
        buffers.vertices.extend(
            positions
                .into_iter()
                .zip(colors)
                .map(|(position, color)| Vertex { position, color }),
        );
        buffers.indices.extend([base, base + 1, base + 2]);
    }
    Ok(())
}

fn mesh_node_is_opaque(node: &EvaluatedNodeView<'_>) -> Result<bool, String> {
    if node.style.opacity < 0.999 {
        return Ok(false);
    }
    let colors_are_opaque = |colors: &[String]| -> Result<bool, String> {
        colors.iter().try_fold(true, |opaque, color| {
            Ok(opaque && parse_color(color)?[3] >= 0.999)
        })
    };
    match node.kind.as_ref() {
        NodeKind::Mesh {
            colors,
            texture_pixels,
            ..
        } if texture_pixels.is_empty() => {
            if colors.is_empty() {
                Ok(node
                    .style
                    .fill
                    .or(node.style.stroke)
                    .is_some_and(|color| color[3] >= 0.999))
            } else {
                colors_are_opaque(colors)
            }
        }
        NodeKind::Surface {
            colors,
            stroke_colors,
            stroke_radii,
            ..
        } => {
            let opaque_strokes = stroke_colors.iter().zip(stroke_radii).try_fold(
                true,
                |opaque, (color, radius)| {
                    Ok::<bool, String>(
                        opaque && (*radius <= f32::EPSILON || parse_color(color)?[3] >= 0.999),
                    )
                },
            )?;
            Ok(colors_are_opaque(colors)? && opaque_strokes)
        }
        _ => Ok(false),
    }
}

const PATH_3D_FLATNESS: f32 = 0.001;
const PATH_3D_MAX_SUBDIVISION_DEPTH: u8 = 12;
const PATH_3D_MAX_FLATTENED_POINTS: usize = 500_000;

struct FlattenedSubpath3d {
    points: Vec<[f32; 3]>,
    closed: bool,
}

#[derive(Clone, Copy)]
struct PathProjection3d {
    position: [f32; 3],
    right: [f32; 3],
    up: [f32; 3],
    forward: [f32; 3],
    near: f32,
    far: f32,
    tan_half_fov: f32,
    aspect: f32,
    half_width: f32,
    half_height: f32,
}

impl PathProjection3d {
    fn new(frame: &EvaluatedFrameView<'_>) -> Self {
        let camera = frame.camera_3d;
        let forward = normalize_3d(sub_3d(camera.target, camera.position));
        let right = normalize_3d(cross_3d(forward, camera.up));
        Self {
            position: camera.position,
            right,
            up: normalize_3d(cross_3d(right, forward)),
            forward,
            near: camera.near,
            far: camera.far,
            tan_half_fov: (camera.fov_y * 0.5).tan().max(0.0001),
            aspect: (frame.width / frame.height).max(0.0001),
            half_width: frame.width * 0.5,
            half_height: frame.height * 0.5,
        }
    }

    fn world_to_view(self, point: [f32; 3]) -> [f32; 3] {
        let relative = sub_3d(point, self.position);
        [
            dot_3d(relative, self.right),
            dot_3d(relative, self.up),
            dot_3d(relative, self.forward),
        ]
    }

    fn project(self, point: [f32; 3]) -> [f32; 2] {
        [
            point[0] / (point[2] * self.tan_half_fov * self.aspect) * self.half_width,
            point[1] / (point[2] * self.tan_half_fov) * self.half_height,
        ]
    }
}

fn project_path_3d(
    frame: &EvaluatedFrameView<'_>,
    node: &EvaluatedNodeView<'_>,
    commands: &[PathCommand3d],
) -> Result<Vec<PathCommand>, String> {
    let projection = PathProjection3d::new(frame);
    let subpaths = flatten_path_3d(node, commands, projection)?;
    let mut projected = Vec::new();
    for subpath in subpaths {
        if subpath.closed {
            append_clipped_closed_path(&mut projected, &subpath.points, projection);
        } else {
            append_clipped_open_path(&mut projected, &subpath.points, projection);
        }
    }
    Ok(projected)
}

fn flatten_path_3d(
    node: &EvaluatedNodeView<'_>,
    commands: &[PathCommand3d],
    projection: PathProjection3d,
) -> Result<Vec<FlattenedSubpath3d>, String> {
    let to_view = |point| projection.world_to_view(transform_point_3d(point, node.transform_3d));
    let mut subpaths = Vec::new();
    let mut active: Option<FlattenedSubpath3d> = None;
    let mut current = None;
    let mut point_count = 0usize;
    for command in commands {
        match command {
            PathCommand3d::MoveTo { x, y, z } => {
                finish_flattened_subpath(&mut subpaths, &mut active);
                let point = to_view([*x, *y, *z]);
                active = Some(FlattenedSubpath3d {
                    points: Vec::new(),
                    closed: false,
                });
                push_flattened_point(
                    &mut active.as_mut().expect("subpath was just created").points,
                    point,
                    &mut point_count,
                    node.id,
                )?;
                current = Some(point);
            }
            PathCommand3d::LineTo { x, y, z } => {
                let _ =
                    current.ok_or_else(|| "Path lineTo requires a preceding moveTo.".to_owned())?;
                let point = to_view([*x, *y, *z]);
                push_flattened_point(
                    &mut active.as_mut().expect("active point has a subpath").points,
                    point,
                    &mut point_count,
                    node.id,
                )?;
                current = Some(point);
            }
            PathCommand3d::QuadTo {
                cx,
                cy,
                cz,
                x,
                y,
                z,
            } => {
                let start =
                    current.ok_or_else(|| "Path quadTo requires a preceding moveTo.".to_owned())?;
                let control = to_view([*cx, *cy, *cz]);
                let end = to_view([*x, *y, *z]);
                flatten_quad_3d(
                    start,
                    control,
                    end,
                    projection.near,
                    projection.far,
                    0,
                    &mut active.as_mut().expect("active point has a subpath").points,
                    &mut point_count,
                    node.id,
                )?;
                current = Some(end);
            }
            PathCommand3d::CubicTo {
                c1x,
                c1y,
                c1z,
                c2x,
                c2y,
                c2z,
                x,
                y,
                z,
            } => {
                let start = current
                    .ok_or_else(|| "Path cubicTo requires a preceding moveTo.".to_owned())?;
                let control_1 = to_view([*c1x, *c1y, *c1z]);
                let control_2 = to_view([*c2x, *c2y, *c2z]);
                let end = to_view([*x, *y, *z]);
                flatten_cubic_3d(
                    start,
                    control_1,
                    control_2,
                    end,
                    projection.near,
                    projection.far,
                    0,
                    &mut active.as_mut().expect("active point has a subpath").points,
                    &mut point_count,
                    node.id,
                )?;
                current = Some(end);
            }
            PathCommand3d::Close => {
                if let Some(subpath) = active.as_mut() {
                    subpath.closed = true;
                }
                finish_flattened_subpath(&mut subpaths, &mut active);
                current = None;
            }
        }
    }
    finish_flattened_subpath(&mut subpaths, &mut active);
    Ok(subpaths)
}

fn finish_flattened_subpath(
    output: &mut Vec<FlattenedSubpath3d>,
    active: &mut Option<FlattenedSubpath3d>,
) {
    if let Some(subpath) = active.take() {
        output.push(subpath);
    }
}

fn push_flattened_point(
    points: &mut Vec<[f32; 3]>,
    point: [f32; 3],
    point_count: &mut usize,
    node_id: &str,
) -> Result<(), String> {
    if points.last().is_some_and(|previous| *previous == point) {
        return Ok(());
    }
    if *point_count >= PATH_3D_MAX_FLATTENED_POINTS {
        return Err(format!(
            "Path3d {node_id} exceeds the native flattening limit of {PATH_3D_MAX_FLATTENED_POINTS} points."
        ));
    }
    points.push(point);
    *point_count += 1;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn flatten_quad_3d(
    start: [f32; 3],
    control: [f32; 3],
    end: [f32; 3],
    near: f32,
    far: f32,
    depth: u8,
    output: &mut Vec<[f32; 3]>,
    point_count: &mut usize,
    node_id: &str,
) -> Result<(), String> {
    if control_hull_outside_view_depth(&[start, control, end], near, far)
        || point_line_distance_squared_3d(control, start, end)
            <= PATH_3D_FLATNESS * PATH_3D_FLATNESS
    {
        return push_flattened_point(output, end, point_count, node_id);
    }
    let start_control = lerp_3d(start, control, 0.5);
    let control_end = lerp_3d(control, end, 0.5);
    let midpoint = lerp_3d(start_control, control_end, 0.5);
    if depth >= PATH_3D_MAX_SUBDIVISION_DEPTH {
        push_flattened_point(output, midpoint, point_count, node_id)?;
        return push_flattened_point(output, end, point_count, node_id);
    }
    flatten_quad_3d(
        start,
        start_control,
        midpoint,
        near,
        far,
        depth + 1,
        output,
        point_count,
        node_id,
    )?;
    flatten_quad_3d(
        midpoint,
        control_end,
        end,
        near,
        far,
        depth + 1,
        output,
        point_count,
        node_id,
    )
}

#[allow(clippy::too_many_arguments)]
fn flatten_cubic_3d(
    start: [f32; 3],
    control_1: [f32; 3],
    control_2: [f32; 3],
    end: [f32; 3],
    near: f32,
    far: f32,
    depth: u8,
    output: &mut Vec<[f32; 3]>,
    point_count: &mut usize,
    node_id: &str,
) -> Result<(), String> {
    let flatness = point_line_distance_squared_3d(control_1, start, end)
        .max(point_line_distance_squared_3d(control_2, start, end));
    if control_hull_outside_view_depth(&[start, control_1, control_2, end], near, far)
        || flatness <= PATH_3D_FLATNESS * PATH_3D_FLATNESS
    {
        return push_flattened_point(output, end, point_count, node_id);
    }
    let start_control = lerp_3d(start, control_1, 0.5);
    let controls = lerp_3d(control_1, control_2, 0.5);
    let control_end = lerp_3d(control_2, end, 0.5);
    let left_control = lerp_3d(start_control, controls, 0.5);
    let right_control = lerp_3d(controls, control_end, 0.5);
    let midpoint = lerp_3d(left_control, right_control, 0.5);
    if depth >= PATH_3D_MAX_SUBDIVISION_DEPTH {
        push_flattened_point(output, midpoint, point_count, node_id)?;
        return push_flattened_point(output, end, point_count, node_id);
    }
    flatten_cubic_3d(
        start,
        start_control,
        left_control,
        midpoint,
        near,
        far,
        depth + 1,
        output,
        point_count,
        node_id,
    )?;
    flatten_cubic_3d(
        midpoint,
        right_control,
        control_end,
        end,
        near,
        far,
        depth + 1,
        output,
        point_count,
        node_id,
    )
}

fn control_hull_outside_view_depth(points: &[[f32; 3]], near: f32, far: f32) -> bool {
    points.iter().all(|point| point[2] < near) || points.iter().all(|point| point[2] > far)
}

fn point_line_distance_squared_3d(
    point: [f32; 3],
    line_start: [f32; 3],
    line_end: [f32; 3],
) -> f32 {
    let line = sub_3d(line_end, line_start);
    let length_squared = dot_3d(line, line);
    if length_squared <= f32::EPSILON {
        return dot_3d(sub_3d(point, line_start), sub_3d(point, line_start));
    }
    let offset = sub_3d(point, line_start);
    dot_3d(cross_3d(offset, line), cross_3d(offset, line)) / length_squared
}

fn append_clipped_open_path(
    output: &mut Vec<PathCommand>,
    points: &[[f32; 3]],
    projection: PathProjection3d,
) {
    let mut last_projected = None;
    for pair in points.windows(2) {
        let Some((from, to)) =
            clip_segment_to_view_depth(pair[0], pair[1], projection.near, projection.far)
        else {
            last_projected = None;
            continue;
        };
        if dot_3d(sub_3d(to, from), sub_3d(to, from)) <= f32::EPSILON {
            continue;
        }
        let from = projection.project(from);
        let to = projection.project(to);
        if !last_projected.is_some_and(|last| points_2d_nearly_equal(last, from)) {
            output.push(PathCommand::MoveTo {
                x: from[0],
                y: from[1],
            });
        }
        output.push(PathCommand::LineTo { x: to[0], y: to[1] });
        last_projected = Some(to);
    }
}

fn append_clipped_closed_path(
    output: &mut Vec<PathCommand>,
    points: &[[f32; 3]],
    projection: PathProjection3d,
) {
    let clipped = clip_polygon_to_view_depth(
        points.to_vec(),
        projection.near,
        projection.far,
        |point| point[2],
        |from, to, amount| lerp_3d(*from, *to, amount),
    );
    let mut projected = Vec::with_capacity(clipped.len());
    for point in clipped {
        let point = projection.project(point);
        if !projected
            .last()
            .is_some_and(|previous| points_2d_nearly_equal(*previous, point))
        {
            projected.push(point);
        }
    }
    if projected.len() > 1
        && points_2d_nearly_equal(projected[0], *projected.last().expect("non-empty path"))
    {
        projected.pop();
    }
    if projected.len() < 3 {
        return;
    }
    output.push(PathCommand::MoveTo {
        x: projected[0][0],
        y: projected[0][1],
    });
    output.extend(projected.iter().skip(1).map(|point| PathCommand::LineTo {
        x: point[0],
        y: point[1],
    }));
    output.push(PathCommand::Close);
}

fn clip_segment_to_view_depth(
    from: [f32; 3],
    to: [f32; 3],
    near: f32,
    far: f32,
) -> Option<([f32; 3], [f32; 3])> {
    let depth_delta = to[2] - from[2];
    if depth_delta.abs() <= f32::EPSILON {
        return (near..=far).contains(&from[2]).then_some((from, to));
    }
    let near_amount = (near - from[2]) / depth_delta;
    let far_amount = (far - from[2]) / depth_delta;
    let start = near_amount.min(far_amount).max(0.0);
    let end = near_amount.max(far_amount).min(1.0);
    (start <= end).then(|| (lerp_3d(from, to, start), lerp_3d(from, to, end)))
}

fn points_2d_nearly_equal(left: [f32; 2], right: [f32; 2]) -> bool {
    let scale = left
        .iter()
        .chain(&right)
        .fold(1.0_f32, |scale, value| scale.max(value.abs()));
    (left[0] - right[0]).abs() <= 1e-5 * scale && (left[1] - right[1]).abs() <= 1e-5 * scale
}

fn transform_point_3d(point: [f32; 3], transform: Transform) -> [f32; 3] {
    let mut point = [
        point[0] * transform.scale_x,
        point[1] * transform.scale_y,
        point[2] * transform.scale_z,
    ];
    let (sin_x, cos_x) = transform.rotation_x.sin_cos();
    point = [
        point[0],
        point[1] * cos_x - point[2] * sin_x,
        point[1] * sin_x + point[2] * cos_x,
    ];
    let (sin_y, cos_y) = transform.rotation_y.sin_cos();
    point = [
        point[0] * cos_y + point[2] * sin_y,
        point[1],
        -point[0] * sin_y + point[2] * cos_y,
    ];
    let (sin_z, cos_z) = transform.rotation.sin_cos();
    [
        point[0] * cos_z - point[1] * sin_z + transform.x,
        point[0] * sin_z + point[1] * cos_z + transform.y,
        point[2] + transform.z,
    ]
}

fn transform_normal_3d(normal: [f32; 3], transform: Transform) -> Result<[f32; 3], &'static str> {
    let scales = [transform.scale_x, transform.scale_y, transform.scale_z];
    if scales
        .iter()
        .any(|scale| *scale == 0.0 || !scale.recip().is_finite())
    {
        return Err(
            "the 3D scale is singular because a scale component is zero or non-invertible.",
        );
    }
    let mut normal = [
        normal[0] / transform.scale_x,
        normal[1] / transform.scale_y,
        normal[2] / transform.scale_z,
    ];
    let (sin_x, cos_x) = transform.rotation_x.sin_cos();
    normal = [
        normal[0],
        normal[1] * cos_x - normal[2] * sin_x,
        normal[1] * sin_x + normal[2] * cos_x,
    ];
    let (sin_y, cos_y) = transform.rotation_y.sin_cos();
    normal = [
        normal[0] * cos_y + normal[2] * sin_y,
        normal[1],
        -normal[0] * sin_y + normal[2] * cos_y,
    ];
    let (sin_z, cos_z) = transform.rotation.sin_cos();
    Ok(normalize_3d([
        normal[0] * cos_z - normal[1] * sin_z,
        normal[0] * sin_z + normal[1] * cos_z,
        normal[2],
    ]))
}

fn add_3d(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    [left[0] + right[0], left[1] + right[1], left[2] + right[2]]
}

fn sub_3d(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    [left[0] - right[0], left[1] - right[1], left[2] - right[2]]
}

fn mul_3d(value: [f32; 3], scalar: f32) -> [f32; 3] {
    [value[0] * scalar, value[1] * scalar, value[2] * scalar]
}

fn dot_3d(left: [f32; 3], right: [f32; 3]) -> f32 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

fn cross_3d(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    [
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    ]
}

fn normalize_3d(value: [f32; 3]) -> [f32; 3] {
    let length = dot_3d(value, value).sqrt();
    if length <= f32::EPSILON {
        [0.0; 3]
    } else {
        mul_3d(value, 1.0 / length)
    }
}

fn view_depth_to_clip(near: f32, far: f32, view_z: f32) -> f32 {
    (far / (far - near) - far * near / ((far - near) * view_z)).clamp(0.0, 1.0)
}

fn clip_to_view_depth(near: f32, far: f32, clip_z: f32) -> f32 {
    let scale = far / (far - near);
    far * near / ((far - near) * (scale - clip_z))
}

fn vector_triangle_view_depth(
    frame: &EvaluatedFrameView<'_>,
    buffers: &VertexBuffers<Vertex, u32>,
    triangle_start: u32,
) -> f32 {
    let indices = &buffers.indices[triangle_start as usize..triangle_start as usize + 3];
    indices
        .iter()
        .map(|index| {
            clip_to_view_depth(
                frame.camera_3d.near,
                frame.camera_3d.far,
                buffers.vertices[*index as usize].position[2],
            )
        })
        .sum::<f32>()
        / 3.0
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
        let clip = to_clip(self.frame, world);
        Vertex {
            position: [clip[0], clip[1], 0.0],
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
    use realtime_manim_scene_core::{Scene, Transform};
    use realtime_manim_text_engine::TextEngine;

    use super::{
        DrawCommand, build_dashed_path, build_geometry, cross_3d, dot_3d, normalize_3d,
        partial_path, polyline_path, sub_3d, transform_normal_3d, transform_point_3d,
    };

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

    fn text_geometry(scene_json: &str) -> super::Geometry {
        let scene = Scene::from_json(scene_json).expect("text scene");
        let frame = scene.evaluate_view(0.0).expect("text frame");
        let mut text = TextEngine::new().expect("text engine");
        build_geometry(&frame, &mut text).expect("text geometry")
    }

    fn position_signature(geometry: &super::Geometry) -> Vec<[u32; 3]> {
        geometry
            .vertices
            .iter()
            .map(|vertex| vertex.position.map(f32::to_bits))
            .collect()
    }

    #[test]
    fn markup_color_spans_preserve_joined_arabic_geometry() {
        let joined = text_geometry(
            r##"{
              "version":2,"title":"joined","width":8,"height":4,"duration":1,
              "background":"#070910","nodes":[
                {"id":"joined","type":"text","text":"سلام","fontSize":1.5,
                 "style":{"fill":"#58c4ddff","stroke":null}}
              ]
            }"##,
        );
        let same_color = text_geometry(
            r##"{
              "version":2,"title":"same color spans","width":8,"height":4,"duration":1,
              "background":"#070910","nodes":[
                {"id":"markup","type":"markupText","fontSize":1.5,"spans":[
                  {"text":"س","color":"#58c4ddff"},{"text":"ل","color":"#58c4ddff"},
                  {"text":"ا","color":"#58c4ddff"},{"text":"م","color":"#58c4ddff"}
                ],"style":{"fill":"#58c4ddff","stroke":null}}
              ]
            }"##,
        );
        let colored = text_geometry(
            r##"{
              "version":2,"title":"colored spans","width":8,"height":4,"duration":1,
              "background":"#070910","nodes":[
                {"id":"markup","type":"markupText","fontSize":1.5,"spans":[
                  {"text":"س","color":"#fc6255ff"},{"text":"ل","color":"#58c4ddff"},
                  {"text":"ا","color":"#fc6255ff"},{"text":"م","color":"#58c4ddff"}
                ],"style":{"fill":"#ffffffff","stroke":null}}
              ]
            }"##,
        );

        assert_eq!(same_color.indices, joined.indices);
        assert_eq!(position_signature(&same_color), position_signature(&joined));
        assert_eq!(colored.indices, joined.indices);
        assert_eq!(position_signature(&colored), position_signature(&joined));
        assert!(colored.vertices.iter().any(|vertex| vertex.color[0] > 0.9));
        assert!(
            colored
                .vertices
                .iter()
                .any(|vertex| vertex.color[2] > 0.8 && vertex.color[0] < 0.5)
        );
    }

    #[test]
    fn native_markup_colors_follow_rtl_visual_order() {
        let geometry = text_geometry(
            r##"{
              "version":2,"title":"RTL span order","width":8,"height":4,"duration":1,
              "background":"#070910","nodes":[
                {"id":"hebrew","type":"markupText","fontSize":1.5,"spans":[
                  {"text":"א","color":"#fc6255ff"},{"text":"ב","color":"#83c167ff"},
                  {"text":"ג","color":"#58c4ddff"}
                ],"style":{"fill":"#ffffffff","stroke":null}}
              ]
            }"##,
        );
        let mean_x = |target: [f32; 3]| {
            let matching = geometry
                .vertices
                .iter()
                .filter(|vertex| {
                    (vertex.color[0] - target[0]).abs() < 0.01
                        && (vertex.color[1] - target[1]).abs() < 0.01
                        && (vertex.color[2] - target[2]).abs() < 0.01
                })
                .map(|vertex| vertex.position[0])
                .collect::<Vec<_>>();
            assert!(!matching.is_empty());
            matching.iter().sum::<f32>() / matching.len() as f32
        };
        let red = mean_x([252.0 / 255.0, 98.0 / 255.0, 85.0 / 255.0]);
        let green = mean_x([131.0 / 255.0, 193.0 / 255.0, 103.0 / 255.0]);
        let blue = mean_x([88.0 / 255.0, 196.0 / 255.0, 221.0 / 255.0]);
        assert!(
            blue < green && green < red,
            "RTL means: {blue} {green} {red}"
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

    #[test]
    fn media_3d_proof_builds_every_strict_native_draw_path() {
        let scene = Scene::from_json(include_str!("../media-3d-proof.json")).unwrap();
        let frame = scene.evaluate_view(2.0).unwrap();
        let mut text = TextEngine::new().unwrap();
        let geometry = build_geometry(&frame, &mut text).unwrap();
        assert_eq!(geometry.visible_nodes, 5);
        assert_eq!(geometry.rendered_nodes, 5);
        assert!(geometry.unsupported.is_empty());
        assert_eq!(geometry.image_vertices.len(), 6);
        assert_eq!(geometry.mesh_texture_vertices.len(), 6);
        assert!(geometry.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Vector {
                depth_test: true,
                ..
            }
        )));
        assert!(
            geometry
                .commands
                .iter()
                .any(|command| matches!(command, DrawCommand::Image { .. }))
        );
        assert!(
            geometry
                .commands
                .iter()
                .any(|command| matches!(command, DrawCommand::MeshTexture { .. }))
        );
        assert!(geometry.vertices.iter().all(|vertex| {
            vertex
                .position
                .iter()
                .all(|coordinate| coordinate.is_finite())
                && (0.0..=1.0).contains(&vertex.position[2])
        }));
    }

    #[test]
    fn mesh_near_plane_is_clipped_instead_of_dropped() {
        let scene = Scene::from_json(
            r##"{
              "version":2,"title":"native near clip","width":16,"height":9,"duration":1,
              "background":"#000000",
              "camera3d":{"position":[0,0,0],"target":[0,0,1],"up":[0,1,0],"fovY":0.9,"near":1,"far":10,"ambient":1},
              "nodes":[{
                "id":"crossing","type":"mesh",
                "vertices":[[-0.6,-0.6,0.5],[1.25,-0.6,2],[0,1.35,2]],
                "triangles":[[0,1,2]],"colors":["#ff0000","#00ff00","#0000ff"],
                "unlit":true,"doubleSided":true,"style":{"fill":"#ffffff","stroke":null}
              }]
            }"##,
        )
        .unwrap();
        let frame = scene.evaluate_view(0.0).unwrap();
        let mut text = TextEngine::new().unwrap();
        let geometry = build_geometry(&frame, &mut text).unwrap();
        assert_eq!(geometry.indices.len(), 6);
        assert!(geometry.vertices.iter().all(|vertex| {
            vertex.position[2].is_finite() && (0.0..=1.0).contains(&vertex.position[2])
        }));
    }

    #[test]
    fn inverse_transpose_normal_stays_perpendicular_after_nonuniform_scale() {
        let transform = Transform {
            rotation: 0.31,
            rotation_x: -0.27,
            rotation_y: 0.43,
            scale_x: 2.0,
            scale_y: 0.75,
            scale_z: 0.4,
            ..Transform::default()
        };
        let tangent_1 = [1.0, 0.0, -1.0];
        let tangent_2 = [0.0, 1.0, -1.0];
        let local_normal = normalize_3d(cross_3d(tangent_1, tangent_2));
        let origin = transform_point_3d([0.0; 3], transform);
        let transformed_tangent_1 = sub_3d(transform_point_3d(tangent_1, transform), origin);
        let transformed_tangent_2 = sub_3d(transform_point_3d(tangent_2, transform), origin);
        let geometric_normal = normalize_3d(cross_3d(transformed_tangent_1, transformed_tangent_2));
        let transformed_normal = transform_normal_3d(local_normal, transform).unwrap();

        assert!(dot_3d(transformed_normal, transformed_tangent_1).abs() < 1e-5);
        assert!(dot_3d(transformed_normal, transformed_tangent_2).abs() < 1e-5);
        assert!(dot_3d(transformed_normal, geometric_normal) > 0.999_99);
    }

    #[test]
    fn singular_normal_scale_returns_an_exact_renderer_error() {
        let scene = Scene::from_json(
            r##"{
              "version":2,"title":"singular normal","width":8,"height":4,"duration":1,
              "background":"#000000",
              "nodes":[{
                "id":"singular","type":"mesh",
                "vertices":[[-1,-1,0],[1,-1,0],[0,1,0]],"triangles":[[0,1,2]],
                "normals":[[0,0,1],[0,0,1],[0,0,1]],
                "transform":{"scaleY":0},
                "style":{"fill":"#ffffff","stroke":null}
              }]
            }"##,
        )
        .unwrap();
        let frame = scene.evaluate_view(0.0).unwrap();
        let mut text = TextEngine::new().unwrap();
        let error = build_geometry(&frame, &mut text).unwrap_err();
        assert_eq!(
            error,
            "Mesh singular normal transform failed: the 3D scale is singular because a scale component is zero or non-invertible."
        );
    }

    #[test]
    fn path3d_line_is_clipped_at_both_depth_planes() {
        let scene = Scene::from_json(
            r##"{
              "version":2,"title":"3D line clipping","width":16,"height":9,"duration":1,
              "background":"#000000",
              "camera3d":{"position":[0,0,0],"target":[0,0,1],"up":[0,1,0],"fovY":0.9,"near":1,"far":3},
              "nodes":[{
                "id":"crossing","type":"path3d","commands":[
                  {"op":"moveTo","x":-1,"y":0,"z":0.5},
                  {"op":"lineTo","x":1,"y":0,"z":4}
                ],"style":{"fill":null,"stroke":"#ffffff","strokeWidth":0.08}
              }]
            }"##,
        )
        .unwrap();
        let frame = scene.evaluate_view(0.0).unwrap();
        let mut text = TextEngine::new().unwrap();
        let geometry = build_geometry(&frame, &mut text).unwrap();
        assert_eq!(geometry.rendered_nodes, 1);
        assert!(geometry.triangle_count() > 0);
        assert!(
            geometry
                .vertices
                .iter()
                .all(|vertex| { vertex.position[0].is_finite() && vertex.position[1].is_finite() })
        );
    }

    #[test]
    fn path3d_cubic_remains_visible_when_both_endpoints_are_behind_near_plane() {
        let scene = Scene::from_json(
            r##"{
              "version":2,"title":"3D cubic clipping","width":16,"height":9,"duration":1,
              "background":"#000000",
              "camera3d":{"position":[0,0,0],"target":[0,0,1],"up":[0,1,0],"fovY":0.9,"near":1,"far":10},
              "nodes":[{
                "id":"curved-crossing","type":"path3d","commands":[
                  {"op":"moveTo","x":-1,"y":0,"z":0.5},
                  {"op":"cubicTo","c1x":-0.6,"c1y":1,"c1z":3,"c2x":0.6,"c2y":1,"c2z":3,"x":1,"y":0,"z":0.5}
                ],"style":{"fill":null,"stroke":"#38bdf8","strokeWidth":0.08}
              }]
            }"##,
        )
        .unwrap();
        let frame = scene.evaluate_view(0.0).unwrap();
        let mut text = TextEngine::new().unwrap();
        let geometry = build_geometry(&frame, &mut text).unwrap();
        assert_eq!(geometry.rendered_nodes, 1);
        assert!(geometry.triangle_count() > 8);
        assert!(
            geometry
                .vertices
                .iter()
                .all(|vertex| { vertex.position[0].is_finite() && vertex.position[1].is_finite() })
        );
    }

    #[test]
    fn transparent_triangles_are_sorted_far_to_near_across_nodes() {
        let scene = Scene::from_json(
            r##"{
              "version":2,"title":"global transparency","width":8,"height":4,"duration":1,
              "background":"#000000",
              "camera3d":{"position":[0,0,0],"target":[0,0,1],"up":[0,1,0],"near":1,"far":10},
              "nodes":[
                {"id":"near-first","type":"mesh","vertices":[[-1,-1,2],[0,1,2],[1,-1,2]],"triangles":[[0,1,2]],"colors":["#0000ffff","#0000ffff","#0000ffff"],"unlit":true,"doubleSided":true,"style":{"fill":"#ffffff","stroke":null,"opacity":0.5}},
                {"id":"far-second","type":"mesh","vertices":[[-3,-3,6],[0,3,6],[3,-3,6]],"triangles":[[0,1,2]],"colors":["#ff0000ff","#ff0000ff","#ff0000ff"],"unlit":true,"doubleSided":true,"style":{"fill":"#ffffff","stroke":null,"opacity":0.5}}
              ]
            }"##,
        )
        .unwrap();
        let frame = scene.evaluate_view(0.0).unwrap();
        let mut text = TextEngine::new().unwrap();
        let geometry = build_geometry(&frame, &mut text).unwrap();
        assert_eq!(geometry.transparent_primitives.len(), 2);
        assert_eq!(geometry.transparent_primitives[0].command_index, 1);
        assert_eq!(geometry.transparent_primitives[1].command_index, 0);
        assert!(geometry.transparent_primitives[0].depth > 5.99);
        assert!(geometry.transparent_primitives[1].depth < 2.01);
    }

    #[test]
    fn single_sided_mesh_culls_back_faces() {
        let scene = Scene::from_json(
            r##"{
              "version":2,"title":"native culling","width":8,"height":4,"duration":1,
              "background":"#000000",
              "camera3d":{"position":[0,0,5],"target":[0,0,0],"up":[0,1,0],"ambient":1},
              "nodes":[
                {"id":"front","type":"mesh","vertices":[[-1,-1,0],[1,-1,0],[0,1,0]],"triangles":[[0,1,2]],"unlit":true,"style":{"fill":"#ffffff","stroke":null}},
                {"id":"back","type":"mesh","vertices":[[-1,-1,0],[1,-1,0],[0,1,0]],"triangles":[[2,1,0]],"unlit":true,"style":{"fill":"#ffffff","stroke":null}}
              ]
            }"##,
        )
        .unwrap();
        let frame = scene.evaluate_view(0.0).unwrap();
        let mut text = TextEngine::new().unwrap();
        let geometry = build_geometry(&frame, &mut text).unwrap();
        assert_eq!(geometry.indices.len(), 3);
    }

    #[test]
    fn single_sided_textured_mesh_culls_back_faces() {
        let scene = Scene::from_json(
            r##"{
              "version":2,"title":"native textured culling","width":8,"height":4,"duration":1,
              "background":"#000000",
              "camera3d":{"position":[0,0,5],"target":[0,0,0],"up":[0,1,0],"ambient":1},
              "nodes":[
                {"id":"front","type":"mesh","vertices":[[-1,-1,0],[1,-1,0],[0,1,0]],"triangles":[[0,1,2]],"uvs":[[0,0],[1,0],[0.5,1]],"texturePixels":"/wAA/w==","textureWidth":1,"textureHeight":1,"textureResampling":"nearest","unlit":true,"style":{"fill":"#ffffff","stroke":null}},
                {"id":"back","type":"mesh","vertices":[[-1,-1,0],[1,-1,0],[0,1,0]],"triangles":[[2,1,0]],"uvs":[[0,0],[1,0],[0.5,1]],"texturePixels":"AAD//w==","textureWidth":1,"textureHeight":1,"textureResampling":"nearest","unlit":true,"style":{"fill":"#ffffff","stroke":null}}
              ]
            }"##,
        )
        .unwrap();
        let frame = scene.evaluate_view(0.0).unwrap();
        let mut text = TextEngine::new().unwrap();
        let geometry = build_geometry(&frame, &mut text).unwrap();
        assert_eq!(geometry.mesh_texture_vertices.len(), 3);
    }
}
