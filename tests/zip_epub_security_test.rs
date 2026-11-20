use epub_translator_google::adapters::epub::ZipEpubAdapter;
use epub_translator_google::domain::entities::{EpubBook, EpubChapter, EpubMetadata, EpubResource};
use epub_translator_google::domain::errors::EpubError;
use epub_translator_google::domain::ports::EpubPort;
use std::fs::{self, File};
use std::io::{BufWriter, Cursor, Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

static TEST_DIR_SEQ: AtomicU64 = AtomicU64::new(0);

struct TestTempDir {
    path: PathBuf,
}

impl TestTempDir {
    fn new(prefix: &str) -> Self {
        let seq = TEST_DIR_SEQ.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let path = std::env::temp_dir().join(format!("epub_sec_{prefix}_{pid}_{seq}"));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    fn file(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }

    fn entries(&self) -> Vec<String> {
        fs::read_dir(&self.path)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter_map(|e| e.file_name().into_string().ok())
            .collect()
    }
}

impl Drop for TestTempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

const VALID_CONTAINER_XML: &str = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#;

const VALID_OPF_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="pub-id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>Security Test Book</dc:title>
    <dc:language>en</dc:language>
  </metadata>
  <manifest>
    <item id="c1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine>
    <itemref idref="c1"/>
  </spine>
</package>"#;

const VALID_CHAPTER_XHTML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml">
  <head><title>Chapter 1</title></head>
  <body><p>Valid chapter content.</p></body>
</html>"#;

fn fast_deflated_options() -> SimpleFileOptions {
    SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .compression_level(Some(1))
}

fn write_minimal_epub_with_extra<F>(path: &Path, extra: F)
where
    F: FnOnce(&mut ZipWriter<BufWriter<File>>),
{
    let file = File::create(path).unwrap();
    let mut zip = ZipWriter::new(BufWriter::new(file));
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let deflated = fast_deflated_options();

    zip.start_file("mimetype", stored).unwrap();
    zip.write_all(b"application/epub+zip").unwrap();

    zip.start_file("META-INF/container.xml", deflated).unwrap();
    zip.write_all(VALID_CONTAINER_XML.as_bytes()).unwrap();

    zip.start_file("OEBPS/content.opf", deflated).unwrap();
    zip.write_all(VALID_OPF_XML.as_bytes()).unwrap();

    zip.start_file("OEBPS/chapter1.xhtml", deflated).unwrap();
    zip.write_all(VALID_CHAPTER_XHTML.as_bytes()).unwrap();

    extra(&mut zip);
    zip.finish().unwrap();
}

fn write_custom_epub(
    path: &Path,
    mimetype_bytes: &[u8],
    container_xml: &str,
    opf_path: &str,
    opf_xml: &str,
    chapter_path: &str,
    chapter_xml: &str,
) {
    let file = File::create(path).unwrap();
    let mut zip = ZipWriter::new(BufWriter::new(file));
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let deflated = fast_deflated_options();

    zip.start_file("mimetype", stored).unwrap();
    zip.write_all(mimetype_bytes).unwrap();

    zip.start_file("META-INF/container.xml", deflated).unwrap();
    zip.write_all(container_xml.as_bytes()).unwrap();

    zip.start_file(opf_path, deflated).unwrap();
    zip.write_all(opf_xml.as_bytes()).unwrap();

    zip.start_file(chapter_path, deflated).unwrap();
    zip.write_all(chapter_xml.as_bytes()).unwrap();

    zip.finish().unwrap();
}

fn write_zero_stream<W: Write + Seek>(zip: &mut ZipWriter<W>, total_bytes: usize) {
    let chunk = vec![0u8; 1024 * 1024];
    let mut remaining = total_bytes;
    while remaining > 0 {
        let to_write = remaining.min(chunk.len());
        zip.write_all(&chunk[..to_write]).unwrap();
        remaining -= to_write;
    }
}

fn build_compressed_zero_archive(entry_size: usize) -> Vec<u8> {
    let mut buf = Cursor::new(Vec::new());
    {
        let mut zip = ZipWriter::new(&mut buf);
        zip.start_file("blob.bin", fast_deflated_options()).unwrap();
        write_zero_stream(&mut zip, entry_size);
        zip.finish().unwrap();
    }
    buf.into_inner()
}

fn patch_zip_uncompressed_size(path: &Path, entry_name: &str, spoofed_size: u32) {
    let mut data = fs::read(path).unwrap();
    let needle = entry_name.as_bytes();
    let mut patched_lfh = false;
    let mut patched_cdh = false;

    for i in 0..data.len().saturating_sub(30 + needle.len()) {
        if &data[i..i + 4] == b"PK\x03\x04" {
            let name_len = u16::from_le_bytes([data[i + 26], data[i + 27]]) as usize;
            if i + 30 + name_len <= data.len() && &data[i + 30..i + 30 + name_len] == needle {
                data[i + 22..i + 26].copy_from_slice(&spoofed_size.to_le_bytes());
                patched_lfh = true;
            }
        } else if &data[i..i + 4] == b"PK\x01\x02" && i + 46 + needle.len() <= data.len() {
            let name_len = u16::from_le_bytes([data[i + 28], data[i + 29]]) as usize;
            if i + 46 + name_len <= data.len() && &data[i + 46..i + 46 + name_len] == needle {
                data[i + 24..i + 28].copy_from_slice(&spoofed_size.to_le_bytes());
                patched_cdh = true;
            }
        }
    }

    assert!(patched_lfh && patched_cdh);
    let mut file = File::create(path).unwrap();
    file.seek(std::io::SeekFrom::Start(0)).unwrap();
    file.write_all(&data).unwrap();
}

#[tokio::test]
async fn test_read_epub_rejects_zip_bomb_entry_above_max_entry_bytes() {
    let dir = TestTempDir::new("bomb_entry_read");
    let epub_path = dir.file("bomb.epub");
    let oversized = ZipEpubAdapter::MAX_ENTRY_BYTES as usize + 1;

    write_minimal_epub_with_extra(&epub_path, |zip| {
        zip.start_file("OEBPS/bomb.bin", fast_deflated_options())
            .unwrap();
        write_zero_stream(zip, oversized);
    });

    let err = ZipEpubAdapter::new()
        .read_epub(&epub_path)
        .await
        .unwrap_err();
    assert!(matches!(err, EpubError::InvalidStructure(_)));
}

#[tokio::test]
async fn test_verify_epub_rejects_zip_bomb_xhtml_above_max_entry_bytes() {
    let dir = TestTempDir::new("bomb_xhtml_verify");
    let epub_path = dir.file("bomb_xhtml.epub");
    let oversized = ZipEpubAdapter::MAX_ENTRY_BYTES as usize + 1;

    write_minimal_epub_with_extra(&epub_path, |zip| {
        zip.start_file("OEBPS/bomb.xhtml", fast_deflated_options())
            .unwrap();
        write_zero_stream(zip, oversized);
    });

    let err = ZipEpubAdapter::new()
        .verify_epub(&epub_path)
        .await
        .unwrap_err();
    assert!(matches!(err, EpubError::InvalidStructure(_)));
}

#[tokio::test]
async fn test_read_and_verify_metadata_boundary_at_and_above_max_metadata_bytes() {
    let dir = TestTempDir::new("meta_bva");
    let exact_path = dir.file("exact_meta.epub");
    let over_path = dir.file("over_meta.epub");
    let limit = ZipEpubAdapter::MAX_METADATA_BYTES as usize;

    let mut exact_opf = String::with_capacity(limit);
    exact_opf.push_str(VALID_OPF_XML);
    exact_opf.extend(std::iter::repeat_n(' ', limit - VALID_OPF_XML.len()));
    assert_eq!(exact_opf.len(), limit);

    write_custom_epub(
        &exact_path,
        b"application/epub+zip",
        VALID_CONTAINER_XML,
        "OEBPS/content.opf",
        &exact_opf,
        "OEBPS/chapter1.xhtml",
        VALID_CHAPTER_XHTML,
    );

    let mut over_opf = exact_opf;
    over_opf.push(' ');
    assert_eq!(over_opf.len(), limit + 1);

    write_custom_epub(
        &over_path,
        b"application/epub+zip",
        VALID_CONTAINER_XML,
        "OEBPS/content.opf",
        &over_opf,
        "OEBPS/chapter1.xhtml",
        VALID_CHAPTER_XHTML,
    );

    let adapter = ZipEpubAdapter::new();
    let book = adapter.read_epub(&exact_path).await.unwrap();
    assert_eq!(book.metadata.title.as_deref(), Some("Security Test Book"));
    let report = adapter.verify_epub(&exact_path).await.unwrap();
    assert!(report.is_valid, "{:?}", report.errors);

    assert!(matches!(
        adapter.read_epub(&over_path).await.unwrap_err(),
        EpubError::InvalidStructure(_)
    ));
    assert!(matches!(
        adapter.verify_epub(&over_path).await.unwrap_err(),
        EpubError::InvalidStructure(_)
    ));
}

#[tokio::test]
async fn test_read_and_verify_reject_oversized_container_xml() {
    let dir = TestTempDir::new("bomb_container");
    let epub_path = dir.file("oversized_container.epub");
    let oversized = ZipEpubAdapter::MAX_METADATA_BYTES as usize + 1;

    let mut big_container = String::with_capacity(oversized);
    big_container.push_str(VALID_CONTAINER_XML);
    big_container.extend(std::iter::repeat_n(
        ' ',
        oversized - VALID_CONTAINER_XML.len(),
    ));

    write_custom_epub(
        &epub_path,
        b"application/epub+zip",
        &big_container,
        "OEBPS/content.opf",
        VALID_OPF_XML,
        "OEBPS/chapter1.xhtml",
        VALID_CHAPTER_XHTML,
    );

    let adapter = ZipEpubAdapter::new();
    assert!(matches!(
        adapter.read_epub(&epub_path).await.unwrap_err(),
        EpubError::InvalidStructure(_)
    ));
    assert!(matches!(
        adapter.verify_epub(&epub_path).await.unwrap_err(),
        EpubError::InvalidStructure(_)
    ));
}

#[tokio::test]
async fn test_read_epub_rejects_total_decompressed_limit_across_multiple_entries() {
    let dir = TestTempDir::new("bomb_total");
    let epub_path = dir.file("bomb_total.epub");
    let template_bytes = build_compressed_zero_archive(60 * 1024 * 1024);

    write_minimal_epub_with_extra(&epub_path, |zip| {
        let mut template = ZipArchive::new(Cursor::new(&template_bytes)).unwrap();
        for idx in 0..9 {
            let entry = template.by_index(0).unwrap();
            zip.raw_copy_file_rename(entry, format!("OEBPS/blob_{idx}.bin"))
                .unwrap();
        }
    });

    let err = ZipEpubAdapter::new()
        .read_epub(&epub_path)
        .await
        .unwrap_err();
    assert!(matches!(err, EpubError::InvalidStructure(_)));
}

#[tokio::test]
async fn test_read_and_verify_entry_count_boundary_at_and_above_max_zip_entries() {
    let dir = TestTempDir::new("entries_bva");
    let at_limit_path = dir.file("at_limit.epub");
    let over_limit_path = dir.file("over_limit.epub");

    write_minimal_epub_with_extra(&at_limit_path, |zip| {
        let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        for idx in 4..ZipEpubAdapter::MAX_ZIP_ENTRIES {
            zip.start_file(format!("OEBPS/e_{idx}.txt"), stored)
                .unwrap();
        }
    });

    write_minimal_epub_with_extra(&over_limit_path, |zip| {
        let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        for idx in 4..=ZipEpubAdapter::MAX_ZIP_ENTRIES {
            zip.start_file(format!("OEBPS/e_{idx}.txt"), stored)
                .unwrap();
        }
    });

    let adapter = ZipEpubAdapter::new();
    let book = adapter.read_epub(&at_limit_path).await.unwrap();
    assert_eq!(book.chapters.len(), 1);
    let report = adapter.verify_epub(&at_limit_path).await.unwrap();
    assert!(report.is_valid, "{:?}", report.errors);

    assert!(matches!(
        adapter.read_epub(&over_limit_path).await.unwrap_err(),
        EpubError::InvalidStructure(_)
    ));
    assert!(matches!(
        adapter.verify_epub(&over_limit_path).await.unwrap_err(),
        EpubError::InvalidStructure(_)
    ));
}

#[tokio::test]
async fn test_read_epub_handles_spoofed_large_header_sizes_without_oom() {
    let dir = TestTempDir::new("spoof_large");
    let above_max_path = dir.file("spoof_4gb.epub");
    let below_max_path = dir.file("spoof_32mb.epub");

    write_minimal_epub_with_extra(&above_max_path, |zip| {
        zip.start_file("OEBPS/tiny.bin", fast_deflated_options())
            .unwrap();
        zip.write_all(b"small-data").unwrap();
    });
    patch_zip_uncompressed_size(&above_max_path, "OEBPS/tiny.bin", u32::MAX);

    write_minimal_epub_with_extra(&below_max_path, |zip| {
        zip.start_file("OEBPS/tiny.bin", fast_deflated_options())
            .unwrap();
        zip.write_all(b"small-data").unwrap();
    });
    patch_zip_uncompressed_size(&below_max_path, "OEBPS/tiny.bin", 32 * 1024 * 1024);

    let adapter = ZipEpubAdapter::new();
    assert!(matches!(
        adapter.read_epub(&above_max_path).await.unwrap_err(),
        EpubError::InvalidStructure(_)
    ));
    assert!(adapter.read_epub(&below_max_path).await.is_err());
}

#[tokio::test]
async fn test_read_epub_rejects_spoofed_small_header_size_on_large_stream() {
    let dir = TestTempDir::new("spoof_small");
    let epub_path = dir.file("spoof_small.epub");

    write_minimal_epub_with_extra(&epub_path, |zip| {
        zip.start_file("OEBPS/spoofed.bin", fast_deflated_options())
            .unwrap();
        write_zero_stream(zip, 1024 * 1024);
    });
    patch_zip_uncompressed_size(&epub_path, "OEBPS/spoofed.bin", 10);

    let err = ZipEpubAdapter::new()
        .read_epub(&epub_path)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        EpubError::InvalidStructure(_) | EpubError::Io(_)
    ));
}

