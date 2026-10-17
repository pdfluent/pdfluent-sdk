//! Behaviour tests for `pdfluent models`.
//!
//! `status` reports what the per-user model dir holds, `install` is the one
//! command that ever downloads weights -- and refuses while a needed pin is
//! a TODO. The install matrix runs against `file://` sources, so no test
//! here touches the network; the end-to-end proves an install out of local
//! copies runs `balanced` (and TATR) with no flags, and skips loudly
//! without the local files. On the default build both name the `ort-layout`
//! cargo feature.
//!
//! No `// CLI covers:` marker: the tier is not an advertised capability until
//! its release ships, so the register names no `models` command to cover.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

mod common;

#[cfg(not(feature = "ort-layout"))]
use common::run;
#[cfg(all(feature = "ort-layout", feature = "ort-tables"))]
use common::run_with_env_timeout;
#[cfg(feature = "ort-layout")]
use common::{run_with_env, stdout_json, stdout_text, tempdir};
#[cfg(all(feature = "ort-layout", feature = "ort-tables"))]
use sha2::Digest as _;

/// `PDFLUENT_MODEL_DIR` pointed at `dir`, so the command reads a scratch dir
/// rather than the developer's own models.
#[cfg(feature = "ort-layout")]
fn model_dir_env(dir: &std::path::Path) -> (String, String) {
    (
        "PDFLUENT_MODEL_DIR".to_string(),
        dir.to_string_lossy().to_string(),
    )
}

/// This platform's ONNX Runtime release asset, as the installer maps it.
#[cfg(feature = "ort-layout")]
fn ort_remote_for_this_platform() -> Option<&'static str> {
    Some(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "onnxruntime-macos-arm64-1.23.2.dylib",
        ("macos", "x86_64") => "onnxruntime-macos-x86_64-1.23.2.dylib",
        ("linux", "x86_64") => "onnxruntime-linux-x86_64-1.23.2.so",
        ("linux", "aarch64") => "onnxruntime-linux-arm64-1.23.2.so",
        ("windows", "x86_64") => "onnxruntime-windows-x86_64-1.23.2.dll",
        _ => return None,
    })
}

/// A 64-hex digest that pins nothing: well-formed, so it vets clean as a
/// test override without matching any release file.
#[cfg(feature = "ort-layout")]
const UNRELATED_DIGEST: &str = "9cccf3aa4cc7df7ed0333594fbd6bf2df121ccbbbfeb6f53571cd1fa3e275917";

/// True when `dir` holds no files at all: a refused install leaves nothing
/// behind (empty parent dirs are not files).
#[cfg(feature = "ort-layout")]
fn no_files_under(dir: &std::path::Path) -> bool {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                return false;
            }
        }
    }
    true
}

