use std::fmt::Write;

use super::{FontVariant, TextError};

/// Pango underline styles supported by the retained renderers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PangoUnderline {
    #[default]
    None,
    Single,
    Double,
    Low,
    Error,
}

/// Fully inherited style for a decoded Pango-markup text range.
///
/// Sizes, rise, and letter spacing are normalized to em units so the same
/// parsed document can be laid out at any scene font size. Paint values are
/// canonical #rrggbbaa strings.
#[derive(Clone, Debug, PartialEq)]
pub struct MarkupStyle {
    pub variant: FontVariant,
    pub font_family: Option<String>,
    pub font_scale: f32,
    pub rise: f32,
    pub letter_spacing: f32,
    pub foreground: Option<String>,
    pub background: Option<String>,
    pub underline: PangoUnderline,
    pub underline_color: Option<String>,
    pub strikethrough: bool,
    pub strikethrough_color: Option<String>,
}

impl Default for MarkupStyle {
    fn default() -> Self {
        Self {
            variant: FontVariant::Regular,
            font_family: None,
            font_scale: 1.0,
            rise: 0.0,
            letter_spacing: 0.0,
            foreground: None,
            background: None,
            underline: PangoUnderline::None,
            underline_color: None,
            strikethrough: false,
            strikethrough_color: None,
        }
    }
}

/// One decoded source range carrying its complete inherited Pango style.
#[derive(Clone, Debug, PartialEq)]
pub struct MarkupSpan {
    pub text: String,
    pub style: MarkupStyle,
}

#[derive(Clone, Debug)]
struct OpenElement {
    name: String,
    style: MarkupStyle,
}

type MarkupAttributes = Vec<(String, String)>;

struct StartTag {
    name: String,
    attributes: MarkupAttributes,
    self_closing: bool,
}

