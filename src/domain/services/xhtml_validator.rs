use super::xhtml_common::{extract_tag_name, is_valid_xml_entity, is_void_tag};
use super::xhtml_conformance::XhtmlConformance;
use quick_xml::Reader;
use quick_xml::events::Event;
use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum XhtmlValidationError {
    #[error("XHTML output is empty or whitespace only")]
    EmptyOutput,

    #[error("Malformed XML: {0}")]
    MalformedXml(String),

    #[error("Tag mismatch: expected </{expected}>, found </{found}>")]
    TagMismatch { expected: String, found: String },

    #[error("Unexpected end tag </{0}>")]
    UnexpectedEndTag(String),

    #[error("Unclosed tags at EOF: {0}")]
    UnclosedTags(String),
}

pub struct XhtmlValidator;

impl XhtmlValidator {
    pub fn validate(xml: &str) -> Result<(), XhtmlValidationError> {
        let trimmed = xml.trim();
        if trimmed.is_empty() {
            return Err(XhtmlValidationError::EmptyOutput);
        }

        let mut reader = Reader::from_str(trimmed);
        reader.config_mut().check_end_names = true;

        let mut buf = Vec::new();
        let mut tag_stack: Vec<String> = Vec::new();

        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(e)) => {
                    let name = e.name().as_ref().to_string();
                    tag_stack.push(name);
                }
                Ok(Event::End(e)) => {
                    let name = e.name().as_ref().to_string();
                    if let Some(top) = tag_stack.pop() {
                        if !top.eq_ignore_ascii_case(&name) {
                            return Err(XhtmlValidationError::TagMismatch {
                                expected: top,
                                found: name,
                            });
                        }
                    } else {
                        return Err(XhtmlValidationError::UnexpectedEndTag(name));
                    }
                }
                Ok(Event::Empty(_)) => {}
                Ok(Event::Text(t)) => {
                    let text = t.as_ref();
                    let mut idx = 0;
                    while let Some(amp_pos) = text[idx..].find('&') {
                        let actual_pos = idx + amp_pos;
                        if !is_valid_xml_entity(&text[actual_pos..]) {
                            return Err(XhtmlValidationError::MalformedXml(
                                "Unescaped '&' found in XML text content".to_string(),
                            ));
                        }
                        idx = actual_pos + 1;
                    }
                }
                Ok(
                    Event::Comment(_)
                    | Event::CData(_)
                    | Event::Decl(_)
                    | Event::DocType(_)
                    | Event::PI(_)
                    | Event::GeneralRef(_),
                ) => {}
                Ok(Event::Eof) => break,
                Err(e) => return Err(XhtmlValidationError::MalformedXml(e.to_string())),
            }
            buf.clear();
        }

        if !tag_stack.is_empty() {
            return Err(XhtmlValidationError::UnclosedTags(tag_stack.join(", ")));
        }

        Ok(())
    }

    pub fn heal(xml: &str) -> String {
        let trimmed = xml.trim();
        if trimmed.is_empty() {
            return String::new();
        }

        let quote_healed = Self::heal_attribute_quotes(trimmed);
        let entity_healed = Self::heal_naked_ampersands(&quote_healed);
        XhtmlConformance::sanitize(&Self::heal_tags(&entity_healed))
    }

    pub fn heal_and_validate(xml: &str) -> Result<String, XhtmlValidationError> {
        let trimmed = xml.trim();
        if trimmed.is_empty() {
            return Err(XhtmlValidationError::EmptyOutput);
        }

        if Self::validate(trimmed).is_ok() {
            return Ok(trimmed.to_string());
        }

        let healed = Self::heal(trimmed);
        if healed.trim().is_empty() {
            return Err(XhtmlValidationError::EmptyOutput);
        }

        Self::validate(&healed)?;
        Ok(healed)
    }

    fn looks_like_attr_start(slice: &str) -> bool {
        let mut chars = slice.chars();
        let Some(first) = chars.next() else {
            return false;
        };
        if !first.is_ascii_alphabetic() && first != '_' && first != ':' {
            return false;
        }
        for ch in chars {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == ':' || ch == '-' || ch == '.' {
                continue;
            }
            if ch == '=' {
                return true;
            }
            if ch.is_whitespace() {
                let after = slice.split_once(ch).map(|x| x.1.trim_start()).unwrap_or("");
                return after.starts_with('=');
            }
            return false;
        }
        false
    }

    fn heal_attribute_quotes(xml: &str) -> String {
        let mut out = String::with_capacity(xml.len() + 64);
        let bytes = xml.as_bytes();
        let len = bytes.len();
        let mut i = 0;

        while i < len {
            if bytes[i] == b'<' {
                if xml[i..].starts_with("<!--") {
                    if let Some(end) = xml[i..].find("-->") {
                        let full_end = i + end + 3;
                        out.push_str(&xml[i..full_end]);
                        i = full_end;
                    } else {
                        out.push_str(&xml[i..]);
                        out.push_str("-->");
                        break;
                    }
                    continue;
                }
                if xml[i..].starts_with("<![CDATA[") {
                    if let Some(end) = xml[i..].find("]]>") {
                        let full_end = i + end + 3;
                        out.push_str(&xml[i..full_end]);
                        i = full_end;
                    } else {
                        out.push_str(&xml[i..]);
                        out.push_str("]]>");
                        break;
                    }
                    continue;
                }
                if xml[i..].starts_with("<?") {
                    if let Some(end) = xml[i..].find("?>") {
                        let full_end = i + end + 2;
                        out.push_str(&xml[i..full_end]);
                        i = full_end;
                    } else {
                        out.push_str(&xml[i..]);
                        out.push_str("?>");
                        break;
                    }
                    continue;
                }
                if xml[i..].starts_with("</") {
                    if let Some(end) = xml[i..].find('>') {
                        let full_end = i + end + 1;
                        out.push_str(&xml[i..full_end]);
                        i = full_end;
                    } else {
                        out.push_str(&xml[i..]);
                        out.push('>');
                        break;
                    }
                    continue;
                }

                out.push('<');
                i += 1;
                let mut in_quote: Option<char> = None;

                while i < len {
                    let c = xml[i..].chars().next().unwrap();
                    let c_len = c.len_utf8();

                    if let Some(q) = in_quote {
                        if c == q {
                            in_quote = None;
                            out.push(c);
                            i += c_len;
                        } else if c == '>' {
                            out.push(q);
                            in_quote = None;
                            out.push('>');
                            i += 1;
                            break;
                        } else if xml[i..].starts_with("/>") {
                            out.push(q);
                            in_quote = None;
                            out.push_str("/>");
                            i += 2;
                            break;
                        } else if c.is_whitespace() {
                            let rem = xml[i..].trim_start();
                            if Self::looks_like_attr_start(rem) {
                                out.push(q);
                                in_quote = None;
                                out.push(c);
                                i += c_len;
                            } else {
                                out.push(c);
                                i += c_len;
                            }
                        } else {
                            out.push(c);
                            i += c_len;
                        }
                    } else if c == '"' || c == '\'' {
                        in_quote = Some(c);
                        out.push(c);
                        i += c_len;
                    } else if c == '>' {
                        out.push('>');
                        i += 1;
                        break;
                    } else if xml[i..].starts_with("/>") {
                        out.push_str("/>");
                        i += 2;
                        break;
                    } else {
                        out.push(c);
                        i += c_len;
                    }
                }

                if let Some(q) = in_quote {
                    out.push(q);
                    out.push('>');
                }
            } else {
                let c = xml[i..].chars().next().unwrap();
                let c_len = c.len_utf8();
                out.push(c);
                i += c_len;
            }
        }

        out
    }

    fn heal_naked_ampersands(xml: &str) -> String {
        let mut out = String::with_capacity(xml.len() + 16);
        let mut i = 0;
        let mut in_tag = false;

        while i < xml.len() {
            let ch = xml[i..].chars().next().unwrap();
            let ch_len = ch.len_utf8();

            if ch == '<' {
                in_tag = true;
                out.push('<');
                i += ch_len;
            } else if ch == '>' {
                in_tag = false;
                out.push('>');
                i += ch_len;
            } else if ch == '&' && !in_tag {
                if is_valid_xml_entity(&xml[i..]) {
                    out.push('&');
                } else {
                    out.push_str("&amp;");
                }
                i += ch_len;
            } else {
                out.push(ch);
                i += ch_len;
            }
        }

        out
    }

    fn heal_tags(xml: &str) -> String {
        let mut out = String::with_capacity(xml.len() + 64);
        let mut tag_stack: Vec<String> = Vec::new();
        let mut pos = 0;

        while pos < xml.len() {
            let next_open = match xml[pos..].find('<') {
                Some(idx) => pos + idx,
                None => {
                    out.push_str(&xml[pos..]);
                    break;
                }
            };

            if next_open > pos {
                out.push_str(&xml[pos..next_open]);
            }

            if xml[next_open..].starts_with("<!--") {
                let end = match xml[next_open..].find("-->") {
                    Some(idx) => next_open + idx + 3,
                    None => xml.len(),
                };
                out.push_str(&xml[next_open..end]);
                pos = end;
                continue;
            }

            if xml[next_open..].starts_with("<![CDATA[") {
                let end = match xml[next_open..].find("]]>") {
                    Some(idx) => next_open + idx + 3,
                    None => xml.len(),
                };
                out.push_str(&xml[next_open..end]);
                pos = end;
                continue;
            }

            if xml[next_open..].starts_with("<?") {
                let end = match xml[next_open..].find("?>") {
                    Some(idx) => next_open + idx + 2,
                    None => xml.len(),
                };
                out.push_str(&xml[next_open..end]);
                pos = end;
                continue;
            }

            if xml[next_open..].starts_with("<!") {
                let end = match xml[next_open..].find('>') {
                    Some(idx) => next_open + idx + 1,
                    None => xml.len(),
                };
                out.push_str(&xml[next_open..end]);
                pos = end;
                continue;
            }

            if xml[next_open..].starts_with("</") {
                let end = match xml[next_open..].find('>') {
                    Some(idx) => next_open + idx + 1,
                    None => xml.len(),
                };
                let tag_name = extract_tag_name(&xml[next_open..end]);

                if tag_name.is_empty() {
                    pos = end;
                    continue;
                }

                if tag_stack
                    .last()
                    .map(|t| t.eq_ignore_ascii_case(&tag_name))
                    .unwrap_or(false)
                {
                    let actual = tag_stack.pop().unwrap();
                    out.push_str(&format!("</{actual}>"));
                } else if tag_stack.iter().any(|t| t.eq_ignore_ascii_case(&tag_name)) {
                    while let Some(top) = tag_stack.pop() {
                        if top.eq_ignore_ascii_case(&tag_name) {
                            out.push_str(&format!("</{top}>"));
                            break;
                        } else {
                            out.push_str(&format!("</{top}>"));
                        }
                    }
                }

                pos = end;
                continue;
            }

            let end = match xml[next_open..].find('>') {
                Some(idx) => next_open + idx + 1,
                None => xml.len(),
            };

            let tag_str = &xml[next_open..end];
            let tag_name = extract_tag_name(tag_str);

            if tag_name.is_empty() {
                out.push_str(tag_str);
                pos = end;
                continue;
            }

            let is_self_closing = tag_str.trim_end().ends_with("/>");

            if is_self_closing {
                out.push_str(tag_str);
            } else if is_void_tag(&tag_name) {
                let without_gt = tag_str.strip_suffix('>').unwrap_or(tag_str).trim_end();
                out.push_str(without_gt);
                out.push_str(" />");
            } else {
                tag_stack.push(tag_name);
                out.push_str(tag_str);
            }

            pos = end;
        }

        while let Some(unclosed) = tag_stack.pop() {
            out.push_str(&format!("</{unclosed}>"));
        }

        out
    }
}
