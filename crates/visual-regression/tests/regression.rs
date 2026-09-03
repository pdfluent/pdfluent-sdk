// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::{fs, path::PathBuf, process::Command};
use visual_regression::{
    compare, fixtures, inventory, png_bytes, read_png, render, root, tolerances, write_failure,
    Page, Result, Tolerance,
};
fn artifacts() -> PathBuf {
    root().join("../../target/visual-regression")
}

#[test]
fn visual_baseline() -> Result<()> {
    let start = std::time::Instant::now();
    let fixtures = fixtures::generate()?;
    assert!((30..=45).contains(&fixtures.len()));
    let names: Vec<_> = fixtures.iter().map(|f| f.name).collect();
    let tolerances = tolerances(&names)?;
    let baseline = root().join("baseline");
    let update = std::env::var("UPDATE_VISUAL_BASELINE").as_deref() == Ok("1");
    let mut expected_names = Vec::new();
    let mut failures = Vec::new();
    let mut page_count = 0;
    for fixture in &fixtures {
        // Updating baselines never silently regenerates source PDFs.
        assert_eq!(
            fs::read(
                root()
                    .join("fixtures")
                    .join(format!("{}.pdf", fixture.name))
            )?,
            fixture.bytes,
            "fixture {} differs; run cargo run -p visual-regression --features generate-fixtures --bin generate",
            fixture.name
        );
        let pages = match render(&fixture.bytes) {
            Ok(p) => p,
            Err(e) => {
                failures.push(format!("{}: render failed: {e}", fixture.name));
                continue;
            }
        };
        if pages.len() != fixture.pages {
            failures.push(format!(
                "{}: expected {} pages, rendered {}",
                fixture.name,
                fixture.pages,
                pages.len()
            ));
        }
        for (i, actual) in pages.iter().enumerate() {
            page_count += 1;
            let name = format!("{}-p{}", fixture.name, i + 1);
            let file = format!("{name}.png");
            expected_names.push(file.clone());
            let path = baseline.join(&file);
            let dir = artifacts().join(&name);
            if dir.exists() {
                fs::remove_dir_all(&dir)?;
            }
            if update {
                fs::write(&path, png_bytes(actual)?)?;
            }
            let expected = match read_png(&path) {
                Ok(p) => p,
                Err(e) => {
                    fs::create_dir_all(&dir)?;
                    fs::write(dir.join("actual.png"), png_bytes(actual)?)?;
                    failures.push(format!("{name}: missing or unreadable baseline: {e}"));
                    continue;
                }
            };
            let tolerance = tolerances[fixture.name];
            let diff = compare(&expected, actual, tolerance);
            if diff.fails(tolerance) {
                write_failure(&dir, &expected, actual, tolerance)?;
                failures.push(format!("{name}: {} / {} pixels ({:.6}%), bbox {:?} (inclusive), dimensions {}x{} -> {}x{}",diff.count,diff.total,diff.fraction()*100.0,diff.bounds,expected.width,expected.height,actual.width,actual.height));
            }
        }
    }
    expected_names.sort();
    if update {
        for stale in inventory(&baseline, "png")? {
            if !expected_names.contains(&stale) {
                fs::remove_file(baseline.join(stale))?;
            }
        }
    }
    let actual_names = inventory(&baseline, "png")?;
    if actual_names != expected_names {
        failures.push(format!(
            "baseline page inventory differs: expected {expected_names:?}; found {actual_names:?}"
        ));
    }
    let mut fixture_names: Vec<_> = names.iter().map(|n| format!("{n}.pdf")).collect();
    fixture_names.sort();
    assert_eq!(
        inventory(&root().join("fixtures"), "pdf")?,
        fixture_names,
        "fixture inventory drift"
    );
    let fixture_size = directory_size(&root().join("fixtures"))?;
    let baseline_size = directory_size(&baseline)?;
    assert!(
        fixture_size + baseline_size < 8 * 1024 * 1024,
        "fixtures and baselines exceed 8 MiB"
    );
    println!("{page_count} pages; fixtures {fixture_size} bytes; baselines {baseline_size} bytes; {:.3}s",start.elapsed().as_secs_f64());
    for failure in &failures {
        eprintln!("FAIL {failure}");
    }
    assert!(
        failures.is_empty(),
        "{} visual regression failure(s)",
        failures.len()
    );
    Ok(())
}
fn directory_size(path: &std::path::Path) -> Result<u64> {
    let mut size = 0;
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        size += if entry.file_type()?.is_dir() {
            directory_size(&entry.path())?
        } else {
            entry.metadata()?.len()
        };
    }
    Ok(size)
}