#[tokio::test]
async fn test_verify_epub_rejects_oversized_mimetype_without_reading_payload() {
    let dir = TestTempDir::new("mimetype_large");
    let epub_path = dir.file("oversized_mimetype.epub");
    let mut big_mime = b"application/epub+zip".to_vec();
    big_mime.resize(2 * 1024 * 1024, b'X');

    write_custom_epub(
        &epub_path,
        &big_mime,
        VALID_CONTAINER_XML,
        "OEBPS/content.opf",
        VALID_OPF_XML,
        "OEBPS/chapter1.xhtml",
        VALID_CHAPTER_XHTML,
    );

    let report = ZipEpubAdapter::new().verify_epub(&epub_path).await.unwrap();
    assert!(!report.is_valid);
    assert!(!report.mimetype_valid);
    assert!(report.errors.iter().any(|e| e.contains("20 bytes")));
}

#[tokio::test]
async fn test_verify_epub_rejects_mimetype_21_bytes() {
    let dir = TestTempDir::new("mimetype_21");
    let epub_path = dir.file("mimetype_21.epub");

    write_custom_epub(
        &epub_path,
        b"application/epub+zipX",
        VALID_CONTAINER_XML,
        "OEBPS/content.opf",
        VALID_OPF_XML,
        "OEBPS/chapter1.xhtml",
        VALID_CHAPTER_XHTML,
    );

    let report = ZipEpubAdapter::new().verify_epub(&epub_path).await.unwrap();
    assert!(!report.is_valid);
    assert!(!report.mimetype_valid);
}

