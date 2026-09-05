//! How many columns do real documents get after conversion to docx?
use std::io::Read;

fn main() {
    let (mut converted, mut tables, mut widest) = (0usize, 0usize, 0usize);
    let mut widest_name = String::new();
    let mut distribution: std::collections::BTreeMap<usize, usize> = Default::default();
    for path in std::env::args().skip(1) {
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let Ok(doc) = pdfluent::PdfDocument::from_bytes(&bytes) else {
            continue;
        };
        // `to_docx` writes a file; the trunk this example comes from also had a
        // bytes variant, master does not.
        let out = std::env::temp_dir().join("pdfluent-column-count.docx");
        if doc.to_docx(&out).is_err() {
            continue;
        }
        let Ok(docx) = std::fs::read(&out) else {
            continue;
        };
        converted += 1;
        let Ok(mut zip) = zip::ZipArchive::new(std::io::Cursor::new(&docx)) else {
            continue;
        };
        let mut xml = String::new();
        {
            let Ok(mut f) = zip.by_name("word/document.xml") else {
                continue;
            };
            let _ = f.read_to_string(&mut xml);
        }
        for grid in xml.split("<w:tblGrid>").skip(1) {
            let end = grid.find("</w:tblGrid>").unwrap_or(0);
            let n = grid[..end].matches("<w:gridCol").count();
            tables += 1;
            *distribution.entry(n).or_default() += 1;
            if n > widest {
                widest = n;
                widest_name = path.rsplit('/').next().unwrap_or("").to_string();
            }
        }
    }
    println!("  documents converted: {converted}");
    println!("  tables found:        {tables}");
    println!("  most columns:        {widest}  ({widest_name})");
    let wide: usize = distribution
        .iter()
        .filter(|(k, _)| **k > 8)
        .map(|(_, v)| *v)
        .sum();
    println!("  tables with more than 8 columns: {wide}");
}
