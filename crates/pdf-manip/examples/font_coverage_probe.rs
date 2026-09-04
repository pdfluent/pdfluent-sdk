//! Probe which fonts this writer accepts and what scripts they cover.
//!
//! Answers the practical question a caller has to settle before shipping:
//! which font files can I actually hand to `FontFallback::EmbedUnicode`, and
//! which languages does each one buy me.
//!
//! Usage: cargo run -p pdf-manip --example font_coverage_probe -- <font>...

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdf_manip::unicode_font::UnicodeFont;

/// One representative sample per script band, chosen to include the
/// diacritics that actually break — not just the base alphabet.
const SAMPLES: &[(&str, &str)] = &[
    ("Latijn-West", "Grüße naïve çedilla"),
    ("Latijn-Oost", "zażółć příliš őrült"),
    ("Turks", "yağız şoföre"),
    ("Cyrillisch", "Съешь ещё"),
    ("Grieks", "Γειά σου"),
    ("Hebreeuws", "שלום עולם"),
    ("Arabisch", "مرحبا بالعالم"),
    ("Devanagari", "नमस्ते दुनिया"),
    ("Thai", "สวัสดีชาวโลก"),
    ("Chinees", "中文文本"),
    ("Japans", "日本語のテキスト"),
    ("Koreaans", "한국어 텍스트"),
];

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: font_coverage_probe <font>...");
        std::process::exit(2);
    }

    for path in &args {
        let short = path.rsplit('/').next().unwrap_or(path);
        let Ok(data) = std::fs::read(path) else {
            println!("{short:38} KAN NIET LEZEN");
            continue;
        };
        let size_mb = data.len() as f64 / 1_048_576.0;

        match UnicodeFont::from_bytes(data) {
            Err(e) => {
                let reason = if e.to_string().contains("glyf") {
                    "CFF-outlines (gebruik de .ttf-variant)"
                } else {
                    "onbruikbaar"
                };
                println!("{short:38} {size_mb:>6.1} MB  GEWEIGERD: {reason}");
            }
            Ok(font) => {
                let covered: Vec<&str> = SAMPLES
                    .iter()
                    .filter(|(_, text)| font.covers(text))
                    .map(|(label, _)| *label)
                    .collect();
                println!(
                    "{short:38} {size_mb:>6.1} MB  OK  [{}]",
                    if covered.is_empty() {
                        "geen van de testtalen".to_string()
                    } else {
                        covered.join(", ")
                    }
                );
            }
        }
    }
}
