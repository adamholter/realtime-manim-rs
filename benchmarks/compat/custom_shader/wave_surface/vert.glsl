#version 330

uniform vec2 frame_shape;
uniform vec3 camera_center;
uniform mat3 camera_rotation;
uniform float focal_distance;

in vec3 point;
in vec3 du_point;
in vec3 dv_point;
in vec4 color;

out vec3 xyz_coords;
out vec3 v_normal;
out vec4 v_color;

vec3 into_frame(vec3 value) {
    return camera_rotation * (value - camera_center);
}

vec4 project_point(vec3 value) {
    vec4 result = vec4(value, 1.0);
    result.x *= 2.0 / frame_shape.x;
    result.y *= 2.0 / frame_shape.y;
    float perspective = max(0.0, focal_distance / (focal_distance - result.z));
    result.xy *= perspective;
    result.z *= -0.01;
    return result;
}

void main() {
    xyz_coords = into_frame(point);
    vec3 du = camera_rotation * (du_point - point);
    vec3 dv = camera_rotation * (dv_point - point);
    v_normal = normalize(cross(du, dv));
    v_color = color;
    gl_Position = project_point(xyz_coords);
}