/// Parses the safe, deterministic Pango markup surface implemented by the
/// native and browser renderers.
///
/// The parser accepts nested span, b, i, u, s, big, small, sup, and sub
/// elements plus XML character references. It deliberately rejects
/// declarations, processing instructions, unknown elements, unknown
/// attributes, malformed nesting, and unsupported Pango features rather than
/// silently flattening them.
pub fn parse_pango_markup(markup: &str) -> Result<Vec<MarkupSpan>, TextError> {
    if markup.is_empty() {
        return Err(markup_error(0, "markup must not be empty"));
    }
    if markup.len() > 64 * 1024 {
        return Err(markup_error(0, "markup exceeds the 64 KiB safety limit"));
    }

    let mut spans: Vec<MarkupSpan> = Vec::new();
    let mut stack = vec![OpenElement {
        name: "#document".to_owned(),
        style: MarkupStyle::default(),
    }];
    let mut cursor = 0usize;
    let mut root_markup_seen = false;
    let mut root_markup_closed = false;

    while cursor < markup.len() {
        let tail = &markup[cursor..];
        if tail.starts_with('<') {
            let close = find_tag_end(markup, cursor)?;
            let raw = &markup[cursor + 1..close];
            if raw.starts_with('!') || raw.starts_with('?') {
                return Err(markup_error(
                    cursor,
                    "XML declarations, comments, CDATA, and processing instructions are not supported",
                ));
            }
            if let Some(raw_name) = raw.strip_prefix('/') {
                let name = raw_name.trim();
                validate_element_name(name, cursor)?;
                let open = stack
                    .pop()
                    .expect("the document element remains until an invalid close");
                if open.name == "#document" {
                    return Err(markup_error(
                        cursor,
                        &format!("unexpected closing element </{name}>"),
                    ));
                }
                if open.name != name {
                    return Err(markup_error(
                        cursor,
                        &format!("expected </{}> before </{name}>", open.name),
                    ));
                }
                if name == "markup" {
                    root_markup_closed = true;
                }
            } else {
                let StartTag {
                    name,
                    attributes,
                    self_closing,
                } = parse_start_tag(raw, cursor)?;
                if root_markup_closed {
                    return Err(markup_error(
                        cursor,
                        "content after the closing <markup> element is not allowed",
                    ));
                }
                if name == "markup" {
                    if root_markup_seen || stack.len() != 1 || !spans.is_empty() {
                        return Err(markup_error(
                            cursor,
                            "<markup> is optional but, when present, must be the single outer element",
                        ));
                    }
                    if !attributes.is_empty() {
                        return Err(markup_error(cursor, "<markup> does not accept attributes"));
                    }
                    root_markup_seen = true;
                } else if root_markup_seen && stack.len() == 1 {
                    return Err(markup_error(
                        cursor,
                        "content outside <markup> is not allowed",
                    ));
                }

                let parent = stack
                    .last()
                    .expect("document style always remains on the stack")
                    .style
                    .clone();
                let style = apply_element(&name, &attributes, parent, cursor)?;
                if self_closing {
                    if name == "markup" {
                        root_markup_closed = true;
                    }
                } else {
                    if stack.len() >= 65 {
                        return Err(markup_error(cursor, "markup nesting exceeds 64 elements"));
                    }
                    stack.push(OpenElement { name, style });
                }
            }
            cursor = close + 1;
        } else {
            let next = tail
                .find('<')
                .map_or(markup.len(), |offset| cursor + offset);
            let raw_text = &markup[cursor..next];
            if root_markup_closed {
                if !raw_text.trim().is_empty() {
                    return Err(markup_error(cursor, "text outside <markup> is not allowed"));
                }
                // XML permits insignificant document whitespace after the
                // optional root element. It is syntax, not rendered text.
                cursor = next;
                continue;
            }
            let text = decode_entities(raw_text, cursor)?;
            if !text.is_empty() {
                let style = stack
                    .last()
                    .expect("document style always remains on the stack")
                    .style
                    .clone();
                push_span(&mut spans, text, style);
            }
            cursor = next;
        }
    }

    if stack.len() != 1 {
        let open = stack.last().expect("an unclosed element remains");
        return Err(markup_error(
            markup.len(),
            &format!("unclosed <{}> element", open.name),
        ));
    }
    if root_markup_seen && !root_markup_closed {
        return Err(markup_error(markup.len(), "unclosed <markup> element"));
    }
    if spans.iter().all(|span| span.text.is_empty()) {
        return Err(markup_error(0, "markup contains no text"));
    }
    Ok(spans)
}

fn push_span(spans: &mut Vec<MarkupSpan>, text: String, style: MarkupStyle) {
    if let Some(previous) = spans.last_mut()
        && previous.style == style
    {
        previous.text.push_str(&text);
    } else {
        spans.push(MarkupSpan { text, style });
    }
}

fn find_tag_end(markup: &str, start: usize) -> Result<usize, TextError> {
    let mut quote = None;
    for (offset, character) in markup[start + 1..].char_indices() {
        match character {
            '\'' | '"' if quote == Some(character) => quote = None,
            '\'' | '"' if quote.is_none() => quote = Some(character),
            '>' if quote.is_none() => return Ok(start + 1 + offset),
            _ => {}
        }
    }
    Err(markup_error(start, "unterminated markup element"))
}

fn parse_start_tag(raw: &str, offset: usize) -> Result<StartTag, TextError> {
    let trimmed = raw.trim();
    let (content, self_closing) = if let Some(content) = trimmed.strip_suffix('/') {
        (content.trim_end(), true)
    } else {
        (trimmed, false)
    };
    if content.is_empty() {
        return Err(markup_error(offset, "empty markup element"));
    }
    let name_end = content.find(char::is_whitespace).unwrap_or(content.len());
    let name = &content[..name_end];
    validate_element_name(name, offset)?;
    let attributes = parse_attributes(&content[name_end..], offset + 1 + name_end)?;
    Ok(StartTag {
        name: name.to_owned(),
        attributes,
        self_closing,
    })
}

