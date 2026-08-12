//! Portable production text shaping for the native and WebAssembly renderers.
//!
//! Text is shaped with Rustybuzz and converted from the bundled Noto Sans
//! OpenType outlines into lyon paths. There is no bitmap or browser-font
//! fallback, so layout and glyph geometry are deterministic across targets.

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use lyon::{math::point, path::Path};
use rustybuzz::{
    Face, UnicodeBuffer, shape,
    ttf_parser::{GlyphId, OutlineBuilder},
};

/// Horizontal alignment of a shaped line around its text-node origin.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextAlign {
    Left,
    #[default]
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FontVariant {
    #[default]
    Regular,
    Bold,
    Italic,
    BoldItalic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FontSelection<'a> {
    pub family: &'a str,
    pub variant: FontVariant,
}

/// One glyph positioned in scene units relative to a text node.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PositionedGlyph {
    pub font_id: u32,
    pub glyph_id: u16,
    pub variant: FontVariant,
    pub x: f32,
    pub y: f32,
    pub scale: f32,
}

/// Shaped multiline text with deterministic metrics.
#[derive(Clone, Debug, PartialEq)]
pub struct TextLayout {
    pub glyphs: Vec<PositionedGlyph>,
    pub width: f32,
    pub height: f32,
    pub ascender: f32,
    pub descender: f32,
}

#[derive(Clone, Debug)]
struct ShapedGlyph {
    glyph_id: u16,
    x: f32,
    y: f32,
}

#[derive(Clone, Debug)]
struct ShapedLine {
    glyphs: Vec<ShapedGlyph>,
    advance: f32,
}

/// Error returned when the bundled font or one of its glyphs is invalid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextError(String);

impl fmt::Display for TextError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for TextError {}

/// Cached font shaper and vector-outline provider.
pub struct TextEngine {
    families: Vec<FontFamily>,
    family_ids: HashMap<String, u32>,
    lines: HashMap<(u32, FontVariant, String), ShapedLine>,
    outlines: HashMap<(u32, FontVariant, u16), Option<Path>>,
}

enum FontBytes {
    Static(&'static [u8]),
    Owned(Arc<[u8]>),
}

impl FontBytes {
    fn as_slice(&self) -> &[u8] {
        match self {
            Self::Static(bytes) => bytes,
            Self::Owned(bytes) => bytes,
        }
    }
}

struct FontFamily {
    name: String,
    variants: [Option<FontBytes>; 4],
}

impl TextEngine {
    /// Creates the deterministic bundled Noto Sans text engine.
    pub fn new() -> Result<Self, TextError> {
        let validate = |data, label| {
            Face::from_slice(data, 0)
                .ok_or_else(|| TextError(format!("The bundled Noto Sans {label} font is invalid.")))
        };
        validate(ttf_noto_sans::REGULAR, "Regular")?;
        validate(ttf_noto_sans::BOLD, "Bold")?;
        validate(ttf_noto_sans::ITALIC, "Italic")?;
        validate(ttf_noto_sans::BOLD_ITALIC, "Bold Italic")?;
        let family = FontFamily {
            name: "Noto Sans".to_owned(),
            variants: [
                Some(FontBytes::Static(ttf_noto_sans::REGULAR)),
                Some(FontBytes::Static(ttf_noto_sans::BOLD)),
                Some(FontBytes::Static(ttf_noto_sans::ITALIC)),
                Some(FontBytes::Static(ttf_noto_sans::BOLD_ITALIC)),
            ],
        };
        Ok(Self {
            families: vec![family],
            family_ids: HashMap::from([("noto sans".to_owned(), 0)]),
            lines: HashMap::new(),
            outlines: HashMap::new(),
        })
    }

    /// The portable default font family.
    pub const fn family_name(&self) -> &'static str {
        "Noto Sans"
    }

    /// Registers one OpenType face under a portable family name.
    ///
    /// Register the four variants separately when they are available. Missing
    /// variants fall back to the family's regular face, then its first face.
    pub fn register_font(
        &mut self,
        family: &str,
        variant: FontVariant,
        bytes: Vec<u8>,
    ) -> Result<(), TextError> {
        self.register_font_shared(family, variant, Arc::from(bytes))
    }