#[tokio::test]
async fn test_verify_epub_rejects_spoofed_mimetype_size_20_with_larger_stream() {
    let dir = TestTempDir::new("mimetype_spoof");
    let epub_path = dir.file("mimetype_spoof.epub");

    write_custom_epub(
        &epub_path,
        b"application/epub+zipEXTRA",
        VALID_CONTAINER_XML,
        "OEBPS/content.opf",
        VALID_OPF_XML,
        "OEBPS/chapter1.xhtml",
        VALID_CHAPTER_XHTML,
    );
    patch_zip_uncompressed_size(&epub_path, "mimetype", 20);

    let err = ZipEpubAdapter::new()
        .verify_epub(&epub_path)
        .await
        .unwrap_err();
    assert!(matches!(err, EpubError::InvalidStructure(_)));
}

#[tokio::test]
async fn test_read_and_verify_reject_path_traversal_entries() {
    let malicious_names = [
        "../evil.xhtml",
        "OEBPS/../../etc/passwd",
        "OEBPS/../evil.xhtml",
        "..\\..\\evil.txt",
        "/etc/passwd",
        "\\Windows\\win.ini",
        "C:\\evil.txt",
        "C:/evil.txt",
        "OEBPS/D:stream.xhtml",
    ];

    let adapter = ZipEpubAdapter::new();
    for (idx, bad_name) in malicious_names.iter().enumerate() {
        let dir = TestTempDir::new(&format!("zip_slip_{idx}"));
        let epub_path = dir.file("traversal.epub");

        write_minimal_epub_with_extra(&epub_path, |zip| {
            zip.start_file(*bad_name, fast_deflated_options()).unwrap();
            zip.write_all(b"payload").unwrap();
        });

        let read_err = adapter.read_epub(&epub_path).await.unwrap_err();
        assert!(
            matches!(read_err, EpubError::InvalidStructure(_)),
            "read_epub must reject '{bad_name}', got {read_err:?}"
        );

        let verify_err = adapter.verify_epub(&epub_path).await.unwrap_err();
        assert!(
            matches!(verify_err, EpubError::InvalidStructure(_)),
            "verify_epub must reject '{bad_name}', got {verify_err:?}"
        );
    }
}