fn validate_element_name(name: &str, offset: usize) -> Result<(), TextError> {
    if name.is_empty()
        || !name
            .chars()
            .all(|character| character.is_ascii_lowercase() || character == '_')
    {
        return Err(markup_error(
            offset,
            &format!("invalid element name {name:?}"),
        ));
    }
    Ok(())
}

fn parse_attributes(raw: &str, offset: usize) -> Result<MarkupAttributes, TextError> {
    let bytes = raw.as_bytes();
    let mut cursor = 0usize;
    let mut result = Vec::new();
    while cursor < bytes.len() {
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor == bytes.len() {
            break;
        }
        let name_start = cursor;
        while cursor < bytes.len() && (bytes[cursor].is_ascii_lowercase() || bytes[cursor] == b'_')
        {
            cursor += 1;
        }
        if cursor == name_start {
            return Err(markup_error(offset + cursor, "invalid attribute name"));
        }
        let name = &raw[name_start..cursor];
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if bytes.get(cursor) != Some(&b'=') {
            return Err(markup_error(
                offset + cursor,
                &format!("attribute {name} requires = and a quoted value"),
            ));
        }
        cursor += 1;
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        let Some(quote @ (b'\'' | b'"')) = bytes.get(cursor).copied() else {
            return Err(markup_error(
                offset + cursor,
                &format!("attribute {name} requires a quoted value"),
            ));
        };
        cursor += 1;
        let value_start = cursor;
        while cursor < bytes.len() && bytes[cursor] != quote {
            if bytes[cursor] == b'<' {
                return Err(markup_error(
                    offset + cursor,
                    "< is not allowed in attribute values",
                ));
            }
            cursor += 1;
        }
        if cursor == bytes.len() {
            return Err(markup_error(
                offset + value_start,
                &format!("unterminated value for attribute {name}"),
            ));
        }
        let value = decode_entities(&raw[value_start..cursor], offset + value_start)?;
        cursor += 1;
        if result
            .iter()
            .any(|(existing, _): &(String, String)| existing == name)
        {
            return Err(markup_error(
                offset + name_start,
                &format!("duplicate attribute {name}"),
            ));
        }
        result.push((name.to_owned(), value));
    }
    Ok(result)
}

fn apply_element(
    name: &str,
    attributes: &[(String, String)],
    mut style: MarkupStyle,
    offset: usize,
) -> Result<MarkupStyle, TextError> {
    match name {
        "markup" => {}
        "b" => {
            no_attributes(name, attributes, offset)?;
            style.variant = with_weight(style.variant, true);
        }
        "i" => {
            no_attributes(name, attributes, offset)?;
            style.variant = with_italic(style.variant, true);
        }
        "u" => {
            no_attributes(name, attributes, offset)?;
            style.underline = PangoUnderline::Single;
        }
        "s" => {
            no_attributes(name, attributes, offset)?;
            style.strikethrough = true;
        }
        "big" => {
            no_attributes(name, attributes, offset)?;
            style.font_scale *= 1.2;
        }
        "small" => {
            no_attributes(name, attributes, offset)?;
            style.font_scale /= 1.2;
        }
        "sup" => {
            no_attributes(name, attributes, offset)?;
            style.font_scale *= 5.0 / 6.0;
            style.rise += 0.5;
        }
        "sub" => {
            no_attributes(name, attributes, offset)?;
            style.font_scale *= 5.0 / 6.0;
            style.rise -= 0.25;
        }
        "span" => apply_span_attributes(attributes, &mut style, offset)?,
        "tt" => {
            return Err(markup_error(
                offset,
                "<tt> requires a registered portable monospace face and is not yet supported; use <span font_family=\"...\">",
            ));
        }
        _ => {
            return Err(markup_error(
                offset,
                &format!("unsupported Pango element <{name}>"),
            ));
        }
    }
    Ok(style)
}

