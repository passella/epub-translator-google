pub mod translate_book;
pub mod xhtml_common;
pub mod xhtml_conformance;
pub mod xhtml_partitioner;
pub mod xhtml_tag;
pub mod xhtml_validator;

pub use translate_book::{TranslateBookUseCase, update_xml_tag};
pub use xhtml_common::{
    VOID_TAGS, extract_tag_name, find_tag_closing_bracket, is_valid_xml_entity, is_void_tag,
};
pub use xhtml_conformance::{ConformanceIssue, XhtmlConformance};
pub use xhtml_partitioner::{PlanSegment, XhtmlDocumentPlan, XhtmlPartitioner};
pub use xhtml_tag::{StartTag, TagAttribute, parse_attributes, render_tag, start_tags};
pub use xhtml_validator::{XhtmlValidationError, XhtmlValidator};