#[tokio::test]
async fn test_read_and_verify_reject_container_xml_full_path_traversal() {
    let malicious_full_paths = [
        "../evil.opf",
        "OEBPS/../../evil.opf",
        "/etc/passwd",
        "C:\\evil.opf",
        "%2e%2e/evil.opf",
    ];

    let adapter = ZipEpubAdapter::new();
    for (idx, bad_opf_path) in malicious_full_paths.iter().enumerate() {
        let container_xml = format!(
            r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="{bad_opf_path}" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#
        );

        assert!(matches!(
            ZipEpubAdapter::find_opf_path_from_container_xml(&container_xml).unwrap_err(),
            EpubError::InvalidStructure(_)
        ));

        let dir = TestTempDir::new(&format!("container_trav_{idx}"));
        let epub_path = dir.file("container_traversal.epub");
        write_custom_epub(
            &epub_path,
            b"application/epub+zip",
            &container_xml,
            "OEBPS/content.opf",
            VALID_OPF_XML,
            "OEBPS/chapter1.xhtml",
            VALID_CHAPTER_XHTML,
        );

        assert!(matches!(
            adapter.read_epub(&epub_path).await.unwrap_err(),
            EpubError::InvalidStructure(_)
        ));
        assert!(matches!(
            adapter.verify_epub(&epub_path).await.unwrap_err(),
            EpubError::InvalidStructure(_)
        ));
    }
}

