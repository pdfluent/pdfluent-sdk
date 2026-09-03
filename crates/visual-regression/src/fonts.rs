// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! A tiny original TrueType face: A, B and C polygon glyphs, no external font.
use std::collections::BTreeMap;
fn u16s(values: &[u16]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_be_bytes()).collect()
}
fn put16(data: &mut [u8], pos: usize, value: u16) {
    data[pos..pos + 2].copy_from_slice(&value.to_be_bytes());
}
fn put32(data: &mut [u8], pos: usize, value: u32) {
    data[pos..pos + 4].copy_from_slice(&value.to_be_bytes());
}
fn checksum(data: &[u8]) -> u32 {
    data.chunks(4)
        .map(|c| {
            let mut b = [0; 4];
            b[..c.len()].copy_from_slice(c);
            u32::from_be_bytes(b)
        })
        .fold(0, u32::wrapping_add)
}
pub fn truetype() -> Vec<u8> {
    let mut tables: BTreeMap<&str, Vec<u8>> = BTreeMap::new();
    let mut head = vec![0; 54];
    put32(&mut head, 0, 0x10000);
    put32(&mut head, 12, 0x5f0f3cf5);
    put16(&mut head, 18, 1000);
    put16(&mut head, 40, 600);
    put16(&mut head, 42, 700);
    put16(&mut head, 46, 8);
    put16(&mut head, 48, 2);
    put16(&mut head, 50, 1);
    tables.insert("head", head);
    let mut hhea = vec![0; 36];
    put32(&mut hhea, 0, 0x10000);
    put16(&mut hhea, 4, 800);
    put16(&mut hhea, 6, (-200i16) as u16);
    put16(&mut hhea, 10, 650);
    put16(&mut hhea, 18, 1);
    put16(&mut hhea, 34, 4);
    tables.insert("hhea", hhea);
    let mut maxp = vec![0; 32];
    put32(&mut maxp, 0, 0x10000);
    put16(&mut maxp, 4, 4);
    put16(&mut maxp, 6, 12);
    put16(&mut maxp, 8, 1);
    put16(&mut maxp, 14, 1);
    tables.insert("maxp", maxp);
    tables.insert("hmtx", u16s(&[650, 0, 650, 0, 650, 0, 650, 0]));
    let outlines: [&[(i16, i16)]; 4] = [
        &[],
        &[
            (50, 0),
            (270, 700),
            (380, 700),
            (600, 0),
            (470, 0),
            (325, 500),
            (180, 0),
        ],
        &[
            (70, 0),
            (70, 700),
            (430, 700),
            (560, 570),
            (420, 350),
            (560, 130),
            (430, 0),
        ],
        &[
            (570, 80),
            (430, 0),
            (180, 0),
            (50, 150),
            (50, 550),
            (180, 700),
            (430, 700),
            (570, 620),
            (450, 500),
            (200, 500),
            (200, 200),
            (450, 200),
        ],
    ];
    let mut glyf = Vec::new();
    let mut loca = Vec::new();
    for outline in outlines {
        loca.extend_from_slice(&(glyf.len() as u32).to_be_bytes());
        if outline.is_empty() {
            continue;
        }
        glyf.extend(u16s(&[1, 0, 0, 600, 700, (outline.len() - 1) as u16, 0]));
        glyf.extend(vec![1; outline.len()]);
        for axis in 0..2 {
            let mut previous = 0i16;
            for &(x, y) in outline {
                let next = if axis == 0 { x } else { y };
                glyf.extend_from_slice(&(next - previous).to_be_bytes());
                previous = next;
            }
        }
        while !glyf.len().is_multiple_of(4) {
            glyf.push(0);
        }
    }
    loca.extend_from_slice(&(glyf.len() as u32).to_be_bytes());
    tables.insert("glyf", glyf);
    tables.insert("loca", loca);
    let mut cmap = u16s(&[0, 1, 3, 1]);
    cmap.extend_from_slice(&12u32.to_be_bytes());
    cmap.extend(u16s(&[
        4,
        32,
        0,
        4,
        4,
        1,
        0,
        67,
        65535,
        0,
        65,
        65535,
        (-64i16) as u16,
        1,
        0,
        0,
    ]));
    tables.insert("cmap", cmap);
    let name: Vec<u8> = "VRPolygon"
        .encode_utf16()
        .flat_map(u16::to_be_bytes)
        .collect();
    let mut names = u16s(&[0, 1, 18, 3, 1, 0x409, 6, name.len() as u16, 0]);
    names.extend(name);
    tables.insert("name", names);
    let mut post = vec![0; 32];
    put32(&mut post, 0, 0x30000);
    tables.insert("post", post);
    let mut os2 = vec![0; 78];
    put16(&mut os2, 2, 650);
    put16(&mut os2, 4, 400);
    put16(&mut os2, 6, 5);
    put16(&mut os2, 64, 65);
    put16(&mut os2, 66, 67);
    put16(&mut os2, 68, 800);
    put16(&mut os2, 70, (-200i16) as u16);
    put16(&mut os2, 74, 800);
    put16(&mut os2, 76, 200);
    tables.insert("OS/2", os2);
    let n = tables.len();
    let mut output = vec![0; 12 + 16 * n];
    put32(&mut output, 0, 0x10000);
    put16(&mut output, 4, n as u16);
    put16(&mut output, 6, 128);
    put16(&mut output, 8, 3);
    put16(&mut output, 10, (n * 16 - 128) as u16);
    let mut head_offset = 0;
    for (i, (tag, data)) in tables.into_iter().enumerate() {
        let at = 12 + i * 16;
        output[at..at + 4].copy_from_slice(tag.as_bytes());
        put32(&mut output, at + 4, checksum(&data));
        let offset = output.len();
        put32(&mut output, at + 8, offset as u32);
        put32(&mut output, at + 12, data.len() as u32);
        if tag == "head" {
            head_offset = offset;
        }
        output.extend(data);
        while !output.len().is_multiple_of(4) {
            output.push(0);
        }
    }
    let adjustment = 0xb1b0afbau32.wrapping_sub(checksum(&output));
    put32(&mut output, head_offset + 8, adjustment);
    output
}
