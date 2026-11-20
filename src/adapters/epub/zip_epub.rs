use crate::domain::entities::{
    EpubBook, EpubChapter, EpubMetadata, EpubResource, EpubVerificationReport, Language,
};
use crate::domain::errors::EpubError;
use crate::domain::ports::EpubPort;
use crate::domain::services::{XhtmlConformance, XhtmlValidator};
use async_trait::async_trait;
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

const INITIAL_ALLOC_CAP: usize = 64 * 1024;
const MAX_ENTRY_BYTES: u64 = 64 * 1024 * 1024;
const MAX_METADATA_BYTES: u64 = 4 * 1024 * 1024;
const MAX_TOTAL_DECOMPRESSED_BYTES: u64 = 512 * 1024 * 1024;
const MAX_ZIP_ENTRIES: usize = 10_000;
const MIMETYPE_EXPECTED_LEN: u64 = 20;
const MIMETYPE_EXPECTED_BYTES: &[u8] = b"application/epub+zip";

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempFileGuard {
    path: PathBuf,
    armed: bool,
}

impl TempFileGuard {
    fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

struct ScannedEntry {
    name: String,
    is_dir: bool,
}

enum ContainerOpfLookup {
    Found(String),
    MissingAttribute,
    XmlError(EpubError),
}

pub struct ZipEpubAdapter;

impl ZipEpubAdapter {
    pub const INITIAL_ALLOC_CAP: usize = INITIAL_ALLOC_CAP;
    pub const MAX_ENTRY_BYTES: u64 = MAX_ENTRY_BYTES;
    pub const MAX_METADATA_BYTES: u64 = MAX_METADATA_BYTES;
    pub const MAX_TOTAL_DECOMPRESSED_BYTES: u64 = MAX_TOTAL_DECOMPRESSED_BYTES;
    pub const MAX_ZIP_ENTRIES: usize = MAX_ZIP_ENTRIES;

    pub fn new() -> Self {
        Self
    }

    pub fn find_opf_path_from_container_xml(xml: &str) -> Result<String, EpubError> {
        match lookup_opf_path_from_container_xml(xml) {
            ContainerOpfLookup::Found(raw_path) => validate_container_full_path(&raw_path),
            ContainerOpfLookup::MissingAttribute => Err(EpubError::InvalidStructure(
                "Could not find rootfile full-path attribute in container.xml".to_string(),
            )),
            ContainerOpfLookup::XmlError(err) => Err(err),
        }
    }