#[tokio::test]
async fn test_read_and_verify_reject_opf_href_traversal() {
    let malicious_hrefs = [
        "../../etc/passwd",
        "/etc/passwd",
        "C:\\Windows\\win.ini",
        "C:/Windows/win.ini",
        "file:///etc/passwd",
        "%2e%2e/%2e%2e/etc/passwd",
        "..%5c..%5cevil.xhtml",
    ];

    let adapter = ZipEpubAdapter::new();
    for (idx, bad_href) in malicious_hrefs.iter().enumerate() {
        let opf_xml = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>Test</dc:title>
  </metadata>
  <manifest>
    <item id="c1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>
    <item id="evil" href="{bad_href}" media-type="text/plain"/>
  </manifest>
  <spine><itemref idref="c1"/></spine>
</package>"#
        );

        let dir = TestTempDir::new(&format!("opf_href_trav_{idx}"));
        let epub_path = dir.file("opf_traversal.epub");
        write_custom_epub(
            &epub_path,
            b"application/epub+zip",
            VALID_CONTAINER_XML,
            "OEBPS/content.opf",
            &opf_xml,
            "OEBPS/chapter1.xhtml",
            VALID_CHAPTER_XHTML,
        );

        let read_err = adapter.read_epub(&epub_path).await.unwrap_err();
        assert!(
            matches!(read_err, EpubError::InvalidStructure(_)),
            "read_epub must reject OPF href '{bad_href}', got {read_err:?}"
        );

        let verify_err = adapter.verify_epub(&epub_path).await.unwrap_err();
        assert!(
            matches!(verify_err, EpubError::InvalidStructure(_)),
            "verify_epub must reject OPF href '{bad_href}', got {verify_err:?}"
        );
    }
}

#[tokio::test]
async fn test_read_and_verify_allow_valid_relative_opf_href_within_container() {
    let dir = TestTempDir::new("opf_href_valid");
    let epub_path = dir.file("valid_nested.epub");

    let container_xml = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/opf/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#;

    let opf_xml = r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>Nested Book</dc:title>
    <link rel="dcterms:conformsTo" href="https://example.org/epub-a11y"/>
  </metadata>
  <manifest>
    <item id="c1" href="../xhtml/chapter1.xhtml#start" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="c1"/></spine>
</package>"#;

    write_custom_epub(
        &epub_path,
        b"application/epub+zip",
        container_xml,
        "OEBPS/opf/content.opf",
        opf_xml,
        "OEBPS/xhtml/chapter1.xhtml",
        VALID_CHAPTER_XHTML,
    );

    let adapter = ZipEpubAdapter::new();
    let book = adapter.read_epub(&epub_path).await.unwrap();
    assert_eq!(book.chapters.len(), 1);
    assert_eq!(book.chapters[0].path, "OEBPS/xhtml/chapter1.xhtml");

    let report = adapter.verify_epub(&epub_path).await.unwrap();
    assert!(report.is_valid, "{:?}", report.errors);
}

