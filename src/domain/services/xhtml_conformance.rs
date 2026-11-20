use super::xhtml_common::{extract_tag_name, find_tag_closing_bracket};
use super::xhtml_tag::{TagAttribute, parse_attributes, rewrite_start_tags, start_tags};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fmt;

const STRUCTURAL_ATTRIBUTES: [&str; 4] = ["id", "class", "href", "src"];
const IMAGE_DIMENSIONS: [&str; 2] = ["width", "height"];
const IMAGE_TAG: &str = "img";
const ID_ATTRIBUTE: &str = "id";
const STYLE_ATTRIBUTE: &str = "style";
const ACTIVE_TAGS: [&str; 4] = ["script", "iframe", "object", "embed"];
const URI_ATTRIBUTES: [&str; 5] = ["href", "src", "action", "xlink:href", "formaction"];
const ALLOWED_CSS_UNITS: [&str; 10] = ["px", "em", "rem", "%", "pt", "cm", "mm", "in", "vw", "vh"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConformanceIssue {
    DuplicateId(String),
    NonIntegerImageDimension { attribute: String, value: String },
}

impl fmt::Display for ConformanceIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateId(id) => write!(f, "duplicate id '{id}'"),
            Self::NonIntegerImageDimension { attribute, value } => {
                write!(
                    f,
                    "<img> attribute {attribute}=\"{value}\" is not a non-negative integer"
                )
            }
        }
    }
}

pub struct XhtmlConformance;

impl XhtmlConformance {
    pub fn audit(xml: &str) -> Vec<ConformanceIssue> {
        let mut issues = Vec::new();
        let mut seen_ids = HashSet::new();
        let mut reported_ids = HashSet::new();
        for tag in start_tags(xml) {
            let is_image = tag.name == IMAGE_TAG;
            for attribute in parse_attributes(tag.raw) {
                if attribute.is_named(ID_ATTRIBUTE)
                    && !seen_ids.insert(attribute.value.clone())
                    && reported_ids.insert(attribute.value.clone())
                {
                    issues.push(ConformanceIssue::DuplicateId(attribute.value.into_owned()));
                } else if is_image && Self::is_invalid_dimension(&attribute) {
                    issues.push(ConformanceIssue::NonIntegerImageDimension {
                        attribute: attribute.name.to_string(),
                        value: attribute.value.into_owned(),
                    });
                }
            }
        }
        issues
    }

    pub fn sanitize(xml: &str) -> String {
        let stripped = Self::strip_active_elements(xml);
        let mut seen_ids: HashSet<Cow<'_, str>> = HashSet::new();
        rewrite_start_tags(&stripped, |tag, attributes| {
            let is_image = tag.name == IMAGE_TAG;
            let mut changed = false;
            let mut kept = Vec::with_capacity(attributes.len());
            let mut declarations = Vec::new();
            for attribute in attributes {
                if Self::should_drop_attribute(&attribute, &mut seen_ids) {
                    changed = true;
                } else if is_image && Self::is_invalid_dimension(&attribute) {
                    changed = true;
                    declarations.extend(Self::css_declaration(&attribute));
                } else {
                    kept.push(attribute);
                }
            }
            changed.then(|| Self::merge_style(kept, &declarations))
        })
    }