    /// Registers an OpenType face backed by shared immutable bytes.
    pub fn register_font_shared(
        &mut self,
        family: &str,
        variant: FontVariant,
        bytes: Arc<[u8]>,
    ) -> Result<(), TextError> {
        let name = family.trim();
        if name.is_empty() || name.len() > 120 {
            return Err(TextError(
                "Font family names must contain 1–120 characters.".to_owned(),
            ));
        }
        if bytes.is_empty() || bytes.len() > 32 * 1024 * 1024 {
            return Err(TextError(
                "Font data must contain 1 byte–32 MiB.".to_owned(),
            ));
        }
        Face::from_slice(&bytes, 0)
            .ok_or_else(|| TextError(format!("{name} is not a valid OpenType font.")))?;

        let normalized = name.to_lowercase();
        let font_id = if let Some(font_id) = self.family_ids.get(&normalized).copied() {
            font_id
        } else {
            let font_id = u32::try_from(self.families.len())
                .map_err(|_| TextError("Too many font families are registered.".to_owned()))?;
            self.families.push(FontFamily {
                name: name.to_owned(),
                variants: [None, None, None, None],
            });
            self.family_ids.insert(normalized, font_id);
            font_id
        };
        self.families[font_id as usize].variants[variant.index()] = Some(FontBytes::Owned(bytes));
        self.lines.retain(|(id, _, _), _| *id != font_id);
        self.outlines.retain(|(id, _, _), _| *id != font_id);
        Ok(())
    }

    pub fn registered_families(&self) -> Vec<&str> {
        self.families
            .iter()
            .map(|family| family.name.as_str())
            .collect()
    }

    /// Reports whether a family can be selected without falling back silently.
    pub fn has_family(&self, family: &str) -> bool {
        self.family_ids.contains_key(&family.trim().to_lowercase())
    }

    /// Shapes text once and returns glyph positions scaled into scene units.
    pub fn layout(
        &mut self,
        text: &str,
        font_size: f32,
        align: TextAlign,
        line_height: f32,
        letter_spacing: f32,
    ) -> Result<TextLayout, TextError> {
        self.layout_variant(
            text,
            font_size,
            align,
            line_height,
            letter_spacing,
            FontVariant::Regular,
        )
    }

    pub fn layout_variant(
        &mut self,
        text: &str,
        font_size: f32,
        align: TextAlign,
        line_height: f32,
        letter_spacing: f32,
        variant: FontVariant,
    ) -> Result<TextLayout, TextError> {
        self.layout_family_variant(
            text,
            font_size,
            align,
            line_height,
            letter_spacing,
            FontSelection {
                family: self.family_name(),
                variant,
            },
        )
    }

