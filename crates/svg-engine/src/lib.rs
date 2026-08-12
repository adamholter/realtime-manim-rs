//! Deterministic SVG ingestion for native and WebAssembly renderers.
//!
//! `usvg` resolves SVG references, transforms, and basic shapes. This crate
//! converts the normalized result into cached lyon paths suitable for the
//! shared GPU vector renderer.

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use lyon::{math::point, path::Path};
use tiny_skia_path::{PathSegment, Point, Transform};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SvgFillRule {
    NonZero,
    EvenOdd,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SvgLineCap {
    Butt,
    Round,
    Square,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SvgLineJoin {
    Miter,
    Round,
    Bevel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SvgGradientSpread {
    Pad,
    Repeat,
    Reflect,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SvgGradientStop {
    pub offset: f32,
    pub color: [f32; 4],
}

#[derive(Clone, Debug, PartialEq)]
pub struct SvgLinearGradient {
    pub from: [f32; 2],
    pub to: [f32; 2],
    pub stops: Vec<SvgGradientStop>,
    pub spread: SvgGradientSpread,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SvgRadialGradient {
    /// Gradient center and focal point in the radial gradient's own user space.
    pub center: [f32; 2],
    pub focal: [f32; 2],
    pub radius: f32,
    pub focal_radius: f32,
    /// Maps normalized path coordinates back into gradient user space. Keeping
    /// this inverse preserves arbitrary SVG gradient transforms, including
    /// rotated and skewed elliptical radial gradients.
    pub inverse_transform: [f32; 6],
    pub stops: Vec<SvgGradientStop>,
    pub spread: SvgGradientSpread,
}

#[derive(Clone, Debug)]
pub enum SvgPaint {
    Solid([f32; 4]),
    LinearGradient(SvgLinearGradient),
    RadialGradient(SvgRadialGradient),
    Pattern(Arc<SvgPattern>),
}

impl PartialEq for SvgPaint {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Solid(left), Self::Solid(right)) => left == right,
            (Self::LinearGradient(left), Self::LinearGradient(right)) => left == right,
            (Self::RadialGradient(left), Self::RadialGradient(right)) => left == right,
            (Self::Pattern(left), Self::Pattern(right)) => Arc::ptr_eq(left, right),
            _ => false,
        }
    }
}

/// A normalized vector pattern tile. The tile rectangle and transform remain in
/// the owning path's user space; child paths remain vector geometry.
#[derive(Clone, Debug)]
pub struct SvgPattern {
    pub rect: [f32; 4],
    pub transform: [f32; 6],
    pub paths: Vec<SvgPath>,
    pub images: Vec<SvgRasterImage>,
    pub order: Vec<SvgElementRef>,
}

#[derive(Clone, Debug)]
pub struct SvgClipShape {
    pub path: Path,
    pub fill_rule: SvgFillRule,
    /// Clip paths attached to a subgroup inside a clip definition. These are
    /// intersected with this shape before the parent clip unions its shapes.
    pub clips: Vec<SvgClip>,
}

/// One union of clip shapes. Multiple entries on a path are intersected,
/// matching linked and nested SVG clip paths.
#[derive(Clone, Debug)]
pub struct SvgClip {
    pub shapes: Vec<SvgClipShape>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SvgMaskType {
    Alpha,
    Luminance,
}

/// A vector SVG mask rendered into a native GPU alpha layer. The mask region
/// remains a vector clip so transformed masks do not silently become axis-aligned.
#[derive(Clone, Debug)]
pub struct SvgMask {
    pub key: u32,
    pub kind: SvgMaskType,
    pub region: SvgClipShape,
    pub paths: Vec<SvgPath>,
    pub images: Vec<SvgRasterImage>,
    pub order: Vec<SvgElementRef>,
}

#[derive(Clone, Debug)]
pub struct SvgPath {
    pub path: Path,
    pub fill: Option<SvgPaint>,
    pub fill_rule: SvgFillRule,
    pub stroke: Option<SvgPaint>,
    pub stroke_width: f32,
    pub line_cap: SvgLineCap,
    pub line_join: SvgLineJoin,
    pub clips: Vec<SvgClip>,
    pub masks: Vec<Arc<SvgMask>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SvgImageResampling {
    Nearest,
    Linear,
}

/// Decoded embedded raster image with its SVG placement and inherited effects.
#[derive(Clone, Debug)]
pub struct SvgRasterImage {
    pub key: u64,
    pub pixels: Arc<Vec<u8>>,
    pub pixel_width: u32,
    pub pixel_height: u32,
    pub transform: [f32; 6],
    pub opacity: f32,
    pub resampling: SvgImageResampling,
    pub clips: Vec<SvgClip>,
    pub masks: Vec<Arc<SvgMask>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SvgElementRef {
    Path(usize),
    Image(usize),
}

/// A normalized vector document in SVG canvas coordinates.
#[derive(Clone, Debug)]
pub struct SvgDocument {
    pub width: f32,
    pub height: f32,
    pub paths: Vec<SvgPath>,
    pub images: Vec<SvgRasterImage>,
    pub order: Vec<SvgElementRef>,
}

/// SVG parsing or conversion error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SvgError(String);

impl fmt::Display for SvgError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for SvgError {}

/// Cached SVG parser and path normalizer.
#[derive(Default)]
pub struct SvgEngine {
    documents: HashMap<String, SvgDocument>,
}

impl SvgEngine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Parses an SVG once and returns its normalized vector paths.
    pub fn document(&mut self, source: &str) -> Result<&SvgDocument, SvgError> {
        if !self.documents.contains_key(source) {
            let document = parse_document(source)?;
            self.documents.insert(source.to_owned(), document);
        }
        self.documents
            .get(source)
            .ok_or_else(|| SvgError("SVG cache insertion failed.".to_owned()))
    }

    pub fn cache_size(&self) -> usize {
        self.documents.len()
    }
}

fn parse_document(source: &str) -> Result<SvgDocument, SvgError> {
    let tree = usvg::Tree::from_str(source, &usvg::Options::default())
        .map_err(|error| SvgError(format!("SVG parsing failed: {error}")))?;
    let size = tree.size();
    let mut paths = Vec::new();
    let mut images = Vec::new();
    let mut order = Vec::new();
    let mut next_mask_key = 0;
    collect_group(
        tree.root(),
        1.0,
        &[],
        &[],
        &mut next_mask_key,
        &mut paths,
        &mut images,
        &mut order,
    )?;
    if order.is_empty() {
        return Err(SvgError(
            "SVG contains no renderable vector paths.".to_owned(),
        ));
    }
    Ok(SvgDocument {
        width: size.width(),
        height: size.height(),
        paths,
        images,
        order,
    })
}

#[allow(clippy::too_many_arguments)]
fn collect_group(
    group: &usvg::Group,
    parent_opacity: f32,
    parent_clips: &[SvgClip],
    parent_masks: &[Arc<SvgMask>],
    next_mask_key: &mut u32,
    output: &mut Vec<SvgPath>,
    images: &mut Vec<SvgRasterImage>,
    order: &mut Vec<SvgElementRef>,
) -> Result<(), SvgError> {
    let opacity = parent_opacity * group.opacity().get();
    let mut clips = parent_clips.to_vec();
    if let Some(clip) = group.clip_path() {
        collect_clip_chain(clip, group.abs_transform(), &mut clips)?;
    }
    let mut masks = parent_masks.to_vec();
    if let Some(mask) = group.mask() {
        collect_mask_chain(mask, group.abs_transform(), next_mask_key, &mut masks)?;
    }
    for node in group.children() {
        match node {
            usvg::Node::Group(child) => collect_group(
                child,
                opacity,
                &clips,
                &masks,
                next_mask_key,
                output,
                images,
                order,
            )?,
            usvg::Node::Path(path) if path.is_visible() => {
                order.push(SvgElementRef::Path(output.len()));
                output.push(convert_styled_path(path, opacity, &clips, &masks)?);
            }
            usvg::Node::Text(text) => collect_group(
                text.flattened(),
                opacity,
                &clips,
                &masks,
                next_mask_key,
                output,
                images,
                order,
            )?,
            usvg::Node::Image(image) if image.is_visible() => match image.kind() {
                usvg::ImageKind::SVG(tree) => collect_mask_group(
                    tree.root(),
                    image.abs_transform(),
                    opacity,
                    &clips,
                    &masks,
                    next_mask_key,
                    output,
                    images,
                    order,
                )?,
                _ => push_raster_image(
                    image,
                    image.abs_transform(),
                    opacity,
                    &clips,
                    &masks,
                    images,
                    order,
                )?,
            },
            usvg::Node::Path(_) | usvg::Node::Image(_) => {}
        }
    }
    Ok(())
}

fn collect_mask_chain(
    mask: &usvg::Mask,
    base_transform: Transform,
    next_mask_key: &mut u32,
    output: &mut Vec<Arc<SvgMask>>,
) -> Result<(), SvgError> {
    let key = *next_mask_key;
    *next_mask_key = next_mask_key
        .checked_add(1)
        .ok_or_else(|| SvgError("SVG contains too many masks.".to_owned()))?;
    let mut paths = Vec::new();
    let mut images = Vec::new();
    let mut order = Vec::new();
    collect_mask_group(
        mask.root(),
        base_transform,
        1.0,
        &[],
        &[],
        next_mask_key,
        &mut paths,
        &mut images,
        &mut order,
    )?;
    let rect = mask.rect();
    let mut builder = Path::builder();
    let corners = [
        Point::from_xy(rect.x(), rect.y()),
        Point::from_xy(rect.right(), rect.y()),
        Point::from_xy(rect.right(), rect.bottom()),
        Point::from_xy(rect.x(), rect.bottom()),
    ];
    let first = transformed(corners[0], base_transform);
    builder.begin(point(first.x, first.y));
    for corner in corners.iter().skip(1) {
        let corner = transformed(*corner, base_transform);
        builder.line_to(point(corner.x, corner.y));
    }
    builder.close();
    output.push(Arc::new(SvgMask {
        key,
        kind: match mask.kind() {
            usvg::MaskType::Alpha => SvgMaskType::Alpha,
            usvg::MaskType::Luminance => SvgMaskType::Luminance,
        },
        region: SvgClipShape {
            path: builder.build(),
            fill_rule: SvgFillRule::NonZero,
            clips: Vec::new(),
        },
        paths,
        images,
        order,
    }));
    if let Some(linked) = mask.mask() {
        collect_mask_chain(linked, base_transform, next_mask_key, output)?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn collect_mask_group(
    group: &usvg::Group,
    base_transform: Transform,
    parent_opacity: f32,
    parent_clips: &[SvgClip],
    parent_masks: &[Arc<SvgMask>],
    next_mask_key: &mut u32,
    output: &mut Vec<SvgPath>,
    images: &mut Vec<SvgRasterImage>,
    order: &mut Vec<SvgElementRef>,
) -> Result<(), SvgError> {
    let opacity = parent_opacity * group.opacity().get();
    let mut clips = parent_clips.to_vec();
    if let Some(clip) = group.clip_path() {
        collect_clip_chain(
            clip,
            base_transform.pre_concat(group.abs_transform()),
            &mut clips,
        )?;
    }
    let mut masks = parent_masks.to_vec();
    if let Some(mask) = group.mask() {
        collect_mask_chain(
            mask,
            base_transform.pre_concat(group.abs_transform()),
            next_mask_key,
            &mut masks,
        )?;
    }
    for node in group.children() {
        match node {
            usvg::Node::Group(child) => collect_mask_group(
                child,
                base_transform,
                opacity,
                &clips,
                &masks,
                next_mask_key,
                output,
                images,
                order,
            )?,
            usvg::Node::Path(path) if path.is_visible() => {
                let transform = base_transform.pre_concat(path.abs_transform());
                order.push(SvgElementRef::Path(output.len()));
                output.push(convert_styled_path_at(
                    path, opacity, &clips, &masks, transform,
                )?);
            }
            usvg::Node::Text(text) => collect_mask_group(
                text.flattened(),
                base_transform,
                opacity,
                &clips,
                &masks,
                next_mask_key,
                output,
                images,
                order,
            )?,
            usvg::Node::Image(image) if image.is_visible() => {
                let transform = base_transform.pre_concat(image.abs_transform());
                match image.kind() {
                    usvg::ImageKind::SVG(tree) => collect_mask_group(
                        tree.root(),
                        transform,
                        opacity,
                        &clips,
                        &masks,
                        next_mask_key,
                        output,
                        images,
                        order,
                    )?,
                    _ => {
                        push_raster_image(image, transform, opacity, &clips, &masks, images, order)?
                    }
                }
            }
            usvg::Node::Path(_) | usvg::Node::Image(_) => {}
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn push_raster_image(
    source: &usvg::Image,
    transform: Transform,
    opacity: f32,
    clips: &[SvgClip],
    masks: &[Arc<SvgMask>],
    images: &mut Vec<SvgRasterImage>,
    order: &mut Vec<SvgElementRef>,
) -> Result<(), SvgError> {
    let (data, format) = match source.kind() {
        usvg::ImageKind::JPEG(data) => (data.as_slice(), image::ImageFormat::Jpeg),
        usvg::ImageKind::PNG(data) => (data.as_slice(), image::ImageFormat::Png),
        usvg::ImageKind::GIF(data) => (data.as_slice(), image::ImageFormat::Gif),
        usvg::ImageKind::WEBP(data) => (data.as_slice(), image::ImageFormat::WebP),
        usvg::ImageKind::SVG(_) => return Ok(()),
    };
    let decoded = image::load_from_memory_with_format(data, format)
        .map_err(|error| SvgError(format!("Embedded SVG image decoding failed: {error}")))?
        .to_rgba8();
    let (pixel_width, pixel_height) = decoded.dimensions();
    let pixels = decoded.into_raw();
    let key = pixels.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    }) ^ (u64::from(pixel_width) << 32)
        ^ u64::from(pixel_height);
    order.push(SvgElementRef::Image(images.len()));
    images.push(SvgRasterImage {
        key,
        pixels: Arc::new(pixels),
        pixel_width,
        pixel_height,
        transform: [
            transform.sx,
            transform.ky,
            transform.kx,
            transform.sy,
            transform.tx,
            transform.ty,
        ],
        opacity,
        resampling: match source.rendering_mode() {
            usvg::ImageRendering::OptimizeSpeed
            | usvg::ImageRendering::CrispEdges
            | usvg::ImageRendering::Pixelated => SvgImageResampling::Nearest,
            usvg::ImageRendering::OptimizeQuality
            | usvg::ImageRendering::Smooth
            | usvg::ImageRendering::HighQuality => SvgImageResampling::Linear,
        },
        clips: clips.to_vec(),
        masks: masks.to_vec(),
    });
    Ok(())
}

fn collect_clip_chain(
    clip: &usvg::ClipPath,
    base_transform: Transform,
    output: &mut Vec<SvgClip>,
) -> Result<(), SvgError> {
    let mut shapes = Vec::new();
    collect_clip_shapes(
        clip.root(),
        base_transform.pre_concat(clip.transform()),
        &[],
        &mut shapes,
    )?;
    if !shapes.is_empty() {
        output.push(SvgClip { shapes });
    }
    if let Some(linked) = clip.clip_path() {
        collect_clip_chain(linked, base_transform, output)?;
    }
    Ok(())
}

fn collect_clip_shapes(
    group: &usvg::Group,
    transform: Transform,
    parent_clips: &[SvgClip],
    output: &mut Vec<SvgClipShape>,
) -> Result<(), SvgError> {
    let mut clips = parent_clips.to_vec();
    if let Some(clip) = group.clip_path() {
        collect_clip_chain(clip, transform, &mut clips)?;
    }
    for node in group.children() {
        match node {
            usvg::Node::Path(path) if path.is_visible() => {
                output.push(SvgClipShape {
                    path: convert_path(path.data(), transform)?,
                    fill_rule: match path.fill().map(usvg::Fill::rule) {
                        Some(usvg::FillRule::EvenOdd) => SvgFillRule::EvenOdd,
                        Some(usvg::FillRule::NonZero) | None => SvgFillRule::NonZero,
                    },
                    clips: clips.clone(),
                });
            }
            usvg::Node::Group(child) => {
                collect_clip_shapes(
                    child,
                    transform.pre_concat(child.transform()),
                    &clips,
                    output,
                )?;
            }
            usvg::Node::Text(text) => {
                collect_clip_shapes(text.flattened(), transform, &clips, output)?;
            }
            usvg::Node::Path(_) | usvg::Node::Image(_) => {}
        }
    }
    Ok(())
}

fn convert_styled_path(
    path: &usvg::Path,
    group_opacity: f32,
    clips: &[SvgClip],
    masks: &[Arc<SvgMask>],
) -> Result<SvgPath, SvgError> {
    convert_styled_path_at(path, group_opacity, clips, masks, path.abs_transform())
}

fn convert_styled_path_at(
    path: &usvg::Path,
    group_opacity: f32,
    clips: &[SvgClip],
    masks: &[Arc<SvgMask>],
    transform: Transform,
) -> Result<SvgPath, SvgError> {
    let (scale_x, scale_y) = transform.get_scale();
    let stroke_scale = (scale_x.abs() * scale_y.abs()).sqrt();
    let fill = path.fill().and_then(|fill| {
        paint(
            fill.paint(),
            fill.opacity().get() * group_opacity,
            transform,
        )
    });
    let stroke = path.stroke().and_then(|stroke| {
        paint(
            stroke.paint(),
            stroke.opacity().get() * group_opacity,
            transform,
        )
    });
    let fill_rule = match path.fill().map(usvg::Fill::rule) {
        Some(usvg::FillRule::EvenOdd) => SvgFillRule::EvenOdd,
        Some(usvg::FillRule::NonZero) | None => SvgFillRule::NonZero,
    };
    let (stroke_width, line_cap, line_join) = path
        .stroke()
        .map(|stroke| {
            (
                stroke.width().get() * stroke_scale,
                match stroke.linecap() {
                    usvg::LineCap::Butt => SvgLineCap::Butt,
                    usvg::LineCap::Round => SvgLineCap::Round,
                    usvg::LineCap::Square => SvgLineCap::Square,
                },
                match stroke.linejoin() {
                    usvg::LineJoin::Round => SvgLineJoin::Round,
                    usvg::LineJoin::Bevel => SvgLineJoin::Bevel,
                    usvg::LineJoin::Miter | usvg::LineJoin::MiterClip => SvgLineJoin::Miter,
                },
            )
        })
        .unwrap_or((0.0, SvgLineCap::Butt, SvgLineJoin::Miter));
    Ok(SvgPath {
        path: convert_path(path.data(), transform)?,
        fill,
        fill_rule,
        stroke,
        stroke_width,
        line_cap,
        line_join,
        clips: clips.to_vec(),
        masks: masks.to_vec(),
    })
}

fn paint(paint: &usvg::Paint, opacity: f32, path_transform: Transform) -> Option<SvgPaint> {
    match paint {
        usvg::Paint::Color(color) => Some(SvgPaint::Solid([
            f32::from(color.red) / 255.0,
            f32::from(color.green) / 255.0,
            f32::from(color.blue) / 255.0,
            opacity.clamp(0.0, 1.0),
        ])),
        usvg::Paint::LinearGradient(gradient) => {
            let transform = path_transform.pre_concat(gradient.transform());
            let from = transformed(Point::from_xy(gradient.x1(), gradient.y1()), transform);
            let to = transformed(Point::from_xy(gradient.x2(), gradient.y2()), transform);
            Some(SvgPaint::LinearGradient(SvgLinearGradient {
                from: [from.x, from.y],
                to: [to.x, to.y],
                stops: gradient
                    .stops()
                    .iter()
                    .map(|stop| {
                        let color = stop.color();
                        SvgGradientStop {
                            offset: stop.offset().get(),
                            color: [
                                f32::from(color.red) / 255.0,
                                f32::from(color.green) / 255.0,
                                f32::from(color.blue) / 255.0,
                                (stop.opacity().get() * opacity).clamp(0.0, 1.0),
                            ],
                        }
                    })
                    .collect(),
                spread: match gradient.spread_method() {
                    usvg::SpreadMethod::Pad => SvgGradientSpread::Pad,
                    usvg::SpreadMethod::Repeat => SvgGradientSpread::Repeat,
                    usvg::SpreadMethod::Reflect => SvgGradientSpread::Reflect,
                },
            }))
        }
        usvg::Paint::RadialGradient(gradient) => {
            let transform = path_transform.pre_concat(gradient.transform());
            let inverse = transform.invert()?;
            Some(SvgPaint::RadialGradient(SvgRadialGradient {
                center: [gradient.cx(), gradient.cy()],
                focal: [gradient.fx(), gradient.fy()],
                radius: gradient.r().get(),
                focal_radius: gradient.fr().get(),
                inverse_transform: [
                    inverse.sx, inverse.ky, inverse.kx, inverse.sy, inverse.tx, inverse.ty,
                ],
                stops: gradient
                    .stops()
                    .iter()
                    .map(|stop| {
                        let color = stop.color();
                        SvgGradientStop {
                            offset: stop.offset().get(),
                            color: [
                                f32::from(color.red) / 255.0,
                                f32::from(color.green) / 255.0,
                                f32::from(color.blue) / 255.0,
                                (stop.opacity().get() * opacity).clamp(0.0, 1.0),
                            ],
                        }
                    })
                    .collect(),
                spread: match gradient.spread_method() {
                    usvg::SpreadMethod::Pad => SvgGradientSpread::Pad,
                    usvg::SpreadMethod::Repeat => SvgGradientSpread::Repeat,
                    usvg::SpreadMethod::Reflect => SvgGradientSpread::Reflect,
                },
            }))
        }
        usvg::Paint::Pattern(pattern) => {
            let mut paths = Vec::new();
            let mut images = Vec::new();
            let mut order = Vec::new();
            let mut next_mask_key = 0;
            collect_group(
                pattern.root(),
                opacity,
                &[],
                &[],
                &mut next_mask_key,
                &mut paths,
                &mut images,
                &mut order,
            )
            .ok()?;
            let rect = pattern.rect();
            let transform = path_transform.pre_concat(pattern.transform());
            Some(SvgPaint::Pattern(Arc::new(SvgPattern {
                rect: [rect.x(), rect.y(), rect.width(), rect.height()],
                transform: [
                    transform.sx,
                    transform.ky,
                    transform.kx,
                    transform.sy,
                    transform.tx,
                    transform.ty,
                ],
                paths,
                images,
                order,
            })))
        }
    }
}

fn convert_path(source: &tiny_skia_path::Path, transform: Transform) -> Result<Path, SvgError> {
    let mut builder = Path::builder();
    let mut contour_open = false;
    for segment in source.segments() {
        match segment {
            PathSegment::MoveTo(position) => {
                if contour_open {
                    builder.end(false);
                }
                let position = transformed(position, transform);
                builder.begin(point(position.x, position.y));
                contour_open = true;
            }
            PathSegment::LineTo(position) => {
                let position = transformed(position, transform);
                builder.line_to(point(position.x, position.y));
            }
            PathSegment::QuadTo(control, position) => {
                let control = transformed(control, transform);
                let position = transformed(position, transform);
                builder.quadratic_bezier_to(
                    point(control.x, control.y),
                    point(position.x, position.y),
                );
            }
            PathSegment::CubicTo(control_1, control_2, position) => {
                let control_1 = transformed(control_1, transform);
                let control_2 = transformed(control_2, transform);
                let position = transformed(position, transform);
                builder.cubic_bezier_to(
                    point(control_1.x, control_1.y),
                    point(control_2.x, control_2.y),
                    point(position.x, position.y),
                );
            }
            PathSegment::Close => {
                if contour_open {
                    builder.close();
                    contour_open = false;
                }
            }
        }
    }
    if contour_open {
        builder.end(false);
    }
    Ok(builder.build())
}

fn transformed(mut point: Point, transform: Transform) -> Point {
    transform.map_point(&mut point);
    point
}

#[cfg(test)]
mod tests {
    use super::{
        SvgElementRef, SvgEngine, SvgFillRule, SvgGradientSpread, SvgImageResampling, SvgLineCap,
        SvgLineJoin, SvgMaskType, SvgPaint,
    };

    const SAMPLE: &str = r##"
        <svg xmlns="http://www.w3.org/2000/svg" width="120" height="60">
          <defs><path id="glyph" d="M0 0 L20 0 L10 20 Z"/></defs>
          <use href="#glyph" transform="translate(15 10)"/>
          <circle cx="80" cy="30" r="12"/>
        </svg>
    "##;

    #[test]
    fn resolves_references_and_basic_shapes_into_paths() {
        let mut engine = SvgEngine::new();
        let document = engine.document(SAMPLE).unwrap();
        assert_eq!(document.width, 120.0);
        assert_eq!(document.height, 60.0);
        assert_eq!(document.paths.len(), 2);
    }

    #[test]
    fn caches_normalized_documents() {
        let mut engine = SvgEngine::new();
        let _ = engine.document(SAMPLE).unwrap();
        let _ = engine.document(SAMPLE).unwrap();
        assert_eq!(engine.cache_size(), 1);
    }

    #[test]
    fn preserves_solid_path_paint_and_stroke_geometry() {
        let source = r##"
          <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20">
            <path d="M1 1H19V19Z" fill="#123456" fill-opacity=".5"
              fill-rule="evenodd" stroke="#abcdef" stroke-opacity=".75"
              stroke-width="2" stroke-linecap="round" stroke-linejoin="bevel"/>
          </svg>
        "##;
        let mut engine = SvgEngine::new();
        let path = &engine.document(source).unwrap().paths[0];
        assert_eq!(
            path.fill,
            Some(SvgPaint::Solid([
                18.0 / 255.0,
                52.0 / 255.0,
                86.0 / 255.0,
                0.5,
            ]))
        );
        assert_eq!(
            path.stroke,
            Some(SvgPaint::Solid([
                171.0 / 255.0,
                205.0 / 255.0,
                239.0 / 255.0,
                0.75,
            ]))
        );
        assert_eq!(path.stroke_width, 2.0);
        assert_eq!(path.fill_rule, SvgFillRule::EvenOdd);
        assert_eq!(path.line_cap, SvgLineCap::Round);
        assert_eq!(path.line_join, SvgLineJoin::Bevel);
    }

    #[test]
    fn preserves_linear_gradient_coordinates_stops_and_spread() {
        let source = r##"
          <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20">
            <defs>
              <linearGradient id="gradient" gradientUnits="userSpaceOnUse"
                x1="0" y1="0" x2="20" y2="0" spreadMethod="reflect">
                <stop offset="0" stop-color="#ff0000"/>
                <stop offset="1" stop-color="#0000ff" stop-opacity=".5"/>
              </linearGradient>
            </defs>
            <rect x="0" y="0" width="20" height="20"
              fill="url(#gradient)" fill-opacity=".5"/>
          </svg>
        "##;
        let mut engine = SvgEngine::new();
        let path = &engine.document(source).unwrap().paths[0];
        let Some(SvgPaint::LinearGradient(gradient)) = &path.fill else {
            panic!("expected a linear gradient");
        };
        assert_eq!(gradient.from, [0.0, 0.0]);
        assert_eq!(gradient.to, [20.0, 0.0]);
        assert_eq!(gradient.spread, SvgGradientSpread::Reflect);
        assert_eq!(gradient.stops.len(), 2);
        assert_eq!(gradient.stops[0].color, [1.0, 0.0, 0.0, 0.5]);
        assert_eq!(gradient.stops[1].color, [0.0, 0.0, 1.0, 0.25]);
    }

    #[test]
    fn preserves_radial_gradient_geometry_stops_transform_and_spread() {
        let source = r##"
          <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20">
            <defs>
              <radialGradient id="gradient" gradientUnits="userSpaceOnUse"
                cx="10" cy="9" r="8" fx="7" fy="8" fr="1"
                gradientTransform="translate(2 3) scale(2 1)" spreadMethod="repeat">
                <stop offset="0" stop-color="#ffffff"/>
                <stop offset="1" stop-color="#000000" stop-opacity=".5"/>
              </radialGradient>
            </defs>
            <rect x="0" y="0" width="20" height="20"
              fill="url(#gradient)" fill-opacity=".5"/>
          </svg>
        "##;
        let mut engine = SvgEngine::new();
        let path = &engine.document(source).unwrap().paths[0];
        let Some(SvgPaint::RadialGradient(gradient)) = &path.fill else {
            panic!("expected a radial gradient");
        };
        assert_eq!(gradient.center, [10.0, 9.0]);
        assert_eq!(gradient.focal, [7.0, 8.0]);
        assert_eq!(gradient.radius, 8.0);
        assert_eq!(gradient.focal_radius, 1.0);
        assert_eq!(gradient.spread, SvgGradientSpread::Repeat);
        assert_eq!(gradient.stops[0].color, [1.0, 1.0, 1.0, 0.5]);
        assert_eq!(gradient.stops[1].color, [0.0, 0.0, 0.0, 0.25]);
        assert!(
            gradient
                .inverse_transform
                .iter()
                .all(|value| value.is_finite())
        );
    }

    #[test]
    fn retains_group_and_linked_clip_paths_as_intersected_vector_unions() {
        let source = r##"
          <svg xmlns="http://www.w3.org/2000/svg" width="100" height="80">
            <defs>
              <clipPath id="round"><circle cx="50" cy="40" r="32"/></clipPath>
              <clipPath id="band" clip-path="url(#round)">
                <rect x="10" y="24" width="80" height="32"/>
              </clipPath>
            </defs>
            <g clip-path="url(#band)">
              <rect x="0" y="0" width="100" height="80" fill="#fff"/>
            </g>
          </svg>
        "##;
        let mut engine = SvgEngine::new();
        let path = &engine.document(source).unwrap().paths[0];
        assert_eq!(path.clips.len(), 2);
        assert_eq!(path.clips[0].shapes.len(), 1);
        assert_eq!(path.clips[1].shapes.len(), 1);
    }

    #[test]
    fn retains_clips_on_subgroups_inside_clip_definitions() {
        let source = r##"
          <svg xmlns="http://www.w3.org/2000/svg" width="100" height="80">
            <defs>
              <clipPath id="spot"><circle cx="68" cy="40" r="22"/></clipPath>
              <clipPath id="compound">
                <rect x="8" y="12" width="28" height="56" rx="8"/>
                <rect x="42" y="8" width="52" height="64" clip-path="url(#spot)"/>
              </clipPath>
            </defs>
            <rect width="100" height="80" fill="#fff" clip-path="url(#compound)"/>
          </svg>
        "##;
        let mut engine = SvgEngine::new();
        let path = &engine.document(source).unwrap().paths[0];
        assert_eq!(path.clips.len(), 1);
        assert_eq!(path.clips[0].shapes.len(), 2);
        assert!(path.clips[0].shapes[0].clips.is_empty());
        assert_eq!(path.clips[0].shapes[1].clips.len(), 1);
        assert_eq!(path.clips[0].shapes[1].clips[0].shapes.len(), 1);
    }

    #[test]
    fn retains_alpha_and_linked_luminance_masks_as_vector_layers() {
        let source = r##"
          <svg xmlns="http://www.w3.org/2000/svg" width="100" height="80">
            <defs>
              <mask id="fade" mask-type="luminance" x="10" y="8" width="80" height="64">
                <rect x="10" y="8" width="80" height="64" fill="#808080"/>
              </mask>
              <mask id="spot" mask-type="alpha" mask="url(#fade)">
                <circle cx="50" cy="40" r="32" fill="#fff" fill-opacity=".75"/>
              </mask>
            </defs>
            <g mask="url(#spot)">
              <rect x="0" y="0" width="100" height="80" fill="#fff"/>
            </g>
          </svg>
        "##;
        let mut engine = SvgEngine::new();
        let path = &engine.document(source).unwrap().paths[0];
        assert_eq!(path.masks.len(), 2);
        assert_eq!(path.masks[0].kind, SvgMaskType::Alpha);
        assert_eq!(path.masks[1].kind, SvgMaskType::Luminance);
        assert_eq!(path.masks[0].paths.len(), 1);
        assert_eq!(path.masks[1].paths.len(), 1);
        assert_ne!(path.masks[0].key, path.masks[1].key);
    }

    #[test]
    fn retains_masks_nested_inside_mask_content() {
        let source = r##"
          <svg xmlns="http://www.w3.org/2000/svg" width="100" height="80">
            <defs>
              <mask id="inner" mask-type="luminance" x="0" y="0" width="100" height="80">
                <rect x="0" y="0" width="100" height="80" fill="#fff"/>
                <circle cx="50" cy="40" r="14" fill="#000"/>
              </mask>
              <mask id="outer" mask-type="alpha" x="0" y="0" width="100" height="80">
                <g mask="url(#inner)">
                  <circle cx="50" cy="40" r="34" fill="#fff"/>
                </g>
              </mask>
            </defs>
            <rect width="100" height="80" fill="#38bdf8" mask="url(#outer)"/>
          </svg>
        "##;
        let mut engine = SvgEngine::new();
        let path = &engine.document(source).unwrap().paths[0];
        assert_eq!(path.masks.len(), 1);
        assert_eq!(path.masks[0].paths.len(), 1);
        assert_eq!(path.masks[0].paths[0].masks.len(), 1);
        assert_eq!(path.masks[0].paths[0].masks[0].kind, SvgMaskType::Luminance);
    }

    #[test]
    fn retains_vector_pattern_tiles_and_child_paints() {
        let source = r##"
          <svg xmlns="http://www.w3.org/2000/svg" width="120" height="80">
            <defs>
              <linearGradient id="ink"><stop stop-color="#38bdf8"/><stop offset="1" stop-color="#fb7185"/></linearGradient>
              <pattern id="grid" patternUnits="userSpaceOnUse" x="2" y="3" width="20" height="16" patternTransform="rotate(12)">
                <rect x="2" y="3" width="10" height="8" fill="url(#ink)"/>
                <circle cx="17" cy="11" r="3" fill="#fff"/>
              </pattern>
            </defs>
            <rect x="8" y="7" width="104" height="66" rx="10" fill="url(#grid)"/>
          </svg>
        "##;
        let mut engine = SvgEngine::new();
        let path = &engine.document(source).unwrap().paths[0];
        let Some(SvgPaint::Pattern(pattern)) = &path.fill else {
            panic!("expected vector pattern paint");
        };
        assert_eq!(pattern.rect, [2.0, 3.0, 20.0, 16.0]);
        assert_eq!(pattern.paths.len(), 2);
        assert!(matches!(
            pattern.paths[0].fill,
            Some(SvgPaint::LinearGradient(_))
        ));
        assert!(matches!(pattern.paths[1].fill, Some(SvgPaint::Solid(_))));
    }

    #[test]
    fn retains_recursively_nested_vector_pattern_paints() {
        let source = r##"
          <svg xmlns="http://www.w3.org/2000/svg" width="120" height="80">
            <defs>
              <pattern id="dots" patternUnits="userSpaceOnUse" width="8" height="8">
                <circle cx="4" cy="4" r="2" fill="#fff"/>
              </pattern>
              <pattern id="cards" patternUnits="userSpaceOnUse" width="32" height="24">
                <rect width="32" height="24" fill="#1e293b"/>
                <rect x="4" y="4" width="24" height="16" fill="url(#dots)"/>
              </pattern>
            </defs>
            <rect width="120" height="80" fill="url(#cards)"/>
          </svg>
        "##;
        let mut engine = SvgEngine::new();
        let path = &engine.document(source).unwrap().paths[0];
        let Some(SvgPaint::Pattern(outer)) = &path.fill else {
            panic!("expected outer vector pattern paint");
        };
        let Some(SvgPaint::Pattern(inner)) = &outer.paths[1].fill else {
            panic!("expected nested vector pattern paint");
        };
        assert_eq!(inner.rect, [0.0, 0.0, 8.0, 8.0]);
        assert_eq!(inner.paths.len(), 1);
    }

    #[test]
    fn retains_pattern_paints_inside_mask_content() {
        let source = r##"
          <svg xmlns="http://www.w3.org/2000/svg" width="120" height="80">
            <defs>
              <pattern id="dots" patternUnits="userSpaceOnUse" width="8" height="8">
                <circle cx="4" cy="4" r="3" fill="#fff"/>
              </pattern>
              <mask id="pattern-mask" mask-type="luminance" maskUnits="userSpaceOnUse" x="0" y="0" width="120" height="80">
                <rect width="120" height="80" fill="url(#dots)"/>
              </mask>
            </defs>
            <rect width="120" height="80" fill="#38bdf8" mask="url(#pattern-mask)"/>
          </svg>
        "##;
        let mut engine = SvgEngine::new();
        let path = &engine.document(source).unwrap().paths[0];
        assert_eq!(path.masks.len(), 1);
        assert!(matches!(
            path.masks[0].paths[0].fill,
            Some(SvgPaint::Pattern(_))
        ));
    }

    #[test]
    fn expands_embedded_svg_images_into_vector_paths() {
        let source = r##"
          <svg xmlns="http://www.w3.org/2000/svg" width="100" height="80">
            <image x="10" y="8" width="80" height="64"
              href="data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHdpZHRoPSIyMCIgaGVpZ2h0PSIxNiI+PHJlY3QgeD0iMiIgeT0iMyIgd2lkdGg9IjE2IiBoZWlnaHQ9IjEwIiBmaWxsPSIjMzhiZGY4Ii8+PC9zdmc+"/>
          </svg>
        "##;
        let mut engine = SvgEngine::new();
        let document = engine.document(source).unwrap();
        assert_eq!(document.paths.len(), 1);
        assert!(matches!(
            document.paths[0].fill,
            Some(SvgPaint::Solid([_, _, _, 1.0]))
        ));
    }

    #[test]
    fn decodes_embedded_raster_images_and_retains_paint_order() {
        let source = r##"
          <svg xmlns="http://www.w3.org/2000/svg" width="100" height="80">
            <rect width="100" height="80" fill="#111827"/>
            <image x="20" y="10" width="60" height="60" style="image-rendering:pixelated"
              href="data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAAFElEQVR4nGP4z8DwHwSZGME0QyMAP24Gf5yFloAAAAAASUVORK5CYII="/>
          </svg>
        "##;
        let mut engine = SvgEngine::new();
        let document = engine.document(source).unwrap();
        assert_eq!(document.images.len(), 1);
        assert_eq!(
            (
                document.images[0].pixel_width,
                document.images[0].pixel_height
            ),
            (2, 2)
        );
        assert_eq!(document.images[0].pixels.len(), 16);
        assert_eq!(document.images[0].resampling, SvgImageResampling::Nearest);
        assert_eq!(
            document.order,
            [SvgElementRef::Path(0), SvgElementRef::Image(0)]
        );
    }
}
