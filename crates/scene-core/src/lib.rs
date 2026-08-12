//! Renderer-neutral retained scene IR and deterministic explicit-time evaluator.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::f32::consts::PI;

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde::{Deserialize, Serialize};

const MAX_NODES: usize = 5_000;
const MAX_TRACKS: usize = 20_000;
const MAX_SIGNALS: usize = 1_000;
const MAX_BINDINGS: usize = 10_000;
const MAX_CONTROLS: usize = 64;
const MAX_AUDIO_CLIPS: usize = 64;
const MAX_CAPTIONS: usize = 1_000;
const MAX_AUDIO_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Scene {
    pub version: u32,
    pub title: String,
    #[serde(default = "default_width")]
    pub width: f32,
    #[serde(default = "default_height")]
    pub height: f32,
    #[serde(default = "default_pixel_width")]
    pub pixel_width: u32,
    #[serde(default = "default_pixel_height")]
    pub pixel_height: u32,
    pub duration: f32,
    #[serde(default = "default_fps")]
    pub fps: u32,
    #[serde(default = "default_background")]
    pub background: String,
    #[serde(default)]
    pub camera: Camera,
    #[serde(default)]
    pub camera_3d: Camera3d,
    pub nodes: Vec<Node>,
    #[serde(default)]
    pub tracks: Vec<Track>,
    #[serde(default)]
    pub signals: Vec<Signal>,
    #[serde(default)]
    pub bindings: Vec<Binding>,
    #[serde(default)]
    pub controls: Vec<Control>,
    #[serde(default)]
    pub audio: Vec<AudioClip>,
    #[serde(default)]
    pub captions: Vec<Caption>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AudioClip {
    pub id: String,
    pub data: String,
    pub mime_type: String,
    pub start_time: f32,
    #[serde(default)]
    pub gain_db: f32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Caption {
    pub text: String,
    pub start: f32,
    pub end: f32,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Camera {
    #[serde(default)]
    pub x: f32,
    #[serde(default)]
    pub y: f32,
    #[serde(default = "one")]
    pub zoom: f32,
    #[serde(default)]
    pub rotation: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            zoom: 1.0,
            rotation: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Camera3d {
    #[serde(default = "default_camera_3d_position")]
    pub position: [f32; 3],
    #[serde(default)]
    pub target: [f32; 3],
    #[serde(default = "default_camera_3d_up")]
    pub up: [f32; 3],
    #[serde(default = "default_camera_3d_fov")]
    pub fov_y: f32,
    #[serde(default = "default_camera_3d_near")]
    pub near: f32,
    #[serde(default = "default_camera_3d_far")]
    pub far: f32,
    #[serde(default = "default_camera_3d_ambient")]
    pub ambient: f32,
    #[serde(default = "default_light_direction")]
    pub light_direction: [f32; 3],
}

impl Default for Camera3d {
    fn default() -> Self {
        Self {
            position: default_camera_3d_position(),
            target: [0.0, 0.0, 0.0],
            up: default_camera_3d_up(),
            fov_y: default_camera_3d_fov(),
            near: default_camera_3d_near(),
            far: default_camera_3d_far(),
            ambient: default_camera_3d_ambient(),
            light_direction: default_light_direction(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub id: String,
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub z_index: i32,
    #[serde(default)]
    pub transform: Transform,
    #[serde(default)]
    pub style: Style,
    #[serde(default)]
    pub appear_at: f32,
    #[serde(default = "infinity")]
    pub disappear_at: f32,
    #[serde(flatten)]
    pub kind: NodeKind,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum NodeKind {
    Group,
    Billboard {
        anchor: [f32; 3],
        base: [f32; 2],
    },
    Circle {
        radius: f32,
    },
    Rect {
        width: f32,
        height: f32,
        #[serde(default)]
        corner_radius: f32,
    },
    Line {
        from: [f32; 2],
        to: [f32; 2],
    },
    Arrow {
        from: [f32; 2],
        to: [f32; 2],
        #[serde(default = "default_tip_size")]
        tip_size: f32,
    },
    Polyline {
        points: Vec<[f32; 2]>,
        #[serde(default)]
        closed: bool,
    },
    Path {
        commands: Vec<PathCommand>,
    },
    Path3d {
        commands: Vec<PathCommand3d>,
    },
    TracePath {
        segments: Vec<TraceSegment>,
        frames: Vec<TraceFrame>,
    },
    PathRef {
        source: String,
    },
    Text {
        text: String,
        font_size: f32,
        #[serde(default = "default_font_family")]
        font_family: String,
        #[serde(default)]
        align: TextAlign,
        #[serde(default)]
        weight: FontWeight,
        #[serde(default)]
        slant: FontSlant,
    },
    MarkupText {
        spans: Vec<TextSpan>,
        font_size: f32,
        #[serde(default = "default_font_family")]
        font_family: String,
        #[serde(default)]
        align: TextAlign,
    },
    Svg {
        svg: String,
        height: f32,
        #[serde(default = "yes")]
        preserve_styles: bool,
    },
    Image {
        pixels: String,
        pixel_width: u32,
        pixel_height: u32,
        corners: Vec<[f32; 2]>,
        #[serde(default)]
        resampling: ImageResampling,
    },
    PointCloud {
        points: Vec<PointMark>,
        #[serde(default = "default_point_radius")]
        radius: f32,
        #[serde(default)]
        screen_space_radius: bool,
    },
    Mesh {
        vertices: Vec<[f32; 3]>,
        triangles: Vec<[u32; 3]>,
        #[serde(default)]
        colors: Vec<String>,
        #[serde(default)]
        normals: Vec<[f32; 3]>,
        #[serde(default)]
        uvs: Vec<[f32; 2]>,
        #[serde(default)]
        texture_pixels: String,
        #[serde(default)]
        texture_width: u32,
        #[serde(default)]
        texture_height: u32,
        #[serde(default)]
        dark_texture_pixels: String,
        #[serde(default)]
        dark_texture_width: u32,
        #[serde(default)]
        dark_texture_height: u32,
        #[serde(default)]
        texture_resampling: ImageResampling,
        #[serde(default)]
        gloss: f32,
        #[serde(default)]
        shadow: f32,
        #[serde(default = "default_mesh_light_position")]
        light_position: [f32; 3],
        #[serde(default)]
        unlit: bool,
        #[serde(default)]
        double_sided: bool,
    },
    Surface {
        vertices: Vec<[f32; 3]>,
        patches: Vec<Vec<u32>>,
        colors: Vec<String>,
        stroke_colors: Vec<String>,
        stroke_radii: Vec<f32>,
        #[serde(default)]
        unlit: bool,
        #[serde(default)]
        double_sided: bool,
    },
    CustomShaderMesh {
        vertex_wgsl: String,
        fragment_wgsl: String,
        attributes: Vec<ShaderAttribute>,
        vertex_stride: u32,
        vertex_data: Vec<f32>,
        #[serde(default)]
        indices: Vec<u32>,
        #[serde(default)]
        primitive: ShaderPrimitive,
        #[serde(default)]
        uniforms: Vec<ShaderUniform>,
        #[serde(default)]
        depth_test: bool,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShaderAttribute {
    pub name: String,
    pub location: u32,
    pub offset: u32,
    pub format: ShaderVertexFormat,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ShaderVertexFormat {
    Float32,
    Float32x2,
    Float32x3,
    Float32x4,
    Sint32,
    Sint32x2,
    Sint32x3,
    Sint32x4,
    Uint32,
    Uint32x2,
    Uint32x3,
    Uint32x4,
}

impl ShaderVertexFormat {
    pub const fn size(self) -> u32 {
        match self {
            Self::Float32 => 4,
            Self::Float32x2 | Self::Sint32x2 | Self::Uint32x2 => 8,
            Self::Float32x3 | Self::Sint32x3 | Self::Uint32x3 => 12,
            Self::Float32x4 | Self::Sint32x4 | Self::Uint32x4 => 16,
            Self::Sint32 | Self::Uint32 => 4,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ShaderPrimitive {
    PointList,
    LineList,
    LineStrip,
    #[default]
    TriangleList,
    TriangleStrip,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShaderUniform {
    pub name: String,
    pub binding: u32,
    #[serde(default)]
    pub sampler_binding: Option<u32>,
    #[serde(rename = "type")]
    pub uniform_type: ShaderUniformType,
    #[serde(default = "one_u32")]
    pub array_length: u32,
    #[serde(default)]
    pub values: Vec<f32>,
    #[serde(default)]
    pub texture_pixels: String,
    #[serde(default)]
    pub texture_width: u32,
    #[serde(default)]
    pub texture_height: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub enum ShaderUniformType {
    #[serde(rename = "float")]
    Float,
    #[serde(rename = "vec2")]
    Vec2,
    #[serde(rename = "vec3")]
    Vec3,
    #[serde(rename = "vec4")]
    Vec4,
    #[serde(rename = "mat3")]
    Mat3,
    #[serde(rename = "mat4")]
    Mat4,
    #[serde(rename = "int")]
    Int,
    #[serde(rename = "ivec2")]
    Ivec2,
    #[serde(rename = "ivec3")]
    Ivec3,
    #[serde(rename = "ivec4")]
    Ivec4,
    #[serde(rename = "uint")]
    Uint,
    #[serde(rename = "uvec2")]
    Uvec2,
    #[serde(rename = "uvec3")]
    Uvec3,
    #[serde(rename = "uvec4")]
    Uvec4,
    #[serde(rename = "bool")]
    Bool,
    #[serde(rename = "bvec2")]
    Bvec2,
    #[serde(rename = "bvec3")]
    Bvec3,
    #[serde(rename = "bvec4")]
    Bvec4,
    #[serde(rename = "sampler2D")]
    Sampler2d,
}

impl ShaderUniformType {
    const fn value_count(self) -> usize {
        match self {
            Self::Float => 1,
            Self::Vec2 => 2,
            Self::Vec3 => 3,
            Self::Vec4 => 4,
            Self::Mat3 => 9,
            Self::Mat4 => 16,
            Self::Int | Self::Uint | Self::Bool => 1,
            Self::Ivec2 | Self::Uvec2 | Self::Bvec2 => 2,
            Self::Ivec3 | Self::Uvec3 | Self::Bvec3 => 3,
            Self::Ivec4 | Self::Uvec4 | Self::Bvec4 => 4,
            Self::Sampler2d => 0,
        }
    }
}

impl ShaderUniform {
    fn value_count(&self) -> usize {
        self.uniform_type
            .value_count()
            .saturating_mul(self.array_length as usize)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "op", rename_all = "camelCase", deny_unknown_fields)]
pub enum PathCommand {
    MoveTo {
        x: f32,
        y: f32,
    },
    LineTo {
        x: f32,
        y: f32,
    },
    QuadTo {
        cx: f32,
        cy: f32,
        x: f32,
        y: f32,
    },
    CubicTo {
        c1x: f32,
        c1y: f32,
        c2x: f32,
        c2y: f32,
        x: f32,
        y: f32,
    },
    Close,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "op", rename_all = "camelCase", deny_unknown_fields)]
pub enum PathCommand3d {
    MoveTo {
        x: f32,
        y: f32,
        z: f32,
    },
    LineTo {
        x: f32,
        y: f32,
        z: f32,
    },
    QuadTo {
        cx: f32,
        cy: f32,
        cz: f32,
        x: f32,
        y: f32,
        z: f32,
    },
    CubicTo {
        c1x: f32,
        c1y: f32,
        c1z: f32,
        c2x: f32,
        c2y: f32,
        c2z: f32,
        x: f32,
        y: f32,
        z: f32,
    },
    Close,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TraceSegment {
    pub start: [f32; 2],
    pub control_1: [f32; 2],
    pub control_2: [f32; 2],
    pub end: [f32; 2],
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TraceFrame {
    pub at: f32,
    pub start: u32,
    pub count: u32,
    #[serde(default)]
    pub closed: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PointMark {
    pub x: f32,
    pub y: f32,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub radius: Option<f32>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextSpan {
    pub text: String,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub weight: FontWeight,
    #[serde(default)]
    pub slant: FontSlant,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Transform {
    #[serde(default)]
    pub x: f32,
    #[serde(default)]
    pub y: f32,
    #[serde(default)]
    pub z: f32,
    #[serde(default)]
    pub rotation: f32,
    #[serde(default)]
    pub rotation_x: f32,
    #[serde(default)]
    pub rotation_y: f32,
    #[serde(default = "one")]
    pub scale_x: f32,
    #[serde(default = "one")]
    pub scale_y: f32,
    #[serde(default = "one")]
    pub scale_z: f32,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            rotation: 0.0,
            rotation_x: 0.0,
            rotation_y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            scale_z: 1.0,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Style {
    #[serde(default)]
    pub fill: Option<String>,
    #[serde(default)]
    pub fill_gradient: Option<LinearGradient>,
    #[serde(default = "default_stroke")]
    pub stroke: Option<String>,
    #[serde(default)]
    pub stroke_gradient: Option<LinearGradient>,
    #[serde(default = "default_stroke_width")]
    pub stroke_width: f32,
    #[serde(default = "one")]
    pub opacity: f32,
    #[serde(default)]
    pub draw_start: f32,
    #[serde(default = "one")]
    pub draw_progress: f32,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            fill: None,
            fill_gradient: None,
            stroke: default_stroke(),
            stroke_gradient: None,
            stroke_width: default_stroke_width(),
            opacity: 1.0,
            draw_start: 0.0,
            draw_progress: 1.0,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LinearGradient {
    pub from: [f32; 2],
    pub to: [f32; 2],
    pub stops: Vec<GradientStop>,
    #[serde(default)]
    pub spread: GradientSpread,
    #[serde(default)]
    pub space: GradientSpace,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum GradientSpread {
    #[default]
    Pad,
    Repeat,
    Reflect,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum GradientSpace {
    #[default]
    Local,
    World,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GradientStop {
    pub offset: f32,
    pub color: String,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TextAlign {
    Left,
    #[default]
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FontWeight {
    #[default]
    Normal,
    Bold,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FontSlant {
    #[default]
    Normal,
    Italic,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ImageResampling {
    Nearest,
    Box,
    Bilinear,
    Hamming,
    #[default]
    Bicubic,
    Lanczos,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Track {
    pub target: String,
    pub property: Property,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keyframes: Vec<Keyframe>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keyframes_from: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Keyframe {
    pub at: f32,
    pub value: TrackValue,
    #[serde(default)]
    pub easing: Easing,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum TrackValue {
    Number(f32),
    Numbers(Vec<f32>),
    Point3d([f32; 3]),
    Color(String),
    Colors(Vec<String>),
    Points(Vec<[f32; 2]>),
    Points3d(Vec<[f32; 3]>),
    PathCommands(Vec<PathCommand>),
    Gradient(LinearGradient),
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum Property {
    X,
    Y,
    Z,
    Rotation,
    RotationX,
    RotationY,
    ScaleX,
    ScaleY,
    ScaleZ,
    Opacity,
    StrokeWidth,
    DrawStart,
    DrawProgress,
    DrawRange,
    Fill,
    FillGradient,
    Stroke,
    StrokeGradient,
    Radius,
    Points,
    Vertices,
    Normals,
    Colors,
    SurfaceColors,
    StrokeRadii,
    LightPosition,
    Commands,
    PathData,
    Transform2d,
    Affine2d,
    BillboardAnchor,
    CameraX,
    CameraY,
    CameraZoom,
    CameraRotation,
    Camera3dPosition,
    Camera3dTarget,
    Camera3dUp,
    Camera3dFovY,
    Camera3d,
    Camera3dOrbit,
    ShaderVertexData,
    ShaderUniformValues,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Easing {
    #[default]
    Linear,
    Smooth,
    EaseIn,
    EaseOut,
    EaseInOut,
    ThereAndBack,
    Bounce,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Signal {
    pub id: String,
    pub keyframes: Vec<NumberKeyframe>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NumberKeyframe {
    pub at: f32,
    pub value: f32,
    #[serde(default)]
    pub easing: Easing,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Binding {
    pub target: String,
    pub property: Property,
    pub expression: Expr,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Control {
    pub id: String,
    pub label: String,
    pub signal: String,
    pub min: f32,
    pub max: f32,
    pub step: f32,
    pub default: f32,
    #[serde(default)]
    pub timeline: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "op", rename_all = "camelCase", deny_unknown_fields)]
pub enum Expr {
    Constant {
        value: f32,
    },
    Time,
    Signal {
        id: String,
    },
    Add {
        args: Vec<Expr>,
    },
    Multiply {
        args: Vec<Expr>,
    },
    Subtract {
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Divide {
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Sin {
        value: Box<Expr>,
    },
    Cos {
        value: Box<Expr>,
    },
    Abs {
        value: Box<Expr>,
    },
    Min {
        args: Vec<Expr>,
    },
    Max {
        args: Vec<Expr>,
    },
    Clamp {
        value: Box<Expr>,
        min: f32,
        max: f32,
    },
    Lerp {
        from: Box<Expr>,
        to: Box<Expr>,
        amount: Box<Expr>,
    },
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluatedFrame {
    pub title: String,
    pub width: f32,
    pub height: f32,
    pub background: [f32; 4],
    pub camera: Camera,
    pub camera_3d: Camera3d,
    pub nodes: Vec<EvaluatedNode>,
}

pub struct EvaluatedFrameView<'a> {
    pub title: &'a str,
    pub width: f32,
    pub height: f32,
    pub background: [f32; 4],
    pub camera: Camera,
    pub camera_3d: Camera3d,
    pub nodes: Vec<EvaluatedNodeView<'a>>,
    pub cloned_kinds: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluatedNode {
    pub id: String,
    pub z_index: i32,
    pub transform: [f32; 6],
    pub transform_3d: Transform,
    pub style: EvaluatedStyle,
    pub kind: NodeKind,
}

#[derive(Clone, Debug)]
pub struct EvaluatedNodeView<'a> {
    pub id: &'a str,
    pub z_index: i32,
    pub transform: [f32; 6],
    pub transform_3d: Transform,
    pub stroke_in_world_space: bool,
    pub style: EvaluatedStyle,
    pub kind: Cow<'a, NodeKind>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluationProfile {
    pub node_count: usize,
    pub static_node_count: usize,
    pub dynamic_node_count: usize,
    pub track_count: usize,
    pub binding_count: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluatedStyle {
    pub fill: Option<[f32; 4]>,
    pub fill_gradient: Option<EvaluatedLinearGradient>,
    pub stroke: Option<[f32; 4]>,
    pub stroke_gradient: Option<EvaluatedLinearGradient>,
    pub stroke_width: f32,
    pub opacity: f32,
    pub draw_start: f32,
    pub draw_progress: f32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluatedLinearGradient {
    pub from: [f32; 2],
    pub to: [f32; 2],
    pub stops: Vec<EvaluatedGradientStop>,
    pub spread: GradientSpread,
    pub space: GradientSpace,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluatedGradientStop {
    pub offset: f32,
    pub color: [f32; 4],
}

#[derive(Clone, Copy, Debug)]
struct NodeState {
    transform: Transform,
    affine_2d: Option<[f32; 6]>,
    opacity: f32,
    stroke_width: f32,
    draw_start: f32,
    draw_progress: f32,
    radius: Option<f32>,
}

struct RuntimeValues<'a> {
    states: HashMap<&'a str, NodeState>,
    camera: Camera,
    camera_3d: Camera3d,
    fill_overrides: HashMap<&'a str, String>,
    fill_gradient_overrides: HashMap<&'a str, LinearGradient>,
    stroke_overrides: HashMap<&'a str, String>,
    stroke_gradient_overrides: HashMap<&'a str, LinearGradient>,
    point_overrides: HashMap<&'a str, Vec<[f32; 2]>>,
    vertex_overrides: HashMap<&'a str, Vec<[f32; 3]>>,
    normal_overrides: HashMap<&'a str, Vec<[f32; 3]>>,
    color_overrides: HashMap<&'a str, Vec<String>>,
    surface_color_overrides: HashMap<&'a str, Vec<String>>,
    stroke_radii_overrides: HashMap<&'a str, Vec<f32>>,
    light_position_overrides: HashMap<&'a str, [f32; 3]>,
    command_overrides: HashMap<&'a str, Vec<PathCommand>>,
    path_data_overrides: HashMap<&'a str, Vec<f32>>,
    billboard_anchor_overrides: HashMap<&'a str, [f32; 3]>,
    shader_vertex_overrides: HashMap<&'a str, Vec<f32>>,
    shader_uniform_overrides: HashMap<&'a str, Vec<f32>>,
}

impl Scene {
    pub fn from_json(json: &str) -> Result<Self, String> {
        let scene: Self = serde_json::from_str(json)
            .map_err(|error| format!("Scene JSON is invalid: {error}"))?;
        scene.validate()?;
        Ok(scene)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.version != 2 {
            return Err("Scene version must be 2.".to_owned());
        }
        if self.title.trim().is_empty() || self.title.len() > 120 {
            return Err("Scene title must contain 1–120 characters.".to_owned());
        }
        if !(4.0..=40.0).contains(&self.width) || !(2.25..=22.5).contains(&self.height) {
            return Err("Scene dimensions are outside the supported range.".to_owned());
        }
        if self.pixel_width == 0
            || self.pixel_height == 0
            || self.pixel_width > 8_192
            || self.pixel_height > 8_192
        {
            return Err("Scene pixel dimensions must be between 1 and 8192.".to_owned());
        }
        if !(0.25..=60.0).contains(&self.duration) || !(1..=120).contains(&self.fps) {
            return Err("Scene duration must be 0.25–60 seconds and FPS 1–120.".to_owned());
        }
        parse_color(&self.background)?;
        let camera_3d = self.camera_3d;
        finite_point_3d(camera_3d.position)?;
        finite_point_3d(camera_3d.target)?;
        finite_point_3d(camera_3d.up)?;
        finite_point_3d(camera_3d.light_direction)?;
        if !(0.05..PI).contains(&camera_3d.fov_y)
            || camera_3d.near <= 0.0
            || camera_3d.far <= camera_3d.near
            || !(0.0..=1.0).contains(&camera_3d.ambient)
        {
            return Err("3D camera parameters are invalid.".to_owned());
        }
        if self.nodes.is_empty() || self.nodes.len() > MAX_NODES {
            return Err(format!("Scene must contain 1–{MAX_NODES} nodes."));
        }
        if self.tracks.len() > MAX_TRACKS
            || self.signals.len() > MAX_SIGNALS
            || self.bindings.len() > MAX_BINDINGS
            || self.controls.len() > MAX_CONTROLS
            || self.audio.len() > MAX_AUDIO_CLIPS
            || self.captions.len() > MAX_CAPTIONS
        {
            return Err("Scene exceeds evaluator safety limits.".to_owned());
        }
        let mut audio_bytes = 0usize;
        let mut audio_ids = HashSet::new();
        for clip in &self.audio {
            validate_id(&clip.id)?;
            if !audio_ids.insert(clip.id.as_str()) {
                return Err(format!("Duplicate audio id: {}.", clip.id));
            }
            if !matches!(
                clip.mime_type.as_str(),
                "audio/wav"
                    | "audio/x-wav"
                    | "audio/mpeg"
                    | "audio/mp4"
                    | "audio/aac"
                    | "audio/ogg"
                    | "audio/webm"
                    | "audio/flac"
            ) {
                return Err(format!("Audio {} has an unsupported MIME type.", clip.id));
            }
            if !clip.start_time.is_finite()
                || !(0.0..=self.duration).contains(&clip.start_time)
                || !clip.gain_db.is_finite()
                || !(-120.0..=60.0).contains(&clip.gain_db)
            {
                return Err(format!("Audio {} has invalid timing or gain.", clip.id));
            }
            let decoded = BASE64
                .decode(&clip.data)
                .map_err(|error| format!("Audio {} is not valid base64: {error}", clip.id))?;
            audio_bytes = audio_bytes.saturating_add(decoded.len());
            if audio_bytes > MAX_AUDIO_BYTES {
                return Err("Scene audio exceeds the 64 MiB safety limit.".to_owned());
            }
        }
        for caption in &self.captions {
            if caption.text.trim().is_empty() || caption.text.len() > 4_000 {
                return Err("Captions must contain 1–4,000 characters.".to_owned());
            }
            if !caption.start.is_finite()
                || !caption.end.is_finite()
                || caption.start < 0.0
                || caption.end <= caption.start
                || caption.end > self.duration + 0.0001
            {
                return Err("Caption timing is invalid.".to_owned());
            }
        }

        let mut ids = HashSet::new();
        for node in &self.nodes {
            validate_id(&node.id)?;
            if !ids.insert(node.id.as_str()) {
                return Err(format!("Duplicate node id: {}.", node.id));
            }
            node.validate(self.duration)?;
        }
        for node in &self.nodes {
            if let Some(parent) = &node.parent
                && (!ids.contains(parent.as_str()) || parent == &node.id)
            {
                return Err(format!("Node {} has an invalid parent.", node.id));
            }
            if let NodeKind::PathRef { source } = &node.kind {
                let source_node = self
                    .nodes
                    .iter()
                    .find(|candidate| candidate.id == *source)
                    .ok_or_else(|| format!("Node {} references a missing path.", node.id))?;
                if !matches!(source_node.kind, NodeKind::Path { .. }) {
                    return Err(format!(
                        "Node {} must reference a concrete path node.",
                        node.id
                    ));
                }
            }
        }
        self.validate_parent_cycles()?;

        let mut signal_ids = HashSet::new();
        for signal in &self.signals {
            validate_id(&signal.id)?;
            if !signal_ids.insert(signal.id.as_str()) {
                return Err(format!("Duplicate signal id: {}.", signal.id));
            }
            validate_number_keyframes(&signal.keyframes, self.duration)?;
        }
        for track in &self.tracks {
            track.validate(self.duration, &ids)?;
            let keyframes = track.resolved_keyframes(&self.tracks)?;
            if matches!(
                track.property,
                Property::ShaderVertexData | Property::ShaderUniformValues
            ) {
                let node = self
                    .nodes
                    .iter()
                    .find(|node| node.id == track.target)
                    .ok_or_else(|| format!("Missing shader track target {}.", track.target))?;
                let NodeKind::CustomShaderMesh {
                    vertex_data,
                    uniforms,
                    ..
                } = &node.kind
                else {
                    return Err("Shader tracks must target custom shader meshes.".to_owned());
                };
                let expected = if track.property == Property::ShaderVertexData {
                    vertex_data.len()
                } else {
                    uniforms.iter().map(ShaderUniform::value_count).sum()
                };
                if keyframes.iter().any(|keyframe| {
                    !matches!(&keyframe.value, TrackValue::Numbers(values) if values.len() == expected)
                }) {
                    return Err(format!(
                        "Shader track {} values do not match the target layout.",
                        track.target
                    ));
                }
            }
            if track.property == Property::PathData {
                let node = self
                    .nodes
                    .iter()
                    .find(|node| node.id == track.target)
                    .ok_or_else(|| format!("Missing path-data target {}.", track.target))?;
                let NodeKind::Path { commands } = &node.kind else {
                    return Err("Path-data tracks must target concrete paths.".to_owned());
                };
                let expected = path_command_data_len(commands);
                if keyframes.iter().any(|keyframe| {
                    !matches!(&keyframe.value, TrackValue::Numbers(values) if values.len() == expected)
                }) {
                    return Err(format!(
                        "Path-data track {} values do not match the target topology.",
                        track.target
                    ));
                }
            }
            if track.property == Property::BillboardAnchor {
                let node = self
                    .nodes
                    .iter()
                    .find(|node| node.id == track.target)
                    .ok_or_else(|| format!("Missing billboard target {}.", track.target))?;
                if !matches!(node.kind, NodeKind::Billboard { .. }) {
                    return Err("Billboard-anchor tracks must target billboard nodes.".to_owned());
                }
            }
            if matches!(
                track.property,
                Property::Vertices | Property::SurfaceColors | Property::StrokeRadii
            ) {
                let node = self
                    .nodes
                    .iter()
                    .find(|node| node.id == track.target)
                    .ok_or_else(|| format!("Missing surface track target {}.", track.target))?;
                if let NodeKind::Surface {
                    vertices,
                    patches,
                    colors,
                    stroke_colors,
                    stroke_radii,
                    ..
                } = &node.kind
                {
                    let expected = match track.property {
                        Property::Vertices => vertices.len(),
                        Property::SurfaceColors => colors.len() + stroke_colors.len(),
                        Property::StrokeRadii => stroke_radii.len(),
                        _ => unreachable!(),
                    };
                    let layout_matches =
                        keyframes
                            .iter()
                            .all(|keyframe| match (track.property, &keyframe.value) {
                                (Property::Vertices, TrackValue::Points3d(values)) => {
                                    values.len() == expected
                                }
                                (Property::SurfaceColors, TrackValue::Colors(values)) => {
                                    values.len() == expected
                                }
                                (Property::StrokeRadii, TrackValue::Numbers(values)) => {
                                    values.len() == expected
                                }
                                _ => false,
                            });
                    if !layout_matches {
                        return Err(format!(
                            "Surface track {} values do not match the target topology ({} patches).",
                            track.target,
                            patches.len()
                        ));
                    }
                } else if matches!(
                    track.property,
                    Property::SurfaceColors | Property::StrokeRadii
                ) {
                    return Err(format!(
                        "{:?} tracks must target compact surfaces.",
                        track.property
                    ));
                }
            }
            if matches!(track.property, Property::Transform2d | Property::Affine2d) {
                let expected = match &keyframes[0].value {
                    TrackValue::Numbers(values) => values.len(),
                    _ => unreachable!("transform tracks are validated above"),
                };
                if keyframes.iter().any(|keyframe| {
                    !matches!(&keyframe.value, TrackValue::Numbers(values) if values.len() == expected)
                }) {
                    return Err(format!(
                        "Transform track {} keyframes must use one layout.",
                        track.target
                    ));
                }
            }
            if track.property == Property::Camera3d {
                let mask = match &keyframes[0].value {
                    TrackValue::Numbers(values) => values[0],
                    _ => unreachable!("camera tracks are validated above"),
                };
                if keyframes.iter().any(|keyframe| {
                    !matches!(&keyframe.value, TrackValue::Numbers(values) if values[0] == mask)
                }) {
                    return Err("Camera3d track field masks must remain constant.".to_owned());
                }
            }
        }
        for binding in &self.bindings {
            if binding.target != "__camera__" && !ids.contains(binding.target.as_str()) {
                return Err(format!(
                    "Binding target does not exist: {}.",
                    binding.target
                ));
            }
            if !binding.property.is_numeric() {
                return Err("Bindings may target numeric properties only.".to_owned());
            }
            binding.expression.validate(&signal_ids, 0)?;
        }
        let mut control_ids = HashSet::new();
        let mut timeline_controls = 0usize;
        for control in &self.controls {
            validate_id(&control.id)?;
            if !control_ids.insert(control.id.as_str()) {
                return Err(format!("Duplicate control id: {}.", control.id));
            }
            if control.label.trim().is_empty() || control.label.len() > 80 {
                return Err(format!("Control {} has an invalid label.", control.id));
            }
            if !signal_ids.contains(control.signal.as_str()) {
                return Err(format!(
                    "Control {} references missing signal {}.",
                    control.id, control.signal
                ));
            }
            if control.timeline {
                timeline_controls += 1;
                let signal = self
                    .signals
                    .iter()
                    .find(|signal| signal.id == control.signal)
                    .ok_or_else(|| format!("Missing timeline signal {}.", control.signal))?;
                let nondecreasing = signal
                    .keyframes
                    .windows(2)
                    .all(|pair| pair[0].value <= pair[1].value);
                let nonincreasing = signal
                    .keyframes
                    .windows(2)
                    .all(|pair| pair[0].value >= pair[1].value);
                let linear = signal
                    .keyframes
                    .iter()
                    .all(|keyframe| matches!(keyframe.easing, Easing::Linear));
                let signal_min = signal
                    .keyframes
                    .iter()
                    .map(|keyframe| keyframe.value)
                    .reduce(f32::min)
                    .unwrap_or(control.min);
                let signal_max = signal
                    .keyframes
                    .iter()
                    .map(|keyframe| keyframe.value)
                    .reduce(f32::max)
                    .unwrap_or(control.max);
                if !linear
                    || (!nondecreasing && !nonincreasing)
                    || (signal_max - signal_min).abs() <= f32::EPSILON
                    || (signal_min - control.min).abs() > 0.0001
                    || (signal_max - control.max).abs() > 0.0001
                {
                    return Err(format!(
                        "Timeline control {} needs one monotonic signal spanning its range.",
                        control.id
                    ));
                }
            }
            if !control.min.is_finite()
                || !control.max.is_finite()
                || !control.step.is_finite()
                || !control.default.is_finite()
                || control.min >= control.max
                || control.step <= 0.0
                || !(control.min..=control.max).contains(&control.default)
            {
                return Err(format!(
                    "Control {} has an invalid numeric range.",
                    control.id
                ));
            }
        }
        if timeline_controls > 1 {
            return Err("A scene may declare at most one timeline control.".to_owned());
        }
        Ok(())
    }

    pub fn time_for_signal_value(&self, signal_id: &str, value: f32) -> Result<f32, String> {
        let signal = self
            .signals
            .iter()
            .find(|signal| signal.id == signal_id)
            .ok_or_else(|| format!("Missing signal: {signal_id}."))?;
        let ascending = signal
            .keyframes
            .last()
            .map(|keyframe| keyframe.value)
            .unwrap_or(0.0)
            >= signal.keyframes[0].value;
        let mut low = signal.keyframes[0].at;
        let mut high = signal.keyframes.last().map_or(low, |keyframe| keyframe.at);
        for _ in 0..32 {
            let middle = (low + high) * 0.5;
            let sampled = sample_number_keyframes(&signal.keyframes, middle);
            if (ascending && sampled < value) || (!ascending && sampled > value) {
                low = middle;
            } else {
                high = middle;
            }
        }
        Ok((low + high) * 0.5)
    }

    pub fn evaluation_profile(&self) -> EvaluationProfile {
        let dynamic_ids: HashSet<&str> = self
            .tracks
            .iter()
            .map(|track| track.target.as_str())
            .chain(self.bindings.iter().map(|binding| binding.target.as_str()))
            .filter(|target| *target != "__camera__")
            .collect();
        EvaluationProfile {
            node_count: self.nodes.len(),
            static_node_count: self.nodes.len().saturating_sub(dynamic_ids.len()),
            dynamic_node_count: dynamic_ids.len(),
            track_count: self.tracks.len(),
            binding_count: self.bindings.len(),
        }
    }

    fn validate_parent_cycles(&self) -> Result<(), String> {
        let parents: HashMap<&str, &str> = self
            .nodes
            .iter()
            .filter_map(|node| {
                node.parent
                    .as_deref()
                    .map(|parent| (node.id.as_str(), parent))
            })
            .collect();
        for node in &self.nodes {
            let mut visited = HashSet::new();
            let mut current = node.id.as_str();
            while let Some(parent) = parents.get(current).copied() {
                if !visited.insert(parent) {
                    return Err(format!("Parent cycle detected from node {}.", node.id));
                }
                current = parent;
            }
        }
        Ok(())
    }

    pub fn evaluate(&self, time: f32) -> Result<EvaluatedFrame, String> {
        Ok(self.evaluate_view(time)?.into_owned())
    }

    pub fn evaluate_with_signal_overrides(
        &self,
        time: f32,
        signal_overrides: &HashMap<String, f32>,
    ) -> Result<EvaluatedFrame, String> {
        Ok(self
            .evaluate_view_with_signal_overrides(time, signal_overrides)?
            .into_owned())
    }

    pub fn evaluate_view(&self, time: f32) -> Result<EvaluatedFrameView<'_>, String> {
        self.evaluate_view_with_signal_overrides(time, &HashMap::new())
    }

    pub fn evaluate_view_with_signal_overrides<'a>(
        &'a self,
        time: f32,
        signal_overrides: &HashMap<String, f32>,
    ) -> Result<EvaluatedFrameView<'a>, String> {
        let time = time.clamp(0.0, self.duration);
        let signal_values: HashMap<&str, f32> = self
            .signals
            .iter()
            .map(|signal| {
                (
                    signal.id.as_str(),
                    signal_overrides
                        .get(&signal.id)
                        .copied()
                        .unwrap_or_else(|| sample_number_keyframes(&signal.keyframes, time)),
                )
            })
            .collect();
        let mut runtime = RuntimeValues {
            states: self
                .nodes
                .iter()
                .map(|node| {
                    (
                        node.id.as_str(),
                        NodeState {
                            transform: node.transform,
                            affine_2d: None,
                            opacity: node.style.opacity,
                            stroke_width: node.style.stroke_width,
                            draw_start: node.style.draw_start,
                            draw_progress: node.style.draw_progress,
                            radius: match node.kind {
                                NodeKind::Circle { radius } => Some(radius),
                                _ => None,
                            },
                        },
                    )
                })
                .collect(),
            camera: self.camera,
            camera_3d: self.camera_3d,
            fill_overrides: HashMap::new(),
            fill_gradient_overrides: HashMap::new(),
            stroke_overrides: HashMap::new(),
            stroke_gradient_overrides: HashMap::new(),
            point_overrides: HashMap::new(),
            vertex_overrides: HashMap::new(),
            normal_overrides: HashMap::new(),
            color_overrides: HashMap::new(),
            surface_color_overrides: HashMap::new(),
            stroke_radii_overrides: HashMap::new(),
            light_position_overrides: HashMap::new(),
            command_overrides: HashMap::new(),
            path_data_overrides: HashMap::new(),
            billboard_anchor_overrides: HashMap::new(),
            shader_vertex_overrides: HashMap::new(),
            shader_uniform_overrides: HashMap::new(),
        };

        let concrete_track_keyframes: HashMap<(&str, Property), &[Keyframe]> = self
            .tracks
            .iter()
            .filter(|track| track.keyframes_from.is_none())
            .map(|track| {
                (
                    (track.target.as_str(), track.property),
                    track.keyframes.as_slice(),
                )
            })
            .collect();
        for track in &self.tracks {
            let keyframes = if let Some(source) = track.keyframes_from.as_deref() {
                concrete_track_keyframes
                    .get(&(source, track.property))
                    .copied()
                    .ok_or_else(|| {
                        format!(
                            "Track {} references missing {:?} keyframes on {source}.",
                            track.target, track.property
                        )
                    })?
            } else {
                &track.keyframes
            };
            let value = sample_track_values(keyframes, time);
            apply_value(&track.target, track.property, value, &mut runtime)?;
        }
        for binding in &self.bindings {
            let value = binding.expression.evaluate(time, &signal_values, 0)?;
            apply_value(
                &binding.target,
                binding.property,
                TrackValue::Number(value),
                &mut runtime,
            )?;
        }
        for node in &self.nodes {
            if let NodeKind::Billboard { anchor, base } = node.kind
                && let Some(projected) = project_camera_3d_point(
                    runtime
                        .billboard_anchor_overrides
                        .get(node.id.as_str())
                        .copied()
                        .unwrap_or(anchor),
                    runtime.camera_3d,
                    self.width,
                    self.height,
                )
            {
                let state = runtime
                    .states
                    .get_mut(node.id.as_str())
                    .ok_or_else(|| format!("Missing billboard state for {}.", node.id))?;
                state.transform.x += projected[0] - base[0];
                state.transform.y += projected[1] - base[1];
            }
        }

        let nodes_by_id: HashMap<&str, &Node> = self
            .nodes
            .iter()
            .map(|node| (node.id.as_str(), node))
            .collect();
        let mut matrices: HashMap<&str, [f32; 6]> = HashMap::new();
        for node in &self.nodes {
            resolve_matrix(
                node.id.as_str(),
                &nodes_by_id,
                &runtime.states,
                &mut matrices,
                &mut HashSet::new(),
            )?;
        }

        let mut output = Vec::with_capacity(self.nodes.len());
        for node in &self.nodes {
            if time < node.appear_at || time >= node.disappear_at {
                continue;
            }
            let state = runtime
                .states
                .get(node.id.as_str())
                .ok_or_else(|| format!("Missing state for node {}.", node.id))?;
            if state.opacity <= 0.0 {
                continue;
            }
            let mut kind = match &node.kind {
                NodeKind::PathRef { source } => Cow::Borrowed(
                    &nodes_by_id
                        .get(source.as_str())
                        .ok_or_else(|| format!("Missing path reference: {source}."))?
                        .kind,
                ),
                NodeKind::TracePath { segments, frames } => Cow::Owned(NodeKind::Path {
                    commands: sample_trace_path(segments, frames, time),
                }),
                _ => Cow::Borrowed(&node.kind),
            };
            if let Some(radius) = state.radius
                && let NodeKind::Circle {
                    radius: original_radius,
                } = &node.kind
                && radius != *original_radius
                && let NodeKind::Circle {
                    radius: kind_radius,
                } = kind.to_mut()
            {
                *kind_radius = radius;
            }
            if let Some(points) = runtime.point_overrides.get(node.id.as_str()) {
                match kind.to_mut() {
                    NodeKind::Polyline {
                        points: kind_points,
                        ..
                    } => *kind_points = points.clone(),
                    NodeKind::PointCloud {
                        points: kind_points,
                        ..
                    } => {
                        *kind_points = points
                            .iter()
                            .enumerate()
                            .map(|(index, point)| PointMark {
                                x: point[0],
                                y: point[1],
                                color: kind_points.get(index).and_then(|mark| mark.color.clone()),
                                radius: kind_points.get(index).and_then(|mark| mark.radius),
                            })
                            .collect();
                    }
                    NodeKind::Image {
                        corners: kind_corners,
                        ..
                    } if points.len() == 4 => *kind_corners = points.clone(),
                    _ => {}
                }
            }
            if let Some(vertices) = runtime.vertex_overrides.get(node.id.as_str()) {
                match kind.to_mut() {
                    NodeKind::Mesh {
                        vertices: kind_vertices,
                        ..
                    }
                    | NodeKind::Surface {
                        vertices: kind_vertices,
                        ..
                    } => *kind_vertices = vertices.clone(),
                    _ => {}
                }
            }
            if let Some(normals) = runtime.normal_overrides.get(node.id.as_str())
                && let NodeKind::Mesh {
                    normals: kind_normals,
                    ..
                } = kind.to_mut()
            {
                *kind_normals = normals.clone();
            }
            if let Some(colors) = runtime.color_overrides.get(node.id.as_str()) {
                match kind.to_mut() {
                    NodeKind::Mesh {
                        colors: kind_colors,
                        ..
                    }
                    | NodeKind::Surface {
                        colors: kind_colors,
                        ..
                    } => *kind_colors = colors.clone(),
                    _ => {}
                }
            }
            if let Some(colors) = runtime.surface_color_overrides.get(node.id.as_str())
                && let NodeKind::Surface {
                    colors: kind_colors,
                    stroke_colors: kind_stroke_colors,
                    ..
                } = kind.to_mut()
            {
                let fill_color_count = kind_colors.len();
                *kind_colors = colors[..fill_color_count].to_vec();
                *kind_stroke_colors = colors[fill_color_count..].to_vec();
            }
            if let Some(stroke_radii) = runtime.stroke_radii_overrides.get(node.id.as_str())
                && let NodeKind::Surface {
                    stroke_radii: kind_stroke_radii,
                    ..
                } = kind.to_mut()
            {
                *kind_stroke_radii = stroke_radii.clone();
            }
            if let Some(colors) = runtime.color_overrides.get(node.id.as_str())
                && let NodeKind::PointCloud { points, .. } = kind.to_mut()
                && colors.len() == points.len()
            {
                for (point, color) in points.iter_mut().zip(colors) {
                    point.color = Some(color.clone());
                }
            }
            if let Some(light_position) = runtime.light_position_overrides.get(node.id.as_str())
                && let NodeKind::Mesh {
                    light_position: kind_light_position,
                    ..
                } = kind.to_mut()
            {
                *kind_light_position = *light_position;
            }
            if let Some(commands) = runtime.command_overrides.get(node.id.as_str())
                && let NodeKind::Path {
                    commands: kind_commands,
                } = kind.to_mut()
            {
                *kind_commands = commands.clone();
            }
            if let Some(data) = runtime.path_data_overrides.get(node.id.as_str())
                && let NodeKind::Path {
                    commands: kind_commands,
                } = kind.to_mut()
                && let Some(commands) = path_commands_with_data(kind_commands, data)
            {
                *kind_commands = commands;
            }
            if let Some(vertex_data) = runtime.shader_vertex_overrides.get(node.id.as_str())
                && let NodeKind::CustomShaderMesh {
                    vertex_data: kind_vertex_data,
                    ..
                } = kind.to_mut()
            {
                *kind_vertex_data = vertex_data.clone();
            }
            if let Some(values) = runtime.shader_uniform_overrides.get(node.id.as_str())
                && let NodeKind::CustomShaderMesh { uniforms, .. } = kind.to_mut()
            {
                let mut offset = 0;
                for uniform in uniforms {
                    let count = uniform.value_count();
                    if count == 0 {
                        continue;
                    }
                    uniform
                        .values
                        .clone_from_slice(&values[offset..offset + count]);
                    offset += count;
                }
            }
            if matches!(kind.as_ref(), NodeKind::Group | NodeKind::Billboard { .. }) {
                continue;
            }
            let fill = runtime
                .fill_overrides
                .get(node.id.as_str())
                .map(String::as_str)
                .or(node.style.fill.as_deref())
                .map(parse_color)
                .transpose()?
                .map(|mut color| {
                    color[3] *= state.opacity;
                    color
                });
            let fill_gradient = if runtime.fill_overrides.contains_key(node.id.as_str()) {
                None
            } else {
                runtime
                    .fill_gradient_overrides
                    .get(node.id.as_str())
                    .or(node.style.fill_gradient.as_ref())
                    .map(|gradient| evaluate_gradient(gradient, state.opacity))
                    .transpose()?
            };
            let stroke = runtime
                .stroke_overrides
                .get(node.id.as_str())
                .map(String::as_str)
                .or(node.style.stroke.as_deref())
                .map(parse_color)
                .transpose()?
                .map(|mut color| {
                    color[3] *= state.opacity;
                    color
                });
            let stroke_gradient = if runtime.stroke_overrides.contains_key(node.id.as_str()) {
                None
            } else {
                runtime
                    .stroke_gradient_overrides
                    .get(node.id.as_str())
                    .or(node.style.stroke_gradient.as_ref())
                    .map(|gradient| evaluate_gradient(gradient, state.opacity))
                    .transpose()?
            };
            output.push(EvaluatedNodeView {
                id: node.id.as_str(),
                z_index: node.z_index,
                transform: matrices[node.id.as_str()],
                transform_3d: state.transform,
                stroke_in_world_space: state.affine_2d.is_some(),
                style: EvaluatedStyle {
                    fill,
                    fill_gradient,
                    stroke,
                    stroke_gradient,
                    stroke_width: state.stroke_width.max(0.0),
                    opacity: state.opacity.clamp(0.0, 1.0),
                    draw_start: state.draw_start.clamp(0.0, 1.0),
                    draw_progress: state.draw_progress.clamp(0.0, 1.0),
                },
                kind,
            });
        }
        output.sort_by_key(|node| node.z_index);

        let cloned_kinds = output
            .iter()
            .filter(|node| matches!(node.kind, Cow::Owned(_)))
            .count();
        Ok(EvaluatedFrameView {
            title: self.title.as_str(),
            width: self.width,
            height: self.height,
            background: parse_color(&self.background)?,
            camera: runtime.camera,
            camera_3d: runtime.camera_3d,
            nodes: output,
            cloned_kinds,
        })
    }
}

impl EvaluatedFrameView<'_> {
    fn into_owned(self) -> EvaluatedFrame {
        EvaluatedFrame {
            title: self.title.to_owned(),
            width: self.width,
            height: self.height,
            background: self.background,
            camera: self.camera,
            camera_3d: self.camera_3d,
            nodes: self
                .nodes
                .into_iter()
                .map(|node| EvaluatedNode {
                    id: node.id.to_owned(),
                    z_index: node.z_index,
                    transform: node.transform,
                    transform_3d: node.transform_3d,
                    style: node.style,
                    kind: node.kind.into_owned(),
                })
                .collect(),
        }
    }
}

impl Node {
    fn validate(&self, duration: f32) -> Result<(), String> {
        if !self.transform.x.is_finite()
            || !self.transform.y.is_finite()
            || !self.transform.z.is_finite()
            || !self.transform.rotation.is_finite()
            || !self.transform.rotation_x.is_finite()
            || !self.transform.rotation_y.is_finite()
            || !self.transform.scale_x.is_finite()
            || !self.transform.scale_y.is_finite()
            || !self.transform.scale_z.is_finite()
        {
            return Err(format!("Node {} has an invalid transform.", self.id));
        }
        if self.transform.scale_x.abs() > 1_000.0
            || self.transform.scale_y.abs() > 1_000.0
            || self.transform.scale_z.abs() > 1_000.0
        {
            return Err(format!("Node {} scale is too large.", self.id));
        }
        if self.appear_at < 0.0 || self.disappear_at <= self.appear_at || self.appear_at > duration
        {
            return Err(format!("Node {} has an invalid lifetime.", self.id));
        }
        if let Some(fill) = &self.style.fill {
            parse_color(fill)?;
        }
        if let Some(gradient) = &self.style.fill_gradient {
            validate_gradient(gradient)?;
        }
        if let Some(stroke) = &self.style.stroke {
            parse_color(stroke)?;
        }
        if let Some(gradient) = &self.style.stroke_gradient {
            validate_gradient(gradient)?;
        }
        if !self.style.stroke_width.is_finite()
            || !(0.0..=10.0).contains(&self.style.stroke_width)
            || !(0.0..=1.0).contains(&self.style.opacity)
            || !(0.0..=1.0).contains(&self.style.draw_start)
            || !(0.0..=1.0).contains(&self.style.draw_progress)
        {
            return Err(format!("Node {} has an invalid style.", self.id));
        }
        match &self.kind {
            NodeKind::Group => {}
            NodeKind::Billboard { anchor, base } => {
                finite_point_3d(*anchor)?;
                finite_point(*base)?;
            }
            NodeKind::Circle { radius } => positive(*radius, "circle radius")?,
            NodeKind::Rect {
                width,
                height,
                corner_radius,
            } => {
                positive(*width, "rectangle width")?;
                positive(*height, "rectangle height")?;
                if *corner_radius < 0.0 || !corner_radius.is_finite() {
                    return Err("Rectangle corner radius must be nonnegative.".to_owned());
                }
            }
            NodeKind::Line { from, to } | NodeKind::Arrow { from, to, .. } => {
                finite_point(*from)?;
                finite_point(*to)?;
            }
            NodeKind::Polyline { points, .. } => validate_points(points, 2)?,
            NodeKind::Path { commands } => {
                validate_path_commands(commands)?;
            }
            NodeKind::Path3d { commands } => {
                validate_path_commands_3d(commands)?;
            }
            NodeKind::TracePath { segments, frames } => {
                validate_trace_path(segments, frames, duration)?;
            }
            NodeKind::PathRef { source } => validate_id(source)?,
            NodeKind::Text {
                text,
                font_size,
                font_family,
                ..
            } => {
                if text.is_empty() || text.len() > 4_000 {
                    return Err("Text nodes must contain 1–4,000 characters.".to_owned());
                }
                positive(*font_size, "font size")?;
                validate_font_family(font_family)?;
            }
            NodeKind::MarkupText {
                spans,
                font_size,
                font_family,
                ..
            } => {
                if spans.is_empty() || spans.len() > 1_000 {
                    return Err("Markup text must contain 1–1,000 spans.".to_owned());
                }
                positive(*font_size, "font size")?;
                validate_font_family(font_family)?;
                let mut text_bytes = 0;
                for span in spans {
                    if span.text.is_empty() || span.text.contains('\n') {
                        return Err(
                            "Markup text spans must be nonempty single-line text.".to_owned()
                        );
                    }
                    text_bytes += span.text.len();
                    if let Some(color) = &span.color {
                        parse_color(color)?;
                    }
                }
                if text_bytes > 4_000 {
                    return Err("Markup text is limited to 4,000 bytes.".to_owned());
                }
            }
            NodeKind::Svg { svg, height, .. } => {
                if svg.is_empty() || svg.len() > 4_000_000 {
                    return Err("SVG nodes must contain 1–4,000,000 characters.".to_owned());
                }
                positive(*height, "SVG height")?;
            }
            NodeKind::Image {
                pixels,
                pixel_width,
                pixel_height,
                corners,
                ..
            } => {
                if *pixel_width == 0
                    || *pixel_height == 0
                    || *pixel_width > 8_192
                    || *pixel_height > 8_192
                    || u64::from(*pixel_width) * u64::from(*pixel_height) > 16_777_216
                {
                    return Err("Image dimensions exceed 16,777,216 pixels.".to_owned());
                }
                if corners.len() != 4 {
                    return Err("Images require four projected corners.".to_owned());
                }
                validate_points(corners, 4)?;
                let decoded = BASE64
                    .decode(pixels)
                    .map_err(|_| "Image pixels must be valid base64.".to_owned())?;
                let expected = *pixel_width as usize * *pixel_height as usize * 4;
                if decoded.len() != expected {
                    return Err(format!(
                        "Image pixel data is {} bytes; expected {expected}.",
                        decoded.len()
                    ));
                }
            }
            NodeKind::PointCloud { points, radius, .. } => {
                if points.is_empty() || points.len() > 100_000 {
                    return Err("Point clouds must contain 1–100,000 points.".to_owned());
                }
                positive(*radius, "point radius")?;
                for point in points {
                    finite_point([point.x, point.y])?;
                    if let Some(color) = &point.color {
                        parse_color(color)?;
                    }
                }
            }
            NodeKind::Mesh {
                vertices,
                triangles,
                colors,
                normals,
                uvs,
                texture_pixels,
                texture_width,
                texture_height,
                dark_texture_pixels,
                dark_texture_width,
                dark_texture_height,
                gloss,
                shadow,
                light_position,
                ..
            } => {
                if vertices.len() < 3 || vertices.len() > 1_000_000 {
                    return Err("Meshes must contain 3–1,000,000 vertices.".to_owned());
                }
                if triangles.is_empty() || triangles.len() > 2_000_000 {
                    return Err("Meshes must contain 1–2,000,000 triangles.".to_owned());
                }
                for vertex in vertices {
                    finite_point_3d(*vertex)?;
                }
                if !colors.is_empty() && colors.len() != vertices.len() {
                    return Err("Mesh colors must be empty or match the vertex count.".to_owned());
                }
                for color in colors {
                    parse_color(color)?;
                }
                if !normals.is_empty() && normals.len() != vertices.len() {
                    return Err("Mesh normals must be empty or match the vertex count.".to_owned());
                }
                for normal in normals {
                    finite_point_3d(*normal)?;
                }
                if texture_pixels.is_empty() {
                    if !uvs.is_empty()
                        || *texture_width != 0
                        || *texture_height != 0
                        || !dark_texture_pixels.is_empty()
                        || *dark_texture_width != 0
                        || *dark_texture_height != 0
                    {
                        return Err(
                            "Untextured meshes cannot declare UVs or texture dimensions."
                                .to_owned(),
                        );
                    }
                } else {
                    if uvs.len() != vertices.len() {
                        return Err("Textured mesh UVs must match the vertex count.".to_owned());
                    }
                    for uv in uvs {
                        finite_point(*uv)?;
                    }
                    if *texture_width == 0 || *texture_height == 0 {
                        return Err("Textured meshes need nonzero texture dimensions.".to_owned());
                    }
                    if *texture_width > 8_192
                        || *texture_height > 8_192
                        || texture_width.saturating_mul(*texture_height) > 16_777_216
                    {
                        return Err("Mesh textures exceed 16,777,216 pixels.".to_owned());
                    }
                    let decoded = BASE64
                        .decode(texture_pixels)
                        .map_err(|_| "Mesh texture pixels must be valid base64.".to_owned())?;
                    let expected = *texture_width as usize * *texture_height as usize * 4;
                    if decoded.len() != expected {
                        return Err(format!(
                            "Mesh texture data is {} bytes; expected {expected}.",
                            decoded.len()
                        ));
                    }
                    if dark_texture_pixels.is_empty() {
                        if *dark_texture_width != 0 || *dark_texture_height != 0 {
                            return Err(
                                "Dark texture dimensions require dark texture pixels.".to_owned()
                            );
                        }
                    } else {
                        if *dark_texture_width == 0 || *dark_texture_height == 0 {
                            return Err("Dark textures need nonzero texture dimensions.".to_owned());
                        }
                        if *dark_texture_width > 8_192
                            || *dark_texture_height > 8_192
                            || dark_texture_width.saturating_mul(*dark_texture_height) > 16_777_216
                        {
                            return Err("Dark mesh textures exceed 16,777,216 pixels.".to_owned());
                        }
                        let decoded = BASE64.decode(dark_texture_pixels).map_err(|_| {
                            "Dark mesh texture pixels must be valid base64.".to_owned()
                        })?;
                        let expected =
                            *dark_texture_width as usize * *dark_texture_height as usize * 4;
                        if decoded.len() != expected {
                            return Err(format!(
                                "Dark mesh texture data is {} bytes; expected {expected}.",
                                decoded.len()
                            ));
                        }
                    }
                }
                if !gloss.is_finite()
                    || !shadow.is_finite()
                    || !(0.0..=1.0).contains(gloss)
                    || !(0.0..=1.0).contains(shadow)
                {
                    return Err("Mesh gloss and shadow must be within 0–1.".to_owned());
                }
                finite_point_3d(*light_position)?;
                for triangle in triangles {
                    if triangle
                        .iter()
                        .any(|index| *index as usize >= vertices.len())
                    {
                        return Err("Mesh triangle index is out of bounds.".to_owned());
                    }
                }
            }
            NodeKind::Surface {
                vertices,
                patches,
                colors,
                stroke_colors,
                stroke_radii,
                ..
            } => {
                if vertices.len() < 3 || vertices.len() > 1_000_000 {
                    return Err("Surfaces must contain 3–1,000,000 vertices.".to_owned());
                }
                if patches.is_empty() || patches.len() > 2_000_000 {
                    return Err("Surfaces must contain 1–2,000,000 patches.".to_owned());
                }
                for vertex in vertices {
                    finite_point_3d(*vertex)?;
                }
                let mut corner_count = 0usize;
                for patch in patches {
                    if patch.len() < 3 || patch.len() > 1_024 {
                        return Err("Surface patches must contain 3–1,024 corners.".to_owned());
                    }
                    if patch.iter().any(|index| *index as usize >= vertices.len()) {
                        return Err("Surface patch index is out of bounds.".to_owned());
                    }
                    corner_count = corner_count
                        .checked_add(patch.len())
                        .ok_or_else(|| "Surface corner count overflowed.".to_owned())?;
                }
                if colors.len() != corner_count {
                    return Err("Surface colors must match the flattened patch corners.".to_owned());
                }
                if stroke_colors.len() != patches.len() || stroke_radii.len() != patches.len() {
                    return Err(
                        "Surface stroke colors and radii must match the patch count.".to_owned(),
                    );
                }
                for color in colors.iter().chain(stroke_colors) {
                    parse_color(color)?;
                }
                for radius in stroke_radii {
                    if !radius.is_finite() || *radius < 0.0 || *radius > 100.0 {
                        return Err("Surface stroke radii must be within 0–100.".to_owned());
                    }
                }
            }
            NodeKind::CustomShaderMesh {
                vertex_wgsl,
                fragment_wgsl,
                attributes,
                vertex_stride,
                vertex_data,
                indices,
                uniforms,
                ..
            } => {
                if vertex_wgsl.is_empty()
                    || fragment_wgsl.is_empty()
                    || vertex_wgsl.len() > 1_000_000
                    || fragment_wgsl.len() > 1_000_000
                {
                    return Err(
                        "Custom shader programs must contain 1–1,000,000 characters per stage."
                            .to_owned(),
                    );
                }
                if *vertex_stride == 0
                    || *vertex_stride > 2_048
                    || vertex_stride % 4 != 0
                    || attributes.is_empty()
                    || attributes.len() > 16
                {
                    return Err("Custom shader vertex layout is invalid.".to_owned());
                }
                if vertex_data.is_empty()
                    || vertex_data.len() > 16_000_000
                    || vertex_data.iter().any(|value| !value.is_finite())
                    || vertex_data.len() * 4 % *vertex_stride as usize != 0
                {
                    return Err("Custom shader vertex data is invalid.".to_owned());
                }
                let vertex_count = vertex_data.len() * 4 / *vertex_stride as usize;
                let mut locations = HashSet::new();
                for attribute in attributes {
                    if attribute.name.is_empty()
                        || attribute.name.len() > 80
                        || attribute.location >= 16
                        || !locations.insert(attribute.location)
                        || attribute.offset % 4 != 0
                        || attribute.offset + attribute.format.size() > *vertex_stride
                    {
                        return Err("Custom shader attribute layout is invalid.".to_owned());
                    }
                }
                if indices.len() > 16_000_000
                    || indices.iter().any(|index| *index as usize >= vertex_count)
                {
                    return Err("Custom shader indices are invalid.".to_owned());
                }
                let mut bindings = HashSet::new();
                for uniform in uniforms {
                    if uniform.name.is_empty()
                        || uniform.name.len() > 80
                        || !bindings.insert(uniform.binding)
                    {
                        return Err("Custom shader uniform binding is invalid.".to_owned());
                    }
                    if uniform.uniform_type == ShaderUniformType::Sampler2d {
                        let Some(sampler_binding) = uniform.sampler_binding else {
                            return Err("Custom shader sampler2D uniforms need a sampler binding."
                                .to_owned());
                        };
                        if uniform.array_length != 1
                            || !bindings.insert(sampler_binding)
                            || uniform.texture_width == 0
                            || uniform.texture_height == 0
                            || uniform.texture_width > 8_192
                            || uniform.texture_height > 8_192
                            || uniform.texture_width.saturating_mul(uniform.texture_height)
                                > 16_777_216
                        {
                            return Err("Custom shader texture binding is invalid.".to_owned());
                        }
                        let decoded = BASE64.decode(&uniform.texture_pixels).map_err(|_| {
                            "Custom shader texture must be valid base64.".to_owned()
                        })?;
                        let expected =
                            uniform.texture_width as usize * uniform.texture_height as usize * 4;
                        if decoded.len() != expected || !uniform.values.is_empty() {
                            return Err("Custom shader texture data is invalid.".to_owned());
                        }
                    } else if uniform.array_length == 0
                        || uniform.array_length > 256
                        || uniform.sampler_binding.is_some()
                        || !uniform.texture_pixels.is_empty()
                        || uniform.texture_width != 0
                        || uniform.texture_height != 0
                        || uniform.values.len() != uniform.value_count()
                        || uniform.values.iter().any(|value| !value.is_finite())
                    {
                        return Err("Custom shader uniform value is invalid.".to_owned());
                    }
                }
            }
        }
        Ok(())
    }
}

impl Track {
    fn validate(&self, duration: f32, ids: &HashSet<&str>) -> Result<(), String> {
        if self.target != "__camera__" && !ids.contains(self.target.as_str()) {
            return Err(format!("Track target does not exist: {}.", self.target));
        }
        if self.keyframes.is_empty() == self.keyframes_from.is_none() {
            return Err(format!(
                "Track {} must declare exactly one of keyframes or keyframesFrom.",
                self.target
            ));
        }
        if let Some(source) = &self.keyframes_from {
            if source != "__camera__" {
                validate_id(source)?;
                if !ids.contains(source.as_str()) {
                    return Err(format!("Track keyframe source does not exist: {source}."));
                }
            }
        }
        let mut previous = -1.0;
        for keyframe in &self.keyframes {
            if !keyframe.at.is_finite()
                || keyframe.at < previous
                || !(0.0..=duration).contains(&keyframe.at)
            {
                return Err(format!("Track {} has invalid keyframe times.", self.target));
            }
            self.property.validate_value(&keyframe.value)?;
            previous = keyframe.at;
        }
        if self.property.is_camera() && self.target != "__camera__" {
            return Err("Camera properties must target __camera__.".to_owned());
        }
        Ok(())
    }

    fn resolved_keyframes<'a>(&'a self, tracks: &'a [Track]) -> Result<&'a [Keyframe], String> {
        let Some(source) = self.keyframes_from.as_deref() else {
            return Ok(&self.keyframes);
        };
        let mut matches = tracks
            .iter()
            .filter(|candidate| candidate.target == source && candidate.property == self.property);
        let source_track = matches.next().ok_or_else(|| {
            format!(
                "Track {} references missing {:?} keyframes on {source}.",
                self.target, self.property
            )
        })?;
        if matches.next().is_some() {
            return Err(format!(
                "Track {} references ambiguous {:?} keyframes on {source}.",
                self.target, self.property
            ));
        }
        if source_track.keyframes_from.is_some() || source_track.keyframes.is_empty() {
            return Err(format!(
                "Track {} must reference a concrete keyframe track.",
                self.target
            ));
        }
        Ok(&source_track.keyframes)
    }
}

impl Property {
    fn is_numeric(self) -> bool {
        !matches!(
            self,
            Self::Fill
                | Self::FillGradient
                | Self::Stroke
                | Self::StrokeGradient
                | Self::Points
                | Self::Vertices
                | Self::Normals
                | Self::Colors
                | Self::SurfaceColors
                | Self::StrokeRadii
                | Self::LightPosition
                | Self::Commands
                | Self::PathData
                | Self::Transform2d
                | Self::Affine2d
                | Self::BillboardAnchor
                | Self::DrawRange
                | Self::Camera3dPosition
                | Self::Camera3dTarget
                | Self::Camera3dUp
                | Self::Camera3d
                | Self::Camera3dOrbit
                | Self::ShaderVertexData
                | Self::ShaderUniformValues
        )
    }

    fn is_camera(self) -> bool {
        matches!(
            self,
            Self::CameraX
                | Self::CameraY
                | Self::CameraZoom
                | Self::CameraRotation
                | Self::Camera3dPosition
                | Self::Camera3dTarget
                | Self::Camera3dUp
                | Self::Camera3dFovY
                | Self::Camera3d
                | Self::Camera3dOrbit
        )
    }

    fn validate_value(self, value: &TrackValue) -> Result<(), String> {
        match (self, value) {
            (Self::Fill | Self::Stroke, TrackValue::Color(color)) => {
                parse_color(color)?;
                Ok(())
            }
            (Self::FillGradient | Self::StrokeGradient, TrackValue::Gradient(gradient)) => {
                validate_gradient(gradient)
            }
            (Self::Points, TrackValue::Points(points)) => validate_points(points, 1),
            (Self::Vertices | Self::Normals, TrackValue::Points3d(points)) => {
                if points.len() < 3 || points.len() > 1_000_000 {
                    return Err(
                        "Mesh vertex and normal tracks must contain 3–1,000,000 points.".to_owned(),
                    );
                }
                for point in points {
                    finite_point_3d(*point)?;
                }
                Ok(())
            }
            (Self::Colors | Self::SurfaceColors, TrackValue::Colors(colors)) => {
                if colors.is_empty() || colors.len() > 1_000_000 {
                    return Err("Color tracks must contain 1–1,000,000 colors.".to_owned());
                }
                for color in colors {
                    parse_color(color)?;
                }
                Ok(())
            }
            (Self::StrokeRadii, TrackValue::Numbers(radii)) => {
                if radii.is_empty() || radii.len() > 2_000_000 {
                    return Err(
                        "Surface stroke-radius tracks must contain 1–2,000,000 values.".to_owned(),
                    );
                }
                if radii
                    .iter()
                    .any(|radius| !radius.is_finite() || !(0.0..=100.0).contains(radius))
                {
                    return Err("Surface stroke radii must be within 0–100.".to_owned());
                }
                Ok(())
            }
            (Self::LightPosition, TrackValue::Points3d(points)) if points.len() == 1 => {
                finite_point_3d(points[0])
            }
            (
                Self::BillboardAnchor
                | Self::Camera3dPosition
                | Self::Camera3dTarget
                | Self::Camera3dUp
                | Self::Camera3dOrbit,
                TrackValue::Point3d(point),
            ) => finite_point_3d(*point),
            (
                Self::BillboardAnchor
                | Self::Camera3dPosition
                | Self::Camera3dTarget
                | Self::Camera3dUp
                | Self::Camera3dOrbit,
                TrackValue::Numbers(point),
            ) if point.len() == 3 => finite_point_3d([point[0], point[1], point[2]]),
            (Self::Commands, TrackValue::PathCommands(commands)) => {
                validate_path_commands(commands)
            }
            (Self::PathData, TrackValue::Numbers(values))
                if !values.is_empty()
                    && values.len() <= 300_000
                    && values.iter().all(|value| value.is_finite()) =>
            {
                Ok(())
            }
            (Self::Transform2d, TrackValue::Numbers(values))
                if matches!(values.len(), 5 | 6)
                    && values.iter().all(|value| value.is_finite()) =>
            {
                Ok(())
            }
            (Self::Affine2d, TrackValue::Numbers(values))
                if matches!(values.len(), 6 | 7)
                    && values.iter().all(|value| value.is_finite()) =>
            {
                Ok(())
            }
            (Self::DrawRange, TrackValue::Numbers(values))
                if values.len() == 2
                    && values[0].is_finite()
                    && values[1].is_finite()
                    && (0.0..=1.0).contains(&values[0])
                    && (values[0]..=1.0).contains(&values[1]) =>
            {
                Ok(())
            }
            (Self::Camera3d, TrackValue::Numbers(values))
                if valid_camera_3d_track_value(values) =>
            {
                Ok(())
            }
            (Self::ShaderVertexData | Self::ShaderUniformValues, TrackValue::Numbers(values))
                if !values.is_empty()
                    && values.len() <= 16_000_000
                    && values.iter().all(|value| value.is_finite()) =>
            {
                Ok(())
            }
            (property, TrackValue::Number(value)) if property.is_numeric() && value.is_finite() => {
                Ok(())
            }
            _ => Err(format!("Track value does not match property {self:?}.")),
        }
    }
}

impl Expr {
    fn validate(&self, signals: &HashSet<&str>, depth: usize) -> Result<(), String> {
        if depth > 32 {
            return Err("Expression nesting exceeds 32 levels.".to_owned());
        }
        match self {
            Self::Constant { value } if !value.is_finite() => {
                Err("Expression constants must be finite.".to_owned())
            }
            Self::Signal { id } if !signals.contains(id.as_str()) => {
                Err(format!("Expression references missing signal: {id}."))
            }
            Self::Add { args }
            | Self::Multiply { args }
            | Self::Min { args }
            | Self::Max { args } => {
                if args.is_empty() || args.len() > 32 {
                    return Err("Expression argument count must be 1–32.".to_owned());
                }
                for arg in args {
                    arg.validate(signals, depth + 1)?;
                }
                Ok(())
            }
            Self::Subtract { left, right } | Self::Divide { left, right } => {
                left.validate(signals, depth + 1)?;
                right.validate(signals, depth + 1)
            }
            Self::Sin { value }
            | Self::Cos { value }
            | Self::Abs { value }
            | Self::Clamp { value, .. } => value.validate(signals, depth + 1),
            Self::Lerp { from, to, amount } => {
                from.validate(signals, depth + 1)?;
                to.validate(signals, depth + 1)?;
                amount.validate(signals, depth + 1)
            }
            _ => Ok(()),
        }
    }

    fn evaluate(
        &self,
        time: f32,
        signals: &HashMap<&str, f32>,
        depth: usize,
    ) -> Result<f32, String> {
        if depth > 32 {
            return Err("Expression recursion limit reached.".to_owned());
        }
        let next = depth + 1;
        let value = match self {
            Self::Constant { value } => *value,
            Self::Time => time,
            Self::Signal { id } => *signals
                .get(id.as_str())
                .ok_or_else(|| format!("Missing signal: {id}."))?,
            Self::Add { args } => args
                .iter()
                .map(|arg| arg.evaluate(time, signals, next))
                .sum::<Result<f32, _>>()?,
            Self::Multiply { args } => args
                .iter()
                .map(|arg| arg.evaluate(time, signals, next))
                .try_fold(1.0, |acc, value| value.map(|value| acc * value))?,
            Self::Subtract { left, right } => {
                left.evaluate(time, signals, next)? - right.evaluate(time, signals, next)?
            }
            Self::Divide { left, right } => {
                let denominator = right.evaluate(time, signals, next)?;
                if denominator.abs() < f32::EPSILON {
                    0.0
                } else {
                    left.evaluate(time, signals, next)? / denominator
                }
            }
            Self::Sin { value } => value.evaluate(time, signals, next)?.sin(),
            Self::Cos { value } => value.evaluate(time, signals, next)?.cos(),
            Self::Abs { value } => value.evaluate(time, signals, next)?.abs(),
            Self::Min { args } => args
                .iter()
                .map(|arg| arg.evaluate(time, signals, next))
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .reduce(f32::min)
                .unwrap_or(0.0),
            Self::Max { args } => args
                .iter()
                .map(|arg| arg.evaluate(time, signals, next))
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .reduce(f32::max)
                .unwrap_or(0.0),
            Self::Clamp { value, min, max } => {
                value.evaluate(time, signals, next)?.clamp(*min, *max)
            }
            Self::Lerp { from, to, amount } => {
                let from = from.evaluate(time, signals, next)?;
                let to = to.evaluate(time, signals, next)?;
                let amount = amount.evaluate(time, signals, next)?;
                from + (to - from) * amount
            }
        };
        Ok(if value.is_finite() { value } else { 0.0 })
    }
}

fn valid_camera_3d_track_value(values: &[f32]) -> bool {
    if values.len() < 2 || values.iter().any(|value| !value.is_finite()) {
        return false;
    }
    let mask = values[0] as u8;
    if values[0] != f32::from(mask) || !(1..=15).contains(&mask) {
        return false;
    }
    let expected = 1
        + usize::from(mask & 1 != 0) * 3
        + usize::from(mask & 2 != 0) * 3
        + usize::from(mask & 4 != 0) * 3
        + usize::from(mask & 8 != 0);
    if values.len() != expected {
        return false;
    }
    mask & 8 == 0 || (0.05..PI).contains(values.last().unwrap())
}

fn camera_orbit_up(position: [f32; 3], target: [f32; 3]) -> [f32; 3] {
    let direction = [
        position[0] - target[0],
        position[1] - target[1],
        position[2] - target[2],
    ];
    let length =
        (direction[0] * direction[0] + direction[1] * direction[1] + direction[2] * direction[2])
            .sqrt()
            .max(0.000_001);
    let forward = [
        direction[0] / length,
        direction[1] / length,
        direction[2] / length,
    ];
    let mut up = [
        -forward[2] * forward[0],
        -forward[2] * forward[1],
        1.0 - forward[2] * forward[2],
    ];
    let up_length = (up[0] * up[0] + up[1] * up[1] + up[2] * up[2]).sqrt();
    if up_length <= 0.000_001 {
        up = [0.0, 1.0, 0.0];
    } else {
        up = [up[0] / up_length, up[1] / up_length, up[2] / up_length];
    }
    up
}

fn project_camera_3d_point(
    point: [f32; 3],
    camera: Camera3d,
    width: f32,
    height: f32,
) -> Option<[f32; 2]> {
    fn subtract(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
        [left[0] - right[0], left[1] - right[1], left[2] - right[2]]
    }
    fn dot(left: [f32; 3], right: [f32; 3]) -> f32 {
        left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
    }
    fn cross(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
        [
            left[1] * right[2] - left[2] * right[1],
            left[2] * right[0] - left[0] * right[2],
            left[0] * right[1] - left[1] * right[0],
        ]
    }
    fn normalize(value: [f32; 3]) -> Option<[f32; 3]> {
        let length = dot(value, value).sqrt();
        (length > 0.000_001).then(|| [value[0] / length, value[1] / length, value[2] / length])
    }
    let forward = normalize(subtract(camera.target, camera.position))?;
    let right = normalize(cross(forward, camera.up))?;
    let camera_up = normalize(cross(right, forward))?;
    let relative = subtract(point, camera.position);
    let view_z = dot(relative, forward);
    if !(camera.near..=camera.far).contains(&view_z) {
        return None;
    }
    let tan_half_fov = (camera.fov_y * 0.5).tan().max(0.0001);
    let aspect = (width / height).max(0.0001);
    Some([
        dot(relative, right) / (view_z * tan_half_fov * aspect) * width * 0.5,
        dot(relative, camera_up) / (view_z * tan_half_fov) * height * 0.5,
    ])
}

fn apply_value<'a>(
    target: &'a str,
    property: Property,
    value: TrackValue,
    runtime: &mut RuntimeValues<'a>,
) -> Result<(), String> {
    match (property, value) {
        (Property::BillboardAnchor, TrackValue::Point3d(value)) => {
            runtime.billboard_anchor_overrides.insert(target, value);
        }
        (Property::BillboardAnchor, TrackValue::Numbers(value)) if value.len() == 3 => {
            runtime
                .billboard_anchor_overrides
                .insert(target, [value[0], value[1], value[2]]);
        }
        (Property::CameraX, TrackValue::Number(value)) => runtime.camera.x = value,
        (Property::CameraY, TrackValue::Number(value)) => runtime.camera.y = value,
        (Property::CameraZoom, TrackValue::Number(value)) => {
            runtime.camera.zoom = value.max(0.001);
        }
        (Property::CameraRotation, TrackValue::Number(value)) => runtime.camera.rotation = value,
        (Property::Camera3dPosition, TrackValue::Point3d(value)) => {
            runtime.camera_3d.position = value;
        }
        (Property::Camera3dTarget, TrackValue::Point3d(value)) => {
            runtime.camera_3d.target = value;
        }
        (Property::Camera3dUp, TrackValue::Point3d(value)) => {
            runtime.camera_3d.up = value;
        }
        (Property::Camera3dOrbit, TrackValue::Point3d(value)) => {
            runtime.camera_3d.position = value;
            runtime.camera_3d.up = camera_orbit_up(value, runtime.camera_3d.target);
        }
        (
            Property::Camera3dPosition
            | Property::Camera3dTarget
            | Property::Camera3dUp
            | Property::Camera3dOrbit,
            TrackValue::Numbers(value),
        ) if value.len() == 3 => {
            let point = [value[0], value[1], value[2]];
            match property {
                Property::Camera3dPosition => runtime.camera_3d.position = point,
                Property::Camera3dTarget => runtime.camera_3d.target = point,
                Property::Camera3dUp => runtime.camera_3d.up = point,
                Property::Camera3dOrbit => {
                    runtime.camera_3d.position = point;
                    runtime.camera_3d.up = camera_orbit_up(point, runtime.camera_3d.target);
                }
                _ => unreachable!(),
            }
        }
        (Property::Camera3dFovY, TrackValue::Number(value)) => {
            runtime.camera_3d.fov_y = value.clamp(0.05, PI - 0.0001);
        }
        (Property::Camera3d, TrackValue::Numbers(value)) if valid_camera_3d_track_value(&value) => {
            let mask = value[0] as u8;
            let mut index = 1;
            if mask & 1 != 0 {
                runtime.camera_3d.position = [value[index], value[index + 1], value[index + 2]];
                index += 3;
            }
            if mask & 2 != 0 {
                runtime.camera_3d.target = [value[index], value[index + 1], value[index + 2]];
                index += 3;
            }
            if mask & 4 != 0 {
                runtime.camera_3d.up = [value[index], value[index + 1], value[index + 2]];
                index += 3;
            }
            if mask & 8 != 0 {
                runtime.camera_3d.fov_y = value[index].clamp(0.05, PI - 0.0001);
            }
        }
        (Property::Fill, TrackValue::Color(value)) => {
            runtime.fill_overrides.insert(target, value);
        }
        (Property::FillGradient, TrackValue::Gradient(value)) => {
            runtime.fill_gradient_overrides.insert(target, value);
        }
        (Property::Stroke, TrackValue::Color(value)) => {
            runtime.stroke_overrides.insert(target, value);
        }
        (Property::StrokeGradient, TrackValue::Gradient(value)) => {
            runtime.stroke_gradient_overrides.insert(target, value);
        }
        (Property::Points, TrackValue::Points(value)) => {
            runtime.point_overrides.insert(target, value);
        }
        (Property::Vertices, TrackValue::Points3d(value)) => {
            runtime.vertex_overrides.insert(target, value);
        }
        (Property::Normals, TrackValue::Points3d(value)) => {
            runtime.normal_overrides.insert(target, value);
        }
        (Property::Colors, TrackValue::Colors(value)) => {
            runtime.color_overrides.insert(target, value);
        }
        (Property::SurfaceColors, TrackValue::Colors(value)) => {
            runtime.surface_color_overrides.insert(target, value);
        }
        (Property::StrokeRadii, TrackValue::Numbers(value)) => {
            runtime.stroke_radii_overrides.insert(target, value);
        }
        (Property::LightPosition, TrackValue::Points3d(value)) if value.len() == 1 => {
            runtime.light_position_overrides.insert(target, value[0]);
        }
        (Property::Commands, TrackValue::PathCommands(value)) => {
            runtime.command_overrides.insert(target, value);
        }
        (Property::PathData, TrackValue::Numbers(value)) => {
            runtime.path_data_overrides.insert(target, value);
        }
        (Property::DrawRange, TrackValue::Numbers(value)) if value.len() == 2 => {
            let state = runtime
                .states
                .get_mut(target)
                .ok_or_else(|| format!("Missing draw-range target: {target}."))?;
            state.draw_start = value[0].clamp(0.0, 1.0);
            state.draw_progress = value[1].clamp(0.0, 1.0);
        }
        (Property::Transform2d, TrackValue::Numbers(value)) if matches!(value.len(), 5 | 6) => {
            let state = runtime
                .states
                .get_mut(target)
                .ok_or_else(|| format!("Missing transform target: {target}."))?;
            state.transform.x = value[0];
            state.transform.y = value[1];
            state.transform.rotation = value[2];
            state.transform.scale_x = value[3];
            state.transform.scale_y = value[4];
            if value.len() == 6 {
                state.stroke_width = value[5].max(0.0);
            }
        }
        (Property::Affine2d, TrackValue::Numbers(value)) if matches!(value.len(), 6 | 7) => {
            let state = runtime
                .states
                .get_mut(target)
                .ok_or_else(|| format!("Missing affine target: {target}."))?;
            state.affine_2d = Some([value[0], value[1], value[2], value[3], value[4], value[5]]);
            if value.len() == 7 {
                state.stroke_width = value[6].max(0.0);
            }
        }
        (Property::ShaderVertexData, TrackValue::Numbers(value)) => {
            runtime.shader_vertex_overrides.insert(target, value);
        }
        (Property::ShaderUniformValues, TrackValue::Numbers(value)) => {
            runtime.shader_uniform_overrides.insert(target, value);
        }
        (property, TrackValue::Number(value)) => {
            let state = runtime
                .states
                .get_mut(target)
                .ok_or_else(|| format!("Missing numeric target: {target}."))?;
            match property {
                Property::X => state.transform.x = value,
                Property::Y => state.transform.y = value,
                Property::Z => state.transform.z = value,
                Property::Rotation => state.transform.rotation = value,
                Property::RotationX => state.transform.rotation_x = value,
                Property::RotationY => state.transform.rotation_y = value,
                Property::ScaleX => state.transform.scale_x = value,
                Property::ScaleY => state.transform.scale_y = value,
                Property::ScaleZ => state.transform.scale_z = value,
                Property::Opacity => state.opacity = value.clamp(0.0, 1.0),
                Property::StrokeWidth => state.stroke_width = value.max(0.0),
                Property::DrawStart => state.draw_start = value.clamp(0.0, 1.0),
                Property::DrawProgress => state.draw_progress = value.clamp(0.0, 1.0),
                Property::Radius => state.radius = Some(value.max(0.0)),
                _ => return Err(format!("Numeric value cannot target {property:?}.")),
            }
        }
        _ => return Err(format!("Track value type does not match {property:?}.")),
    }
    Ok(())
}

fn resolve_matrix<'a>(
    id: &'a str,
    nodes: &HashMap<&'a str, &'a Node>,
    states: &HashMap<&'a str, NodeState>,
    cache: &mut HashMap<&'a str, [f32; 6]>,
    visiting: &mut HashSet<&'a str>,
) -> Result<[f32; 6], String> {
    if let Some(matrix) = cache.get(id) {
        return Ok(*matrix);
    }
    if !visiting.insert(id) {
        return Err(format!("Parent cycle while evaluating {id}."));
    }
    let node = nodes
        .get(id)
        .ok_or_else(|| format!("Missing node while evaluating {id}."))?;
    let state = states
        .get(id)
        .ok_or_else(|| format!("Missing state while evaluating {id}."))?;
    let local = state
        .affine_2d
        .unwrap_or_else(|| transform_matrix(state.transform));
    let world = if let Some(parent) = node.parent.as_deref() {
        multiply_matrix(
            resolve_matrix(parent, nodes, states, cache, visiting)?,
            local,
        )
    } else {
        local
    };
    visiting.remove(id);
    cache.insert(id, world);
    Ok(world)
}

fn transform_matrix(transform: Transform) -> [f32; 6] {
    let (sin, cos) = transform.rotation.sin_cos();
    [
        cos * transform.scale_x,
        sin * transform.scale_x,
        -sin * transform.scale_y,
        cos * transform.scale_y,
        transform.x,
        transform.y,
    ]
}

fn multiply_matrix(a: [f32; 6], b: [f32; 6]) -> [f32; 6] {
    [
        a[0] * b[0] + a[2] * b[1],
        a[1] * b[0] + a[3] * b[1],
        a[0] * b[2] + a[2] * b[3],
        a[1] * b[2] + a[3] * b[3],
        a[0] * b[4] + a[2] * b[5] + a[4],
        a[1] * b[4] + a[3] * b[5] + a[5],
    ]
}

fn sample_track_values(keyframes: &[Keyframe], time: f32) -> TrackValue {
    if keyframes.len() == 1 || time <= keyframes[0].at {
        return keyframes[0].value.clone();
    }
    for pair in keyframes.windows(2) {
        if time <= pair[1].at {
            let span = (pair[1].at - pair[0].at).max(f32::EPSILON);
            let amount = pair[1].easing.apply((time - pair[0].at) / span);
            return interpolate_value(&pair[0].value, &pair[1].value, amount);
        }
    }
    keyframes
        .last()
        .map(|keyframe| keyframe.value.clone())
        .unwrap_or(TrackValue::Number(0.0))
}

fn interpolate_value(from: &TrackValue, to: &TrackValue, amount: f32) -> TrackValue {
    match (from, to) {
        (TrackValue::Number(from), TrackValue::Number(to)) => {
            TrackValue::Number(from + (to - from) * amount)
        }
        (TrackValue::Numbers(from), TrackValue::Numbers(to)) if from.len() == to.len() => {
            TrackValue::Numbers(
                from.iter()
                    .zip(to)
                    .map(|(from, to)| from + (to - from) * amount)
                    .collect(),
            )
        }
        (TrackValue::Point3d(from), TrackValue::Point3d(to)) => TrackValue::Point3d([
            from[0] + (to[0] - from[0]) * amount,
            from[1] + (to[1] - from[1]) * amount,
            from[2] + (to[2] - from[2]) * amount,
        ]),
        (TrackValue::Color(from), TrackValue::Color(to)) => {
            let from = parse_color(from).unwrap_or([0.0, 0.0, 0.0, 1.0]);
            let to = parse_color(to).unwrap_or(from);
            TrackValue::Color(format_color([
                from[0] + (to[0] - from[0]) * amount,
                from[1] + (to[1] - from[1]) * amount,
                from[2] + (to[2] - from[2]) * amount,
                from[3] + (to[3] - from[3]) * amount,
            ]))
        }
        (TrackValue::Colors(from), TrackValue::Colors(to)) if from.len() == to.len() => {
            TrackValue::Colors(
                from.iter()
                    .zip(to)
                    .map(|(from, to)| {
                        let from = parse_color(from).unwrap_or([0.0, 0.0, 0.0, 1.0]);
                        let to = parse_color(to).unwrap_or(from);
                        format_color([
                            from[0] + (to[0] - from[0]) * amount,
                            from[1] + (to[1] - from[1]) * amount,
                            from[2] + (to[2] - from[2]) * amount,
                            from[3] + (to[3] - from[3]) * amount,
                        ])
                    })
                    .collect(),
            )
        }
        (TrackValue::Points(from), TrackValue::Points(to)) if from.len() == to.len() => {
            TrackValue::Points(
                from.iter()
                    .zip(to)
                    .map(|(from, to)| {
                        [
                            from[0] + (to[0] - from[0]) * amount,
                            from[1] + (to[1] - from[1]) * amount,
                        ]
                    })
                    .collect(),
            )
        }
        (TrackValue::Points3d(from), TrackValue::Points3d(to)) if from.len() == to.len() => {
            TrackValue::Points3d(
                from.iter()
                    .zip(to)
                    .map(|(from, to)| {
                        [
                            from[0] + (to[0] - from[0]) * amount,
                            from[1] + (to[1] - from[1]) * amount,
                            from[2] + (to[2] - from[2]) * amount,
                        ]
                    })
                    .collect(),
            )
        }
        (TrackValue::PathCommands(from), TrackValue::PathCommands(to))
            if from.len() == to.len() =>
        {
            interpolate_path_commands(from, to, amount)
                .map(TrackValue::PathCommands)
                .unwrap_or_else(|| {
                    if amount < 0.5 {
                        TrackValue::PathCommands(from.clone())
                    } else {
                        TrackValue::PathCommands(to.clone())
                    }
                })
        }
        (TrackValue::Gradient(from), TrackValue::Gradient(to))
            if from.stops.len() == to.stops.len() =>
        {
            TrackValue::Gradient(interpolate_gradient(from, to, amount))
        }
        _ if amount < 0.5 => from.clone(),
        _ => to.clone(),
    }
}

fn interpolate_path_commands(
    from: &[PathCommand],
    to: &[PathCommand],
    amount: f32,
) -> Option<Vec<PathCommand>> {
    let lerp = |left: f32, right: f32| left + (right - left) * amount;
    from.iter()
        .zip(to)
        .map(|(from, to)| match (from, to) {
            (PathCommand::MoveTo { x: x1, y: y1 }, PathCommand::MoveTo { x: x2, y: y2 }) => {
                Some(PathCommand::MoveTo {
                    x: lerp(*x1, *x2),
                    y: lerp(*y1, *y2),
                })
            }
            (PathCommand::LineTo { x: x1, y: y1 }, PathCommand::LineTo { x: x2, y: y2 }) => {
                Some(PathCommand::LineTo {
                    x: lerp(*x1, *x2),
                    y: lerp(*y1, *y2),
                })
            }
            (
                PathCommand::QuadTo {
                    cx: cx1,
                    cy: cy1,
                    x: x1,
                    y: y1,
                },
                PathCommand::QuadTo {
                    cx: cx2,
                    cy: cy2,
                    x: x2,
                    y: y2,
                },
            ) => Some(PathCommand::QuadTo {
                cx: lerp(*cx1, *cx2),
                cy: lerp(*cy1, *cy2),
                x: lerp(*x1, *x2),
                y: lerp(*y1, *y2),
            }),
            (
                PathCommand::CubicTo {
                    c1x: c1x1,
                    c1y: c1y1,
                    c2x: c2x1,
                    c2y: c2y1,
                    x: x1,
                    y: y1,
                },
                PathCommand::CubicTo {
                    c1x: c1x2,
                    c1y: c1y2,
                    c2x: c2x2,
                    c2y: c2y2,
                    x: x2,
                    y: y2,
                },
            ) => Some(PathCommand::CubicTo {
                c1x: lerp(*c1x1, *c1x2),
                c1y: lerp(*c1y1, *c1y2),
                c2x: lerp(*c2x1, *c2x2),
                c2y: lerp(*c2y1, *c2y2),
                x: lerp(*x1, *x2),
                y: lerp(*y1, *y2),
            }),
            (PathCommand::Close, PathCommand::Close) => Some(PathCommand::Close),
            _ => None,
        })
        .collect()
}

fn validate_gradient(gradient: &LinearGradient) -> Result<(), String> {
    finite_point(gradient.from)?;
    finite_point(gradient.to)?;
    let dx = gradient.to[0] - gradient.from[0];
    let dy = gradient.to[1] - gradient.from[1];
    if dx * dx + dy * dy <= f32::EPSILON {
        return Err("Gradient endpoints must be distinct.".to_owned());
    }
    if gradient.stops.len() < 2 || gradient.stops.len() > 64 {
        return Err("Gradients must contain 2–64 stops.".to_owned());
    }
    let mut previous = -1.0;
    for stop in &gradient.stops {
        if !stop.offset.is_finite() || !(0.0..=1.0).contains(&stop.offset) || stop.offset < previous
        {
            return Err("Gradient offsets must be ordered within 0–1.".to_owned());
        }
        parse_color(&stop.color)?;
        previous = stop.offset;
    }
    Ok(())
}

fn evaluate_gradient(
    gradient: &LinearGradient,
    opacity: f32,
) -> Result<EvaluatedLinearGradient, String> {
    Ok(EvaluatedLinearGradient {
        from: gradient.from,
        to: gradient.to,
        spread: gradient.spread,
        space: gradient.space,
        stops: gradient
            .stops
            .iter()
            .map(|stop| {
                parse_color(&stop.color).map(|mut color| {
                    color[3] *= opacity;
                    EvaluatedGradientStop {
                        offset: stop.offset,
                        color,
                    }
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
    })
}

fn interpolate_gradient(from: &LinearGradient, to: &LinearGradient, amount: f32) -> LinearGradient {
    let lerp = |left: f32, right: f32| left + (right - left) * amount;
    LinearGradient {
        from: [
            lerp(from.from[0], to.from[0]),
            lerp(from.from[1], to.from[1]),
        ],
        to: [lerp(from.to[0], to.to[0]), lerp(from.to[1], to.to[1])],
        spread: if amount < 0.5 { from.spread } else { to.spread },
        space: if amount < 0.5 { from.space } else { to.space },
        stops: from
            .stops
            .iter()
            .zip(&to.stops)
            .map(|(from, to)| {
                let from_color = parse_color(&from.color).unwrap_or([0.0, 0.0, 0.0, 0.0]);
                let to_color = parse_color(&to.color).unwrap_or(from_color);
                GradientStop {
                    offset: lerp(from.offset, to.offset),
                    color: format_color([
                        lerp(from_color[0], to_color[0]),
                        lerp(from_color[1], to_color[1]),
                        lerp(from_color[2], to_color[2]),
                        lerp(from_color[3], to_color[3]),
                    ]),
                }
            })
            .collect(),
    }
}

fn validate_number_keyframes(keyframes: &[NumberKeyframe], duration: f32) -> Result<(), String> {
    if keyframes.is_empty() {
        return Err("Signals require at least one keyframe.".to_owned());
    }
    let mut previous = -1.0;
    for keyframe in keyframes {
        if !keyframe.at.is_finite()
            || !keyframe.value.is_finite()
            || keyframe.at < previous
            || !(0.0..=duration).contains(&keyframe.at)
        {
            return Err("Signal keyframes are invalid.".to_owned());
        }
        previous = keyframe.at;
    }
    Ok(())
}

fn sample_number_keyframes(keyframes: &[NumberKeyframe], time: f32) -> f32 {
    if keyframes.len() == 1 || time <= keyframes[0].at {
        return keyframes[0].value;
    }
    for pair in keyframes.windows(2) {
        if time <= pair[1].at {
            let span = (pair[1].at - pair[0].at).max(f32::EPSILON);
            let amount = pair[1].easing.apply((time - pair[0].at) / span);
            return pair[0].value + (pair[1].value - pair[0].value) * amount;
        }
    }
    keyframes.last().map_or(0.0, |keyframe| keyframe.value)
}

impl Easing {
    fn apply(self, amount: f32) -> f32 {
        let amount = amount.clamp(0.0, 1.0);
        match self {
            Self::Linear => amount,
            Self::Smooth => amount * amount * (3.0 - 2.0 * amount),
            Self::EaseIn => amount * amount,
            Self::EaseOut => 1.0 - (1.0 - amount) * (1.0 - amount),
            Self::EaseInOut => {
                if amount < 0.5 {
                    2.0 * amount * amount
                } else {
                    1.0 - (-2.0 * amount + 2.0).powi(2) / 2.0
                }
            }
            Self::ThereAndBack => (amount * PI).sin(),
            Self::Bounce => {
                let n1 = 7.5625;
                let d1 = 2.75;
                if amount < 1.0 / d1 {
                    n1 * amount * amount
                } else if amount < 2.0 / d1 {
                    let amount = amount - 1.5 / d1;
                    n1 * amount * amount + 0.75
                } else if amount < 2.5 / d1 {
                    let amount = amount - 2.25 / d1;
                    n1 * amount * amount + 0.9375
                } else {
                    let amount = amount - 2.625 / d1;
                    n1 * amount * amount + 0.984_375
                }
            }
        }
    }
}

pub fn parse_color(value: &str) -> Result<[f32; 4], String> {
    let value = value
        .strip_prefix('#')
        .ok_or_else(|| "Colors must begin with #.".to_owned())?;
    let (rgb, alpha) = match value.len() {
        6 => (value, "ff"),
        8 => (&value[..6], &value[6..]),
        _ => return Err("Colors must contain six or eight hexadecimal digits.".to_owned()),
    };
    let channel = |range: std::ops::Range<usize>| {
        u8::from_str_radix(&rgb[range], 16)
            .map(|value| f32::from(value) / 255.0)
            .map_err(|_| "Color contains invalid hexadecimal digits.".to_owned())
    };
    let alpha = u8::from_str_radix(alpha, 16)
        .map(|value| f32::from(value) / 255.0)
        .map_err(|_| "Color contains invalid alpha.".to_owned())?;
    Ok([channel(0..2)?, channel(2..4)?, channel(4..6)?, alpha])
}

fn format_color(color: [f32; 4]) -> String {
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!(
        "#{:02x}{:02x}{:02x}{:02x}",
        channel(color[0]),
        channel(color[1]),
        channel(color[2]),
        channel(color[3])
    )
}

fn validate_id(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 80
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "_-.".contains(character))
    {
        return Err(format!("Invalid id: {value}."));
    }
    Ok(())
}

fn validate_points(points: &[[f32; 2]], minimum: usize) -> Result<(), String> {
    if points.len() < minimum || points.len() > 100_000 {
        return Err(format!("Point list must contain {minimum}–100,000 points."));
    }
    for point in points {
        finite_point(*point)?;
    }
    Ok(())
}

fn path_command_data_len(commands: &[PathCommand]) -> usize {
    commands
        .iter()
        .map(|command| match command {
            PathCommand::MoveTo { .. } | PathCommand::LineTo { .. } => 2,
            PathCommand::QuadTo { .. } => 4,
            PathCommand::CubicTo { .. } => 6,
            PathCommand::Close => 0,
        })
        .sum()
}

fn path_commands_with_data(template: &[PathCommand], data: &[f32]) -> Option<Vec<PathCommand>> {
    if path_command_data_len(template) != data.len() {
        return None;
    }
    let mut index = 0;
    let mut next = || {
        let value = data.get(index).copied();
        index += 1;
        value
    };
    template
        .iter()
        .map(|command| {
            Some(match command {
                PathCommand::MoveTo { .. } => PathCommand::MoveTo {
                    x: next()?,
                    y: next()?,
                },
                PathCommand::LineTo { .. } => PathCommand::LineTo {
                    x: next()?,
                    y: next()?,
                },
                PathCommand::QuadTo { .. } => PathCommand::QuadTo {
                    cx: next()?,
                    cy: next()?,
                    x: next()?,
                    y: next()?,
                },
                PathCommand::CubicTo { .. } => PathCommand::CubicTo {
                    c1x: next()?,
                    c1y: next()?,
                    c2x: next()?,
                    c2y: next()?,
                    x: next()?,
                    y: next()?,
                },
                PathCommand::Close => PathCommand::Close,
            })
        })
        .collect()
}

fn validate_path_commands(commands: &[PathCommand]) -> Result<(), String> {
    if commands.is_empty() || commands.len() > 50_000 {
        return Err("Path must contain 1–50,000 commands.".to_owned());
    }
    let finite = commands.iter().all(|command| match command {
        PathCommand::MoveTo { x, y } | PathCommand::LineTo { x, y } => {
            x.is_finite() && y.is_finite()
        }
        PathCommand::QuadTo { cx, cy, x, y } => {
            cx.is_finite() && cy.is_finite() && x.is_finite() && y.is_finite()
        }
        PathCommand::CubicTo {
            c1x,
            c1y,
            c2x,
            c2y,
            x,
            y,
        } => [c1x, c1y, c2x, c2y, x, y]
            .iter()
            .all(|value| value.is_finite()),
        PathCommand::Close => true,
    });
    if finite {
        Ok(())
    } else {
        Err("Path commands must be finite.".to_owned())
    }
}

fn validate_path_commands_3d(commands: &[PathCommand3d]) -> Result<(), String> {
    if commands.is_empty() || commands.len() > 50_000 {
        return Err("3D path must contain 1–50,000 commands.".to_owned());
    }
    let finite = commands.iter().all(|command| match command {
        PathCommand3d::MoveTo { x, y, z } | PathCommand3d::LineTo { x, y, z } => {
            [x, y, z].iter().all(|value| value.is_finite())
        }
        PathCommand3d::QuadTo {
            cx,
            cy,
            cz,
            x,
            y,
            z,
        } => [cx, cy, cz, x, y, z].iter().all(|value| value.is_finite()),
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
        } => [c1x, c1y, c1z, c2x, c2y, c2z, x, y, z]
            .iter()
            .all(|value| value.is_finite()),
        PathCommand3d::Close => true,
    });
    if finite {
        Ok(())
    } else {
        Err("3D path commands must be finite.".to_owned())
    }
}

fn validate_trace_path(
    segments: &[TraceSegment],
    frames: &[TraceFrame],
    duration: f32,
) -> Result<(), String> {
    if segments.is_empty() || segments.len() > 100_000 {
        return Err("Trace paths must contain 1–100,000 cubic segments.".to_owned());
    }
    for segment in segments {
        for point in [
            segment.start,
            segment.control_1,
            segment.control_2,
            segment.end,
        ] {
            finite_point(point)?;
        }
    }
    if frames.is_empty() || frames.len() > 10_000 {
        return Err("Trace paths must contain 1–10,000 frames.".to_owned());
    }
    let mut previous = -1.0;
    for frame in frames {
        if !frame.at.is_finite()
            || frame.at < previous
            || !(0.0..=duration).contains(&frame.at)
            || frame.count == 0
            || frame.start as usize + frame.count as usize > segments.len()
        {
            return Err("Trace path frames are invalid.".to_owned());
        }
        previous = frame.at;
    }
    Ok(())
}

fn trace_frame_commands(segments: &[TraceSegment], frame: TraceFrame) -> Vec<PathCommand> {
    let start = frame.start as usize;
    let end = start + frame.count as usize;
    let selected = &segments[start..end];
    let mut commands = Vec::with_capacity(selected.len() + 2);
    commands.push(PathCommand::MoveTo {
        x: selected[0].start[0],
        y: selected[0].start[1],
    });
    commands.extend(selected.iter().map(|segment| PathCommand::CubicTo {
        c1x: segment.control_1[0],
        c1y: segment.control_1[1],
        c2x: segment.control_2[0],
        c2y: segment.control_2[1],
        x: segment.end[0],
        y: segment.end[1],
    }));
    if frame.closed {
        commands.push(PathCommand::Close);
    }
    commands
}

fn sample_trace_path(
    segments: &[TraceSegment],
    frames: &[TraceFrame],
    time: f32,
) -> Vec<PathCommand> {
    if frames.len() == 1 || time <= frames[0].at {
        return trace_frame_commands(segments, frames[0]);
    }
    for pair in frames.windows(2) {
        if time <= pair[1].at {
            let span = (pair[1].at - pair[0].at).max(f32::EPSILON);
            let amount = ((time - pair[0].at) / span).clamp(0.0, 1.0);
            let from = trace_frame_commands(segments, pair[0]);
            let to = trace_frame_commands(segments, pair[1]);
            return if from.len() == to.len() {
                interpolate_path_commands(&from, &to, amount).unwrap_or(if amount < 0.5 {
                    from
                } else {
                    to
                })
            } else if amount < 0.5 {
                from
            } else {
                to
            };
        }
    }
    trace_frame_commands(
        segments,
        *frames.last().expect("trace frames are validated"),
    )
}

fn finite_point(point: [f32; 2]) -> Result<(), String> {
    if point[0].is_finite() && point[1].is_finite() {
        Ok(())
    } else {
        Err("Points must be finite.".to_owned())
    }
}

fn finite_point_3d(point: [f32; 3]) -> Result<(), String> {
    if point.iter().all(|value| value.is_finite()) {
        Ok(())
    } else {
        Err("3D points must be finite.".to_owned())
    }
}

fn positive(value: f32, label: &str) -> Result<(), String> {
    if value.is_finite() && value > 0.0 && value <= 10_000.0 {
        Ok(())
    } else {
        Err(format!("{label} must be positive and finite."))
    }
}

const fn one() -> f32 {
    1.0
}

const fn one_u32() -> u32 {
    1
}

const fn yes() -> bool {
    true
}

const fn infinity() -> f32 {
    f32::INFINITY
}

const fn default_width() -> f32 {
    16.0
}

const fn default_height() -> f32 {
    9.0
}

const fn default_pixel_width() -> u32 {
    1280
}

const fn default_pixel_height() -> u32 {
    720
}

const fn default_fps() -> u32 {
    60
}

fn default_background() -> String {
    "#0f172a".to_owned()
}

fn default_font_family() -> String {
    "Noto Sans".to_owned()
}

fn validate_font_family(family: &str) -> Result<(), String> {
    if family.trim().is_empty() || family.len() > 120 {
        return Err("Font family names must contain 1–120 characters.".to_owned());
    }
    Ok(())
}

fn default_stroke() -> Option<String> {
    Some("#f8fafc".to_owned())
}

const fn default_stroke_width() -> f32 {
    0.04
}

const fn default_tip_size() -> f32 {
    0.24
}

const fn default_point_radius() -> f32 {
    0.08
}

const fn default_camera_3d_position() -> [f32; 3] {
    [0.0, 0.0, 8.0]
}

const fn default_camera_3d_up() -> [f32; 3] {
    [0.0, 1.0, 0.0]
}

const fn default_camera_3d_fov() -> f32 {
    std::f32::consts::FRAC_PI_4
}

const fn default_camera_3d_near() -> f32 {
    0.1
}

const fn default_camera_3d_far() -> f32 {
    100.0
}

const fn default_camera_3d_ambient() -> f32 {
    0.28
}

const fn default_light_direction() -> [f32; 3] {
    [-0.4, 0.7, 1.0]
}

const fn default_mesh_light_position() -> [f32; 3] {
    [-10.0, 10.0, 10.0]
}

#[cfg(test)]
mod tests {
    use super::*;

    const GENERAL_SCENE: &str = r##"{
      "version": 2,
      "title": "General scene",
      "width": 16,
      "height": 9,
      "duration": 4,
      "fps": 60,
      "background": "#101827",
      "nodes": [
        {"id":"root","type":"group","transform":{"x":1,"y":0}},
        {"id":"circle","parent":"root","type":"circle","radius":1,"style":{"fill":"#3b82f6","stroke":"#ffffff","strokeWidth":0.04}},
        {"id":"label","type":"text","text":"hello","fontSize":0.5,"transform":{"y":-2},"style":{"fill":"#ffffff","stroke":null}}
      ],
      "tracks": [
        {"target":"circle","property":"x","keyframes":[{"at":0,"value":-2},{"at":4,"value":2,"easing":"smooth"}]},
        {"target":"circle","property":"fill","keyframes":[{"at":0,"value":"#3b82f6"},{"at":4,"value":"#f97316"}]}
      ],
      "signals": [{"id":"pulse","keyframes":[{"at":0,"value":0},{"at":2,"value":1},{"at":4,"value":0}]}],
      "bindings": [{"target":"circle","property":"scaleX","expression":{"op":"add","args":[{"op":"constant","value":1},{"op":"signal","id":"pulse"}]}}]
    }"##;

    #[test]
    fn evaluates_general_nodes_tracks_and_bindings() {
        let scene = Scene::from_json(GENERAL_SCENE).expect("scene should parse");
        assert_eq!((scene.pixel_width, scene.pixel_height), (1280, 720));
        let frame = scene.evaluate(2.0).expect("scene should evaluate");
        assert_eq!(frame.nodes.len(), 2);
        let circle = frame
            .nodes
            .iter()
            .find(|node| node.id == "circle")
            .expect("circle should exist");
        assert!((circle.transform[0] - 2.0).abs() < 0.001);
        assert!((circle.transform[4] - 1.0).abs() < 0.001);
    }

    #[test]
    fn text_font_families_default_and_round_trip() {
        let default_scene = Scene::from_json(GENERAL_SCENE).expect("scene should parse");
        let NodeKind::Text { font_family, .. } = &default_scene.nodes[2].kind else {
            panic!("label should remain text");
        };
        assert_eq!(font_family, "Noto Sans");

        let custom = GENERAL_SCENE.replace(
            r#""text":"hello","fontSize":0.5"#,
            r#""text":"hello","fontSize":0.5,"fontFamily":"Uploaded Sans""#,
        );
        let scene = Scene::from_json(&custom).expect("custom family should parse");
        let NodeKind::Text { font_family, .. } = &scene.nodes[2].kind else {
            panic!("label should remain text");
        };
        assert_eq!(font_family, "Uploaded Sans");
        assert!(
            serde_json::to_string(&scene)
                .unwrap()
                .contains("Uploaded Sans")
        );
    }

    #[test]
    fn scene_units_and_pixel_output_dimensions_are_independent() {
        let json = GENERAL_SCENE.replacen(
            "\"height\": 9,",
            "\"height\": 9,\n      \"pixelWidth\": 854,\n      \"pixelHeight\": 480,",
            1,
        );
        let scene = Scene::from_json(&json).expect("pixel dimensions should parse");
        assert_eq!((scene.width, scene.height), (16.0, 9.0));
        assert_eq!((scene.pixel_width, scene.pixel_height), (854, 480));

        let invalid = json.replacen("\"pixelWidth\": 854", "\"pixelWidth\": 0", 1);
        assert!(Scene::from_json(&invalid).is_err());
    }

    #[test]
    fn point_clouds_retain_screen_space_radius_mode() {
        let scene = Scene::from_json(
            r##"{
              "version":2,
              "title":"Screen points",
              "duration":1,
              "nodes":[{
                "id":"points",
                "type":"pointCloud",
                "radius":0.12,
                "screenSpaceRadius":true,
                "points":[{"x":-1,"y":0,"color":"#22c55eff"},{"x":1,"y":0}]
              }]
            }"##,
        )
        .expect("screen-space point cloud should validate");
        let frame = scene
            .evaluate_view(0.5)
            .expect("point cloud should evaluate");
        assert!(matches!(
            frame.nodes[0].kind.as_ref(),
            NodeKind::PointCloud {
                screen_space_radius: true,
                ..
            }
        ));
    }

    #[test]
    fn compact_surface_patches_round_trip_and_remain_borrowed() {
        let scene = Scene::from_json(
            r##"{
              "version": 2,
              "title": "Compact surface",
              "duration": 1,
              "nodes": [{
                "id": "surface",
                "type": "surface",
                "vertices": [[-1,-1,0],[1,-1,0],[1,1,0],[-1,1,0]],
                "patches": [[0,1,2,3]],
                "colors": ["#2563ebcc","#2563ebcc","#1d4ed8cc","#1d4ed8cc"],
                "strokeColors": ["#ffffffff"],
                "strokeRadii": [0.004],
                "unlit": true,
                "doubleSided": true,
                "style": {"fill":"#2563eb","stroke":null,"opacity":0.8}
              }]
            }"##,
        )
        .expect("compact surface should validate");
        let frame = scene.evaluate_view(0.5).expect("surface should evaluate");
        assert_eq!(frame.nodes.len(), 1);
        assert!(matches!(
            frame.nodes[0].kind,
            Cow::Borrowed(NodeKind::Surface { .. })
        ));
        let round_tripped = serde_json::to_string(&scene).expect("surface should serialize");
        assert!(round_tripped.contains("\"type\":\"surface\""));
    }

    #[test]
    fn compact_surfaces_animate_vertices_materials_and_stroke_radii() {
        let scene = Scene::from_json(
            r##"{
              "version": 2,
              "title": "Dynamic compact surface",
              "duration": 1,
              "nodes": [{
                "id": "surface",
                "type": "surface",
                "vertices": [[-1,-1,0],[1,-1,0],[1,1,0],[-1,1,0]],
                "patches": [[0,1,2,3]],
                "colors": ["#2563ebcc","#2563ebcc","#1d4ed8cc","#1d4ed8cc"],
                "strokeColors": ["#ffffffff"],
                "strokeRadii": [0.004],
                "unlit": true,
                "doubleSided": true,
                "style": {"fill":"#2563eb","stroke":null,"opacity":0.8}
              }],
              "tracks": [
                {"target":"surface","property":"vertices","keyframes":[
                  {"at":0,"value":[[-1,-1,0],[1,-1,0],[1,1,0],[-1,1,0]]},
                  {"at":0.75,"value":[[-1,-1,0],[1,-1,0],[1,1,1],[-1,1,0]]}
                ]},
                {"target":"surface","property":"surfaceColors","keyframes":[
                  {"at":0,"value":["#2563ebcc","#2563ebcc","#1d4ed8cc","#1d4ed8cc","#ffffffff"]},
                  {"at":0.75,"value":["#ef4444cc","#ef4444cc","#dc2626cc","#dc2626cc","#facc15ff"]}
                ]},
                {"target":"surface","property":"strokeRadii","keyframes":[
                  {"at":0,"value":[0.004]},
                  {"at":0.75,"value":[0.02]}
                ]}
              ]
            }"##,
        )
        .expect("dynamic compact surface should validate");
        let frame = scene.evaluate_view(0.75).expect("surface should evaluate");
        let NodeKind::Surface {
            vertices,
            colors,
            stroke_colors,
            stroke_radii,
            ..
        } = frame.nodes[0].kind.as_ref()
        else {
            panic!("surface should remain compact");
        };
        assert_eq!(vertices[2], [1.0, 1.0, 1.0]);
        assert_eq!(colors[0], "#ef4444cc");
        assert_eq!(stroke_colors[0], "#facc15ff");
        assert!((stroke_radii[0] - 0.02).abs() < 0.000_001);
        assert!(matches!(&frame.nodes[0].kind, Cow::Owned(_)));

        let invalid = serde_json::to_string(&scene)
            .expect("scene should serialize")
            .replace(
                "[\"#ef4444cc\",\"#ef4444cc\",\"#dc2626cc\",\"#dc2626cc\",\"#facc15ff\"]",
                "[\"#ef4444cc\"]",
            );
        assert!(Scene::from_json(&invalid).is_err());
    }

    #[test]
    fn camera_independent_three_dimensional_paths_round_trip() {
        let scene = Scene::from_json(
            r##"{
              "version": 2,
              "title": "3D path",
              "duration": 1,
              "nodes": [{
                "id": "curve",
                "type": "path3d",
                "commands": [
                  {"op":"moveTo","x":-1,"y":0,"z":0},
                  {"op":"cubicTo","c1x":-0.5,"c1y":1,"c1z":0.5,"c2x":0.5,"c2y":1,"c2z":-0.5,"x":1,"y":0,"z":0}
                ],
                "style": {"fill":null,"stroke":"#38bdf8"}
              }]
            }"##,
        )
        .expect("3D path should validate");
        let frame = scene.evaluate_view(0.5).expect("3D path should evaluate");
        assert!(matches!(
            frame.nodes[0].kind,
            Cow::Borrowed(NodeKind::Path3d { .. })
        ));
        let round_tripped = serde_json::to_string(&scene).expect("3D path should serialize");
        assert!(round_tripped.contains("\"type\":\"path3d\""));
    }

    #[test]
    fn sliding_cubic_trace_paths_materialize_at_explicit_time() {
        let scene = Scene::from_json(
            r##"{
              "version": 2,
              "title": "Trace path",
              "duration": 2,
              "nodes": [{
                "id": "trail",
                "type": "tracePath",
                "segments": [
                  {"start":[0,0],"control1":[0.3,0],"control2":[0.7,0],"end":[1,0]},
                  {"start":[1,0],"control1":[1.3,0.3],"control2":[1.7,0.7],"end":[2,1]}
                ],
                "frames": [
                  {"at":0,"start":0,"count":1},
                  {"at":1,"start":0,"count":2},
                  {"at":2,"start":1,"count":1}
                ],
                "style": {"fill":null,"stroke":"#38bdf8"}
              }]
            }"##,
        )
        .expect("trace path should validate");
        let frame = scene
            .evaluate_view(1.0)
            .expect("trace path should evaluate");
        let NodeKind::Path { commands } = frame.nodes[0].kind.as_ref() else {
            panic!("trace should materialize as a path");
        };
        assert_eq!(commands.len(), 3);
    }

    #[test]
    fn retained_path_references_resolve_inactive_source_geometry() {
        let scene = Scene::from_json(
            r##"{
              "version": 2,
              "title": "Path reference",
              "duration": 2,
              "nodes": [
                {
                  "id": "source",
                  "type": "path",
                  "commands": [
                    {"op":"moveTo","x":0,"y":0},
                    {"op":"cubicTo","c1x":1,"c1y":0,"c2x":1,"c2y":1,"x":2,"y":1}
                  ],
                  "disappearAt": 1
                },
                {
                  "id": "copy",
                  "type": "pathRef",
                  "source": "source",
                  "appearAt": 1,
                  "style": {"fill":null,"stroke":"#38bdf8"}
                }
              ]
            }"##,
        )
        .expect("path reference should validate");
        let frame = scene.evaluate_view(1.5).expect("reference should evaluate");
        assert_eq!(frame.nodes.len(), 1);
        assert_eq!(frame.nodes[0].id, "copy");
        assert!(matches!(
            frame.nodes[0].kind,
            Cow::Borrowed(NodeKind::Path { .. })
        ));
    }

    #[test]
    fn reuses_concrete_keyframes_across_track_targets() {
        let mut scene = Scene::from_json(GENERAL_SCENE).expect("scene should parse");
        scene.tracks.push(Track {
            target: "label".to_owned(),
            property: Property::X,
            keyframes: Vec::new(),
            keyframes_from: Some("circle".to_owned()),
        });
        scene.validate().expect("shared keyframes should validate");
        let frame = scene
            .evaluate(2.0)
            .expect("shared keyframes should evaluate");
        let circle = frame.nodes.iter().find(|node| node.id == "circle").unwrap();
        let label = frame.nodes.iter().find(|node| node.id == "label").unwrap();
        assert_eq!(circle.transform[4] - 1.0, label.transform[4]);
    }

    #[test]
    fn evaluates_independent_cubic_draw_window_bounds() {
        let mut scene = Scene::from_json(GENERAL_SCENE).expect("scene should parse");
        scene.tracks.push(Track {
            target: "circle".to_owned(),
            property: Property::DrawStart,
            keyframes: vec![
                Keyframe {
                    at: 0.0,
                    value: TrackValue::Number(0.0),
                    easing: Easing::Linear,
                },
                Keyframe {
                    at: 4.0,
                    value: TrackValue::Number(0.8),
                    easing: Easing::Linear,
                },
            ],
            keyframes_from: None,
        });
        scene.validate().expect("draw window should validate");
        let frame = scene
            .evaluate_view(2.0)
            .expect("draw window should evaluate");
        let circle = frame
            .nodes
            .iter()
            .find(|node| node.id == "circle")
            .expect("circle should exist");
        assert!((circle.style.draw_start - 0.4).abs() < 0.001);
        assert!((circle.style.draw_progress - 1.0).abs() < 0.001);
    }

    #[test]
    fn evaluates_atomic_cubic_draw_window_track() {
        let mut scene = Scene::from_json(GENERAL_SCENE).expect("scene should parse");
        scene.tracks.push(Track {
            target: "circle".to_owned(),
            property: Property::DrawRange,
            keyframes: vec![
                Keyframe {
                    at: 0.0,
                    value: TrackValue::Numbers(vec![0.0, 0.2]),
                    easing: Easing::Linear,
                },
                Keyframe {
                    at: 4.0,
                    value: TrackValue::Numbers(vec![0.8, 1.0]),
                    easing: Easing::Linear,
                },
            ],
            keyframes_from: None,
        });
        scene
            .validate()
            .expect("atomic draw window should validate");
        let frame = scene
            .evaluate_view(2.0)
            .expect("draw window should evaluate");
        let circle = frame
            .nodes
            .iter()
            .find(|node| node.id == "circle")
            .expect("circle should exist");
        assert!((circle.style.draw_start - 0.4).abs() < 0.001);
        assert!((circle.style.draw_progress - 0.6).abs() < 0.001);
    }

    #[test]
    fn random_seek_matches_repeated_seek() {
        let scene = Scene::from_json(GENERAL_SCENE).expect("scene should parse");
        let first = serde_json::to_string(&scene.evaluate(3.1).expect("first seek"))
            .expect("first frame serializes");
        let _ = scene.evaluate(0.4).expect("intermediate seek");
        let second = serde_json::to_string(&scene.evaluate(3.1).expect("second seek"))
            .expect("second frame serializes");
        assert_eq!(first, second);
    }

    #[test]
    fn render_view_borrows_static_node_geometry() {
        let scene = Scene::from_json(GENERAL_SCENE).expect("scene should parse");
        let frame = scene.evaluate_view(2.0).expect("view should evaluate");
        assert_eq!(frame.cloned_kinds, 0);
        assert!(
            frame
                .nodes
                .iter()
                .all(|node| matches!(node.kind, Cow::Borrowed(_)))
        );
        assert_eq!(
            scene.evaluation_profile(),
            EvaluationProfile {
                node_count: 3,
                static_node_count: 2,
                dynamic_node_count: 1,
                track_count: 2,
                binding_count: 1,
            }
        );
    }

    #[test]
    fn rejects_parent_cycles() {
        let invalid = GENERAL_SCENE
            .replace(r#""parent":"root""#, r#""parent":"circle""#)
            .replace(
                r#"{"id":"root","type":"group""#,
                r#"{"id":"root","parent":"circle","type":"group""#,
            );
        let error = Scene::from_json(&invalid).expect_err("cycle should fail");
        assert!(error.contains("cycle") || error.contains("invalid parent"));
    }

    #[test]
    fn rejects_missing_signal() {
        let invalid = GENERAL_SCENE.replace(r#""id":"pulse"}"#, r#""id":"missing"}"#);
        let error = Scene::from_json(&invalid).expect_err("missing signal should fail");
        assert!(error.contains("missing signal"));
    }

    #[test]
    fn signal_controls_override_explicit_time_without_mutating_the_scene() {
        let mut scene = Scene::from_json(GENERAL_SCENE).expect("scene should parse");
        scene.controls.push(Control {
            id: "pulse-control".to_owned(),
            label: "Pulse".to_owned(),
            signal: "pulse".to_owned(),
            min: 0.0,
            max: 3.0,
            step: 0.1,
            default: 1.0,
            timeline: false,
        });
        scene.validate().expect("control should validate");
        let mut overrides = HashMap::new();
        overrides.insert("pulse".to_owned(), 2.5);
        let frame = scene
            .evaluate_with_signal_overrides(0.0, &overrides)
            .expect("controlled frame should evaluate");
        let circle = frame
            .nodes
            .iter()
            .find(|node| node.id == "circle")
            .expect("circle should exist");
        assert!((circle.transform[0] - 3.5).abs() < 0.001);
        assert!((scene.evaluate(0.0).unwrap().nodes[0].transform[0] - 1.0).abs() < 0.001);
    }

    #[test]
    fn monotonic_signal_values_invert_to_exact_scene_time() {
        let mut scene = Scene::from_json(GENERAL_SCENE).expect("scene should parse");
        scene.signals.push(Signal {
            id: "tracker".to_owned(),
            keyframes: vec![
                NumberKeyframe {
                    at: 0.0,
                    value: -2.0,
                    easing: Easing::Linear,
                },
                NumberKeyframe {
                    at: 4.0,
                    value: 6.0,
                    easing: Easing::Linear,
                },
            ],
        });
        scene.controls.push(Control {
            id: "tracker-control".to_owned(),
            label: "Value tracker".to_owned(),
            signal: "tracker".to_owned(),
            min: -2.0,
            max: 6.0,
            step: 0.1,
            default: -2.0,
            timeline: true,
        });
        scene.validate().expect("timeline control should validate");
        let time = scene
            .time_for_signal_value("tracker", 2.0)
            .expect("signal should invert");
        assert!((time - 2.0).abs() < 0.0001);
    }

    #[test]
    fn interpolates_compatible_cubic_path_commands() {
        let from = vec![
            PathCommand::MoveTo { x: 0.0, y: 0.0 },
            PathCommand::CubicTo {
                c1x: 1.0,
                c1y: 0.0,
                c2x: 1.0,
                c2y: 1.0,
                x: 2.0,
                y: 1.0,
            },
        ];
        let to = vec![
            PathCommand::MoveTo { x: 2.0, y: 2.0 },
            PathCommand::CubicTo {
                c1x: 3.0,
                c1y: 2.0,
                c2x: 3.0,
                c2y: 3.0,
                x: 4.0,
                y: 3.0,
            },
        ];
        let interpolated = interpolate_path_commands(&from, &to, 0.5).unwrap();
        assert_eq!(
            interpolated,
            vec![
                PathCommand::MoveTo { x: 1.0, y: 1.0 },
                PathCommand::CubicTo {
                    c1x: 2.0,
                    c1y: 1.0,
                    c2x: 2.0,
                    c2y: 2.0,
                    x: 3.0,
                    y: 2.0,
                },
            ]
        );
    }

    #[test]
    fn compact_path_data_interpolates_against_retained_topology() {
        let scene = Scene::from_json(
            r##"{
              "version": 2,
              "title": "Compact path morph",
              "duration": 2,
              "nodes": [{
                "id": "morph",
                "type": "path",
                "commands": [
                  {"op":"moveTo","x":0,"y":0},
                  {"op":"cubicTo","c1x":1,"c1y":0,"c2x":1,"c2y":1,"x":2,"y":1},
                  {"op":"close"}
                ],
                "style": {"fill":null,"stroke":"#38bdf8"}
              }],
              "tracks": [{
                "target": "morph",
                "property": "pathData",
                "keyframes": [
                  {"at":0,"value":[0,0,1,0,1,1,2,1]},
                  {"at":2,"value":[0,2,1,2,1,3,2,3]}
                ]
              }]
            }"##,
        )
        .expect("compact path data should validate");
        let frame = scene.evaluate_view(1.0).expect("path should evaluate");
        let NodeKind::Path { commands } = frame.nodes[0].kind.as_ref() else {
            panic!("path data must materialize a path");
        };
        assert_eq!(
            commands,
            &[
                PathCommand::MoveTo { x: 0.0, y: 1.0 },
                PathCommand::CubicTo {
                    c1x: 1.0,
                    c1y: 1.0,
                    c2x: 1.0,
                    c2y: 2.0,
                    x: 2.0,
                    y: 2.0,
                },
                PathCommand::Close,
            ]
        );
    }

    #[test]
    fn composite_two_dimensional_transform_interpolates_in_rust() {
        let mut scene = Scene::from_json(GENERAL_SCENE).expect("scene should parse");
        scene.tracks.push(Track {
            target: "label".to_owned(),
            property: Property::Transform2d,
            keyframes: vec![
                Keyframe {
                    at: 0.0,
                    value: TrackValue::Numbers(vec![0.0, -2.0, 0.0, 1.0, 1.0]),
                    easing: Easing::Linear,
                },
                Keyframe {
                    at: 4.0,
                    value: TrackValue::Numbers(vec![2.0, 0.0, PI, 2.0, 2.0]),
                    easing: Easing::Linear,
                },
            ],
            keyframes_from: None,
        });
        scene.validate().expect("transform should validate");
        let frame = scene.evaluate_view(2.0).expect("scene should evaluate");
        let label = frame
            .nodes
            .iter()
            .find(|node| node.id == "label")
            .expect("label should exist");
        assert!((label.transform[0]).abs() < 0.001);
        assert!((label.transform[1] - 1.5).abs() < 0.001);
        assert!((label.transform[4] - 1.0).abs() < 0.001);
        assert!((label.transform[5] + 1.0).abs() < 0.001);
    }

    #[test]
    fn full_affine_transform_interpolates_in_rust() {
        let mut scene = Scene::from_json(GENERAL_SCENE).expect("scene should parse");
        scene.tracks.push(Track {
            target: "label".to_owned(),
            property: Property::Affine2d,
            keyframes: vec![
                Keyframe {
                    at: 0.0,
                    value: TrackValue::Numbers(vec![1.0, 0.0, 0.0, 1.0, 0.0, -2.0]),
                    easing: Easing::Linear,
                },
                Keyframe {
                    at: 2.0,
                    value: TrackValue::Numbers(vec![1.0, 0.6, 0.4, 1.2, 2.0, 0.0]),
                    easing: Easing::Linear,
                },
            ],
            keyframes_from: None,
        });
        scene.validate().expect("affine transform should validate");
        let frame = scene.evaluate_view(1.0).expect("scene should evaluate");
        let label = frame
            .nodes
            .iter()
            .find(|node| node.id == "label")
            .expect("label should exist");
        assert_eq!(label.transform, [1.0, 0.3, 0.2, 1.1, 1.0, -1.0]);
    }

    #[test]
    fn interpolates_three_dimensional_camera_tracks() {
        let mut scene = Scene::from_json(GENERAL_SCENE).expect("scene should parse");
        scene.tracks.extend([
            Track {
                target: "__camera__".to_owned(),
                property: Property::Camera3dPosition,
                keyframes: vec![
                    Keyframe {
                        at: 0.0,
                        value: TrackValue::Point3d([0.0, -8.0, 4.0]),
                        easing: Easing::Linear,
                    },
                    Keyframe {
                        at: 4.0,
                        value: TrackValue::Point3d([4.0, -4.0, 8.0]),
                        easing: Easing::Linear,
                    },
                ],
                keyframes_from: None,
            },
            Track {
                target: "__camera__".to_owned(),
                property: Property::Camera3dUp,
                keyframes: vec![
                    Keyframe {
                        at: 0.0,
                        value: TrackValue::Point3d([0.0, 0.0, 1.0]),
                        easing: Easing::Linear,
                    },
                    Keyframe {
                        at: 4.0,
                        value: TrackValue::Point3d([0.0, 1.0, 0.0]),
                        easing: Easing::Linear,
                    },
                ],
                keyframes_from: None,
            },
            Track {
                target: "__camera__".to_owned(),
                property: Property::Camera3dFovY,
                keyframes: vec![
                    Keyframe {
                        at: 0.0,
                        value: TrackValue::Number(0.8),
                        easing: Easing::Linear,
                    },
                    Keyframe {
                        at: 4.0,
                        value: TrackValue::Number(1.2),
                        easing: Easing::Linear,
                    },
                ],
                keyframes_from: None,
            },
        ]);
        scene.validate().expect("camera tracks should validate");
        let frame = scene.evaluate(2.0).expect("camera tracks should evaluate");
        assert_eq!(frame.camera_3d.position, [2.0, -6.0, 6.0]);
        assert_eq!(frame.camera_3d.up, [0.0, 0.5, 0.5]);
        assert!((frame.camera_3d.fov_y - 1.0).abs() < 0.001);
        for node in &mut scene.nodes {
            node.disappear_at = scene.duration;
        }
        let round_tripped = Scene::from_json(
            &serde_json::to_string(&scene).expect("camera scene should serialize"),
        )
        .expect("camera vectors should deserialize from JSON arrays");
        assert_eq!(
            round_tripped
                .evaluate(2.0)
                .expect("round-tripped camera should evaluate")
                .camera_3d
                .position,
            [2.0, -6.0, 6.0]
        );
    }

    #[test]
    fn interpolates_atomic_three_dimensional_camera_state() {
        let mut scene = Scene::from_json(GENERAL_SCENE).expect("scene should parse");
        scene.tracks.push(Track {
            target: "__camera__".to_owned(),
            property: Property::Camera3d,
            keyframes: vec![
                Keyframe {
                    at: 0.0,
                    value: TrackValue::Numbers(vec![
                        15.0, 0.0, -8.0, 4.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.8,
                    ]),
                    easing: Easing::Linear,
                },
                Keyframe {
                    at: 4.0,
                    value: TrackValue::Numbers(vec![
                        15.0, 4.0, -4.0, 8.0, 2.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.2,
                    ]),
                    easing: Easing::Linear,
                },
            ],
            keyframes_from: None,
        });
        scene.validate().expect("camera state should validate");
        let frame = scene.evaluate(2.0).expect("camera state should evaluate");
        assert_eq!(frame.camera_3d.position, [2.0, -6.0, 6.0]);
        assert_eq!(frame.camera_3d.target, [1.0, 0.0, 0.5]);
        assert_eq!(frame.camera_3d.up, [0.0, 0.5, 0.5]);
        assert!((frame.camera_3d.fov_y - 1.0).abs() < 0.001);
    }

    #[test]
    fn derives_roll_free_camera_up_during_native_orbit() {
        let mut scene = Scene::from_json(GENERAL_SCENE).expect("scene should parse");
        scene.tracks.push(Track {
            target: "__camera__".to_owned(),
            property: Property::Camera3dOrbit,
            keyframes: vec![
                Keyframe {
                    at: 0.0,
                    value: TrackValue::Point3d([4.0, -8.0, 4.0]),
                    easing: Easing::Linear,
                },
                Keyframe {
                    at: 4.0,
                    value: TrackValue::Point3d([0.0, -4.0, 8.0]),
                    easing: Easing::Linear,
                },
            ],
            keyframes_from: None,
        });
        scene.validate().expect("orbit track should validate");
        let frame = scene.evaluate(2.0).expect("orbit should evaluate");
        assert_eq!(frame.camera_3d.position, [2.0, -6.0, 6.0]);
        let direction = frame.camera_3d.position;
        let dot = direction[0] * frame.camera_3d.up[0]
            + direction[1] * frame.camera_3d.up[1]
            + direction[2] * frame.camera_3d.up[2];
        let up_length = frame
            .camera_3d
            .up
            .iter()
            .map(|value| value * value)
            .sum::<f32>()
            .sqrt();
        assert!(dot.abs() < 0.001);
        assert!((up_length - 1.0).abs() < 0.001);
    }

    #[test]
    fn billboard_group_projects_one_world_anchor_at_seek_time() {
        let mut scene = Scene::from_json(GENERAL_SCENE).expect("scene should parse");
        let anchor = [2.0, 0.0, 0.0];
        let base = project_camera_3d_point(anchor, scene.camera_3d, scene.width, scene.height)
            .expect("anchor should project");
        scene.nodes.push(Node {
            id: "billboard".to_owned(),
            parent: None,
            z_index: 0,
            transform: Transform::default(),
            style: Style::default(),
            appear_at: 0.0,
            disappear_at: scene.duration + 1.0,
            kind: NodeKind::Billboard { anchor, base },
        });
        scene
            .nodes
            .iter_mut()
            .find(|node| node.id == "label")
            .expect("label exists")
            .parent = Some("billboard".to_owned());
        scene.tracks.push(Track {
            target: "__camera__".to_owned(),
            property: Property::Camera3dOrbit,
            keyframes: vec![
                Keyframe {
                    at: 0.0,
                    value: TrackValue::Point3d([0.0, 0.0, 8.0]),
                    easing: Easing::Linear,
                },
                Keyframe {
                    at: 4.0,
                    value: TrackValue::Point3d([4.0, 0.0, 8.0]),
                    easing: Easing::Linear,
                },
            ],
            keyframes_from: None,
        });
        scene.validate().expect("billboard should validate");
        let start = scene.evaluate_view(0.0).expect("start should evaluate");
        let start_transform = start
            .nodes
            .iter()
            .find(|node| node.id == "label")
            .expect("label should render")
            .transform;
        let end = scene.evaluate_view(4.0).expect("end should evaluate");
        let end_transform = end
            .nodes
            .iter()
            .find(|node| node.id == "label")
            .expect("label should render")
            .transform;
        assert!((start_transform[4] - end_transform[4]).abs() > 0.1);
    }

    #[test]
    fn billboard_anchor_tracks_project_animated_world_positions() {
        let mut scene = Scene::from_json(GENERAL_SCENE).expect("scene should parse");
        let anchor = [0.0, 0.0, 0.0];
        let base = project_camera_3d_point(anchor, scene.camera_3d, scene.width, scene.height)
            .expect("anchor should project");
        scene.nodes.push(Node {
            id: "animated-billboard".to_owned(),
            parent: None,
            z_index: 0,
            transform: Transform::default(),
            style: Style::default(),
            appear_at: 0.0,
            disappear_at: scene.duration + 1.0,
            kind: NodeKind::Billboard { anchor, base },
        });
        scene
            .nodes
            .iter_mut()
            .find(|node| node.id == "label")
            .expect("label exists")
            .parent = Some("animated-billboard".to_owned());
        scene.tracks.push(Track {
            target: "animated-billboard".to_owned(),
            property: Property::BillboardAnchor,
            keyframes: vec![
                Keyframe {
                    at: 0.0,
                    value: TrackValue::Point3d(anchor),
                    easing: Easing::Linear,
                },
                Keyframe {
                    at: 4.0,
                    value: TrackValue::Point3d([2.0, 1.0, 0.0]),
                    easing: Easing::Linear,
                },
            ],
            keyframes_from: None,
        });
        scene
            .validate()
            .expect("animated billboard should validate");
        let start = scene.evaluate_view(0.0).expect("start should evaluate");
        let start_transform = start
            .nodes
            .iter()
            .find(|node| node.id == "label")
            .expect("label should render")
            .transform;
        let end = scene.evaluate_view(4.0).expect("end should evaluate");
        let end_transform = end
            .nodes
            .iter()
            .find(|node| node.id == "label")
            .expect("label should render")
            .transform;
        assert!((start_transform[4] - end_transform[4]).abs() > 0.1);
        assert!((start_transform[5] - end_transform[5]).abs() > 0.1);
    }

    #[test]
    fn validates_and_interpolates_custom_shader_buffers() {
        let scene = Scene::from_json(
            r##"{
              "version": 2,
              "title": "Custom shader",
              "duration": 1,
              "nodes": [{
                "id": "plugin",
                "type": "customShaderMesh",
                "vertexWgsl": "@vertex fn main(@location(0) p: vec2<f32>) -> @builtin(position) vec4<f32> { return vec4<f32>(p, 0.0, 1.0); }",
                "fragmentWgsl": "@fragment fn main() -> @location(0) vec4<f32> { return vec4<f32>(1.0); }",
                "attributes": [{
                  "name": "p",
                  "location": 0,
                  "offset": 0,
                  "format": "float32x2"
                }],
                "vertexStride": 8,
                "vertexData": [-1,-1, 1,-1, 0,1],
                "indices": [0,1,2],
                "uniforms": [{
                  "name": "phase",
                  "binding": 0,
                  "type": "float",
                  "values": [0]
                }, {
                  "name": "weights",
                  "binding": 1,
                  "type": "vec2",
                  "arrayLength": 2,
                  "values": [0, 1, 2, 3]
                }]
              }],
              "tracks": [
                {
                  "target": "plugin",
                  "property": "shaderVertexData",
                  "keyframes": [
                    {"at": 0, "value": [-1,-1, 1,-1, 0,1]},
                    {"at": 1, "value": [0,-1, 2,-1, 1,1]}
                  ]
                },
                {
                  "target": "plugin",
                  "property": "shaderUniformValues",
                  "keyframes": [
                    {"at": 0, "value": [0, 0, 1, 2, 3]},
                    {"at": 1, "value": [1, 1, 2, 3, 4]}
                  ]
                }
              ]
            }"##,
        )
        .expect("custom shader scene should validate");
        let frame = scene.evaluate(0.5).expect("shader tracks should evaluate");
        let node = frame
            .nodes
            .iter()
            .find(|node| node.id == "plugin")
            .expect("shader node should be visible");
        let NodeKind::CustomShaderMesh {
            vertex_data,
            uniforms,
            ..
        } = &node.kind
        else {
            panic!("expected custom shader mesh");
        };
        assert_eq!(vertex_data.as_slice(), &[-0.5, -1.0, 1.5, -1.0, 0.5, 1.0]);
        assert_eq!(uniforms[0].values, vec![0.5]);
        assert_eq!(uniforms[1].values, vec![0.5, 1.5, 2.5, 3.5]);
    }
}
