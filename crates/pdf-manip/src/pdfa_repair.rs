//! Byte-level and page-tree repair used by the PDF/A conversion pipeline.
//!
//! Real-world PDFs handed to a converter are frequently damaged: broken xref
//! tables, trailers pointing at a non-Catalog object, `Kids` arrays holding
//! free-object references, `#XX`-escaped names lopdf cannot parse. None of
//! that is a PDF/A question, but a converter that gives up on them converts
//! far fewer documents than one that repairs them first.
//!
//! Everything here is internal to [`crate::pdfa`]. The routines are ordered
//! from cheapest to most invasive, and each returns `None`/`false` rather
//! than guessing when it cannot make a confident repair.

/// Accept a loaded Document only if it has at least 1 object.
/// lopdf sometimes "succeeds" loading corrupt data but finds 0 objects.
pub(crate) fn accept_doc(doc: lopdf::Document) -> Option<lopdf::Document> {
    if doc.objects.is_empty() {
        None
    } else {
        Some(doc)
    }
}

/// Try to load PDF data with lopdf, accepting only non-empty documents.
pub(crate) fn try_load(data: &[u8]) -> Option<lopdf::Document> {
    lopdf::Document::load_mem(data).ok().and_then(accept_doc)
}

pub(crate) fn raw_has_hash_names(data: &[u8]) -> bool {
    // Look for `/Name#` patterns — a `/` followed by alphanumeric chars and then `#`.
    let mut i = 0;
    while i + 3 < data.len() {
        if data[i] == b'/' {
            i += 1;
            while i < data.len() && data[i].is_ascii_alphanumeric() {
                i += 1;
            }
            if i < data.len() && data[i] == b'#' {
                return true;
            }
        } else {
            i += 1;
        }
    }
    false
}

/// Replace `#` with `_` in all PDF name tokens (after `/`), both in dict
/// keys and content stream operators. This is a 1-byte substitution that
/// preserves xref offsets and stream lengths.
///
/// Skips: string literals `(...)`, hex strings `<...>`, and `%` comments.
/// Processes content inside streams (since content stream operators like
/// `/Im#22 Do` need the same renaming as their Resource dict keys).
pub(crate) fn sanitize_hash_names_raw(data: &[u8]) -> Vec<u8> {
    let mut result = data.to_vec();
    let mut i = 0;
    let mut in_string = 0i32;
    let mut in_hex_string = false;

    while i < result.len() {
        // Track string literals (parentheses).
        if result[i] == b'(' && !in_hex_string {
            in_string += 1;
            i += 1;
            continue;
        }
        if result[i] == b')' && in_string > 0 {
            in_string -= 1;
            i += 1;
            continue;
        }
        if result[i] == b'\\' && in_string > 0 {
            i += 2;
            continue;
        }
        if in_string > 0 {
            i += 1;
            continue;
        }

        // Skip dict delimiters << and >>.
        if result[i] == b'<' && i + 1 < result.len() && result[i + 1] == b'<' {
            i += 2;
            continue;
        }
        if result[i] == b'>' && i + 1 < result.len() && result[i + 1] == b'>' {
            i += 2;
            continue;
        }

        // Track hex strings.
        if result[i] == b'<' {
            in_hex_string = true;
            i += 1;
            continue;
        }
        if result[i] == b'>' && in_hex_string {
            in_hex_string = false;
            i += 1;
            continue;
        }
        if in_hex_string {
            i += 1;
            continue;
        }

        // Skip comments (but not %PDF header or %%EOF).
        if result[i] == b'%' {
            while i < result.len() && result[i] != b'\n' && result[i] != b'\r' {
                i += 1;
            }
            continue;
        }

        // Name token: replace `#` with `_`.
        if result[i] == b'/' {
            i += 1;
            while i < result.len() && !is_name_delimiter(result[i]) {
                if result[i] == b'#' {
                    result[i] = b'_';
                }
                i += 1;
            }
            continue;
        }

        i += 1;
    }

    result
}

pub(crate) fn is_name_delimiter(b: u8) -> bool {
    matches!(
        b,
        b' ' | b'\t'
            | b'\n'
            | b'\r'
            | b'\0'
            | b'/'
            | b'['
            | b']'
            | b'('
            | b')'
            | b'<'
            | b'>'
            | b'{'
            | b'}'
            | b'%'
    )
}

/// 1. Strip garbage bytes before the %PDF header (offset header).
/// 2. Try appending a minimal %%EOF + startxref if missing/broken.
/// 3. Try truncating trailing garbage after the last %%EOF.
pub(crate) fn try_repair_for_lopdf(data: &[u8]) -> Option<lopdf::Document> {
    // Strategy 1: Find %PDF- offset and strip leading garbage.
    let offset = data.windows(5).position(|w| w == b"%PDF-")?;

    if offset > 0 {
        let trimmed = &data[offset..];
        if let Some(doc) = try_load(trimmed) {
            return Some(doc);
        }
        // Continue with trimmed data for further strategies.
        return try_repair_xref(trimmed);
    }

    try_repair_xref(data)
}