#[cfg(feature = "ort-layout")]
#[test]
fn models_status_on_an_empty_dir_reports_all_missing() {
    let dir = tempdir("models-status-empty");
    let (key, value) = model_dir_env(&dir);
    let out = run_with_env(&["models", "status"], &[(key.as_str(), value.as_str())]);
    assert!(
        out.status.success(),
        "status is a report, not a verdict: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = stdout_text(&out);
    assert!(text.contains("heron_ep03.onnx"), "{text}");
    assert!(text.contains("tatr-v1.1-all.onnx"), "{text}");
    assert!(text.contains("onnxruntime"), "{text}");
    assert!(text.contains("missing"), "{text}");
    assert!(
        text.contains("todo pins: none"),
        "the release table is fully pinned: {text}"
    );
    assert!(
        text.contains(&dir.to_string_lossy().to_string()),
        "the paths are under the model dir: {text}"
    );
}

#[cfg(feature = "ort-layout")]
#[test]
fn models_status_json_carries_all_states() {
    let dir = tempdir("models-status-json");
    let (key, value) = model_dir_env(&dir);
    let out = run_with_env(
        &["--json", "models", "status"],
        &[(key.as_str(), value.as_str())],
    );
    assert!(out.status.success());
    let json = stdout_json(&out);
    assert_eq!(json["ok"], true);
    assert_eq!(json["data"]["models"]["layout"]["state"], "missing");
    assert_eq!(json["data"]["models"]["tables"]["state"], "missing");
    assert_eq!(json["data"]["runtime"]["state"], "missing");
    assert_eq!(
        json["data"]["models"]["layout"]["size"],
        serde_json::Value::Null
    );
    assert_eq!(
        json["data"]["todo_pins"],
        serde_json::json!([]),
        "no TODO pins ride along: {json}"
    );
}

#[cfg(feature = "ort-layout")]
#[test]
fn models_status_reports_a_wrong_sha_as_a_mismatch() {
    // Wrong bytes under a pinned row are a mismatch, in all three rows: the
    // models and now the runtime library too, whose pin is recorded. Nothing
    // is unpinned anymore -- every row `status` reports has a pin.
    let dir = tempdir("models-status-states");
    let (key, value) = model_dir_env(&dir);
    let heron = dir.join("layout-heron-ep03").join("heron_ep03.onnx");
    std::fs::create_dir_all(heron.parent().expect("a parent")).expect("the package dir");
    std::fs::write(&heron, b"these are not the weights").expect("the wrong bytes");
    let tatr = dir.join("tatr-v1.1-all").join("tatr-v1.1-all.onnx");
    std::fs::create_dir_all(tatr.parent().expect("a parent")).expect("the package dir");
    std::fs::write(&tatr, b"these are not the weights either").expect("the wrong bytes");
    let lib = dir.join("onnxruntime-1.23.2").join(format!(
        "libonnxruntime.{}",
        if std::env::consts::OS == "windows" {
            "dll"
        } else if std::env::consts::OS == "macos" {
            "dylib"
        } else {
            "so"
        }
    ));
    std::fs::create_dir_all(lib.parent().expect("a parent")).expect("the runtime dir");
    std::fs::write(&lib, b"this is not a runtime").expect("the wrong bytes");
    let out = run_with_env(&["models", "status"], &[(key.as_str(), value.as_str())]);
    assert!(out.status.success());
    let text = stdout_text(&out);
    assert!(text.contains("CHECKSUM MISMATCH"), "{text}");
    assert!(text.contains("tatr-v1.1-all.onnx"), "{text}");
    let lib_name = lib
        .file_name()
        .expect("a name")
        .to_string_lossy()
        .to_string();
    for (label, file) in [
        ("layout", "heron_ep03.onnx"),
        ("tables", "tatr-v1.1-all.onnx"),
        ("onnxruntime", lib_name.as_str()),
    ] {
        assert!(
            text.lines().any(|line| line.starts_with(label)
                && line.contains(file)
                && line.contains("CHECKSUM MISMATCH")),
            "the {label} row mismatches: {text}"
        );
    }
    assert!(
        !text.contains("present, no sha256 pinned"),
        "every reported row is pinned: {text}"
    );
}

#[cfg(feature = "ort-layout")]
#[test]
fn models_install_refuses_a_wrong_sha_for_this_platforms_pin() {
    // This platform's library pin is recorded, so a test override carrying
    // any other digest refuses as a weakening -- naming the asset, before
    // touching the network, and writing nothing. While the pin was a TODO
    // the same override filled it; now the compiled pin is the authority.
    let remote = ort_remote_for_this_platform().expect("this platform has a library");
    let dir = tempdir("models-install-wrong-sha");
    let root = dir.join("root");
    let (model_key, model_value) = model_dir_env(&root);
    let pins = dir.join("pins.txt");
    std::fs::write(&pins, format!("{remote}={UNRELATED_DIGEST}\n")).expect("the pins file");
    let out = run_with_env(
        &["models", "install", "layout"],
        &[
            (model_key.as_str(), model_value.as_str()),
            ("PDFLUENT_MODELS_TEST_PINS", pins.to_string_lossy().as_ref()),
        ],
    );
    assert_eq!(out.status.code(), Some(3));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("MODEL_NOT_AVAILABLE"), "{err}");
    assert!(err.contains("must not weaken"), "{err}");
    assert!(err.contains(remote), "the refusal names the asset: {err}");
    assert!(no_files_under(&root), "a refused install writes nothing");
}