#[test]
fn comparator_rejects_real_perturbations() -> Result<()> {
    let fixtures = fixtures::generate()?;
    let original = render(
        &fixtures
            .iter()
            .find(|f| f.name == "vector-dashes")
            .unwrap()
            .bytes,
    )?
    .remove(0);
    let t = Tolerance::default();
    assert!(!compare(&original, &original, t).fails(t));
    let mut shifted = original.clone();
    for row in shifted.pixels.chunks_exact_mut(original.width as usize * 4) {
        row.rotate_right(4);
    }
    assert!(
        compare(&original, &shifted, t).fails(t),
        "one-pixel translation must fail"
    );
    let mut edited = original.clone();
    let index = edited.pixels.iter().position(|v| *v <= 252).unwrap();
    edited.pixels[index] += 3;
    let difference = compare(&original, &edited, t);
    assert_eq!(difference.count, 1);
    assert!(difference.fails(t));
    let pixel = index / 4;
    assert_eq!(
        difference.bounds,
        Some([
            (pixel % original.width as usize) as u32,
            (pixel / original.width as usize) as u32,
            (pixel % original.width as usize) as u32,
            (pixel / original.width as usize) as u32
        ])
    );
    edited.pixels[index] -= 1;
    assert!(
        !compare(&original, &edited, t).fails(t),
        "delta two is allowed"
    );
    Ok(())
}

#[test]
fn comparator_dimensions_alpha_fraction_and_artifacts() -> Result<()> {
    let expected = Page {
        width: 2,
        height: 1,
        pixels: vec![40, 80, 120, 255, 10, 20, 30, 255],
    };
    let mut actual = expected.clone();
    actual.pixels[7] = 252;
    let t = Tolerance::default();
    let d = compare(&expected, &actual, t);
    assert_eq!(d.count, 1);
    assert_eq!(d.bounds, Some([1, 0, 1, 0]));
    assert!(d.fails(t));
    assert!(!d.fails(Tolerance { fraction: 0.5, ..t }));
    assert!(d.fails(Tolerance {
        fraction: 0.49,
        ..t
    }));
    let resized = Page {
        width: 1,
        height: 2,
        pixels: expected.pixels.clone(),
    };
    assert!(compare(&expected, &resized, t).fails(Tolerance { fraction: 1.0, ..t }));
    let dir = artifacts().join("comparator-check");
    write_failure(&dir, &expected, &actual, t)?;
    assert_eq!(read_png(&dir.join("expected.png"))?, expected);
    assert_eq!(read_png(&dir.join("actual.png"))?, actual);
    assert_eq!(
        read_png(&dir.join("diff.png"))?.pixels,
        vec![13, 26, 40, 255, 255, 0, 0, 255]
    );
    fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn five_run_determinism() -> Result<()> {
    if let Some(dir) = std::env::var_os("VISUAL_REGRESSION_SNAPSHOT") {
        return snapshot(&PathBuf::from(dir), 1);
    }
    let dir = artifacts().join("determinism");
    if dir.exists() {
        fs::remove_dir_all(&dir)?;
    }
    fs::create_dir_all(&dir)?;
    // Three complete runs here, then two fresh copies of this test executable.
    // Every child renders the entire set; no helper test is skipped or ignored.
    snapshot(&dir.join("same-process"), 3)?;
    for (name, single) in [("single-thread", true), ("default-threads", false)] {
        let mut command = Command::new(std::env::current_exe()?);
        command
            .args(["--exact", "five_run_determinism", "--nocapture"])
            .env("VISUAL_REGRESSION_SNAPSHOT", dir.join(name))
            .env_remove("UPDATE_VISUAL_BASELINE")
            .env_remove("RAYON_NUM_THREADS")
            .env_remove("PDF_RENDER_THREADS");
        if single {
            command
                .env("RAYON_NUM_THREADS", "1")
                .env("PDF_RENDER_THREADS", "1");
        }
        let output = command.output()?;
        assert!(
            output.status.success(),
            "{name}: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        println!("{name}: {}", String::from_utf8_lossy(&output.stdout));
    }
    for extension in ["pdf", "png"] {
        let names = inventory(&dir.join("same-process"), extension)?;
        assert!(!names.is_empty());
        for run in ["single-thread", "default-threads"] {
            assert_eq!(
                inventory(&dir.join(run), extension)?,
                names,
                "{run}: inventory mismatch"
            );
            for name in &names {
                assert_eq!(
                    fs::read(dir.join("same-process").join(name))?,
                    fs::read(dir.join(run).join(name))?,
                    "{run}: {name} differs byte-for-byte"
                );
            }
        }
    }
    fs::remove_dir_all(dir)?;
    Ok(())
}

fn snapshot(dir: &std::path::Path, repeats: usize) -> Result<()> {
    fs::create_dir_all(dir)?;
    for repeat in 0..repeats {
        for fixture in fixtures::generate()? {
            let path = dir.join(format!("{}.pdf", fixture.name));
            if repeat == 0 {
                fs::write(&path, &fixture.bytes)?;
            } else {
                assert!(
                    fs::read(&path)? == fixture.bytes,
                    "{}: generated bytes drifted",
                    fixture.name
                );
            }
            let pages = render(&fixture.bytes)?;
            assert_eq!(
                pages.len(),
                fixture.pages,
                "{}: page count drifted",
                fixture.name
            );
            for (i, page) in pages.iter().enumerate() {
                let path = dir.join(format!("{}-p{}.png", fixture.name, i + 1));
                let bytes = png_bytes(page)?;
                if repeat == 0 {
                    fs::write(&path, bytes)?;
                } else {
                    assert!(
                        fs::read(&path)? == bytes,
                        "{} page {}: run {} drifted",
                        fixture.name,
                        i + 1,
                        repeat + 1
                    );
                }
            }
        }
        println!(
            "snapshot run {}: all fixture and PNG bytes identical",
            repeat + 1
        );
    }
    Ok(())
}