/// Try to fix xref/trailer issues in PDF data.
///
/// Strategies:
/// - Find the last `startxref` and `%%EOF`, and verify the xref offset.
///   If the offset is wrong, try to fix it.
/// - If `%%EOF` is missing, append one.
/// - If `startxref` points to wrong location, try scanning for actual xref position.
pub(crate) fn try_repair_xref(data: &[u8]) -> Option<lopdf::Document> {
    // Look for "startxref" in the last part of the file.
    let search_start = data.len().saturating_sub(4096);
    let tail = &data[search_start..];

    // Find the last startxref.
    let startxref_pos = tail
        .windows(9)
        .rposition(|w| w == b"startxref")
        .map(|p| search_start + p);

    if let Some(sxr) = startxref_pos {
        // Read the xref offset value after "startxref".
        let after = &data[sxr + 9..];
        let offset_str: String = after
            .iter()
            .skip_while(|b| b.is_ascii_whitespace())
            .take_while(|b| b.is_ascii_digit())
            .map(|&b| b as char)
            .collect();

        if let Ok(xref_offset) = offset_str.parse::<usize>() {
            // Check if the xref offset actually points to "xref" or a valid xref stream.
            if xref_offset < data.len() {
                let at_offset = &data[xref_offset..];
                let has_valid_xref = if at_offset.starts_with(b"xref") {
                    // Check it's not an empty xref (immediately followed by "trailer").
                    let after_keyword = &at_offset[4..];
                    let trimmed = after_keyword
                        .iter()
                        .position(|b| !b.is_ascii_whitespace())
                        .map(|p| &after_keyword[p..])
                        .unwrap_or(b"");
                    !trimmed.starts_with(b"trailer")
                } else if at_offset.len() > 10 && at_offset[0].is_ascii_digit() {
                    // Could be a cross-reference stream: "N 0 obj".
                    // Distinguish from xref table entries ("0000NNNNNN 00000 n").
                    // xref entries have a 10-digit offset; xref streams have "N 0 obj".
                    at_offset
                        .windows(5)
                        .take(20)
                        .any(|w| w == b"0 obj" || w == b"0 OBJ")
                } else {
                    false
                };

                if !has_valid_xref {
                    // The offset is wrong. Try to find the actual xref position.
                    if let Some(real_xref) = find_last_xref_pos(data) {
                        let mut repaired = data.to_vec();
                        // Rebuild the startxref section.
                        repaired.truncate(sxr);
                        repaired.extend_from_slice(
                            format!("startxref\n{real_xref}\n%%EOF\n").as_bytes(),
                        );
                        if let Some(doc) = try_load(&repaired) {
                            return Some(doc);
                        }
                        // If it still fails, try fixing xref line endings too.
                        if let Some(double_repaired) = try_fix_xref_line_endings(&repaired) {
                            if let Some(doc) = try_load(&double_repaired) {
                                return Some(doc);
                            }
                        }
                    }
                }
            }
        }
    }

    // Strategy: if no %%EOF in last 512 bytes but startxref exists earlier in file,
    // try appending %%EOF.
    let last_512 = &data[data.len().saturating_sub(512)..];
    let has_eof = last_512.windows(5).any(|w| w == b"%%EOF");

    if !has_eof {
        if let Some(real_xref) = find_last_xref_pos(data) {
            let mut repaired = data.to_vec();
            repaired.extend_from_slice(format!("\nstartxref\n{real_xref}\n%%EOF\n").as_bytes());
            if let Some(doc) = try_load(&repaired) {
                return Some(doc);
            }
        }
    }

    // Strategy: corrupt trailer dict (e.g., filled with `<<<<...`).
    // Reconstruct trailer from xref table by scanning for /Root reference.
    if let Some(repaired) = try_rebuild_trailer(data) {
        if let Some(doc) = try_load(&repaired) {
            return Some(doc);
        }
    }

    // Strategy: corrupt /Size value (e.g., "/Size (h)" instead of "/Size 80").
    // Fix by computing the correct size from the xref table.
    if let Some(repaired) = try_fix_trailer_size(data) {
        if let Some(doc) = try_load(&repaired) {
            return Some(doc);
        }
    }

    // Strategy: comment after startxref value (e.g., "23291 %comment").
    // Strip non-digit chars after the offset number.
    if let Some(repaired) = try_fix_startxref_comment(data) {
        if let Some(doc) = try_load(&repaired) {
            return Some(doc);
        }
    }

    // Strategy: xref entries with wrong line endings (19 bytes with LF only
    // instead of 20 bytes with CR+LF). Fix by normalizing to CR+LF.
    if let Some(repaired) = try_fix_xref_line_endings(data) {
        if let Some(doc) = try_load(&repaired) {
            return Some(doc);
        }
    }

    // Final strategy: rebuild xref from scratch by scanning for "N G obj" markers.
    // This handles PDFs with completely corrupt xref tables (wrong offsets, etc.).
    if let Some(doc) = try_rebuild_xref_from_objects(data) {
        return Some(doc);
    }

    None
}

