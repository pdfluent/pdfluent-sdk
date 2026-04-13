//! Extract embedded font programs from oracle PDFs and cache them for reuse.
//!
//! Usage:
//!   font-cache-builder --input-dir /opt/xfa-golden-set/flattened --output-dir /opt/xfa-font-cache
//!
//! Reads all PDFs in the input directory, extracts TrueType/CFF font programs,
//! and writes them to the output directory keyed by PostScript name.

use anyhow::{Context, Result};
use lopdf::Document;
use std::collections::HashSet;
use std::path::PathBuf;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let (input_dir, output_dir) = parse_args(&args)?;

    std::fs::create_dir_all(&output_dir)
        .with_context(|| format!("cannot create output dir: {}", output_dir.display()))?;

    let mut pdf_files: Vec<PathBuf> = std::fs::read_dir(&input_dir)
        .with_context(|| format!("cannot read input dir: {}", input_dir.display()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
        })
        .collect();
    pdf_files.sort();

    println!(
        "Scanning {} PDFs in {}",
        pdf_files.len(),
        input_dir.display()
    );

    let mut total_fonts = 0usize;
    let mut total_new = 0usize;
    let mut seen_names: HashSet<String> = HashSet::new();

    // Also track what's already cached
    if let Ok(entries) = std::fs::read_dir(&output_dir) {
        for entry in entries.flatten() {
            if let Some(stem) = entry.path().file_stem().and_then(|s| s.to_str()) {
                seen_names.insert(stem.to_lowercase());
            }
        }
    }
    let pre_existing = seen_names.len();
    if pre_existing > 0 {
        println!("Found {pre_existing} pre-existing cached fonts");
    }

    for pdf_path in &pdf_files {
        match extract_fonts_from_pdf(pdf_path, &output_dir, &mut seen_names) {
            Ok((extracted, new)) => {
                total_fonts += extracted;
                total_new += new;
                if new > 0 {
                    println!(
                        "  {} — {} fonts extracted, {} new",
                        pdf_path.file_name().unwrap_or_default().to_string_lossy(),
                        extracted,
                        new
                    );
                }
            }
            Err(e) => {
                eprintln!(
                    "  WARN: {} — {}",
                    pdf_path.file_name().unwrap_or_default().to_string_lossy(),
                    e
                );
            }
        }
    }

    println!(
        "\nDone: {total_fonts} fonts found, {total_new} new fonts cached in {}",
        output_dir.display()
    );
    Ok(())
}

fn parse_args(args: &[String]) -> Result<(PathBuf, PathBuf)> {
    let mut input_dir = None;
    let mut output_dir = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--input-dir" | "-i" => {
                i += 1;
                input_dir = Some(PathBuf::from(&args[i]));
            }
            "--output-dir" | "-o" => {
                i += 1;
                output_dir = Some(PathBuf::from(&args[i]));
            }
            "--help" | "-h" => {
                println!("Usage: font-cache-builder --input-dir <DIR> --output-dir <DIR>");
                println!("\nExtracts embedded font programs from oracle PDFs and caches them.");
                println!("\nOptions:");
                println!("  -i, --input-dir   Directory containing oracle PDFs");
                println!("  -o, --output-dir  Directory to write cached font files");
                std::process::exit(0);
            }
            _ => anyhow::bail!("unknown argument: {}", args[i]),
        }
        i += 1;
    }
    let input_dir = input_dir.context("--input-dir is required")?;
    let output_dir = output_dir.context("--output-dir is required")?;
    Ok((input_dir, output_dir))
}

/// Extract all embedded font programs from a single PDF.
///
/// Returns (total_extracted, newly_cached).
fn extract_fonts_from_pdf(
    pdf_path: &std::path::Path,
    output_dir: &std::path::Path,
    seen_names: &mut HashSet<String>,
) -> Result<(usize, usize)> {
    let doc = Document::load(pdf_path)
        .with_context(|| format!("cannot load PDF: {}", pdf_path.display()))?;

    let mut extracted = 0usize;
    let mut new = 0usize;

    for obj in doc.objects.values() {
        let dict = match obj.as_dict() {
            Ok(d) => d,
            Err(_) => continue,
        };
        let is_font =
            dict.get(b"Type").ok().and_then(|o| o.as_name().ok()) == Some(b"Font".as_slice());
        if !is_font {
            continue;
        }
        let base_font = match dict.get(b"BaseFont").ok().and_then(|o| o.as_name().ok()) {
            Some(n) => String::from_utf8_lossy(n).to_string(),
            None => continue,
        };

        // Try direct FontDescriptor path
        if let Some(data) = extract_font_data_from_fd(&doc, dict) {
            if data.len() < 16 {
                continue;
            }
            let ps_name =
                extract_ps_name_from_data(&data).unwrap_or_else(|| sanitize_font_name(&base_font));
            extracted += 1;

            let key = ps_name.to_lowercase();
            if seen_names.contains(&key) {
                continue;
            }

            let ext = detect_font_format(&data);
            let out_path = output_dir.join(format!("{ps_name}.{ext}"));
            if std::fs::write(&out_path, &data).is_ok() {
                seen_names.insert(key);
                new += 1;
            }
            continue;
        }

        // CIDFont path: DescendantFonts → FontDescriptor → FontFile*
        if let Some(data) = extract_cidfont_data_from_dict(&doc, dict) {
            if data.len() < 16 {
                continue;
            }
            let ps_name =
                extract_ps_name_from_data(&data).unwrap_or_else(|| sanitize_font_name(&base_font));
            extracted += 1;

            let key = ps_name.to_lowercase();
            if seen_names.contains(&key) {
                continue;
            }

            let ext = detect_font_format(&data);
            let out_path = output_dir.join(format!("{ps_name}.{ext}"));
            if std::fs::write(&out_path, &data).is_ok() {
                seen_names.insert(key);
                new += 1;
            }
        }
    }

    Ok((extracted, new))
}