    pub fn parse_opf_metadata(xml: &str) -> EpubMetadata {
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

    fn read_epub_sync(path: &Path) -> Result<EpubBook, EpubError> {
        let file = File::open(path).map_err(EpubError::Io)?;
        let reader = BufReader::new(file);
        let mut archive = ZipArchive::new(reader)
            .map_err(|e| EpubError::Zip(format!("Failed to open ZIP archive: {e}")))?;

        let entries = scan_and_validate_archive_entries(&mut archive)?;
        let mut total_decompressed: u64 = 0;

        let container_idx = entries
            .iter()
            .position(|entry| !entry.is_dir && entry.name == "META-INF/container.xml")
            .ok_or(EpubError::MissingContainerXml)?;

        let container_xml = {
            let mut container_file = archive.by_index(container_idx).map_err(|e| {
                EpubError::Zip(format!("Failed to read zip entry {container_idx}: {e}"))
            })?;
            read_zip_entry_string(
                &mut container_file,
                "META-INF/container.xml",
                MAX_METADATA_BYTES,
                &mut total_decompressed,
            )?
        };

        let opf_path = Self::find_opf_path_from_container_xml(&container_xml)?;

        let mut raw_opf_content = None;
        let mut chapters = Vec::new();
        let mut resources = Vec::new();

        for (i, entry_meta) in entries.iter().enumerate() {
            if entry_meta.is_dir || entry_meta.name == "mimetype" {
                continue;
            }

            if entry_meta.name == opf_path {
                let mut file = archive
                    .by_index(i)
                    .map_err(|e| EpubError::Zip(format!("Failed to access zip entry {i}: {e}")))?;
                let content = read_zip_entry_string(
                    &mut file,
                    &entry_meta.name,
                    MAX_METADATA_BYTES,
                    &mut total_decompressed,
                )?;
                validate_opf_hrefs(&content, &opf_path)?;
                raw_opf_content = Some(content);
                continue;
            }

            if i == container_idx {
                resources.push(EpubResource {
                    path: entry_meta.name.clone(),
                    data: container_xml.as_bytes().to_vec(),
                });
                continue;
            }

            let mut file = archive
                .by_index(i)
                .map_err(|e| EpubError::Zip(format!("Failed to access zip entry {i}: {e}")))?;
            let data = read_zip_entry_bytes(
                &mut file,
                &entry_meta.name,
                MAX_ENTRY_BYTES,
                &mut total_decompressed,
            )?;

            if is_html_entry(&entry_meta.name) {
                match String::from_utf8(data) {
                    Ok(content) => {
                        chapters.push(EpubChapter {
                            path: entry_meta.name.clone(),
                            content,
                            media_type: "application/xhtml+xml".to_string(),
                        });
                    }
                    Err(utf8_err) => {
                        resources.push(EpubResource {
                            path: entry_meta.name.clone(),
                            data: utf8_err.into_bytes(),
                        });
                    }
                }
                continue;
            }

            resources.push(EpubResource {
                path: entry_meta.name.clone(),
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

    fn write_epub_sync(book: &EpubBook, path: &Path) -> Result<(), EpubError> {
        validate_safe_archive_path(&book.opf_path, false)?;
        let mut prepared_chapters = Vec::with_capacity(book.chapters.len());
        for chapter in &book.chapters {
            validate_safe_archive_path(&chapter.path, false)?;
            if chapter.content.trim().is_empty() {
                return Err(EpubError::InvalidStructure(format!(
                    "Chapter '{}' has 0 bytes or is empty",
                    chapter.path
                )));
            }
            let content_to_write = if is_html_entry(&chapter.path) {
                let healed = XhtmlValidator::heal(&chapter.content);
                if healed.trim().is_empty() {
                    return Err(EpubError::InvalidStructure(format!(
                        "Chapter '{}' is empty after XHTML sanitization",
                        chapter.path
                    )));
                }
                healed
            } else {
                chapter.content.clone()
            };
            prepared_chapters.push((&chapter.path, content_to_write));
        }
        for resource in &book.resources {
            validate_safe_archive_path(&resource.path, false)?;
        }

        let (temp_path, temp_file) = create_temp_epub_file(path)?;
        let mut temp_guard = TempFileGuard::new(temp_path.clone());

        let writer = BufWriter::new(temp_file);
        let mut zip = ZipWriter::new(writer);

        let stored_options =
            SimpleFileOptions::default().compression_method(CompressionMethod::Stored);

        zip.start_file("mimetype", stored_options)
            .map_err(|e| EpubError::Zip(format!("Failed to start mimetype file: {e}")))?;
        zip.write_all(MIMETYPE_EXPECTED_BYTES)
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

        for (chapter_path, content_to_write) in &prepared_chapters {
            zip.start_file(chapter_path, deflated_options)
                .map_err(|e| {
                    EpubError::Zip(format!("Failed to write chapter {chapter_path}: {e}"))
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

        let mut buf_writer = zip
            .finish()
            .map_err(|e| EpubError::Zip(format!("Failed to finalize EPUB ZIP container: {e}")))?;
        buf_writer.flush().map_err(EpubError::Io)?;
        let file = buf_writer
            .into_inner()
            .map_err(|e| EpubError::Io(e.into_error()))?;
        file.sync_all().map_err(EpubError::Io)?;
        drop(file);

        std::fs::rename(&temp_path, path).map_err(EpubError::Io)?;
        temp_guard.disarm();

        Ok(())
    }

    fn verify_epub_sync(path: &Path) -> Result<EpubVerificationReport, EpubError> {
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

        let entries = scan_and_validate_archive_entries(&mut archive)?;
        let mut total_decompressed: u64 = 0;

        match archive.by_index(0) {
            Ok(first_entry) => {
                let name = &entries[0].name;
                let comp_method = first_entry.compression();
                let size = first_entry.size();

                if name != "mimetype" {
                    errors.push(format!(
                        "OCF violation: first entry must be 'mimetype', found '{name}'"
                    ));
                } else if comp_method != CompressionMethod::Stored {
                    errors.push(
                        "OCF violation: 'mimetype' must be uncompressed (Stored)".to_string(),
                    );
                } else if size != MIMETYPE_EXPECTED_LEN {
                    errors.push(
                        "OCF violation: 'mimetype' content must be exactly 20 bytes 'application/epub+zip'"
                            .to_string(),
                    );
                } else if first_entry.compressed_size() != MIMETYPE_EXPECTED_LEN {
                    return Err(EpubError::InvalidStructure(format!(
                        "ZIP entry 'mimetype' compressed size {} mismatches declared Stored size 20",
                        first_entry.compressed_size()
                    )));
                } else {
                    let mut data = Vec::with_capacity(MIMETYPE_EXPECTED_LEN as usize + 1);
                    first_entry
                        .take(MIMETYPE_EXPECTED_LEN + 1)
                        .read_to_end(&mut data)
                        .map_err(EpubError::Io)?;
                    if data.len() as u64 > MIMETYPE_EXPECTED_LEN {
                        return Err(EpubError::InvalidStructure(
                            "ZIP entry 'mimetype' decompressed size exceeds 20 bytes".to_string(),
                        ));
                    }
                    total_decompressed = total_decompressed
                        .checked_add(data.len() as u64)
                        .ok_or_else(|| {
                            EpubError::InvalidStructure(
                                "Total decompressed ZIP size overflowed maximum allowed"
                                    .to_string(),
                            )
                        })?;
                    if data.as_slice() != MIMETYPE_EXPECTED_BYTES {
                        errors.push(
                            "OCF violation: 'mimetype' content must be exactly 20 bytes 'application/epub+zip'"
                                .to_string(),
                        );
                    } else {
                        mimetype_valid = true;
                    }
                }
            }
            Err(e) => {
                errors.push(format!("Failed to read first entry in ZIP archive: {e}"));
            }
        }

        let container_idx = entries
            .iter()
            .position(|entry| !entry.is_dir && entry.name == "META-INF/container.xml");

        let container_xml_content = match container_idx {
            Some(idx) => {
                let mut entry = archive.by_index(idx).map_err(|e| {
                    EpubError::Zip(format!("Failed to read container.xml entry {idx}: {e}"))
                })?;
                let bytes = read_zip_entry_bytes(
                    &mut entry,
                    "META-INF/container.xml",
                    MAX_METADATA_BYTES,
                    &mut total_decompressed,
                )?;
                match String::from_utf8(bytes) {
                    Ok(xml) => Some(xml),
                    Err(e) => {
                        errors.push(format!("META-INF/container.xml is not valid UTF-8: {e}"));
                        None
                    }
                }
            }
            None => {
                errors.push("Missing required entry: META-INF/container.xml".to_string());
                None
            }
        };

        let opf_path = match container_xml_content {
            Some(xml) => match XhtmlValidator::validate(&xml) {
                Ok(()) => {
                    container_xml_valid = true;
                    match lookup_opf_path_from_container_xml(&xml) {
                        ContainerOpfLookup::Found(raw_path) => {
                            Some(validate_container_full_path(&raw_path)?)
                        }
                        ContainerOpfLookup::MissingAttribute => {
                            errors.push(
                                "Failed to parse OPF path in container.xml: Could not find rootfile full-path attribute in container.xml"
                                    .to_string(),
                            );
                            None
                        }
                        ContainerOpfLookup::XmlError(e) => {
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
            None => None,
        };

        if let Some(ref expected_opf) = opf_path {
            let opf_idx = entries
                .iter()
                .position(|entry| !entry.is_dir && entry.name == expected_opf.as_str());
            match opf_idx {
                Some(idx) => {
                    let mut entry = archive.by_index(idx).map_err(|e| {
                        EpubError::Zip(format!(
                            "Failed to read OPF package document '{expected_opf}': {e}"
                        ))
                    })?;
                    let bytes = read_zip_entry_bytes(
                        &mut entry,
                        expected_opf,
                        MAX_METADATA_BYTES,
                        &mut total_decompressed,
                    )?;
                    match String::from_utf8(bytes) {
                        Ok(content) => {
                            validate_opf_hrefs(&content, expected_opf)?;
                            match XhtmlValidator::validate(&content) {
                                Ok(()) => opf_valid = true,
                                Err(e) => {
                                    errors.push(format!(
                                        "OPF package document '{expected_opf}' is not well-formed XML: {e}"
                                    ));
                                }
                            }
                        }
                        Err(e) => {
                            errors.push(format!(
                                "Failed to read OPF package document '{expected_opf}': {e}"
                            ));
                        }
                    }
                }
                None => {
                    errors.push(format!(
                        "Package document declared in container.xml not found: '{expected_opf}'"
                    ));
                }
            }
        }

        for (i, entry_meta) in entries.iter().enumerate() {
            if entry_meta.is_dir || !is_html_entry(&entry_meta.name) {
                continue;
            }
            let name = &entry_meta.name;
            let mut entry = archive.by_index(i).map_err(|e| {
                EpubError::Zip(format!("Failed to access XHTML entry '{name}': {e}"))
            })?;
            total_xhtml_files += 1;
            let bytes =
                read_zip_entry_bytes(&mut entry, name, MAX_ENTRY_BYTES, &mut total_decompressed)?;
            match String::from_utf8(bytes) {
                Ok(content) => {
                    if content.trim().is_empty() {
                        errors.push(format!("XHTML file '{name}' has 0 bytes or is empty"));
                    } else {
                        match XhtmlValidator::validate(&content) {
                            Ok(()) => {
                                let issues = XhtmlConformance::audit(&content);
                                if issues.is_empty() {
                                    valid_xhtml_files += 1;
                                }
                                errors.extend(issues.into_iter().map(|issue| {
                                    format!(
                                        "XHTML file '{name}' violates EPUB conformance: {issue}"
                                    )
                                }));
                            }
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

impl Default for ZipEpubAdapter {
    fn default() -> Self {
        Self::new()
    }
}

fn create_temp_epub_file(dest_path: &Path) -> Result<(PathBuf, File), EpubError> {
    if dest_path.file_name().is_none() {
        return Err(EpubError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Output path must reference a file",
        )));
    }
    let parent_dir = match dest_path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    let pid = std::process::id();
    for _ in 0..16 {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let seq = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let temp_name = format!(".tmp.{pid}.{nanos}.{seq}");
        let candidate = parent_dir.join(temp_name);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => return Ok((candidate, file)),
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(EpubError::Io(err)),
        }
    }
    Err(EpubError::Io(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "Failed to allocate unique temporary EPUB file",
    )))
}

fn scan_and_validate_archive_entries<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
) -> Result<Vec<ScannedEntry>, EpubError> {
    let total_entries = archive.len();
    if total_entries > MAX_ZIP_ENTRIES {
        return Err(EpubError::InvalidStructure(format!(
            "ZIP archive contains {total_entries} entries, exceeding maximum allowed ({MAX_ZIP_ENTRIES})"
        )));
    }

    let mut entries = Vec::with_capacity(total_entries);
    for i in 0..total_entries {
        let file = archive
            .by_index(i)
            .map_err(|e| EpubError::Zip(format!("Failed to read zip entry {i}: {e}")))?;
        let is_dir = file.is_dir();
        let name = validate_zip_entry_path(&file)?;
        entries.push(ScannedEntry { name, is_dir });
    }
    Ok(entries)
}

fn validate_zip_entry_path<R: Read>(
    entry: &zip::read::ZipFile<'_, R>,
) -> Result<String, EpubError> {
    let raw_name = entry
        .name()
        .map_err(|e| EpubError::Zip(format!("Failed to read zip entry name: {e}")))?
        .into_owned();
    let lower = raw_name.to_ascii_lowercase();
    if lower.contains("%2e%2e")
        || lower.contains("%2e.")
        || lower.contains(".%2e")
        || lower.contains("%2f")
        || lower.contains("%5c")
        || lower.contains("%00")
    {
        return Err(EpubError::InvalidStructure(format!(
            "Percent-encoded traversal or separator rejected in ZIP entry: '{raw_name}'"
        )));
    }
    if entry.enclosed_name().is_none() {
        return Err(EpubError::InvalidStructure(format!(
            "Unsafe ZIP entry path rejected by enclosed_name: '{raw_name}'"
        )));
    }
    validate_safe_archive_path(&raw_name, entry.is_dir())?;
    Ok(raw_name)
}

fn has_drive_prefix(segment: &str) -> bool {
    let bytes = segment.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

fn validate_safe_archive_path(raw_path: &str, allow_trailing_slash: bool) -> Result<(), EpubError> {
    if raw_path.is_empty() || raw_path.contains('\0') {
        return Err(EpubError::InvalidStructure(format!(
            "Empty or NUL-containing archive path rejected: '{raw_path}'"
        )));
    }
    if raw_path.starts_with('/') || raw_path.starts_with('\\') || raw_path.contains('\\') {
        return Err(EpubError::InvalidStructure(format!(
            "Absolute or backslash archive path rejected: '{raw_path}'"
        )));
    }
    let lower = raw_path.to_ascii_lowercase();
    if raw_path.contains("://") || lower.starts_with("file:") {
        return Err(EpubError::InvalidStructure(format!(
            "URI scheme archive path rejected: '{raw_path}'"
        )));
    }
    if !allow_trailing_slash && raw_path.ends_with('/') {
        return Err(EpubError::InvalidStructure(format!(
            "File archive path cannot end with '/': '{raw_path}'"
        )));
    }
    let trimmed = if allow_trailing_slash {
        raw_path.trim_end_matches('/')
    } else {
        raw_path
    };
    if trimmed.is_empty() {
        return Err(EpubError::InvalidStructure(format!(
            "Invalid root archive path rejected: '{raw_path}'"
        )));
    }
    for seg in trimmed.split('/') {
        if seg.is_empty() || seg == "." || seg == ".." || has_drive_prefix(seg) {
            return Err(EpubError::InvalidStructure(format!(
                "Unsafe path traversal or drive prefix in archive path: '{raw_path}'"
            )));
        }
    }
    Ok(())
}

fn percent_decode_utf8(input: &str) -> Result<String, EpubError> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut idx = 0;
    while idx < bytes.len() {
        if bytes[idx] == b'%'
            && idx + 2 < bytes.len()
            && let (Some(hi), Some(lo)) = (hex_value(bytes[idx + 1]), hex_value(bytes[idx + 2]))
        {
            out.push((hi << 4) | lo);
            idx += 3;
            continue;
        }
        out.push(bytes[idx]);
        idx += 1;
    }
    String::from_utf8(out).map_err(|e| {
        EpubError::InvalidStructure(format!(
            "Invalid UTF-8 sequence in EPUB path '{input}': {e}"
        ))
    })
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn validate_container_full_path(raw_full_path: &str) -> Result<String, EpubError> {
    let lower = raw_full_path.to_ascii_lowercase();
    if lower.contains("%2f") || lower.contains("%5c") || lower.contains("%00") {
        return Err(EpubError::InvalidStructure(format!(
            "Percent-encoded separator or NUL rejected in container.xml full-path: '{raw_full_path}'"
        )));
    }
    validate_safe_archive_path(raw_full_path, false)?;
    let decoded = percent_decode_utf8(raw_full_path)?;
    validate_safe_archive_path(&decoded, false)?;
    Ok(raw_full_path.to_string())
}

fn validate_opf_hrefs(opf_xml: &str, opf_path: &str) -> Result<(), EpubError> {
    let mut opf_dir_segments: Vec<&str> = opf_path.split('/').collect();
    opf_dir_segments.pop();

    let mut reader = Reader::from_str(opf_xml);
    reader.config_mut().trim_text(true);

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                for attr in e.attributes() {
                    let attr = attr.map_err(|err| {
                        EpubError::Xml(format!("Failed to parse OPF attribute: {err}"))
                    })?;
                    if attr.key.local_name().as_ref() == "href" {
                        let val = quick_xml::escape::unescape(&attr.value).map_err(|err| {
                            EpubError::Xml(format!("Failed to unescape OPF href attribute: {err}"))
                        })?;
                        validate_opf_href(val.as_ref(), opf_path, &opf_dir_segments)?;
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(err) => {
                return Err(EpubError::Xml(format!(
                    "XML error in OPF package document: {err}"
                )));
            }
            _ => {}
        }
    }

    Ok(())
}

fn has_uri_scheme(value: &str) -> bool {
    let first_segment = value.split(['/', '?', '#']).next().unwrap_or("");
    first_segment.contains(':')
}

fn validate_opf_href(
    raw_href: &str,
    opf_path: &str,
    opf_dir_segments: &[&str],
) -> Result<(), EpubError> {
    if raw_href.contains('\0') || raw_href.chars().any(|c| c.is_control()) {
        return Err(EpubError::InvalidStructure(format!(
            "Control character or NUL byte rejected in OPF href: '{raw_href}'"
        )));
    }
    let without_fragment = raw_href.split('#').next().unwrap_or("");
    let path_part = without_fragment.split('?').next().unwrap_or("").trim();
    if path_part.is_empty() {
        return Ok(());
    }

    let decoded_raw = percent_decode_utf8(path_part)?;
    if decoded_raw.contains('\0') || decoded_raw.chars().any(|c| c.is_control()) {
        return Err(EpubError::InvalidStructure(format!(
            "Decoded control character or NUL byte rejected in OPF href: '{raw_href}'"
        )));
    }
    let decoded = decoded_raw.trim();

    let lower = decoded.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return Ok(());
    }
    if has_uri_scheme(path_part) || has_uri_scheme(decoded) {
        return Err(EpubError::InvalidStructure(format!(
            "Disallowed URI scheme in OPF href: '{raw_href}'"
        )));
    }
    if path_part.starts_with('/')
        || path_part.starts_with('\\')
        || path_part.contains('\\')
        || decoded.starts_with('/')
        || decoded.starts_with('\\')
        || decoded.contains('\\')
        || has_drive_prefix(path_part)
        || has_drive_prefix(decoded)
    {
        return Err(EpubError::InvalidStructure(format!(
            "Absolute, backslash, or drive-prefixed path rejected in OPF href: '{raw_href}'"
        )));
    }

    let mut resolved: Vec<&str> = opf_dir_segments.to_vec();
    for seg in decoded.split('/') {
        if seg.is_empty() {
            return Err(EpubError::InvalidStructure(format!(
                "Empty path segment rejected in OPF href: '{raw_href}'"
            )));
        }
        if seg == "." {
            continue;
        }
        if seg == ".." {
            if resolved.pop().is_none() {
                return Err(EpubError::InvalidStructure(format!(
                    "Path traversal detected in OPF href '{raw_href}' relative to '{opf_path}'"
                )));
            }
            continue;
        }
        if has_drive_prefix(seg) {
            return Err(EpubError::InvalidStructure(format!(
                "Drive prefix rejected in OPF href: '{raw_href}'"
            )));
        }
        resolved.push(seg);
    }

    if resolved.is_empty() {
        return Err(EpubError::InvalidStructure(format!(
            "OPF href '{raw_href}' resolves to container root"
        )));
    }

    Ok(())
}

fn lookup_opf_path_from_container_xml(xml: &str) -> ContainerOpfLookup {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    loop {
        match reader.read_event() {
            Ok(Event::Empty(ref e)) | Ok(Event::Start(ref e)) => {
                let local_name = e.local_name();
                if local_name.as_ref() == "rootfile" {
                    for attr in e.attributes() {
                        let attr = match attr {
                            Ok(attr) => attr,
                            Err(err) => {
                                return ContainerOpfLookup::XmlError(EpubError::Xml(format!(
                                    "Failed to parse container.xml attribute: {err}"
                                )));
                            }
                        };
                        if attr.key.as_ref() == "full-path" {
                            let val = match quick_xml::escape::unescape(&attr.value) {
                                Ok(val) => val,
                                Err(err) => {
                                    return ContainerOpfLookup::XmlError(EpubError::Xml(format!(
                                        "Failed to unescape full-path attribute: {err}"
                                    )));
                                }
                            };
                            if !val.is_empty() {
                                return ContainerOpfLookup::Found(val.into_owned());
                            }
                        }
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(err) => {
                return ContainerOpfLookup::XmlError(EpubError::Xml(format!(
                    "XML error in container.xml: {err}"
                )));
            }
            _ => {}
        }
    }

    ContainerOpfLookup::MissingAttribute
}

fn read_zip_entry_bytes<R: Read>(
    entry: &mut zip::read::ZipFile<'_, R>,
    entry_name: &str,
    max_bytes: u64,
    total_decompressed: &mut u64,
) -> Result<Vec<u8>, EpubError> {
    let remaining_budget = MAX_TOTAL_DECOMPRESSED_BYTES.saturating_sub(*total_decompressed);
    if entry.size() > max_bytes {
        return Err(EpubError::InvalidStructure(format!(
            "ZIP entry '{entry_name}' declared size {} bytes exceeds maximum allowed ({max_bytes} bytes)",
            entry.size()
        )));
    }
    if entry.size() > remaining_budget {
        return Err(EpubError::InvalidStructure(format!(
            "ZIP entry '{entry_name}' declared size {} bytes would exceed total decompressed limit ({MAX_TOTAL_DECOMPRESSED_BYTES} bytes)",
            entry.size()
        )));
    }

    let effective_max = max_bytes.min(remaining_budget);
    let initial_cap =
        usize::try_from(entry.size().min(INITIAL_ALLOC_CAP as u64)).unwrap_or(INITIAL_ALLOC_CAP);
    let mut data = Vec::with_capacity(initial_cap);
    entry
        .take(effective_max.saturating_add(1))
        .read_to_end(&mut data)
        .map_err(EpubError::Io)?;

    let actual_len = data.len() as u64;
    if actual_len > max_bytes {
        return Err(EpubError::InvalidStructure(format!(
            "ZIP entry '{entry_name}' decompressed size exceeds maximum allowed ({max_bytes} bytes)"
        )));
    }
    if actual_len > remaining_budget {
        return Err(EpubError::InvalidStructure(format!(
            "Total decompressed ZIP size exceeds maximum allowed ({MAX_TOTAL_DECOMPRESSED_BYTES} bytes)"
        )));
    }
    if actual_len != entry.size() {
        return Err(EpubError::InvalidStructure(format!(
            "ZIP entry '{entry_name}' decompressed size ({actual_len} bytes) does not match declared header size ({} bytes)",
            entry.size()
        )));
    }

    let next_total = total_decompressed.checked_add(actual_len).ok_or_else(|| {
        EpubError::InvalidStructure(
            "Total decompressed ZIP size overflowed maximum allowed".to_string(),
        )
    })?;
    *total_decompressed = next_total;

    Ok(data)
}

fn read_zip_entry_string<R: Read>(
    entry: &mut zip::read::ZipFile<'_, R>,
    entry_name: &str,
    max_bytes: u64,
    total_decompressed: &mut u64,
) -> Result<String, EpubError> {
    let bytes = read_zip_entry_bytes(entry, entry_name, max_bytes, total_decompressed)?;
    String::from_utf8(bytes).map_err(|e| {
        EpubError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("ZIP entry '{entry_name}' is not valid UTF-8: {e}"),
        ))
    })
}

fn is_html_entry(path: &str) -> bool {
    let filename = path.rsplit('/').next().unwrap_or(path);
    let Some((_, ext)) = filename.rsplit_once('.') else {
        return false;
    };
    ext.eq_ignore_ascii_case("xhtml")
        || ext.eq_ignore_ascii_case("html")
        || ext.eq_ignore_ascii_case("htm")
}

pub fn extract_tag_content(xml: &str, tag_name: &str) -> Option<String> {
    let target = tag_name;
    let target_local = if let Some(pos) = tag_name.find(':') {
        &tag_name[pos + 1..]
    } else {
        tag_name
    };

    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut inside_tag = false;
    let mut collected_text = String::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => {
                let name = e.name();
                let local = e.local_name();
                if name.as_ref() == target || local.as_ref() == target_local {
                    inside_tag = true;
                    collected_text.clear();
                }
            }
            Ok(Event::Text(ref e)) if inside_tag => {
                if let Ok(text) = quick_xml::escape::unescape(e.as_ref()) {
                    collected_text.push_str(&text);
                }
            }
            Ok(Event::CData(ref e)) if inside_tag => {
                collected_text.push_str(e.as_ref());
            }
            Ok(Event::End(ref e)) => {
                let name = e.name();
                let local = e.local_name();
                if inside_tag && (name.as_ref() == target || local.as_ref() == target_local) {
                    let trimmed = collected_text.trim();
                    if !trimmed.is_empty() {
                        return Some(trimmed.to_string());
                    }
                    inside_tag = false;
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => return None,
            _ => {}
        }
    }

    None
}

#[async_trait]
impl EpubPort for ZipEpubAdapter {
    async fn read_epub(&self, path: &Path) -> Result<EpubBook, EpubError> {
        let path = path.to_path_buf();
        tokio::task::spawn_blocking(move || Self::read_epub_sync(&path))
            .await
            .map_err(|e| {
                EpubError::Io(std::io::Error::other(format!("Task execution failed: {e}")))
            })?
    }

    async fn write_epub(&self, book: &EpubBook, path: &Path) -> Result<(), EpubError> {
        let book = book.clone();
        let path = path.to_path_buf();
        tokio::task::spawn_blocking(move || Self::write_epub_sync(&book, &path))
            .await
            .map_err(|e| {
                EpubError::Io(std::io::Error::other(format!("Task execution failed: {e}")))
            })?
    }

    async fn verify_epub(&self, path: &Path) -> Result<EpubVerificationReport, EpubError> {
        let path = path.to_path_buf();
        tokio::task::spawn_blocking(move || Self::verify_epub_sync(&path))
            .await
            .map_err(|e| {
                EpubError::Io(std::io::Error::other(format!("Task execution failed: {e}")))
            })?
    }
}