    pub fn layout_family_variant(
        &mut self,
        text: &str,
        font_size: f32,
        align: TextAlign,
        line_height: f32,
        letter_spacing: f32,
        selection: FontSelection<'_>,
    ) -> Result<TextLayout, TextError> {
        if !font_size.is_finite() || font_size <= 0.0 {
            return Err(TextError(
                "Font size must be positive and finite.".to_owned(),
            ));
        }
        if !line_height.is_finite() || line_height <= 0.0 {
            return Err(TextError(
                "Line height must be positive and finite.".to_owned(),
            ));
        }
        if !letter_spacing.is_finite() {
            return Err(TextError("Letter spacing must be finite.".to_owned()));
        }

        let font_id = self
            .family_ids
            .get(&selection.family.trim().to_lowercase())
            .copied()
            .ok_or_else(|| {
                TextError(format!(
                    "Font family {} is not registered.",
                    selection.family
                ))
            })?;
        let face = self.face(font_id, selection.variant)?;
        let units_per_em = face.units_per_em() as f32;
        let scale = font_size / units_per_em;
        let ascender = face.ascender() as f32 * scale;
        let descender = face.descender() as f32 * scale;
        let lines: Vec<&str> = text.split('\n').collect();
        let line_advance = font_size * line_height;
        let block_height = if lines.is_empty() {
            0.0
        } else {
            ascender - descender + line_advance * lines.len().saturating_sub(1) as f32
        };
        let first_baseline = -0.5 * (ascender + descender)
            + 0.5 * line_advance * lines.len().saturating_sub(1) as f32;

        let mut glyphs = Vec::new();
        let mut width = 0.0_f32;
        for (line_index, text_line) in lines.into_iter().enumerate() {
            let shaped = self
                .shape_line(text_line, font_id, selection.variant)?
                .clone();
            let spacing_total = letter_spacing * shaped.glyphs.len().saturating_sub(1) as f32;
            let line_width = shaped.advance * scale + spacing_total;
            width = width.max(line_width);
            let start_x = match align {
                TextAlign::Left => 0.0,
                TextAlign::Center => -line_width * 0.5,
                TextAlign::Right => -line_width,
            };
            let baseline = first_baseline - line_index as f32 * line_advance;
            for (glyph_index, glyph) in shaped.glyphs.iter().enumerate() {
                glyphs.push(PositionedGlyph {
                    font_id,
                    glyph_id: glyph.glyph_id,
                    variant: selection.variant,
                    x: start_x + glyph.x * scale + glyph_index as f32 * letter_spacing,
                    y: baseline + glyph.y * scale,
                    scale,
                });
            }
        }

        Ok(TextLayout {
            glyphs,
            width,
            height: block_height,
            ascender,
            descender,
        })
    }

    /// Returns a cached vector outline in font units.
    pub fn glyph_outline(
        &mut self,
        glyph_id: u16,
        variant: FontVariant,
    ) -> Result<Option<&Path>, TextError> {
        self.glyph_outline_for_font(0, glyph_id, variant)
    }

    pub fn glyph_outline_for_font(
        &mut self,
        font_id: u32,
        glyph_id: u16,
        variant: FontVariant,
    ) -> Result<Option<&Path>, TextError> {
        let key = (font_id, variant, glyph_id);
        if !self.outlines.contains_key(&key) {
            let mut collector = OutlineCollector::default();
            let result = self
                .face(font_id, variant)?
                .outline_glyph(GlyphId(glyph_id), &mut collector);
            let outline = if result.is_some() {
                Some(collector.finish()?)
            } else {
                None
            };
            self.outlines.insert(key, outline);
        }
        Ok(self.outlines.get(&key).and_then(Option::as_ref))
    }

    /// Number of shaped-line and glyph-outline entries currently cached.
    pub fn cache_sizes(&self) -> (usize, usize) {
        (self.lines.len(), self.outlines.len())
    }

    fn shape_line(
        &mut self,
        text: &str,
        font_id: u32,
        variant: FontVariant,
    ) -> Result<&ShapedLine, TextError> {
        let key = (font_id, variant, text.to_owned());
        if !self.lines.contains_key(&key) {
            let shaped_line = {
                let face = self.face(font_id, variant)?;
                let mut buffer = UnicodeBuffer::new();
                buffer.push_str(text);
                buffer.guess_segment_properties();
                let shaped = shape(&face, &[], buffer);
                let mut cursor_x = 0.0_f32;
                let mut cursor_y = 0.0_f32;
                let glyphs = shaped
                    .glyph_infos()
                    .iter()
                    .zip(shaped.glyph_positions())
                    .map(|(info, position)| {
                        let glyph = ShapedGlyph {
                            glyph_id: info.glyph_id as u16,
                            x: cursor_x + position.x_offset as f32,
                            y: cursor_y + position.y_offset as f32,
                        };
                        cursor_x += position.x_advance as f32;
                        cursor_y += position.y_advance as f32;
                        glyph
                    })
                    .collect();
                ShapedLine {
                    glyphs,
                    advance: cursor_x.abs(),
                }
            };
            self.lines.insert(key.clone(), shaped_line);
        }
        Ok(self.lines.get(&key).expect("shaped line was inserted"))
    }