#[cfg(feature = "ort-layout")]
#[test]
fn models_install_accepts_both_package_names() {
    // Both names parse -- `layout tables` in one call and `tables` alone:
    // against an empty `file://` source both fail at the first download,
    // not at the argument parser, and with no usage error. (The fetch
    // failure is what proves the parse; the success path needs the release
    // bytes, which the end-to-end below covers.)
    let dir = tempdir("models-install-names");
    let (key, value) = model_dir_env(&dir);
    let source = dir.join("empty-source");
    std::fs::create_dir(&source).expect("the empty source dir");
    let base = format!("file://{}/", source.display());
    for packages in [&["layout", "tables"][..], &["tables"][..]] {
        let mut argv = vec!["models", "install"];
        argv.extend_from_slice(packages);
        let out = run_with_env(
            &argv,
            &[
                (key.as_str(), value.as_str()),
                ("PDFLUENT_MODELS_BASE", base.as_str()),
            ],
        );
        assert_eq!(out.status.code(), Some(3), "{packages:?} parses");
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(err.contains("MODEL_NOT_AVAILABLE"), "{err}");
        assert!(err.contains("could not download"), "{err}");
        assert!(!err.contains("possible values"), "no usage error: {err}");
    }
}

#[cfg(feature = "ort-layout")]
#[test]
fn models_install_rejects_an_unknown_package() {
    let dir = tempdir("models-install-unknown");
    let (key, value) = model_dir_env(&dir);
    let out = run_with_env(
        &["models", "install", "bogus"],
        &[(key.as_str(), value.as_str())],
    );
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("possible values"), "{err}");
    assert!(err.contains("layout"), "{err}");
    assert!(err.contains("tables"), "{err}");
}

#[cfg(feature = "ort-layout")]
#[test]
fn models_install_from_a_file_url_with_wrong_bytes_refuses_and_leaves_nothing() {
    // The table fully pinned, the model bytes wrong: the size gate fires
    // first (25 bytes where the pin says 306,654,422) and nothing is left
    // behind. No network: a `file://` source, and the pins enforced
    // identically. (The digest refusal itself is pinned by the unit tests,
    // which feed bytes without a socket.)
    if ort_remote_for_this_platform().is_none() {
        eprintln!("skipped: no ONNX Runtime library is packaged for this platform");
        return;
    }
    let dir = tempdir("models-install-bad-sha");
    let root = dir.join("root");
    let (model_key, model_value) = model_dir_env(&root);
    let source = dir.join("source");
    std::fs::create_dir(&source).expect("the source dir");
    std::fs::write(source.join("heron_ep03.onnx"), b"these are not the weights")
        .expect("the corrupt weights");
    let base = format!("file://{}/", source.display());
    let out = run_with_env(
        &["models", "install", "layout"],
        &[
            (model_key.as_str(), model_value.as_str()),
            ("PDFLUENT_MODELS_BASE", base.as_str()),
        ],
    );
    assert_eq!(out.status.code(), Some(3));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("MODEL_NOT_AVAILABLE"), "{err}");
    assert!(err.contains("expected 306654422 bytes, got 25"), "{err}");
    assert!(err.contains("nothing was installed"), "{err}");
    assert!(
        !root
            .join("layout-heron-ep03")
            .join("heron_ep03.onnx")
            .exists(),
        "the target is not written"
    );
    assert!(
        no_files_under(&root),
        "a mismatch leaves nothing behind, not even a .part file"
    );
}

