use epub_translator_google::adapters::epub::ZipEpubAdapter;
use epub_translator_google::domain::entities::Language;
use epub_translator_google::domain::ports::EpubPort;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Write};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

fn create_sample_epub(file_path: &std::path::Path) {
    let file = File::create(file_path).unwrap();
    let writer = BufWriter::new(file);
    let mut zip = ZipWriter::new(writer);

    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    zip.start_file("mimetype", stored).unwrap();
    zip.write_all(b"application/epub+zip").unwrap();

    let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    zip.start_file("META-INF/container.xml", deflated).unwrap();
    let container_xml = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#;
    zip.write_all(container_xml.as_bytes()).unwrap();

    zip.start_file("OEBPS/content.opf", deflated).unwrap();
    let opf_xml = r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="pub-id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>Sample Book Title</dc:title>
    <dc:language>en</dc:language>
    <dc:creator>Test Author</dc:creator>
  </metadata>
  <manifest>
    <item id="c1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>
    <item id="img" href="image.png" media-type="image/png"/>
  </manifest>
  <spine>
    <itemref idref="c1"/>
  </spine>
</package>"#;
    zip.write_all(opf_xml.as_bytes()).unwrap();

    zip.start_file("OEBPS/chapter1.xhtml", deflated).unwrap();
    let chapter_html = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml">
  <head><title>Chapter 1</title></head>
  <body>
    <h1>Chapter 1: The Beginning</h1>
    <p>This is a sample story in English.</p>
  </body>
</html>"#;
    zip.write_all(chapter_html.as_bytes()).unwrap();

    zip.start_file("OEBPS/image.png", deflated).unwrap();
    zip.write_all(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A])
        .unwrap();

    zip.finish().unwrap();
}

#[tokio::test]
async fn test_zip_epub_read_and_write() {
    let temp_dir = std::env::temp_dir();
    let input_path = temp_dir.join("test_sample_in.epub");
    let output_path = temp_dir.join("test_sample_out.epub");

    create_sample_epub(&input_path);

    let adapter = ZipEpubAdapter::new();
    let mut book = adapter.read_epub(&input_path).await.unwrap();

    assert_eq!(book.metadata.title.as_deref(), Some("Sample Book Title"));
    assert_eq!(
        book.metadata.language.as_ref().map(|l| l.as_str()),
        Some("en")
    );
    assert_eq!(book.chapters.len(), 1);
    assert_eq!(book.chapters[0].path, "OEBPS/chapter1.xhtml");
    assert!(book.chapters[0].content.contains("The Beginning"));
    assert_eq!(book.resources.len(), 2);

    book.chapters[0].content = book.chapters[0]
        .content
        .replace("The Beginning", "O Começo");
    book.metadata.language = Some(Language::new("pt-BR"));
    book.raw_opf_content = book.raw_opf_content.map(|opf| {
        opf.replace(
            "<dc:language>en</dc:language>",
            "<dc:language>pt-BR</dc:language>",
        )
    });

    adapter.write_epub(&book, &output_path).await.unwrap();

    let out_file = File::open(&output_path).unwrap();
    let mut out_zip = ZipArchive::new(BufReader::new(out_file)).unwrap();

    let mut first_entry = out_zip.by_index(0).unwrap();
    assert_eq!(first_entry.name().unwrap(), "mimetype");
    assert_eq!(first_entry.compression(), CompressionMethod::Stored);

    let mut mimetype_content = String::new();
    first_entry.read_to_string(&mut mimetype_content).unwrap();
    assert_eq!(mimetype_content, "application/epub+zip");
    drop(first_entry);

    let read_back_book = adapter.read_epub(&output_path).await.unwrap();
    assert_eq!(
        read_back_book
            .metadata
            .language
            .as_ref()
            .map(|l| l.as_str()),
        Some("pt-BR")
    );
    assert!(read_back_book.chapters[0].content.contains("O Começo"));

    let _ = fs::remove_file(input_path);
    let _ = fs::remove_file(output_path);
}

#[tokio::test]
async fn test_zip_epub_verify_valid_container() {
    let temp_dir = std::env::temp_dir();
    let epub_path = temp_dir.join("test_verify_valid.epub");
    create_sample_epub(&epub_path);

    let adapter = ZipEpubAdapter::new();
    let report = adapter.verify_epub(&epub_path).await.unwrap();

    assert!(report.is_valid);
    assert!(report.mimetype_valid);
    assert!(report.container_xml_valid);
    assert!(report.opf_valid);
    assert_eq!(report.total_xhtml_files, 1);
    assert_eq!(report.valid_xhtml_files, 1);
    assert!(report.errors.is_empty());

    let _ = fs::remove_file(epub_path);
}