    fn face(&self, font_id: u32, variant: FontVariant) -> Result<Face<'_>, TextError> {
        let family = self
            .families
            .get(font_id as usize)
            .ok_or_else(|| TextError(format!("Font id {font_id} is not registered.")))?;
        let bytes = family.variants[variant.index()]
            .as_ref()
            .or(family.variants[FontVariant::Regular.index()].as_ref())
            .or_else(|| family.variants.iter().flatten().next())
            .ok_or_else(|| TextError(format!("Font family {} has no faces.", family.name)))?;
        Face::from_slice(bytes.as_slice(), 0)
            .ok_or_else(|| TextError(format!("Font family {} is invalid.", family.name)))
    }
}

impl FontVariant {
    const fn index(self) -> usize {
        match self {
            Self::Regular => 0,
            Self::Bold => 1,
            Self::Italic => 2,
            Self::BoldItalic => 3,
        }
    }
}

impl Default for TextEngine {
    fn default() -> Self {
        Self::new().expect("the compile-time Noto Sans font must be valid")
    }
}

#[derive(Clone, Debug)]
enum OutlineCommand {
    Move(f32, f32),
    Line(f32, f32),
    Quad(f32, f32, f32, f32),
    Cubic(f32, f32, f32, f32, f32, f32),
    Close,
}

#[derive(Default)]
struct OutlineCollector {
    commands: Vec<OutlineCommand>,
}