fn no_attributes(
    name: &str,
    attributes: &[(String, String)],
    offset: usize,
) -> Result<(), TextError> {
    if let Some((attribute, _)) = attributes.first() {
        return Err(markup_error(
            offset,
            &format!("<{name}> does not accept attribute {attribute}"),
        ));
    }
    Ok(())
}

fn apply_span_attributes(
    attributes: &[(String, String)],
    style: &mut MarkupStyle,
    offset: usize,
) -> Result<(), TextError> {
    for (name, value) in attributes {
        match name.as_str() {
            "foreground" | "fgcolor" | "color" => {
                style.foreground = Some(parse_markup_color(value, name, offset)?);
            }
            "background" | "bgcolor" => {
                style.background = Some(parse_markup_color(value, name, offset)?);
            }
            "font_family" | "face" => {
                let family = value.trim();
                if family.is_empty() || family.len() > 120 {
                    return Err(markup_error(
                        offset,
                        "font_family must contain 1–120 characters",
                    ));
                }
                style.font_family = Some(family.to_owned());
            }
            "weight" | "font_weight" => {
                let bold = match value.to_ascii_lowercase().as_str() {
                    "normal" | "book" | "medium" | "400" | "500" => false,
                    "semibold" | "bold" | "ultrabold" | "heavy" | "600" | "700" | "800" | "900"
                    | "1000" => true,
                    "thin" | "ultralight" | "light" | "100" | "200" | "300" => false,
                    _ => {
                        return Err(markup_error(
                            offset,
                            &format!("unsupported Pango weight {value:?}"),
                        ));
                    }
                };
                style.variant = with_weight(style.variant, bold);
            }
            "style" | "font_style" => {
                let italic = match value.to_ascii_lowercase().as_str() {
                    "normal" => false,
                    "italic" | "oblique" => true,
                    _ => {
                        return Err(markup_error(
                            offset,
                            &format!("unsupported Pango style {value:?}"),
                        ));
                    }
                };
                style.variant = with_italic(style.variant, italic);
            }
            "size" | "font_size" => {
                style.font_scale = parse_size(value, style.font_scale, offset)?;
            }
            "rise" => style.rise = parse_em_length(value, "rise", offset)?,
            "letter_spacing" => {
                style.letter_spacing = parse_em_length(value, "letter_spacing", offset)?;
            }
            "underline" => {
                style.underline = match value.to_ascii_lowercase().as_str() {
                    "none" => PangoUnderline::None,
                    "single" => PangoUnderline::Single,
                    "double" => PangoUnderline::Double,
                    "low" => PangoUnderline::Low,
                    "error" => PangoUnderline::Error,
                    _ => {
                        return Err(markup_error(
                            offset,
                            &format!("unsupported Pango underline {value:?}"),
                        ));
                    }
                };
            }
            "underline_color" => {
                style.underline_color = Some(parse_markup_color(value, name, offset)?);
            }
            "strikethrough" => {
                style.strikethrough = parse_boolean(value, name, offset)?;
            }
            "strikethrough_color" => {
                style.strikethrough_color = Some(parse_markup_color(value, name, offset)?);
            }
            unsupported => {
                return Err(markup_error(
                    offset,
                    &format!("unsupported Pango <span> attribute {unsupported}"),
                ));
            }
        }
    }
    Ok(())
}

fn parse_boolean(value: &str, attribute: &str, offset: usize) -> Result<bool, TextError> {
    match value.to_ascii_lowercase().as_str() {
        "true" | "yes" | "1" => Ok(true),
        "false" | "no" | "0" => Ok(false),
        _ => Err(markup_error(
            offset,
            &format!("{attribute} must be true or false, not {value:?}"),
        )),
    }
}

