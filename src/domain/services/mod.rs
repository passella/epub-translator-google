pub mod translate_book;
pub mod xhtml_common;
pub mod xhtml_partitioner;
pub mod xhtml_validator;

pub use translate_book::{TranslateBookUseCase, update_xml_tag};
pub use xhtml_common::{
    VOID_TAGS, extract_tag_name, find_tag_closing_bracket, is_valid_xml_entity, is_void_tag,
};
pub use xhtml_partitioner::{PlanSegment, XhtmlDocumentPlan, XhtmlPartitioner};
pub use xhtml_validator::{XhtmlValidationError, XhtmlValidator};