impl OutlineCollector {
    fn finish(self) -> Result<Path, TextError> {
        let mut builder = Path::builder();
        let mut contour_open = false;
        for command in self.commands {
            match command {
                OutlineCommand::Move(x, y) => {
                    if contour_open {
                        builder.end(false);
                    }
                    builder.begin(point(x, y));
                    contour_open = true;
                }
                OutlineCommand::Line(x, y) => {
                    builder.line_to(point(x, y));
                }
                OutlineCommand::Quad(cx, cy, x, y) => {
                    builder.quadratic_bezier_to(point(cx, cy), point(x, y));
                }
                OutlineCommand::Cubic(c1x, c1y, c2x, c2y, x, y) => {
                    builder.cubic_bezier_to(point(c1x, c1y), point(c2x, c2y), point(x, y));
                }
                OutlineCommand::Close => {
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
}

impl OutlineBuilder for OutlineCollector {
    fn move_to(&mut self, x: f32, y: f32) {
        self.commands.push(OutlineCommand::Move(x, y));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.commands.push(OutlineCommand::Line(x, y));
    }

    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        self.commands.push(OutlineCommand::Quad(cx, cy, x, y));
    }

    fn curve_to(&mut self, c1x: f32, c1y: f32, c2x: f32, c2y: f32, x: f32, y: f32) {
        self.commands
            .push(OutlineCommand::Cubic(c1x, c1y, c2x, c2y, x, y));
    }

    fn close(&mut self) {
        self.commands.push(OutlineCommand::Close);
    }
}

#[cfg(test)]
mod tests {
    use super::{FontSelection, FontVariant, TextAlign, TextEngine};

    #[test]
    fn uses_proportional_shaped_metrics() {
        let mut engine = TextEngine::new().unwrap();
        let narrow = engine
            .layout("iiii", 1.0, TextAlign::Left, 1.2, 0.0)
            .unwrap();
        let wide = engine
            .layout("WWWW", 1.0, TextAlign::Left, 1.2, 0.0)
            .unwrap();
        assert!(narrow.width < wide.width * 0.6);
    }

    #[test]
    fn applies_open_type_kerning() {
        let mut engine = TextEngine::new().unwrap();
        let pair = engine.layout("AV", 1.0, TextAlign::Left, 1.2, 0.0).unwrap();
        let separated = engine
            .layout("A", 1.0, TextAlign::Left, 1.2, 0.0)
            .unwrap()
            .width
            + engine
                .layout("V", 1.0, TextAlign::Left, 1.2, 0.0)
                .unwrap()
                .width;
        assert!(pair.width < separated);
    }

    #[test]
    fn caches_shaping_and_real_vector_outlines() {
        let mut engine = TextEngine::new().unwrap();
        let layout = engine
            .layout("Vector", 1.0, TextAlign::Center, 1.2, 0.0)
            .unwrap();
        for glyph in &layout.glyphs {
            let _ = engine.glyph_outline(glyph.glyph_id, glyph.variant).unwrap();
        }
        let first_sizes = engine.cache_sizes();
        let _ = engine
            .layout("Vector", 2.0, TextAlign::Right, 1.4, 0.1)
            .unwrap();
        for glyph in &layout.glyphs {
            let _ = engine.glyph_outline(glyph.glyph_id, glyph.variant).unwrap();
        }
        assert_eq!(engine.cache_sizes(), first_sizes);
        assert!(first_sizes.0 > 0);
        assert!(first_sizes.1 > 0);
    }

    #[test]
    fn centers_multiline_text_around_the_origin() {
        let mut engine = TextEngine::new().unwrap();
        let layout = engine
            .layout("top\nbottom", 1.0, TextAlign::Center, 1.25, 0.0)
            .unwrap();
        let minimum = layout
            .glyphs
            .iter()
            .map(|glyph| glyph.y)
            .fold(f32::INFINITY, f32::min);
        let maximum = layout
            .glyphs
            .iter()
            .map(|glyph| glyph.y)
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(minimum < 0.0);
        assert!(maximum > 0.0);
    }

    #[test]
    fn uses_real_bold_and_italic_faces() {
        let mut engine = TextEngine::new().unwrap();
        let regular = engine
            .layout_variant(
                "Quality",
                1.0,
                TextAlign::Left,
                1.2,
                0.0,
                FontVariant::Regular,
            )
            .unwrap();
        let bold = engine
            .layout_variant("Quality", 1.0, TextAlign::Left, 1.2, 0.0, FontVariant::Bold)
            .unwrap();
        let italic = engine
            .layout_variant(
                "Quality",
                1.0,
                TextAlign::Left,
                1.2,
                0.0,
                FontVariant::Italic,
            )
            .unwrap();
        assert_ne!(regular.glyphs, bold.glyphs);
        assert_ne!(regular.glyphs, italic.glyphs);
        assert_eq!(bold.glyphs[0].variant, FontVariant::Bold);
        assert_eq!(italic.glyphs[0].variant, FontVariant::Italic);
    }

    #[test]
    fn registers_owned_font_faces_without_leaking_lifetimes() {
        let mut engine = TextEngine::new().unwrap();
        engine
            .register_font(
                "Uploaded Sans",
                FontVariant::Regular,
                ttf_noto_sans::BOLD.to_vec(),
            )
            .unwrap();
        let layout = engine
            .layout_family_variant(
                "Agent",
                1.0,
                TextAlign::Left,
                1.2,
                0.0,
                FontSelection {
                    family: "uploaded sans",
                    variant: FontVariant::Italic,
                },
            )
            .unwrap();
        assert!(layout.width > 0.0);
        assert!(layout.glyphs.iter().all(|glyph| glyph.font_id == 1));
        for glyph in layout.glyphs {
            assert!(
                engine
                    .glyph_outline_for_font(glyph.font_id, glyph.glyph_id, glyph.variant)
                    .unwrap()
                    .is_some()
            );
        }
        assert_eq!(engine.registered_families(), ["Noto Sans", "Uploaded Sans"]);
    }

    #[test]
    fn rejects_invalid_uploaded_fonts_and_unknown_families() {
        let mut engine = TextEngine::new().unwrap();
        assert!(
            engine
                .register_font("Broken", FontVariant::Regular, vec![0, 1, 2])
                .is_err()
        );
        assert!(
            engine
                .layout_family_variant(
                    "text",
                    1.0,
                    TextAlign::Left,
                    1.2,
                    0.0,
                    FontSelection {
                        family: "Missing",
                        variant: FontVariant::Regular,
                    },
                )
                .is_err()
        );
    }

    #[test]
    fn reports_registered_families_case_insensitively() {
        let engine = TextEngine::new().expect("font engine");
        assert!(engine.has_family("Noto Sans"));
        assert!(engine.has_family("  noto sans  "));
        assert!(!engine.has_family("Missing Sans"));
    }
}
