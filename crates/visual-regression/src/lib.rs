// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Self-referential, lossless regression checks through the public SDK engine.
mod encryption;
pub mod fixtures;
mod fonts;

use pdf_engine::{PdfDocument, RenderOptions};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Tolerance {
    pub channel: u8,
    pub fraction: f64,
}
impl Default for Tolerance {
    fn default() -> Self {
        Self {
            channel: 2,
            fraction: 0.0,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    default: Tolerance,
    #[serde(default)]
    fixtures: BTreeMap<String, Tolerance>,
}
pub fn tolerances(names: &[&str]) -> Result<BTreeMap<String, Tolerance>> {
    let config: Config = toml::from_str(&fs::read_to_string(root().join("tolerances.toml"))?)?;
    for (name, t) in &config.fixtures {
        if !names.contains(&name.as_str()) {
            return Err(format!("unknown tolerance fixture: {name}").into());
        }
        validate_tolerance(*t)?;
    }
    validate_tolerance(config.default)?;
    Ok(names
        .iter()
        .map(|name| {
            (
                name.to_string(),
                config
                    .fixtures
                    .get(*name)
                    .copied()
                    .unwrap_or(config.default),
            )
        })
        .collect())
}
fn validate_tolerance(t: Tolerance) -> Result<()> {
    if !t.fraction.is_finite() || !(0.0..=1.0).contains(&t.fraction) {
        return Err("fraction must be finite and between 0 and 1".into());
    }
    Ok(())
}

#[derive(Debug)]
pub struct Difference {
    pub dimensions_match: bool,
    pub count: usize,
    pub total: usize,
    pub bounds: Option<[u32; 4]>,
}
impl Difference {
    pub fn fraction(&self) -> f64 {
        self.count as f64 / self.total as f64
    }
    pub fn fails(&self, tolerance: Tolerance) -> bool {
        !self.dimensions_match || self.fraction() > tolerance.fraction
    }
}
fn valid(page: &Page) {
    assert!(page.width > 0 && page.height > 0);
    assert_eq!(
        page.pixels.len(),
        page.width as usize * page.height as usize * 4
    );
}
fn changed(a: &[u8], b: &[u8], channel: u8) -> bool {
    a.iter().zip(b).any(|(x, y)| x.abs_diff(*y) > channel)
}
pub fn compare(expected: &Page, actual: &Page, tolerance: Tolerance) -> Difference {
    valid(expected);
    valid(actual);
    let width = expected.width.max(actual.width);
    let height = expected.height.max(actual.height);
    let mut d = Difference {
        dimensions_match: expected.width == actual.width && expected.height == actual.height,
        count: 0,
        total: width as usize * height as usize,
        bounds: None,
    };
    for y in 0..height {
        for x in 0..width {
            let a = pixel(expected, x, y);
            let b = pixel(actual, x, y);
            let differs = match (a, b) {
                (Some(a), Some(b)) => changed(a, b, tolerance.channel),
                (None, None) => false,
                _ => true,
            };
            if differs {
                d.count += 1;
                d.bounds = Some(match d.bounds {
                    None => [x, y, x, y],
                    Some([x0, y0, x1, y1]) => [x0.min(x), y0.min(y), x1.max(x), y1.max(y)],
                });
            }
        }
    }
    d
}
fn pixel(page: &Page, x: u32, y: u32) -> Option<&[u8]> {
    if x >= page.width || y >= page.height {
        return None;
    }
    let i = (y as usize * page.width as usize + x as usize) * 4;
    Some(&page.pixels[i..i + 4])
}
pub fn png_bytes(page: &Page) -> Result<Vec<u8>> {
    valid(page);
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, page.width, page.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Best);
        encoder.write_header()?.write_image_data(&page.pixels)?;
    }
    Ok(bytes)
}
pub fn read_png(path: &Path) -> Result<Page> {
    let mut reader = png::Decoder::new(fs::File::open(path)?).read_info()?;
    let mut pixels = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut pixels)?;
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return Err("baseline must be RGBA8".into());
    }
    pixels.truncate(info.buffer_size());
    Ok(Page {
        width: info.width,
        height: info.height,
        pixels,
    })
}
pub fn render(bytes: &[u8]) -> Result<Vec<Page>> {
    let doc = PdfDocument::open_with_password(bytes.to_vec(), "")?;
    if doc.page_count() == 0 {
        return Err("fixture has no pages".into());
    }
    (0..doc.page_count())
        .map(|i| {
            let p = doc.render_page(
                i,
                &RenderOptions {
                    dpi: 96.0,
                    ..Default::default()
                },
            )?;
            Ok(Page {
                width: p.width,
                height: p.height,
                pixels: p.pixels,
            })
        })
        .collect()
}
pub fn write_failure(dir: &Path, expected: &Page, actual: &Page, t: Tolerance) -> Result<()> {
    fs::create_dir_all(dir)?;
    fs::write(dir.join("expected.png"), png_bytes(expected)?)?;
    fs::write(dir.join("actual.png"), png_bytes(actual)?)?;
    let width = expected.width.max(actual.width);
    let height = expected.height.max(actual.height);
    let mut diff = Page {
        width,
        height,
        pixels: Vec::new(),
    };
    for y in 0..height {
        for x in 0..width {
            match (pixel(expected, x, y), pixel(actual, x, y)) {
                (Some(a), Some(b)) if !changed(a, b, t.channel) => {
                    diff.pixels.extend([a[0] / 3, a[1] / 3, a[2] / 3, 255])
                }
                _ => diff.pixels.extend([255, 0, 0, 255]),
            }
        }
    }
    fs::write(dir.join("diff.png"), png_bytes(&diff)?)?;
    Ok(())
}

/// A sorted inventory also catches stale baselines and removed/added pages.
pub fn inventory(dir: &Path, extension: &str) -> Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().and_then(|v| v.to_str()) == Some(extension) {
            names.push(
                path.file_name()
                    .unwrap()
                    .to_str()
                    .ok_or("non UTF-8 filename")?
                    .to_owned(),
            );
        }
    }
    names.sort();
    Ok(names)
}