// Manim's Pango input uses 48 pt as its default logical size. Numeric Pango
// values are in 1/1024 pt, so normalize both absolute forms against 48 pt.
fn parse_size(value: &str, inherited_scale: f32, offset: usize) -> Result<f32, TextError> {
    let lower = value.trim().to_ascii_lowercase();
    let scale = match lower.as_str() {
        "xx-small" => 0.578_703_7,
        "x-small" => 0.694_444_4,
        "small" => 0.833_333_3,
        "medium" => 1.0,
        "large" => 1.2,
        "x-large" => 1.44,
        "xx-large" => 1.728,
        "smaller" => inherited_scale / 1.2,
        "larger" => inherited_scale * 1.2,
        _ if lower.ends_with('%') => {
            inherited_scale * parse_finite(&lower[..lower.len() - 1], "size percentage", offset)?
                / 100.0
        }
        _ if lower.ends_with("pt") => {
            parse_finite(&lower[..lower.len() - 2], "point size", offset)? / 48.0
        }
        _ => parse_finite(&lower, "Pango size", offset)? / (48.0 * 1024.0),
    };
    if !(0.01..=100.0).contains(&scale) {
        return Err(markup_error(
            offset,
            "size must resolve to 1%–10,000% of the base size",
        ));
    }
    Ok(scale)
}

fn parse_em_length(value: &str, attribute: &str, offset: usize) -> Result<f32, TextError> {
    let lower = value.trim().to_ascii_lowercase();
    let em = if lower.ends_with("pt") {
        parse_finite(&lower[..lower.len() - 2], attribute, offset)? / 48.0
    } else {
        parse_finite(&lower, attribute, offset)? / (48.0 * 1024.0)
    };
    if !(-100.0..=100.0).contains(&em) {
        return Err(markup_error(
            offset,
            &format!("{attribute} exceeds the ±100 em safety limit"),
        ));
    }
    Ok(em)
}

fn parse_finite(value: &str, label: &str, offset: usize) -> Result<f32, TextError> {
    let parsed = value
        .parse::<f32>()
        .map_err(|_| markup_error(offset, &format!("{label} is not a number: {value:?}")))?;
    if !parsed.is_finite() {
        return Err(markup_error(offset, &format!("{label} must be finite")));
    }
    Ok(parsed)
}

fn parse_markup_color(value: &str, attribute: &str, offset: usize) -> Result<String, TextError> {
    let lower = value.trim().to_ascii_lowercase();
    let rgba = if let Some(hex) = lower.strip_prefix('#') {
        match hex.len() {
            3 => [
                nibble_pair(hex, 0, offset)?,
                nibble_pair(hex, 1, offset)?,
                nibble_pair(hex, 2, offset)?,
                255,
            ],
            4 => [
                nibble_pair(hex, 0, offset)?,
                nibble_pair(hex, 1, offset)?,
                nibble_pair(hex, 2, offset)?,
                nibble_pair(hex, 3, offset)?,
            ],
            6 | 8 => {
                let byte = |index| {
                    u8::from_str_radix(&hex[index..index + 2], 16).map_err(|_| {
                        markup_error(offset, &format!("invalid {attribute} color {value:?}"))
                    })
                };
                [
                    byte(0)?,
                    byte(2)?,
                    byte(4)?,
                    if hex.len() == 8 { byte(6)? } else { 255 },
                ]
            }
            _ => {
                return Err(markup_error(
                    offset,
                    &format!("invalid {attribute} color {value:?}"),
                ));
            }
        }
    } else {
        named_color(&lower).ok_or_else(|| {
            markup_error(
                offset,
                &format!("unsupported named {attribute} color {value:?}; use #rrggbb or #rrggbbaa"),
            )
        })?
    };
    Ok(format!(
        "#{:02x}{:02x}{:02x}{:02x}",
        rgba[0], rgba[1], rgba[2], rgba[3]
    ))
}

fn nibble_pair(hex: &str, index: usize, offset: usize) -> Result<u8, TextError> {
    let value =
        hex.as_bytes()
            .get(index)
            .copied()
            .and_then(|value| (value as char).to_digit(16))
            .ok_or_else(|| markup_error(offset, "invalid hexadecimal color digit"))? as u8;
    Ok(value * 17)
}

