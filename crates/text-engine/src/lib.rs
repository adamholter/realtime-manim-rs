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
use unicode_segmentation::UnicodeSegmentation;

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

#[derive(Clone, Copy, Debug)]
struct FontRun {
    font_id: u32,
    start: usize,
    end: usize,
}

#[derive(Clone, Debug)]
struct PendingRun {
    font_id: u32,
    scale: f32,
    shaped: ShapedLine,
}

#[derive(Clone, Debug)]
struct PendingLine {
    runs: Vec<PendingRun>,
    advance: f32,
    glyph_count: usize,
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
    coverage: HashMap<(u32, FontVariant, char), bool>,
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
            coverage: HashMap::new(),
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
        self.coverage.retain(|(id, _, _), _| *id != font_id);
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
        self.layout_family_chain_variant(
            text,
            font_size,
            align,
            line_height,
            letter_spacing,
            selection,
            &[],
        )
    }

    /// Shapes text with a deterministic registered-font fallback chain.
    ///
    /// The selected family is always tried first. With an explicit chain, its
    /// families follow in supplied order. With an empty chain, every uploaded
    /// family follows in registration order. Bundled Noto Sans is the final
    /// last resort in both cases. Adjacent grapheme clusters that resolve to
    /// the same face are shaped together so kerning, ligatures, and
    /// complex-script substitutions remain intact within each resolved run.
    #[allow(clippy::too_many_arguments)]
    pub fn layout_family_chain_variant(
        &mut self,
        text: &str,
        font_size: f32,
        align: TextAlign,
        line_height: f32,
        letter_spacing: f32,
        selection: FontSelection<'_>,
        fallback_families: &[&str],
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

        let font_chain = self.resolve_font_chain(selection.family, fallback_families)?;
        let primary_font_id = font_chain[0];
        let lines: Vec<&str> = text.split('\n').collect();
        let mut pending_lines = Vec::with_capacity(lines.len());
        let mut ascender = f32::NEG_INFINITY;
        let mut descender = f32::INFINITY;
        for text_line in &lines {
            let font_runs =
                self.font_runs(text_line, &font_chain, selection.variant, selection.family)?;
            let mut pending_runs = Vec::with_capacity(font_runs.len());
            let mut advance = 0.0_f32;
            let mut glyph_count = 0usize;

            if font_runs.is_empty() {
                let face = self.face(primary_font_id, selection.variant)?;
                let scale = font_size / face.units_per_em() as f32;
                ascender = ascender.max(face.ascender() as f32 * scale);
                descender = descender.min(face.descender() as f32 * scale);
            }

            for font_run in font_runs {
                let face = self.face(font_run.font_id, selection.variant)?;
                let scale = font_size / face.units_per_em() as f32;
                ascender = ascender.max(face.ascender() as f32 * scale);
                descender = descender.min(face.descender() as f32 * scale);
                let shaped = self
                    .shape_line(
                        &text_line[font_run.start..font_run.end],
                        font_run.font_id,
                        selection.variant,
                    )?
                    .clone();
                advance += shaped.advance * scale;
                glyph_count = glyph_count.saturating_add(shaped.glyphs.len());
                pending_runs.push(PendingRun {
                    font_id: font_run.font_id,
                    scale,
                    shaped,
                });
            }
            pending_lines.push(PendingLine {
                runs: pending_runs,
                advance,
                glyph_count,
            });
        }

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
        for (line_index, line) in pending_lines.into_iter().enumerate() {
            let spacing_total = letter_spacing * line.glyph_count.saturating_sub(1) as f32;
            let line_width = line.advance + spacing_total;
            width = width.max(line_width);
            let start_x = match align {
                TextAlign::Left => 0.0,
                TextAlign::Center => -line_width * 0.5,
                TextAlign::Right => -line_width,
            };
            let baseline = first_baseline - line_index as f32 * line_advance;
            let mut run_x = start_x;
            let mut glyph_index = 0usize;
            for run in line.runs {
                for glyph in &run.shaped.glyphs {
                    glyphs.push(PositionedGlyph {
                        font_id: run.font_id,
                        glyph_id: glyph.glyph_id,
                        variant: selection.variant,
                        x: run_x + glyph.x * run.scale + glyph_index as f32 * letter_spacing,
                        y: baseline + glyph.y * run.scale,
                        scale: run.scale,
                    });
                    glyph_index += 1;
                }
                run_x += run.shaped.advance * run.scale;
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

    fn resolve_font_chain(
        &self,
        primary_family: &str,
        explicit_fallbacks: &[&str],
    ) -> Result<Vec<u32>, TextError> {
        let family_id = |family: &str| {
            self.family_ids
                .get(&family.trim().to_lowercase())
                .copied()
                .ok_or_else(|| TextError(format!("Font family {family} is not registered.")))
        };
        let primary_id = family_id(primary_family)?;
        let mut chain = vec![primary_id];
        for family in explicit_fallbacks {
            let font_id = family_id(family)?;
            if !chain.contains(&font_id) {
                chain.push(font_id);
            }
        }

        // With no explicit chain, uploaded faces are deterministic fallbacks
        // in registration order. With an explicit chain, do not silently add
        // unrelated faces; only the portable bundled last resort is appended.
        if explicit_fallbacks.is_empty() {
            for font_id in 1..self.families.len() as u32 {
                if !chain.contains(&font_id) {
                    chain.push(font_id);
                }
            }
        }
        if !chain.contains(&0) {
            chain.push(0);
        }
        Ok(chain)
    }

    fn font_runs(
        &mut self,
        text: &str,
        font_chain: &[u32],
        variant: FontVariant,
        primary_family: &str,
    ) -> Result<Vec<FontRun>, TextError> {
        let mut runs: Vec<FontRun> = Vec::new();
        for (start, grapheme) in text.grapheme_indices(true) {
            let font_id = if grapheme.chars().all(is_default_ignorable) {
                font_chain[0]
            } else {
                let mut selected = None;
                for font_id in font_chain {
                    if self.font_covers_grapheme(*font_id, variant, grapheme)? {
                        selected = Some(*font_id);
                        break;
                    }
                }
                selected.ok_or_else(|| {
                    let codepoints = grapheme
                        .chars()
                        .filter(|character| !is_default_ignorable(*character))
                        .map(|character| format!("U+{:04X}", character as u32))
                        .collect::<Vec<_>>()
                        .join(" ");
                    let families = font_chain
                        .iter()
                        .filter_map(|font_id| self.families.get(*font_id as usize))
                        .map(|family| family.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ");
                    TextError(format!(
                        "No registered vector font covers {codepoints} for {primary_family}. Fallback order: {families}."
                    ))
                })?
            };
            let end = start + grapheme.len();
            if let Some(run) = runs.last_mut()
                && run.font_id == font_id
            {
                run.end = end;
            } else {
                runs.push(FontRun {
                    font_id,
                    start,
                    end,
                });
            }
        }
        Ok(runs)
    }

    fn font_covers_grapheme(
        &mut self,
        font_id: u32,
        variant: FontVariant,
        grapheme: &str,
    ) -> Result<bool, TextError> {
        for character in grapheme.chars() {
            if is_default_ignorable(character) {
                continue;
            }
            if !self.supports_character(font_id, variant, character)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn supports_character(
        &mut self,
        font_id: u32,
        variant: FontVariant,
        character: char,
    ) -> Result<bool, TextError> {
        let key = (font_id, variant, character);
        if let Some(supported) = self.coverage.get(&key).copied() {
            return Ok(supported);
        }
        let supported = {
            let face = self.face(font_id, variant)?;
            preferred_glyph_index(&face, character).is_some_and(|glyph_id| {
                character.is_whitespace()
                    || face.outline_glyph(glyph_id, &mut OutlineProbe).is_some()
            })
        };
        self.coverage.insert(key, supported);
        Ok(supported)
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
                let family_name = self.families[font_id as usize].name.clone();
                let face = self.face(font_id, variant)?;
                let mut buffer = UnicodeBuffer::new();
                buffer.push_str(text);
                buffer.guess_segment_properties();
                let shaped = shape(&face, &[], buffer);
                let mut cursor_x = 0.0_f32;
                let mut cursor_y = 0.0_f32;
                let mut glyphs = Vec::with_capacity(shaped.len());
                for (info, position) in shaped.glyph_infos().iter().zip(shaped.glyph_positions()) {
                    let source_character = text
                        .get(info.cluster as usize..)
                        .and_then(|cluster| cluster.chars().next());
                    cursor_x += position.x_advance as f32;
                    cursor_y += position.y_advance as f32;
                    if info.glyph_id == 0 {
                        if source_character.is_some_and(is_default_ignorable) {
                            continue;
                        }
                        let codepoint = source_character
                            .map(|character| format!("U+{:04X}", character as u32))
                            .unwrap_or_else(|| "an invalid cluster".to_owned());
                        return Err(TextError(format!(
                            "Font family {family_name} produced a missing glyph for {codepoint}."
                        )));
                    }
                    let glyph_id = info.glyph_id as u16;
                    if source_character.is_some_and(|character| {
                        !character.is_whitespace() && !is_default_ignorable(character)
                    }) && face
                        .outline_glyph(GlyphId(glyph_id), &mut OutlineProbe)
                        .is_none()
                    {
                        let character = source_character.expect("visible source character");
                        return Err(TextError(format!(
                            "Font family {family_name} has no vector outline for U+{:04X}.",
                            character as u32
                        )));
                    }
                    glyphs.push(ShapedGlyph {
                        glyph_id,
                        x: cursor_x - position.x_advance as f32 + position.x_offset as f32,
                        y: cursor_y - position.y_advance as f32 + position.y_offset as f32,
                    });
                }
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

fn preferred_glyph_index(face: &Face<'_>, character: char) -> Option<GlyphId> {
    let face: &rustybuzz::ttf_parser::Face<'_> = face.as_ref();
    let cmap = face.tables().cmap?;
    let codepoint = character as u32;
    for subtable in cmap.subtables {
        if subtable.is_unicode()
            && let Some(glyph_id) = subtable.glyph_index(codepoint)
        {
            return Some(glyph_id);
        }
    }
    None
}

fn is_default_ignorable(character: char) -> bool {
    matches!(
        character as u32,
        0x00AD
            | 0x034F
            | 0x061C
            | 0x115F..=0x1160
            | 0x17B4..=0x17B5
            | 0x180B..=0x180F
            | 0x200B..=0x200F
            | 0x202A..=0x202E
            | 0x2060..=0x206F
            | 0x3164
            | 0xFE00..=0xFE0F
            | 0xFEFF
            | 0xFFA0
            | 0x1BCA0..=0x1BCA3
            | 0x1D173..=0x1D17A
            | 0xE0000..=0xE0FFF
    )
}

struct OutlineProbe;

impl OutlineBuilder for OutlineProbe {
    fn move_to(&mut self, _x: f32, _y: f32) {}

    fn line_to(&mut self, _x: f32, _y: f32) {}

    fn quad_to(&mut self, _cx: f32, _cy: f32, _x: f32, _y: f32) {}

    fn curve_to(&mut self, _c1x: f32, _c1y: f32, _c2x: f32, _c2y: f32, _x: f32, _y: f32) {}

    fn close(&mut self) {}
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
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

    use super::{FontSelection, FontVariant, TextAlign, TextEngine};

    // The 400-byte A-only outline font from ttf-parser's Apache-2.0/MIT test
    // corpus. Keeping the fixture inline makes fallback tests reproducible on
    // native and Wasm hosts without relying on system fonts.
    const A_ONLY_FONT: &str = "AAEAAAAHAEAAAgAwY21hcAAJAHYAAAEAAAAALGdseWbxy2aYAAABNAAAAFxoZWFk8jXd+AAAAHwAAAA2aGhlYQZhAMoAAAC0AAAAJGhtdHgEdABqAAAA+AAAAAhsb2NhAC4AFAAAASwAAAAGbWF4cAAFAAsAAADYAAAAIAABAAAAAQAA9ZwpRF8PPPUAAgPoAAAAALSS9AAAAAAA3C+mXAAGAAACWAK8AAAAAwACAAAAAAAAAAEAAAQA/nAAAAJYAAb//wJYAAEAAAAAAAAAAAAAAAAAAAACAAEAAAACAAsAAgAAAAAAAAAAAAAAAAAAAAAAAAAAAAACWABkAhwABgAAAAEAAAADAAAADAAEACAAAAAEAAQAAQAAAEH//wAAAEH////AAAEAAAAAAAAAFAAuAAAAAgBkAAACWAK8AAMABwAAMxEhESUhESFkAfT+NAGk/lwCvP1EKAJsAAIABgAAAh0CkAACAAoAABMzAwETMxMjJyMHrcRj/vjaYN1ZPu9CAQsBQP21ApD9cMjIAA==";

    fn a_only_font() -> Vec<u8> {
        BASE64.decode(A_ONLY_FONT).expect("embedded test font")
    }

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

    #[test]
    fn resolves_mixed_font_runs_in_explicit_order() {
        let mut engine = TextEngine::new().expect("font engine");
        engine
            .register_font("A Only", FontVariant::Regular, a_only_font())
            .expect("partial primary font");
        engine
            .register_font(
                "Greek First",
                FontVariant::Regular,
                ttf_noto_sans::BOLD.to_vec(),
            )
            .expect("first fallback");
        engine
            .register_font(
                "Greek Second",
                FontVariant::Regular,
                ttf_noto_sans::REGULAR.to_vec(),
            )
            .expect("second fallback");

        let layout = engine
            .layout_family_chain_variant(
                "AΩA",
                1.0,
                TextAlign::Left,
                1.2,
                0.0,
                FontSelection {
                    family: "A Only",
                    variant: FontVariant::Regular,
                },
                &["Greek First", "Greek Second"],
            )
            .expect("mixed-font layout");

        assert_eq!(
            layout
                .glyphs
                .iter()
                .map(|glyph| glyph.font_id)
                .collect::<Vec<_>>(),
            [1, 2, 1]
        );
        assert!(layout.glyphs.iter().all(|glyph| glyph.glyph_id != 0));
    }

    #[test]
    fn keeps_a_combining_grapheme_in_one_fallback_run() {
        let mut engine = TextEngine::new().expect("font engine");
        engine
            .register_font("A Only", FontVariant::Regular, a_only_font())
            .expect("partial primary font");

        let layout = engine
            .layout_family_variant(
                "A\u{301}",
                1.0,
                TextAlign::Left,
                1.2,
                0.0,
                FontSelection {
                    family: "A Only",
                    variant: FontVariant::Regular,
                },
            )
            .expect("combined grapheme fallback");

        assert!(!layout.glyphs.is_empty());
        assert!(layout.glyphs.iter().all(|glyph| glyph.font_id == 0));
        assert!(layout.glyphs.iter().all(|glyph| glyph.glyph_id != 0));
    }

    #[test]
    fn rejects_uncovered_text_instead_of_emitting_notdef() {
        let mut engine = TextEngine::new().expect("font engine");
        let error = engine
            .layout("😀", 1.0, TextAlign::Left, 1.2, 0.0)
            .expect_err("bundled Noto Sans intentionally has no emoji outline");
        assert!(error.to_string().contains("U+1F600"));
        assert!(
            error
                .to_string()
                .contains("No registered vector font covers")
        );
    }
}