/// Rebuild a PDF's xref table from scratch by scanning for object definitions.
///
/// Scans the file body for "N G obj" patterns, collects all object offsets,
/// finds /Root (Catalog), and builds a new valid xref table + trailer.
pub(crate) fn try_rebuild_xref_from_objects(data: &[u8]) -> Option<lopdf::Document> {
    // Scan for "N 0 obj" patterns (generation 0, which is most common).
    let mut objects: Vec<(u32, usize)> = Vec::new(); // (obj_num, offset)
    let mut root_ref: Option<u32> = None;
    let mut info_ref: Option<u32> = None;

    let mut pos = 0;
    while pos + 10 < data.len() {
        // Look for a digit followed by " 0 obj".
        if data[pos].is_ascii_digit() {
            // Check if this position starts a line (or is at the start).
            let at_line_start = pos == 0 || data[pos - 1] == b'\n' || data[pos - 1] == b'\r';
            if at_line_start {
                // Try to parse "N 0 obj" or "N G obj".
                let end = std::cmp::min(pos + 20, data.len());
                let chunk = &data[pos..end];
                if let Some(obj_info) = parse_obj_marker(chunk) {
                    objects.push((obj_info.0, pos));
                    // Check if this object is /Type /Catalog.
                    // Use a 64 KiB window — large objects (e.g. signed PDFs with
                    // embedded PKCS7 data) can push /Type/Catalog far from the header.
                    let obj_end = std::cmp::min(pos + 65536, data.len());
                    let obj_data = &data[pos..obj_end];
                    let has_catalog = obj_data
                        .windows(b"/Type /Catalog".len())
                        .any(|w| w == b"/Type /Catalog")
                        || obj_data
                            .windows(b"/Type/Catalog".len())
                            .any(|w| w == b"/Type/Catalog");
                    if has_catalog {
                        root_ref = Some(obj_info.0);
                    }
                    if obj_data
                        .windows(8)
                        .any(|w| w == b"/Author " || w == b"/Creator")
                        && obj_data
                            .windows(14)
                            .any(|w| w == b"/CreationDate " || w == b"/ModDate ")
                        && info_ref.is_none()
                    {
                        info_ref = Some(obj_info.0);
                    }
                    // Skip past "obj" to avoid re-matching.
                    pos += 5;
                    continue;
                }
            }
        }
        pos += 1;
    }

    // If no Catalog found via object scan (e.g. it's in a compressed ObjStm),
    // try to extract /Root from an existing trailer dict as a fallback. (#445)
    if root_ref.is_none() {
        root_ref = extract_root_from_trailer(data);
    }

    if objects.is_empty() || root_ref.is_none() {
        return None;
    }

    // Sort by object number.
    objects.sort_by_key(|&(num, _)| num);
    objects.dedup_by_key(|o| o.0);

    let max_obj = objects.last().map(|o| o.0).unwrap_or(0);
    let size = max_obj + 1;

    // Build xref table.
    let mut xref_entries = Vec::new();
    // Entry 0: free list head. Exactly 20 bytes: 10+1+5+1+1+CR+LF.
    xref_entries.push(format!("{:010} {:05} f\r\n", 0, 65535));

    let obj_map: std::collections::HashMap<u32, usize> = objects.iter().cloned().collect();

    for num in 1..size {
        if let Some(&offset) = obj_map.get(&num) {
            xref_entries.push(format!("{:010} {:05} n\r\n", offset, 0));
        } else {
            xref_entries.push(format!("{:010} {:05} f\r\n", 0, 0));
        }
    }

    // Find %PDF header for clean data start.
    let header_start = data.windows(5).position(|w| w == b"%PDF-").unwrap_or(0);
    let body = &data[header_start..];

    // Find the end of the last object (before any existing xref/trailer).
    // For linearized PDFs the xref table appears BEFORE the page content objects,
    // so we must not cut at the xref position — use the full body instead and
    // let our new trailing xref+trailer take precedence over any embedded one.
    let xref_cut = find_last_xref_pos(body);
    let last_endobj = body.windows(6).rposition(|w| w == b"endobj").map(|p| p + 6);
    // If there are objects after the xref cut point, include everything.
    let body_end = match (xref_cut, last_endobj) {
        (Some(xp), Some(le)) if le > xp => body.len(),
        (Some(xp), _) => xp,
        _ => body.len(),
    };

    // Build new PDF: original body + new xref + trailer.
    let mut repaired = body[..body_end].to_vec();
    // Ensure newline before xref.
    if !repaired.ends_with(b"\n") {
        repaired.push(b'\n');
    }
    let xref_pos = repaired.len();
    repaired.extend_from_slice(format!("xref\n0 {size}\n").as_bytes());
    for entry in &xref_entries {
        repaired.extend_from_slice(entry.as_bytes());
    }

    // Trailer.
    let root = root_ref.unwrap();
    repaired.extend_from_slice(b"trailer\n<<\n");
    repaired.extend_from_slice(format!("/Size {size}\n/Root {root} 0 R\n").as_bytes());
    if let Some(info) = info_ref {
        repaired.extend_from_slice(format!("/Info {info} 0 R\n").as_bytes());
    }
    repaired.extend_from_slice(b">>\n");
    repaired.extend_from_slice(format!("startxref\n{xref_pos}\n%%EOF\n").as_bytes());

    try_load(&repaired)
}