#[tokio::test]
async fn test_write_epub_atomic_in_place_overwrite_succeeds_and_leaves_no_temp_files() {
    let dir = TestTempDir::new("atomic_inplace");
    let epub_path = dir.file("book.epub");
    write_minimal_epub_with_extra(&epub_path, |_| {});

    let adapter = ZipEpubAdapter::new();
    let mut book = adapter.read_epub(&epub_path).await.unwrap();
    book.chapters[0].content = book.chapters[0]
        .content
        .replace("Valid chapter content.", "Updated atomic content.");

    adapter.write_epub(&book, &epub_path).await.unwrap();

    assert_eq!(dir.entries(), vec!["book.epub".to_string()]);
    let report = adapter.verify_epub(&epub_path).await.unwrap();
    assert!(report.is_valid, "{:?}", report.errors);

    let reloaded = adapter.read_epub(&epub_path).await.unwrap();
    assert!(
        reloaded.chapters[0]
            .content
            .contains("Updated atomic content.")
    );
}

#[tokio::test]
async fn test_write_epub_atomic_preserves_existing_file_on_error_and_cleans_temp() {
    let dir = TestTempDir::new("atomic_rollback");
    let epub_path = dir.file("target.epub");
    write_minimal_epub_with_extra(&epub_path, |_| {});
    let mut original_bytes = Vec::new();
    File::open(&epub_path)
        .unwrap()
        .read_to_end(&mut original_bytes)
        .unwrap();

    let invalid_book = EpubBook {
        metadata: EpubMetadata::default(),
        chapters: vec![EpubChapter {
            path: "OEBPS/chapter1.xhtml".to_string(),
            content: "   \n  ".to_string(),
            media_type: "application/xhtml+xml".to_string(),
        }],
        resources: vec![EpubResource {
            path: "OEBPS/res.txt".to_string(),
            data: b"ok".to_vec(),
        }],
        raw_container_xml: Some(VALID_CONTAINER_XML.to_string()),
        raw_opf_content: Some(VALID_OPF_XML.to_string()),
        opf_path: "OEBPS/content.opf".to_string(),
    };

    let err = ZipEpubAdapter::new()
        .write_epub(&invalid_book, &epub_path)
        .await
        .unwrap_err();
    assert!(matches!(err, EpubError::InvalidStructure(_)));

    let mut current_bytes = Vec::new();
    File::open(&epub_path)
        .unwrap()
        .read_to_end(&mut current_bytes)
        .unwrap();
    assert_eq!(current_bytes, original_bytes);
    assert_eq!(dir.entries(), vec!["target.epub".to_string()]);
}

#[tokio::test]
async fn test_write_epub_rejects_unsafe_path_without_modifying_destination() {
    let dir = TestTempDir::new("write_unsafe_path");
    let epub_path = dir.file("existing.epub");
    fs::write(&epub_path, b"sentinel-bytes").unwrap();

    let unsafe_book = EpubBook {
        metadata: EpubMetadata::default(),
        chapters: vec![EpubChapter {
            path: "../escape.xhtml".to_string(),
            content: VALID_CHAPTER_XHTML.to_string(),
            media_type: "application/xhtml+xml".to_string(),
        }],
        resources: Vec::new(),
        raw_container_xml: Some(VALID_CONTAINER_XML.to_string()),
        raw_opf_content: Some(VALID_OPF_XML.to_string()),
        opf_path: "OEBPS/content.opf".to_string(),
    };

    let err = ZipEpubAdapter::new()
        .write_epub(&unsafe_book, &epub_path)
        .await
        .unwrap_err();
    assert!(matches!(err, EpubError::InvalidStructure(_)));
    assert_eq!(fs::read(&epub_path).unwrap(), b"sentinel-bytes");
    assert_eq!(dir.entries(), vec!["existing.epub".to_string()]);
}

