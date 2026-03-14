//! Inspect CIDFont structure to debug CIDSet issues.
use lopdf::{Document, Object};

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: inspect_cidset <pdf>");
    let data = std::fs::read(&path).expect("read");
    let doc = Document::load_mem(&data).expect("parse");

    for (id, obj) in &doc.objects {
        if let Object::Dictionary(dict) = obj {
            let subtype = dict
                .get(b"Subtype")
                .ok()
                .and_then(|o| {
                    if let Object::Name(n) = o {
                        Some(n.clone())
                    } else {
                        None
                    }
                })
                .unwrap_or_default();

            if subtype == b"CIDFontType0" || subtype == b"CIDFontType2" {
                let name = dict
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

                println!(
                    "CIDFont {:?}: subtype={}, name={}",
                    id,
                    String::from_utf8_lossy(&subtype),
                    name
                );

                if let Some(Object::Reference(fd_id)) = dict.get(b"FontDescriptor").ok() {
                    println!("  FontDescriptor: {:?}", fd_id);
                    if let Some(Object::Dictionary(fd)) = doc.objects.get(fd_id) {
                        let has_ff2 = fd.has(b"FontFile2");
                        let has_ff3 = fd.has(b"FontFile3");
                        let has_cidset = fd.has(b"CIDSet");
                        println!(
                            "  FontFile2={}, FontFile3={}, CIDSet={}",
                            has_ff2, has_ff3, has_cidset
                        );

                        if has_cidset {
                            match fd.get(b"CIDSet").ok() {
                                Some(Object::Reference(cs_id)) => {
                                    println!("  CIDSet ref: {:?}", cs_id);
                                    if let Some(Object::Stream(s)) = doc.objects.get(cs_id) {
                                        let bytes = &s.content;
                                        println!("  CIDSet bytes: {}", bytes.len());
                                        let bits: u32 = bytes.iter().map(|b| b.count_ones()).sum();
                                        println!("  CIDSet bits set: {}", bits);
                                        println!("  First 8 bytes: {:08b} {:08b} {:08b} {:08b} {:08b} {:08b} {:08b} {:08b}",
                                            bytes.get(0).copied().unwrap_or(0),
                                            bytes.get(1).copied().unwrap_or(0),
                                            bytes.get(2).copied().unwrap_or(0),
                                            bytes.get(3).copied().unwrap_or(0),
                                            bytes.get(4).copied().unwrap_or(0),
                                            bytes.get(5).copied().unwrap_or(0),
                                            bytes.get(6).copied().unwrap_or(0),
                                            bytes.get(7).copied().unwrap_or(0),
                                        );
                                    }
                                }
                                Some(other) => println!("  CIDSet (inline): {:?}", other),
                                None => {}
                            }
                        }

                        // Check FontFile2 (TrueType)
                        if has_ff2 {
                            if let Some(Object::Reference(ff_id)) = fd.get(b"FontFile2").ok() {
                                if let Some(Object::Stream(s)) = doc.objects.get(ff_id) {
                                    let mut s2 = s.clone();
                                    let _ = s2.decompress();
                                    let data = &s2.content;
                                    println!(
                                        "  FontFile2 raw={}, decompressed={} bytes",
                                        s.content.len(),
                                        data.len()
                                    );
                                    if data.len() >= 12 {
                                        let num_tables =
                                            u16::from_be_bytes([data[4], data[5]]) as usize;
                                        println!("  numTables={}", num_tables);
                                        for i in 0..num_tables {
                                            let off = 12 + i * 16;
                                            if off + 16 <= data.len()
                                                && &data[off..off + 4] == b"maxp"
                                            {
                                                let toff = u32::from_be_bytes([
                                                    data[off + 8],
                                                    data[off + 9],
                                                    data[off + 10],
                                                    data[off + 11],
                                                ])
                                                    as usize;
                                                if toff + 6 <= data.len() {
                                                    let ng = u16::from_be_bytes([
                                                        data[toff + 4],
                                                        data[toff + 5],
                                                    ]);
                                                    println!("  maxp.numGlyphs = {}", ng);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Check FontFile3 (CFF)
                        if has_ff3 {
                            if let Some(Object::Reference(ff_id)) = fd.get(b"FontFile3").ok() {
                                if let Some(Object::Stream(s)) = doc.objects.get(ff_id) {
                                    let data = &s.content;
                                    println!("  FontFile3 size: {} bytes", data.len());
                                    let subtype3 = s
                                        .dict
                                        .get(b"Subtype")
                                        .ok()
                                        .and_then(|o| {
                                            if let Object::Name(n) = o {
                                                Some(String::from_utf8_lossy(n).to_string())
                                            } else {
                                                None
                                            }
                                        })
                                        .unwrap_or_default();
                                    println!("  FontFile3 Subtype: {}", subtype3);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// Also check CIDToGIDMap and DW/W arrays
