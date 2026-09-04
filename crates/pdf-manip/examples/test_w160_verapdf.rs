//! Test which width value for code 160 makes veraPDF pass on gen-571.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.
use lopdf::{Document, Object};

fn set_font_width(doc: &mut Document, font_name: &str, code: u32, new_w: i64) -> bool {
    let ids: Vec<_> = doc.objects.keys().copied().collect();
    let mut changed = false;
    for id in ids {
        let (fc, widths_ref_opt) = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&id) else {
                continue;
            };
            let base = dict
                .get(b"BaseFont")
                .ok()
                .and_then(|o| {
                    if let Object::Name(n) = o {
                        Some(String::from_utf8_lossy(n).to_string())
                    } else {
                        None
                    }
                })
                .unwrap_or_default();
            if !base.contains(font_name) {
                continue;
            }
            let fc = dict
                .get(b"FirstChar")
                .ok()
                .and_then(|o| {
                    if let Object::Integer(i) = o {
                        Some(*i as u32)
                    } else {
                        None
                    }
                })
                .unwrap_or(0);
            let wr = match dict.get(b"Widths").ok() {
                Some(Object::Reference(r)) => Some(*r),
                _ => None,
            };
            (fc, wr)
        };
        if code < fc {
            continue;
        }
        let idx = (code - fc) as usize;
        if let Some(r) = widths_ref_opt {
            if let Some(Object::Array(ref mut arr)) = doc.objects.get_mut(&r) {
                if idx < arr.len() {
                    arr[idx] = Object::Integer(new_w);
                    changed = true;
                }
            }
        }
    }
    changed
}

#[allow(dead_code)]
fn verapdf_check_full(path: &str) -> (bool, Vec<String>) {
    let out = std::process::Command::new("verapdf")
        .args(["--flavour", "2b", "--format", "json", path])
        .output()
        .unwrap_or_else(|_| panic!("verapdf not found"));
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    let compliant = s.contains("\"compliant\":true") || s.contains("\"compliant\": true");

    // Extract all failure contexts
    let mut contexts = Vec::new();
    let mut pos = 0;
    while let Some(i) = s[pos..].find("\"failedChecks\"") {
        let region_start = pos + i;
        let region_end = (region_start + 500).min(s.len());
        let region = &s[region_start..region_end];
        // Get failedChecks number
        if let Some(colon) = region.find(':') {
            let num_start = colon + 1;
            let num_end = region[num_start..]
                .find(|c: char| !c.is_ascii_digit() && c != ' ')
                .map(|e| num_start + e)
                .unwrap_or(num_start + 5);
            let num_str = region[num_start..num_end].trim();
            if let Ok(n) = num_str.parse::<u64>() {
                if n > 0 {
                    // Find clause nearby
                    let clause_pos = s[..region_start]
                        .rfind("\"clause\"")
                        .unwrap_or(region_start);
                    let clause_region = &s[clause_pos..region_start + 20];
                    contexts.push(format!(
                        "failed={n} clause=...{}",
                        &clause_region[..clause_region.len().min(80)]
                    ));
                }
            }
        }
        pos = region_start + 1;
    }
    (compliant, contexts)
}

fn verapdf_failures_json(path: &str) -> String {
    let out = std::process::Command::new("verapdf")
        .args(["--flavour", "2b", "--format", "json", path])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn main() {
    // Test gen-571: change code 160 width and run veraPDF
    let data = std::fs::read("/tmp/gen-571-retest.pdf").unwrap();
    println!("=== gen-571: Testing code 160 width values ===");
    for &w in &[0i64, 250, 278, 333, 500, 722, 778] {
        let mut doc = Document::load_mem(&data).unwrap();
        let changed = set_font_width(&mut doc, "PMALQQ+Times-Roman", 160, w);
        if !changed {
            println!("  w={w}: NOT CHANGED");
            continue;
        }
        let out = format!("/tmp/test571_{w}.pdf");
        doc.save(&out).unwrap();
        let json = verapdf_failures_json(&out);
        let compliant = json.contains("\"compliant\":true");
        // Count failures
        let fail_count = json.matches("\"status\":\"failed\"").count();
        // Find all clause contexts
        let mut ctxs = Vec::new();
        for ctx_match in json.match_indices("\"context\":") {
            let start = ctx_match.0 + 11;
            if start >= json.len() {
                continue;
            }
            let rest = &json[start..];
            let ctx_start = rest.find('"').unwrap_or(0) + 1;
            let ctx_rest = &rest[ctx_start..];
            if let Some(end) = ctx_rest.find('"') {
                let ctx = &ctx_rest[..end];
                if !ctx.is_empty() && ctx.contains("false") {
                    let short = if ctx.len() > 100 {
                        &ctx[ctx.len() - 100..]
                    } else {
                        ctx
                    };
                    ctxs.push(short.to_string());
                }
            }
        }
        println!(
            "  w={w}: {} failures={fail_count} ctxs={:?}",
            if compliant { "PASS✓" } else { "fail " },
            &ctxs[..ctxs.len().min(3)]
        );
        let _ = std::fs::remove_file(&out);
    }
}