/// Scan raw PDF bytes for a `trailer` dict and extract the `/Root N 0 R` value.
///
/// Used as a fallback when the Catalog can't be found by scanning object bodies
/// (e.g. the Catalog is inside a compressed ObjStm).  Only reads the last trailer
/// section so cross-reference updates are respected.  (#445)
pub(crate) fn extract_root_from_trailer(data: &[u8]) -> Option<u32> {
    // Search backward from the end for "trailer" keyword.
    let mut search_end = data.len();
    while search_end >= 7 {
        let Some(pos) = data[..search_end].windows(7).rposition(|w| w == b"trailer") else {
            break;
        };
        // Skip "startxref" false matches — "trailer" can appear in different contexts.
        let after = &data[pos + 7..];
        // Find "/Root" inside the trailer dict (<< ... >>).
        if let Some(root_offset) = after.windows(5).position(|w| w == b"/Root") {
            let after_root = &after[root_offset + 5..];
            // Skip whitespace.
            let digits_start = after_root
                .iter()
                .position(|b| b.is_ascii_digit())
                .unwrap_or(after_root.len());
            let digits: Vec<u8> = after_root[digits_start..]
                .iter()
                .take_while(|b| b.is_ascii_digit())
                .copied()
                .collect();
            if let Ok(s) = std::str::from_utf8(&digits) {
                if let Ok(n) = s.parse::<u32>() {
                    if n > 0 {
                        return Some(n);
                    }
                }
            }
        }
        search_end = pos;
    }
    None
}

/// Parse "N G obj" at the start of a byte slice. Returns (obj_num, gen_num) if valid.
pub(crate) fn parse_obj_marker(data: &[u8]) -> Option<(u32, u16)> {
    let s = std::str::from_utf8(data).ok()?;
    let mut parts = s.split_whitespace();
    let num: u32 = parts.next()?.parse().ok()?;
    let gen: u16 = parts.next()?.parse().ok()?;
    let keyword = parts.next()?;
    if keyword == "obj" {
        Some((num, gen))
    } else {
        None
    }
}

/// Rebuild a corrupt trailer by scanning for /Root in the xref range.
pub(crate) fn try_rebuild_trailer(data: &[u8]) -> Option<Vec<u8>> {
    // Find the xref table.
    let xref_pos = find_last_xref_pos(data)?;
    let after_xref = &data[xref_pos..];

    // Parse the xref subsection header: "xref\nSTART COUNT\n"
    let header_end = after_xref
        .windows(1)
        .skip(5) // "xref\n"
        .position(|w| w[0] == b'\n')
        .map(|p| p + 5 + 1)?;

    let header_line = std::str::from_utf8(&after_xref[5..header_end]).ok()?;
    let parts: Vec<&str> = header_line.split_whitespace().collect();
    if parts.len() != 2 {
        return None;
    }
    let _start: usize = parts[0].parse().ok()?;
    let count: usize = parts[1].parse().ok()?;

    // Find the trailer keyword.
    let trailer_pos = data[xref_pos..].windows(7).position(|w| w == b"trailer")?;
    let abs_trailer = xref_pos + trailer_pos;

    // Check if trailer dict is corrupt (e.g., filled with `<` chars).
    let after_trailer = &data[abs_trailer + 7..];
    let dict_start = after_trailer
        .iter()
        .position(|&b| !b.is_ascii_whitespace())?;
    let dict_data = &after_trailer[dict_start..];

    // A valid trailer dict starts with "<<" followed by something other than "<".
    let is_corrupt = dict_data.starts_with(b"<<<") || !dict_data.starts_with(b"<<");

    if !is_corrupt {
        return None;
    }

    // Scan all xref entries to find /Root by checking each object.
    // Parse xref entries (20 bytes each): "OFFSET GENERATION STATUS\n"
    let entries_start = xref_pos + header_end;
    let mut root_ref = None;
    for i in 1..count {
        let entry_offset = entries_start + i * 20;
        if entry_offset + 20 > data.len() {
            break;
        }
        let entry = &data[entry_offset..entry_offset + 20];
        if entry[17] == b'f' {
            continue; // free entry
        }
        let offset_str = std::str::from_utf8(&entry[..10]).ok()?;
        let obj_offset: usize = offset_str.trim().parse().ok()?;
        if obj_offset + 20 >= data.len() {
            continue;
        }
        // Check if this object contains /Type /Catalog.
        let obj_data = &data[obj_offset..std::cmp::min(obj_offset + 4096, data.len())];
        if obj_data
            .windows(14)
            .any(|w| w == b"/Type /Catalog" || w == b"/Type/Catalog")
        {
            root_ref = Some(i);
            break;
        }
    }

    let root_ref = root_ref?;

    // Rebuild: everything up to trailer, then a valid trailer dict.
    let mut repaired = data[..abs_trailer].to_vec();
    repaired.extend_from_slice(
        format!(
            "trailer\n<< /Size {count} /Root {root_ref} 0 R >>\nstartxref\n{xref_pos}\n%%EOF\n"
        )
        .as_bytes(),
    );
    Some(repaired)
}