#[cfg(feature = "ort-layout")]
#[test]
fn models_install_refuses_an_override_that_weakens_a_text_pin() {
    // The notice is pinned now, so an override carrying any other digest
    // refuses as a weakening -- naming the asset, before fetching
    // anything, and writing nothing. The compiled pins are the authority
    // for the texts as for the weights.
    let dir = tempdir("models-install-text-weaken");
    let root = dir.join("root");
    let (model_key, model_value) = model_dir_env(&root);
    let pins = dir.join("pins.txt");
    std::fs::write(&pins, format!("MODELS-NOTICE.txt={UNRELATED_DIGEST}\n"))
        .expect("the pins file");
    let out = run_with_env(
        &["models", "install", "layout"],
        &[
            (model_key.as_str(), model_value.as_str()),
            ("PDFLUENT_MODELS_TEST_PINS", pins.to_string_lossy().as_ref()),
        ],
    );
    assert_eq!(out.status.code(), Some(3));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("MODEL_NOT_AVAILABLE"), "{err}");
    assert!(err.contains("must not weaken"), "{err}");
    assert!(
        err.contains("MODELS-NOTICE.txt"),
        "the refusal names the asset: {err}"
    );
    assert!(no_files_under(&root), "a refused install writes nothing");
}

#[cfg(feature = "ort-layout")]
#[test]
fn models_install_refuses_an_override_that_weakens_a_release_pin() {
    // The test override can fill TODOs but never move a release pin: a
    // differing digest for a filled asset refuses.
    let dir = tempdir("models-install-weaken");
    let root = dir.join("root");
    let (model_key, model_value) = model_dir_env(&root);
    let pins = dir.join("pins.txt");
    std::fs::write(&pins, format!("heron_ep03.onnx={UNRELATED_DIGEST}\n")).expect("the pins file");
    let out = run_with_env(
        &["models", "install", "layout"],
        &[
            (model_key.as_str(), model_value.as_str()),
            ("PDFLUENT_MODELS_TEST_PINS", pins.to_string_lossy().as_ref()),
        ],
    );
    assert_eq!(out.status.code(), Some(3));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("MODEL_NOT_AVAILABLE"), "{err}");
    assert!(err.contains("must not weaken"), "{err}");
    assert!(err.contains("heron_ep03.onnx"), "{err}");
    assert!(no_files_under(&root), "a refused install writes nothing");
}

/// One end-to-end input: the override env var, or the documented default.
/// `None` skips the test aloud, naming what is missing.
#[cfg(all(feature = "ort-layout", feature = "ort-tables"))]
fn local_file(env_key: &str, default: Option<std::path::PathBuf>) -> Option<std::path::PathBuf> {
    if let Ok(path) = std::env::var(env_key) {
        if path.is_empty() {
            eprintln!("skipped: {env_key} is set but empty");
            return None;
        }
        let path = std::path::PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
        eprintln!("skipped: {env_key}={} is not a file", path.display());
        return None;
    }
    match default {
        Some(path) if path.is_file() => Some(path),
        Some(path) => {
            eprintln!(
                "skipped: {env_key} is not set and {} is not there",
                path.display()
            );
            None
        }
        None => {
            eprintln!("skipped: {env_key} is not set and there is no default");
            None
        }
    }
}

/// One end-to-end input dir: the override env var, or the documented
/// default. `None` skips the test aloud, naming what is missing.
#[cfg(all(feature = "ort-layout", feature = "ort-tables"))]
fn local_dir(env_key: &str, default: Option<std::path::PathBuf>) -> Option<std::path::PathBuf> {
    if let Ok(path) = std::env::var(env_key) {
        if path.is_empty() {
            eprintln!("skipped: {env_key} is set but empty");
            return None;
        }
        let path = std::path::PathBuf::from(path);
        if path.is_dir() {
            return Some(path);
        }
        eprintln!("skipped: {env_key}={} is not a dir", path.display());
        return None;
    }
    match default {
        Some(path) if path.is_dir() => Some(path),
        Some(path) => {
            eprintln!(
                "skipped: {env_key} is not set and {} is not there",
                path.display()
            );
            None
        }
        None => {
            eprintln!("skipped: {env_key} is not set and there is no default");
            None
        }
    }
}