#[tokio::test]
async fn test_zip_epub_verify_detects_corrupted_zero_byte_chapter() {
    let temp_dir = std::env::temp_dir();
    let epub_path = temp_dir.join("test_verify_zero_byte.epub");

    {
        let file = File::create(&epub_path).unwrap();
        let writer = BufWriter::new(file);
        let mut zip = ZipWriter::new(writer);

        let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        zip.start_file("mimetype", stored).unwrap();
        zip.write_all(b"application/epub+zip").unwrap();

        let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        zip.start_file("META-INF/container.xml", deflated).unwrap();
        zip.write_all(br#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#).unwrap();

        zip.start_file("content.opf", deflated).unwrap();
        zip.write_all(br#"<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Test</dc:title></metadata><manifest><item id="c1" href="ch019_Index.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c1"/></spine></package>"#).unwrap();

        zip.start_file("ch019_Index.xhtml", deflated).unwrap();
        zip.write_all(b"").unwrap();

        zip.finish().unwrap();
    }

    let adapter = ZipEpubAdapter::new();
    let report = adapter.verify_epub(&epub_path).await.unwrap();

    assert!(!report.is_valid);
    assert_eq!(report.total_xhtml_files, 1);
    assert_eq!(report.valid_xhtml_files, 0);
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.contains("ch019_Index.xhtml") && e.contains("0 bytes"))
    );

    let _ = fs::remove_file(epub_path);
}

#[tokio::test]
async fn test_zip_epub_verify_detects_mimetype_and_xml_violations() {
    let temp_dir = std::env::temp_dir();
    let epub_path = temp_dir.join("test_verify_violations.epub");

    {
        let file = File::create(&epub_path).unwrap();
        let writer = BufWriter::new(file);
        let mut zip = ZipWriter::new(writer);

        let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        zip.start_file("mimetype", deflated).unwrap();
        zip.write_all(b"application/epub+zip").unwrap();

        zip.start_file("META-INF/container.xml", deflated).unwrap();
        zip.write_all(b"<container><invalid></container>").unwrap();

        zip.finish().unwrap();
    }

    let adapter = ZipEpubAdapter::new();
    let report = adapter.verify_epub(&epub_path).await.unwrap();

    assert!(!report.is_valid);
    assert!(!report.mimetype_valid);
    assert!(!report.container_xml_valid);
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.contains("mimetype") && e.contains("uncompressed"))
    );

    let _ = fs::remove_file(epub_path);
}

#[tokio::test]
async fn test_zip_epub_write_refuses_empty_chapter() {
    let temp_dir = std::env::temp_dir();
    let out_path = temp_dir.join("test_refuse_empty_chapter.epub");

    let book = epub_translator_google::domain::entities::EpubBook {
        metadata: Default::default(),
        chapters: vec![epub_translator_google::domain::entities::EpubChapter {
            path: "ch019_Index.xhtml".to_string(),
            content: "   \n\t  ".to_string(),
            media_type: "application/xhtml+xml".to_string(),
        }],
        resources: Vec::new(),
        raw_container_xml: Some("<container/>".to_string()),
        raw_opf_content: Some("<package/>".to_string()),
        opf_path: "content.opf".to_string(),
    };

    let adapter = ZipEpubAdapter::new();
    let res = adapter.write_epub(&book, &out_path).await;
    assert!(res.is_err());

    let _ = fs::remove_file(out_path);
}

#[test]
fn test_find_opf_path_with_attributes_and_whitespace() {
    let container_xml = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile media-type="application/oebps-package+xml" full-path="EPUB/package.opf" id="item1"/>
  </rootfiles>
</container>"#;

    let opf_path = ZipEpubAdapter::find_opf_path_from_container_xml(container_xml).unwrap();
    assert_eq!(opf_path, "EPUB/package.opf");
}

#[test]
fn test_parse_opf_metadata_with_attributes_on_dc_tags() {
    let opf_xml = r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title id="t1" dir="ltr">Advanced Rust Architecture</dc:title>
    <dc:language id="pub-lang">en-US</dc:language>
    <dc:creator id="author" opf:role="aut">Jane Doe</dc:creator>
    <dc:identifier id="isbn" opf:scheme="ISBN">978-0-123456-47-2</dc:identifier>
    <dc:publisher id="pub">Tech Press</dc:publisher>
  </metadata>
</package>"#;

    let metadata = ZipEpubAdapter::parse_opf_metadata(opf_xml);
    assert_eq!(
        metadata.title,
        Some("Advanced Rust Architecture".to_string())
    );
    assert_eq!(metadata.language, Some(Language::new("en-US")));
    assert_eq!(metadata.creator, Some("Jane Doe".to_string()));
    assert_eq!(metadata.identifier, Some("978-0-123456-47-2".to_string()));
    assert_eq!(metadata.publisher, Some("Tech Press".to_string()));
}

#[test]
fn test_update_xml_tag_preserves_attributes_on_dc_language() {
    use epub_translator_google::domain::services::update_xml_tag;

    let opf_xml = r#"<package><metadata><dc:language id="pub-language" dir="ltr">en</dc:language></metadata></package>"#;
    let updated = update_xml_tag(opf_xml, "dc:language", "pt-BR");
    assert!(updated.contains(r#"<dc:language id="pub-language" dir="ltr">pt-BR</dc:language>"#));
}

#[test]
fn test_update_xml_tag_handles_empty_self_closing_tag() {
    use epub_translator_google::domain::services::update_xml_tag;

    let opf_xml = r#"<package><metadata><dc:language/></metadata></package>"#;
    let updated = update_xml_tag(opf_xml, "dc:language", "pt-BR");
    assert!(updated.contains(r#"<dc:language>pt-BR</dc:language>"#));
}