/// Fix trailer with invalid /Size value (e.g., "/Size (h)" instead of "/Size 80").
pub(crate) fn try_fix_trailer_size(data: &[u8]) -> Option<Vec<u8>> {
    // Find "/Size" in the trailer.
    let trailer_pos = data.windows(7).rposition(|w| w == b"trailer")?;
    let after_trailer = &data[trailer_pos..];

    // Find /Size followed by non-integer value.
    let size_pos = after_trailer.windows(5).position(|w| w == b"/Size")?;
    let abs_size = trailer_pos + size_pos;
    let after_size = &data[abs_size + 5..];

    // Skip whitespace after /Size.
    let val_start = after_size.iter().position(|b| !b.is_ascii_whitespace())?;

    // If the value starts with a digit, /Size is already valid.
    if after_size[val_start].is_ascii_digit() {
        return None;
    }

    // Compute the correct size from the xref table.
    let xref_pos = find_last_xref_pos(data)?;
    let after_xref = &data[xref_pos..];
    let header_end = after_xref
        .windows(1)
        .skip(5)
        .position(|w| w[0] == b'\n')
        .map(|p| p + 5 + 1)?;
    let header_line = std::str::from_utf8(&after_xref[5..header_end]).ok()?;
    let parts: Vec<&str> = header_line.split_whitespace().collect();
    if parts.len() != 2 {
        return None;
    }
    let count: usize = parts[1].parse().ok()?;

    // Find the end of the bad value (next / or >>).
    let val_end = after_size[val_start..]
        .iter()
        .position(|&b| b == b'/' || b == b'>')
        .unwrap_or(after_size.len() - val_start);

    let mut repaired = data[..abs_size + 5].to_vec();
    repaired.push(b' ');
    repaired.extend_from_slice(count.to_string().as_bytes());
    repaired.push(b' ');
    repaired.extend_from_slice(&data[abs_size + 5 + val_start + val_end..]);
    Some(repaired)
}

/// Fix startxref with trailing comment (e.g., "23291 %Must be...").
pub(crate) fn try_fix_startxref_comment(data: &[u8]) -> Option<Vec<u8>> {
    let search_start = data.len().saturating_sub(4096);
    let sxr_pos = data[search_start..]
        .windows(9)
        .rposition(|w| w == b"startxref")
        .map(|p| search_start + p)?;

    let after = &data[sxr_pos + 9..];

    // Extract digits.
    let digits_start = after.iter().position(|b| b.is_ascii_digit())?;
    let digits_end = after[digits_start..]
        .iter()
        .position(|b| !b.is_ascii_digit())
        .unwrap_or(after.len() - digits_start);

    // Check if there's non-whitespace, non-EOF content after the digits.
    let remainder = &after[digits_start + digits_end..];
    let has_garbage = remainder
        .iter()
        .take_while(|&&b| b != b'%' || remainder.windows(5).any(|w| w != b"%%EOF"))
        .any(|b| !b.is_ascii_whitespace() && *b != b'%');

    // Also check: is there a '%' that is NOT '%%EOF'?
    let after_digits = &after[digits_start + digits_end..];
    let trimmed = after_digits
        .iter()
        .skip_while(|b| b.is_ascii_whitespace())
        .cloned()
        .collect::<Vec<u8>>();
    let needs_fix = !trimmed.is_empty()
        && !trimmed.starts_with(b"%%EOF")
        && (trimmed[0] == b'%' || has_garbage);

    if !needs_fix {
        return None;
    }

    let offset_str = std::str::from_utf8(&after[digits_start..digits_start + digits_end]).ok()?;

    let mut repaired = data[..sxr_pos].to_vec();
    repaired.extend_from_slice(format!("startxref\n{offset_str}\n%%EOF\n").as_bytes());
    Some(repaired)
}

/// Fix xref entries with wrong line endings (19 bytes with LF only).
/// lopdf requires exactly 20-byte entries. Normalizes to CR+LF.
pub(crate) fn try_fix_xref_line_endings(data: &[u8]) -> Option<Vec<u8>> {
    let xref_pos = find_last_xref_pos(data)?;

    // Find subsection header: "xref\nSTART COUNT\n"
    let after = &data[xref_pos + 4..]; // skip "xref"
    let header_start = after.iter().position(|b| !b.is_ascii_whitespace())?;
    let header_end = after[header_start..]
        .iter()
        .position(|&b| b == b'\n')
        .map(|p| header_start + p + 1)?;

    let header_line = std::str::from_utf8(&after[header_start..header_end]).ok()?;
    let parts: Vec<&str> = header_line.split_whitespace().collect();
    if parts.len() != 2 {
        return None;
    }
    let count: usize = parts[1].parse().ok()?;

    // Find trailer position to delimit entries.
    let trailer_pos = data[xref_pos..]
        .windows(7)
        .position(|w| w == b"trailer")
        .map(|p| xref_pos + p)?;

    let entries_start = xref_pos + 4 + header_end;
    let entries_data = &data[entries_start..trailer_pos];

    // Split entries by newline.
    let entries: Vec<&[u8]> = entries_data
        .split(|&b| b == b'\n')
        .filter(|e| !e.is_empty())
        .collect();

    if entries.len() != count {
        return None;
    }

    // Check if entries need fixing. After split by \n, entries with CR+LF
    // end with \r (19 bytes), entries with LF-only don't (18 bytes).
    // lopdf requires exactly 20-byte entries (content + SP + CR + LF).
    let has_cr = entries.iter().any(|e| e.ends_with(b"\r"));
    if has_cr {
        // Already has CR+LF line endings — check for other issues.
        let all_correct = entries.iter().all(|e| e.ends_with(b"\r") && e.len() == 19);
        if all_correct {
            return None;
        }
    }

    // Rebuild with proper 20-byte entries (content + CR + LF).
    let mut repaired = data[..entries_start].to_vec();
    for entry in &entries {
        let stripped = if entry.ends_with(b"\r") {
            &entry[..entry.len() - 1]
        } else {
            entry
        };
        // Pad to 18 chars if needed (left-pad offset with zeros).
        if stripped.len() == 18 {
            repaired.extend_from_slice(stripped);
        } else {
            // Try to parse and reformat.
            let s = std::str::from_utf8(stripped).ok()?;
            let parts: Vec<&str> = s.split_whitespace().collect();
            if parts.len() != 3 {
                return None;
            }
            repaired.extend_from_slice(
                format!("{:0>10} {:0>5} {}", parts[0], parts[1], parts[2]).as_bytes(),
            );
        }
        repaired.extend_from_slice(b"\r\n");
    }
    repaired.extend_from_slice(&data[trailer_pos..]);

    // Fix startxref to point to the xref in the repaired data.
    // The xref position hasn't changed since we only modified entry content.
    Some(repaired)
}