#[cfg(all(feature = "ort-layout", feature = "ort-tables"))]
fn pdfluent_local(rel: &str) -> Option<std::path::PathBuf> {
    std::env::var("HOME")
        .ok()
        .filter(|home| !home.is_empty())
        .map(|home| {
            std::path::PathBuf::from(home)
                .join(".local")
                .join("share")
                .join("pdfluent")
                .join(rel)
        })
}

/// This platform's `os-arch` label, as the install line prints it.
#[cfg(all(feature = "ort-layout", feature = "ort-tables"))]
fn ort_platform_label() -> Option<&'static str> {
    Some(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "macos-arm64",
        ("macos", "x86_64") => "macos-x86_64",
        ("linux", "x86_64") => "linux-x86_64",
        ("linux", "aarch64") => "linux-aarch64",
        ("windows", "x86_64") => "windows-x86_64",
        _ => return None,
    })
}

/// Link the local copies into the `file://` source without copying
/// hundreds of megabytes; copy where links do not exist.
#[cfg(all(feature = "ort-layout", feature = "ort-tables"))]
fn link_or_copy(src: &std::path::Path, dst: &std::path::Path) {
    #[cfg(unix)]
    if std::os::unix::fs::symlink(src, dst).is_ok() {
        return;
    }
    std::fs::copy(src, dst).expect("the test file copies");
}

#[cfg(all(feature = "ort-layout", feature = "ort-tables"))]
fn sha256_file(path: &std::path::Path) -> String {
    let mut file = std::fs::File::open(path).expect("the local file reads");
    let mut hasher = sha2::Sha256::new();
    std::io::copy(&mut file, &mut hasher).expect("the local file hashes");
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>()
}

/// The release pins for the ONNX Runtime files, as recorded in the
/// installer's pin table: the sha256 of the EXTRACTED files from the
/// official microsoft/onnxruntime v1.23.2 archives. The end-to-end runs only
/// on the release bytes, so these decide its skip.
#[cfg(all(feature = "ort-layout", feature = "ort-tables"))]
fn release_pin_for(remote: &str) -> Option<&'static str> {
    Some(match remote {
        "onnxruntime-macos-arm64-1.23.2.dylib" => {
            "d306d2bc768540766c7ed8a1e0ff05d2870c77a934ebeee4a7bafa1b732ef299"
        }
        "onnxruntime-macos-x86_64-1.23.2.dylib" => {
            "8c9c78de65ea3786f987c0d980e9c1b13a3a5fbc6b3e2965ba05b450e6e4c054"
        }
        "onnxruntime-linux-x86_64-1.23.2.so" => {
            "13ab8084954fa4a47c777880180b90810d6020f021441395712b48a75b74c68b"
        }
        "onnxruntime-linux-arm64-1.23.2.so" => {
            "648ffa64fbe027ae27139109410900cf776a030dec2dbbac51053318cc44c286"
        }
        "onnxruntime-windows-x86_64-1.23.2.dll" => {
            "dec964ab1ee36cc9b0ae247d13b376627992fc57dec0454354017ab8fd84f1ea"
        }
        "onnxruntime-LICENSE.txt" => {
            "2f07c72751aed99790b8a4869cf2311df85a860b22ded05fa22803587a48922c"
        }
        "onnxruntime-ThirdPartyNotices.txt" => {
            "e9e90971a8e75a9a8ac0c6412e29c1202d079998389915aa485f46c816c3b4cc"
        }
        _ => return None,
    })
}

/// Skip the end-to-end aloud unless `path` is the release bytes for
/// `remote`: installing anything else would fail its checksum, and that
/// failure would prove the local files stale, not the installer broken.
#[cfg(all(feature = "ort-layout", feature = "ort-tables"))]
fn require_release_bytes(path: &std::path::Path, remote: &str) -> Option<()> {
    let sha = sha256_file(path);
    let expected = release_pin_for(remote).expect("a release pin for every runtime file");
    if sha != expected {
        eprintln!(
            "skipped: {} is not the release bytes for {remote} \
             (sha256 {sha} != {expected}); the install end-to-end needs the models-v1 release files",
            path.display()
        );
        return None;
    }
    Some(())
}

