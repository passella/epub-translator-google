use crate::domain::entities::{
    EpubBook, EpubChapter, EpubMetadata, EpubResource, EpubVerificationReport, Language,
};
use crate::domain::errors::EpubError;
use crate::domain::ports::EpubPort;
use crate::domain::services::XhtmlValidator;
use async_trait::async_trait;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

pub struct ZipEpubAdapter;

impl ZipEpubAdapter {
    pub fn new() -> Self {
        Self
    }

    fn find_opf_path_from_container_xml(xml: &str) -> Result<String, EpubError> {
        let needle = "full-path=\"";
        if let Some(start_pos) = xml.find(needle) {
            let rest = &xml[start_pos + needle.len()..];
            if let Some(end_pos) = rest.find('"') {
                return Ok(rest[..end_pos].to_string());
            }
        }

        let needle_single = "full-path='";
        if let Some(start_pos) = xml.find(needle_single) {
            let rest = &xml[start_pos + needle_single.len()..];
            if let Some(end_pos) = rest.find('\'') {
                return Ok(rest[..end_pos].to_string());
            }
        }

        Err(EpubError::InvalidStructure(
            "Could not find rootfile full-path attribute in container.xml".to_string(),
        ))
    }

    fn parse_opf_metadata(xml: &str) -> EpubMetadata {
        let title = extract_tag_content(xml, "dc:title");
        let language = extract_tag_content(xml, "dc:language").map(Language::new);
        let creator = extract_tag_content(xml, "dc:creator");
        let identifier = extract_tag_content(xml, "dc:identifier");
        let publisher = extract_tag_content(xml, "dc:publisher");

        EpubMetadata {
            title,
            language,
            creator,
            identifier,
            publisher,
        }
    }
}

impl Default for ZipEpubAdapter {
    fn default() -> Self {
        Self::new()
    }
}

fn extract_tag_content(xml: &str, tag_name: &str) -> Option<String> {
    let open_tag = format!("<{tag_name}");
    let close_tag = format!("</{tag_name}>");

    let start = xml.find(&open_tag)?;
    let tag_end = xml[start..].find('>')? + start + 1;
    let end = xml[tag_end..].find(&close_tag)? + tag_end;

    let text = xml[tag_end..end].trim();
    if text.is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}

#[async_trait]
impl EpubPort for ZipEpubAdapter {
    async fn read_epub(&self, path: &Path) -> Result<EpubBook, EpubError> {
        let file = File::open(path).map_err(EpubError::Io)?;
        let reader = BufReader::new(file);
        let mut archive = ZipArchive::new(reader)
            .map_err(|e| EpubError::Zip(format!("Failed to open ZIP archive: {e}")))?;

        let mut container_xml_content = None;
        for i in 0..archive.len() {
            let mut file = archive
                .by_index(i)
                .map_err(|e| EpubError::Zip(format!("Failed to read zip entry {i}: {e}")))?;
            if file.name() == "META-INF/container.xml" {
                let mut content = String::new();
                file.read_to_string(&mut content).map_err(EpubError::Io)?;
                container_xml_content = Some(content);
                break;
            }
        }

        let container_xml = container_xml_content.ok_or(EpubError::MissingContainerXml)?;
        let opf_path = Self::find_opf_path_from_container_xml(&container_xml)?;

        let mut raw_opf_content = None;
        let mut chapters = Vec::new();
        let mut resources = Vec::new();

        for i in 0..archive.len() {
            let mut file = archive
                .by_index(i)
                .map_err(|e| EpubError::Zip(format!("Failed to access zip entry {i}: {e}")))?;
            let file_name = file.name().to_string();

            if file.is_dir() || file_name == "mimetype" {
                continue;
            }

            if file_name == opf_path {
                let mut content = String::new();
                file.read_to_string(&mut content).map_err(EpubError::Io)?;
                raw_opf_content = Some(content);
                continue;
            }

            let lower = file_name.to_lowercase();
            let is_html =
                lower.ends_with(".xhtml") || lower.ends_with(".html") || lower.ends_with(".htm");

            if is_html {
                let mut content = String::new();
                if file.read_to_string(&mut content).is_ok() {
                    chapters.push(EpubChapter {
                        path: file_name,
                        content,
                        media_type: "application/xhtml+xml".to_string(),
                    });
                    continue;
                }
            }

            let mut data = Vec::with_capacity(file.size() as usize);
            file.read_to_end(&mut data).map_err(EpubError::Io)?;
            resources.push(EpubResource {
                path: file_name,
                data,
            });
        }

        let opf_str = raw_opf_content.ok_or(EpubError::MissingPackageDocument)?;
        let metadata = Self::parse_opf_metadata(&opf_str);

        Ok(EpubBook {
            metadata,
            chapters,
            resources,
            raw_container_xml: Some(container_xml),
            raw_opf_content: Some(opf_str),
            opf_path,
        })
    }