/// Find the byte offset of the last valid standalone "xref" keyword in the data.
/// Skips "xref" substrings inside "startxref" and empty xref tables.
pub(crate) fn find_last_xref_pos(data: &[u8]) -> Option<usize> {
    // Iterate backwards through all "xref" matches.
    let mut search_end = data.len();
    while search_end >= 4 {
        if let Some(pos) = data[..search_end].windows(4).rposition(|w| w == b"xref") {
            // Skip if this is part of "startxref".
            if pos >= 5 && &data[pos - 5..pos] == b"start" {
                search_end = pos;
                continue;
            }
            // Verify it's at the start of a line.
            if pos == 0 || data[pos - 1] == b'\n' || data[pos - 1] == b'\r' {
                // Skip empty xref tables ("xref\ntrailer" with no entries).
                let after = &data[pos + 4..];
                let non_ws = after
                    .iter()
                    .position(|b| !b.is_ascii_whitespace())
                    .map(|p| &after[p..])
                    .unwrap_or(b"");
                if non_ws.starts_with(b"trailer") {
                    search_end = pos;
                    continue;
                }
                return Some(pos);
            }
            search_end = pos;
        } else {
            break;
        }
    }
    None
}

/// Try to fix lopdf documents where get_pages() returns empty because page
/// dictionaries lack a /Type /Page entry.
///
/// Walks the page tree from the catalog and adds /Type /Page to leaf nodes
/// that have a /MediaBox (strong indicator of a page). Returns true if pages
/// were found after the fix.
/// Fix a wrong Root reference in the trailer.
///
/// Some corrupt PDFs have a trailer that points to a non-Catalog object.
/// We check if the Root object has `/Type /Catalog`; if not, scan all objects
/// to find the real catalog and update the trailer.
pub(crate) fn fix_wrong_root(doc: &mut lopdf::Document) {
    let catalog_id = find_catalog_object(doc);

    let root_id = match doc.trailer.get(b"Root").ok() {
        Some(lopdf::Object::Reference(id)) => *id,
        _ => {
            if let Some(id) = catalog_id {
                doc.trailer.set("Root", lopdf::Object::Reference(id));
            }
            return;
        }
    };

    // Check if Root actually has /Type /Catalog.
    let is_catalog = match doc.objects.get(&root_id) {
        Some(lopdf::Object::Dictionary(d)) => {
            matches!(d.get(b"Type").ok(), Some(lopdf::Object::Name(n)) if n == b"Catalog")
        }
        _ => false,
    };

    if is_catalog {
        return; // Root is correct.
    }

    if let Some(id) = catalog_id {
        doc.trailer.set("Root", lopdf::Object::Reference(id));
    }
}

pub(crate) fn find_catalog_object(doc: &lopdf::Document) -> Option<lopdf::ObjectId> {
    let mut catalog_without_pages: Option<lopdf::ObjectId> = None;
    let mut pages_holder: Option<lopdf::ObjectId> = None;

    for (id, obj) in &doc.objects {
        if let lopdf::Object::Dictionary(d) = obj {
            let is_catalog =
                matches!(d.get(b"Type").ok(), Some(lopdf::Object::Name(n)) if n == b"Catalog");
            let has_pages = d.has(b"Pages");

            if is_catalog && has_pages {
                return Some(*id);
            }

            if is_catalog && catalog_without_pages.is_none() {
                catalog_without_pages = Some(*id);
            }

            if has_pages && pages_holder.is_none() {
                pages_holder = Some(*id);
            }
        }
    }

    catalog_without_pages.or(pages_holder)
}

