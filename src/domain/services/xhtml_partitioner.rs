use super::xhtml_common::{extract_tag_name, find_tag_closing_bracket};

#[derive(Clone, Debug, PartialEq)]
pub enum PlanSegment {
    Verbatim(String),
    Translatable(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct XhtmlDocumentPlan {
    pub prelude: String,
    pub segments: Vec<PlanSegment>,
    pub postlude: String,
}

impl XhtmlDocumentPlan {
    pub fn translatable_chunks(&self) -> Vec<&str> {
        let mut chunks = Vec::new();
        for seg in &self.segments {
            if let PlanSegment::Translatable(s) = seg {
                chunks.push(s.as_str());
            }
        }
        chunks
    }

    pub fn recompose<S: AsRef<str>>(&self, translated_chunks: &[S]) -> String {
        let mut output = String::new();
        output.push_str(&self.prelude);

        let mut chunk_idx = 0;
        for seg in &self.segments {
            match seg {
                PlanSegment::Verbatim(v) => output.push_str(v),
                PlanSegment::Translatable(_) => {
                    if let Some(trans) = translated_chunks.get(chunk_idx) {
                        output.push_str(trans.as_ref());
                    }
                    chunk_idx += 1;
                }
            }
        }

        output.push_str(&self.postlude);
        output
    }
}

#[derive(Clone, Debug)]
enum ParsedBlock {
    Verbatim(String),
    Element {
        full: String,
        is_container: bool,
        open_tag: String,
        inner: String,
        close_tag: String,
    },
}

struct PartitionFrame {
    blocks: std::vec::IntoIter<ParsedBlock>,
    current_translatable: String,
    close_tag: Option<String>,
    depth: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct XhtmlPartitioner {
    threshold: usize,
}

impl Default for XhtmlPartitioner {
    fn default() -> Self {
        Self::new()
    }
}

impl XhtmlPartitioner {
    pub const DEFAULT_THRESHOLD: usize = 100 * 1024;
    pub const MAX_NESTING_DEPTH: usize = 256;

    pub fn new() -> Self {
        Self {
            threshold: Self::DEFAULT_THRESHOLD,
        }
    }

    pub fn with_threshold(threshold: usize) -> Self {
        Self {
            threshold: threshold.max(1),
        }
    }

    pub fn threshold(&self) -> usize {
        self.threshold
    }

    pub fn is_giant(&self, html: &str) -> bool {
        html.len() > self.threshold
    }

    pub fn partition(&self, html: &str) -> XhtmlDocumentPlan {
        let (prelude, body_content, postlude) = Self::split_document_shell(html);
        let blocks = Self::parse_top_level_blocks(&body_content);
        let mut segments = Vec::new();
        Self::group_blocks_into_segments(blocks, self.threshold, &mut segments);

        XhtmlDocumentPlan {
            prelude,
            segments,
            postlude,
        }
    }

    fn split_document_shell(html: &str) -> (String, String, String) {
        let lower = html.to_ascii_lowercase();
        let body_needle = "<body";
        let body_close_needle = "</body";

        let Some(body_open_start) = lower.find(body_needle) else {
            return (String::new(), html.to_string(), String::new());
        };

        let next_char = lower[body_open_start + body_needle.len()..].chars().next();
        let is_valid_body_tag = matches!(next_char, Some(' ' | '\t' | '\r' | '\n' | '>'));
        if !is_valid_body_tag {
            return (String::new(), html.to_string(), String::new());
        }

        let Some(rel_open_bracket) = find_tag_closing_bracket(&html[body_open_start..]) else {
            return (String::new(), html.to_string(), String::new());
        };
        let body_open_end = body_open_start + rel_open_bracket + 1;

        let Some(rel_close_start) = lower[body_open_end..].find(body_close_needle) else {
            return (
                html[..body_open_end].to_string(),
                html[body_open_end..].to_string(),
                String::new(),
            );
        };
        let body_close_start = body_open_end + rel_close_start;

        let prelude = html[..body_open_end].to_string();
        let body_content = html[body_open_end..body_close_start].to_string();
        let postlude = html[body_close_start..].to_string();

        (prelude, body_content, postlude)
    }

    fn parse_top_level_blocks(content: &str) -> Vec<ParsedBlock> {
        let mut blocks = Vec::new();
        let mut pos = 0;

        while pos < content.len() {
            let text_start = pos;
            let next_open = match content[pos..].find('<') {
                Some(idx) => pos + idx,
                None => {
                    let rem = &content[pos..];
                    if !rem.is_empty() {
                        blocks.push(ParsedBlock::Verbatim(rem.to_string()));
                    }
                    break;
                }
            };

            if next_open > text_start {
                blocks.push(ParsedBlock::Verbatim(
                    content[text_start..next_open].to_string(),
                ));
            }

            if content[next_open..].starts_with("<!--") {
                let end = match content[next_open..].find("-->") {
                    Some(idx) => next_open + idx + 3,
                    None => content.len(),
                };
                blocks.push(ParsedBlock::Verbatim(content[next_open..end].to_string()));
                pos = end;
                continue;
            }

            if content[next_open..].starts_with("<![CDATA[") {
                let end = match content[next_open..].find("]]>") {
                    Some(idx) => next_open + idx + 3,
                    None => content.len(),
                };
                blocks.push(ParsedBlock::Verbatim(content[next_open..end].to_string()));
                pos = end;
                continue;
            }

            if content[next_open..].starts_with("<?") {
                let end = match content[next_open..].find("?>") {
                    Some(idx) => next_open + idx + 2,
                    None => content.len(),
                };
                blocks.push(ParsedBlock::Verbatim(content[next_open..end].to_string()));
                pos = end;
                continue;
            }

            if content[next_open..].starts_with("<!") {
                let end = match find_tag_closing_bracket(&content[next_open..]) {
                    Some(idx) => next_open + idx + 1,
                    None => content.len(),
                };
                blocks.push(ParsedBlock::Verbatim(content[next_open..end].to_string()));
                pos = end;
                continue;
            }

            if content[next_open..].starts_with("</") {
                let end = match find_tag_closing_bracket(&content[next_open..]) {
                    Some(idx) => next_open + idx + 1,
                    None => content.len(),
                };
                blocks.push(ParsedBlock::Verbatim(content[next_open..end].to_string()));
                pos = end;
                continue;
            }

            let tag_name = extract_tag_name(&content[next_open..]);
            let open_tag_end = match find_tag_closing_bracket(&content[next_open..]) {
                Some(idx) => next_open + idx + 1,
                None => content.len(),
            };

            let open_tag_str = &content[next_open..open_tag_end];
            let is_self_closing = open_tag_str.trim_end().ends_with("/>");

            if is_self_closing || tag_name.is_empty() {
                blocks.push(ParsedBlock::Element {
                    full: open_tag_str.to_string(),
                    is_container: false,
                    open_tag: open_tag_str.to_string(),
                    inner: String::new(),
                    close_tag: String::new(),
                });
                pos = open_tag_end;
                continue;
            }

            let elem_end = match Self::find_matching_close_tag(&content[next_open..], &tag_name) {
                Some(idx) => (next_open + idx).min(content.len()),
                None => content.len(),
            };

            let open_tag_end = open_tag_end.min(elem_end);

            let close_tag_start =
                match Self::find_last_close_tag_start(&content[next_open..elem_end], &tag_name) {
                    Some(idx) => (next_open + idx).max(open_tag_end).min(elem_end),
                    None => elem_end,
                };

            let open_tag = content[next_open..open_tag_end].to_string();
            let inner = content[open_tag_end..close_tag_start].to_string();
            let close_tag = content[close_tag_start..elem_end].to_string();
            let full = content[next_open..elem_end].to_string();

            blocks.push(ParsedBlock::Element {
                full,
                is_container: true,
                open_tag,
                inner,
                close_tag,
            });
            pos = elem_end;
        }

        blocks
    }

    fn group_blocks_into_segments(
        blocks: Vec<ParsedBlock>,
        threshold: usize,
        segments: &mut Vec<PlanSegment>,
    ) {
        let mut stack = vec![PartitionFrame {
            blocks: blocks.into_iter(),
            current_translatable: String::new(),
            close_tag: None,
            depth: 0,
        }];

        while let Some(frame) = stack.last_mut() {
            let Some(block) = frame.blocks.next() else {
                if let Some(finished) = stack.pop() {
                    if !finished.current_translatable.is_empty() {
                        segments.push(PlanSegment::Translatable(finished.current_translatable));
                    }
                    if let Some(close_tag) = finished.close_tag {
                        segments.push(PlanSegment::Verbatim(close_tag));
                    }
                }
                continue;
            };

            match block {
                ParsedBlock::Verbatim(v) => {
                    if v.trim().is_empty() {
                        if !frame.current_translatable.is_empty() {
                            frame.current_translatable.push_str(&v);
                        } else {
                            segments.push(PlanSegment::Verbatim(v));
                        }
                    } else {
                        frame.current_translatable.push_str(&v);
                    }
                }
                ParsedBlock::Element {
                    full,
                    is_container,
                    open_tag,
                    inner,
                    close_tag,
                } => {
                    if full.len() <= threshold {
                        if !frame.current_translatable.is_empty()
                            && frame.current_translatable.len() + full.len() > threshold
                        {
                            segments.push(PlanSegment::Translatable(std::mem::take(
                                &mut frame.current_translatable,
                            )));
                        }
                        frame.current_translatable.push_str(&full);
                    } else if is_container
                        && !inner.trim().is_empty()
                        && frame.depth < Self::MAX_NESTING_DEPTH
                    {
                        if !frame.current_translatable.is_empty() {
                            segments.push(PlanSegment::Translatable(std::mem::take(
                                &mut frame.current_translatable,
                            )));
                        }
                        segments.push(PlanSegment::Verbatim(open_tag));
                        let child_depth = frame.depth + 1;
                        let inner_blocks = Self::parse_top_level_blocks(&inner);
                        stack.push(PartitionFrame {
                            blocks: inner_blocks.into_iter(),
                            current_translatable: String::new(),
                            close_tag: Some(close_tag),
                            depth: child_depth,
                        });
                    } else {
                        if !frame.current_translatable.is_empty() {
                            segments.push(PlanSegment::Translatable(std::mem::take(
                                &mut frame.current_translatable,
                            )));
                        }
                        segments.push(PlanSegment::Translatable(full));
                    }
                }
            }
        }
    }

    fn find_matching_close_tag(slice: &str, target_tag: &str) -> Option<usize> {
        let mut depth = 1usize;
        let mut cur = find_tag_closing_bracket(slice)? + 1;

        while cur < slice.len() && depth > 0 {
            let next_open = match slice[cur..].find('<') {
                Some(idx) => cur + idx,
                None => break,
            };

            if slice[next_open..].starts_with("<!--") {
                cur = match slice[next_open..].find("-->") {
                    Some(idx) => next_open + idx + 3,
                    None => slice.len(),
                };
                continue;
            }

            if slice[next_open..].starts_with("<![CDATA[") {
                cur = match slice[next_open..].find("]]>") {
                    Some(idx) => next_open + idx + 3,
                    None => slice.len(),
                };
                continue;
            }

            if slice[next_open..].starts_with("<?") {
                cur = match slice[next_open..].find("?>") {
                    Some(idx) => next_open + idx + 2,
                    None => slice.len(),
                };
                continue;
            }

            if slice[next_open..].starts_with("</") {
                let tag_name = extract_tag_name(&slice[next_open..]);
                let close_end = match find_tag_closing_bracket(&slice[next_open..]) {
                    Some(idx) => next_open + idx + 1,
                    None => slice.len(),
                };
                if tag_name.eq_ignore_ascii_case(target_tag) {
                    depth -= 1;
                    if depth == 0 {
                        return Some(close_end);
                    }
                }
                cur = close_end;
                continue;
            }

            if slice[next_open..].starts_with('<') {
                let tag_name = extract_tag_name(&slice[next_open..]);
                let tag_end = match find_tag_closing_bracket(&slice[next_open..]) {
                    Some(idx) => next_open + idx + 1,
                    None => slice.len(),
                };
                let is_self = slice[next_open..tag_end].trim_end().ends_with("/>");
                if !is_self && tag_name.eq_ignore_ascii_case(target_tag) {
                    depth += 1;
                }
                cur = tag_end;
                continue;
            }

            cur = next_open + 1;
        }

        None
    }

    fn find_last_close_tag_start(slice: &str, target_tag: &str) -> Option<usize> {
        if target_tag.is_empty() {
            return None;
        }
        let needle = format!("</{target_tag}");
        slice
            .as_bytes()
            .windows(needle.len())
            .rposition(|window| window.eq_ignore_ascii_case(needle.as_bytes()))
    }
}