    async fn write_epub(&self, book: &EpubBook, path: &Path) -> Result<(), EpubError> {
        let file = File::create(path).map_err(EpubError::Io)?;
        let writer = BufWriter::new(file);
        let mut zip = ZipWriter::new(writer);

        let stored_options =
            SimpleFileOptions::default().compression_method(CompressionMethod::Stored);

        zip.start_file("mimetype", stored_options)
            .map_err(|e| EpubError::Zip(format!("Failed to start mimetype file: {e}")))?;
        zip.write_all(b"application/epub+zip")
            .map_err(EpubError::Io)?;

        let deflated_options =
            SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

        if let Some(container_xml) = &book.raw_container_xml {
            zip.start_file("META-INF/container.xml", deflated_options)
                .map_err(|e| EpubError::Zip(format!("Failed to write container.xml: {e}")))?;
            zip.write_all(container_xml.as_bytes())
                .map_err(EpubError::Io)?;
        }

        if let Some(opf_content) = &book.raw_opf_content {
            zip.start_file(&book.opf_path, deflated_options)
                .map_err(|e| EpubError::Zip(format!("Failed to write OPF file: {e}")))?;
            zip.write_all(opf_content.as_bytes())
                .map_err(EpubError::Io)?;
        }

        for chapter in &book.chapters {
            if chapter.content.trim().is_empty() {
                return Err(EpubError::InvalidStructure(format!(
                    "Chapter '{}' has 0 bytes or is empty",
                    chapter.path
                )));
            }
            let is_xhtml = chapter.path.ends_with(".xhtml")
                || chapter.path.ends_with(".html")
                || chapter.path.ends_with(".htm");
            let content_to_write = if is_xhtml {
                XhtmlValidator::heal(&chapter.content)
            } else {
                chapter.content.clone()
            };
            zip.start_file(&chapter.path, deflated_options)
                .map_err(|e| {
                    EpubError::Zip(format!("Failed to write chapter {}: {e}", chapter.path))
                })?;
            zip.write_all(content_to_write.as_bytes())
                .map_err(EpubError::Io)?;
        }

        for resource in &book.resources {
            if resource.path == "META-INF/container.xml" || resource.path == book.opf_path {
                continue;
            }
            zip.start_file(&resource.path, deflated_options)
                .map_err(|e| {
                    EpubError::Zip(format!("Failed to write resource {}: {e}", resource.path))
                })?;
            zip.write_all(&resource.data).map_err(EpubError::Io)?;
        }

        zip.finish()
            .map_err(|e| EpubError::Zip(format!("Failed to finalize EPUB ZIP container: {e}")))?;

        Ok(())
    }