pub(crate) fn try_fix_missing_page_types(doc: &mut lopdf::Document) -> bool {
    // Collect page-like object IDs from the page tree.
    let page_ids = collect_page_tree_leaves(doc);

    if page_ids.is_empty() {
        return false;
    }

    let mut fixed = false;
    for page_id in &page_ids {
        if let Ok(dict) = doc.get_dictionary_mut(*page_id) {
            // Only fix if it doesn't already have /Type or has wrong type.
            let needs_fix = !matches!(dict.get_type(), Ok(t) if t == b"Page");

            if needs_fix {
                // Verify it looks like a page (has MediaBox or Contents).
                let has_media_box = dict.get(b"MediaBox").is_ok();
                let has_contents = dict.get(b"Contents").is_ok();
                let has_parent = dict.get(b"Parent").is_ok();

                if has_media_box || has_contents || has_parent {
                    dict.set("Type", lopdf::Object::Name(b"Page".to_vec()));
                    fixed = true;
                }
            }
        }
    }

    if fixed {
        // Re-check if pages are now found.
        !doc.get_pages().is_empty()
    } else {
        false
    }
}

/// Normalize page tree node /Type values to avoid malformed tree nodes.
///
/// Some repaired PDFs contain page-tree dictionaries without explicit /Type,
/// which can make validators fail with "unknown type of page tree node".
/// Rules applied:
/// - node with non-empty /Kids => /Type /Pages
/// - leaf node with page-like keys => /Type /Page
pub(crate) fn normalize_page_tree_types(doc: &mut lopdf::Document) -> usize {
    let pages_id = match doc
        .catalog()
        .and_then(|cat| cat.get(b"Pages"))
        .and_then(lopdf::Object::as_reference)
    {
        Ok(id) => id,
        Err(_) => return 0,
    };

    let mut visited = std::collections::HashSet::new();
    let mut stack = vec![pages_id];
    let mut set_pages: Vec<lopdf::ObjectId> = Vec::new();
    let mut set_page: Vec<lopdf::ObjectId> = Vec::new();
    let limit = doc.objects.len().min(10_000);

    while let Some(id) = stack.pop() {
        if !visited.insert(id) || visited.len() > limit {
            continue;
        }
        let dict = match doc.get_dictionary(id) {
            Ok(d) => d,
            Err(_) => continue,
        };

        let kids: Vec<lopdf::ObjectId> = match dict.get(b"Kids").and_then(lopdf::Object::as_array) {
            Ok(arr) => arr.iter().filter_map(|o| o.as_reference().ok()).collect(),
            Err(_) => Vec::new(),
        };

        if !kids.is_empty() {
            let is_pages = matches!(dict.get_type(), Ok(t) if t == b"Pages");
            if !is_pages {
                set_pages.push(id);
            }
            for kid in kids {
                stack.push(kid);
            }
            continue;
        }

        let looks_page = dict.get(b"MediaBox").is_ok()
            || dict.get(b"Contents").is_ok()
            || dict.get(b"Parent").is_ok();
        let is_page = matches!(dict.get_type(), Ok(t) if t == b"Page");
        if looks_page && !is_page {
            set_page.push(id);
        }
    }

    let mut fixed = 0usize;
    for id in set_pages {
        if let Ok(dict) = doc.get_dictionary_mut(id) {
            dict.set("Type", lopdf::Object::Name(b"Pages".to_vec()));
            fixed += 1;
        }
    }
    for id in set_page {
        if let Ok(dict) = doc.get_dictionary_mut(id) {
            dict.set("Type", lopdf::Object::Name(b"Page".to_vec()));
            fixed += 1;
        }
    }

    fixed
}

/// Remove invalid `0 0 R` (free-object) references from page-tree Kids arrays.
///
/// Linearized or severely corrupt PDFs repaired by lopdf sometimes end up with
/// null-object references in Kids arrays. veraPDF crashes with "unknown type of
/// page tree node" when it encounters these. Stripping them prevents the crash.
/// (#poppler-22493)
pub(crate) fn strip_null_page_kids(doc: &mut lopdf::Document) {
    // Collect IDs of Pages nodes whose Kids contain 0 0 R entries.
    let pages_ids: Vec<lopdf::ObjectId> = doc
        .objects
        .iter()
        .filter_map(|(id, obj)| {
            let lopdf::Object::Dictionary(dict) = obj else {
                return None;
            };
            let Ok(lopdf::Object::Array(kids)) = dict.get(b"Kids") else {
                return None;
            };
            let has_null = kids
                .iter()
                .any(|k| matches!(k, lopdf::Object::Reference(id) if id.0 == 0));
            if has_null {
                Some(*id)
            } else {
                None
            }
        })
        .collect();

    for id in pages_ids {
        // Clone Kids, filter out 0 0 R, then write back.
        let filtered: Vec<lopdf::Object> = {
            let dict = match doc.objects.get(&id) {
                Some(lopdf::Object::Dictionary(d)) => d,
                _ => continue,
            };
            let Ok(lopdf::Object::Array(kids)) = dict.get(b"Kids") else {
                continue;
            };
            kids.iter()
                .filter(|k| !matches!(k, lopdf::Object::Reference(r) if r.0 == 0))
                .cloned()
                .collect()
        };
        if let Some(lopdf::Object::Dictionary(dict)) = doc.objects.get_mut(&id) {
            let count = filtered.len() as i64;
            dict.set("Kids", lopdf::Object::Array(filtered));
            // Update Count to match the number of remaining immediate Kids.
            // (A fully accurate recursive count is expensive; a conservative
            // Kids.len() count is better than a stale inflated value.)
            dict.set("Count", lopdf::Object::Integer(count));
        }
    }
}