fn named_color(name: &str) -> Option<[u8; 4]> {
    let rgb = match name {
        "aliceblue" => 0xf0f8ff,
        "black" => 0x000000,
        "blue" => 0x0000ff,
        "cyan" | "aqua" => 0x00ffff,
        "darkgray" | "darkgrey" => 0xa9a9a9,
        "fuchsia" | "magenta" => 0xff00ff,
        "gray" | "grey" => 0x808080,
        "green" => 0x008000,
        "lightgray" | "lightgrey" => 0xd3d3d3,
        "lime" => 0x00ff00,
        "maroon" => 0x800000,
        "navy" => 0x000080,
        "olive" => 0x808000,
        "orange" => 0xffa500,
        "purple" => 0x800080,
        "red" => 0xff0000,
        "silver" => 0xc0c0c0,
        "teal" => 0x008080,
        "transparent" => return Some([0, 0, 0, 0]),
        "violet" => 0xee82ee,
        "white" => 0xffffff,
        "yellow" => 0xffff00,
        _ => return None,
    };
    Some([
        ((rgb >> 16) & 0xff) as u8,
        ((rgb >> 8) & 0xff) as u8,
        (rgb & 0xff) as u8,
        255,
    ])
}

fn decode_entities(raw: &str, source_offset: usize) -> Result<String, TextError> {
    if !raw.contains('&') {
        return Ok(raw.to_owned());
    }
    let mut output = String::with_capacity(raw.len());
    let mut cursor = 0usize;
    while cursor < raw.len() {
        let Some(relative) = raw[cursor..].find('&') else {
            output.push_str(&raw[cursor..]);
            break;
        };
        let start = cursor + relative;
        output.push_str(&raw[cursor..start]);
        let Some(end_relative) = raw[start + 1..].find(';') else {
            return Err(markup_error(
                source_offset + start,
                "unterminated character entity",
            ));
        };
        let end = start + 1 + end_relative;
        let entity = &raw[start + 1..end];
        let character = match entity {
            "amp" => '&',
            "lt" => '<',
            "gt" => '>',
            "quot" => '"',
            "apos" => '\'',
            _ if entity.starts_with("#x") || entity.starts_with("#X") => {
                decode_numeric_entity(&entity[2..], 16, source_offset + start)?
            }
            _ if entity.starts_with('#') => {
                decode_numeric_entity(&entity[1..], 10, source_offset + start)?
            }
            _ => {
                return Err(markup_error(
                    source_offset + start,
                    &format!("unknown character entity &{entity};"),
                ));
            }
        };
        output.push(character);
        cursor = end + 1;
    }
    Ok(output)
}

fn decode_numeric_entity(digits: &str, radix: u32, offset: usize) -> Result<char, TextError> {
    if digits.is_empty() {
        return Err(markup_error(offset, "empty numeric character entity"));
    }
    let value = u32::from_str_radix(digits, radix)
        .map_err(|_| markup_error(offset, "invalid numeric character entity"))?;
    char::from_u32(value)
        .filter(|character| !matches!(*character as u32, 0 | 0xD800..=0xDFFF))
        .ok_or_else(|| markup_error(offset, "numeric character entity is not valid Unicode"))
}

fn with_weight(variant: FontVariant, bold: bool) -> FontVariant {
    match (
        bold,
        matches!(variant, FontVariant::Italic | FontVariant::BoldItalic),
    ) {
        (false, false) => FontVariant::Regular,
        (false, true) => FontVariant::Italic,
        (true, false) => FontVariant::Bold,
        (true, true) => FontVariant::BoldItalic,
    }
}

fn with_italic(variant: FontVariant, italic: bool) -> FontVariant {
    match (
        matches!(variant, FontVariant::Bold | FontVariant::BoldItalic),
        italic,
    ) {
        (false, false) => FontVariant::Regular,
        (false, true) => FontVariant::Italic,
        (true, false) => FontVariant::Bold,
        (true, true) => FontVariant::BoldItalic,
    }
}

