//! General retained Rust scene evaluator with a batched WebGPU vector renderer.
//!
//! JavaScript owns chat and DOM controls. Rust owns scene validation, explicit-time
//! evaluation, geometry generation, animation timing, GPU submission, and presentation.

#[cfg(any(target_arch = "wasm32", test))]
mod geometry_3d {
    use realtime_manim_scene_core::{
        EvaluatedFrameView, EvaluatedNodeView, PathCommand, PathCommand3d, Transform,
    };

    const PATH_3D_FLATNESS: f32 = 0.001;
    const PATH_3D_MAX_SUBDIVISION_DEPTH: u8 = 12;
    const PATH_3D_MAX_FLATTENED_POINTS: usize = 500_000;

    pub(super) fn sort_back_to_front_by_depth<T>(items: &mut [T], depth: impl Fn(&T) -> f32) {
        items.sort_by(|left, right| depth(right).total_cmp(&depth(left)));
    }

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

    pub(super) fn project_path_3d(
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
        let to_view =
            |point| projection.world_to_view(transform_point_3d(point, node.transform_3d));
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
                    let _ = current
                        .ok_or_else(|| "Path lineTo requires a preceding moveTo.".to_owned())?;
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
                    let start = current
                        .ok_or_else(|| "Path quadTo requires a preceding moveTo.".to_owned())?;
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
                "Path3d {node_id} exceeds the browser flattening limit of {PATH_3D_MAX_FLATTENED_POINTS} points."
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
                let amount = ((boundary - previous_depth) / (current_depth - previous_depth))
                    .clamp(0.0, 1.0);
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

    pub(super) fn transform_normal_3d(
        normal: [f32; 3],
        transform: Transform,
    ) -> Result<[f32; 3], &'static str> {
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

    fn lerp_3d(from: [f32; 3], to: [f32; 3], amount: f32) -> [f32; 3] {
        [
            from[0] + (to[0] - from[0]) * amount,
            from[1] + (to[1] - from[1]) * amount,
            from[2] + (to[2] - from[2]) * amount,
        ]
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
        let length = dot_3d(value, value).sqrt().max(f32::EPSILON);
        mul_3d(value, 1.0 / length)
    }

    #[cfg(test)]
    mod tests {
        use realtime_manim_scene_core::{NodeKind, Scene, Transform};

        use super::{
            cross_3d, dot_3d, normalize_3d, project_path_3d, sub_3d, transform_normal_3d,
            transform_point_3d,
        };

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
            let geometric_normal =
                normalize_3d(cross_3d(transformed_tangent_1, transformed_tangent_2));
            let transformed_normal = transform_normal_3d(local_normal, transform).unwrap();

            assert!(dot_3d(transformed_normal, transformed_tangent_1).abs() < 1e-5);
            assert!(dot_3d(transformed_normal, transformed_tangent_2).abs() < 1e-5);
            assert!(dot_3d(transformed_normal, geometric_normal) > 0.999_99);
        }

        #[test]
        fn singular_normal_scale_returns_exact_renderer_error() {
            let error = transform_normal_3d(
                [0.0, 0.0, 1.0],
                Transform {
                    scale_y: 0.0,
                    ..Transform::default()
                },
            )
            .unwrap_err();
            assert_eq!(
                error,
                "the 3D scale is singular because a scale component is zero or non-invertible."
            );
        }

        #[test]
        fn path3d_line_is_clipped_at_both_depth_planes() {
            let scene = Scene::from_json(
                r##"{
                  "version":2,"title":"browser 3D line clipping","width":16,"height":9,"duration":1,
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
            let node = &frame.nodes[0];
            let NodeKind::Path3d { commands } = node.kind.as_ref() else {
                panic!("expected Path3d");
            };
            let projected = project_path_3d(&frame, node, commands).unwrap();
            assert_eq!(projected.len(), 2);
            assert!(projected.iter().all(|command| match command {
                realtime_manim_scene_core::PathCommand::MoveTo { x, y }
                | realtime_manim_scene_core::PathCommand::LineTo { x, y } => {
                    x.is_finite() && y.is_finite()
                }
                _ => false,
            }));
        }

        #[test]
        fn path3d_cubic_stays_visible_with_both_endpoints_behind_near_plane() {
            let scene = Scene::from_json(
                r##"{
                  "version":2,"title":"browser 3D cubic clipping","width":16,"height":9,"duration":1,
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
            let node = &frame.nodes[0];
            let NodeKind::Path3d { commands } = node.kind.as_ref() else {
                panic!("expected Path3d");
            };
            let projected = project_path_3d(&frame, node, commands).unwrap();
            assert!(projected.len() > 8);
            assert!(matches!(
                projected.first(),
                Some(realtime_manim_scene_core::PathCommand::MoveTo { .. })
            ));
            assert!(projected.iter().all(|command| match command {
                realtime_manim_scene_core::PathCommand::MoveTo { x, y }
                | realtime_manim_scene_core::PathCommand::LineTo { x, y } => {
                    x.is_finite() && y.is_finite()
                }
                _ => false,
            }));
        }

        #[test]
        fn transparent_triangles_sort_globally_across_retained_nodes() {
            let mut triangles = [(2.0_f32, "near-first"), (6.0, "far-second")];
            super::sort_back_to_front_by_depth(&mut triangles, |triangle| triangle.0);
            assert_eq!(triangles[0].1, "far-second");
            assert_eq!(triangles[1].1, "near-first");
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod web {
    use super::geometry_3d::{project_path_3d, sort_back_to_front_by_depth, transform_normal_3d};
    use std::cell::{Cell, RefCell};
    use std::collections::HashMap;
    use std::f32::consts::TAU;
    use std::mem;
    use std::ops::Range;
    use std::rc::{Rc, Weak};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
    use bytemuck::{Pod, Zeroable};
    use lyon::{
        algorithms::measure::{PathMeasurements, SampleType},
        math::point,
        path::{Event as PathEvent, Path},
        tessellation::{
            BuffersBuilder, FillOptions, FillTessellator, FillVertex, FillVertexConstructor,
            StrokeOptions, StrokeTessellator, StrokeVertex, StrokeVertexConstructor, VertexBuffers,
        },
    };
    use realtime_manim_scene_core::{
        Camera, EvaluatedFrameView, EvaluatedLinearGradient, EvaluatedNodeView, FontSlant,
        FontWeight, GradientSpace, GradientSpread, ImageResampling, NodeKind, PathCommand, Scene,
        ShaderAttribute, ShaderPrimitive, ShaderUniform, ShaderUniformType, ShaderVertexFormat,
        StrokeCap, StrokeJoin, TextAlign, TextSpan, TextUnderline, Transform, parse_color,
    };
    use realtime_manim_svg_engine::{
        SvgClip, SvgElementRef, SvgEngine, SvgFillRule, SvgGradientSpread, SvgImageResampling,
        SvgLineCap, SvgLineJoin, SvgMask, SvgMaskType, SvgPaint, SvgPattern, SvgRadialGradient,
        SvgRasterImage,
    };
    use realtime_manim_text_engine::{
        AttributedTextSpan, FontSelection, FontVariant, MarkupSpan, PangoUnderline,
        TextAlign as ShapedTextAlign, TextEngine, parse_pango_markup,
    };
    use wasm_bindgen::{JsCast, prelude::*};
    use wasm_bindgen_futures::spawn_local;
    use web_sys::HtmlCanvasElement;

    type AnimationCallback = Closure<dyn FnMut(f64)>;

    thread_local! {
        static ENGINE: RefCell<Option<Engine>> = const { RefCell::new(None) };
        static ANIMATION_FRAME: RefCell<Option<AnimationCallback>> = const {
            RefCell::new(None)
        };
        static ANIMATION_FRAME_ID: RefCell<Option<i32>> = const { RefCell::new(None) };
    }

    static INITIALIZATION_IN_PROGRESS: AtomicBool = AtomicBool::new(false);
    static LIFECYCLE_GENERATION: AtomicU32 = AtomicU32::new(0);
    static DEVICE_LOST: AtomicBool = AtomicBool::new(false);
    static RECOVERY_IN_PROGRESS: AtomicBool = AtomicBool::new(false);
    static RECOVERY_COUNT: AtomicU32 = AtomicU32::new(0);

    struct PlayerInner {
        engine: RefCell<Option<Engine>>,
        animation_frame: RefCell<Option<AnimationCallback>>,
        animation_frame_id: Cell<Option<i32>>,
        destroyed: Cell<bool>,
        device_lost: Arc<AtomicBool>,
        recovery_in_progress: Cell<bool>,
        recovery_count: Cell<u32>,
        lifecycle_generation: Cell<u32>,
    }

    /// An independent WebGPU renderer bound to one canvas.
    ///
    /// Each handle owns its engine and animation-frame loop. Dropping JavaScript's
    /// wrapper is not deterministic, so callers should invoke `destroy()`.
    #[wasm_bindgen]
    pub struct WebPlayer {
        inner: Rc<PlayerInner>,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Pod, Zeroable)]
    struct Vertex {
        position: [f32; 3],
        color: [f32; 4],
        gradient_position: [f32; 2],
        gradient_meta: [f32; 4],
        mask_meta: [f32; 2],
    }

    const VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![
        0 => Float32x3,
        1 => Float32x4,
        2 => Float32x2,
        3 => Float32x4,
        4 => Float32x2
    ];
    const IMAGE_VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 4] =
        wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32, 3 => Float32x2];
    const MESH_TEXTURE_VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 9] = wgpu::vertex_attr_array![
        0 => Float32x3,
        1 => Float32x2,
        2 => Float32,
        3 => Float32x3,
        4 => Float32x3,
        5 => Float32x3,
        6 => Float32,
        7 => Float32,
        8 => Float32
    ];
    const SAMPLE_COUNT: u32 = 4;
    const DEPTH_STENCIL_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24PlusStencil8;
    const MAX_REGISTERED_FONT_FACES: usize = 64;
    const MAX_REGISTERED_FONT_BYTES: usize = 128 * 1024 * 1024;

    #[repr(C)]
    #[derive(Clone, Copy, Pod, Zeroable)]
    struct GpuGradientStop {
        color: [f32; 4],
        data: [f32; 4],
    }

    #[repr(C)]
    #[derive(Clone, Copy, Pod, Zeroable)]
    struct ImageVertex {
        position: [f32; 2],
        uv: [f32; 2],
        opacity: f32,
        mask_meta: [f32; 2],
    }

    #[repr(C)]
    #[derive(Clone, Copy, Pod, Zeroable)]
    struct MeshTextureVertex {
        position: [f32; 3],
        uv: [f32; 2],
        opacity: f32,
        point: [f32; 3],
        normal: [f32; 3],
        light_position: [f32; 3],
        gloss: f32,
        shadow: f32,
        has_dark_texture: f32,
    }

    struct GpuImage {
        _texture: wgpu::Texture,
        _dark_texture: wgpu::Texture,
        opaque: bool,
        nearest_bind_group: wgpu::BindGroup,
        linear_bind_group: wgpu::BindGroup,
        bicubic_bind_group: wgpu::BindGroup,
    }

    struct GpuCustomShader {
        pipeline: wgpu::RenderPipeline,
        bind_group: wgpu::BindGroup,
        vertex_buffer: wgpu::Buffer,
        vertex_capacity: usize,
        index_buffer: wgpu::Buffer,
        index_count: u32,
        vertex_count: u32,
        uniform_buffers: Vec<(String, wgpu::Buffer)>,
        _textures: Vec<wgpu::Texture>,
        _samplers: Vec<wgpu::Sampler>,
    }

    struct GpuMaskTargets {
        texture: wgpu::Texture,
        _array_view: wgpu::TextureView,
        _msaa_texture: wgpu::Texture,
        msaa_view: wgpu::TextureView,
        _resolve_texture: wgpu::Texture,
        resolve_view: wgpu::TextureView,
        _depth_texture: wgpu::Texture,
        depth_view: wgpu::TextureView,
        bind_group: wgpu::BindGroup,
        width: u32,
        height: u32,
        layers: u32,
    }

    enum DrawCommand {
        Vector {
            indices: Range<u32>,
            depth_test: bool,
            transparent_3d: bool,
        },
        ClipPush {
            indices: Range<u32>,
            reference: u32,
        },
        ClipPop {
            indices: Range<u32>,
            reference: u32,
        },
        ClippedVector {
            indices: Range<u32>,
            reference: u32,
        },
        MaskedVector {
            indices: Range<u32>,
            reference: u32,
        },
        Image {
            vertices: Range<u32>,
            key: String,
            resampling: ImageResampling,
            reference: u32,
            masked: bool,
        },
        MeshTexture {
            vertices: Range<u32>,
            key: String,
            resampling: ImageResampling,
            depth_test: bool,
        },
        CustomShader {
            key: String,
        },
    }

    enum MaskDrawCommand {
        ClipPush {
            indices: Range<u32>,
            reference: u32,
        },
        ClipPop {
            indices: Range<u32>,
            reference: u32,
        },
        Vector {
            indices: Range<u32>,
            reference: u32,
        },
        MaskedVector {
            indices: Range<u32>,
            reference: u32,
        },
        Image {
            vertices: Range<u32>,
            key: String,
            resampling: ImageResampling,
            reference: u32,
            masked: bool,
        },
    }

    trait SvgCommandSink {
        fn clip_push(&mut self, indices: Range<u32>, reference: u32);
        fn clip_pop(&mut self, indices: Range<u32>, reference: u32);
        fn vector(&mut self, indices: Range<u32>, reference: u32, masked: bool);
        fn image(
            &mut self,
            vertices: Range<u32>,
            key: String,
            resampling: ImageResampling,
            reference: u32,
            masked: bool,
        );
    }

    impl SvgCommandSink for Vec<DrawCommand> {
        fn clip_push(&mut self, indices: Range<u32>, reference: u32) {
            self.push(DrawCommand::ClipPush { indices, reference });
        }

        fn clip_pop(&mut self, indices: Range<u32>, reference: u32) {
            self.push(DrawCommand::ClipPop { indices, reference });
        }

        fn vector(&mut self, indices: Range<u32>, reference: u32, masked: bool) {
            self.push(if masked {
                DrawCommand::MaskedVector { indices, reference }
            } else {
                DrawCommand::ClippedVector { indices, reference }
            });
        }

        fn image(
            &mut self,
            vertices: Range<u32>,
            key: String,
            resampling: ImageResampling,
            reference: u32,
            masked: bool,
        ) {
            self.push(DrawCommand::Image {
                vertices,
                key,
                resampling,
                reference,
                masked,
            });
        }
    }

    impl SvgCommandSink for Vec<MaskDrawCommand> {
        fn clip_push(&mut self, indices: Range<u32>, reference: u32) {
            self.push(MaskDrawCommand::ClipPush { indices, reference });
        }

        fn clip_pop(&mut self, indices: Range<u32>, reference: u32) {
            self.push(MaskDrawCommand::ClipPop { indices, reference });
        }

        fn vector(&mut self, indices: Range<u32>, reference: u32, masked: bool) {
            self.push(if masked {
                MaskDrawCommand::MaskedVector { indices, reference }
            } else {
                MaskDrawCommand::Vector { indices, reference }
            });
        }

        fn image(
            &mut self,
            vertices: Range<u32>,
            key: String,
            resampling: ImageResampling,
            reference: u32,
            masked: bool,
        ) {
            self.push(MaskDrawCommand::Image {
                vertices,
                key,
                resampling,
                reference,
                masked,
            });
        }
    }

    struct MaskLayerGeometry {
        commands: Vec<MaskDrawCommand>,
    }

    struct TransparentPrimitive {
        depth: f32,
        command_index: usize,
        range: Range<u32>,
    }

    #[derive(Default)]
    struct FrameGeometry {
        vertices: Vec<Vertex>,
        indices: Vec<u32>,
        gradient_stops: Vec<GpuGradientStop>,
        image_vertices: Vec<ImageVertex>,
        mesh_texture_vertices: Vec<MeshTextureVertex>,
        commands: Vec<DrawCommand>,
        transparent_primitives: Vec<TransparentPrimitive>,
        mask_layers: Vec<MaskLayerGeometry>,
        mask_indices: Vec<u32>,
    }

    struct Engine {
        surface: wgpu::Surface<'static>,
        device: wgpu::Device,
        queue: wgpu::Queue,
        config: wgpu::SurfaceConfiguration,
        msaa_texture: wgpu::Texture,
        msaa_view: wgpu::TextureView,
        depth_texture: wgpu::Texture,
        depth_view: wgpu::TextureView,
        pipeline: wgpu::RenderPipeline,
        mesh_pipeline: wgpu::RenderPipeline,
        mesh_transparent_pipeline: wgpu::RenderPipeline,
        clipped_pipeline: wgpu::RenderPipeline,
        masked_pipeline: wgpu::RenderPipeline,
        masked_clipped_pipeline: wgpu::RenderPipeline,
        clip_push_pipeline: wgpu::RenderPipeline,
        clip_pop_pipeline: wgpu::RenderPipeline,
        gradient_bind_group_layout: wgpu::BindGroupLayout,
        gradient_stop_buffer: wgpu::Buffer,
        gradient_bind_group: wgpu::BindGroup,
        gradient_stop_capacity: usize,
        mask_bind_group_layout: wgpu::BindGroupLayout,
        mask_index_buffer: wgpu::Buffer,
        mask_index_capacity: usize,
        mask_targets: Option<GpuMaskTargets>,
        image_sample_pipeline: wgpu::RenderPipeline,
        image_box_pipeline: wgpu::RenderPipeline,
        image_hamming_pipeline: wgpu::RenderPipeline,
        image_bicubic_pipeline: wgpu::RenderPipeline,
        image_lanczos_pipeline: wgpu::RenderPipeline,
        image_clipped_pipeline: wgpu::RenderPipeline,
        image_masked_pipeline: wgpu::RenderPipeline,
        image_masked_clipped_pipeline: wgpu::RenderPipeline,
        mesh_texture_sample_pipeline: wgpu::RenderPipeline,
        mesh_texture_box_pipeline: wgpu::RenderPipeline,
        mesh_texture_hamming_pipeline: wgpu::RenderPipeline,
        mesh_texture_bicubic_pipeline: wgpu::RenderPipeline,
        mesh_texture_lanczos_pipeline: wgpu::RenderPipeline,
        mesh_texture_depth_sample_pipeline: wgpu::RenderPipeline,
        mesh_texture_depth_box_pipeline: wgpu::RenderPipeline,
        mesh_texture_depth_hamming_pipeline: wgpu::RenderPipeline,
        mesh_texture_depth_bicubic_pipeline: wgpu::RenderPipeline,
        mesh_texture_depth_lanczos_pipeline: wgpu::RenderPipeline,
        image_bind_group_layout: wgpu::BindGroupLayout,
        nearest_sampler: wgpu::Sampler,
        linear_sampler: wgpu::Sampler,
        image_cache: HashMap<String, GpuImage>,
        custom_shader_cache: HashMap<String, GpuCustomShader>,
        vertex_buffer: wgpu::Buffer,
        index_buffer: wgpu::Buffer,
        image_vertex_buffer: wgpu::Buffer,
        mesh_texture_vertex_buffer: wgpu::Buffer,
        vertex_capacity: usize,
        index_capacity: usize,
        image_vertex_capacity: usize,
        mesh_texture_vertex_capacity: usize,
        frame_geometry: FrameGeometry,
        canvas: HtmlCanvasElement,
        render_size_override: Option<(u32, u32)>,
        scene: Scene,
        signal_overrides: HashMap<String, f32>,
        svg_engine: SvgEngine,
        text_engine: TextEngine,
        registered_fonts: Vec<RegisteredFontFace>,
        paused: bool,
        manual_time: Option<f32>,
        pause_started_ms: f64,
        paused_total_ms: f64,
        first_frame_ms: Option<f64>,
        last_scene_time: f32,
        last_sample_ms: f64,
        frames_in_sample: u32,
    }

    struct RecoveryState {
        canvas: HtmlCanvasElement,
        scene: Scene,
        signal_overrides: HashMap<String, f32>,
        render_size_override: Option<(u32, u32)>,
        scene_time: f32,
        manual_time: Option<f32>,
        paused: bool,
        registered_fonts: Vec<RegisteredFontFace>,
    }

    #[derive(Clone)]
    struct RegisteredFontFace {
        family: String,
        variant: FontVariant,
        data: Arc<[u8]>,
    }

    impl Engine {
        async fn new(
            canvas: HtmlCanvasElement,
            player_device_lost: Option<Arc<AtomicBool>>,
        ) -> Result<Self, JsValue> {
            let width = canvas.width().max(1);
            let height = canvas.height().max(1);
            let mut instance_descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
            instance_descriptor.backends = wgpu::Backends::BROWSER_WEBGPU;
            let instance = wgpu::Instance::new(instance_descriptor);
            let surface = instance
                .create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone()))
                .map_err(|error| {
                    js_error(format!("Could not create the WebGPU canvas: {error}"))
                })?;
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    compatible_surface: Some(&surface),
                    force_fallback_adapter: false,
                    apply_limit_buckets: false,
                })
                .await
                .map_err(|error| {
                    js_error(format!(
                        "No WebGPU adapter is available in this browser: {error}"
                    ))
                })?;
            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor {
                    label: Some("realtime-manim general vector device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: wgpu::MemoryHints::Performance,
                    trace: wgpu::Trace::Off,
                    experimental_features: wgpu::ExperimentalFeatures::disabled(),
                })
                .await
                .map_err(|error| js_error(format!("WebGPU device creation failed: {error}")))?;
            device.set_device_lost_callback(move |_reason, _message| {
                if let Some(device_lost) = player_device_lost.as_ref() {
                    device_lost.store(true, Ordering::Release);
                } else {
                    DEVICE_LOST.store(true, Ordering::Release);
                }
            });

            let mut config = surface
                .get_default_config(&adapter, width, height)
                .ok_or_else(|| js_error("The WebGPU surface has no compatible format."))?;
            config.present_mode = wgpu::PresentMode::Fifo;
            surface.configure(&device, &config);
            let (msaa_texture, msaa_view) = create_msaa_target(&device, &config);
            let (depth_texture, depth_view) = create_depth_target(&device, &config);

            let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));
            let gradient_bind_group_layout =
                device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("realtime-manim vector gradient bind group layout"),
                    entries: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    }],
                });
            let mask_bind_group_layout =
                device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("realtime-manim SVG mask bind group layout"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                                view_dimension: wgpu::TextureViewDimension::D2Array,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                    ],
                });
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("realtime-manim general vector pipeline layout"),
                bind_group_layouts: &[Some(&gradient_bind_group_layout)],
                immediate_size: 0,
            });
            let masked_pipeline_layout =
                device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("realtime-manim masked vector pipeline layout"),
                    bind_group_layouts: &[
                        Some(&gradient_bind_group_layout),
                        Some(&mask_bind_group_layout),
                    ],
                    immediate_size: 0,
                });
            let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("realtime-manim general vector pipeline"),
                layout: Some(&pipeline_layout),
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
                        format: config.format,
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
                depth_stencil: Some(pass_depth_state(false)),
                multisample: wgpu::MultisampleState {
                    count: SAMPLE_COUNT,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                multiview_mask: None,
                cache: None,
            });
            let mesh_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("realtime-manim depth-tested mesh pipeline"),
                layout: Some(&pipeline_layout),
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
                        format: config.format,
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
                depth_stencil: Some(pass_depth_state(true)),
                multisample: wgpu::MultisampleState {
                    count: SAMPLE_COUNT,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                multiview_mask: None,
                cache: None,
            });
            let mesh_transparent_pipeline = create_vector_fragment_pipeline(
                &device,
                &shader,
                &pipeline_layout,
                config.format,
                "realtime-manim transparent depth-tested mesh pipeline",
                "fs_main",
                transparent_depth_state(),
            );
            let clipped_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("realtime-manim stencil-clipped vector pipeline"),
                layout: Some(&pipeline_layout),
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
                        format: config.format,
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
                depth_stencil: Some(clipped_depth_state()),
                multisample: wgpu::MultisampleState {
                    count: SAMPLE_COUNT,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                multiview_mask: None,
                cache: None,
            });
            let create_clip_pipeline = |label, operation| {
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(label),
                    layout: Some(&pipeline_layout),
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
                        entry_point: Some("fs_clip"),
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: config.format,
                            blend: None,
                            write_mask: wgpu::ColorWrites::empty(),
                        })],
                    }),
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::TriangleList,
                        front_face: wgpu::FrontFace::Ccw,
                        cull_mode: None,
                        ..Default::default()
                    },
                    depth_stencil: Some(clip_depth_state(operation)),
                    multisample: wgpu::MultisampleState {
                        count: SAMPLE_COUNT,
                        mask: !0,
                        alpha_to_coverage_enabled: false,
                    },
                    multiview_mask: None,
                    cache: None,
                })
            };
            let clip_push_pipeline = create_clip_pipeline(
                "realtime-manim SVG clip push pipeline",
                wgpu::StencilOperation::IncrementClamp,
            );
            let clip_pop_pipeline = create_clip_pipeline(
                "realtime-manim SVG clip pop pipeline",
                wgpu::StencilOperation::DecrementClamp,
            );
            let masked_pipeline = create_vector_fragment_pipeline(
                &device,
                &shader,
                &masked_pipeline_layout,
                config.format,
                "realtime-manim SVG masked vector pipeline",
                "fs_masked",
                pass_depth_state(false),
            );
            let masked_clipped_pipeline = create_vector_fragment_pipeline(
                &device,
                &shader,
                &masked_pipeline_layout,
                config.format,
                "realtime-manim SVG masked and clipped vector pipeline",
                "fs_masked",
                clipped_depth_state(),
            );
            let image_bind_group_layout =
                device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("realtime-manim image bind group layout"),
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
                    label: Some("realtime-manim image pipeline layout"),
                    bind_group_layouts: &[Some(&image_bind_group_layout)],
                    immediate_size: 0,
                });
            let masked_image_pipeline_layout =
                device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("realtime-manim masked image pipeline layout"),
                    bind_group_layouts: &[
                        Some(&image_bind_group_layout),
                        Some(&mask_bind_group_layout),
                    ],
                    immediate_size: 0,
                });
            let image_sample_pipeline = create_image_pipeline(
                &device,
                &config,
                &shader,
                &image_pipeline_layout,
                "fs_image_sample",
                "realtime-manim sampled image pipeline",
                pass_depth_state(false),
            );
            let image_box_pipeline = create_image_pipeline(
                &device,
                &config,
                &shader,
                &image_pipeline_layout,
                "fs_image_box",
                "realtime-manim box image pipeline",
                pass_depth_state(false),
            );
            let image_hamming_pipeline = create_image_pipeline(
                &device,
                &config,
                &shader,
                &image_pipeline_layout,
                "fs_image_hamming",
                "realtime-manim hamming image pipeline",
                pass_depth_state(false),
            );
            let image_bicubic_pipeline = create_image_pipeline(
                &device,
                &config,
                &shader,
                &image_pipeline_layout,
                "fs_image_bicubic",
                "realtime-manim bicubic image pipeline",
                pass_depth_state(false),
            );
            let image_lanczos_pipeline = create_image_pipeline(
                &device,
                &config,
                &shader,
                &image_pipeline_layout,
                "fs_image_lanczos",
                "realtime-manim lanczos image pipeline",
                pass_depth_state(false),
            );
            let image_clipped_pipeline = create_image_pipeline(
                &device,
                &config,
                &shader,
                &image_pipeline_layout,
                "fs_image_sample",
                "realtime-manim clipped image pipeline",
                clipped_depth_state(),
            );
            let image_masked_pipeline = create_image_pipeline(
                &device,
                &config,
                &shader,
                &masked_image_pipeline_layout,
                "fs_image_sample_masked",
                "realtime-manim masked image pipeline",
                pass_depth_state(false),
            );
            let image_masked_clipped_pipeline = create_image_pipeline(
                &device,
                &config,
                &shader,
                &masked_image_pipeline_layout,
                "fs_image_sample_masked",
                "realtime-manim masked and clipped image pipeline",
                clipped_depth_state(),
            );
            let mesh_texture_sample_pipeline = create_mesh_texture_pipeline(
                &device,
                &config,
                &shader,
                &image_pipeline_layout,
                "fs_mesh_texture_sample",
                "realtime-manim sampled textured mesh pipeline",
                false,
            );
            let mesh_texture_box_pipeline = create_mesh_texture_pipeline(
                &device,
                &config,
                &shader,
                &image_pipeline_layout,
                "fs_mesh_texture_box",
                "realtime-manim box textured mesh pipeline",
                false,
            );
            let mesh_texture_hamming_pipeline = create_mesh_texture_pipeline(
                &device,
                &config,
                &shader,
                &image_pipeline_layout,
                "fs_mesh_texture_hamming",
                "realtime-manim hamming textured mesh pipeline",
                false,
            );
            let mesh_texture_bicubic_pipeline = create_mesh_texture_pipeline(
                &device,
                &config,
                &shader,
                &image_pipeline_layout,
                "fs_mesh_texture_bicubic",
                "realtime-manim bicubic textured mesh pipeline",
                false,
            );
            let mesh_texture_lanczos_pipeline = create_mesh_texture_pipeline(
                &device,
                &config,
                &shader,
                &image_pipeline_layout,
                "fs_mesh_texture_lanczos",
                "realtime-manim lanczos textured mesh pipeline",
                false,
            );
            let mesh_texture_depth_sample_pipeline = create_mesh_texture_pipeline(
                &device,
                &config,
                &shader,
                &image_pipeline_layout,
                "fs_mesh_texture_sample",
                "realtime-manim depth-tested sampled textured mesh pipeline",
                true,
            );
            let mesh_texture_depth_box_pipeline = create_mesh_texture_pipeline(
                &device,
                &config,
                &shader,
                &image_pipeline_layout,
                "fs_mesh_texture_box",
                "realtime-manim depth-tested box textured mesh pipeline",
                true,
            );
            let mesh_texture_depth_hamming_pipeline = create_mesh_texture_pipeline(
                &device,
                &config,
                &shader,
                &image_pipeline_layout,
                "fs_mesh_texture_hamming",
                "realtime-manim depth-tested hamming textured mesh pipeline",
                true,
            );
            let mesh_texture_depth_bicubic_pipeline = create_mesh_texture_pipeline(
                &device,
                &config,
                &shader,
                &image_pipeline_layout,
                "fs_mesh_texture_bicubic",
                "realtime-manim depth-tested bicubic textured mesh pipeline",
                true,
            );
            let mesh_texture_depth_lanczos_pipeline = create_mesh_texture_pipeline(
                &device,
                &config,
                &shader,
                &image_pipeline_layout,
                "fs_mesh_texture_lanczos",
                "realtime-manim depth-tested lanczos textured mesh pipeline",
                true,
            );
            let nearest_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("realtime-manim nearest image sampler"),
                mag_filter: wgpu::FilterMode::Nearest,
                min_filter: wgpu::FilterMode::Nearest,
                mipmap_filter: wgpu::MipmapFilterMode::Nearest,
                ..Default::default()
            });
            let linear_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("realtime-manim linear image sampler"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::MipmapFilterMode::Linear,
                ..Default::default()
            });

            let vertex_capacity = 16_384;
            let index_capacity = 32_768;
            let image_vertex_capacity = 384;
            let mesh_texture_vertex_capacity = 384;
            let vertex_buffer = create_buffer(
                &device,
                "realtime-manim vertices",
                vertex_capacity * mem::size_of::<Vertex>(),
                wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            );
            let index_buffer = create_buffer(
                &device,
                "realtime-manim indices",
                index_capacity * mem::size_of::<u32>(),
                wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            );
            let image_vertex_buffer = create_buffer(
                &device,
                "realtime-manim image vertices",
                image_vertex_capacity * mem::size_of::<ImageVertex>(),
                wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            );
            let mesh_texture_vertex_buffer = create_buffer(
                &device,
                "realtime-manim textured mesh vertices",
                mesh_texture_vertex_capacity * mem::size_of::<MeshTextureVertex>(),
                wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            );
            let gradient_stop_capacity = 128;
            let gradient_stop_buffer = create_buffer(
                &device,
                "realtime-manim gradient stops",
                gradient_stop_capacity * mem::size_of::<GpuGradientStop>(),
                wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            );
            let gradient_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("realtime-manim vector gradient bind group"),
                layout: &gradient_bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: gradient_stop_buffer.as_entire_binding(),
                }],
            });
            let mask_index_capacity = 64;
            let mask_index_buffer = create_buffer(
                &device,
                "realtime-manim SVG mask layer indices",
                mask_index_capacity * mem::size_of::<u32>(),
                wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            );
            Ok(Self {
                surface,
                device,
                queue,
                config,
                msaa_texture,
                msaa_view,
                depth_texture,
                depth_view,
                pipeline,
                mesh_pipeline,
                mesh_transparent_pipeline,
                clipped_pipeline,
                masked_pipeline,
                masked_clipped_pipeline,
                clip_push_pipeline,
                clip_pop_pipeline,
                gradient_bind_group_layout,
                gradient_stop_buffer,
                gradient_bind_group,
                gradient_stop_capacity,
                mask_bind_group_layout,
                mask_index_buffer,
                mask_index_capacity,
                mask_targets: None,
                image_sample_pipeline,
                image_box_pipeline,
                image_hamming_pipeline,
                image_bicubic_pipeline,
                image_lanczos_pipeline,
                image_clipped_pipeline,
                image_masked_pipeline,
                image_masked_clipped_pipeline,
                mesh_texture_sample_pipeline,
                mesh_texture_box_pipeline,
                mesh_texture_hamming_pipeline,
                mesh_texture_bicubic_pipeline,
                mesh_texture_lanczos_pipeline,
                mesh_texture_depth_sample_pipeline,
                mesh_texture_depth_box_pipeline,
                mesh_texture_depth_hamming_pipeline,
                mesh_texture_depth_bicubic_pipeline,
                mesh_texture_depth_lanczos_pipeline,
                image_bind_group_layout,
                nearest_sampler,
                linear_sampler,
                image_cache: HashMap::new(),
                custom_shader_cache: HashMap::new(),
                vertex_buffer,
                index_buffer,
                image_vertex_buffer,
                mesh_texture_vertex_buffer,
                vertex_capacity,
                index_capacity,
                image_vertex_capacity,
                mesh_texture_vertex_capacity,
                frame_geometry: FrameGeometry::default(),
                canvas,
                render_size_override: None,
                scene: Scene::from_json(DEFAULT_SCENE).map_err(js_error)?,
                signal_overrides: HashMap::new(),
                svg_engine: SvgEngine::new(),
                text_engine: TextEngine::new().map_err(|error| js_error(error.to_string()))?,
                registered_fonts: Vec::new(),
                paused: false,
                manual_time: None,
                pause_started_ms: 0.0,
                paused_total_ms: 0.0,
                first_frame_ms: None,
                last_scene_time: 0.0,
                last_sample_ms: 0.0,
                frames_in_sample: 0,
            })
        }

        fn load_scene(&mut self, mut scene: Scene, now_ms: f64) -> Result<(), JsValue> {
            for node in &mut scene.nodes {
                if let NodeKind::MarkupText {
                    spans,
                    markup: Some(markup),
                    ..
                } = &mut node.kind
                {
                    let parsed = parse_pango_markup(markup).map_err(|error| {
                        js_error(format!(
                            "Pango markup parsing failed for {}: {error}",
                            node.id
                        ))
                    })?;
                    *spans = parsed.iter().map(scene_text_span).collect();
                    if let NodeKind::MarkupText { markup, .. } = &mut node.kind {
                        *markup = None;
                    }
                }
            }
            for node in &scene.nodes {
                match &node.kind {
                    NodeKind::Text {
                        text,
                        font_size,
                        font_family,
                        align,
                        weight,
                        slant,
                    } => {
                        if !self.text_engine.has_family(font_family) {
                            return Err(js_error(format!(
                                "Node {} selects unregistered font family {font_family}.",
                                node.id
                            )));
                        }
                        let align = match align {
                            TextAlign::Left => ShapedTextAlign::Left,
                            TextAlign::Center => ShapedTextAlign::Center,
                            TextAlign::Right => ShapedTextAlign::Right,
                        };
                        self.text_engine
                            .layout_family_variant(
                                text,
                                *font_size,
                                align,
                                1.25,
                                0.0,
                                FontSelection {
                                    family: font_family,
                                    variant: font_variant(*weight, *slant),
                                },
                            )
                            .map_err(|error| {
                                js_error(format!("Text shaping failed for {}: {error}", node.id))
                            })?;
                    }
                    NodeKind::MarkupText {
                        spans,
                        font_size,
                        font_family,
                        align,
                        ..
                    } => {
                        if !self.text_engine.has_family(font_family) {
                            return Err(js_error(format!(
                                "Node {} selects unregistered font family {font_family}.",
                                node.id
                            )));
                        }
                        let shaped_spans = spans
                            .iter()
                            .map(|span| AttributedTextSpan {
                                text: &span.text,
                                variant: font_variant(span.weight, span.slant),
                                font_family: span.font_family.as_deref(),
                                font_scale: span.font_scale,
                                rise: span.rise,
                                letter_spacing: span.letter_spacing,
                            })
                            .collect::<Vec<_>>();
                        let align = match align {
                            TextAlign::Left => ShapedTextAlign::Left,
                            TextAlign::Center => ShapedTextAlign::Center,
                            TextAlign::Right => ShapedTextAlign::Right,
                        };
                        self.text_engine
                            .layout_family_chain_attributed_spans(
                                &shaped_spans,
                                *font_size,
                                align,
                                1.25,
                                0.0,
                                font_family,
                                &[],
                            )
                            .map_err(|error| {
                                js_error(format!("Markup shaping failed for {}: {error}", node.id))
                            })?;
                    }
                    _ => {}
                }
            }
            self.scene = scene;
            self.signal_overrides.clear();
            self.image_cache.clear();
            self.custom_shader_cache.clear();
            self.paused = false;
            self.manual_time = None;
            self.last_scene_time = 0.0;
            self.reset_clock(now_ms);
            Ok(())
        }

        fn register_font(
            &mut self,
            family: &str,
            variant: FontVariant,
            data: Vec<u8>,
        ) -> Result<(), JsValue> {
            let normalized_family = family.trim();
            let existing_index = self.registered_fonts.iter().position(|face| {
                face.variant == variant && face.family.eq_ignore_ascii_case(normalized_family)
            });
            if existing_index.is_none() && self.registered_fonts.len() >= MAX_REGISTERED_FONT_FACES
            {
                return Err(js_error("A player can register at most 64 font faces."));
            }
            let current_bytes = self
                .registered_fonts
                .iter()
                .try_fold(0usize, |total, face| total.checked_add(face.data.len()))
                .ok_or_else(|| js_error("Registered font bytes overflowed the supported range."))?;
            let replaced_bytes = existing_index
                .map(|index| self.registered_fonts[index].data.len())
                .unwrap_or(0);
            let next_bytes = current_bytes
                .checked_sub(replaced_bytes)
                .and_then(|total| total.checked_add(data.len()))
                .ok_or_else(|| js_error("Registered font bytes overflowed the supported range."))?;
            if next_bytes > MAX_REGISTERED_FONT_BYTES {
                return Err(js_error(
                    "A player can register at most 128 MiB of font data.",
                ));
            }
            let data: Arc<[u8]> = Arc::from(data);
            self.text_engine
                .register_font_shared(family, variant, Arc::clone(&data))
                .map_err(|error| js_error(error.to_string()))?;
            if let Some(index) = existing_index {
                let face = &mut self.registered_fonts[index];
                face.family = normalized_family.to_owned();
                face.data = data;
            } else {
                self.registered_fonts.push(RegisteredFontFace {
                    family: normalized_family.to_owned(),
                    variant,
                    data,
                });
            }
            Ok(())
        }

        fn recovery_state(&self, now_ms: f64) -> RecoveryState {
            RecoveryState {
                canvas: self.canvas.clone(),
                scene: self.scene.clone(),
                signal_overrides: self.signal_overrides.clone(),
                render_size_override: self.render_size_override,
                scene_time: self.scene_time(now_ms),
                manual_time: self.manual_time,
                paused: self.paused,
                registered_fonts: self.registered_fonts.clone(),
            }
        }

        fn restore_state(&mut self, state: RecoveryState, now_ms: f64) -> Result<(), JsValue> {
            for face in &state.registered_fonts {
                self.text_engine
                    .register_font_shared(&face.family, face.variant, Arc::clone(&face.data))
                    .map_err(|error| js_error(error.to_string()))?;
            }
            self.scene = state.scene;
            self.signal_overrides = state.signal_overrides;
            self.render_size_override = state.render_size_override;
            self.registered_fonts = state.registered_fonts;
            self.manual_time = state.manual_time;
            self.last_scene_time = state.scene_time;
            self.first_frame_ms = Some(now_ms - f64::from(state.scene_time) * 1000.0);
            self.pause_started_ms = now_ms;
            self.paused_total_ms = 0.0;
            self.paused = state.paused;
            self.resize_if_needed();
            Ok(())
        }

        fn set_paused(&mut self, paused: bool, now_ms: f64) {
            if paused == self.paused {
                return;
            }
            if paused {
                self.pause_started_ms = now_ms;
            } else {
                self.paused_total_ms += now_ms - self.pause_started_ms;
            }
            self.paused = paused;
        }

        fn reset_clock(&mut self, now_ms: f64) {
            self.first_frame_ms = Some(now_ms);
            self.pause_started_ms = now_ms;
            self.paused_total_ms = 0.0;
            self.last_scene_time = 0.0;
        }

        fn scene_time(&self, now_ms: f64) -> f32 {
            if let Some(manual_time) = self.manual_time {
                return manual_time;
            }
            let Some(first_frame_ms) = self.first_frame_ms else {
                return 0.0;
            };
            let effective_now_ms = if self.paused {
                self.pause_started_ms
            } else {
                now_ms
            };
            let elapsed =
                ((effective_now_ms - first_frame_ms - self.paused_total_ms) / 1000.0) as f32;
            elapsed.rem_euclid(self.scene.duration)
        }

        fn resume_from_current_time(&mut self, now_ms: f64) {
            let scene_time = self.scene_time(now_ms);
            self.manual_time = None;
            self.paused = false;
            self.first_frame_ms = Some(now_ms - f64::from(scene_time) * 1000.0);
            self.pause_started_ms = now_ms;
            self.paused_total_ms = 0.0;
            self.last_scene_time = scene_time;
        }

        fn resize_if_needed(&mut self) {
            let (width, height) = self.render_size_override.unwrap_or_else(|| {
                let ratio = device_pixel_ratio();
                (
                    ((self.canvas.client_width() as f64 * f64::from(ratio)).round() as u32).max(1),
                    ((self.canvas.client_height() as f64 * f64::from(ratio)).round() as u32).max(1),
                )
            });
            if width == self.config.width && height == self.config.height {
                return;
            }
            self.canvas.set_width(width);
            self.canvas.set_height(height);
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
            (self.msaa_texture, self.msaa_view) = create_msaa_target(&self.device, &self.config);
            (self.depth_texture, self.depth_view) = create_depth_target(&self.device, &self.config);
        }

        fn render(&mut self, now_ms: f64) {
            self.resize_if_needed();
            self.first_frame_ms.get_or_insert(now_ms);
            let scene_time = self.scene_time(now_ms);
            self.last_scene_time = scene_time;
            let frame = match self
                .scene
                .evaluate_view_with_signal_overrides(scene_time, &self.signal_overrides)
            {
                Ok(frame) => frame,
                Err(error) => {
                    set_text("error-detail", &error);
                    show_error();
                    return;
                }
            };
            if let Err(error) = prepare_images(
                &frame,
                &self.device,
                &self.queue,
                &self.image_bind_group_layout,
                &self.nearest_sampler,
                &self.linear_sampler,
                &mut self.image_cache,
            ) {
                set_text("error-detail", &error);
                show_error();
                return;
            }
            if let Err(error) = prepare_svg_images(
                &frame,
                &mut self.svg_engine,
                &self.device,
                &self.queue,
                &self.image_bind_group_layout,
                &self.nearest_sampler,
                &self.linear_sampler,
                &mut self.image_cache,
            ) {
                set_text("error-detail", &error);
                show_error();
                return;
            }
            if let Err(error) = prepare_custom_shaders(
                &frame,
                &self.device,
                &self.queue,
                &self.config,
                &mut self.custom_shader_cache,
            ) {
                set_text("error-detail", &error);
                show_error();
                return;
            }
            let mut geometry = mem::take(&mut self.frame_geometry);
            if let Err(error) = build_geometry_into(
                &frame,
                &self.image_cache,
                &mut self.text_engine,
                &mut self.svg_engine,
                &mut geometry,
            ) {
                self.frame_geometry = geometry;
                set_text("error-detail", &error);
                show_error();
                return;
            }
            let background = frame.background;
            drop(frame);
            self.upload_geometry(
                &geometry.vertices,
                &geometry.indices,
                &geometry.gradient_stops,
                &geometry.image_vertices,
                &geometry.mesh_texture_vertices,
            );
            self.prepare_mask_targets(geometry.mask_layers.len(), &geometry.mask_indices);

            let surface_texture = match self.surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(texture)
                | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
                wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                    self.frame_geometry = geometry;
                    return;
                }
                wgpu::CurrentSurfaceTexture::Outdated
                | wgpu::CurrentSurfaceTexture::Lost
                | wgpu::CurrentSurfaceTexture::Validation => {
                    self.surface.configure(&self.device, &self.config);
                    self.frame_geometry = geometry;
                    return;
                }
            };
            let view = surface_texture
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("realtime-manim general frame encoder"),
                });
            if let Some(mask_targets) = self.mask_targets.as_ref() {
                for (layer_index, layer) in geometry.mask_layers.iter().enumerate() {
                    {
                        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("realtime-manim SVG vector mask pass"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: &mask_targets.msaa_view,
                                depth_slice: None,
                                resolve_target: Some(&mask_targets.resolve_view),
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                                    store: wgpu::StoreOp::Store,
                                },
                            })],
                            depth_stencil_attachment: Some(
                                wgpu::RenderPassDepthStencilAttachment {
                                    view: &mask_targets.depth_view,
                                    depth_ops: Some(wgpu::Operations {
                                        load: wgpu::LoadOp::Clear(1.0),
                                        store: wgpu::StoreOp::Store,
                                    }),
                                    stencil_ops: Some(wgpu::Operations {
                                        load: wgpu::LoadOp::Clear(0),
                                        store: wgpu::StoreOp::Store,
                                    }),
                                },
                            ),
                            ..Default::default()
                        });
                        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                        pass.set_bind_group(0, &self.gradient_bind_group, &[]);
                        pass.set_index_buffer(
                            self.index_buffer.slice(..),
                            wgpu::IndexFormat::Uint32,
                        );
                        for command in &layer.commands {
                            match command {
                                MaskDrawCommand::ClipPush { indices, reference } => {
                                    pass.set_pipeline(&self.clip_push_pipeline);
                                    pass.set_stencil_reference(*reference);
                                    pass.draw_indexed(indices.clone(), 0, 0..1);
                                }
                                MaskDrawCommand::ClipPop { indices, reference } => {
                                    pass.set_pipeline(&self.clip_pop_pipeline);
                                    pass.set_stencil_reference(*reference);
                                    pass.draw_indexed(indices.clone(), 0, 0..1);
                                }
                                MaskDrawCommand::Vector { indices, reference } => {
                                    pass.set_pipeline(if *reference == 0 {
                                        &self.pipeline
                                    } else {
                                        &self.clipped_pipeline
                                    });
                                    pass.set_stencil_reference(*reference);
                                    pass.draw_indexed(indices.clone(), 0, 0..1);
                                }
                                MaskDrawCommand::MaskedVector { indices, reference } => {
                                    pass.set_pipeline(if *reference == 0 {
                                        &self.masked_pipeline
                                    } else {
                                        &self.masked_clipped_pipeline
                                    });
                                    pass.set_bind_group(1, &mask_targets.bind_group, &[]);
                                    pass.set_stencil_reference(*reference);
                                    pass.draw_indexed(indices.clone(), 0, 0..1);
                                }
                                MaskDrawCommand::Image {
                                    vertices,
                                    key,
                                    resampling,
                                    reference,
                                    masked,
                                } => {
                                    let Some(image) = self.image_cache.get(key) else {
                                        continue;
                                    };
                                    let bind_group = match resampling {
                                        ImageResampling::Nearest => &image.nearest_bind_group,
                                        _ => &image.linear_bind_group,
                                    };
                                    let pipeline = if *masked {
                                        if *reference == 0 {
                                            &self.image_masked_pipeline
                                        } else {
                                            &self.image_masked_clipped_pipeline
                                        }
                                    } else if *reference > 0 {
                                        &self.image_clipped_pipeline
                                    } else {
                                        &self.image_sample_pipeline
                                    };
                                    pass.set_pipeline(pipeline);
                                    pass.set_stencil_reference(*reference);
                                    pass.set_bind_group(0, bind_group, &[]);
                                    if *masked {
                                        pass.set_bind_group(1, &mask_targets.bind_group, &[]);
                                    }
                                    pass.set_vertex_buffer(0, self.image_vertex_buffer.slice(..));
                                    pass.draw(vertices.clone(), 0..1);
                                    pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                                    pass.set_bind_group(0, &self.gradient_bind_group, &[]);
                                }
                            }
                        }
                    }
                    encoder.copy_texture_to_texture(
                        wgpu::TexelCopyTextureInfo {
                            texture: &mask_targets._resolve_texture,
                            mip_level: 0,
                            origin: wgpu::Origin3d::ZERO,
                            aspect: wgpu::TextureAspect::All,
                        },
                        wgpu::TexelCopyTextureInfo {
                            texture: &mask_targets.texture,
                            mip_level: 0,
                            origin: wgpu::Origin3d {
                                x: 0,
                                y: 0,
                                z: layer_index as u32,
                            },
                            aspect: wgpu::TextureAspect::All,
                        },
                        wgpu::Extent3d {
                            width: self.config.width,
                            height: self.config.height,
                            depth_or_array_layers: 1,
                        },
                    );
                }
            }
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("realtime-manim general vector pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &self.msaa_view,
                        depth_slice: None,
                        resolve_target: Some(&view),
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
                        view: &self.depth_view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(0),
                            store: wgpu::StoreOp::Store,
                        }),
                    }),
                    ..Default::default()
                });
                // Populate depth with every opaque 3D primitive before blending anything.
                for command in &geometry.commands {
                    match command {
                        DrawCommand::Vector {
                            indices,
                            depth_test: true,
                            ..
                        } => {
                            pass.set_pipeline(&self.mesh_pipeline);
                            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                            pass.set_bind_group(0, &self.gradient_bind_group, &[]);
                            pass.set_index_buffer(
                                self.index_buffer.slice(..),
                                wgpu::IndexFormat::Uint32,
                            );
                            pass.draw_indexed(indices.clone(), 0, 0..1);
                        }
                        DrawCommand::MeshTexture {
                            vertices,
                            key,
                            resampling,
                            depth_test: true,
                        } => {
                            let Some(image) = self.image_cache.get(key) else {
                                continue;
                            };
                            let (pipeline, bind_group) = match resampling {
                                ImageResampling::Nearest => (
                                    &self.mesh_texture_depth_sample_pipeline,
                                    &image.nearest_bind_group,
                                ),
                                ImageResampling::Box => (
                                    &self.mesh_texture_depth_box_pipeline,
                                    &image.bicubic_bind_group,
                                ),
                                ImageResampling::Bilinear => (
                                    &self.mesh_texture_depth_sample_pipeline,
                                    &image.linear_bind_group,
                                ),
                                ImageResampling::Hamming => (
                                    &self.mesh_texture_depth_hamming_pipeline,
                                    &image.bicubic_bind_group,
                                ),
                                ImageResampling::Bicubic => (
                                    &self.mesh_texture_depth_bicubic_pipeline,
                                    &image.bicubic_bind_group,
                                ),
                                ImageResampling::Lanczos => (
                                    &self.mesh_texture_depth_lanczos_pipeline,
                                    &image.bicubic_bind_group,
                                ),
                            };
                            pass.set_pipeline(pipeline);
                            pass.set_bind_group(0, bind_group, &[]);
                            pass.set_vertex_buffer(0, self.mesh_texture_vertex_buffer.slice(..));
                            pass.draw(vertices.clone(), 0..1);
                        }
                        _ => {}
                    }
                }

                // Blend all transparent 3D triangles globally far-to-near without writing depth.
                for primitive in &geometry.transparent_primitives {
                    let Some(command) = geometry.commands.get(primitive.command_index) else {
                        continue;
                    };
                    match command {
                        DrawCommand::Vector {
                            transparent_3d: true,
                            ..
                        } => {
                            pass.set_pipeline(&self.mesh_transparent_pipeline);
                            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                            pass.set_bind_group(0, &self.gradient_bind_group, &[]);
                            pass.set_index_buffer(
                                self.index_buffer.slice(..),
                                wgpu::IndexFormat::Uint32,
                            );
                            pass.draw_indexed(primitive.range.clone(), 0, 0..1);
                        }
                        DrawCommand::MeshTexture {
                            key,
                            resampling,
                            depth_test: false,
                            ..
                        } => {
                            let Some(image) = self.image_cache.get(key) else {
                                continue;
                            };
                            let (pipeline, bind_group) = match resampling {
                                ImageResampling::Nearest => (
                                    &self.mesh_texture_sample_pipeline,
                                    &image.nearest_bind_group,
                                ),
                                ImageResampling::Box => {
                                    (&self.mesh_texture_box_pipeline, &image.bicubic_bind_group)
                                }
                                ImageResampling::Bilinear => {
                                    (&self.mesh_texture_sample_pipeline, &image.linear_bind_group)
                                }
                                ImageResampling::Hamming => (
                                    &self.mesh_texture_hamming_pipeline,
                                    &image.bicubic_bind_group,
                                ),
                                ImageResampling::Bicubic => (
                                    &self.mesh_texture_bicubic_pipeline,
                                    &image.bicubic_bind_group,
                                ),
                                ImageResampling::Lanczos => (
                                    &self.mesh_texture_lanczos_pipeline,
                                    &image.bicubic_bind_group,
                                ),
                            };
                            pass.set_pipeline(pipeline);
                            pass.set_bind_group(0, bind_group, &[]);
                            pass.set_vertex_buffer(0, self.mesh_texture_vertex_buffer.slice(..));
                            pass.draw(primitive.range.clone(), 0..1);
                        }
                        _ => {}
                    }
                }

                // Screen-space vectors, images, SVG, and custom shaders remain ordered overlays.
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
                            pass.set_pipeline(&self.pipeline);
                            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                            pass.set_bind_group(0, &self.gradient_bind_group, &[]);
                            pass.set_index_buffer(
                                self.index_buffer.slice(..),
                                wgpu::IndexFormat::Uint32,
                            );
                            pass.draw_indexed(indices.clone(), 0, 0..1);
                        }
                        DrawCommand::ClipPush { indices, reference } => {
                            pass.set_pipeline(&self.clip_push_pipeline);
                            pass.set_stencil_reference(*reference);
                            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                            pass.set_bind_group(0, &self.gradient_bind_group, &[]);
                            pass.set_index_buffer(
                                self.index_buffer.slice(..),
                                wgpu::IndexFormat::Uint32,
                            );
                            pass.draw_indexed(indices.clone(), 0, 0..1);
                        }
                        DrawCommand::ClipPop { indices, reference } => {
                            pass.set_pipeline(&self.clip_pop_pipeline);
                            pass.set_stencil_reference(*reference);
                            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                            pass.set_bind_group(0, &self.gradient_bind_group, &[]);
                            pass.set_index_buffer(
                                self.index_buffer.slice(..),
                                wgpu::IndexFormat::Uint32,
                            );
                            pass.draw_indexed(indices.clone(), 0, 0..1);
                        }
                        DrawCommand::ClippedVector { indices, reference } => {
                            pass.set_pipeline(&self.clipped_pipeline);
                            pass.set_stencil_reference(*reference);
                            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                            pass.set_bind_group(0, &self.gradient_bind_group, &[]);
                            pass.set_index_buffer(
                                self.index_buffer.slice(..),
                                wgpu::IndexFormat::Uint32,
                            );
                            pass.draw_indexed(indices.clone(), 0, 0..1);
                        }
                        DrawCommand::MaskedVector { indices, reference } => {
                            let Some(mask_targets) = self.mask_targets.as_ref() else {
                                continue;
                            };
                            pass.set_pipeline(if *reference == 0 {
                                &self.masked_pipeline
                            } else {
                                &self.masked_clipped_pipeline
                            });
                            pass.set_stencil_reference(*reference);
                            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                            pass.set_bind_group(0, &self.gradient_bind_group, &[]);
                            pass.set_bind_group(1, &mask_targets.bind_group, &[]);
                            pass.set_index_buffer(
                                self.index_buffer.slice(..),
                                wgpu::IndexFormat::Uint32,
                            );
                            pass.draw_indexed(indices.clone(), 0, 0..1);
                        }
                        DrawCommand::Image {
                            vertices,
                            key,
                            resampling,
                            reference,
                            masked,
                        } => {
                            let Some(image) = self.image_cache.get(key) else {
                                continue;
                            };
                            let (regular_pipeline, bind_group) = match resampling {
                                ImageResampling::Nearest => {
                                    (&self.image_sample_pipeline, &image.nearest_bind_group)
                                }
                                ImageResampling::Box => {
                                    (&self.image_box_pipeline, &image.bicubic_bind_group)
                                }
                                ImageResampling::Bilinear => {
                                    (&self.image_sample_pipeline, &image.linear_bind_group)
                                }
                                ImageResampling::Hamming => {
                                    (&self.image_hamming_pipeline, &image.bicubic_bind_group)
                                }
                                ImageResampling::Bicubic => {
                                    (&self.image_bicubic_pipeline, &image.bicubic_bind_group)
                                }
                                ImageResampling::Lanczos => {
                                    (&self.image_lanczos_pipeline, &image.bicubic_bind_group)
                                }
                            };
                            let pipeline = if *masked {
                                if *reference == 0 {
                                    &self.image_masked_pipeline
                                } else {
                                    &self.image_masked_clipped_pipeline
                                }
                            } else if *reference > 0 {
                                &self.image_clipped_pipeline
                            } else {
                                regular_pipeline
                            };
                            pass.set_pipeline(pipeline);
                            pass.set_stencil_reference(*reference);
                            pass.set_bind_group(0, bind_group, &[]);
                            if *masked {
                                let Some(mask_targets) = self.mask_targets.as_ref() else {
                                    continue;
                                };
                                pass.set_bind_group(1, &mask_targets.bind_group, &[]);
                            }
                            pass.set_vertex_buffer(0, self.image_vertex_buffer.slice(..));
                            pass.draw(vertices.clone(), 0..1);
                        }
                        DrawCommand::MeshTexture { .. } => {
                            continue;
                        }
                        DrawCommand::CustomShader { key } => {
                            let Some(shader) = self.custom_shader_cache.get(key) else {
                                continue;
                            };
                            pass.set_pipeline(&shader.pipeline);
                            pass.set_bind_group(0, &shader.bind_group, &[]);
                            pass.set_vertex_buffer(0, shader.vertex_buffer.slice(..));
                            if shader.index_count > 0 {
                                pass.set_index_buffer(
                                    shader.index_buffer.slice(..),
                                    wgpu::IndexFormat::Uint32,
                                );
                                pass.draw_indexed(0..shader.index_count, 0, 0..1);
                            } else {
                                pass.draw(0..shader.vertex_count, 0..1);
                            }
                        }
                    }
                }
            }
            self.queue.submit(Some(encoder.finish()));
            self.queue.present(surface_texture);
            self.update_frame_metrics(now_ms);
            self.frame_geometry = geometry;
        }

        fn upload_geometry(
            &mut self,
            vertices: &[Vertex],
            indices: &[u32],
            gradient_stops: &[GpuGradientStop],
            image_vertices: &[ImageVertex],
            mesh_texture_vertices: &[MeshTextureVertex],
        ) {
            if vertices.len() > self.vertex_capacity {
                self.vertex_capacity = vertices.len().next_power_of_two();
                self.vertex_buffer = create_buffer(
                    &self.device,
                    "realtime-manim expanded vertices",
                    self.vertex_capacity * mem::size_of::<Vertex>(),
                    wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                );
            }
            if indices.len() > self.index_capacity {
                self.index_capacity = indices.len().next_power_of_two();
                self.index_buffer = create_buffer(
                    &self.device,
                    "realtime-manim expanded indices",
                    self.index_capacity * mem::size_of::<u32>(),
                    wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                );
            }
            if gradient_stops.len() > self.gradient_stop_capacity {
                self.gradient_stop_capacity = gradient_stops.len().next_power_of_two();
                self.gradient_stop_buffer = create_buffer(
                    &self.device,
                    "realtime-manim expanded gradient stops",
                    self.gradient_stop_capacity * mem::size_of::<GpuGradientStop>(),
                    wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                );
                self.gradient_bind_group =
                    self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("realtime-manim expanded vector gradient bind group"),
                        layout: &self.gradient_bind_group_layout,
                        entries: &[wgpu::BindGroupEntry {
                            binding: 0,
                            resource: self.gradient_stop_buffer.as_entire_binding(),
                        }],
                    });
            }
            if image_vertices.len() > self.image_vertex_capacity {
                self.image_vertex_capacity = image_vertices.len().next_power_of_two();
                self.image_vertex_buffer = create_buffer(
                    &self.device,
                    "realtime-manim expanded image vertices",
                    self.image_vertex_capacity * mem::size_of::<ImageVertex>(),
                    wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                );
            }
            if mesh_texture_vertices.len() > self.mesh_texture_vertex_capacity {
                self.mesh_texture_vertex_capacity = mesh_texture_vertices.len().next_power_of_two();
                self.mesh_texture_vertex_buffer = create_buffer(
                    &self.device,
                    "realtime-manim expanded textured mesh vertices",
                    self.mesh_texture_vertex_capacity * mem::size_of::<MeshTextureVertex>(),
                    wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                );
            }
            if !vertices.is_empty() {
                self.queue
                    .write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(vertices));
            }
            if !indices.is_empty() {
                self.queue
                    .write_buffer(&self.index_buffer, 0, bytemuck::cast_slice(indices));
            }
            if !gradient_stops.is_empty() {
                self.queue.write_buffer(
                    &self.gradient_stop_buffer,
                    0,
                    bytemuck::cast_slice(gradient_stops),
                );
            }
            if !image_vertices.is_empty() {
                self.queue.write_buffer(
                    &self.image_vertex_buffer,
                    0,
                    bytemuck::cast_slice(image_vertices),
                );
            }
            if !mesh_texture_vertices.is_empty() {
                self.queue.write_buffer(
                    &self.mesh_texture_vertex_buffer,
                    0,
                    bytemuck::cast_slice(mesh_texture_vertices),
                );
            }
        }

        fn prepare_mask_targets(&mut self, layer_count: usize, mask_indices: &[u32]) {
            if layer_count == 0 {
                self.mask_targets = None;
                return;
            }
            let mut buffer_changed = false;
            if mask_indices.len() > self.mask_index_capacity {
                self.mask_index_capacity = mask_indices.len().next_power_of_two();
                self.mask_index_buffer = create_buffer(
                    &self.device,
                    "realtime-manim expanded SVG mask layer indices",
                    self.mask_index_capacity * mem::size_of::<u32>(),
                    wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                );
                buffer_changed = true;
            }
            if !mask_indices.is_empty() {
                self.queue.write_buffer(
                    &self.mask_index_buffer,
                    0,
                    bytemuck::cast_slice(mask_indices),
                );
            }
            let layers = u32::try_from(layer_count).unwrap_or(u32::MAX);
            if buffer_changed
                || self.mask_targets.as_ref().is_none_or(|targets| {
                    targets.width != self.config.width
                        || targets.height != self.config.height
                        || targets.layers != layers
                })
            {
                self.mask_targets = Some(create_mask_targets(
                    &self.device,
                    &self.config,
                    &self.mask_bind_group_layout,
                    &self.mask_index_buffer,
                    layers,
                ));
            }
        }

        fn update_frame_metrics(&mut self, now_ms: f64) {
            self.frames_in_sample += 1;
            if self.last_sample_ms == 0.0 {
                self.last_sample_ms = now_ms;
                return;
            }
            let elapsed = now_ms - self.last_sample_ms;
            if elapsed < 500.0 {
                return;
            }
            let fps = f64::from(self.frames_in_sample) * 1000.0 / elapsed;
            let frame_ms = elapsed / f64::from(self.frames_in_sample);
            set_text("fps-value", &format!("{fps:.0}"));
            set_text("frame-time-value", &format!("{frame_ms:.1} ms"));
            self.frames_in_sample = 0;
            self.last_sample_ms = now_ms;
        }
    }

    fn create_buffer(
        device: &wgpu::Device,
        label: &str,
        size: usize,
        usage: wgpu::BufferUsages,
    ) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: size.max(4) as u64,
            usage,
            mapped_at_creation: false,
        })
    }

    fn create_image_pipeline(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
        shader: &wgpu::ShaderModule,
        layout: &wgpu::PipelineLayout,
        fragment_entry: &str,
        label: &str,
        depth_stencil: wgpu::DepthStencilState,
    ) -> wgpu::RenderPipeline {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_image"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: mem::size_of::<ImageVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &IMAGE_VERTEX_ATTRIBUTES,
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some(fragment_entry),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
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
            depth_stencil: Some(depth_stencil),
            multisample: wgpu::MultisampleState {
                count: SAMPLE_COUNT,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        })
    }

    fn create_mesh_texture_pipeline(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
        shader: &wgpu::ShaderModule,
        layout: &wgpu::PipelineLayout,
        fragment_entry: &str,
        label: &str,
        depth_test: bool,
    ) -> wgpu::RenderPipeline {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_mesh_texture"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: mem::size_of::<MeshTextureVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &MESH_TEXTURE_VERTEX_ATTRIBUTES,
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some(fragment_entry),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
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
            depth_stencil: Some(if depth_test {
                pass_depth_state(true)
            } else {
                transparent_depth_state()
            }),
            multisample: wgpu::MultisampleState {
                count: SAMPLE_COUNT,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        })
    }

    fn prepare_images(
        frame: &EvaluatedFrameView<'_>,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        nearest_sampler: &wgpu::Sampler,
        linear_sampler: &wgpu::Sampler,
        cache: &mut HashMap<String, GpuImage>,
    ) -> Result<(), String> {
        for node in &frame.nodes {
            let image = match node.kind.as_ref() {
                NodeKind::Image {
                    pixels,
                    pixel_width,
                    pixel_height,
                    ..
                } => Some((pixels, *pixel_width, *pixel_height, "", 0, 0)),
                NodeKind::Mesh {
                    texture_pixels,
                    texture_width,
                    texture_height,
                    dark_texture_pixels,
                    dark_texture_width,
                    dark_texture_height,
                    ..
                } if !texture_pixels.is_empty() => Some((
                    texture_pixels,
                    *texture_width,
                    *texture_height,
                    dark_texture_pixels.as_str(),
                    *dark_texture_width,
                    *dark_texture_height,
                )),
                _ => None,
            };
            let Some((
                pixels,
                pixel_width,
                pixel_height,
                dark_pixels,
                dark_pixel_width,
                dark_pixel_height,
            )) = image
            else {
                continue;
            };
            if cache.contains_key(node.id) {
                continue;
            }
            let decoded = BASE64
                .decode(pixels)
                .map_err(|_| format!("Image {} contains invalid base64.", node.id))?;
            let expected = pixel_width as usize * pixel_height as usize * 4;
            if decoded.len() != expected {
                return Err(format!(
                    "Image {} contains {} bytes; expected {expected}.",
                    node.id,
                    decoded.len()
                ));
            }
            let (dark_decoded, dark_pixel_width, dark_pixel_height) = if dark_pixels.is_empty() {
                (decoded.clone(), pixel_width, pixel_height)
            } else {
                let decoded = BASE64
                    .decode(dark_pixels)
                    .map_err(|_| format!("Image {} contains invalid dark base64.", node.id))?;
                let expected = dark_pixel_width as usize * dark_pixel_height as usize * 4;
                if decoded.len() != expected {
                    return Err(format!(
                        "Image {} contains {} dark bytes; expected {expected}.",
                        node.id,
                        decoded.len()
                    ));
                }
                (decoded, dark_pixel_width, dark_pixel_height)
            };
            insert_rgba_image(
                cache,
                node.id,
                &decoded,
                pixel_width,
                pixel_height,
                &dark_decoded,
                dark_pixel_width,
                dark_pixel_height,
                device,
                queue,
                layout,
                nearest_sampler,
                linear_sampler,
            );
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn insert_rgba_image(
        cache: &mut HashMap<String, GpuImage>,
        key: &str,
        pixels: &[u8],
        pixel_width: u32,
        pixel_height: u32,
        dark_pixels: &[u8],
        dark_pixel_width: u32,
        dark_pixel_height: u32,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        nearest_sampler: &wgpu::Sampler,
        linear_sampler: &wgpu::Sampler,
    ) {
        let upload = |label: &str, data: &[u8], width: u32, height: u32| {
            let size = wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            };
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(width.saturating_mul(4)),
                    rows_per_image: Some(height),
                },
                size,
            );
            texture
        };
        let texture = upload(
            "realtime-manim image texture",
            pixels,
            pixel_width,
            pixel_height,
        );
        let dark_texture = upload(
            "realtime-manim dark image texture",
            dark_pixels,
            dark_pixel_width,
            dark_pixel_height,
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let dark_view = dark_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let create_bind_group = |sampler: &wgpu::Sampler, label: &str| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(label),
                layout,
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
        let nearest_bind_group =
            create_bind_group(nearest_sampler, "realtime-manim nearest image bind group");
        let linear_bind_group =
            create_bind_group(linear_sampler, "realtime-manim linear image bind group");
        let bicubic_bind_group =
            create_bind_group(nearest_sampler, "realtime-manim bicubic image bind group");
        let opaque = pixels.chunks_exact(4).all(|pixel| pixel[3] == u8::MAX)
            && dark_pixels.chunks_exact(4).all(|pixel| pixel[3] == u8::MAX);
        cache.insert(
            key.to_owned(),
            GpuImage {
                _texture: texture,
                _dark_texture: dark_texture,
                opaque,
                nearest_bind_group,
                linear_bind_group,
                bicubic_bind_group,
            },
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn prepare_svg_images(
        frame: &EvaluatedFrameView<'_>,
        svg_engine: &mut SvgEngine,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        nearest_sampler: &wgpu::Sampler,
        linear_sampler: &wgpu::Sampler,
        cache: &mut HashMap<String, GpuImage>,
    ) -> Result<(), String> {
        for node in &frame.nodes {
            let NodeKind::Svg { svg, .. } = node.kind.as_ref() else {
                continue;
            };
            let document = svg_engine
                .document(svg)
                .map_err(|error| format!("SVG parsing failed for {}: {error}", node.id))?;
            let mut images = document.images.iter().collect::<Vec<_>>();
            collect_nested_svg_images(&document.paths, &mut images);
            for image in images {
                let key = svg_image_key(image);
                if cache.contains_key(&key) {
                    continue;
                }
                insert_rgba_image(
                    cache,
                    &key,
                    &image.pixels,
                    image.pixel_width,
                    image.pixel_height,
                    &image.pixels,
                    image.pixel_width,
                    image.pixel_height,
                    device,
                    queue,
                    layout,
                    nearest_sampler,
                    linear_sampler,
                );
            }
        }
        Ok(())
    }

    fn svg_image_key(image: &SvgRasterImage) -> String {
        format!("svg-resource-{:016x}", image.key)
    }

    fn collect_nested_svg_images<'a>(
        paths: &'a [realtime_manim_svg_engine::SvgPath],
        output: &mut Vec<&'a SvgRasterImage>,
    ) {
        for path in paths {
            for paint in [path.fill.as_ref(), path.stroke.as_ref()]
                .into_iter()
                .flatten()
            {
                if let SvgPaint::Pattern(pattern) = paint {
                    output.extend(pattern.images.iter());
                    collect_nested_svg_images(&pattern.paths, output);
                }
            }
            for mask in &path.masks {
                output.extend(mask.images.iter());
                collect_nested_svg_images(&mask.paths, output);
            }
        }
    }

    fn prepare_custom_shaders(
        frame: &EvaluatedFrameView<'_>,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        config: &wgpu::SurfaceConfiguration,
        cache: &mut HashMap<String, GpuCustomShader>,
    ) -> Result<(), String> {
        for node in &frame.nodes {
            let NodeKind::CustomShaderMesh {
                vertex_wgsl,
                fragment_wgsl,
                attributes,
                vertex_stride,
                vertex_data,
                indices,
                primitive,
                uniforms,
                depth_test,
            } = node.kind.as_ref()
            else {
                continue;
            };
            let vertex_bytes = pack_shader_vertices(vertex_data, *vertex_stride, attributes)?;
            if let Some(shader) = cache.get_mut(node.id) {
                if vertex_bytes.len() > shader.vertex_capacity {
                    shader.vertex_capacity = vertex_bytes.len().next_power_of_two();
                    shader.vertex_buffer = create_buffer(
                        device,
                        "realtime-manim expanded custom shader vertices",
                        shader.vertex_capacity,
                        wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    );
                }
                queue.write_buffer(&shader.vertex_buffer, 0, &vertex_bytes);
                shader.vertex_count = (vertex_bytes.len() / *vertex_stride as usize)
                    .try_into()
                    .unwrap_or(u32::MAX);
                for (name, buffer) in &shader.uniform_buffers {
                    let Some(uniform) = uniforms.iter().find(|uniform| uniform.name == *name)
                    else {
                        continue;
                    };
                    queue.write_buffer(buffer, 0, &pack_shader_uniform(uniform)?);
                }
                continue;
            }

            let vertex_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("realtime-manim translated Manim vertex shader"),
                source: wgpu::ShaderSource::Wgsl(vertex_wgsl.as_str().into()),
            });
            let fragment_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("realtime-manim translated Manim fragment shader"),
                source: wgpu::ShaderSource::Wgsl(fragment_wgsl.as_str().into()),
            });
            let mut layout_entries = Vec::new();
            for uniform in uniforms {
                if uniform.uniform_type == ShaderUniformType::Sampler2d {
                    layout_entries.push(wgpu::BindGroupLayoutEntry {
                        binding: uniform.binding,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    });
                    layout_entries.push(wgpu::BindGroupLayoutEntry {
                        binding: uniform.sampler_binding.ok_or_else(|| {
                            format!("Shader {} misses a sampler binding.", node.id)
                        })?,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    });
                } else {
                    layout_entries.push(wgpu::BindGroupLayoutEntry {
                        binding: uniform.binding,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    });
                }
            }
            layout_entries.sort_by_key(|entry| entry.binding);
            let bind_group_layout =
                device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("realtime-manim custom shader bind group layout"),
                    entries: &layout_entries,
                });

            let mut uniform_buffers = Vec::new();
            let mut texture_resources = Vec::new();
            let mut sampler_resources = Vec::new();
            for uniform in uniforms {
                if uniform.uniform_type == ShaderUniformType::Sampler2d {
                    let pixels = BASE64.decode(&uniform.texture_pixels).map_err(|_| {
                        format!("Shader {} texture {} is invalid.", node.id, uniform.name)
                    })?;
                    let size = wgpu::Extent3d {
                        width: uniform.texture_width,
                        height: uniform.texture_height,
                        depth_or_array_layers: 1,
                    };
                    let texture = device.create_texture(&wgpu::TextureDescriptor {
                        label: Some("realtime-manim custom shader texture"),
                        size,
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::Rgba8UnormSrgb,
                        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                        view_formats: &[],
                    });
                    queue.write_texture(
                        wgpu::TexelCopyTextureInfo {
                            texture: &texture,
                            mip_level: 0,
                            origin: wgpu::Origin3d::ZERO,
                            aspect: wgpu::TextureAspect::All,
                        },
                        &pixels,
                        wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(uniform.texture_width.saturating_mul(4)),
                            rows_per_image: Some(uniform.texture_height),
                        },
                        size,
                    );
                    texture_resources.push((uniform.binding, texture));
                    sampler_resources.push((
                        uniform.sampler_binding.unwrap_or(uniform.binding + 1),
                        device.create_sampler(&wgpu::SamplerDescriptor {
                            label: Some("realtime-manim custom shader sampler"),
                            mag_filter: wgpu::FilterMode::Linear,
                            min_filter: wgpu::FilterMode::Linear,
                            mipmap_filter: wgpu::MipmapFilterMode::Linear,
                            ..Default::default()
                        }),
                    ));
                } else {
                    let bytes = pack_shader_uniform(uniform)?;
                    let buffer = create_buffer(
                        device,
                        "realtime-manim custom shader uniform",
                        bytes.len(),
                        wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    );
                    queue.write_buffer(&buffer, 0, &bytes);
                    uniform_buffers.push((uniform.name.clone(), uniform.binding, buffer));
                }
            }
            let texture_views = texture_resources
                .iter()
                .map(|(binding, texture)| {
                    (
                        *binding,
                        texture.create_view(&wgpu::TextureViewDescriptor::default()),
                    )
                })
                .collect::<Vec<_>>();
            let mut bind_entries = Vec::new();
            for (_, binding, buffer) in &uniform_buffers {
                bind_entries.push(wgpu::BindGroupEntry {
                    binding: *binding,
                    resource: buffer.as_entire_binding(),
                });
            }
            for (binding, view) in &texture_views {
                bind_entries.push(wgpu::BindGroupEntry {
                    binding: *binding,
                    resource: wgpu::BindingResource::TextureView(view),
                });
            }
            for (binding, sampler) in &sampler_resources {
                bind_entries.push(wgpu::BindGroupEntry {
                    binding: *binding,
                    resource: wgpu::BindingResource::Sampler(sampler),
                });
            }
            bind_entries.sort_by_key(|entry| entry.binding);
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("realtime-manim custom shader bind group"),
                layout: &bind_group_layout,
                entries: &bind_entries,
            });
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("realtime-manim custom shader pipeline layout"),
                bind_group_layouts: &[Some(&bind_group_layout)],
                immediate_size: 0,
            });
            let vertex_attributes = attributes
                .iter()
                .map(|attribute| {
                    Ok(wgpu::VertexAttribute {
                        format: shader_vertex_format(attribute.format),
                        offset: u64::from(attribute.offset),
                        shader_location: attribute.location,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            let topology = shader_topology(*primitive);
            let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("realtime-manim translated Manim shader pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &vertex_module,
                    entry_point: Some("main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: u64::from(*vertex_stride),
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &vertex_attributes,
                    })],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &fragment_module,
                    entry_point: Some("main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: config.format,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology,
                    strip_index_format: if matches!(
                        topology,
                        wgpu::PrimitiveTopology::LineStrip | wgpu::PrimitiveTopology::TriangleStrip
                    ) && !indices.is_empty()
                    {
                        Some(wgpu::IndexFormat::Uint32)
                    } else {
                        None
                    },
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(pass_depth_state(*depth_test)),
                multisample: wgpu::MultisampleState {
                    count: SAMPLE_COUNT,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                multiview_mask: None,
                cache: None,
            });
            let vertex_capacity = vertex_bytes.len().max(4).next_power_of_two();
            let vertex_buffer = create_buffer(
                device,
                "realtime-manim custom shader vertices",
                vertex_capacity,
                wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            );
            queue.write_buffer(&vertex_buffer, 0, &vertex_bytes);
            let index_capacity = (indices.len() * mem::size_of::<u32>())
                .max(4)
                .next_power_of_two();
            let index_buffer = create_buffer(
                device,
                "realtime-manim custom shader indices",
                index_capacity,
                wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            );
            if !indices.is_empty() {
                queue.write_buffer(&index_buffer, 0, bytemuck::cast_slice(indices));
            }
            cache.insert(
                node.id.to_owned(),
                GpuCustomShader {
                    pipeline,
                    bind_group,
                    vertex_buffer,
                    vertex_capacity,
                    index_buffer,
                    index_count: indices.len().try_into().unwrap_or(u32::MAX),
                    vertex_count: (vertex_bytes.len() / *vertex_stride as usize)
                        .try_into()
                        .unwrap_or(u32::MAX),
                    uniform_buffers: uniform_buffers
                        .into_iter()
                        .map(|(name, _, buffer)| (name, buffer))
                        .collect(),
                    _textures: texture_resources
                        .into_iter()
                        .map(|(_, texture)| texture)
                        .collect(),
                    _samplers: sampler_resources
                        .into_iter()
                        .map(|(_, sampler)| sampler)
                        .collect(),
                },
            );
        }
        Ok(())
    }

    fn pack_shader_uniform(uniform: &ShaderUniform) -> Result<Vec<u8>, String> {
        if uniform.uniform_type == ShaderUniformType::Sampler2d {
            return Ok(Vec::new());
        }
        if uniform.array_length > 1 {
            let array_length = uniform.array_length as usize;
            if uniform.values.is_empty() || uniform.values.len() % array_length != 0 {
                return Err(format!(
                    "Shader uniform array {} has an invalid value count.",
                    uniform.name
                ));
            }
            let values_per_element = uniform.values.len() / array_length;
            let mut bytes = Vec::new();
            for values in uniform.values.chunks_exact(values_per_element) {
                let mut element = uniform.clone();
                element.array_length = 1;
                element.values = values.to_vec();
                bytes.extend(pack_shader_uniform(&element)?);
            }
            return Ok(bytes);
        }
        if matches!(
            uniform.uniform_type,
            ShaderUniformType::Int
                | ShaderUniformType::Ivec2
                | ShaderUniformType::Ivec3
                | ShaderUniformType::Ivec4
        ) {
            let mut values = uniform
                .values
                .iter()
                .map(|value| value.round() as i32)
                .collect::<Vec<_>>();
            while values.len() < 4 {
                values.push(0);
            }
            return Ok(bytemuck::cast_slice(&values).to_vec());
        }
        if matches!(
            uniform.uniform_type,
            ShaderUniformType::Uint
                | ShaderUniformType::Uvec2
                | ShaderUniformType::Uvec3
                | ShaderUniformType::Uvec4
                | ShaderUniformType::Bool
                | ShaderUniformType::Bvec2
                | ShaderUniformType::Bvec3
                | ShaderUniformType::Bvec4
        ) {
            let boolean = matches!(
                uniform.uniform_type,
                ShaderUniformType::Bool
                    | ShaderUniformType::Bvec2
                    | ShaderUniformType::Bvec3
                    | ShaderUniformType::Bvec4
            );
            let mut values = uniform
                .values
                .iter()
                .map(|value| {
                    if boolean {
                        u32::from(*value >= 0.5)
                    } else {
                        value.round().max(0.0) as u32
                    }
                })
                .collect::<Vec<_>>();
            while values.len() < 4 {
                values.push(0);
            }
            return Ok(bytemuck::cast_slice(&values).to_vec());
        }
        let mut values = match uniform.uniform_type {
            ShaderUniformType::Mat3 => uniform
                .values
                .chunks_exact(3)
                .flat_map(|column| [column[0], column[1], column[2], 0.0])
                .collect::<Vec<_>>(),
            _ => uniform.values.clone(),
        };
        while values.len() < 4 {
            values.push(0.0);
        }
        if values.is_empty() {
            return Err(format!("Shader uniform {} has no values.", uniform.name));
        }
        Ok(bytemuck::cast_slice(&values).to_vec())
    }

    fn pack_shader_vertices(
        values: &[f32],
        stride: u32,
        attributes: &[ShaderAttribute],
    ) -> Result<Vec<u8>, String> {
        let stride = stride as usize;
        let byte_len = values
            .len()
            .checked_mul(mem::size_of::<f32>())
            .ok_or_else(|| "Custom shader vertex buffer is too large.".to_owned())?;
        if stride == 0 || !byte_len.is_multiple_of(stride) {
            return Err("Custom shader vertex buffer has an invalid stride.".to_owned());
        }
        let vertex_count = byte_len / stride;
        let source_stride = stride / mem::size_of::<f32>();
        let mut bytes = vec![0_u8; byte_len];
        for vertex in 0..vertex_count {
            for attribute in attributes {
                let component_count = attribute.format.size() as usize / 4;
                let source_start = vertex * source_stride + attribute.offset as usize / 4;
                let destination_start = vertex * stride + attribute.offset as usize;
                let source = &values[source_start..source_start + component_count];
                let destination =
                    &mut bytes[destination_start..destination_start + component_count * 4];
                match attribute.format {
                    ShaderVertexFormat::Float32
                    | ShaderVertexFormat::Float32x2
                    | ShaderVertexFormat::Float32x3
                    | ShaderVertexFormat::Float32x4 => {
                        destination.copy_from_slice(bytemuck::cast_slice(source));
                    }
                    ShaderVertexFormat::Sint32
                    | ShaderVertexFormat::Sint32x2
                    | ShaderVertexFormat::Sint32x3
                    | ShaderVertexFormat::Sint32x4 => {
                        let converted = source
                            .iter()
                            .map(|value| value.round() as i32)
                            .collect::<Vec<_>>();
                        destination.copy_from_slice(bytemuck::cast_slice(&converted));
                    }
                    ShaderVertexFormat::Uint32
                    | ShaderVertexFormat::Uint32x2
                    | ShaderVertexFormat::Uint32x3
                    | ShaderVertexFormat::Uint32x4 => {
                        let converted = source
                            .iter()
                            .map(|value| value.round().max(0.0) as u32)
                            .collect::<Vec<_>>();
                        destination.copy_from_slice(bytemuck::cast_slice(&converted));
                    }
                }
            }
        }
        Ok(bytes)
    }

    fn shader_vertex_format(format: ShaderVertexFormat) -> wgpu::VertexFormat {
        match format {
            ShaderVertexFormat::Float32 => wgpu::VertexFormat::Float32,
            ShaderVertexFormat::Float32x2 => wgpu::VertexFormat::Float32x2,
            ShaderVertexFormat::Float32x3 => wgpu::VertexFormat::Float32x3,
            ShaderVertexFormat::Float32x4 => wgpu::VertexFormat::Float32x4,
            ShaderVertexFormat::Sint32 => wgpu::VertexFormat::Sint32,
            ShaderVertexFormat::Sint32x2 => wgpu::VertexFormat::Sint32x2,
            ShaderVertexFormat::Sint32x3 => wgpu::VertexFormat::Sint32x3,
            ShaderVertexFormat::Sint32x4 => wgpu::VertexFormat::Sint32x4,
            ShaderVertexFormat::Uint32 => wgpu::VertexFormat::Uint32,
            ShaderVertexFormat::Uint32x2 => wgpu::VertexFormat::Uint32x2,
            ShaderVertexFormat::Uint32x3 => wgpu::VertexFormat::Uint32x3,
            ShaderVertexFormat::Uint32x4 => wgpu::VertexFormat::Uint32x4,
        }
    }

    fn shader_topology(primitive: ShaderPrimitive) -> wgpu::PrimitiveTopology {
        match primitive {
            ShaderPrimitive::PointList => wgpu::PrimitiveTopology::PointList,
            ShaderPrimitive::LineList => wgpu::PrimitiveTopology::LineList,
            ShaderPrimitive::LineStrip => wgpu::PrimitiveTopology::LineStrip,
            ShaderPrimitive::TriangleList => wgpu::PrimitiveTopology::TriangleList,
            ShaderPrimitive::TriangleStrip => wgpu::PrimitiveTopology::TriangleStrip,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn create_vector_fragment_pipeline(
        device: &wgpu::Device,
        shader: &wgpu::ShaderModule,
        layout: &wgpu::PipelineLayout,
        format: wgpu::TextureFormat,
        label: &str,
        fragment_entry: &str,
        depth_stencil: wgpu::DepthStencilState,
    ) -> wgpu::RenderPipeline {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &VERTEX_ATTRIBUTES,
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
            depth_stencil: Some(depth_stencil),
            multisample: wgpu::MultisampleState {
                count: SAMPLE_COUNT,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        })
    }

    fn pass_depth_state(enabled: bool) -> wgpu::DepthStencilState {
        wgpu::DepthStencilState {
            format: DEPTH_STENCIL_FORMAT,
            depth_write_enabled: Some(enabled),
            depth_compare: Some(if enabled {
                wgpu::CompareFunction::LessEqual
            } else {
                wgpu::CompareFunction::Always
            }),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }
    }

    fn transparent_depth_state() -> wgpu::DepthStencilState {
        wgpu::DepthStencilState {
            format: DEPTH_STENCIL_FORMAT,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }
    }

    fn clipped_depth_state() -> wgpu::DepthStencilState {
        wgpu::DepthStencilState {
            format: DEPTH_STENCIL_FORMAT,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: wgpu::StencilState {
                front: wgpu::StencilFaceState {
                    compare: wgpu::CompareFunction::Equal,
                    fail_op: wgpu::StencilOperation::Keep,
                    depth_fail_op: wgpu::StencilOperation::Keep,
                    pass_op: wgpu::StencilOperation::Keep,
                },
                back: wgpu::StencilFaceState {
                    compare: wgpu::CompareFunction::Equal,
                    fail_op: wgpu::StencilOperation::Keep,
                    depth_fail_op: wgpu::StencilOperation::Keep,
                    pass_op: wgpu::StencilOperation::Keep,
                },
                read_mask: 0xff,
                write_mask: 0,
            },
            bias: wgpu::DepthBiasState::default(),
        }
    }

    fn clip_depth_state(operation: wgpu::StencilOperation) -> wgpu::DepthStencilState {
        let face = wgpu::StencilFaceState {
            compare: wgpu::CompareFunction::Equal,
            fail_op: wgpu::StencilOperation::Keep,
            depth_fail_op: wgpu::StencilOperation::Keep,
            pass_op: operation,
        };
        wgpu::DepthStencilState {
            format: DEPTH_STENCIL_FORMAT,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: wgpu::StencilState {
                front: face,
                back: face,
                read_mask: 0xff,
                write_mask: 0xff,
            },
            bias: wgpu::DepthBiasState::default(),
        }
    }

    fn create_msaa_target(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("realtime-manim 4x MSAA target"),
            size: wgpu::Extent3d {
                width: config.width.max(1),
                height: config.height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: SAMPLE_COUNT,
            dimension: wgpu::TextureDimension::D2,
            format: config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    fn create_depth_target(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("realtime-manim 4x MSAA depth target"),
            size: wgpu::Extent3d {
                width: config.width.max(1),
                height: config.height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: SAMPLE_COUNT,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_STENCIL_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    fn create_mask_targets(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
        layout: &wgpu::BindGroupLayout,
        mask_index_buffer: &wgpu::Buffer,
        requested_layers: u32,
    ) -> GpuMaskTargets {
        let layers = requested_layers.max(1);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("realtime-manim SVG mask texture array"),
            size: wgpu::Extent3d {
                width: config.width.max(1),
                height: config.height.max(1),
                depth_or_array_layers: layers,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: config.format,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let array_view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("realtime-manim SVG mask texture array view"),
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            base_array_layer: 0,
            array_layer_count: Some(layers),
            ..Default::default()
        });
        let msaa_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("realtime-manim SVG mask 4x MSAA scratch"),
            size: wgpu::Extent3d {
                width: config.width.max(1),
                height: config.height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: SAMPLE_COUNT,
            dimension: wgpu::TextureDimension::D2,
            format: config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let msaa_view = msaa_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let resolve_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("realtime-manim SVG mask resolve scratch"),
            size: wgpu::Extent3d {
                width: config.width.max(1),
                height: config.height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let resolve_view = resolve_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("realtime-manim SVG mask stencil scratch"),
            size: wgpu::Extent3d {
                width: config.width.max(1),
                height: config.height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: SAMPLE_COUNT,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_STENCIL_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("realtime-manim SVG mask bind group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&array_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: mask_index_buffer.as_entire_binding(),
                },
            ],
        });
        GpuMaskTargets {
            texture,
            _array_view: array_view,
            _msaa_texture: msaa_texture,
            msaa_view,
            _resolve_texture: resolve_texture,
            resolve_view,
            _depth_texture: depth_texture,
            depth_view,
            bind_group,
            width: config.width,
            height: config.height,
            layers,
        }
    }

    fn build_geometry_into(
        frame: &EvaluatedFrameView<'_>,
        image_cache: &HashMap<String, GpuImage>,
        text_engine: &mut TextEngine,
        svg_engine: &mut SvgEngine,
        output: &mut FrameGeometry,
    ) -> Result<(), String> {
        let mut buffers = VertexBuffers {
            vertices: mem::take(&mut output.vertices),
            indices: mem::take(&mut output.indices),
        };
        let mut image_vertices = mem::take(&mut output.image_vertices);
        let mut mesh_texture_vertices = mem::take(&mut output.mesh_texture_vertices);
        let mut gradient_stops = mem::take(&mut output.gradient_stops);
        let mut commands = mem::take(&mut output.commands);
        let mut transparent_primitives = mem::take(&mut output.transparent_primitives);
        let mut mask_layers = mem::take(&mut output.mask_layers);
        let mut mask_indices = mem::take(&mut output.mask_indices);
        buffers.vertices.clear();
        buffers.indices.clear();
        image_vertices.clear();
        mesh_texture_vertices.clear();
        gradient_stops.clear();
        commands.clear();
        transparent_primitives.clear();
        mask_layers.clear();
        mask_indices.clear();

        let result = (|| {
            for node in &frame.nodes {
                if let NodeKind::Image {
                    corners,
                    resampling,
                    ..
                } = node.kind.as_ref()
                {
                    let start = image_vertices.len() as u32;
                    append_image_vertices(&mut image_vertices, frame, node, corners)?;
                    commands.push(DrawCommand::Image {
                        vertices: start..image_vertices.len() as u32,
                        key: node.id.to_owned(),
                        resampling: *resampling,
                        reference: 0,
                        masked: false,
                    });
                } else if let NodeKind::Svg {
                    svg,
                    height,
                    preserve_styles,
                } = node.kind.as_ref()
                {
                    append_svg(
                        &mut buffers,
                        &mut gradient_stops,
                        &mut commands,
                        &mut image_vertices,
                        &mut mask_layers,
                        &mut mask_indices,
                        frame,
                        node,
                        svg,
                        *height,
                        *preserve_styles,
                        svg_engine,
                    )?;
                } else if matches!(node.kind.as_ref(), NodeKind::CustomShaderMesh { .. }) {
                    commands.push(DrawCommand::CustomShader {
                        key: node.id.to_owned(),
                    });
                } else if let NodeKind::Mesh {
                    vertices,
                    triangles,
                    uvs,
                    texture_pixels,
                    texture_resampling,
                    normals,
                    gloss,
                    shadow,
                    light_position,
                    dark_texture_pixels,
                    ..
                } = node.kind.as_ref()
                    && !texture_pixels.is_empty()
                {
                    let start = mesh_texture_vertices.len() as u32;
                    let depth_test = node.style.opacity >= 0.999
                        && image_cache.get(node.id).is_some_and(|image| image.opaque);
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
                    )?;
                    let end = mesh_texture_vertices.len() as u32;
                    let command_index = commands.len();
                    commands.push(DrawCommand::MeshTexture {
                        vertices: start..end,
                        key: node.id.to_owned(),
                        resampling: *texture_resampling,
                        depth_test,
                    });
                    if !depth_test {
                        transparent_primitives.extend(triangle_depths.into_iter().enumerate().map(
                            |(triangle, depth)| TransparentPrimitive {
                                depth,
                                command_index,
                                range: start + triangle as u32 * 3..start + triangle as u32 * 3 + 3,
                            },
                        ));
                    }
                } else {
                    let start = buffers.indices.len() as u32;
                    append_node(&mut buffers, &mut gradient_stops, frame, node, text_engine)?;
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
                                    depth: vector_triangle_view_depth(
                                        frame,
                                        &buffers,
                                        triangle_start,
                                    ),
                                    command_index,
                                    range: triangle_start..triangle_start + 3,
                                });
                            }
                        }
                    }
                }
            }
            sort_back_to_front_by_depth(&mut transparent_primitives, |primitive| primitive.depth);
            Ok(())
        })();

        output.vertices = buffers.vertices;
        output.indices = buffers.indices;
        output.gradient_stops = gradient_stops;
        output.image_vertices = image_vertices;
        output.mesh_texture_vertices = mesh_texture_vertices;
        output.commands = commands;
        output.transparent_primitives = transparent_primitives;
        output.mask_layers = mask_layers;
        output.mask_indices = mask_indices;
        result
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
                        let visible_stroke_is_opaque =
                            *radius <= f32::EPSILON || parse_color(color)?[3] >= 0.999;
                        Ok::<bool, String>(opaque && visible_stroke_is_opaque)
                    },
                )?;
                Ok(colors_are_opaque(colors)? && opaque_strokes)
            }
            _ => Ok(false),
        }
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
            to_clip(
                frame,
                apply_matrix(node.transform, [corners[index][0], corners[index][1]]),
            )
        };
        let opacity = node.style.opacity;
        output.extend([
            ImageVertex {
                position: position(0),
                uv: [0.0, 0.0],
                opacity,
                mask_meta: [0.0; 2],
            },
            ImageVertex {
                position: position(2),
                uv: [0.0, 1.0],
                opacity,
                mask_meta: [0.0; 2],
            },
            ImageVertex {
                position: position(1),
                uv: [1.0, 0.0],
                opacity,
                mask_meta: [0.0; 2],
            },
            ImageVertex {
                position: position(1),
                uv: [1.0, 0.0],
                opacity,
                mask_meta: [0.0; 2],
            },
            ImageVertex {
                position: position(2),
                uv: [0.0, 1.0],
                opacity,
                mask_meta: [0.0; 2],
            },
            ImageVertex {
                position: position(3),
                uv: [1.0, 1.0],
                opacity,
                mask_meta: [0.0; 2],
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
                let amount = ((boundary - previous_depth) / (current_depth - previous_depth))
                    .clamp(0.0, 1.0);
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
    ) -> Result<Vec<f32>, String> {
        if uvs.len() != vertices.len() {
            return Err(format!(
                "Textured mesh {} requires one UV per vertex.",
                node.id
            ));
        }
        let transformed: Vec<[f32; 3]> = vertices
            .iter()
            .map(|vertex| transform_point_3d(*vertex, node.transform_3d))
            .collect();
        let camera = frame.camera_3d;
        let transformed_normals = if normals.len() == vertices.len() {
            normals
                .iter()
                .map(|normal| transform_normal_3d(*normal, node.transform_3d))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| format!("Mesh {} normal transform failed: {error}", node.id))?
        } else {
            Vec::new()
        };
        let forward = normalize_3d(sub_3d(camera.target, camera.position));
        let right = normalize_3d(cross_3d(forward, camera.up));
        let camera_up = normalize_3d(cross_3d(right, forward));
        let tan_half_fov = (camera.fov_y * 0.5).tan().max(0.0001);
        let aspect = (frame.width / frame.height).max(0.0001);
        let mut projected_triangles = Vec::with_capacity(triangles.len());
        for triangle in triangles {
            let points = triangle.map(|index| transformed[index as usize]);
            let triangle_normals = if transformed_normals.is_empty() {
                let normal = normalize_3d(cross_3d(
                    sub_3d(points[1], points[0]),
                    sub_3d(points[2], points[0]),
                ));
                [normal; 3]
            } else {
                triangle.map(|index| transformed_normals[index as usize])
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
        let triangle_depths = projected_triangles
            .iter()
            .map(|triangle| triangle.0)
            .collect();
        let opacity = node.style.opacity;
        for (_, positions, triangle_uvs, points, triangle_normals) in projected_triangles {
            output.extend(
                positions
                    .into_iter()
                    .zip(triangle_uvs)
                    .zip(points)
                    .zip(triangle_normals)
                    .map(|(((position, uv), point), normal)| MeshTextureVertex {
                        position,
                        uv,
                        opacity,
                        point,
                        normal,
                        light_position,
                        gloss,
                        shadow,
                        has_dark_texture: if has_dark_texture { 1.0 } else { 0.0 },
                    }),
            );
        }
        Ok(triangle_depths)
    }

    fn append_node(
        buffers: &mut VertexBuffers<Vertex, u32>,
        gradient_stops: &mut Vec<GpuGradientStop>,
        frame: &EvaluatedFrameView<'_>,
        node: &EvaluatedNodeView<'_>,
        text_engine: &mut TextEngine,
    ) -> Result<(), String> {
        match node.kind.as_ref() {
            NodeKind::Group | NodeKind::Billboard { .. } => {}
            NodeKind::Text {
                text,
                font_size,
                font_family,
                align,
                weight,
                slant,
            } => append_text(
                buffers,
                gradient_stops,
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
                ..
            } => append_markup_text(
                buffers,
                gradient_stops,
                frame,
                node,
                spans,
                *font_size,
                font_family,
                *align,
                text_engine,
            )?,
            NodeKind::Svg { .. } => {}
            NodeKind::Path3d { commands } => {
                let commands = project_path_3d(frame, node, commands)?;
                if !commands.is_empty() {
                    let mut projected_node = node.clone();
                    projected_node.transform = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
                    let path =
                        command_path(&commands, node.style.draw_start, node.style.draw_progress)?;
                    append_path(buffers, gradient_stops, frame, &projected_node, &path)?;
                }
            }
            NodeKind::Image { .. } => {}
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
                for point_mark in points {
                    let mut point_node = node.clone();
                    point_node.transform =
                        translate_matrix(node.transform, point_mark.x, point_mark.y);
                    if let Some(color) = &point_mark.color {
                        let mut fill = parse_color(color)?;
                        fill[3] *= node.style.opacity;
                        point_node.style.fill = Some(fill);
                        point_node.style.fill_gradient = None;
                    }
                    append_circle(
                        buffers,
                        gradient_stops,
                        frame,
                        &point_node,
                        point_mark.radius.unwrap_or(*radius) * radius_scale,
                    )?;
                }
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
                ..
            } => append_mesh(
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
            )?,
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
            NodeKind::CustomShaderMesh { .. } => {}
            NodeKind::Circle { radius } => {
                append_circle(buffers, gradient_stops, frame, node, *radius)?
            }
            NodeKind::Rect {
                width,
                height,
                corner_radius,
            } => {
                let path = rect_path(*width, *height, *corner_radius);
                append_path(buffers, gradient_stops, frame, node, &path)?;
            }
            NodeKind::Line { from, to } => {
                let points = [*from, *to];
                append_polyline(buffers, gradient_stops, frame, node, &points, false)?;
            }
            NodeKind::Arrow { from, to, tip_size } => {
                append_arrow(buffers, gradient_stops, frame, node, *from, *to, *tip_size)?
            }
            NodeKind::Polyline { points, closed } => {
                append_polyline(buffers, gradient_stops, frame, node, points, *closed)?;
            }
            NodeKind::Path { commands } => {
                let path = command_path(commands, node.style.draw_start, node.style.draw_progress)?;
                append_path(buffers, gradient_stops, frame, node, &path)?;
            }
            NodeKind::PathRef { source } => {
                return Err(format!(
                    "Unresolved retained path reference {} -> {source}.",
                    node.id
                ));
            }
            NodeKind::TracePath { .. } => {
                return Err(format!("Unresolved retained trace path {}.", node.id));
            }
        }
        Ok(())
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
        let transformed: Vec<[f32; 3]> = vertices
            .iter()
            .map(|vertex| transform_point_3d(*vertex, node.transform_3d))
            .collect();
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
            let points = [
                transformed[triangle[0] as usize],
                transformed[triangle[1] as usize],
                transformed[triangle[2] as usize],
            ];
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
                    if normal[2] < 0.0 {
                        normal = mul_3d(normal, -1.0);
                    }
                    let to_camera = sub_3d([0.0, 0.0, 6.0], point);
                    let to_light = sub_3d(light_position, point);
                    let reflection = add_3d(
                        mul_3d(to_light, -1.0),
                        mul_3d(normal, 2.0 * dot_3d(to_light, normal)),
                    );
                    let dot_product = dot_3d(normalize_3d(reflection), normalize_3d(to_camera));
                    let shine = gloss * (-3.0 * (1.0 - dot_product).powi(2)).exp();
                    let lit =
                        1.0 + (dot_3d(normalize_3d(to_light), normal).max(0.0) - 1.0) * shadow;
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
                projected_triangles.push((
                    depth / 3.0,
                    projected,
                    clipped.map(|vertex| vertex.color),
                ));
            }
        }
        if !mesh_node_is_opaque(node)? {
            projected_triangles.sort_by(|left, right| right.0.total_cmp(&left.0));
        }
        for (_, points, colors) in projected_triangles {
            let base = buffers.vertices.len() as u32;
            buffers
                .vertices
                .extend(
                    points
                        .into_iter()
                        .zip(colors)
                        .map(|(position, color)| Vertex {
                            position,
                            color,
                            gradient_position: [0.0; 2],
                            gradient_meta: [0.0; 4],
                            mask_meta: [0.0; 2],
                        }),
                );
            buffers.indices.extend([base, base + 1, base + 2]);
        }
        Ok(())
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
        let length = dot_3d(value, value).sqrt().max(f32::EPSILON);
        mul_3d(value, 1.0 / length)
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
        buffers.indices[triangle_start as usize..triangle_start as usize + 3]
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
    fn append_svg(
        buffers: &mut VertexBuffers<Vertex, u32>,
        gradient_stops: &mut Vec<GpuGradientStop>,
        draw_commands: &mut Vec<DrawCommand>,
        image_vertices: &mut Vec<ImageVertex>,
        mask_layers: &mut Vec<MaskLayerGeometry>,
        mask_indices: &mut Vec<u32>,
        frame: &EvaluatedFrameView<'_>,
        node: &EvaluatedNodeView<'_>,
        source: &str,
        height: f32,
        preserve_styles: bool,
        svg_engine: &mut SvgEngine,
    ) -> Result<(), String> {
        let document = svg_engine
            .document(source)
            .map_err(|error| format!("SVG parsing failed for {}: {error}", node.id))?;
        let scale = height / document.height.max(f32::EPSILON);
        let mut path_node = node.clone();
        path_node.transform = local_matrix(
            node.transform,
            -document.width * scale * 0.5,
            document.height * scale * 0.5,
            scale,
            -scale,
        );
        if !preserve_styles
            && path_node.style.fill.is_none()
            && path_node.style.fill_gradient.is_none()
        {
            path_node.style.fill = path_node.style.stroke;
            path_node.style.fill_gradient = path_node.style.stroke_gradient.clone();
            path_node.style.stroke = None;
            path_node.style.stroke_gradient = None;
        }
        let mut mask_layer_by_key = HashMap::new();
        for element in &document.order {
            let SvgElementRef::Path(path_index) = element else {
                let SvgElementRef::Image(image_index) = element else {
                    unreachable!();
                };
                append_svg_raster_image(
                    buffers,
                    gradient_stops,
                    image_vertices,
                    draw_commands,
                    mask_layers,
                    mask_indices,
                    frame,
                    &path_node,
                    &document.images[*image_index],
                    0,
                    [0.0; 2],
                    false,
                )?;
                continue;
            };
            let svg_path = &document.paths[*path_index];
            if svg_path.clips.len() > 255 {
                return Err(format!("SVG {} exceeds 255 nested clip paths.", node.id));
            }
            let mut content_mask_layers =
                Vec::with_capacity(svg_path.masks.len() + svg_path.clips.len());
            for mask in &svg_path.masks {
                let layer = ensure_svg_mask_layer(
                    buffers,
                    gradient_stops,
                    image_vertices,
                    mask_layers,
                    mask_indices,
                    &mut mask_layer_by_key,
                    frame,
                    &path_node,
                    mask,
                )?;
                content_mask_layers.push(match mask.kind {
                    SvgMaskType::Alpha => layer,
                    SvgMaskType::Luminance => layer | 0x8000_0000,
                });
            }
            let mut styled_node = path_node.clone();
            let mut clip_ranges = Vec::with_capacity(svg_path.clips.len());
            for clip in &svg_path.clips {
                if svg_clip_is_complex(clip) {
                    content_mask_layers.push(ensure_svg_clip_layer(
                        buffers,
                        mask_layers,
                        mask_indices,
                        frame,
                        &styled_node,
                        clip,
                    )?);
                    continue;
                }
                let start = buffers.indices.len() as u32;
                for shape in &clip.shapes {
                    append_path_with_registered_paints(
                        buffers,
                        frame,
                        &styled_node,
                        &shape.path,
                        PathRenderOptions {
                            fill_rule: match shape.fill_rule {
                                SvgFillRule::NonZero => lyon::tessellation::FillRule::NonZero,
                                SvgFillRule::EvenOdd => lyon::tessellation::FillRule::EvenOdd,
                            },
                            ..PathRenderOptions::default()
                        },
                        Some(GeometryPaint::Solid([0.0; 4])),
                        None,
                    )?;
                }
                clip_ranges.push(start..buffers.indices.len() as u32);
            }
            let content_masked = !content_mask_layers.is_empty();
            let mask_meta = if content_masked {
                let start = mask_indices.len() as u32;
                let count = content_mask_layers.len() as f32;
                mask_indices.extend(content_mask_layers);
                [start as f32, count]
            } else {
                [0.0; 2]
            };
            if preserve_styles {
                styled_node.style.stroke_width = svg_path.stroke_width;
                styled_node.style.fill = None;
                styled_node.style.fill_gradient = None;
                styled_node.style.stroke = None;
                styled_node.style.stroke_gradient = None;
            }
            let options = PathRenderOptions {
                fill_rule: match svg_path.fill_rule {
                    SvgFillRule::NonZero => lyon::tessellation::FillRule::NonZero,
                    SvgFillRule::EvenOdd => lyon::tessellation::FillRule::EvenOdd,
                },
                line_cap: match svg_path.line_cap {
                    SvgLineCap::Butt => lyon::tessellation::LineCap::Butt,
                    SvgLineCap::Round => lyon::tessellation::LineCap::Round,
                    SvgLineCap::Square => lyon::tessellation::LineCap::Square,
                },
                line_join: match svg_path.line_join {
                    SvgLineJoin::Miter => lyon::tessellation::LineJoin::Miter,
                    SvgLineJoin::Round => lyon::tessellation::LineJoin::Round,
                    SvgLineJoin::Bevel => lyon::tessellation::LineJoin::Bevel,
                },
            };
            if preserve_styles {
                match svg_path.fill.as_ref() {
                    Some(SvgPaint::Pattern(pattern)) => append_svg_pattern_fill(
                        buffers,
                        gradient_stops,
                        draw_commands,
                        image_vertices,
                        mask_layers,
                        mask_indices,
                        frame,
                        &styled_node,
                        &svg_path.path,
                        options,
                        &clip_ranges,
                        mask_meta,
                        content_masked,
                        pattern,
                        document.width,
                        document.height,
                        node.style.opacity,
                        false,
                        0,
                        0,
                    )?,
                    paint => append_svg_regular_paint(
                        buffers,
                        gradient_stops,
                        draw_commands,
                        frame,
                        &styled_node,
                        &svg_path.path,
                        options,
                        &clip_ranges,
                        mask_meta,
                        content_masked,
                        paint,
                        None,
                        node.style.opacity,
                    )?,
                }
                match svg_path.stroke.as_ref() {
                    Some(SvgPaint::Pattern(pattern)) => append_svg_pattern_fill(
                        buffers,
                        gradient_stops,
                        draw_commands,
                        image_vertices,
                        mask_layers,
                        mask_indices,
                        frame,
                        &styled_node,
                        &svg_path.path,
                        options,
                        &clip_ranges,
                        mask_meta,
                        content_masked,
                        pattern,
                        document.width,
                        document.height,
                        node.style.opacity,
                        true,
                        0,
                        0,
                    )?,
                    paint => append_svg_regular_paint(
                        buffers,
                        gradient_stops,
                        draw_commands,
                        frame,
                        &styled_node,
                        &svg_path.path,
                        options,
                        &clip_ranges,
                        mask_meta,
                        content_masked,
                        None,
                        paint,
                        node.style.opacity,
                    )?,
                }
                continue;
            }
            let content_start = buffers.indices.len() as u32;
            let content_vertex_start = buffers.vertices.len();
            append_path_with_options(
                buffers,
                gradient_stops,
                frame,
                &styled_node,
                &svg_path.path,
                options,
            )?;
            let content_end = buffers.indices.len() as u32;
            if content_end <= content_start {
                continue;
            }
            for vertex in &mut buffers.vertices[content_vertex_start..] {
                vertex.mask_meta = mask_meta;
            }
            if clip_ranges.is_empty() && !content_masked {
                draw_commands.push(DrawCommand::Vector {
                    indices: content_start..content_end,
                    depth_test: false,
                    transparent_3d: false,
                });
            } else {
                for (depth, indices) in clip_ranges.iter().enumerate() {
                    draw_commands.push(DrawCommand::ClipPush {
                        indices: indices.clone(),
                        reference: depth as u32,
                    });
                }
                if !content_masked {
                    draw_commands.push(DrawCommand::ClippedVector {
                        indices: content_start..content_end,
                        reference: clip_ranges.len() as u32,
                    });
                } else {
                    draw_commands.push(DrawCommand::MaskedVector {
                        indices: content_start..content_end,
                        reference: clip_ranges.len() as u32,
                    });
                }
                for (depth, indices) in clip_ranges.iter().enumerate().rev() {
                    draw_commands.push(DrawCommand::ClipPop {
                        indices: indices.clone(),
                        reference: depth as u32 + 1,
                    });
                }
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn append_svg_raster_image<S: SvgCommandSink>(
        buffers: &mut VertexBuffers<Vertex, u32>,
        gradient_stops: &mut Vec<GpuGradientStop>,
        image_vertices: &mut Vec<ImageVertex>,
        draw_commands: &mut S,
        mask_layers: &mut Vec<MaskLayerGeometry>,
        mask_indices: &mut Vec<u32>,
        frame: &EvaluatedFrameView<'_>,
        path_node: &EvaluatedNodeView<'_>,
        image: &SvgRasterImage,
        clip_reference_base: u32,
        outer_mask_meta: [f32; 2],
        outer_masked: bool,
    ) -> Result<(), String> {
        let mut descriptors = if outer_masked {
            let start = outer_mask_meta[0] as usize;
            let count = outer_mask_meta[1] as usize;
            let end = start.saturating_add(count);
            if end > mask_indices.len() {
                return Err("Embedded SVG image contains invalid outer mask metadata.".to_owned());
            }
            mask_indices[start..end].to_vec()
        } else {
            Vec::new()
        };
        descriptors.reserve(image.masks.len() + image.clips.len());
        let mut mask_layer_by_key = HashMap::new();
        for mask in &image.masks {
            let layer = ensure_svg_mask_layer(
                buffers,
                gradient_stops,
                image_vertices,
                mask_layers,
                mask_indices,
                &mut mask_layer_by_key,
                frame,
                path_node,
                mask,
            )?;
            descriptors.push(match mask.kind {
                SvgMaskType::Alpha => layer,
                SvgMaskType::Luminance => layer | 0x8000_0000,
            });
        }
        let mut clip_ranges = Vec::with_capacity(image.clips.len());
        for clip in &image.clips {
            if svg_clip_is_complex(clip) {
                descriptors.push(ensure_svg_clip_layer(
                    buffers,
                    mask_layers,
                    mask_indices,
                    frame,
                    path_node,
                    clip,
                )?);
                continue;
            }
            let start = buffers.indices.len() as u32;
            for shape in &clip.shapes {
                append_path_with_registered_paints(
                    buffers,
                    frame,
                    path_node,
                    &shape.path,
                    PathRenderOptions {
                        fill_rule: match shape.fill_rule {
                            SvgFillRule::NonZero => lyon::tessellation::FillRule::NonZero,
                            SvgFillRule::EvenOdd => lyon::tessellation::FillRule::EvenOdd,
                        },
                        ..PathRenderOptions::default()
                    },
                    Some(GeometryPaint::Solid([0.0; 4])),
                    None,
                )?;
            }
            clip_ranges.push(start..buffers.indices.len() as u32);
        }
        let masked = !descriptors.is_empty();
        let mask_meta = if !masked {
            [0.0; 2]
        } else {
            let start = mask_indices.len() as u32;
            let count = descriptors.len() as f32;
            mask_indices.extend(descriptors);
            [start as f32, count]
        };
        let mut image_node = path_node.clone();
        image_node.transform = compose_matrix(path_node.transform, image.transform);
        image_node.style.opacity *= image.opacity;
        let width = image.pixel_width as f32;
        let height = image.pixel_height as f32;
        let corners = [[0.0, 0.0], [width, 0.0], [0.0, height], [width, height]];
        let start = image_vertices.len() as u32;
        append_image_vertices(image_vertices, frame, &image_node, &corners)?;
        for vertex in &mut image_vertices[start as usize..] {
            vertex.mask_meta = mask_meta;
        }
        for (depth, indices) in clip_ranges.iter().enumerate() {
            draw_commands.clip_push(indices.clone(), clip_reference_base + depth as u32);
        }
        draw_commands.image(
            start..image_vertices.len() as u32,
            svg_image_key(image),
            match image.resampling {
                SvgImageResampling::Nearest => ImageResampling::Nearest,
                SvgImageResampling::Linear => ImageResampling::Bilinear,
            },
            clip_reference_base + clip_ranges.len() as u32,
            masked,
        );
        for (depth, indices) in clip_ranges.iter().enumerate().rev() {
            draw_commands.clip_pop(indices.clone(), clip_reference_base + depth as u32 + 1);
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn append_svg_regular_paint(
        buffers: &mut VertexBuffers<Vertex, u32>,
        gradient_stops: &mut Vec<GpuGradientStop>,
        draw_commands: &mut Vec<DrawCommand>,
        frame: &EvaluatedFrameView<'_>,
        path_node: &EvaluatedNodeView<'_>,
        path: &Path,
        options: PathRenderOptions,
        clip_ranges: &[Range<u32>],
        mask_meta: [f32; 2],
        masked: bool,
        fill: Option<&SvgPaint>,
        stroke: Option<&SvgPaint>,
        opacity: f32,
    ) -> Result<(), String> {
        let content_start = buffers.indices.len() as u32;
        let content_vertex_start = buffers.vertices.len();
        append_path_with_registered_paints(
            buffers,
            frame,
            path_node,
            path,
            options,
            register_svg_paint(gradient_stops, fill, opacity),
            register_svg_paint(gradient_stops, stroke, opacity),
        )?;
        let content_end = buffers.indices.len() as u32;
        if content_end <= content_start {
            return Ok(());
        }
        for vertex in &mut buffers.vertices[content_vertex_start..] {
            vertex.mask_meta = mask_meta;
        }
        if clip_ranges.is_empty() && !masked {
            draw_commands.push(DrawCommand::Vector {
                indices: content_start..content_end,
                depth_test: false,
                transparent_3d: false,
            });
            return Ok(());
        }
        for (depth, indices) in clip_ranges.iter().enumerate() {
            draw_commands.push(DrawCommand::ClipPush {
                indices: indices.clone(),
                reference: depth as u32,
            });
        }
        if masked {
            draw_commands.push(DrawCommand::MaskedVector {
                indices: content_start..content_end,
                reference: clip_ranges.len() as u32,
            });
        } else {
            draw_commands.push(DrawCommand::ClippedVector {
                indices: content_start..content_end,
                reference: clip_ranges.len() as u32,
            });
        }
        for (depth, indices) in clip_ranges.iter().enumerate().rev() {
            draw_commands.push(DrawCommand::ClipPop {
                indices: indices.clone(),
                reference: depth as u32 + 1,
            });
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn append_svg_regular_paint_at_reference<S: SvgCommandSink>(
        buffers: &mut VertexBuffers<Vertex, u32>,
        gradient_stops: &mut Vec<GpuGradientStop>,
        draw_commands: &mut S,
        frame: &EvaluatedFrameView<'_>,
        path_node: &EvaluatedNodeView<'_>,
        path: &Path,
        options: PathRenderOptions,
        mask_meta: [f32; 2],
        masked: bool,
        fill: Option<&SvgPaint>,
        stroke: Option<&SvgPaint>,
        opacity: f32,
        reference: u32,
    ) -> Result<(), String> {
        let content_start = buffers.indices.len() as u32;
        let content_vertex_start = buffers.vertices.len();
        append_path_with_registered_paints(
            buffers,
            frame,
            path_node,
            path,
            options,
            register_svg_paint(gradient_stops, fill, opacity),
            register_svg_paint(gradient_stops, stroke, opacity),
        )?;
        let content_end = buffers.indices.len() as u32;
        if content_end <= content_start {
            return Ok(());
        }
        for vertex in &mut buffers.vertices[content_vertex_start..] {
            vertex.mask_meta = mask_meta;
        }
        draw_commands.vector(content_start..content_end, reference, masked);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn append_svg_pattern_fill<S: SvgCommandSink>(
        buffers: &mut VertexBuffers<Vertex, u32>,
        gradient_stops: &mut Vec<GpuGradientStop>,
        draw_commands: &mut S,
        image_vertices: &mut Vec<ImageVertex>,
        mask_layers: &mut Vec<MaskLayerGeometry>,
        mask_indices: &mut Vec<u32>,
        frame: &EvaluatedFrameView<'_>,
        path_node: &EvaluatedNodeView<'_>,
        owner_path: &Path,
        owner_options: PathRenderOptions,
        outer_clip_ranges: &[Range<u32>],
        mask_meta: [f32; 2],
        masked: bool,
        pattern: &SvgPattern,
        document_width: f32,
        document_height: f32,
        opacity: f32,
        stroke_pattern: bool,
        clip_reference_base: u32,
        pattern_depth: u8,
    ) -> Result<(), String> {
        if pattern_depth >= 32 {
            return Err(format!(
                "SVG {} exceeds 32 recursively nested pattern paints.",
                path_node.id
            ));
        }
        let [tile_x, tile_y, tile_width, tile_height] = pattern.rect;
        if tile_width <= f32::EPSILON || tile_height <= f32::EPSILON {
            return Err(format!(
                "SVG {} contains an empty pattern tile.",
                path_node.id
            ));
        }
        for child in &pattern.paths {
            if child.clips.len() >= 253 {
                return Err(format!(
                    "SVG {} pattern content exceeds 252 nested clip paths.",
                    path_node.id
                ));
            }
            if clip_reference_base + outer_clip_ranges.len() as u32 + child.clips.len() as u32 + 2
                >= 255
            {
                return Err(format!(
                    "SVG {} exceeds 254 nested stencil levels inside pattern content.",
                    path_node.id
                ));
            }
        }

        let inverse = invert_matrix(pattern.transform).ok_or_else(|| {
            format!(
                "SVG {} contains a singular pattern transform.",
                path_node.id
            )
        })?;
        let expansion = stroke_pattern.then_some(path_node.style.stroke_width * 0.5);
        let coverage = path_coordinate_bounds(owner_path, expansion).unwrap_or([
            0.0,
            0.0,
            document_width,
            document_height,
        ]);
        let document_corners = [
            [coverage[0], coverage[1]],
            [coverage[2], coverage[1]],
            [coverage[2], coverage[3]],
            [coverage[0], coverage[3]],
        ];
        let mut minimum = [f32::INFINITY; 2];
        let mut maximum = [f32::NEG_INFINITY; 2];
        for corner in document_corners {
            let corner = apply_matrix(inverse, corner);
            minimum[0] = minimum[0].min(corner[0]);
            minimum[1] = minimum[1].min(corner[1]);
            maximum[0] = maximum[0].max(corner[0]);
            maximum[1] = maximum[1].max(corner[1]);
        }
        let column_start = ((minimum[0] - tile_x) / tile_width).floor() as i32 - 1;
        let column_end = ((maximum[0] - tile_x) / tile_width).ceil() as i32 + 1;
        let row_start = ((minimum[1] - tile_y) / tile_height).floor() as i32 - 1;
        let row_end = ((maximum[1] - tile_y) / tile_height).ceil() as i32 + 1;
        let columns = i64::from(column_end - column_start + 1);
        let rows = i64::from(row_end - row_start + 1);
        if columns <= 0 || rows <= 0 || columns.saturating_mul(rows) > 16_384 {
            return Err(format!(
                "SVG {} pattern would require more than 16,384 visible vector tiles.",
                path_node.id
            ));
        }

        let outer_mask_descriptors = if masked {
            let start = mask_meta[0] as usize;
            let count = mask_meta[1] as usize;
            let end = start.saturating_add(count);
            if end > mask_indices.len() {
                return Err(format!(
                    "SVG {} contains invalid outer mask metadata.",
                    path_node.id
                ));
            }
            mask_indices[start..end].to_vec()
        } else {
            Vec::new()
        };

        let owner_clip_start = buffers.indices.len() as u32;
        append_path_with_registered_paints(
            buffers,
            frame,
            path_node,
            owner_path,
            owner_options,
            (!stroke_pattern).then_some(GeometryPaint::Solid([0.0; 4])),
            stroke_pattern.then_some(GeometryPaint::Solid([0.0; 4])),
        )?;
        let owner_clip_range = owner_clip_start..buffers.indices.len() as u32;
        if owner_clip_range.is_empty() {
            return Ok(());
        }

        for (depth, indices) in outer_clip_ranges.iter().enumerate() {
            draw_commands.clip_push(indices.clone(), clip_reference_base + depth as u32);
        }
        draw_commands.clip_push(
            owner_clip_range.clone(),
            clip_reference_base + outer_clip_ranges.len() as u32,
        );

        let tile_path = rectangle_path(tile_x, tile_y, tile_width, tile_height);
        for row in row_start..=row_end {
            for column in column_start..=column_end {
                let offset_x = column as f32 * tile_width;
                let offset_y = row as f32 * tile_height;
                let pattern_matrix = translate_matrix(pattern.transform, offset_x, offset_y);
                let mut tile_node = path_node.clone();
                tile_node.transform = compose_matrix(path_node.transform, pattern_matrix);

                let tile_clip_start = buffers.indices.len() as u32;
                append_path_with_registered_paints(
                    buffers,
                    frame,
                    &tile_node,
                    &tile_path,
                    PathRenderOptions::default(),
                    Some(GeometryPaint::Solid([0.0; 4])),
                    None,
                )?;
                let tile_clip_range = tile_clip_start..buffers.indices.len() as u32;

                let tile_depth = clip_reference_base + outer_clip_ranges.len() as u32 + 1;
                draw_commands.clip_push(tile_clip_range.clone(), tile_depth);
                let mut tile_mask_layer_by_key = HashMap::new();
                for element in &pattern.order {
                    let SvgElementRef::Path(child_index) = element else {
                        let SvgElementRef::Image(image_index) = element else {
                            unreachable!();
                        };
                        append_svg_raster_image(
                            buffers,
                            gradient_stops,
                            image_vertices,
                            draw_commands,
                            mask_layers,
                            mask_indices,
                            frame,
                            &tile_node,
                            &pattern.images[*image_index],
                            tile_depth + 1,
                            mask_meta,
                            masked,
                        )?;
                        continue;
                    };
                    let child = &pattern.paths[*child_index];
                    let mut child_mask_descriptors = outer_mask_descriptors.clone();
                    for child_mask in &child.masks {
                        let layer = ensure_svg_mask_layer(
                            buffers,
                            gradient_stops,
                            image_vertices,
                            mask_layers,
                            mask_indices,
                            &mut tile_mask_layer_by_key,
                            frame,
                            &tile_node,
                            child_mask,
                        )?;
                        child_mask_descriptors.push(match child_mask.kind {
                            SvgMaskType::Alpha => layer,
                            SvgMaskType::Luminance => layer | 0x8000_0000,
                        });
                    }
                    let mut child_clip_ranges = Vec::with_capacity(child.clips.len());
                    for clip in &child.clips {
                        if svg_clip_is_complex(clip) {
                            child_mask_descriptors.push(ensure_svg_clip_layer(
                                buffers,
                                mask_layers,
                                mask_indices,
                                frame,
                                &tile_node,
                                clip,
                            )?);
                            continue;
                        }
                        let clip_start = buffers.indices.len() as u32;
                        for shape in &clip.shapes {
                            append_path_with_registered_paints(
                                buffers,
                                frame,
                                &tile_node,
                                &shape.path,
                                PathRenderOptions {
                                    fill_rule: match shape.fill_rule {
                                        SvgFillRule::NonZero => {
                                            lyon::tessellation::FillRule::NonZero
                                        }
                                        SvgFillRule::EvenOdd => {
                                            lyon::tessellation::FillRule::EvenOdd
                                        }
                                    },
                                    ..PathRenderOptions::default()
                                },
                                Some(GeometryPaint::Solid([0.0; 4])),
                                None,
                            )?;
                        }
                        child_clip_ranges.push(clip_start..buffers.indices.len() as u32);
                    }
                    let child_masked = !child_mask_descriptors.is_empty();
                    let child_mask_meta = if child_mask_descriptors == outer_mask_descriptors {
                        mask_meta
                    } else {
                        let start = mask_indices.len() as u32;
                        let count = child_mask_descriptors.len() as f32;
                        mask_indices.extend(child_mask_descriptors);
                        [start as f32, count]
                    };

                    let mut child_node = tile_node.clone();
                    child_node.style.opacity = 1.0;
                    child_node.style.stroke_width = child.stroke_width;
                    child_node.style.fill = None;
                    child_node.style.fill_gradient = None;
                    child_node.style.stroke = None;
                    child_node.style.stroke_gradient = None;
                    let child_options = PathRenderOptions {
                        fill_rule: match child.fill_rule {
                            SvgFillRule::NonZero => lyon::tessellation::FillRule::NonZero,
                            SvgFillRule::EvenOdd => lyon::tessellation::FillRule::EvenOdd,
                        },
                        line_cap: match child.line_cap {
                            SvgLineCap::Butt => lyon::tessellation::LineCap::Butt,
                            SvgLineCap::Round => lyon::tessellation::LineCap::Round,
                            SvgLineCap::Square => lyon::tessellation::LineCap::Square,
                        },
                        line_join: match child.line_join {
                            SvgLineJoin::Miter => lyon::tessellation::LineJoin::Miter,
                            SvgLineJoin::Round => lyon::tessellation::LineJoin::Round,
                            SvgLineJoin::Bevel => lyon::tessellation::LineJoin::Bevel,
                        },
                    };
                    for (depth, indices) in child_clip_ranges.iter().enumerate() {
                        draw_commands.clip_push(indices.clone(), tile_depth + 1 + depth as u32);
                    }
                    let reference = tile_depth + 1 + child_clip_ranges.len() as u32;
                    match child.fill.as_ref() {
                        Some(SvgPaint::Pattern(child_pattern)) => append_svg_pattern_fill(
                            buffers,
                            gradient_stops,
                            draw_commands,
                            image_vertices,
                            mask_layers,
                            mask_indices,
                            frame,
                            &child_node,
                            &child.path,
                            child_options,
                            &[],
                            child_mask_meta,
                            child_masked,
                            child_pattern,
                            document_width,
                            document_height,
                            opacity,
                            false,
                            reference,
                            pattern_depth + 1,
                        )?,
                        paint => append_svg_regular_paint_at_reference(
                            buffers,
                            gradient_stops,
                            draw_commands,
                            frame,
                            &child_node,
                            &child.path,
                            child_options,
                            child_mask_meta,
                            child_masked,
                            paint,
                            None,
                            opacity,
                            reference,
                        )?,
                    }
                    match child.stroke.as_ref() {
                        Some(SvgPaint::Pattern(child_pattern)) => append_svg_pattern_fill(
                            buffers,
                            gradient_stops,
                            draw_commands,
                            image_vertices,
                            mask_layers,
                            mask_indices,
                            frame,
                            &child_node,
                            &child.path,
                            child_options,
                            &[],
                            child_mask_meta,
                            child_masked,
                            child_pattern,
                            document_width,
                            document_height,
                            opacity,
                            true,
                            reference,
                            pattern_depth + 1,
                        )?,
                        paint => append_svg_regular_paint_at_reference(
                            buffers,
                            gradient_stops,
                            draw_commands,
                            frame,
                            &child_node,
                            &child.path,
                            child_options,
                            child_mask_meta,
                            child_masked,
                            None,
                            paint,
                            opacity,
                            reference,
                        )?,
                    }
                    for (depth, indices) in child_clip_ranges.iter().enumerate().rev() {
                        draw_commands.clip_pop(indices.clone(), tile_depth + 2 + depth as u32);
                    }
                }
                draw_commands.clip_pop(tile_clip_range, tile_depth + 1);
            }
        }

        draw_commands.clip_pop(
            owner_clip_range,
            clip_reference_base + outer_clip_ranges.len() as u32 + 1,
        );
        for (depth, indices) in outer_clip_ranges.iter().enumerate().rev() {
            draw_commands.clip_pop(indices.clone(), clip_reference_base + depth as u32 + 1);
        }
        Ok(())
    }

    fn rectangle_path(x: f32, y: f32, width: f32, height: f32) -> Path {
        let mut builder = Path::builder();
        builder.begin(point(x, y));
        builder.line_to(point(x + width, y));
        builder.line_to(point(x + width, y + height));
        builder.line_to(point(x, y + height));
        builder.close();
        builder.build()
    }

    fn path_coordinate_bounds(path: &Path, expansion: Option<f32>) -> Option<[f32; 4]> {
        let mut bounds = [
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        ];
        let mut include = |point: lyon::math::Point| {
            bounds[0] = bounds[0].min(point.x);
            bounds[1] = bounds[1].min(point.y);
            bounds[2] = bounds[2].max(point.x);
            bounds[3] = bounds[3].max(point.y);
        };
        for event in path.iter() {
            match event {
                PathEvent::Begin { at } => include(at),
                PathEvent::Line { from, to } => {
                    include(from);
                    include(to);
                }
                PathEvent::Quadratic { from, ctrl, to } => {
                    include(from);
                    include(ctrl);
                    include(to);
                }
                PathEvent::Cubic {
                    from,
                    ctrl1,
                    ctrl2,
                    to,
                } => {
                    include(from);
                    include(ctrl1);
                    include(ctrl2);
                    include(to);
                }
                PathEvent::End { last, first, .. } => {
                    include(last);
                    include(first);
                }
            }
        }
        if !bounds.iter().all(|value| value.is_finite()) {
            return None;
        }
        let expansion = expansion.unwrap_or(0.0).max(0.0);
        Some([
            bounds[0] - expansion,
            bounds[1] - expansion,
            bounds[2] + expansion,
            bounds[3] + expansion,
        ])
    }

    fn svg_clip_is_complex(clip: &SvgClip) -> bool {
        clip.shapes.iter().any(|shape| !shape.clips.is_empty())
    }

    #[allow(clippy::too_many_arguments)]
    fn ensure_svg_clip_layer(
        buffers: &mut VertexBuffers<Vertex, u32>,
        mask_layers: &mut Vec<MaskLayerGeometry>,
        mask_indices: &mut Vec<u32>,
        frame: &EvaluatedFrameView<'_>,
        path_node: &EvaluatedNodeView<'_>,
        clip: &SvgClip,
    ) -> Result<u32, String> {
        let mut commands = Vec::new();
        for shape in &clip.shapes {
            let mut nested_layers = Vec::with_capacity(shape.clips.len());
            for nested_clip in &shape.clips {
                nested_layers.push(ensure_svg_clip_layer(
                    buffers,
                    mask_layers,
                    mask_indices,
                    frame,
                    path_node,
                    nested_clip,
                )?);
            }
            let mask_meta = if nested_layers.is_empty() {
                [0.0; 2]
            } else {
                let start = mask_indices.len() as u32;
                let count = nested_layers.len() as f32;
                mask_indices.extend(nested_layers);
                [start as f32, count]
            };
            let content_start = buffers.indices.len() as u32;
            let content_vertex_start = buffers.vertices.len();
            append_path_with_registered_paints(
                buffers,
                frame,
                path_node,
                &shape.path,
                PathRenderOptions {
                    fill_rule: match shape.fill_rule {
                        SvgFillRule::NonZero => lyon::tessellation::FillRule::NonZero,
                        SvgFillRule::EvenOdd => lyon::tessellation::FillRule::EvenOdd,
                    },
                    ..PathRenderOptions::default()
                },
                Some(GeometryPaint::Solid([1.0; 4])),
                None,
            )?;
            let content_end = buffers.indices.len() as u32;
            if content_end <= content_start {
                continue;
            }
            for vertex in &mut buffers.vertices[content_vertex_start..] {
                vertex.mask_meta = mask_meta;
            }
            commands.vector(content_start..content_end, 0, !shape.clips.is_empty());
        }
        if mask_layers.len() >= 256 {
            return Err(format!(
                "SVG {} exceeds the WebGPU limit of 256 active mask layers.",
                path_node.id
            ));
        }
        let layer = mask_layers.len() as u32;
        mask_layers.push(MaskLayerGeometry { commands });
        Ok(layer)
    }

    #[allow(clippy::too_many_arguments)]
    fn ensure_svg_mask_layer(
        buffers: &mut VertexBuffers<Vertex, u32>,
        gradient_stops: &mut Vec<GpuGradientStop>,
        image_vertices: &mut Vec<ImageVertex>,
        mask_layers: &mut Vec<MaskLayerGeometry>,
        mask_indices: &mut Vec<u32>,
        mask_layer_by_key: &mut HashMap<u32, u32>,
        frame: &EvaluatedFrameView<'_>,
        path_node: &EvaluatedNodeView<'_>,
        mask: &SvgMask,
    ) -> Result<u32, String> {
        if let Some(layer) = mask_layer_by_key.get(&mask.key) {
            return Ok(*layer);
        }
        let mut commands = Vec::new();
        let region_start = buffers.indices.len() as u32;
        append_path_with_registered_paints(
            buffers,
            frame,
            path_node,
            &mask.region.path,
            PathRenderOptions {
                fill_rule: match mask.region.fill_rule {
                    SvgFillRule::NonZero => lyon::tessellation::FillRule::NonZero,
                    SvgFillRule::EvenOdd => lyon::tessellation::FillRule::EvenOdd,
                },
                ..PathRenderOptions::default()
            },
            Some(GeometryPaint::Solid([1.0; 4])),
            None,
        )?;
        let region_range = region_start..buffers.indices.len() as u32;
        commands.push(MaskDrawCommand::ClipPush {
            indices: region_range.clone(),
            reference: 0,
        });

        for element in &mask.order {
            let SvgElementRef::Path(path_index) = element else {
                let SvgElementRef::Image(image_index) = element else {
                    unreachable!();
                };
                append_svg_raster_image(
                    buffers,
                    gradient_stops,
                    image_vertices,
                    &mut commands,
                    mask_layers,
                    mask_indices,
                    frame,
                    path_node,
                    &mask.images[*image_index],
                    1,
                    [0.0; 2],
                    false,
                )?;
                continue;
            };
            let mask_path = &mask.paths[*path_index];
            if mask_path.clips.len() >= 255 {
                return Err("SVG mask content exceeds 254 nested clip paths.".to_owned());
            }
            let mut content_mask_layers = Vec::with_capacity(mask_path.masks.len());
            for nested_mask in &mask_path.masks {
                let layer = ensure_svg_mask_layer(
                    buffers,
                    gradient_stops,
                    image_vertices,
                    mask_layers,
                    mask_indices,
                    mask_layer_by_key,
                    frame,
                    path_node,
                    nested_mask,
                )?;
                content_mask_layers.push(match nested_mask.kind {
                    SvgMaskType::Alpha => layer,
                    SvgMaskType::Luminance => layer | 0x8000_0000,
                });
            }
            let mut clip_ranges = Vec::with_capacity(mask_path.clips.len());
            for clip in &mask_path.clips {
                if svg_clip_is_complex(clip) {
                    content_mask_layers.push(ensure_svg_clip_layer(
                        buffers,
                        mask_layers,
                        mask_indices,
                        frame,
                        path_node,
                        clip,
                    )?);
                    continue;
                }
                let start = buffers.indices.len() as u32;
                for shape in &clip.shapes {
                    append_path_with_registered_paints(
                        buffers,
                        frame,
                        path_node,
                        &shape.path,
                        PathRenderOptions {
                            fill_rule: match shape.fill_rule {
                                SvgFillRule::NonZero => lyon::tessellation::FillRule::NonZero,
                                SvgFillRule::EvenOdd => lyon::tessellation::FillRule::EvenOdd,
                            },
                            ..PathRenderOptions::default()
                        },
                        Some(GeometryPaint::Solid([0.0; 4])),
                        None,
                    )?;
                }
                clip_ranges.push(start..buffers.indices.len() as u32);
            }
            let content_masked = !content_mask_layers.is_empty();
            let mask_meta = if content_masked {
                let start = mask_indices.len() as u32;
                let count = content_mask_layers.len() as f32;
                mask_indices.extend(content_mask_layers);
                [start as f32, count]
            } else {
                [0.0; 2]
            };

            let mut styled_node = path_node.clone();
            styled_node.style.opacity = 1.0;
            styled_node.style.stroke_width = mask_path.stroke_width;
            styled_node.style.fill = None;
            styled_node.style.fill_gradient = None;
            styled_node.style.stroke = None;
            styled_node.style.stroke_gradient = None;
            let options = PathRenderOptions {
                fill_rule: match mask_path.fill_rule {
                    SvgFillRule::NonZero => lyon::tessellation::FillRule::NonZero,
                    SvgFillRule::EvenOdd => lyon::tessellation::FillRule::EvenOdd,
                },
                line_cap: match mask_path.line_cap {
                    SvgLineCap::Butt => lyon::tessellation::LineCap::Butt,
                    SvgLineCap::Round => lyon::tessellation::LineCap::Round,
                    SvgLineCap::Square => lyon::tessellation::LineCap::Square,
                },
                line_join: match mask_path.line_join {
                    SvgLineJoin::Miter => lyon::tessellation::LineJoin::Miter,
                    SvgLineJoin::Round => lyon::tessellation::LineJoin::Round,
                    SvgLineJoin::Bevel => lyon::tessellation::LineJoin::Bevel,
                },
            };
            for (depth, indices) in clip_ranges.iter().enumerate() {
                commands.clip_push(indices.clone(), depth as u32 + 1);
            }
            let reference = clip_ranges.len() as u32 + 1;
            match mask_path.fill.as_ref() {
                Some(SvgPaint::Pattern(pattern)) => append_svg_pattern_fill(
                    buffers,
                    gradient_stops,
                    &mut commands,
                    image_vertices,
                    mask_layers,
                    mask_indices,
                    frame,
                    &styled_node,
                    &mask_path.path,
                    options,
                    &[],
                    mask_meta,
                    content_masked,
                    pattern,
                    1.0,
                    1.0,
                    1.0,
                    false,
                    reference,
                    0,
                )?,
                paint => append_svg_regular_paint_at_reference(
                    buffers,
                    gradient_stops,
                    &mut commands,
                    frame,
                    &styled_node,
                    &mask_path.path,
                    options,
                    mask_meta,
                    content_masked,
                    paint,
                    None,
                    1.0,
                    reference,
                )?,
            }
            match mask_path.stroke.as_ref() {
                Some(SvgPaint::Pattern(pattern)) => append_svg_pattern_fill(
                    buffers,
                    gradient_stops,
                    &mut commands,
                    image_vertices,
                    mask_layers,
                    mask_indices,
                    frame,
                    &styled_node,
                    &mask_path.path,
                    options,
                    &[],
                    mask_meta,
                    content_masked,
                    pattern,
                    1.0,
                    1.0,
                    1.0,
                    true,
                    reference,
                    0,
                )?,
                paint => append_svg_regular_paint_at_reference(
                    buffers,
                    gradient_stops,
                    &mut commands,
                    frame,
                    &styled_node,
                    &mask_path.path,
                    options,
                    mask_meta,
                    content_masked,
                    None,
                    paint,
                    1.0,
                    reference,
                )?,
            }
            for (depth, indices) in clip_ranges.iter().enumerate().rev() {
                commands.clip_pop(indices.clone(), depth as u32 + 2);
            }
        }
        commands.push(MaskDrawCommand::ClipPop {
            indices: region_range,
            reference: 1,
        });
        if mask_layers.len() >= 256 {
            return Err(format!(
                "SVG {} exceeds the WebGPU limit of 256 active mask layers.",
                path_node.id
            ));
        }
        let layer = mask_layers.len() as u32;
        mask_layers.push(MaskLayerGeometry { commands });
        mask_layer_by_key.insert(mask.key, layer);
        Ok(layer)
    }

    fn append_circle(
        buffers: &mut VertexBuffers<Vertex, u32>,
        gradient_stops: &mut Vec<GpuGradientStop>,
        frame: &EvaluatedFrameView<'_>,
        node: &EvaluatedNodeView<'_>,
        radius: f32,
    ) -> Result<(), String> {
        let segments = 64;
        let points: Vec<[f32; 2]> = (0..segments)
            .map(|index| {
                let angle = index as f32 / segments as f32 * TAU;
                [angle.cos() * radius, angle.sin() * radius]
            })
            .collect();
        append_polyline(buffers, gradient_stops, frame, node, &points, true)
    }

    fn append_arrow(
        buffers: &mut VertexBuffers<Vertex, u32>,
        gradient_stops: &mut Vec<GpuGradientStop>,
        frame: &EvaluatedFrameView<'_>,
        node: &EvaluatedNodeView<'_>,
        from: [f32; 2],
        to: [f32; 2],
        tip_size: f32,
    ) -> Result<(), String> {
        append_polyline(buffers, gradient_stops, frame, node, &[from, to], false)?;
        let dx = to[0] - from[0];
        let dy = to[1] - from[1];
        let length = (dx * dx + dy * dy).sqrt().max(f32::EPSILON);
        let ux = dx / length;
        let uy = dy / length;
        let perpendicular = [-uy, ux];
        let base = [to[0] - ux * tip_size, to[1] - uy * tip_size];
        let points = [
            to,
            [
                base[0] + perpendicular[0] * tip_size * 0.55,
                base[1] + perpendicular[1] * tip_size * 0.55,
            ],
            [
                base[0] - perpendicular[0] * tip_size * 0.55,
                base[1] - perpendicular[1] * tip_size * 0.55,
            ],
        ];
        let mut tip_node = node.clone();
        tip_node.style.fill = node.style.stroke.or(node.style.fill);
        tip_node.style.fill_gradient = None;
        tip_node.style.stroke = None;
        tip_node.style.stroke_gradient = None;
        append_polyline(buffers, gradient_stops, frame, &tip_node, &points, true)
    }

    fn append_polyline(
        buffers: &mut VertexBuffers<Vertex, u32>,
        gradient_stops: &mut Vec<GpuGradientStop>,
        frame: &EvaluatedFrameView<'_>,
        node: &EvaluatedNodeView<'_>,
        points: &[[f32; 2]],
        closed: bool,
    ) -> Result<(), String> {
        let visible_points = partial_points(
            points,
            node.style.draw_start,
            node.style.draw_progress,
            closed,
        );
        if visible_points.len() < 2 {
            return Ok(());
        }
        let mut builder = Path::builder();
        builder.begin(point(visible_points[0][0], visible_points[0][1]));
        for next in visible_points.iter().skip(1) {
            builder.line_to(point(next[0], next[1]));
        }
        if closed && node.style.draw_start <= 0.001 && node.style.draw_progress >= 0.999 {
            builder.close();
        } else {
            builder.end(false);
        }
        append_path(buffers, gradient_stops, frame, node, &builder.build())
    }

    fn append_path(
        buffers: &mut VertexBuffers<Vertex, u32>,
        gradient_stops: &mut Vec<GpuGradientStop>,
        frame: &EvaluatedFrameView<'_>,
        node: &EvaluatedNodeView<'_>,
        path: &Path,
    ) -> Result<(), String> {
        let options = PathRenderOptions {
            line_cap: match node.style.stroke_cap {
                StrokeCap::Butt => lyon::tessellation::LineCap::Butt,
                StrokeCap::Square => lyon::tessellation::LineCap::Square,
                StrokeCap::Round => lyon::tessellation::LineCap::Round,
            },
            line_join: match node.style.stroke_join {
                StrokeJoin::Miter => lyon::tessellation::LineJoin::Miter,
                StrokeJoin::MiterClip => lyon::tessellation::LineJoin::MiterClip,
                StrokeJoin::Round => lyon::tessellation::LineJoin::Round,
                StrokeJoin::Bevel => lyon::tessellation::LineJoin::Bevel,
            },
            ..PathRenderOptions::default()
        };
        append_path_with_options(buffers, gradient_stops, frame, node, path, options)
    }

    #[derive(Clone, Copy)]
    struct PathRenderOptions {
        fill_rule: lyon::tessellation::FillRule,
        line_cap: lyon::tessellation::LineCap,
        line_join: lyon::tessellation::LineJoin,
    }

    impl Default for PathRenderOptions {
        fn default() -> Self {
            Self {
                fill_rule: lyon::tessellation::FillRule::NonZero,
                line_cap: lyon::tessellation::LineCap::Butt,
                line_join: lyon::tessellation::LineJoin::Miter,
            }
        }
    }

    fn append_path_with_options(
        buffers: &mut VertexBuffers<Vertex, u32>,
        gradient_stops: &mut Vec<GpuGradientStop>,
        frame: &EvaluatedFrameView<'_>,
        node: &EvaluatedNodeView<'_>,
        path: &Path,
        options: PathRenderOptions,
    ) -> Result<(), String> {
        let fill_paint = node
            .style
            .fill_gradient
            .as_ref()
            .map(|gradient| register_gradient(gradient_stops, gradient))
            .or(node.style.fill.map(GeometryPaint::Solid));
        let stroke_paint = node
            .style
            .stroke_gradient
            .as_ref()
            .map(|gradient| register_gradient(gradient_stops, gradient))
            .or(node.style.stroke.map(GeometryPaint::Solid));
        append_path_with_registered_paints(
            buffers,
            frame,
            node,
            path,
            options,
            fill_paint,
            stroke_paint,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn append_path_with_registered_paints(
        buffers: &mut VertexBuffers<Vertex, u32>,
        frame: &EvaluatedFrameView<'_>,
        node: &EvaluatedNodeView<'_>,
        path: &Path,
        options: PathRenderOptions,
        fill_paint: Option<GeometryPaint>,
        stroke_paint: Option<GeometryPaint>,
    ) -> Result<(), String> {
        let tolerance = screen_space_curve_tolerance(frame, node.transform);
        if let Some(paint) = fill_paint {
            let mut tessellator = FillTessellator::new();
            tessellator
                .tessellate_path(
                    path,
                    &FillOptions::default()
                        .with_fill_rule(options.fill_rule)
                        .with_tolerance(tolerance),
                    &mut BuffersBuilder::new(
                        buffers,
                        GeometryConstructor {
                            frame,
                            matrix: node.transform,
                            paint_matrix: if paint.world_space() {
                                node.transform
                            } else {
                                [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]
                            },
                            paint,
                        },
                    ),
                )
                .map_err(|error| format!("Fill tessellation failed for {}: {error}.", node.id))?;
        }
        if let Some(paint) = stroke_paint
            && node.style.stroke_width > 0.0
        {
            let inverse = invert_matrix(node.transform);
            let world_space_stroke = node.stroke_in_world_space
                && (matches!(paint, GeometryPaint::Solid(_))
                    || paint.world_space()
                    || inverse.is_some());
            let transformed_path = world_space_stroke.then(|| {
                path.clone().transformed(&lyon::geom::Transform::new(
                    node.transform[0],
                    node.transform[1],
                    node.transform[2],
                    node.transform[3],
                    node.transform[4],
                    node.transform[5],
                ))
            });
            let base_stroke_path = transformed_path.as_ref().unwrap_or(path);
            let dashed_path = build_dashed_path(
                base_stroke_path,
                &node.style.dash_array,
                node.style.dash_offset,
                tolerance,
            )?;
            let stroke_path = dashed_path.as_ref().unwrap_or(base_stroke_path);
            let stroke_matrix = if world_space_stroke {
                [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]
            } else {
                node.transform
            };
            let mut tessellator = StrokeTessellator::new();
            tessellator
                .tessellate_path(
                    stroke_path,
                    &StrokeOptions::default()
                        .with_line_width(node.style.stroke_width)
                        .with_line_cap(options.line_cap)
                        .with_line_join(options.line_join)
                        .with_tolerance(tolerance),
                    &mut BuffersBuilder::new(
                        buffers,
                        GeometryConstructor {
                            frame,
                            matrix: stroke_matrix,
                            paint_matrix: if paint.world_space() {
                                if world_space_stroke {
                                    [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]
                                } else {
                                    node.transform
                                }
                            } else if world_space_stroke {
                                inverse.unwrap_or([1.0, 0.0, 0.0, 1.0, 0.0, 0.0])
                            } else {
                                [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]
                            },
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
                    let subpath = mem::replace(&mut source_builder, Path::builder()).build();
                    source_active = false;
                    let measurements = PathMeasurements::from_path(&subpath, tolerance);
                    let length = measurements.length();
                    estimated_segments += length / minimum_segment;
                    if estimated_segments > 200_000.0 {
                        return Err(
                            "Dash pattern produces more than 200,000 path segments.".to_owned()
                        );
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

    fn screen_space_curve_tolerance(frame: &EvaluatedFrameView<'_>, matrix: [f32; 6]) -> f32 {
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

    #[allow(clippy::too_many_arguments)]
    fn append_text(
        buffers: &mut VertexBuffers<Vertex, u32>,
        gradient_stops: &mut Vec<GpuGradientStop>,
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
        let variant = font_variant(weight, slant);
        let layout = text_engine
            .layout_family_variant(
                text,
                font_size,
                align,
                1.25,
                0.0,
                FontSelection {
                    family: font_family,
                    variant,
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
            if glyph_node.style.fill.is_none() && glyph_node.style.fill_gradient.is_none() {
                glyph_node.style.fill = glyph_node.style.stroke;
                glyph_node.style.fill_gradient = glyph_node.style.stroke_gradient.clone();
                glyph_node.style.stroke = None;
                glyph_node.style.stroke_gradient = None;
            }
            append_path(buffers, gradient_stops, frame, &glyph_node, path)?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn append_markup_text(
        buffers: &mut VertexBuffers<Vertex, u32>,
        gradient_stops: &mut Vec<GpuGradientStop>,
        frame: &EvaluatedFrameView<'_>,
        node: &EvaluatedNodeView<'_>,
        spans: &[realtime_manim_scene_core::TextSpan],
        font_size: f32,
        font_family: &str,
        align: TextAlign,
        text_engine: &mut TextEngine,
    ) -> Result<(), String> {
        let shaped_spans = spans
            .iter()
            .map(|span| AttributedTextSpan {
                text: &span.text,
                variant: font_variant(span.weight, span.slant),
                font_family: span.font_family.as_deref(),
                font_scale: span.font_scale,
                rise: span.rise,
                letter_spacing: span.letter_spacing,
            })
            .collect::<Vec<_>>();
        let align = match align {
            TextAlign::Left => ShapedTextAlign::Left,
            TextAlign::Center => ShapedTextAlign::Center,
            TextAlign::Right => ShapedTextAlign::Right,
        };
        let layout = text_engine
            .layout_family_chain_attributed_spans(
                &shaped_spans,
                font_size,
                align,
                1.25,
                0.0,
                font_family,
                &[],
            )
            .map_err(|error| format!("Markup shaping failed for {}: {error}", node.id))?;
        let mut source_end = 0usize;
        let mut span_ends = Vec::with_capacity(spans.len());
        let mut span_paints = Vec::with_capacity(spans.len());
        for span in spans {
            source_end = source_end.saturating_add(span.text.len());
            span_ends.push(source_end);
            span_paints.push(MarkupPaint {
                foreground: span.color.as_deref().map(parse_color).transpose()?,
                background: span.background.as_deref().map(parse_color).transpose()?,
                underline: span.underline,
                underline_color: span
                    .underline_color
                    .as_deref()
                    .map(parse_color)
                    .transpose()?,
                strikethrough: span.strikethrough,
                strikethrough_color: span
                    .strikethrough_color
                    .as_deref()
                    .map(parse_color)
                    .transpose()?,
            });
        }
        let mut backgrounds: Vec<MarkupDecoration> = Vec::new();
        let mut decorations: Vec<MarkupDecoration> = Vec::new();
        for glyph in &layout.glyphs {
            let path = text_engine
                .glyph_outline_for_font(glyph.font_id, glyph.glyph_id, glyph.variant)
                .map_err(|error| format!("Markup outline failed for {}: {error}", node.id))?;
            // A ligature spanning paint boundaries uses the paint at its
            // source-cluster start; splitting the glyph would break shaping.
            let span_index = span_ends.partition_point(|end| *end <= glyph.source_start);
            let paint = span_paints.get(span_index).copied().unwrap_or_default();
            let ink_bounds = path.and_then(|path| path_coordinate_bounds(path, None));
            let logical_end = glyph.x + glyph.advance;
            let min_x = ink_bounds.map_or(glyph.x.min(logical_end), |bounds| {
                (glyph.x + bounds[0] * glyph.scale).min(glyph.x.min(logical_end))
            });
            let max_x = ink_bounds.map_or(glyph.x.max(logical_end), |bounds| {
                (glyph.x + bounds[2] * glyph.scale).max(glyph.x.max(logical_end))
            });
            let min_y = glyph.baseline + glyph.descender;
            let max_y = glyph.baseline + glyph.ascender;
            if let Some(color) = paint.background {
                push_markup_decoration(
                    &mut backgrounds,
                    MarkupDecorationKind::Background,
                    span_index,
                    min_x,
                    max_x,
                    min_y,
                    max_y,
                    color,
                );
            }
            let decoration_thickness = glyph.underline_thickness.max(0.001);
            let underline_y = match paint.underline {
                TextUnderline::None => None,
                TextUnderline::Low => Some(min_y - decoration_thickness * 1.5),
                TextUnderline::Single | TextUnderline::Double | TextUnderline::Error => {
                    Some(glyph.baseline + glyph.underline_position)
                }
            };
            if let Some(y) = underline_y {
                push_markup_decoration(
                    &mut decorations,
                    MarkupDecorationKind::Underline(paint.underline),
                    span_index,
                    min_x,
                    max_x,
                    y,
                    y + decoration_thickness,
                    paint
                        .underline_color
                        .or(paint.foreground)
                        .or(node.style.fill)
                        .or(node.style.stroke)
                        .unwrap_or([1.0; 4]),
                );
            }
            if paint.strikethrough {
                let y = glyph.baseline + glyph.strikeout_position;
                push_markup_decoration(
                    &mut decorations,
                    MarkupDecorationKind::Strikethrough,
                    span_index,
                    min_x,
                    max_x,
                    y,
                    y + glyph.strikeout_thickness.max(0.001),
                    paint
                        .strikethrough_color
                        .or(paint.foreground)
                        .or(node.style.fill)
                        .or(node.style.stroke)
                        .unwrap_or([1.0; 4]),
                );
            }
        }
        for background in backgrounds {
            append_markup_decoration(buffers, gradient_stops, frame, node, background, font_size)?;
        }
        for glyph in layout.glyphs {
            let Some(path) = text_engine
                .glyph_outline_for_font(glyph.font_id, glyph.glyph_id, glyph.variant)
                .map_err(|error| format!("Markup outline failed for {}: {error}", node.id))?
            else {
                continue;
            };
            let span_index = span_ends.partition_point(|end| *end <= glyph.source_start);
            let paint = span_paints.get(span_index).copied().unwrap_or_default();
            let mut glyph_node = node.clone();
            glyph_node.transform =
                local_matrix(node.transform, glyph.x, glyph.y, glyph.scale, glyph.scale);
            glyph_node.style.fill = paint
                .foreground
                .map(|mut color| {
                    color[3] *= node.style.opacity;
                    color
                })
                .or(node.style.fill)
                .or(node.style.stroke);
            glyph_node.style.fill_gradient = None;
            glyph_node.style.stroke = None;
            glyph_node.style.stroke_gradient = None;
            append_path(buffers, gradient_stops, frame, &glyph_node, path)?;
        }
        for decoration in decorations {
            append_markup_decoration(buffers, gradient_stops, frame, node, decoration, font_size)?;
        }
        Ok(())
    }

    #[derive(Clone, Copy, Default)]
    struct MarkupPaint {
        foreground: Option<[f32; 4]>,
        background: Option<[f32; 4]>,
        underline: TextUnderline,
        underline_color: Option<[f32; 4]>,
        strikethrough: bool,
        strikethrough_color: Option<[f32; 4]>,
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum MarkupDecorationKind {
        Background,
        Underline(TextUnderline),
        Strikethrough,
    }

    #[derive(Clone, Copy)]
    struct MarkupDecoration {
        kind: MarkupDecorationKind,
        span_index: usize,
        min_x: f32,
        max_x: f32,
        min_y: f32,
        max_y: f32,
        color: [f32; 4],
    }

    #[allow(clippy::too_many_arguments)]
    fn push_markup_decoration(
        decorations: &mut Vec<MarkupDecoration>,
        kind: MarkupDecorationKind,
        span_index: usize,
        min_x: f32,
        max_x: f32,
        min_y: f32,
        max_y: f32,
        color: [f32; 4],
    ) {
        if let Some(previous) = decorations.iter_mut().rev().find(|previous| {
            previous.kind == kind
                && previous.span_index == span_index
                && previous.color == color
                && ((previous.max_x - min_x).abs() < 0.05 || (max_x - previous.min_x).abs() < 0.05)
                && (previous.min_y - min_y).abs() < 0.05
                && (previous.max_y - max_y).abs() < 0.05
        }) {
            previous.max_x = previous.max_x.max(max_x);
            previous.min_x = previous.min_x.min(min_x);
            return;
        }
        decorations.push(MarkupDecoration {
            kind,
            span_index,
            min_x,
            max_x,
            min_y,
            max_y,
            color,
        });
    }

    fn append_markup_decoration(
        buffers: &mut VertexBuffers<Vertex, u32>,
        gradient_stops: &mut Vec<GpuGradientStop>,
        frame: &EvaluatedFrameView<'_>,
        node: &EvaluatedNodeView<'_>,
        decoration: MarkupDecoration,
        font_size: f32,
    ) -> Result<(), String> {
        let mut decoration_node = node.clone();
        decoration_node.style.fill = Some({
            let mut color = decoration.color;
            color[3] *= node.style.opacity;
            color
        });
        decoration_node.style.fill_gradient = None;
        decoration_node.style.stroke = None;
        decoration_node.style.stroke_gradient = None;
        let width = (decoration.max_x - decoration.min_x).max(0.001);
        let height = (decoration.max_y - decoration.min_y).max(0.001);
        let center_x = (decoration.min_x + decoration.max_x) * 0.5;
        let center_y = (decoration.min_y + decoration.max_y) * 0.5;
        let path = match decoration.kind {
            MarkupDecorationKind::Underline(TextUnderline::Error) => {
                error_underline_path(width, height, font_size)
            }
            _ => rect_path(width, height, 0.0),
        };
        decoration_node.transform = local_matrix(node.transform, center_x, center_y, 1.0, 1.0);
        append_path(buffers, gradient_stops, frame, &decoration_node, &path)?;
        if decoration.kind == MarkupDecorationKind::Underline(TextUnderline::Double) {
            decoration_node.transform =
                local_matrix(node.transform, center_x, center_y - height * 2.0, 1.0, 1.0);
            append_path(buffers, gradient_stops, frame, &decoration_node, &path)?;
        }
        Ok(())
    }

    fn error_underline_path(width: f32, height: f32, font_size: f32) -> Path {
        let amplitude = (height * 1.5).max(font_size * 0.035);
        let wavelength = (font_size * 0.18).max(0.04);
        let segments = ((width / (wavelength * 0.5)).ceil() as usize).clamp(2, 512);
        let mut builder = Path::builder().with_svg();
        builder.move_to(point(-width * 0.5, 0.0));
        for index in 1..=segments {
            let x = -width * 0.5 + width * index as f32 / segments as f32;
            let y = if index % 2 == 0 {
                -amplitude
            } else {
                amplitude
            };
            builder.line_to(point(x, y));
        }
        builder.line_to(point(width * 0.5, -amplitude + height));
        for index in (0..segments).rev() {
            let x = -width * 0.5 + width * index as f32 / segments as f32;
            let y = if index % 2 == 0 {
                -amplitude + height
            } else {
                amplitude + height
            };
            builder.line_to(point(x, y));
        }
        builder.close();
        builder.build()
    }

    fn font_variant(weight: FontWeight, slant: FontSlant) -> FontVariant {
        match (weight, slant) {
            (FontWeight::Normal, FontSlant::Normal) => FontVariant::Regular,
            (FontWeight::Bold, FontSlant::Normal) => FontVariant::Bold,
            (FontWeight::Normal, FontSlant::Italic) => FontVariant::Italic,
            (FontWeight::Bold, FontSlant::Italic) => FontVariant::BoldItalic,
        }
    }

    fn scene_text_span(span: &MarkupSpan) -> TextSpan {
        let (weight, slant) = match span.style.variant {
            FontVariant::Regular => (FontWeight::Normal, FontSlant::Normal),
            FontVariant::Bold => (FontWeight::Bold, FontSlant::Normal),
            FontVariant::Italic => (FontWeight::Normal, FontSlant::Italic),
            FontVariant::BoldItalic => (FontWeight::Bold, FontSlant::Italic),
        };
        TextSpan {
            text: span.text.clone(),
            color: span.style.foreground.clone(),
            weight,
            slant,
            font_family: span.style.font_family.clone(),
            font_scale: span.style.font_scale,
            rise: span.style.rise,
            letter_spacing: span.style.letter_spacing,
            background: span.style.background.clone(),
            underline: match span.style.underline {
                PangoUnderline::None => TextUnderline::None,
                PangoUnderline::Single => TextUnderline::Single,
                PangoUnderline::Double => TextUnderline::Double,
                PangoUnderline::Low => TextUnderline::Low,
                PangoUnderline::Error => TextUnderline::Error,
            },
            underline_color: span.style.underline_color.clone(),
            strikethrough: span.style.strikethrough,
            strikethrough_color: span.style.strikethrough_color.clone(),
        }
    }

    fn font_variant_labels(weight: &str, slant: &str) -> Result<FontVariant, JsValue> {
        match (weight, slant) {
            ("normal", "normal") => Ok(FontVariant::Regular),
            ("bold", "normal") => Ok(FontVariant::Bold),
            ("normal", "italic") => Ok(FontVariant::Italic),
            ("bold", "italic") => Ok(FontVariant::BoldItalic),
            _ => Err(js_error(
                "Font weight must be normal or bold and slant must be normal or italic.",
            )),
        }
    }

    #[derive(Clone, Copy)]
    enum GeometryPaint {
        Solid([f32; 4]),
        LinearGradient {
            from: [f32; 2],
            to: [f32; 2],
            start: u32,
            count: u32,
            spread: f32,
            world_space: bool,
        },
        RadialGradient {
            inverse_transform: [f32; 6],
            start: u32,
            count: u32,
            spread: f32,
        },
    }

    impl GeometryPaint {
        fn vertex_data(self, position: [f32; 2]) -> ([f32; 4], [f32; 2], [f32; 4]) {
            match self {
                Self::Solid(color) => (color, [0.0; 2], [0.0; 4]),
                Self::LinearGradient {
                    from,
                    to,
                    start,
                    count,
                    spread,
                    world_space: _,
                } => {
                    let axis = [to[0] - from[0], to[1] - from[1]];
                    let length_squared = axis[0] * axis[0] + axis[1] * axis[1];
                    let amount = if length_squared <= f32::EPSILON {
                        0.0
                    } else {
                        ((position[0] - from[0]) * axis[0] + (position[1] - from[1]) * axis[1])
                            / length_squared
                    };
                    (
                        [0.0; 4],
                        [amount, 0.0],
                        [start as f32, count as f32, spread, 0.0],
                    )
                }
                Self::RadialGradient {
                    inverse_transform,
                    start,
                    count,
                    spread,
                } => (
                    [0.0; 4],
                    apply_matrix(inverse_transform, position),
                    [start as f32, count as f32, spread, 1.0],
                ),
            }
        }

        fn world_space(self) -> bool {
            matches!(
                self,
                Self::LinearGradient {
                    world_space: true,
                    ..
                }
            )
        }
    }

    fn register_gradient(
        output: &mut Vec<GpuGradientStop>,
        gradient: &EvaluatedLinearGradient,
    ) -> GeometryPaint {
        let start = output.len() as u32;
        output.extend(gradient.stops.iter().map(|stop| GpuGradientStop {
            color: stop.color,
            data: [stop.offset, 0.0, 0.0, 0.0],
        }));
        GeometryPaint::LinearGradient {
            from: gradient.from,
            to: gradient.to,
            start,
            count: gradient.stops.len() as u32,
            spread: gradient_spread_index(gradient.spread),
            world_space: gradient.space == GradientSpace::World,
        }
    }

    fn register_svg_paint(
        output: &mut Vec<GpuGradientStop>,
        paint: Option<&SvgPaint>,
        opacity: f32,
    ) -> Option<GeometryPaint> {
        match paint? {
            SvgPaint::Solid(color) => {
                let mut color = *color;
                color[3] *= opacity;
                Some(GeometryPaint::Solid(color))
            }
            SvgPaint::LinearGradient(gradient) => {
                let start = output.len() as u32;
                output.extend(gradient.stops.iter().map(|stop| {
                    let mut color = stop.color;
                    color[3] *= opacity;
                    GpuGradientStop {
                        color,
                        data: [stop.offset, 0.0, 0.0, 0.0],
                    }
                }));
                Some(GeometryPaint::LinearGradient {
                    from: gradient.from,
                    to: gradient.to,
                    start,
                    count: gradient.stops.len() as u32,
                    spread: svg_gradient_spread_index(gradient.spread),
                    world_space: false,
                })
            }
            SvgPaint::RadialGradient(gradient) => {
                Some(register_svg_radial_gradient(output, gradient, opacity))
            }
            SvgPaint::Pattern(_) => None,
        }
    }

    fn register_svg_radial_gradient(
        output: &mut Vec<GpuGradientStop>,
        gradient: &SvgRadialGradient,
        opacity: f32,
    ) -> GeometryPaint {
        output.push(GpuGradientStop {
            color: [
                gradient.focal[0],
                gradient.focal[1],
                gradient.center[0],
                gradient.center[1],
            ],
            data: [gradient.focal_radius, gradient.radius, 0.0, 0.0],
        });
        let start = output.len() as u32;
        output.extend(gradient.stops.iter().map(|stop| {
            let mut color = stop.color;
            color[3] *= opacity;
            GpuGradientStop {
                color,
                data: [stop.offset, 0.0, 0.0, 0.0],
            }
        }));
        GeometryPaint::RadialGradient {
            inverse_transform: gradient.inverse_transform,
            start,
            count: gradient.stops.len() as u32,
            spread: svg_gradient_spread_index(gradient.spread),
        }
    }

    fn gradient_spread_index(spread: GradientSpread) -> f32 {
        match spread {
            GradientSpread::Pad => 0.0,
            GradientSpread::Repeat => 1.0,
            GradientSpread::Reflect => 2.0,
        }
    }

    fn svg_gradient_spread_index(spread: SvgGradientSpread) -> f32 {
        match spread {
            SvgGradientSpread::Pad => 0.0,
            SvgGradientSpread::Repeat => 1.0,
            SvgGradientSpread::Reflect => 2.0,
        }
    }

    #[derive(Clone, Copy)]
    struct GeometryConstructor<'a> {
        frame: &'a EvaluatedFrameView<'a>,
        matrix: [f32; 6],
        paint_matrix: [f32; 6],
        paint: GeometryPaint,
    }

    impl FillVertexConstructor<Vertex> for GeometryConstructor<'_> {
        fn new_vertex(&mut self, vertex: FillVertex<'_>) -> Vertex {
            let position = vertex.position();
            let projected = to_clip(
                self.frame,
                apply_matrix(self.matrix, [position.x, position.y]),
            );
            let paint_position = apply_matrix(self.paint_matrix, [position.x, position.y]);
            let (color, gradient_position, gradient_meta) = self.paint.vertex_data(paint_position);
            Vertex {
                position: [projected[0], projected[1], 0.0],
                color,
                gradient_position,
                gradient_meta,
                mask_meta: [0.0; 2],
            }
        }
    }

    impl StrokeVertexConstructor<Vertex> for GeometryConstructor<'_> {
        fn new_vertex(&mut self, vertex: StrokeVertex<'_, '_>) -> Vertex {
            let position = vertex.position();
            let projected = to_clip(
                self.frame,
                apply_matrix(self.matrix, [position.x, position.y]),
            );
            let paint_position = apply_matrix(self.paint_matrix, [position.x, position.y]);
            let (color, gradient_position, gradient_meta) = self.paint.vertex_data(paint_position);
            Vertex {
                position: [projected[0], projected[1], 0.0],
                color,
                gradient_position,
                gradient_meta,
                mask_meta: [0.0; 2],
            }
        }
    }

    fn rect_path(width: f32, height: f32, corner_radius: f32) -> Path {
        let half_width = width * 0.5;
        let half_height = height * 0.5;
        let radius = corner_radius.min(half_width).min(half_height);
        let mut builder = Path::builder().with_svg();
        if radius <= 0.0 {
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

    fn command_path(
        commands: &[PathCommand],
        draw_start: f32,
        draw_progress: f32,
    ) -> Result<Path, String> {
        let cubic_subpaths = cubic_subpaths(commands);
        let cubic_count = cubic_subpaths
            .as_ref()
            .map(|subpaths| subpaths.iter().map(|(_, curves, _)| curves.len()).sum())
            .unwrap_or(0);
        if (draw_start > 0.000_001 || draw_progress < 0.999_999)
            && cubic_count > 0
            && cubic_subpaths.is_some()
        {
            return partial_cubic_command_path(commands, draw_start, draw_progress, cubic_count);
        }
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
        let path = builder.build();
        if draw_start > 0.000_001 || draw_progress < 0.999_999 {
            let start = draw_start.clamp(0.0, 1.0);
            let end = draw_progress.clamp(start, 1.0);
            let mut output = Path::builder();
            if end > start + f32::EPSILON {
                let measurements = PathMeasurements::from_path(&path, 0.001);
                if measurements.length() > f32::EPSILON {
                    measurements
                        .create_sampler(&path, SampleType::Normalized)
                        .split_range(start..end, &mut output);
                }
            }
            return Ok(output.build());
        }
        Ok(path)
    }

    #[cfg(test)]
    mod command_path_tests {
        use lyon::path::Event as PathEvent;
        use realtime_manim_scene_core::PathCommand;

        use super::command_path;

        #[test]
        fn line_and_quadratic_paths_honor_draw_ranges() {
            let line = [
                PathCommand::MoveTo { x: 0.0, y: 0.0 },
                PathCommand::LineTo { x: 10.0, y: 0.0 },
            ];
            let hidden = command_path(&line, 0.0, 0.0).expect("hidden line");
            assert!(
                !hidden
                    .iter()
                    .any(|event| matches!(event, PathEvent::Line { .. }))
            );
            let half = command_path(&line, 0.0, 0.5).expect("partial line");
            let line_end = half.iter().find_map(|event| match event {
                PathEvent::Line { to, .. } => Some(to),
                _ => None,
            });
            assert!((line_end.expect("line segment").x - 5.0).abs() < 0.001);

            let quadratic = [
                PathCommand::MoveTo { x: 0.0, y: 0.0 },
                PathCommand::QuadTo {
                    cx: 5.0,
                    cy: 5.0,
                    x: 10.0,
                    y: 0.0,
                },
            ];
            let partial = command_path(&quadratic, 0.25, 0.75).expect("partial quadratic");
            assert!(
                partial
                    .iter()
                    .any(|event| matches!(event, PathEvent::Quadratic { .. }))
            );
        }
    }

    type CubicSubpath = ([f32; 2], Vec<[[f32; 2]; 3]>, bool);

    fn cubic_subpaths(commands: &[PathCommand]) -> Option<Vec<CubicSubpath>> {
        let mut output = Vec::new();
        let mut current: Option<CubicSubpath> = None;
        for command in commands {
            match command {
                PathCommand::MoveTo { x, y } => {
                    if let Some(subpath) = current.take()
                        && !subpath.1.is_empty()
                    {
                        output.push(subpath);
                    }
                    current = Some(([*x, *y], Vec::new(), false));
                }
                PathCommand::CubicTo {
                    c1x,
                    c1y,
                    c2x,
                    c2y,
                    x,
                    y,
                } => current
                    .as_mut()?
                    .1
                    .push([[*c1x, *c1y], [*c2x, *c2y], [*x, *y]]),
                PathCommand::Close => current.as_mut()?.2 = true,
                PathCommand::LineTo { .. } | PathCommand::QuadTo { .. } => return None,
            }
        }
        if let Some(subpath) = current
            && !subpath.1.is_empty()
        {
            output.push(subpath);
        }
        (!output.is_empty()).then_some(output)
    }

    fn partial_cubic_command_path(
        commands: &[PathCommand],
        draw_start: f32,
        draw_progress: f32,
        cubic_count: usize,
    ) -> Result<Path, String> {
        let subpaths = cubic_subpaths(commands)
            .ok_or_else(|| "Partial cubic path requires cubic-only subpaths.".to_owned())?;
        let mut builder = Path::builder().with_svg();
        let start_target = draw_start.clamp(0.0, 1.0) * cubic_count as f32;
        let end_target = draw_progress.clamp(0.0, 1.0) * cubic_count as f32;
        let mut global_curve = 0usize;
        let mut any_started = false;
        for (subpath_start, curves, closed) in &subpaths {
            let first_curve = global_curve;
            let last_curve = first_curve + curves.len();
            let mut current = *subpath_start;
            let mut subpath_started = false;
            for curve in curves {
                let local_start = (start_target - global_curve as f32).clamp(0.0, 1.0);
                let local_end = (end_target - global_curve as f32).clamp(0.0, 1.0);
                if local_end > local_start + f32::EPSILON {
                    let [segment_start, control_1, control_2, segment_end] = cubic_segment(
                        current,
                        curve[0],
                        curve[1],
                        curve[2],
                        local_start,
                        local_end,
                    );
                    if !subpath_started {
                        builder.move_to(point(segment_start[0], segment_start[1]));
                        subpath_started = true;
                        any_started = true;
                    }
                    builder.cubic_bezier_to(
                        point(control_1[0], control_1[1]),
                        point(control_2[0], control_2[1]),
                        point(segment_end[0], segment_end[1]),
                    );
                }
                current = curve[2];
                global_curve += 1;
            }
            if subpath_started
                && *closed
                && start_target <= first_curve as f32 + f32::EPSILON
                && end_target + f32::EPSILON >= last_curve as f32
            {
                builder.close();
            }
        }
        if !any_started {
            let point_at_start = cubic_path_point(&subpaths, start_target, cubic_count);
            builder.move_to(point(point_at_start[0], point_at_start[1]));
            builder.cubic_bezier_to(
                point(point_at_start[0], point_at_start[1]),
                point(point_at_start[0], point_at_start[1]),
                point(point_at_start[0], point_at_start[1]),
            );
        }
        Ok(builder.build())
    }

    fn cubic_segment(
        start: [f32; 2],
        control_1: [f32; 2],
        control_2: [f32; 2],
        end: [f32; 2],
        from: f32,
        to: f32,
    ) -> [[f32; 2]; 4] {
        let to = to.clamp(0.0, 1.0);
        let from = from.clamp(0.0, to);
        let left = left_cubic_curve(start, control_1, control_2, end, to);
        if from <= f32::EPSILON || to <= f32::EPSILON {
            return left;
        }
        right_cubic_curve(left, from / to)
    }

    fn left_cubic_curve(
        start: [f32; 2],
        control_1: [f32; 2],
        control_2: [f32; 2],
        end: [f32; 2],
        amount: f32,
    ) -> [[f32; 2]; 4] {
        let [first, second, third] = left_cubic_segment(start, control_1, control_2, end, amount);
        [start, first, second, third]
    }

    fn right_cubic_curve(curve: [[f32; 2]; 4], amount: f32) -> [[f32; 2]; 4] {
        let lerp = |left: [f32; 2], right: [f32; 2]| {
            [
                left[0] + (right[0] - left[0]) * amount,
                left[1] + (right[1] - left[1]) * amount,
            ]
        };
        let first = lerp(curve[0], curve[1]);
        let second = lerp(curve[1], curve[2]);
        let third = lerp(curve[2], curve[3]);
        let fourth = lerp(first, second);
        let fifth = lerp(second, third);
        let sixth = lerp(fourth, fifth);
        [sixth, fifth, third, curve[3]]
    }

    fn cubic_path_point(subpaths: &[CubicSubpath], target: f32, cubic_count: usize) -> [f32; 2] {
        let clamped = target.clamp(0.0, cubic_count as f32);
        if clamped >= cubic_count as f32 {
            return subpaths
                .last()
                .and_then(|(_, curves, _)| curves.last())
                .map_or([0.0, 0.0], |curve| curve[2]);
        }
        let curve_target = clamped;
        let curve_index = curve_target.floor() as usize;
        let amount = curve_target - curve_index as f32;
        let mut visited = 0usize;
        for (start, curves, _) in subpaths {
            let mut current = *start;
            for curve in curves {
                if visited == curve_index {
                    return left_cubic_segment(current, curve[0], curve[1], curve[2], amount)[2];
                }
                visited += 1;
                current = curve[2];
            }
        }
        [0.0, 0.0]
    }

    fn left_cubic_segment(
        start: [f32; 2],
        control_1: [f32; 2],
        control_2: [f32; 2],
        end: [f32; 2],
        amount: f32,
    ) -> [[f32; 2]; 3] {
        let lerp = |left: [f32; 2], right: [f32; 2]| {
            [
                left[0] + (right[0] - left[0]) * amount,
                left[1] + (right[1] - left[1]) * amount,
            ]
        };
        let first = lerp(start, control_1);
        let second = lerp(control_1, control_2);
        let third = lerp(control_2, end);
        let fourth = lerp(first, second);
        let fifth = lerp(second, third);
        let sixth = lerp(fourth, fifth);
        [first, fourth, sixth]
    }

    fn partial_points(
        points: &[[f32; 2]],
        start: f32,
        progress: f32,
        closed: bool,
    ) -> Vec<[f32; 2]> {
        if points.len() < 2 || (start <= 0.001 && progress >= 0.999) {
            return points.to_vec();
        }
        let start = start.clamp(0.0, 1.0);
        let progress = progress.clamp(0.0, 1.0);
        if progress <= start {
            return Vec::new();
        }
        let mut source = points.to_vec();
        if closed {
            source.push(points[0]);
        }
        let lengths: Vec<f32> = source
            .windows(2)
            .map(|pair| {
                let dx = pair[1][0] - pair[0][0];
                let dy = pair[1][1] - pair[0][1];
                (dx * dx + dy * dy).sqrt()
            })
            .collect();
        let total = lengths.iter().sum::<f32>();
        let start_target = total * start;
        let end_target = total * progress;
        let mut output = Vec::new();
        let mut consumed = 0.0;
        for (index, length) in lengths.iter().copied().enumerate() {
            let segment_start = consumed;
            let segment_end = consumed + length;
            consumed = segment_end;
            if segment_end < start_target || segment_start > end_target {
                continue;
            }
            let from = ((start_target - segment_start) / length.max(f32::EPSILON)).clamp(0.0, 1.0);
            let to = ((end_target - segment_start) / length.max(f32::EPSILON)).clamp(0.0, 1.0);
            let interpolate = |amount: f32| {
                [
                    source[index][0] + (source[index + 1][0] - source[index][0]) * amount,
                    source[index][1] + (source[index + 1][1] - source[index][1]) * amount,
                ]
            };
            let from_point = interpolate(from);
            if output.last().copied() != Some(from_point) {
                output.push(from_point);
            }
            let to_point = interpolate(to);
            if output.last().copied() != Some(to_point) {
                output.push(to_point);
            }
            if segment_end >= end_target {
                break;
            }
        }
        output
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

    fn compose_matrix(outer: [f32; 6], inner: [f32; 6]) -> [f32; 6] {
        [
            outer[0] * inner[0] + outer[2] * inner[1],
            outer[1] * inner[0] + outer[3] * inner[1],
            outer[0] * inner[2] + outer[2] * inner[3],
            outer[1] * inner[2] + outer[3] * inner[3],
            outer[0] * inner[4] + outer[2] * inner[5] + outer[4],
            outer[1] * inner[4] + outer[3] * inner[5] + outer[5],
        ]
    }

    fn apply_matrix(matrix: [f32; 6], point: [f32; 2]) -> [f32; 2] {
        [
            matrix[0] * point[0] + matrix[2] * point[1] + matrix[4],
            matrix[1] * point[0] + matrix[3] * point[1] + matrix[5],
        ]
    }

    fn invert_matrix(matrix: [f32; 6]) -> Option<[f32; 6]> {
        let determinant = matrix[0] * matrix[3] - matrix[1] * matrix[2];
        if determinant.abs() <= f32::EPSILON {
            return None;
        }
        let inverse = 1.0 / determinant;
        let a = matrix[3] * inverse;
        let b = -matrix[1] * inverse;
        let c = -matrix[2] * inverse;
        let d = matrix[0] * inverse;
        Some([
            a,
            b,
            c,
            d,
            -(a * matrix[4] + c * matrix[5]),
            -(b * matrix[4] + d * matrix[5]),
        ])
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

    #[wasm_bindgen(start)]
    pub fn start() {
        console_error_panic_hook::set_once();
    }

    fn request_player_frame(inner: &Rc<PlayerInner>) -> Result<(), JsValue> {
        if inner.destroyed.get() {
            return Ok(());
        }
        let callback = inner.animation_frame.borrow();
        let callback = callback
            .as_ref()
            .ok_or_else(|| js_error("Player animation callback is unavailable."))?;
        let frame_id = window()?.request_animation_frame(callback.as_ref().unchecked_ref())?;
        inner.animation_frame_id.set(Some(frame_id));
        Ok(())
    }

    fn begin_player_recovery(inner: &Rc<PlayerInner>) {
        if inner.destroyed.get() || inner.recovery_in_progress.replace(true) {
            return;
        }
        let state = inner
            .engine
            .borrow_mut()
            .take()
            .map(|engine| engine.recovery_state(performance_now()));
        let Some(state) = state else {
            inner.recovery_in_progress.set(false);
            return;
        };
        let generation = inner.lifecycle_generation.get();
        let weak = Rc::downgrade(inner);
        let device_lost = Arc::clone(&inner.device_lost);
        spawn_local(async move {
            let canvas = state.canvas.clone();
            let result = Engine::new(canvas, Some(Arc::clone(&device_lost))).await;
            let Some(inner) = weak.upgrade() else {
                return;
            };
            if inner.destroyed.get() || inner.lifecycle_generation.get() != generation {
                inner.recovery_in_progress.set(false);
                return;
            }
            match result {
                Ok(mut engine) => {
                    if let Err(error) = engine.restore_state(state, performance_now()) {
                        web_sys::console::error_1(&error);
                        inner.recovery_in_progress.set(false);
                        return;
                    }
                    *inner.engine.borrow_mut() = Some(engine);
                    inner
                        .recovery_count
                        .set(inner.recovery_count.get().saturating_add(1));
                }
                Err(error) => web_sys::console::error_1(&error),
            }
            inner.recovery_in_progress.set(false);
        });
    }

    fn install_player_loop(inner: &Rc<PlayerInner>) -> Result<(), JsValue> {
        let weak: Weak<PlayerInner> = Rc::downgrade(inner);
        let callback = Closure::wrap(Box::new(move |now_ms: f64| {
            let Some(inner) = weak.upgrade() else {
                return;
            };
            inner.animation_frame_id.set(None);
            if inner.destroyed.get() {
                return;
            }
            if inner.device_lost.swap(false, Ordering::AcqRel) {
                begin_player_recovery(&inner);
            }
            if !inner.recovery_in_progress.get() {
                if let Some(engine) = inner.engine.borrow_mut().as_mut() {
                    engine.render(now_ms);
                }
            }
            if let Err(error) = request_player_frame(&inner) {
                web_sys::console::error_1(&error);
            }
        }) as Box<dyn FnMut(f64)>);
        *inner.animation_frame.borrow_mut() = Some(callback);
        request_player_frame(inner)
    }

    /// Creates a renderer without using the legacy document-wide singleton.
    #[wasm_bindgen]
    pub async fn create_player(canvas: HtmlCanvasElement) -> Result<WebPlayer, JsValue> {
        let device_lost = Arc::new(AtomicBool::new(false));
        let engine = Engine::new(canvas, Some(Arc::clone(&device_lost))).await?;
        let inner = Rc::new(PlayerInner {
            engine: RefCell::new(Some(engine)),
            animation_frame: RefCell::new(None),
            animation_frame_id: Cell::new(None),
            destroyed: Cell::new(false),
            device_lost,
            recovery_in_progress: Cell::new(false),
            recovery_count: Cell::new(0),
            lifecycle_generation: Cell::new(0),
        });
        install_player_loop(&inner)?;
        Ok(WebPlayer { inner })
    }

    #[wasm_bindgen]
    impl WebPlayer {
        fn with_engine<T>(
            &self,
            callback: impl FnOnce(&mut Engine) -> Result<T, JsValue>,
        ) -> Result<T, JsValue> {
            if self.inner.destroyed.get() {
                return Err(js_error("This WebPlayer has been destroyed."));
            }
            let mut engine = self.inner.engine.borrow_mut();
            callback(
                engine
                    .as_mut()
                    .ok_or_else(|| js_error("This WebPlayer has no engine."))?,
            )
        }

        pub fn load_scene(&self, scene_json: &str) -> Result<(), JsValue> {
            let scene = Scene::from_json(scene_json).map_err(js_error)?;
            self.with_engine(|engine| engine.load_scene(scene, performance_now()))
        }

        pub fn set_paused(&self, paused: bool) -> Result<(), JsValue> {
            self.with_engine(|engine| {
                engine.set_paused(paused, performance_now());
                Ok(())
            })
        }

        pub fn seek(&self, time: f32) -> Result<(), JsValue> {
            self.with_engine(|engine| {
                engine.manual_time = Some(time.clamp(0.0, engine.scene.duration));
                Ok(())
            })
        }

        pub fn resume(&self) -> Result<(), JsValue> {
            self.with_engine(|engine| {
                engine.resume_from_current_time(performance_now());
                Ok(())
            })
        }

        pub fn current_time(&self) -> Result<f32, JsValue> {
            self.with_engine(|engine| Ok(engine.last_scene_time))
        }

        pub fn set_render_size(&self, width: u32, height: u32) -> Result<(), JsValue> {
            if width == 0 || height == 0 || width > 8192 || height > 8192 {
                return Err(js_error(
                    "Render dimensions must be between 1 and 8192 pixels.",
                ));
            }
            self.with_engine(|engine| {
                engine.render_size_override = Some((width, height));
                engine.resize_if_needed();
                Ok(())
            })
        }

        pub fn clear_render_size(&self) -> Result<(), JsValue> {
            self.with_engine(|engine| {
                engine.render_size_override = None;
                engine.resize_if_needed();
                Ok(())
            })
        }

        pub fn set_signal(&self, signal: &str, value: f32) -> Result<(), JsValue> {
            self.with_engine(|engine| {
                let (min, max, timeline) = engine
                    .scene
                    .controls
                    .iter()
                    .find(|control| control.signal == signal)
                    .map(|control| (control.min, control.max, control.timeline))
                    .ok_or_else(|| js_error(format!("Signal {signal} has no declared control.")))?;
                if !value.is_finite() || !(min..=max).contains(&value) {
                    return Err(js_error(format!(
                        "Signal {signal} must be between {min} and {max}."
                    )));
                }
                if timeline {
                    let time = engine
                        .scene
                        .time_for_signal_value(signal, value)
                        .map_err(js_error)?;
                    engine.signal_overrides.remove(signal);
                    engine.manual_time = Some(time);
                    engine.paused = true;
                    engine.pause_started_ms = performance_now();
                    engine.last_scene_time = time;
                } else {
                    engine.signal_overrides.insert(signal.to_owned(), value);
                }
                Ok(())
            })
        }

        pub fn register_font(
            &self,
            family: &str,
            weight: &str,
            slant: &str,
            data: &[u8],
        ) -> Result<(), JsValue> {
            if data.is_empty() || data.len() > 32 * 1024 * 1024 {
                return Err(js_error("Font data must contain 1 byte–32 MiB."));
            }
            let variant = font_variant_labels(weight, slant)?;
            self.with_engine(|engine| engine.register_font(family, variant, data.to_vec()))
        }

        pub fn reset_clock(&self) -> Result<(), JsValue> {
            self.with_engine(|engine| {
                engine.reset_clock(performance_now());
                Ok(())
            })
        }

        pub fn destroy(&self) -> Result<(), JsValue> {
            if self.inner.destroyed.replace(true) {
                return Ok(());
            }
            self.inner
                .lifecycle_generation
                .set(self.inner.lifecycle_generation.get().wrapping_add(1));
            self.inner.device_lost.store(false, Ordering::Release);
            self.inner.recovery_in_progress.set(false);
            if let Some(frame_id) = self.inner.animation_frame_id.take() {
                window()?.cancel_animation_frame(frame_id)?;
            }
            self.inner.animation_frame.borrow_mut().take();
            self.inner.engine.borrow_mut().take();
            Ok(())
        }

        pub fn is_destroyed(&self) -> bool {
            self.inner.destroyed.get()
        }

        pub fn recovery_count(&self) -> u32 {
            self.inner.recovery_count.get()
        }

        pub fn registered_font_count(&self) -> Result<u32, JsValue> {
            self.with_engine(|engine| {
                u32::try_from(engine.registered_fonts.len())
                    .map_err(|_| js_error("Too many font faces are registered."))
            })
        }

        pub fn simulate_device_loss(&self) -> Result<(), JsValue> {
            self.with_engine(|engine| {
                engine.device.destroy();
                Ok(())
            })
        }
    }

    #[wasm_bindgen]
    pub async fn initialize_canvas(canvas: HtmlCanvasElement) -> Result<(), JsValue> {
        if ENGINE.with(|slot| slot.borrow().is_some()) {
            return Err(js_error("The WebGPU engine is already initialized."));
        }
        if INITIALIZATION_IN_PROGRESS
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(js_error("The WebGPU engine is already initializing."));
        }

        let result = Engine::new(canvas, None).await;
        INITIALIZATION_IN_PROGRESS.store(false, Ordering::Release);
        let engine = result?;
        ENGINE.with(|slot| {
            *slot.borrow_mut() = Some(engine);
        });
        LIFECYCLE_GENERATION.fetch_add(1, Ordering::AcqRel);
        DEVICE_LOST.store(false, Ordering::Release);
        RECOVERY_IN_PROGRESS.store(false, Ordering::Release);
        set_status("General WebGPU engine running", false);
        set_text("backend-value", "Rust scene IR → lyon → WebGPU");
        schedule_animation_loop()?;
        Ok(())
    }

    #[wasm_bindgen]
    pub fn destroy_engine() -> Result<(), JsValue> {
        if INITIALIZATION_IN_PROGRESS.load(Ordering::Acquire) {
            return Err(js_error("The WebGPU engine is still initializing."));
        }
        if let Some(frame_id) = ANIMATION_FRAME_ID.with(|slot| slot.borrow_mut().take()) {
            window()?.cancel_animation_frame(frame_id)?;
        }
        ANIMATION_FRAME.with(|slot| {
            slot.borrow_mut().take();
        });
        ENGINE.with(|slot| {
            slot.borrow_mut().take();
        });
        LIFECYCLE_GENERATION.fetch_add(1, Ordering::AcqRel);
        DEVICE_LOST.store(false, Ordering::Release);
        RECOVERY_IN_PROGRESS.store(false, Ordering::Release);
        Ok(())
    }

    fn begin_device_recovery() {
        if RECOVERY_IN_PROGRESS
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        let now_ms = performance_now();
        let state = ENGINE.with(|slot| {
            slot.borrow_mut()
                .take()
                .map(|engine| engine.recovery_state(now_ms))
        });
        let Some(state) = state else {
            RECOVERY_IN_PROGRESS.store(false, Ordering::Release);
            return;
        };
        let lifecycle_generation = LIFECYCLE_GENERATION.load(Ordering::Acquire);
        set_status("Recovering WebGPU device…", false);
        spawn_local(async move {
            let canvas = state.canvas.clone();
            match Engine::new(canvas, None).await {
                Ok(mut engine) => {
                    if let Err(error) = engine.restore_state(state, performance_now()) {
                        set_status("WebGPU recovery failed", true);
                        set_text(
                            "error-detail",
                            &error.as_string().unwrap_or_else(|| {
                                "Registered fonts could not be restored.".to_owned()
                            }),
                        );
                        show_error();
                        web_sys::console::error_1(&error);
                        RECOVERY_IN_PROGRESS.store(false, Ordering::Release);
                        return;
                    }
                    if LIFECYCLE_GENERATION.load(Ordering::Acquire) != lifecycle_generation {
                        RECOVERY_IN_PROGRESS.store(false, Ordering::Release);
                        return;
                    }
                    ENGINE.with(|slot| *slot.borrow_mut() = Some(engine));
                    RECOVERY_COUNT.fetch_add(1, Ordering::AcqRel);
                    hide_error();
                    set_status("General WebGPU engine running", false);
                    set_text("backend-value", "Rust scene IR → lyon → WebGPU (recovered)");
                }
                Err(error) => {
                    set_status("WebGPU recovery failed", true);
                    set_text(
                        "error-detail",
                        &error.as_string().unwrap_or_else(|| {
                            "The browser could not recreate the WebGPU device.".to_owned()
                        }),
                    );
                    show_error();
                    web_sys::console::error_1(&error);
                }
            }
            RECOVERY_IN_PROGRESS.store(false, Ordering::Release);
        });
    }

    fn schedule_animation_loop() -> Result<(), JsValue> {
        if ANIMATION_FRAME.with(|slot| slot.borrow().is_some()) {
            return Err(js_error("The animation loop is already running."));
        }
        let callback = Closure::wrap(Box::new(move |now_ms: f64| {
            ANIMATION_FRAME_ID.with(|slot| {
                slot.borrow_mut().take();
            });
            if DEVICE_LOST.swap(false, Ordering::AcqRel) {
                begin_device_recovery();
            }
            if !RECOVERY_IN_PROGRESS.load(Ordering::Acquire) {
                ENGINE.with(|slot| {
                    if let Some(engine) = slot.borrow_mut().as_mut() {
                        engine.render(now_ms);
                    }
                });
            }
            let _ = request_animation_frame();
        }) as Box<dyn FnMut(f64)>);
        ANIMATION_FRAME.with(|slot| {
            *slot.borrow_mut() = Some(callback);
        });
        request_animation_frame()
    }

    fn request_animation_frame() -> Result<(), JsValue> {
        ANIMATION_FRAME.with(|slot| {
            let slot = slot.borrow();
            let callback = slot
                .as_ref()
                .ok_or_else(|| js_error("Animation callback is not installed."))?;
            let frame_id = window()?.request_animation_frame(callback.as_ref().unchecked_ref())?;
            ANIMATION_FRAME_ID.with(|frame_slot| {
                *frame_slot.borrow_mut() = Some(frame_id);
            });
            Ok(())
        })
    }

    #[wasm_bindgen]
    pub fn load_scene(scene_json: &str) -> Result<(), JsValue> {
        let scene = Scene::from_json(scene_json).map_err(js_error)?;
        let now_ms = performance_now();
        ENGINE.with(|slot| {
            let mut slot = slot.borrow_mut();
            let engine = slot
                .as_mut()
                .ok_or_else(|| js_error("The WebGPU engine is still starting."))?;
            engine.load_scene(scene, now_ms)
        })
    }

    #[wasm_bindgen]
    pub fn validate_scene(scene_json: &str) -> Result<String, JsValue> {
        let scene = Scene::from_json(scene_json).map_err(js_error)?;
        serde_json::to_string(&scene).map_err(|error| js_error(error.to_string()))
    }

    #[wasm_bindgen]
    pub fn evaluate_scene(scene_json: &str, time: f32) -> Result<String, JsValue> {
        let scene = Scene::from_json(scene_json).map_err(js_error)?;
        let frame = scene.evaluate(time).map_err(js_error)?;
        serde_json::to_string(&frame).map_err(|error| js_error(error.to_string()))
    }

    #[wasm_bindgen]
    pub fn set_paused(paused: bool) {
        let now_ms = performance_now();
        ENGINE.with(|slot| {
            if let Some(engine) = slot.borrow_mut().as_mut() {
                engine.set_paused(paused, now_ms);
            }
        });
    }

    #[wasm_bindgen]
    pub fn seek_scene(time: f32) {
        ENGINE.with(|slot| {
            if let Some(engine) = slot.borrow_mut().as_mut() {
                engine.manual_time = Some(time.clamp(0.0, engine.scene.duration));
            }
        });
    }

    #[wasm_bindgen]
    pub fn resume_scene() {
        let now_ms = performance_now();
        ENGINE.with(|slot| {
            if let Some(engine) = slot.borrow_mut().as_mut() {
                engine.resume_from_current_time(now_ms);
            }
        });
    }

    #[wasm_bindgen]
    pub fn current_scene_time() -> f32 {
        ENGINE.with(|slot| {
            slot.borrow()
                .as_ref()
                .map_or(0.0, |engine| engine.last_scene_time)
        })
    }

    #[wasm_bindgen]
    pub fn simulate_device_loss() -> Result<(), JsValue> {
        ENGINE.with(|slot| {
            let slot = slot.borrow();
            let engine = slot
                .as_ref()
                .ok_or_else(|| js_error("The WebGPU engine is unavailable."))?;
            engine.device.destroy();
            Ok(())
        })
    }

    #[wasm_bindgen]
    pub fn device_recovery_count() -> u32 {
        RECOVERY_COUNT.load(Ordering::Acquire)
    }

    #[wasm_bindgen]
    pub fn set_render_size(width: u32, height: u32) -> Result<(), JsValue> {
        if width == 0 || height == 0 || width > 8192 || height > 8192 {
            return Err(js_error(
                "Render dimensions must be between 1 and 8192 pixels.",
            ));
        }
        ENGINE.with(|slot| {
            let mut slot = slot.borrow_mut();
            let engine = slot
                .as_mut()
                .ok_or_else(|| js_error("The WebGPU engine is still starting."))?;
            engine.render_size_override = Some((width, height));
            engine.resize_if_needed();
            Ok(())
        })
    }

    #[wasm_bindgen]
    pub fn clear_render_size() {
        ENGINE.with(|slot| {
            if let Some(engine) = slot.borrow_mut().as_mut() {
                engine.render_size_override = None;
                engine.resize_if_needed();
            }
        });
    }

    #[wasm_bindgen]
    pub fn set_signal(signal: &str, value: f32) -> Result<(), JsValue> {
        ENGINE.with(|slot| {
            let mut slot = slot.borrow_mut();
            let engine = slot
                .as_mut()
                .ok_or_else(|| js_error("The WebGPU engine is still starting."))?;
            let (min, max, timeline) = engine
                .scene
                .controls
                .iter()
                .find(|control| control.signal == signal)
                .map(|control| (control.min, control.max, control.timeline))
                .ok_or_else(|| js_error(format!("Signal {signal} has no declared control.")))?;
            if !value.is_finite() || !(min..=max).contains(&value) {
                return Err(js_error(format!(
                    "Signal {signal} must be between {min} and {max}."
                )));
            }
            if timeline {
                let time = engine
                    .scene
                    .time_for_signal_value(signal, value)
                    .map_err(js_error)?;
                engine.signal_overrides.remove(signal);
                engine.manual_time = Some(time);
                engine.paused = true;
                engine.pause_started_ms = performance_now();
                engine.last_scene_time = time;
            } else {
                engine.signal_overrides.insert(signal.to_owned(), value);
            }
            Ok(())
        })
    }

    #[wasm_bindgen]
    pub fn register_font(
        family: &str,
        weight: &str,
        slant: &str,
        data: &[u8],
    ) -> Result<(), JsValue> {
        if data.is_empty() || data.len() > 32 * 1024 * 1024 {
            return Err(js_error("Font data must contain 1 byte–32 MiB."));
        }
        let variant = font_variant_labels(weight, slant)?;
        ENGINE.with(|slot| {
            let mut slot = slot.borrow_mut();
            let engine = slot
                .as_mut()
                .ok_or_else(|| js_error("The WebGPU engine is still starting."))?;
            engine.register_font(family, variant, data.to_vec())
        })
    }

    #[wasm_bindgen]
    pub fn reset_clock() {
        let now_ms = performance_now();
        ENGINE.with(|slot| {
            if let Some(engine) = slot.borrow_mut().as_mut() {
                engine.reset_clock(now_ms);
            }
        });
    }

    fn set_status(message: &str, failed: bool) {
        set_text("renderer-status", message);
        if let Ok(document) = document()
            && let Some(element) = document.get_element_by_id("renderer-status")
        {
            element.set_class_name(if failed {
                "status status--error"
            } else {
                "status"
            });
        }
    }

    fn set_text(id: &str, message: &str) {
        if let Ok(document) = document()
            && let Some(element) = document.get_element_by_id(id)
        {
            element.set_text_content(Some(message));
        }
    }

    fn show_error() {
        if let Ok(document) = document()
            && let Some(element) = document.get_element_by_id("error-state")
        {
            element.set_class_name("error-state error-state--visible");
        }
    }

    fn hide_error() {
        if let Ok(document) = document()
            && let Some(element) = document.get_element_by_id("error-state")
        {
            element.set_class_name("error-state");
        }
    }

    fn device_pixel_ratio() -> f32 {
        window()
            .map(|window| window.device_pixel_ratio().clamp(1.0, 2.0) as f32)
            .unwrap_or(1.0)
    }

    fn performance_now() -> f64 {
        window()
            .ok()
            .and_then(|window| window.performance())
            .map(|performance| performance.now())
            .unwrap_or(0.0)
    }

    fn window() -> Result<web_sys::Window, JsValue> {
        web_sys::window().ok_or_else(|| js_error("Window is unavailable."))
    }

    fn document() -> Result<web_sys::Document, JsValue> {
        window()?
            .document()
            .ok_or_else(|| js_error("Document is unavailable."))
    }

    fn js_error(message: impl AsRef<str>) -> JsValue {
        js_sys::Error::new(message.as_ref()).into()
    }

    const DEFAULT_SCENE: &str = r##"{
      "version": 2,
      "title": "General engine proof",
      "width": 16,
      "height": 9,
      "pixelWidth": 1280,
      "pixelHeight": 720,
      "duration": 6,
      "fps": 60,
      "background": "#0b1220",
      "nodes": [
        {"id":"title","type":"text","text":"GENERAL RUST SCENE ENGINE","fontSize":0.55,"transform":{"y":3.35},"style":{"fill":"#e2e8f0","stroke":null}},
        {"id":"ring","type":"circle","radius":1.2,"transform":{"x":-4,"y":0.2},"style":{"fill":"#2563eb55","stroke":"#60a5fa","strokeWidth":0.09}},
        {"id":"square","type":"rect","width":2,"height":2,"cornerRadius":0.24,"style":{"fill":"#f9731666","stroke":"#fb923c","strokeWidth":0.09}},
        {"id":"triangle","type":"polyline","points":[[-1.1,-0.9],[1.1,-0.9],[0,1.1]],"closed":true,"transform":{"x":4,"y":0.2},"style":{"fill":"#22c55e55","stroke":"#4ade80","strokeWidth":0.09}},
        {"id":"baseline","type":"line","from":[-6.5,-2.2],"to":[6.5,-2.2],"style":{"stroke":"#475569","strokeWidth":0.035}},
        {"id":"caption","type":"text","text":"paths  groups  tracks  signals  camera","fontSize":0.38,"transform":{"y":-3.1},"style":{"fill":"#94a3b8","stroke":null}}
      ],
      "tracks": [
        {"target":"ring","property":"x","keyframes":[{"at":0,"value":-4},{"at":3,"value":-2.8,"easing":"smooth"},{"at":6,"value":-4,"easing":"smooth"}]},
        {"target":"ring","property":"drawProgress","keyframes":[{"at":0,"value":0},{"at":1.5,"value":1,"easing":"smooth"},{"at":6,"value":1}]},
        {"target":"square","property":"rotation","keyframes":[{"at":0,"value":0},{"at":6,"value":6.28318,"easing":"smooth"}]},
        {"target":"triangle","property":"scaleX","keyframes":[{"at":0,"value":0.65},{"at":3,"value":1.2,"easing":"thereAndBack"},{"at":6,"value":0.65}]},
        {"target":"triangle","property":"scaleY","keyframes":[{"at":0,"value":0.65},{"at":3,"value":1.2,"easing":"thereAndBack"},{"at":6,"value":0.65}]}
      ],
      "signals": [
        {"id":"float","keyframes":[{"at":0,"value":0},{"at":1.5,"value":0.55,"easing":"smooth"},{"at":3,"value":0},{"at":4.5,"value":-0.55,"easing":"smooth"},{"at":6,"value":0}]}
      ],
      "bindings": [
        {"target":"square","property":"y","expression":{"op":"signal","id":"float"}}
      ]
    }"##;
}

/// Identifies the current renderer without freezing the final backend decision.
#[must_use]
pub fn prototype_status() -> &'static str {
    "general retained scene IR with explicit-time Rust evaluation and batched WebGPU vectors"
}

#[cfg(test)]
mod tests {
    use super::prototype_status;

    #[test]
    fn prototype_is_general_scene_path() {
        assert!(prototype_status().contains("general retained scene IR"));
    }
}
