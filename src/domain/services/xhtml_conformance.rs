use super::xhtml_tag::{TagAttribute, parse_attributes, rewrite_start_tags, start_tags};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fmt;

const STRUCTURAL_ATTRIBUTES: [&str; 4] = ["id", "class", "href", "src"];
const IMAGE_DIMENSIONS: [&str; 2] = ["width", "height"];
const IMAGE_TAG: &str = "img";
const ID_ATTRIBUTE: &str = "id";
const STYLE_ATTRIBUTE: &str = "style";

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
        let mut seen_ids: HashSet<Cow<'_, str>> = HashSet::new();
        rewrite_start_tags(xml, |tag, attributes| {
            let is_image = tag.name == IMAGE_TAG;
            let mut changed = false;
            let mut kept = Vec::with_capacity(attributes.len());
            let mut declarations = Vec::new();
            for attribute in attributes {
                if attribute.is_named(ID_ATTRIBUTE) && !seen_ids.insert(attribute.value.clone()) {
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

    fn is_decimal(value: &str) -> bool {
        let mut dots = 0;
        let all_numeric = value.bytes().all(|b| match b {
            b'.' => {
                dots += 1;
                true
            }
            _ => b.is_ascii_digit(),
        });
        all_numeric && dots <= 1 && value.bytes().any(|b| b.is_ascii_digit())
    }

    fn css_declaration(attribute: &TagAttribute<'_>) -> Option<(String, String)> {
        let value = attribute.value.trim();
        let property = attribute.name.to_ascii_lowercase();
        if Self::is_decimal(value) {
            return Some((property, format!("{value}px")));
        }
        let starts_numeric = value
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_digit() || b == b'.');
        starts_numeric.then(|| (property, value.to_string()))
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