/// Remove the scratch dirs on the way out, however the test ends: an
/// install holds some 450 MB, and the disk is tight.
#[cfg(all(feature = "ort-layout", feature = "ort-tables"))]
struct Cleanup(std::path::PathBuf);

#[cfg(all(feature = "ort-layout", feature = "ort-tables"))]
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// An install out of local copies, then `balanced` (and TATR) with no
/// flags: `models install layout tables` from a `file://` source built from
/// the local weights and runtime files, and after it `extract --quality
/// balanced` works with no flags or env beyond the redirected model root --
/// and with TATR where the flag points at the same root. Skipped loudly
/// without the release files (the runtime files must match the release pins;
/// the local test library does not); every ORT load is capped at 120 s, and
/// a hang fails.
#[cfg(all(feature = "ort-layout", feature = "ort-tables"))]
#[test]
fn installed_models_extract_balanced_and_tatr_without_flags() {
    use std::time::Duration;

    let Some(heron) = local_file(
        "PDFLUENT_HERON_TEST_MODEL",
        pdfluent_local("detector-onnx/heron_ep03.onnx"),
    ) else {
        eprintln!(
            "SKIPPED (not a pass): layout model not available locally -- set PDFLUENT_HERON_TEST_MODEL to heron_ep03.onnx (sha256 b7cc5d31…)"
        );
        return;
    };
    // No default dir: the env var is the documented route, and a
    // /private/tmp fallback would ship in the published tree, where the
    // snapshot path audit refuses machine paths.
    let Some(tatr_root) = local_dir("PDFLUENT_TATR_TEST_MODEL_DIR", None) else {
        eprintln!(
            "SKIPPED (not a pass): table model not available locally -- set PDFLUENT_TATR_TEST_MODEL_DIR to a dir with tatr-v1.1-all/tatr-v1.1-all.onnx"
        );
        return;
    };
    let tatr = tatr_root.join("tatr-v1.1-all").join("tatr-v1.1-all.onnx");
    if !tatr.is_file() {
        eprintln!(
            "skipped: {} holds no tatr-v1.1-all/tatr-v1.1-all.onnx",
            tatr_root.display()
        );
        return;
    }
    let Some(dylib) = local_file(
        "PDFLUENT_ORT_TEST_DYLIB",
        pdfluent_local("ort-1.23.2-macos-arm64/libonnxruntime.1.23.2.dylib"),
    ) else {
        eprintln!(
            "SKIPPED (not a pass): ONNX Runtime library matching the release pin not available locally -- the PyPI test library does not match the models-v1 pin"
        );
        return;
    };
    // The release runtime texts have no default: they must be the models-v1
    // bytes, and nothing local carries them yet.
    let ort_text = |key: &str| -> Option<std::path::PathBuf> {
        match std::env::var(key) {
            Ok(path) if !path.is_empty() => {
                let path = std::path::PathBuf::from(path);
                if path.is_file() {
                    return Some(path);
                }
                eprintln!("skipped: {key}={} is not a file", path.display());
                None
            }
            _ => {
                eprintln!(
                    "skipped: {key} is not set; the install end-to-end needs the release ONNX Runtime texts"
                );
                None
            }
        }
    };
    let Some(ort_license) = ort_text("PDFLUENT_ORT_LICENSE_TEST_FILE") else {
        eprintln!(
            "SKIPPED (not a pass): release ONNX Runtime LICENSE text not available locally -- set PDFLUENT_ORT_LICENSE_TEST_FILE to the models-v1 onnxruntime-LICENSE.txt"
        );
        return;
    };
    let Some(ort_notices) = ort_text("PDFLUENT_ORT_NOTICES_TEST_FILE") else {
        eprintln!(
            "SKIPPED (not a pass): release ONNX Runtime notices text not available locally -- set PDFLUENT_ORT_NOTICES_TEST_FILE to the models-v1 onnxruntime-ThirdPartyNotices.txt"
        );
        return;
    };
    let apache = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../LICENSE-APACHE");
    assert!(apache.is_file(), "fixture missing: {}", apache.display());
    let Some(pages) = local_dir(
        "PDFLUENT_TATR_TEST_PAGES_DIR",
        pdfluent_local("fintabnet-train-sample/pages"),
    ) else {
        eprintln!(
            "SKIPPED (not a pass): table test pages not available locally -- set PDFLUENT_TATR_TEST_PAGES_DIR to a dir holding AAL_2015_page_207.pdf"
        );
        return;
    };
    let page = pages.join("AAL_2015_page_207.pdf");
    if !page.is_file() {
        eprintln!("skipped: {} is not in {}", page.display(), pages.display());
        return;
    }
    let (Some(remote), Some(platform)) = (ort_remote_for_this_platform(), ort_platform_label())
    else {
        eprintln!("skipped: no ONNX Runtime library is packaged for this platform");
        return;
    };
    // The install verifies the runtime files against the release pins, so
    // the end-to-end runs only on the release bytes.
    if require_release_bytes(&dylib, remote).is_none()
        || require_release_bytes(&ort_license, "onnxruntime-LICENSE.txt").is_none()
        || require_release_bytes(&ort_notices, "onnxruntime-ThirdPartyNotices.txt").is_none()
    {
        return;
    }

    let work = tempdir("models-e2e");
    let _cleanup = Cleanup(work.clone());
    let source = work.join("source");
    let root = work.join("root");
    std::fs::create_dir(&source).expect("the source dir");
    link_or_copy(&heron, &source.join("heron_ep03.onnx"));
    link_or_copy(&tatr, &source.join("tatr-v1.1-all.onnx"));
    link_or_copy(&dylib, &source.join(remote));
    link_or_copy(&ort_license, &source.join("onnxruntime-LICENSE.txt"));
    link_or_copy(
        &ort_notices,
        &source.join("onnxruntime-ThirdPartyNotices.txt"),
    );
    // The notice and the SDK licence texts come from the tree: they are
    // the release bytes, so the test pins below equal the release pins
    // and the installer verifies them as it would the models-v1 package.
    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let notice = manifest.join("../../scripts/release/models-v1/MODELS-NOTICE.txt");
    let sdk_license = manifest.join("../../LICENSE");
    let commercial = manifest.join("../../LICENSE-COMMERCIAL");
    for path in [&notice, &sdk_license, &commercial] {
        assert!(path.is_file(), "fixture missing: {}", path.display());
    }
    link_or_copy(&notice, &source.join("MODELS-NOTICE.txt"));
    link_or_copy(&apache, &source.join("LICENSE-Apache-2.0.txt"));
    link_or_copy(&sdk_license, &source.join("LICENSE"));
    link_or_copy(&commercial, &source.join("LICENSE-COMMERCIAL"));
    let pins = work.join("pins.txt");
    std::fs::write(
        &pins,
        format!(
            "MODELS-NOTICE.txt={}\n\
             LICENSE={}\n\
             LICENSE-COMMERCIAL={}\n",
            sha256_file(&source.join("MODELS-NOTICE.txt")),
            sha256_file(&source.join("LICENSE")),
            sha256_file(&source.join("LICENSE-COMMERCIAL")),
        ),
    )
    .expect("the pins file writes");

    let root_str = root.to_string_lossy().to_string();
    let base = format!("file://{}/", source.display());
    let pins_str = pins.to_string_lossy().to_string();
    let install_env = [
        ("PDFLUENT_MODEL_DIR", root_str.as_str()),
        ("PDFLUENT_MODELS_BASE", base.as_str()),
        ("PDFLUENT_MODELS_TEST_PINS", pins_str.as_str()),
    ];
    let out = run_with_env(&["models", "install", "layout", "tables"], &install_env);
    assert!(
        out.status.success(),
        "install succeeds: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        stdout_text(&out),
        format!(
            "installed heron_ep03.onnx (layout) — PDFluent SDK licence \
             (AGPL-3.0 or commercial) — see MODELS-NOTICE.txt\n\
             installed tatr-v1.1-all.onnx (tables) — MIT — see MODELS-NOTICE.txt\n\
             installed onnxruntime 1.23.2 ({platform}) — MIT — see onnxruntime-LICENSE.txt\n"
        ),
        "exactly one line per model plus one for the runtime"
    );
    assert_eq!(String::from_utf8_lossy(&out.stderr), "", "no warning text");

    let status_env = [
        ("PDFLUENT_MODEL_DIR", root_str.as_str()),
        ("PDFLUENT_MODELS_TEST_PINS", pins_str.as_str()),
    ];
    let out = run_with_env(&["models", "status"], &status_env);
    assert!(out.status.success());
    let text = stdout_text(&out);
    for label in ["layout", "tables", "onnxruntime"] {
        assert!(
            text.lines()
                .any(|line| line.starts_with(label) && line.contains("sha256 ok")),
            "{label} verifies: {text}"
        );
    }
    assert!(text.contains("306654422"), "{text}");
    assert!(text.contains("118206817"), "{text}");
    assert!(text.contains("todo pins:"), "{text}");

    // After the install, `balanced` works with no flags or env beyond the
    // redirected model root -- on a real machine, with neither at all.
    let page_str = page.to_string_lossy().to_string();
    let balanced_md = work.join("balanced.md");
    let balanced_str = balanced_md.to_string_lossy().to_string();
    let model_env = [("PDFLUENT_MODEL_DIR", root_str.as_str())];
    let out = run_with_env_timeout(
        &[
            "extract",
            page_str.as_str(),
            "--quality",
            "balanced",
            "--out",
            balanced_str.as_str(),
        ],
        &model_env,
        Duration::from_secs(120),
    )
    .expect("balanced finishes within 120 s");
    assert!(
        out.status.success(),
        "balanced extracts: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        balanced_md.is_file() && std::fs::metadata(&balanced_md).expect("a stat").len() > 0,
        "balanced writes markdown"
    );

    // And with TATR where the flag points at the installed root: the trace
    // proves the stage engaged on the installed weights.
    let tatr_md = work.join("tatr.md");
    let tatr_str = tatr_md.to_string_lossy().to_string();
    let tatr_env = [
        ("PDFLUENT_MODEL_DIR", root_str.as_str()),
        ("PDFLUENT_TATR_TRACE", "1"),
    ];
    let out = run_with_env_timeout(
        &[
            "extract",
            page_str.as_str(),
            "--quality",
            "balanced",
            "--table-model-dir",
            root_str.as_str(),
            "--out",
            tatr_str.as_str(),
        ],
        &tatr_env,
        Duration::from_secs(120),
    )
    .expect("balanced with TATR finishes within 120 s");
    assert!(
        out.status.success(),
        "balanced with TATR extracts: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("TATR page="),
        "the TATR stage engaged on the installed weights: {err}"
    );
    assert!(
        tatr_md.is_file() && std::fs::metadata(&tatr_md).expect("a stat").len() > 0,
        "balanced with TATR writes markdown"
    );
}

#[cfg(not(feature = "ort-layout"))]
#[test]
fn models_without_the_feature_names_the_feature() {
    for args in [
        &["models", "install", "layout"][..],
        &["models", "install", "tables"][..],
        &["models", "install", "layout", "tables"][..],
        &["models", "status"][..],
    ] {
        let out = run(args);
        assert_eq!(out.status.code(), Some(3));
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(err.contains("MODEL_NOT_AVAILABLE"), "{err}");
        assert!(err.contains("`ort-layout`"), "{err}");
    }
}
