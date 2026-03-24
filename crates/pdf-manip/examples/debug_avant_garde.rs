// Check what glyph_index returns for U+0081 on the AvantGarde-Book substitute
fn main() {
    let data = std::fs::read("/tmp/pdf-test-6.2.11.5/gen-131_131159.pdf").unwrap();
    let mut doc = lopdf::Document::load_mem(&data).unwrap();

    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false).unwrap();
    }));
    let _ = pdf_manip::pdfa_fonts::embed_fonts(&mut doc);

    for (_id, obj) in &doc.objects {
        let lopdf::Object::Dictionary(dict) = obj else {
            continue;
        };
        let base = match dict.get(b"BaseFont").ok() {
            Some(lopdf::Object::Name(n)) => String::from_utf8_lossy(n).to_string(),
            _ => continue,
        };
        if !base.contains("AvantGarde-Book") {
            continue;
        }

        if let Ok(lopdf::Object::Reference(fd_ref)) = dict.get(b"FontDescriptor").cloned() {
            if let Some(lopdf::Object::Dictionary(fd)) = doc.objects.get(&fd_ref) {
                if let Ok(lopdf::Object::Reference(ff3_ref)) = fd.get(b"FontFile3").cloned() {
                    if let Some(lopdf::Object::Stream(s)) = doc.objects.get(&ff3_ref) {
                        let mut s2 = s.clone();
                        let _ = s2.decompress();
                        let font_data = s2.content.clone();

                        let face = ttf_parser::Face::parse(&font_data, 0).unwrap();
                        let upem = face.units_per_em() as f64;
                        let scale = 1000.0 / upem;

                        // Check U+0081
                        let ch = char::from_u32(0x0081).unwrap();
                        let gid_081 = face.glyph_index(ch);
                        println!("face.glyph_index(U+0081) = {:?}", gid_081);
                        if let Some(gid) = gid_081 {
                            let adv = face.glyph_hor_advance(gid);
                            println!(
                                "  advance = {:?}, scaled = {:?}",
                                adv,
                                adv.map(|w| (w as f64 * scale).round() as i64)
                            );
                        }

                        // Check U+007E (tilde) for comparison
                        let ch126 = char::from_u32(0x007E).unwrap();
                        let gid_126 = face.glyph_index(ch126);
                        println!("face.glyph_index(U+007E ~) = {:?}", gid_126);
                        if let Some(gid) = gid_126 {
                            let adv = face.glyph_hor_advance(gid);
                            println!(
                                "  advance = {:?}, scaled = {:?}",
                                adv,
                                adv.map(|w| (w as f64 * scale).round() as i64)
                            );
                        }

                        // Check hmtx GID 0
                        let adv0 = face.glyph_hor_advance(ttf_parser::GlyphId(0));
                        println!(
                            "hmtx GID 0 advance = {:?}, scaled = {:?}",
                            adv0,
                            adv0.map(|w| (w as f64 * scale).round() as i64)
                        );

                        // Check GID 95
                        let adv95 = face.glyph_hor_advance(ttf_parser::GlyphId(95));
                        println!(
                            "hmtx GID 95 advance = {:?}, scaled = {:?}",
                            adv95,
                            adv95.map(|w| (w as f64 * scale).round() as i64)
                        );

                        // CFF encoding offset
                        let cff_bytes = extract_cff_table(&font_data);
                        if let Some(cff_bytes) = cff_bytes {
                            println!(
                                "Is custom CFF encoding: {}",
                                cff_bytes.len() > 4
                                    && {
                                        let hdr_size = cff_bytes[2] as usize;
                                        // find Top DICT
                                        let enc_off = 0u32;
                                        if hdr_size < cff_bytes.len() {
                                            // Just print a few bytes around the enc offset
                                            if cff_bytes.len() > hdr_size + 3 {
                                                println!("  CFF hdr_size={hdr_size}, first bytes after header: {:02X?}", &cff_bytes[hdr_size..std::cmp::min(hdr_size+8, cff_bytes.len())]);
                                            }
                                        }
                                        enc_off > 1
                                    }
                            );

                            // Check CFF glyph_index via cff_parser
                            if let Some(cff) = cff_parser::Table::parse(cff_bytes) {
                                let gid129 = cff.glyph_index(129u8);
                                println!("cff.glyph_index(129) = {:?}", gid129);
                            }
                        }
                        break;
                    }
                }
            }
        }
        break;
    }
}

fn extract_cff_table(data: &[u8]) -> Option<&[u8]> {
    let raw_face = ttf_parser::RawFace::parse(data, 0).ok()?;
    for record in raw_face.table_records {
        if &record.tag.to_bytes() == b"CFF " {
            let start = record.offset as usize;
            let end = start.checked_add(record.length as usize)?;
            return data.get(start..end);
        }
    }
    None
}
