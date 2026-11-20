use super::xhtml_common::{extract_tag_name, find_tag_closing_bracket, is_valid_xml_entity};
use std::borrow::Cow;

const SKIPPED_MARKUP: [(&str, &str); 5] = [
    ("<!--", "-->"),
    ("<![CDATA[", "]]>"),
    ("<?", "?>"),
    ("<!", ">"),
    ("</", ">"),
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartTag<'a> {
    pub start: usize,
    pub end: usize,
    pub raw: &'a str,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagAttribute<'a> {
    pub name: &'a str,
    pub value: Cow<'a, str>,
}

impl<'a> TagAttribute<'a> {
    pub fn new(name: &'a str, value: impl Into<Cow<'a, str>>) -> Self {
        Self {
            name,
            value: value.into(),
        }
    }

    pub fn is_named(&self, name: &str) -> bool {
        self.name.eq_ignore_ascii_case(name)
    }
}

pub struct StartTagIter<'a> {
    xml: &'a str,
    pos: usize,
}

impl<'a> Iterator for StartTagIter<'a> {
    type Item = StartTag<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let start = self.pos + self.xml.get(self.pos..)?.find('<')?;
            let rest = self.xml.get(start..)?;
            if let Some(skip) = skipped_markup_len(rest) {
                self.pos = start + skip;
                continue;
            }
            let Some(close) = find_tag_closing_bracket(rest) else {
                self.pos = self.xml.len();
                return None;
            };
            let end = start + close + 1;
            self.pos = end;
            let raw = self.xml.get(start..end)?;
            let name = extract_tag_name(raw);
            if !name.is_empty() {
                return Some(StartTag {
                    start,
                    end,
                    raw,
                    name,
                });
            }
        }
    }
}

fn skipped_markup_len(rest: &str) -> Option<usize> {
    SKIPPED_MARKUP
        .iter()
        .find(|(open, _)| rest.starts_with(open))
        .map(|(_, close)| rest.find(close).map_or(rest.len(), |idx| idx + close.len()))
}

pub fn start_tags(xml: &str) -> StartTagIter<'_> {
    StartTagIter { xml, pos: 0 }
}

fn is_name_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, ':' | '_' | '-')
}

fn split_tag(raw: &str) -> (&str, &str, bool) {
    let body = raw.trim_start().strip_prefix('<').unwrap_or(raw);
    let name_len = body.find(|c: char| !is_name_char(c)).unwrap_or(body.len());
    let (name, after_name) = body.split_at(name_len);
    let inner = after_name
        .strip_suffix('>')
        .unwrap_or(after_name)
        .trim_end();
    match inner.strip_suffix('/') {
        Some(attrs) => (name, attrs, true),
        None => (name, inner, false),
    }
}

fn read_value(input: &str) -> (&str, usize) {
    match input.chars().next() {
        Some(quote @ ('"' | '\'')) => {
            let body = &input[1..];
            match body.find(quote) {
                Some(idx) => (&body[..idx], idx + 2),
                None => (body, input.len()),
            }
        }
        _ => {
            let end = input.find(char::is_whitespace).unwrap_or(input.len());
            (&input[..end], end)
        }
    }
}

pub fn parse_attributes(raw: &str) -> Vec<TagAttribute<'_>> {
    let (_, mut rest, _) = split_tag(raw);
    let mut attributes = Vec::new();
    loop {
        rest = rest.trim_start_matches(|c: char| c.is_whitespace() || c == '/');
        if rest.is_empty() {
            return attributes;
        }
        let name_end = rest
            .find(|c: char| c.is_whitespace() || c == '=')
            .unwrap_or(rest.len());
        let name = &rest[..name_end];
        rest = rest[name_end..].trim_start();
        let Some(after_eq) = rest.strip_prefix('=') else {
            attributes.push(TagAttribute::new(name, ""));
            continue;
        };
        let value_input = after_eq.trim_start();
        let (value, consumed) = read_value(value_input);
        rest = &value_input[consumed..];
        if !name.is_empty() {
            attributes.push(TagAttribute::new(name, value));
        }
    }
}

fn push_escaped_attribute_value(value: &str, quote: char, out: &mut String) {
    let mut last = 0;
    for (idx, ch) in value.char_indices() {
        let escaped = match ch {
            '<' => Some("&lt;"),
            '>' => Some("&gt;"),
            '"' if quote == '"' => Some("&quot;"),
            '\'' if quote == '\'' => Some("&apos;"),
            '&' if !is_valid_xml_entity(value.get(idx..).unwrap_or("")) => Some("&amp;"),
            _ => None,
        };
        if let Some(replacement) = escaped {
            if let Some(chunk) = value.get(last..idx) {
                out.push_str(chunk);
            }
            out.push_str(replacement);
            last = idx + ch.len_utf8();
        }
    }
    if let Some(tail) = value.get(last..) {
        out.push_str(tail);
    }
}

pub fn render_tag(raw: &str, attributes: &[TagAttribute<'_>]) -> String {
    let (name, _, self_closing) = split_tag(raw);
    let mut out = String::with_capacity(raw.len() + 32);
    out.push('<');
    out.push_str(name);
    for attribute in attributes {
        let quote = if attribute.value.contains('"') && !attribute.value.contains('\'') {
            '\''
        } else {
            '"'
        };
        out.push(' ');
        out.push_str(attribute.name);
        out.push('=');
        out.push(quote);
        push_escaped_attribute_value(&attribute.value, quote, &mut out);
        out.push(quote);
    }
    out.push_str(if self_closing { " />" } else { ">" });
    out
}

pub fn rewrite_start_tags<'a, F>(xml: &'a str, mut transform: F) -> String
where
    F: FnMut(&StartTag<'a>, Vec<TagAttribute<'a>>) -> Option<Vec<TagAttribute<'a>>>,
{
    let mut out = String::with_capacity(xml.len() + 64);
    let mut last = 0;
    for tag in start_tags(xml) {
        let attributes = parse_attributes(tag.raw);
        if let Some(rewritten) = transform(&tag, attributes) {
            if let Some(chunk) = xml.get(last..tag.start) {
                out.push_str(chunk);
            }
            out.push_str(&render_tag(tag.raw, &rewritten));
            last = tag.end;
        }
    }
    if let Some(tail) = xml.get(last..) {
        out.push_str(tail);
    }
    out
}