    pub fn restore_structural_attributes<'a>(original: &'a str, translated: &'a str) -> String {
        let mut expected_by_tag: HashMap<String, Vec<Vec<TagAttribute<'a>>>> = HashMap::new();
        for tag in start_tags(original) {
            let structural = parse_attributes(tag.raw)
                .into_iter()
                .filter(Self::is_structural)
                .collect();
            expected_by_tag
                .entry(tag.name)
                .or_default()
                .push(structural);
        }
        let mut translated_counts: HashMap<String, usize> = HashMap::new();
        for tag in start_tags(translated) {
            *translated_counts.entry(tag.name).or_default() += 1;
        }
        let mut cursors: HashMap<String, usize> = HashMap::new();
        rewrite_start_tags(translated, |tag, attributes| {
            let expected_list = expected_by_tag.get(&tag.name)?;
            if translated_counts.get(&tag.name) != Some(&expected_list.len()) {
                return None;
            }
            let cursor = cursors.entry(tag.name.clone()).or_default();
            let expected = expected_list.get(*cursor)?;
            *cursor += 1;
            Self::merge_structural(attributes, expected)
        })
    }

    pub fn enforce(original: &str, translated: &str) -> String {
        Self::sanitize(&Self::restore_structural_attributes(original, translated))
    }

    fn local_tag_name(name: &str) -> &str {
        name.rsplit(':').next().unwrap_or(name)
    }

    fn is_active_tag(name: &str) -> bool {
        let local = Self::local_tag_name(name);
        ACTIVE_TAGS.iter().any(|t| local.eq_ignore_ascii_case(t))
    }

    fn strip_active_elements(xml: &str) -> Cow<'_, str> {
        let mut out: Option<String> = None;
        let mut last_copied = 0usize;
        let mut pos = 0usize;

        while let Some(rel_open) = xml.get(pos..).and_then(|s| s.find('<')) {
            let next_open = pos + rel_open;
            let rest = xml.get(next_open..).unwrap_or("");

            if rest.starts_with("<!--") {
                pos = rest
                    .find("-->")
                    .map_or(xml.len(), |idx| next_open + idx + 3);
                continue;
            }
            if rest.starts_with("<![CDATA[") {
                pos = rest
                    .find("]]>")
                    .map_or(xml.len(), |idx| next_open + idx + 3);
                continue;
            }
            if rest.starts_with("<?") {
                pos = rest.find("?>").map_or(xml.len(), |idx| next_open + idx + 2);
                continue;
            }
            if rest.starts_with("<!") {
                pos = find_tag_closing_bracket(rest).map_or(xml.len(), |idx| next_open + idx + 1);
                continue;
            }
            if rest.starts_with("</") {
                let tag_name = extract_tag_name(rest);
                let end =
                    find_tag_closing_bracket(rest).map_or(xml.len(), |idx| next_open + idx + 1);
                if Self::is_active_tag(&tag_name) {
                    let buf = out.get_or_insert_with(|| String::with_capacity(xml.len()));
                    if let Some(chunk) = xml.get(last_copied..next_open) {
                        buf.push_str(chunk);
                    }
                    last_copied = end;
                }
                pos = end;
                continue;
            }

            let tag_name = extract_tag_name(rest);
            let open_end =
                find_tag_closing_bracket(rest).map_or(xml.len(), |idx| next_open + idx + 1);
            if !Self::is_active_tag(&tag_name) {
                pos = open_end;
                continue;
            }

            let buf = out.get_or_insert_with(|| String::with_capacity(xml.len()));
            if let Some(chunk) = xml.get(last_copied..next_open) {
                buf.push_str(chunk);
            }

            let open_tag = xml.get(next_open..open_end).unwrap_or("");
            let is_self_closing = open_tag.trim_end().ends_with("/>");
            let local = Self::local_tag_name(&tag_name);

            let skip_end = if is_self_closing || local.eq_ignore_ascii_case("embed") {
                open_end
            } else if local.eq_ignore_ascii_case("script") {
                Self::find_script_close(xml, open_end).unwrap_or(xml.len())
            } else {
                Self::find_paired_active_close(xml, open_end, local).unwrap_or(open_end)
            };

            last_copied = skip_end;
            pos = skip_end;
        }

        match out {
            Some(mut buf) => {
                if let Some(tail) = xml.get(last_copied..) {
                    buf.push_str(tail);
                }
                Cow::Owned(buf)
            }
            None => Cow::Borrowed(xml),
        }
    }

    fn find_script_close(xml: &str, start: usize) -> Option<usize> {
        let mut pos = start;
        while let Some(rel_open) = xml.get(pos..).and_then(|s| s.find('<')) {
            let next_open = pos + rel_open;
            let rest = xml.get(next_open..).unwrap_or("");
            if rest.starts_with("</") {
                let close_name = extract_tag_name(rest);
                let close_end =
                    find_tag_closing_bracket(rest).map_or(xml.len(), |idx| next_open + idx + 1);
                if Self::local_tag_name(&close_name).eq_ignore_ascii_case("script") {
                    return Some(close_end);
                }
                pos = close_end;
            } else {
                pos = next_open + 1;
            }
        }
        None
    }

    fn find_paired_active_close(xml: &str, start: usize, target_local: &str) -> Option<usize> {
        let mut depth = 1usize;
        let mut pos = start;
        while depth > 0 {
            let rel_open = xml.get(pos..).and_then(|s| s.find('<'))?;
            let next_open = pos + rel_open;
            let rest = xml.get(next_open..).unwrap_or("");
            if rest.starts_with("<!--") {
                pos = rest
                    .find("-->")
                    .map_or(xml.len(), |idx| next_open + idx + 3);
                continue;
            }
            if rest.starts_with("<![CDATA[") {
                pos = rest
                    .find("]]>")
                    .map_or(xml.len(), |idx| next_open + idx + 3);
                continue;
            }
            if rest.starts_with("<?") {
                pos = rest.find("?>").map_or(xml.len(), |idx| next_open + idx + 2);
                continue;
            }
            if rest.starts_with("<!") {
                pos = find_tag_closing_bracket(rest).map_or(xml.len(), |idx| next_open + idx + 1);
                continue;
            }
            if rest.starts_with("</") {
                let close_name = extract_tag_name(rest);
                let close_end =
                    find_tag_closing_bracket(rest).map_or(xml.len(), |idx| next_open + idx + 1);
                if Self::local_tag_name(&close_name).eq_ignore_ascii_case(target_local) {
                    depth -= 1;
                    if depth == 0 {
                        return Some(close_end);
                    }
                }
                pos = close_end;
                continue;
            }
            let open_name = extract_tag_name(rest);
            let open_end =
                find_tag_closing_bracket(rest).map_or(xml.len(), |idx| next_open + idx + 1);
            let open_slice = xml.get(next_open..open_end).unwrap_or("");
            let is_self_closing = open_slice.trim_end().ends_with("/>");
            if !is_self_closing
                && Self::local_tag_name(&open_name).eq_ignore_ascii_case(target_local)
            {
                depth += 1;
            }
            pos = open_end;
        }
        None
    }

    fn should_drop_attribute<'a>(
        attribute: &TagAttribute<'a>,
        seen_ids: &mut HashSet<Cow<'a, str>>,
    ) -> bool {
        Self::is_event_handler_attribute(attribute.name)
            || (Self::is_uri_attribute(attribute.name) && Self::is_dangerous_uri(&attribute.value))
            || (attribute.is_named(STYLE_ATTRIBUTE) && Self::is_dangerous_style(&attribute.value))
            || (attribute.is_named(ID_ATTRIBUTE) && !seen_ids.insert(attribute.value.clone()))
    }

    fn is_event_handler_attribute(name: &str) -> bool {
        let local = Self::local_tag_name(name).trim();
        local.len() > 2
            && local
                .get(..2)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("on"))
    }

    fn is_uri_attribute(name: &str) -> bool {
        let local = Self::local_tag_name(name);
        URI_ATTRIBUTES.iter().any(|candidate| {
            name.eq_ignore_ascii_case(candidate) || local.eq_ignore_ascii_case(candidate)
        })
    }

    fn decode_entity_body(body: &str) -> Option<char> {
        if let Some(num) = body.strip_prefix('#') {
            if let Some(hex) = num.strip_prefix('x').or_else(|| num.strip_prefix('X')) {
                if !hex.is_empty() && hex.chars().all(|c| c.is_ascii_hexdigit()) {
                    return u32::from_str_radix(hex, 16).ok().and_then(char::from_u32);
                }
            } else if !num.is_empty() && num.chars().all(|c| c.is_ascii_digit()) {
                return num.parse::<u32>().ok().and_then(char::from_u32);
            }
            return None;
        }
        match body.to_ascii_lowercase().as_str() {
            "colon" => Some(':'),
            "tab" => Some('\t'),
            "newline" => Some('\n'),
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "sol" => Some('/'),
            "semi" => Some(';'),
            _ => None,
        }
    }

    fn decode_xml_entities(input: &str) -> String {
        let mut decoded = String::with_capacity(input.len());
        let mut pos = 0usize;
        while let Some(rel_amp) = input.get(pos..).and_then(|s| s.find('&')) {
            let amp_idx = pos + rel_amp;
            if let Some(chunk) = input.get(pos..amp_idx) {
                decoded.push_str(chunk);
            }
            let after_amp = input.get(amp_idx + 1..).unwrap_or("");
            if let Some(semi_rel) = after_amp.find(';') {
                let entity_body = after_amp.get(..semi_rel).unwrap_or("");
                if let Some(ch) = Self::decode_entity_body(entity_body) {
                    decoded.push(ch);
                    pos = amp_idx + 1 + semi_rel + 1;
                    continue;
                }
            }
            decoded.push('&');
            pos = amp_idx + 1;
        }
        if let Some(tail) = input.get(pos..) {
            decoded.push_str(tail);
        }
        decoded
    }

    fn is_dangerous_uri(value: &str) -> bool {
        let decoded = Self::decode_xml_entities(value);
        let normalized: String = decoded
            .chars()
            .filter(|ch| !ch.is_control() && !ch.is_whitespace())
            .map(|ch| ch.to_ascii_lowercase())
            .collect();
        if normalized.starts_with("javascript:") || normalized.starts_with("vbscript:") {
            return true;
        }
        normalized.strip_prefix("data:").is_some_and(|rest| {
            rest.starts_with("text/html")
                || rest.starts_with("application/xhtml+xml")
                || rest.starts_with("image/svg+xml")
        })
    }

    fn strip_css_comments(input: &str) -> String {
        let mut out = String::with_capacity(input.len());
        let mut pos = 0usize;
        while let Some(rel_start) = input.get(pos..).and_then(|s| s.find("/*")) {
            let start_idx = pos + rel_start;
            if let Some(chunk) = input.get(pos..start_idx) {
                out.push_str(chunk);
            }
            pos = input
                .get(start_idx + 2..)
                .and_then(|s| s.find("*/"))
                .map_or(input.len(), |rel_end| start_idx + 2 + rel_end + 2);
        }
        if let Some(tail) = input.get(pos..) {
            out.push_str(tail);
        }
        out
    }

    fn is_dangerous_style(value: &str) -> bool {
        let decoded = Self::decode_xml_entities(value);
        let without_comments = Self::strip_css_comments(&decoded);
        let normalized: String = without_comments
            .chars()
            .filter(|ch| !ch.is_control() && !ch.is_whitespace())
            .map(|ch| ch.to_ascii_lowercase())
            .collect();
        normalized.contains("javascript:")
            || normalized.contains("vbscript:")
            || normalized.contains("data:text/html")
            || normalized.contains("data:application/xhtml+xml")
            || normalized.contains("expression(")
    }

    fn is_structural(attribute: &TagAttribute<'_>) -> bool {
        STRUCTURAL_ATTRIBUTES
            .iter()
            .any(|name| attribute.is_named(name))
    }

    fn is_invalid_dimension(attribute: &TagAttribute<'_>) -> bool {
        IMAGE_DIMENSIONS.iter().any(|name| attribute.is_named(name))
            && !Self::is_non_negative_integer(&attribute.value)
    }

    fn is_non_negative_integer(value: &str) -> bool {
        let trimmed = value.trim();
        !trimmed.is_empty() && trimmed.bytes().all(|b| b.is_ascii_digit())
    }

    fn parse_strict_css_dimension(value: &str) -> Option<(&str, Option<&str>)> {
        let int_len = value.bytes().take_while(u8::is_ascii_digit).count();
        if int_len == 0 {
            return None;
        }
        let after_int = value.get(int_len..)?;
        let num_len = if let Some(after_dot) = after_int.strip_prefix('.') {
            let frac_len = after_dot.bytes().take_while(u8::is_ascii_digit).count();
            if frac_len == 0 {
                return None;
            }
            int_len + 1 + frac_len
        } else {
            int_len
        };
        let (number, unit) = value.split_at(num_len);
        if unit.is_empty() {
            Some((number, None))
        } else if ALLOWED_CSS_UNITS.contains(&unit) {
            Some((number, Some(unit)))
        } else {
            None
        }
    }

    fn css_declaration(attribute: &TagAttribute<'_>) -> Option<(String, String)> {
        let value = attribute.value.trim();
        let property = attribute.name.to_ascii_lowercase();
        let (number, unit) = Self::parse_strict_css_dimension(value)?;
        let css_value = match unit {
            Some(u) => format!("{number}{u}"),
            None => format!("{number}px"),
        };
        Some((property, css_value))
    }

    fn declares_property(style: &str, property: &str) -> bool {
        style
            .split(';')
            .filter_map(|declaration| declaration.split_once(':'))
            .any(|(name, _)| name.trim().eq_ignore_ascii_case(property))
    }

    fn merge_style<'a>(
        mut attributes: Vec<TagAttribute<'a>>,
        declarations: &[(String, String)],
    ) -> Vec<TagAttribute<'a>> {
        let style_index = attributes.iter().position(|a| a.is_named(STYLE_ATTRIBUTE));
        let existing = style_index
            .and_then(|idx| attributes.get(idx))
            .map(|a| a.value.trim().trim_end_matches(';').trim().to_string())
            .unwrap_or_default();
        let additions: Vec<String> = declarations
            .iter()
            .filter(|(property, _)| !Self::declares_property(&existing, property))
            .map(|(property, value)| format!("{property}: {value}"))
            .collect();
        if additions.is_empty() {
            return attributes;
        }
        let merged = if existing.is_empty() {
            additions.join("; ")
        } else {
            format!("{existing}; {}", additions.join("; "))
        };
        match style_index.and_then(|idx| attributes.get_mut(idx)) {
            Some(style) => style.value = Cow::Owned(merged),
            None => attributes.push(TagAttribute::new(STYLE_ATTRIBUTE, merged)),
        }
        attributes
    }

    fn structural_value<'b>(attributes: &'b [TagAttribute<'_>], name: &str) -> Option<&'b str> {
        attributes
            .iter()
            .find(|a| a.is_named(name))
            .map(|a| a.value.as_ref())
    }

    fn merge_structural<'a>(
        attributes: Vec<TagAttribute<'a>>,
        expected: &[TagAttribute<'a>],
    ) -> Option<Vec<TagAttribute<'a>>> {
        let unchanged = STRUCTURAL_ATTRIBUTES.iter().all(|name| {
            Self::structural_value(&attributes, name) == Self::structural_value(expected, name)
        });
        if unchanged {
            return None;
        }
        let mut merged: Vec<TagAttribute<'a>> = attributes
            .into_iter()
            .filter_map(|attribute| {
                if !Self::is_structural(&attribute) {
                    return Some(attribute);
                }
                expected
                    .iter()
                    .find(|e| e.is_named(attribute.name))
                    .map(|e| TagAttribute::new(attribute.name, e.value.clone()))
            })
            .collect();
        for original in expected {
            if !merged.iter().any(|a| a.is_named(original.name)) {
                merged.push(original.clone());
            }
        }
        Some(merged)
    }
}