fn markup_error(offset: usize, message: &str) -> TextError {
    let mut detail = String::new();
    let _ = write!(detail, "Pango markup error at byte {offset}: {message}.");
    TextError(detail)
}

#[cfg(test)]
mod tests {
    use super::{PangoUnderline, parse_pango_markup};
    use crate::FontVariant;

    #[test]
    fn parses_nested_inherited_styles_and_entities() {
        let spans = parse_pango_markup(
            "plain <span foreground='AliceBlue' background=\"#1234\"><b>A&amp;<i>B</i></b></span> &#169;",
        )
        .unwrap();
        assert_eq!(
            spans
                .iter()
                .map(|span| span.text.as_str())
                .collect::<String>(),
            "plain A&B ©"
        );
        assert_eq!(spans[1].style.foreground.as_deref(), Some("#f0f8ffff"));
        assert_eq!(spans[1].style.background.as_deref(), Some("#11223344"));
        assert_eq!(spans[1].style.variant, FontVariant::Bold);
        assert_eq!(spans[2].style.variant, FontVariant::BoldItalic);
    }

    #[test]
    fn parses_metrics_and_decorations() {
        let spans = parse_pango_markup(
            "<span size='200%' rise='4.8pt' letter_spacing='1024' underline='double' underline_color='green' strikethrough='true' strikethrough_color='#f00'>x</span>",
        )
        .unwrap();
        let style = &spans[0].style;
        assert_eq!(style.font_scale, 2.0);
        assert!((style.rise - 0.1).abs() < 1e-6);
        assert!((style.letter_spacing - 1.0 / 48.0).abs() < 1e-6);
        assert_eq!(style.underline, PangoUnderline::Double);
        assert_eq!(style.underline_color.as_deref(), Some("#008000ff"));
        assert!(style.strikethrough);
        assert_eq!(style.strikethrough_color.as_deref(), Some("#ff0000ff"));
    }

    #[test]
    fn preserves_paint_boundaries_without_flattening_joining_text() {
        let spans =
            parse_pango_markup("<span foreground='red'>س</span><span foreground='blue'>لام</span>")
                .unwrap();
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].text, "س");
        assert_eq!(spans[1].text, "لام");
        assert_eq!(spans[0].style.variant, spans[1].style.variant);
        assert_eq!(spans[0].style.font_scale, spans[1].style.font_scale);
    }

    #[test]
    fn root_markup_is_optional_but_strict() {
        assert_eq!(
            parse_pango_markup("<markup><b>ok</b></markup> \n").unwrap()[0].text,
            "ok"
        );
        assert!(parse_pango_markup("<markup>x</markup>tail").is_err());
        assert!(parse_pango_markup("head<markup>x</markup>").is_err());
    }

    #[test]
    fn nested_relative_sizes_compose_with_inherited_size() {
        let spans = parse_pango_markup(
            "<big><span size='200%'>a</span><span size='smaller'>b</span></big>",
        )
        .unwrap();
        assert!((spans[0].style.font_scale - 2.4).abs() < 1e-6);
        assert!((spans[1].style.font_scale - 1.0).abs() < 1e-6);
    }

    #[test]
    fn rejects_malformed_or_unsupported_markup_exactly() {
        let cases = [
            ("<b>x</i>", "expected </b> before </i>"),
            ("<blink>x</blink>", "unsupported Pango element <blink>"),
            (
                "<span gravity='east'>x</span>",
                "unsupported Pango <span> attribute gravity",
            ),
            (
                "<span foreground='not-a-color'>x</span>",
                "unsupported named foreground color",
            ),
            (
                "<tt>x</tt>",
                "requires a registered portable monospace face",
            ),
            ("&bogus;", "unknown character entity &bogus;"),
            ("<!-- nope -->", "comments"),
        ];
        for (markup, expected) in cases {
            let error = parse_pango_markup(markup).unwrap_err().to_string();
            assert!(error.contains(expected), "{markup:?}: {error}");
            assert!(error.starts_with("Pango markup error at byte "));
        }
    }
}
