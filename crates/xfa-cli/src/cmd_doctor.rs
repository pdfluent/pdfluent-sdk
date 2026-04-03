//! Installation check command.

use crate::error::CliError;
use anyhow::{bail, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn run() -> Result<()> {
    println!(
        "PDFluent v{} — installation check",
        env!("CARGO_PKG_VERSION")
    );
    println!();

    let mut issues = 0;

    // 1. Binary
    let binary_path = std::env::current_exe().ok();
    match binary_path {
        Some(path) => println!(
            "✓ Binary     v{} at {}",
            env!("CARGO_PKG_VERSION"),
            path.display()
        ),
        None => {
            println!("✗ Binary     Could not determine executable path");
            issues += 1;
        }
    }

    // 2. Fonts
    let font_dirs = if cfg!(target_os = "macos") {
        vec!["/Library/Fonts", "/System/Library/Fonts", "~/Library/Fonts"]
    } else {
        vec!["/usr/share/fonts", "/usr/local/share/fonts", "~/.fonts"]
    };

    let mut total_fonts = 0;
    let mut primary_font_dir = None;

    for dir in &font_dirs {
        let path_str = if dir.starts_with("~/") {
            if let Some(home) = std::env::var_os("HOME") {
                let mut p = PathBuf::from(home);
                p.push(&dir[2..]);
                p
            } else {
                continue;
            }
        } else {
            PathBuf::from(dir)
        };

        if path_str.exists() {
            let count = count_files_recursive(&path_str);
            total_fonts += count;
            if primary_font_dir.is_none() {
                primary_font_dir = Some(path_str);
            }
        }
    }

    if let Some(path) = primary_font_dir {
        println!("✓ Fonts      {} fonts in {}", total_fonts, path.display());
    } else {
        println!("✗ Fonts      No font directories found");
        issues += 1;
    }

    // 3. Temp dir
    let temp_dir = Path::new("/tmp");
    if temp_dir.exists() && is_writable(temp_dir) {
        let free_space = get_free_space(temp_dir);
        println!("✓ Temp       /tmp available ({})", free_space);
    } else {
        println!("✗ Temp       /tmp not available or not writable");
        issues += 1;
    }

    // 4. ICC profiles
    let icc_dirs = if cfg!(target_os = "macos") {
        vec![
            "/Library/ColorSync/Profiles",
            "/System/Library/ColorSync/Profiles",
        ]
    } else {
        vec!["/usr/share/color/icc", "/usr/local/share/color/icc"]
    };

    let mut found_icc = false;
    for dir in &icc_dirs {
        let path = Path::new(dir);
        if path.exists() && count_files_recursive(path) > 0 {
            println!("✓ ICC        ICC profiles found in {}", path.display());
            found_icc = true;
            break;
        }
    }
    if !found_icc {
        println!("✗ ICC        No ICC profiles found — PDF/A color conversion may fail");
        println!("             Install: sudo apt-get install icc-profiles");
        issues += 1;
    }

    // 5. Memory
    match get_memory() {
        Ok(mem) => println!("✓ Memory     {} available", mem),
        Err(_) => {
            println!("✗ Memory     Could not determine available memory");
            issues += 1;
        }
    }

    // 6. veraPDF (optional)
    match Command::new("verapdf").arg("--version").output() {
        Ok(_) => println!("✓ veraPDF    Found"),
        Err(_) => {
            println!("– veraPDF    Not found (optional — only needed for validation testing)")
        }
    }

    println!();
    if issues > 0 {
        println!(
            "{} issue{} found. Run 'pdfluent doctor --fix' for auto-fix suggestions.",
            issues,
            if issues > 1 { "s" } else { "" }
        );
    } else {
        println!("No issues found.");
    }

    Ok(())
}

fn count_files_recursive(path: &Path) -> usize {
    if !path.is_dir() {
        return 0;
    }
    let mut count = 0;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Ok(file_type) = entry.file_type() {
                if file_type.is_dir() {
                    count += count_files_recursive(&entry.path());
                } else {
                    count += 1;
                }
            }
        }
    }
    count
}

fn is_writable(path: &Path) -> bool {
    let test_file = path.join(".pdfluent_doctor_test");
    if fs::write(&test_file, b"test").is_ok() {
        let _ = fs::remove_file(test_file);
        true
    } else {
        false
    }
}

fn get_free_space(path: &Path) -> String {
    let output = Command::new("df").arg("-h").arg(path).output();
    if let Ok(out) = output {
        let s = String::from_utf8_lossy(&out.stdout);
        for line in s.lines().skip(1) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            // df -h output varies by platform, but usually 4th column is free/avail
            if parts.len() >= 4 {
                // On macOS it's column 4 (1-indexed: Filesystem, Size, Used, Avail, Capacity, iused, ifree, %iused, Mounted on)
                // On Linux it's often column 4 too.
                return format!("{} free", parts[3]);
            }
        }
    }
    "Unknown".to_string()
}

fn get_memory() -> Result<String> {
    if cfg!(target_os = "macos") {
        let output = Command::new("sysctl")
            .arg("-n")
            .arg("hw.memsize")
            .output()?;
        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let bytes: u64 = s.parse()?;
        Ok(format!(
            "{:.1} GB",
            bytes as f64 / (1024.0 * 1024.0 * 1024.0)
        ))
    } else {
        let meminfo = fs::read_to_string("/proc/meminfo")?;
        for line in meminfo.lines() {
            if line.starts_with("MemTotal:") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    let kb: u64 = parts[1].parse()?;
                    return Ok(format!("{:.1} GB", kb as f64 / (1024.0 * 1024.0)));
                }
            }
        }
        bail!(CliError {
            message: "Could not parse /proc/meminfo".to_string(),
            why: Some("The /proc/meminfo file format is unexpected.".to_string()),
            fix: Some(
                "This is likely a system configuration issue or an unsupported OS version."
                    .to_string()
            ),
            docs: Some("https://docs.pdfluent.com/errors/E004".to_string()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_writable() {
        assert!(is_writable(Path::new("/tmp")));
    }

    #[test]
    fn test_get_memory() {
        let mem = get_memory();
        assert!(mem.is_ok());
    }

    #[test]
    fn test_count_files() {
        // Just check it doesn't panic
        count_files_recursive(Path::new("."));
    }
}