#[tokio::test]
async fn test_read_and_verify_reject_malformed_opf_with_traversal() {
    let dir = TestTempDir::new("malformed_opf_trav");
    let epub_path = dir.file("malformed_opf.epub");
    let malformed_opf = r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata><x></y></metadata>
  <manifest>
    <item id="evil" href="../../etc/passwd" media-type="text/plain"/>
  </manifest>
</package>"#;

    write_custom_epub(
        &epub_path,
        b"application/epub+zip",
        VALID_CONTAINER_XML,
        "OEBPS/content.opf",
        malformed_opf,
        "OEBPS/chapter1.xhtml",
        VALID_CHAPTER_XHTML,
    );

    let adapter = ZipEpubAdapter::new();
    assert!(matches!(
        adapter.read_epub(&epub_path).await.unwrap_err(),
        EpubError::Xml(_)
    ));
    assert!(matches!(
        adapter.verify_epub(&epub_path).await.unwrap_err(),
        EpubError::Xml(_)
    ));
}

#[tokio::test]
async fn test_read_and_verify_reject_xlink_href_and_disallowed_uri_schemes() {
    let bad_item_attrs = [
        r#"xlink:href="../../etc/passwd""#,
        r#"xlink:href="../../etc/passwd" href="javascript:alert(1)""#,
        r#"href="javascript:alert(1)""#,
        r#"href="JaVaScRiPt%3aalert(1)""#,
        r#"href="java&#x09;script:alert(1)""#,
        r#"href="vbscript:msgbox(1)""#,
        r#"href="data:text/html,&lt;script&gt;alert(1)&lt;/script&gt;""#,
        r#"href="http:relative-trick.xhtml""#,
    ];

    let adapter = ZipEpubAdapter::new();
    for (idx, item_attrs) in bad_item_attrs.iter().enumerate() {
        let opf_xml = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" xmlns:xlink="http://www.w3.org/1999/xlink" version="3.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>Test</dc:title>
  </metadata>
  <manifest>
    <item id="c1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>
    <item id="bad" {item_attrs} media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="c1"/></spine>
</package>"#
        );

        let dir = TestTempDir::new(&format!("opf_scheme_{idx}"));
        let epub_path = dir.file("scheme_or_xlink.epub");
        write_custom_epub(
            &epub_path,
            b"application/epub+zip",
            VALID_CONTAINER_XML,
            "OEBPS/content.opf",
            &opf_xml,
            "OEBPS/chapter1.xhtml",
            VALID_CHAPTER_XHTML,
        );

        let read_err = adapter.read_epub(&epub_path).await.unwrap_err();
        assert!(
            matches!(read_err, EpubError::InvalidStructure(_)),
            "read_epub must reject OPF attrs '{item_attrs}', got {read_err:?}"
        );

        let verify_err = adapter.verify_epub(&epub_path).await.unwrap_err();
        assert!(
            matches!(verify_err, EpubError::InvalidStructure(_)),
            "verify_epub must reject OPF attrs '{item_attrs}', got {verify_err:?}"
        );
    }
}

#[tokio::test]
async fn test_write_epub_rejects_chapter_empty_after_sanitization_without_creating_or_corrupting_dest()
 {
    let dir = TestTempDir::new("post_heal_empty");
    let new_dest = dir.file("not_created.epub");
    let existing_dest = dir.file("existing.epub");
    fs::write(&existing_dest, b"intact-epub-bytes").unwrap();

    let script_only_book = EpubBook {
        metadata: EpubMetadata::default(),
        chapters: vec![EpubChapter {
            path: "OEBPS/chapter1.xhtml".to_string(),
            content: "<script>alert(1)</script>".to_string(),
            media_type: "application/xhtml+xml".to_string(),
        }],
        resources: Vec::new(),
        raw_container_xml: Some(VALID_CONTAINER_XML.to_string()),
        raw_opf_content: Some(VALID_OPF_XML.to_string()),
        opf_path: "OEBPS/content.opf".to_string(),
    };

    let adapter = ZipEpubAdapter::new();
    let err_new = adapter
        .write_epub(&script_only_book, &new_dest)
        .await
        .unwrap_err();
    assert!(matches!(err_new, EpubError::InvalidStructure(_)));
    assert!(!new_dest.exists());

    let err_existing = adapter
        .write_epub(&script_only_book, &existing_dest)
        .await
        .unwrap_err();
    assert!(matches!(err_existing, EpubError::InvalidStructure(_)));
    assert_eq!(fs::read(&existing_dest).unwrap(), b"intact-epub-bytes");
    assert_eq!(dir.entries(), vec!["existing.epub".to_string()]);
}

