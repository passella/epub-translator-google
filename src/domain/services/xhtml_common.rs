pub const VOID_TAGS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

pub fn find_tag_closing_bracket(slice: &str) -> Option<usize> {
    let mut in_quote = None;
    for (idx, ch) in slice.char_indices() {
        if let Some(q) = in_quote {
            if ch == q || ch == '<' || ch == '\n' {
                in_quote = None;
            }
        } else if ch == '"' || ch == '\'' {
            in_quote = Some(ch);
        } else if ch == '>' {
            return Some(idx);
        }
    }
    None
}

pub fn extract_tag_name(slice: &str) -> String {
    let trimmed = slice.trim_start();
    let without_open = if let Some(stripped) = trimmed.strip_prefix("</") {
        stripped
    } else if let Some(stripped) = trimmed.strip_prefix('<') {
        stripped
    } else {
        return String::new();
    };

    let mut name = String::new();
    for ch in without_open.chars() {
        if ch.is_ascii_alphanumeric() || ch == ':' || ch == '_' || ch == '-' {
            name.push(ch);
        } else {
            break;
        }
    }
    name.to_ascii_lowercase()
}

pub fn is_void_tag(name: &str) -> bool {
    VOID_TAGS.iter().any(|v| v.eq_ignore_ascii_case(name))
}

pub fn is_valid_xml_entity(slice: &str) -> bool {
    if !slice.starts_with('&') {
        return false;
    }
    let rest = &slice[1..];
    let Some(semicolon_pos) = rest.find(';') else {
        return false;
    };
    if semicolon_pos == 0 || semicolon_pos > 10 {
        return false;
    }
    let entity_body = &rest[..semicolon_pos];
    if let Some(dec) = entity_body.strip_prefix('#') {
        if let Some(hex) = dec.strip_prefix('x').or_else(|| dec.strip_prefix('X')) {
            !hex.is_empty() && hex.chars().all(|c| c.is_ascii_hexdigit())
        } else {
            !dec.is_empty() && dec.chars().all(|c| c.is_ascii_digit())
        }
    } else {
        entity_body
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    }
}