/// Extract raw font program bytes from a font dictionary's FontDescriptor.
fn extract_font_data_from_fd(doc: &Document, font_dict: &lopdf::Dictionary) -> Option<Vec<u8>> {
    let fd_ref = font_dict.get(b"FontDescriptor").ok()?;
    let fd = resolve_to_dict(doc, fd_ref)?;

    for key in &[b"FontFile2".as_slice(), b"FontFile3", b"FontFile"] {
        if let Some(stream) = fd.get(*key).ok().and_then(|o| resolve_to_stream(doc, o)) {
            return stream
                .decompressed_content()
                .ok()
                .or_else(|| Some(stream.content.clone()));
        }
    }
    None
}

/// Extract font data from CIDFont descendants.
fn extract_cidfont_data_from_dict(
    doc: &Document,
    type0_dict: &lopdf::Dictionary,
) -> Option<Vec<u8>> {
    let descendants = type0_dict.get(b"DescendantFonts").ok()?.as_array().ok()?;
    let desc_ref = descendants.first()?;
    let cid_dict = match desc_ref {
        lopdf::Object::Reference(id) => doc.get_dictionary(*id).ok()?,
        lopdf::Object::Dictionary(d) => d,
        _ => return None,
    };
    extract_font_data_from_fd(doc, cid_dict)
}

/// Resolve a PDF object to a dictionary, following indirect references.
fn resolve_to_dict<'a>(doc: &'a Document, obj: &'a lopdf::Object) -> Option<&'a lopdf::Dictionary> {
    match obj {
        lopdf::Object::Reference(id) => doc.get_dictionary(*id).ok(),
        lopdf::Object::Dictionary(d) => Some(d),
        _ => None,
    }
}

/// Resolve a PDF object to a stream, following indirect references.
fn resolve_to_stream<'a>(doc: &'a Document, obj: &'a lopdf::Object) -> Option<&'a lopdf::Stream> {
    match obj {
        lopdf::Object::Reference(id) => {
            if let Ok(lopdf::Object::Stream(s)) = doc.get_object(*id) {
                Some(s)
            } else {
                None
            }
        }
        lopdf::Object::Stream(s) => Some(s),
        _ => None,
    }
}

/// Try to extract the PostScript name from font data using ttf_parser.
fn extract_ps_name_from_data(data: &[u8]) -> Option<String> {
    let face = ttf_parser::Face::parse(data, 0).ok()?;
    for name_record in face.names() {
        if name_record.name_id == ttf_parser::name_id::POST_SCRIPT_NAME {
            if let Some(s) = name_record.to_string() {
                let s: String = s;
                if !s.is_empty() {
                    return Some(s);
                }
            }
        }
    }
    None
}

/// Sanitize a font name for use as a filename.
fn sanitize_font_name(name: &str) -> String {
    // Strip subset prefix (ABCDEF+)
    let stripped = if name.len() > 7
        && name.as_bytes()[6] == b'+'
        && name[..6].chars().all(|c| c.is_ascii_uppercase())
    {
        &name[7..]
    } else {
        name
    };
    stripped
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Detect font format from magic bytes.
fn detect_font_format(data: &[u8]) -> &'static str {
    if data.len() < 4 {
        return "bin";
    }
    match &data[..4] {
        [0x00, 0x01, 0x00, 0x00] => "ttf",
        b"true" => "ttf",
        b"OTTO" => "otf",
        b"ttcf" => "ttc",
        _ => {
            // CFF starts with major version byte (usually 1)
            if data[0] == 1 && data.len() > 4 {
                "cff"
            } else {
                "bin"
            }
        }
    }
}