#[tokio::test]
async fn test_read_and_verify_reject_percent_encoded_zip_entry_traversal() {
    let encoded_names = [
        "%2e%2e/evil.xhtml",
        "OEBPS/%2E%2E/evil.xhtml",
        "OEBPS/%2e%2E/evil.xhtml",
        "OEBPS/%2fetc/passwd",
        "OEBPS/%5cevil.txt",
    ];

    let adapter = ZipEpubAdapter::new();
    for (idx, bad_name) in encoded_names.iter().enumerate() {
        let dir = TestTempDir::new(&format!("zip_pct_{idx}"));
        let epub_path = dir.file("pct_traversal.epub");

        write_minimal_epub_with_extra(&epub_path, |zip| {
            zip.start_file(*bad_name, fast_deflated_options()).unwrap();
            zip.write_all(b"payload").unwrap();
        });

        assert!(matches!(
            adapter.read_epub(&epub_path).await.unwrap_err(),
            EpubError::InvalidStructure(_)
        ));
        assert!(matches!(
            adapter.verify_epub(&epub_path).await.unwrap_err(),
            EpubError::InvalidStructure(_)
        ));
    }
}

#[tokio::test]
async fn test_read_and_verify_reject_zero_byte_epub_file() {
    let dir = TestTempDir::new("zero_byte_epub");
    let epub_path = dir.file("empty.epub");
    File::create(&epub_path).unwrap();

    let adapter = ZipEpubAdapter::new();
    assert!(matches!(
        adapter.read_epub(&epub_path).await.unwrap_err(),
        EpubError::Zip(_)
    ));
    assert!(matches!(
        adapter.verify_epub(&epub_path).await.unwrap_err(),
        EpubError::Zip(_)
    ));
}

#[tokio::test]
async fn test_read_and_verify_handle_zero_byte_chapter_bva() {
    let dir = TestTempDir::new("zero_byte_chapter");
    let epub_path = dir.file("empty_chapter.epub");

    write_custom_epub(
        &epub_path,
        b"application/epub+zip",
        VALID_CONTAINER_XML,
        "OEBPS/content.opf",
        VALID_OPF_XML,
        "OEBPS/chapter1.xhtml",
        "",
    );

    let adapter = ZipEpubAdapter::new();
    let book = adapter.read_epub(&epub_path).await.unwrap();
    assert_eq!(book.chapters.len(), 1);
    assert!(book.chapters[0].content.is_empty());

    let report = adapter.verify_epub(&epub_path).await.unwrap();
    assert!(!report.is_valid);
    assert_eq!(report.total_xhtml_files, 1);
    assert_eq!(report.valid_xhtml_files, 0);
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.contains("chapter1.xhtml") && e.contains("0 bytes"))
    );

    let out_path = dir.file("out.epub");
    assert!(matches!(
        adapter.write_epub(&book, &out_path).await.unwrap_err(),
        EpubError::InvalidStructure(_)
    ));
    assert!(!out_path.exists());
}

#[tokio::test]
async fn test_read_and_verify_reject_container_xml_percent_2f_ff_and_dot() {
    let bad_full_paths = [
        "%2fetc%2fpasswd",
        "OEBPS%2fcontent.opf",
        "OEBPS/bad%FF.opf",
        ".",
        "./OEBPS/content.opf",
        "OEBPS/.",
    ];

    let adapter = ZipEpubAdapter::new();
    for (idx, bad_path) in bad_full_paths.iter().enumerate() {
        let container_xml = format!(
            r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="{bad_path}" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#
        );

        assert!(matches!(
            ZipEpubAdapter::find_opf_path_from_container_xml(&container_xml).unwrap_err(),
            EpubError::InvalidStructure(_)
        ));

        let dir = TestTempDir::new(&format!("container_bva_{idx}"));
        let epub_path = dir.file("container_bva.epub");
        write_custom_epub(
            &epub_path,
            b"application/epub+zip",
            &container_xml,
            "OEBPS/content.opf",
            VALID_OPF_XML,
            "OEBPS/chapter1.xhtml",
            VALID_CHAPTER_XHTML,
        );

        assert!(matches!(
            adapter.read_epub(&epub_path).await.unwrap_err(),
            EpubError::InvalidStructure(_)
        ));
        assert!(matches!(
            adapter.verify_epub(&epub_path).await.unwrap_err(),
            EpubError::InvalidStructure(_)
        ));
    }
}