    async fn verify_epub(&self, path: &Path) -> Result<EpubVerificationReport, EpubError> {
        let file = File::open(path).map_err(EpubError::Io)?;
        let reader = BufReader::new(file);
        let mut archive = ZipArchive::new(reader).map_err(|e| {
            EpubError::Zip(format!("Failed to open ZIP archive for verification: {e}"))
        })?;

        let mut errors = Vec::new();
        let mut mimetype_valid = false;
        let mut container_xml_valid = false;
        let mut opf_valid = false;
        let mut total_xhtml_files = 0;
        let mut valid_xhtml_files = 0;

        if archive.is_empty() {
            errors.push("ZIP container is completely empty".to_string());
            return Ok(EpubVerificationReport {
                is_valid: false,
                mimetype_valid: false,
                container_xml_valid: false,
                opf_valid: false,
                total_xhtml_files: 0,
                valid_xhtml_files: 0,
                errors,
            });
        }

        match archive.by_index(0) {
            Ok(mut first_entry) => {
                let name = first_entry.name().to_string();
                let comp_method = first_entry.compression();
                let size = first_entry.size();
                let mut data = Vec::new();
                let _ = first_entry.read_to_end(&mut data);

                if name != "mimetype" {
                    errors.push(format!(
                        "OCF violation: first entry must be 'mimetype', found '{name}'"
                    ));
                } else if comp_method != CompressionMethod::Stored {
                    errors.push(
                        "OCF violation: 'mimetype' must be uncompressed (Stored)".to_string(),
                    );
                } else if size != 20 || data != b"application/epub+zip" {
                    errors.push(
                        "OCF violation: 'mimetype' content must be exactly 20 bytes 'application/epub+zip'"
                            .to_string(),
                    );
                } else {
                    mimetype_valid = true;
                }
            }
            Err(e) => {
                errors.push(format!("Failed to read first entry in ZIP archive: {e}"));
            }
        }

        let mut container_xml_content = None;
        for i in 0..archive.len() {
            if let Ok(mut entry) = archive.by_index(i)
                && entry.name() == "META-INF/container.xml"
            {
                let mut content = String::new();
                if entry.read_to_string(&mut content).is_ok() {
                    container_xml_content = Some(content);
                }
                break;
            }
        }

        let opf_path = match container_xml_content {
            Some(xml) => match XhtmlValidator::validate(&xml) {
                Ok(()) => {
                    container_xml_valid = true;
                    match Self::find_opf_path_from_container_xml(&xml) {
                        Ok(path) => Some(path),
                        Err(e) => {
                            errors.push(format!("Failed to parse OPF path in container.xml: {e}"));
                            None
                        }
                    }
                }
                Err(e) => {
                    errors.push(format!(
                        "META-INF/container.xml is not well-formed XML: {e}"
                    ));
                    None
                }
            },
            None => {
                errors.push("Missing required entry: META-INF/container.xml".to_string());
                None
            }
        };

        if let Some(ref expected_opf) = opf_path {
            let mut found_opf = false;
            for i in 0..archive.len() {
                if let Ok(mut entry) = archive.by_index(i)
                    && entry.name() == expected_opf
                {
                    found_opf = true;
                    let mut content = String::new();
                    if entry.read_to_string(&mut content).is_ok() {
                        match XhtmlValidator::validate(&content) {
                            Ok(()) => opf_valid = true,
                            Err(e) => {
                                errors.push(format!(
                                    "OPF package document '{expected_opf}' is not well-formed XML: {e}"
                                ));
                            }
                        }
                    } else {
                        errors.push(format!(
                            "Failed to read OPF package document '{expected_opf}'"
                        ));
                    }
                    break;
                }
            }
            if !found_opf {
                errors.push(format!(
                    "Package document declared in container.xml not found: '{expected_opf}'"
                ));
            }
        }

        for i in 0..archive.len() {
            let Ok(mut entry) = archive.by_index(i) else {
                continue;
            };
            let name = entry.name().to_string();
            let lower = name.to_lowercase();
            let is_xhtml =
                lower.ends_with(".xhtml") || lower.ends_with(".html") || lower.ends_with(".htm");

            if is_xhtml {
                total_xhtml_files += 1;
                let mut content = String::new();
                match entry.read_to_string(&mut content) {
                    Ok(_) => {
                        if content.trim().is_empty() {
                            errors.push(format!("XHTML file '{name}' has 0 bytes or is empty"));
                        } else {
                            match XhtmlValidator::validate(&content) {
                                Ok(()) => valid_xhtml_files += 1,
                                Err(e) => {
                                    errors.push(format!(
                                        "XHTML file '{name}' is not well-formed XML: {e}"
                                    ));
                                }
                            }
                        }
                    }
                    Err(e) => {
                        errors.push(format!("Failed to read XHTML file '{name}' as UTF-8: {e}"));
                    }
                }
            }
        }

        let is_valid = mimetype_valid && container_xml_valid && opf_valid && errors.is_empty();

        Ok(EpubVerificationReport {
            is_valid,
            mimetype_valid,
            container_xml_valid,
            opf_valid,
            total_xhtml_files,
            valid_xhtml_files,
            errors,
        })
    }
}