/// Create a minimal valid page tree when all page recovery attempts failed.
///
/// Returns true when a placeholder page was created.
pub(crate) fn ensure_placeholder_page_tree(doc: &mut lopdf::Document) -> bool {
    if !doc.get_pages().is_empty() {
        return false;
    }

    // Get or create the Pages reference in the Catalog.
    // Some extremely corrupt PDFs have /Pages as a Name value (e.g. `/Pages /Pages`)
    // instead of a Reference. When detected, create a new Pages node and update
    // the Catalog. Fixes: GHOSTSCRIPT-698804-0.pdf.
    let catalog_id = match doc.trailer.get(b"Root").ok() {
        Some(lopdf::Object::Reference(id)) => *id,
        _ => return false,
    };
    let pages_value = doc
        .objects
        .get(&catalog_id)
        .and_then(|o| o.as_dict().ok())
        .and_then(|d| d.get(b"Pages").ok())
        .cloned();
    let pages_id = match pages_value {
        Some(lopdf::Object::Reference(id)) => id,
        Some(lopdf::Object::Name(_)) => {
            // /Pages is a Name — create a new Pages node and point the Catalog at it.
            let new_pages_id = doc.new_object_id();
            doc.objects.insert(
                new_pages_id,
                lopdf::Object::Dictionary(lopdf::Dictionary::new()),
            );
            if let Some(lopdf::Object::Dictionary(cat)) = doc.objects.get_mut(&catalog_id) {
                cat.set("Pages", lopdf::Object::Reference(new_pages_id));
                // These keys belong in the Pages node, not the Catalog.
                cat.remove(b"Kids");
                cat.remove(b"Count");
            }
            new_pages_id
        }
        _ => return false,
    };

    let content_id = doc.new_object_id();
    let empty_stream = lopdf::Stream::new(lopdf::Dictionary::new(), Vec::new());
    doc.objects
        .insert(content_id, lopdf::Object::Stream(empty_stream));

    let page_id = doc.new_object_id();
    let mut page = lopdf::Dictionary::new();
    page.set("Type", lopdf::Object::Name(b"Page".to_vec()));
    page.set("Parent", lopdf::Object::Reference(pages_id));
    page.set(
        "MediaBox",
        lopdf::Object::Array(vec![
            lopdf::Object::Integer(0),
            lopdf::Object::Integer(0),
            lopdf::Object::Integer(612),
            lopdf::Object::Integer(792),
        ]),
    );
    page.set(
        "Resources",
        lopdf::Object::Dictionary(lopdf::Dictionary::new()),
    );
    page.set("Contents", lopdf::Object::Reference(content_id));
    doc.objects.insert(page_id, lopdf::Object::Dictionary(page));

    // If the Pages node doesn't exist yet (e.g. the encrypted/compressed xref
    // section was never loaded), insert a fresh empty dict so we can set it up.
    // Without this, get_dictionary_mut returns Err and the placeholder is never
    // created, leaving the Catalog pointing to a non-existent 62 0 R.
    doc.objects
        .entry(pages_id)
        .or_insert_with(|| lopdf::Object::Dictionary(lopdf::Dictionary::new()));

    if let Ok(pages) = doc.get_dictionary_mut(pages_id) {
        pages.set("Type", lopdf::Object::Name(b"Pages".to_vec()));
        pages.set(
            "Kids",
            lopdf::Object::Array(vec![lopdf::Object::Reference(page_id)]),
        );
        pages.set("Count", lopdf::Object::Integer(1));
    }

    true
}

/// Walk the page tree from the catalog's /Pages entry and collect leaf node IDs.
/// Unlike lopdf's get_pages(), this doesn't require /Type to be present.
pub(crate) fn collect_page_tree_leaves(doc: &lopdf::Document) -> Vec<lopdf::ObjectId> {
    let mut leaves = Vec::new();

    let pages_id = match doc
        .catalog()
        .and_then(|cat| cat.get(b"Pages"))
        .and_then(lopdf::Object::as_reference)
    {
        Ok(id) => id,
        Err(_) => return leaves,
    };

    let mut stack = vec![pages_id];
    let mut visited = std::collections::HashSet::new();
    let limit = doc.objects.len().min(10_000);

    while let Some(id) = stack.pop() {
        if !visited.insert(id) || visited.len() > limit {
            continue;
        }

        let dict = match doc.get_dictionary(id) {
            Ok(d) => d,
            Err(_) => continue,
        };

        // Check if this node has /Kids — if so, it's a Pages node.
        if let Ok(kids) = dict.get(b"Kids").and_then(lopdf::Object::as_array) {
            for kid in kids {
                if let Ok(kid_id) = kid.as_reference() {
                    stack.push(kid_id);
                }
            }
        } else {
            // No /Kids — this is a leaf (page) node.
            leaves.push(id);
        }
    }

    leaves
}
