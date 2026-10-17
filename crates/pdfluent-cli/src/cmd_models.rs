//! `models`: install and inspect the opt-in model tiers' files.
//!
//! Thin over [`pdfluent::model`]: the install commands put files exactly where
//! the shared runtime looks for them, and `status` reports what is there.
//! `install` is the ONLY command that ever downloads a model or a runtime
//! library -- nothing else in this binary touches the network for weights.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

#[cfg(feature = "ort-layout")]
use std::collections::HashMap;
#[cfg(feature = "ort-layout")]
use std::io::{Read, Write};
#[cfg(feature = "ort-layout")]
use std::path::PathBuf;

use clap::{Args, Subcommand};
#[cfg(feature = "ort-layout")]
use serde_json::json;
#[cfg(feature = "ort-layout")]
use sha2::Digest as _;

use crate::output::{self, Ctx};

#[derive(Debug, Args)]
pub struct ModelsArgs {
    #[command(subcommand)]
    pub command: ModelsCommands,
}

#[derive(Debug, Subcommand)]
pub enum ModelsCommands {
    /// Download and verify model packages into the per-user model dir.
    /// The only command that ever downloads a model or runtime library.
    Install(InstallArgs),
    /// Show what is installed in the per-user model dir, and whether the
    /// pins verify.
    Status(StatusArgs),
}

/// Which model packages to install: `layout`, `tables`, or both in one call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum ModelPackage {
    /// The layout model (Heron-ft ep03) plus the platform's ONNX Runtime.
    Layout,
    /// The tables model (TATR v1.1-all) plus the platform's ONNX Runtime.
    Tables,
}

#[derive(Debug, Args)]
pub struct InstallArgs {
    /// Which packages to install; several in one call.
    #[arg(required = true)]
    pub packages: Vec<ModelPackage>,
}

#[derive(Debug, Args)]
pub struct StatusArgs {}

pub(crate) fn run(ctx: &Ctx, args: &ModelsArgs) -> u8 {
    match &args.command {
        ModelsCommands::Install(args) => run_install(ctx, args),
        ModelsCommands::Status(args) => run_status(ctx, args),
    }
}

/// Upper bound on a single installed file, as a guard against a hostile or
/// broken server streaming without end. The layout weights are some 300 MB;
/// the bound sits above them with room, and far below forever.
#[cfg(feature = "ort-layout")]
const MAX_INSTALL_BYTES: u64 = 512 * 1024 * 1024;

/// The release every install downloads from: the `models-v1` release, which
/// owns the asset names below too. Overridable for mirrors, offline installs
/// and the release smoke (see [`MODELS_BASE_ENV`]); the pins are enforced
/// identically whatever the source.
#[cfg(feature = "ort-layout")]
const MODELS_RELEASE_BASE: &str =
    "https://github.com/pdfluent/pdfluent-sdk/releases/download/models-v1/";

/// Source override for mirrors, offline installs and the release smoke:
/// `https://...` or `file:///dir/`. Anything else is refused.
#[cfg(feature = "ort-layout")]
const MODELS_BASE_ENV: &str = "PDFLUENT_MODELS_BASE";

/// TEST-ONLY pin override: a path to a file of `asset=sha256` lines that
/// fills pins the compiled table still marks TODO, so installs can be
/// exercised before the release pins land. It can NEVER weaken a release
/// pin: entries naming an unknown asset, or a filled pin with a different
/// digest, are refused; an equal digest is a no-op.
#[cfg(feature = "ort-layout")]
const TEST_PINS_ENV: &str = "PDFLUENT_MODELS_TEST_PINS";

/// The release workflow's self-check: when this is `1`, the
/// `release_pins_required_fails_while_the_table_has_todos` test fails while
/// ANY table entry is a TODO. The release workflow sets it, so a release
/// with unfilled pins refuses.
#[cfg(feature = "ort-layout")]
#[cfg(test)]
const RELEASE_PINS_REQUIRED_ENV: &str = "PDFLUENT_RELEASE_PINS_REQUIRED";

/// The marker a pin carries while nobody has recorded the digest: refused
/// before any download, because a file that cannot be verified must not be
/// fetched.
#[cfg(feature = "ort-layout")]
const TODO_PIN: &str = "TODO";

/// One release asset: the file under the base, its pin, and its size.
#[cfg(feature = "ort-layout")]
struct ReleaseAsset {
    /// The file under [`MODELS_RELEASE_BASE`], e.g. `heron_ep03.onnx`.
    remote: &'static str,
    /// Lowercase hex SHA-256, or [`TODO_PIN`] while unrecorded.
    sha256: &'static str,
    /// Exact byte size, checked beside the digest; `None` while the pin is
    /// a TODO (both land together).
    size: Option<u64>,
}

// The per-platform ONNX Runtime 1.23.2 library pins, and the two ONNX
// Runtime licence texts' pins: the sha256 of the EXTRACTED files from the
// official microsoft/onnxruntime v1.23.2 archives, every library reporting
// version 1.23.2. No sizes: for these the sha is the authority.
#[cfg(feature = "ort-layout")]
const ORT_MACOS_ARM64_SHA256: &str =
    "d306d2bc768540766c7ed8a1e0ff05d2870c77a934ebeee4a7bafa1b732ef299";
#[cfg(feature = "ort-layout")]
const ORT_MACOS_X86_64_SHA256: &str =
    "8c9c78de65ea3786f987c0d980e9c1b13a3a5fbc6b3e2965ba05b450e6e4c054";
#[cfg(feature = "ort-layout")]
const ORT_LINUX_X86_64_SHA256: &str =
    "13ab8084954fa4a47c777880180b90810d6020f021441395712b48a75b74c68b";
#[cfg(feature = "ort-layout")]
const ORT_LINUX_AARCH64_SHA256: &str =
    "648ffa64fbe027ae27139109410900cf776a030dec2dbbac51053318cc44c286";
#[cfg(feature = "ort-layout")]
const ORT_WINDOWS_X86_64_SHA256: &str =
    "dec964ab1ee36cc9b0ae247d13b376627992fc57dec0454354017ab8fd84f1ea";
#[cfg(feature = "ort-layout")]
const ORT_LICENSE_SHA256: &str = "2f07c72751aed99790b8a4869cf2311df85a860b22ded05fa22803587a48922c";
#[cfg(feature = "ort-layout")]
const ORT_NOTICES_SHA256: &str = "e9e90971a8e75a9a8ac0c6412e29c1202d079998389915aa485f46c816c3b4cc";

/// The ONE pin table: every asset `install` fetches -- models, runtime
/// libraries and licence texts. The compiled pins are the authority;
/// `SHA256SUMS` on the release is informational only. The model digests are
/// referenced from the model specs, not repeated, so the loader and the
/// installer verify against one definition.
#[cfg(feature = "ort-layout")]
const RELEASE_ASSETS: &[ReleaseAsset] = &[
    ReleaseAsset {
        remote: "heron_ep03.onnx",
        sha256: pdfluent::model::LAYOUT_EP03.sha256,
        size: Some(306_654_422),
    },
    ReleaseAsset {
        remote: "tatr-v1.1-all.onnx",
        sha256: pdfluent::model::TABLES_TATR.sha256,
        size: Some(118_206_817),
    },
    ReleaseAsset {
        remote: "onnxruntime-macos-arm64-1.23.2.dylib",
        sha256: ORT_MACOS_ARM64_SHA256,
        size: None,
    },
    ReleaseAsset {
        remote: "onnxruntime-macos-x86_64-1.23.2.dylib",
        sha256: ORT_MACOS_X86_64_SHA256,
        size: None,
    },
    ReleaseAsset {
        remote: "onnxruntime-linux-x86_64-1.23.2.so",
        sha256: ORT_LINUX_X86_64_SHA256,
        size: None,
    },
    ReleaseAsset {
        remote: "onnxruntime-linux-arm64-1.23.2.so",
        sha256: ORT_LINUX_AARCH64_SHA256,
        size: None,
    },
    ReleaseAsset {
        remote: "onnxruntime-windows-x86_64-1.23.2.dll",
        sha256: ORT_WINDOWS_X86_64_SHA256,
        size: None,
    },
    ReleaseAsset {
        remote: "onnxruntime-LICENSE.txt",
        sha256: ORT_LICENSE_SHA256,
        size: None,
    },
    ReleaseAsset {
        remote: "onnxruntime-ThirdPartyNotices.txt",
        sha256: ORT_NOTICES_SHA256,
        size: None,
    },
    // The notice, re-cut for the SDK dual licence, beside the two SDK
    // licence texts: pinned to the tree files' final bytes (models-v1's
    // MODELS-NOTICE.txt, the root LICENSE and LICENSE-COMMERCIAL).
    ReleaseAsset {
        remote: "MODELS-NOTICE.txt",
        sha256: "5eb1f80e9689fedb88c3e1f6c415c8341283e0e37ead0e12f0862e6191ac4b8b",
        size: Some(3_471),
    },
    ReleaseAsset {
        remote: "LICENSE-Apache-2.0.txt",
        sha256: "cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30",
        size: Some(11_358),
    },
    ReleaseAsset {
        remote: "LICENSE",
        sha256: "0d96a4ff68ad6d4b6f1f30f713b18d5184912ba8dd389f86aa7710db079abcb0",
        size: Some(34_523),
    },
    ReleaseAsset {
        remote: "LICENSE-COMMERCIAL",
        sha256: "ca54d9ab46d99e81d0dd2ea6ed112f3effa7f87cd3b65a876b1740c9efe599ed",
        size: Some(10_026),
    },
];

/// One platform's ONNX Runtime library: the release asset on this platform.
#[cfg(feature = "ort-layout")]
struct OrtLibrary {
    /// `os-arch`, for messages and the install line.
    platform: &'static str,
    /// The file under [`MODELS_RELEASE_BASE`]; the pin lives in
    /// [`RELEASE_ASSETS`].
    remote: &'static str,
}

#[cfg(feature = "ort-layout")]
const ORT_LIBRARIES: &[OrtLibrary] = &[
    OrtLibrary {
        platform: "macos-arm64",
        remote: "onnxruntime-macos-arm64-1.23.2.dylib",
    },
    OrtLibrary {
        platform: "macos-x86_64",
        remote: "onnxruntime-macos-x86_64-1.23.2.dylib",
    },
    OrtLibrary {
        platform: "linux-x86_64",
        remote: "onnxruntime-linux-x86_64-1.23.2.so",
    },
    OrtLibrary {
        platform: "linux-aarch64",
        remote: "onnxruntime-linux-arm64-1.23.2.so",
    },
    OrtLibrary {
        platform: "windows-x86_64",
        remote: "onnxruntime-windows-x86_64-1.23.2.dll",
    },
];

/// This platform's ONNX Runtime library, or the reason there is none.
#[cfg(feature = "ort-layout")]
fn library_for_this_platform() -> Result<&'static OrtLibrary, String> {
    let platform = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "macos-arm64",
        ("macos", "x86_64") => "macos-x86_64",
        ("linux", "x86_64") => "linux-x86_64",
        ("linux", "aarch64") => "linux-aarch64",
        ("windows", "x86_64") => "windows-x86_64",
        (os, arch) => {
            return Err(format!(
                "no ONNX Runtime 1.23.2 library is packaged for {os}-{arch}; \
                 this platform cannot run the model tiers"
            ));
        }
    };
    ORT_LIBRARIES
        .iter()
        .find(|lib| lib.platform == platform)
        .ok_or_else(|| format!("no ONNX Runtime library is listed for {platform}"))
}

/// One installable package: its model asset and its install line's licence.
#[cfg(feature = "ort-layout")]
struct PackageSpec {
    /// `layout`/`tables`, for messages and the install line.
    name: &'static str,
    /// The model file under the base; the pin lives in [`RELEASE_ASSETS`].
    model: &'static str,
    /// Licence short name for the install line.
    license: &'static str,
    /// Notice file the install line points at.
    notice: &'static str,
}

/// The licence table next to the pins: what each install line claims. Heron
/// carries the SDK's own dual licence (owner decision 08-10); the Apache
/// text beside it is IBM's base-model attribution, not this file's licence.
#[cfg(feature = "ort-layout")]
const PACKAGES: &[PackageSpec] = &[
    PackageSpec {
        name: "layout",
        model: "heron_ep03.onnx",
        license: "PDFluent SDK licence (AGPL-3.0 or commercial)",
        notice: "MODELS-NOTICE.txt",
    },
    PackageSpec {
        name: "tables",
        model: "tatr-v1.1-all.onnx",
        license: "MIT",
        notice: "MODELS-NOTICE.txt",
    },
];

/// The ONNX Runtime install line's licence.
#[cfg(feature = "ort-layout")]
const ORT_LICENSE: &str = "MIT";
/// The notice file the ONNX Runtime install line points at.
#[cfg(feature = "ort-layout")]
const ORT_NOTICE: &str = "onnxruntime-LICENSE.txt";

/// The root-level texts installed with either model.
#[cfg(feature = "ort-layout")]
const ROOT_TEXTS: &[&str] = &[
    "MODELS-NOTICE.txt",
    "LICENSE-Apache-2.0.txt",
    "LICENSE",
    "LICENSE-COMMERCIAL",
];

/// The licence texts beside the installed library.
#[cfg(feature = "ort-layout")]
const ORT_TEXTS: &[&str] = &[
    "onnxruntime-LICENSE.txt",
    "onnxruntime-ThirdPartyNotices.txt",
];

/// A pin `install` and `status` verify against: the digest plus the size
/// when the table knows it.
#[cfg(feature = "ort-layout")]
struct ResolvedPin {
    hex: String,
    size: Option<u64>,
}

/// Every override entry vetted against `table` before anything is fetched:
/// unknown assets, malformed digests and digests that would weaken a filled
/// pin are all refused. Pure over its inputs; the real path passes
/// [`RELEASE_ASSETS`].
#[cfg(feature = "ort-layout")]
fn check_overrides(
    table: &[ReleaseAsset],
    overrides: &HashMap<String, String>,
) -> Result<(), String> {
    for (remote, hex) in overrides {
        let Some(asset) = table.iter().find(|asset| asset.remote == remote) else {
            return Err(format!(
                "{TEST_PINS_ENV} names {remote}, which is not a release asset"
            ));
        };
        if decode_pin(hex).is_none() {
            return Err(format!(
                "{TEST_PINS_ENV} pins {remote} with a malformed sha256 \
                 (want 64 hex characters)"
            ));
        }
        if asset.sha256 != TODO_PIN && asset.sha256 != hex {
            return Err(format!(
                "{TEST_PINS_ENV} must not weaken the release pin for {remote}: \
                 the compiled pins are the authority"
            ));
        }
    }
    Ok(())
}

/// Look an asset's pin up in `table`, letting `overrides` fill TODOs. Pure
/// over its inputs so tests drive it with a fake table; the real path passes
/// [`RELEASE_ASSETS`]. Call [`check_overrides`] first: a filled pin's
/// override is vetted there, so here an override only ever fills a TODO.
#[cfg(feature = "ort-layout")]
fn resolve_pin(
    table: &[ReleaseAsset],
    remote: &str,
    overrides: &HashMap<String, String>,
) -> Result<ResolvedPin, String> {
    let asset = table
        .iter()
        .find(|asset| asset.remote == remote)
        .ok_or_else(|| format!("unknown release asset {remote}"))?;
    if asset.sha256 == TODO_PIN {
        match overrides.get(remote) {
            Some(hex) => Ok(ResolvedPin {
                hex: hex.clone(),
                size: None,
            }),
            None => Err(format!(
                "refusing to install {remote}: no sha256 is pinned for it; \
                 a file that cannot be verified must not be fetched"
            )),
        }
    } else {
        Ok(ResolvedPin {
            hex: asset.sha256.to_string(),
            size: asset.size,
        })
    }
}

/// Every asset whose compiled pin is still [`TODO_PIN`]: the release debt
/// `status` reports and the release self-check refuses.
#[cfg(feature = "ort-layout")]
fn todo_assets(table: &[ReleaseAsset]) -> Vec<&str> {
    table
        .iter()
        .filter(|asset| asset.sha256 == TODO_PIN)
        .map(|asset| asset.remote)
        .collect()
}

/// The download base: the override or [`MODELS_RELEASE_BASE`], validated and
/// slash-terminated. Pure over the override so tests drive it without the
/// environment.
#[cfg(feature = "ort-layout")]
fn resolve_base(override_base: Option<&str>) -> Result<String, (String, String)> {
    let base = override_base.unwrap_or(MODELS_RELEASE_BASE);
    if !(base.starts_with("https://") || base.starts_with("file://")) {
        return Err((
            "MODEL_NOT_AVAILABLE".to_string(),
            format!(
                "{MODELS_BASE_ENV} must be an https:// or file:/// URL, got {base:?}; \
                 pins are enforced identically against every source"
            ),
        ));
    }
    Ok(if base.ends_with('/') {
        base.to_string()
    } else {
        format!("{base}/")
    })
}

/// Parse [`TEST_PINS_ENV`] text: `asset=sha256` lines, `#` comments and
/// blanks skipped. Pure so tests drive it without a file.
#[cfg(feature = "ort-layout")]
fn parse_test_pins(text: &str) -> Result<HashMap<String, String>, String> {
    let mut out = HashMap::new();
    for (n, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((remote, hex)) = line.split_once('=') else {
            return Err(format!("line {}: want `asset=sha256`, got {raw:?}", n + 1));
        };
        let (remote, hex) = (remote.trim(), hex.trim());
        if remote.is_empty() || hex.is_empty() {
            return Err(format!("line {}: want `asset=sha256`, got {raw:?}", n + 1));
        }
        out.insert(remote.to_string(), hex.to_ascii_lowercase());
    }
    Ok(out)
}

/// Load [`TEST_PINS_ENV`] when set: an empty variable counts as unset.
#[cfg(feature = "ort-layout")]
fn load_test_overrides() -> Result<HashMap<String, String>, (String, String)> {
    let path = match std::env::var(TEST_PINS_ENV)
        .ok()
        .filter(|path| !path.is_empty())
    {
        None => return Ok(HashMap::new()),
        Some(path) => path,
    };
    let text = std::fs::read_to_string(&path).map_err(|e| {
        (
            "MODEL_NOT_AVAILABLE".to_string(),
            format!("cannot read {TEST_PINS_ENV} file {path}: {e}"),
        )
    })?;
    parse_test_pins(&text).map_err(|message| {
        (
            "MODEL_NOT_AVAILABLE".to_string(),
            format!("cannot parse {TEST_PINS_ENV} file {path}: {message}"),
        )
    })
}

/// One file `install` places: what to fetch, where it lands, and what the
/// install line says when it is placed.
#[cfg(feature = "ort-layout")]
struct PlannedFile {
    remote: &'static str,
    dest: PathBuf,
    kind: PlannedKind,
}

#[cfg(feature = "ort-layout")]
enum PlannedKind {
    /// A package model: prints its `installed ...` line when placed.
    Model {
        package: &'static str,
        license: &'static str,
        notice: &'static str,
    },
    /// The platform runtime: prints its `installed ...` line when placed.
    Runtime { platform: &'static str },
    /// A licence text: placed silently.
    Text,
}

/// The model spec a package installs: the SAME spec the stage resolves, so
/// the install lands where the loader looks.
#[cfg(feature = "ort-layout")]
fn model_spec(package: ModelPackage) -> &'static pdfluent::model::ModelSpec {
    match package {
        ModelPackage::Layout => &pdfluent::model::LAYOUT_EP03,
        ModelPackage::Tables => &pdfluent::model::TABLES_TATR,
    }
}

/// The licence row a package installs under.
#[cfg(feature = "ort-layout")]
fn package_spec(package: ModelPackage) -> &'static PackageSpec {
    match package {
        ModelPackage::Layout => &PACKAGES[0],
        ModelPackage::Tables => &PACKAGES[1],
    }
}

/// The install plan for `packages`: models in CLI order (deduped), then the
/// runtime library once, then the texts. Pure over the config so tests point
/// it at a scratch root; the real path passes a default config.
#[cfg(feature = "ort-layout")]
fn build_plan(
    packages: &[ModelPackage],
    config: &pdfluent::model::RuntimeConfig,
) -> Result<Vec<PlannedFile>, String> {
    let library = library_for_this_platform()?;
    let root = pdfluent::model::resolve_model_root(config);
    let lib_dest = pdfluent::model::resolve_ort_dylib(config);
    let ort_dir = lib_dest
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_else(|| root.clone());
    let mut plan = Vec::new();
    let mut seen: Vec<ModelPackage> = Vec::new();
    for package in packages {
        if seen.contains(package) {
            continue;
        }
        seen.push(*package);
        let spec = package_spec(*package);
        plan.push(PlannedFile {
            remote: spec.model,
            dest: pdfluent::model::resolve_model_file(model_spec(*package), config),
            kind: PlannedKind::Model {
                package: spec.name,
                license: spec.license,
                notice: spec.notice,
            },
        });
    }
    if seen.is_empty() {
        return Err("nothing to install".to_string());
    }
    plan.push(PlannedFile {
        remote: library.remote,
        dest: lib_dest,
        kind: PlannedKind::Runtime {
            platform: library.platform,
        },
    });
    for text in ORT_TEXTS {
        plan.push(PlannedFile {
            remote: text,
            dest: ort_dir.join(text),
            kind: PlannedKind::Text,
        });
    }
    for text in ROOT_TEXTS {
        plan.push(PlannedFile {
            remote: text,
            dest: root.join(text),
            kind: PlannedKind::Text,
        });
    }
    Ok(plan)
}

/// The `installed ...` line a placed file prints, or nothing for the silent
/// licence texts.
#[cfg(feature = "ort-layout")]
fn installed_line(file: &PlannedFile) -> Option<String> {
    match &file.kind {
        PlannedKind::Model {
            package,
            license,
            notice,
        } => Some(format!(
            "installed {} ({package}) — {license} — see {notice}",
            file.remote
        )),
        PlannedKind::Runtime { platform } => Some(format!(
            "installed onnxruntime {} ({platform}) — {ORT_LICENSE} — see {ORT_NOTICE}",
            pdfluent::model::ORT_VERSION
        )),
        PlannedKind::Text => None,
    }
}

/// The JSON twin of the install lines: one entry per placed model and the
/// runtime, carrying the same fields, plus the remotes skipped as already
/// verified.
#[cfg(feature = "ort-layout")]
fn install_json(placed: &[&PlannedFile], skipped: &[&str]) -> serde_json::Value {
    let mut installed = Vec::new();
    for file in placed {
        match &file.kind {
            PlannedKind::Model {
                package,
                license,
                notice,
            } => installed.push(json!({
                "package": package,
                "file": file.remote,
                "path": file.dest.to_string_lossy(),
                "license": license,
                "notice": notice,
            })),
            PlannedKind::Runtime { platform } => installed.push(json!({
                "package": "onnxruntime",
                "file": file.remote,
                "path": file.dest.to_string_lossy(),
                "version": pdfluent::model::ORT_VERSION,
                "platform": platform,
                "license": ORT_LICENSE,
                "notice": ORT_NOTICE,
            })),
            PlannedKind::Text => {}
        }
    }
    json!({ "installed": installed, "skipped": skipped })
}

#[cfg(feature = "ort-layout")]
fn run_install(ctx: &Ctx, args: &InstallArgs) -> u8 {
    let base = match resolve_base(
        std::env::var(MODELS_BASE_ENV)
            .ok()
            .filter(|base| !base.is_empty())
            .as_deref(),
    ) {
        Ok(base) => base,
        Err((code, message)) => return output::error("models", ctx, &code, &message),
    };
    let overrides = match load_test_overrides() {
        Ok(overrides) => overrides,
        Err((code, message)) => return output::error("models", ctx, &code, &message),
    };
    if let Err(message) = check_overrides(RELEASE_ASSETS, &overrides) {
        return output::error("models", ctx, "MODEL_NOT_AVAILABLE", &message);
    }
    let config = pdfluent::model::RuntimeConfig::new();
    let plan = match build_plan(&args.packages, &config) {
        Ok(plan) => plan,
        Err(message) => return output::error("models", ctx, "MODEL_NOT_AVAILABLE", &message),
    };
    // Every pin resolves BEFORE any download: a refused asset stops the
    // install while the model dir is still untouched.
    let mut pins = Vec::with_capacity(plan.len());
    for file in &plan {
        match resolve_pin(RELEASE_ASSETS, file.remote, &overrides) {
            Ok(pin) => pins.push(pin),
            Err(message) => {
                return output::error("models", ctx, "MODEL_NOT_AVAILABLE", &message);
            }
        }
    }
    let mut placed: Vec<&PlannedFile> = Vec::new();
    let mut skipped: Vec<&str> = Vec::new();
    for (file, pin) in plan.iter().zip(&pins) {
        if state_of(&file.dest, &pin.hex) == FileState::Verified {
            skipped.push(file.remote);
            continue;
        }
        let url = format!("{base}{}", file.remote);
        if let Err((code, message)) = fetch_verified(&url, pin, &file.dest) {
            return output::error("models", ctx, &code, &message);
        }
        placed.push(file);
    }
    let lines: Vec<String> = placed
        .iter()
        .filter_map(|file| installed_line(file))
        .collect();
    let data = install_json(&placed, &skipped);
    output::success("models", ctx, data, Vec::new(), |ctx| {
        for line in &lines {
            output::note(ctx, line);
        }
    })
}

/// A lowercase hex digest to bytes, or `None` unless it is 64 hex characters.
#[cfg(feature = "ort-layout")]
fn decode_pin(expected: &str) -> Option<[u8; 32]> {
    if expected.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&expected[2 * i..2 * i + 2], 16).ok()?;
    }
    Some(out)
}

/// Make room for one download: the destination's parent directories.
#[cfg(feature = "ort-layout")]
fn ensure_parent(dest: &std::path::Path) -> Result<(), (String, String)> {
    if let Some(parent) = dest.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            return Err((
                "IO".to_string(),
                format!("could not create directory {}: {e}", parent.display()),
            ));
        }
    }
    Ok(())
}

/// The local path a `file://` URL names (RFC 8089), as a string for
/// `File::open`. `windows` selects the host's path rules; it is a parameter
/// so both forms are tested on every host.
///
/// Accepted: `file:///abs/path` and `file://localhost/abs/path`; on Windows
/// also `file:///C:/dir` and `file://localhost/C:/dir` (the leading slash
/// before the drive letter is dropped), `file://server/share/dir` (a UNC
/// path), and the legacy `file://C:/dir` / `file://C:\dir` that 1.1.0 read.
/// Stripping `file://` and opening the rest, as 1.1.0 did, turned the
/// standard `file:///C:/dir` into `/C:/dir`, which Windows refuses (os
/// error 123). Percent-escapes are decoded; a host other than `localhost`
/// is refused off Windows.
#[cfg(feature = "ort-layout")]
fn file_url_to_path(url: &str, windows: bool) -> Result<String, String> {
    let rest = url
        .strip_prefix("file://")
        .ok_or_else(|| format!("not a file:// URL: {url:?}"))?;
    let is_drive = |s: &str| {
        let b = s.as_bytes();
        b.len() >= 2 && b[0].is_ascii_alphabetic() && (b[1] == b':' || b[1] == b'|')
    };
    let drive_fixed = |s: &str| -> String {
        // `C|` is the old spelling of `C:`.
        let mut out = s.to_string();
        if out.as_bytes().get(1) == Some(&b'|') {
            out.replace_range(1..2, ":");
        }
        out
    };
    if windows && is_drive(rest) {
        return Ok(drive_fixed(&percent_decode(rest)?));
    }
    let (host, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    let path = percent_decode(path)?;
    if path.is_empty() || (path == "/" && host.is_empty()) {
        return Err(format!("no path in {url:?}"));
    }
    let local_host = host.is_empty() || host.eq_ignore_ascii_case("localhost");
    if !windows {
        if !local_host {
            return Err(format!(
                "{url:?} names host {host:?}; only a local file URL is read"
            ));
        }
        return Ok(path);
    }
    if !local_host {
        return Ok(format!("\\\\{host}{}", path.replace('/', "\\")));
    }
    match path.strip_prefix('/') {
        Some(after) if is_drive(after) => Ok(drive_fixed(after)),
        // No example drive path in this text: on Windows the binary scan
        // reads one in a shipped string as a leaked build-host path.
        _ => Err(format!(
            "{url:?} names no drive letter; on Windows put one after file:///"
        )),
    }
}

/// `%XX` escapes to bytes, the result read as UTF-8.
#[cfg(feature = "ort-layout")]
fn percent_decode(s: &str) -> Result<String, String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = bytes
                .get(i + 1..i + 3)
                .and_then(|h| std::str::from_utf8(h).ok())
                .and_then(|h| u8::from_str_radix(h, 16).ok())
                .ok_or_else(|| format!("bad percent-escape in {s:?}"))?;
            out.push(hex);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| format!("percent-escapes in {s:?} are not UTF-8"))
}

/// Download one file, verify it, and only then place it.
///
/// A `file://` source reads from the local dir (the release smoke and the
/// tests); anything else goes over HTTPS. Either way the bytes stream
/// through a hasher into a `.part` file beside the target: the target is
/// written by rename only after the size and digest match, so a reader never
/// observes unverified bytes at a final name. On any failure the `.part`
/// file is removed and nothing is left behind. Returns the CLI code beside
/// the message.
#[cfg(feature = "ort-layout")]
fn fetch_verified(
    url: &str,
    pin: &ResolvedPin,
    dest: &std::path::Path,
) -> Result<(), (String, String)> {
    let fail = |code: &str, message: String| (code.to_string(), message);
    let Some(expected) = decode_pin(&pin.hex) else {
        return Err(fail(
            "MODEL_NOT_AVAILABLE",
            format!("refusing to download {url}: no sha256 is pinned for it; a file that cannot be verified must not be fetched"),
        ));
    };
    if url.starts_with("file://") {
        let local = match file_url_to_path(url, cfg!(windows)) {
            Ok(local) => local,
            Err(e) => {
                return Err(fail(
                    "MODEL_NOT_AVAILABLE",
                    format!("could not download {url}: {e}"),
                ));
            }
        };
        let file = match std::fs::File::open(&local) {
            Ok(file) => file,
            Err(e) => {
                return Err(fail(
                    "MODEL_NOT_AVAILABLE",
                    format!("could not download {url}: {e}"),
                ));
            }
        };
        ensure_parent(dest)?;
        let tmp = dest.with_extension("part");
        return place_stream(file, &expected, pin.size, &tmp, dest, url, &pin.hex);
    }
    ensure_parent(dest)?;
    let tmp = dest.with_extension("part");
    let agent = ureq::Agent::new_with_defaults();
    let response = agent.get(url).call();
    let response = match response {
        Ok(response) => response,
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            return Err(fail(
                "MODEL_NOT_AVAILABLE",
                format!("could not download {url}: {e}"),
            ));
        }
    };
    let reader = response.into_body().into_reader();
    place_stream(reader, &expected, pin.size, &tmp, dest, url, &pin.hex)
}

/// Stream one download through the hasher into `tmp`, verifying the size and
/// the digest, and rename it onto `dest` only when both match.
///
/// The target is written by rename after the match, so a reader never
/// observes unverified bytes at a final name. On any failure the `.part`
/// file is removed and nothing is left behind. Takes the stream rather than
/// the URL so tests feed bytes without a socket.
#[cfg(feature = "ort-layout")]
fn place_stream(
    mut reader: impl Read,
    expected: &[u8; 32],
    expected_size: Option<u64>,
    tmp: &std::path::Path,
    dest: &std::path::Path,
    url: &str,
    expected_sha256: &str,
) -> Result<(), (String, String)> {
    let fail = |code: &str, message: String| (code.to_string(), message);
    let mut stream = || -> Result<(bool, u64), String> {
        let mut file = std::fs::File::create(tmp)
            .map_err(|e| format!("could not write {}: {e}", tmp.display()))?;
        let mut hasher = sha2::Sha256::new();
        let mut buf = vec![0u8; 64 * 1024];
        let mut total: u64 = 0;
        loop {
            let n = reader
                .read(&mut buf)
                .map_err(|e| format!("{url}: read error: {e}"))?;
            if n == 0 {
                break;
            }
            total += n as u64;
            if total > MAX_INSTALL_BYTES {
                return Err(format!(
                    "{url}: exceeds {MAX_INSTALL_BYTES} bytes; refusing to continue"
                ));
            }
            hasher.update(&buf[..n]);
            file.write_all(&buf[..n])
                .map_err(|e| format!("could not write {}: {e}", tmp.display()))?;
        }
        file.sync_all()
            .map_err(|e| format!("could not write {}: {e}", tmp.display()))?;
        drop(file);
        Ok((hasher.finalize().as_slice() == expected, total))
    };
    match stream() {
        Ok((digest_ok, total)) => {
            if let Some(size) = expected_size {
                if size != total {
                    let _ = std::fs::remove_file(tmp);
                    return Err(fail(
                        "MODEL_NOT_AVAILABLE",
                        format!("{url}: expected {size} bytes, got {total}; nothing was installed"),
                    ));
                }
            }
            if !digest_ok {
                let _ = std::fs::remove_file(tmp);
                return Err(fail(
                    "MODEL_NOT_AVAILABLE",
                    format!(
                        "{url} failed its checksum: expected sha256 {expected_sha256}; nothing was installed"
                    ),
                ));
            }
            std::fs::rename(tmp, dest)
                .map_err(|e| fail("IO", format!("could not install {}: {e}", dest.display())))
        }
        Err(message) => {
            let _ = std::fs::remove_file(tmp);
            let code = if message.contains("exceeds") || message.contains("read error") {
                "MODEL_NOT_AVAILABLE"
            } else {
                "IO"
            };
            Err(fail(code, message))
        }
    }
}

/// What `status` found at one install path.
#[cfg(feature = "ort-layout")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileState {
    /// Nothing at the path.
    Missing,
    /// Present, and the digest matches the pin.
    Verified,
    /// Present, but the digest differs from the pin.
    Mismatch,
    /// Present, but no digest is pinned, so it cannot be verified.
    Unpinned,
}

#[cfg(feature = "ort-layout")]
impl FileState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Verified => "verified",
            Self::Mismatch => "mismatch",
            Self::Unpinned => "unpinned",
        }
    }
}

/// Hash a present file by streaming it and compare against the pin.
#[cfg(feature = "ort-layout")]
fn state_of(path: &std::path::Path, expected_sha256: &str) -> FileState {
    if !path.is_file() {
        return FileState::Missing;
    }
    let Some(expected) = decode_pin(expected_sha256) else {
        return FileState::Unpinned;
    };
    let mut file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(_) => return FileState::Missing,
    };
    let mut hasher = sha2::Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = match file.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => return FileState::Mismatch,
        };
        hasher.update(&buf[..n]);
    }
    if hasher.finalize().as_slice() == expected {
        FileState::Verified
    } else {
        FileState::Mismatch
    }
}

/// One status row: the state plus the present file's size.
#[cfg(feature = "ort-layout")]
struct StatusRow {
    state: FileState,
    size: Option<u64>,
}

/// The status of one install path under its effective pin: the compiled pin,
/// or the test override where one fills a TODO. An asset no pin resolves
/// for reads as missing or unpinned, never as verified.
#[cfg(feature = "ort-layout")]
fn status_row(
    path: &std::path::Path,
    remote: &str,
    overrides: &HashMap<String, String>,
) -> StatusRow {
    let state = match resolve_pin(RELEASE_ASSETS, remote, overrides) {
        Ok(pin) => state_of(path, &pin.hex),
        Err(_) => {
            if path.is_file() {
                FileState::Unpinned
            } else {
                FileState::Missing
            }
        }
    };
    let size = std::fs::metadata(path).ok().map(|attr| attr.len());
    StatusRow {
        state,
        size: if path.is_file() { size } else { None },
    }
}

#[cfg(feature = "ort-layout")]
fn run_status(ctx: &Ctx, _args: &StatusArgs) -> u8 {
    let overrides = match load_test_overrides() {
        Ok(overrides) => overrides,
        Err((code, message)) => return output::error("models", ctx, &code, &message),
    };
    let config = pdfluent::model::RuntimeConfig::new();
    let layout_path = pdfluent::model::resolve_model_file(&pdfluent::model::LAYOUT_EP03, &config);
    let tables_path = pdfluent::model::resolve_model_file(&pdfluent::model::TABLES_TATR, &config);
    let lib_path = pdfluent::model::resolve_ort_dylib(&config);
    let layout = status_row(&layout_path, "heron_ep03.onnx", &overrides);
    let tables = status_row(&tables_path, "tatr-v1.1-all.onnx", &overrides);
    let runtime = match library_for_this_platform() {
        Ok(library) => status_row(&lib_path, library.remote, &overrides),
        Err(_) => StatusRow {
            state: if lib_path.is_file() {
                FileState::Unpinned
            } else {
                FileState::Missing
            },
            size: None,
        },
    };
    let todos = todo_assets(RELEASE_ASSETS);
    let line = |label: &str, path: &std::path::Path, row: &StatusRow| {
        let sized = |text: &str| match row.size {
            Some(n) => format!("{text} ({n} bytes)"),
            None => text.to_string(),
        };
        match row.state {
            FileState::Missing => format!("{label} {}: missing", path.display()),
            FileState::Verified => sized(&format!("{label} {}: sha256 ok", path.display())),
            FileState::Mismatch => sized(&format!("{label} {}: CHECKSUM MISMATCH", path.display())),
            FileState::Unpinned => sized(&format!(
                "{label} {}: present, no sha256 pinned",
                path.display()
            )),
        }
    };
    let layout_line = line("layout", &layout_path, &layout);
    let tables_line = line("tables", &tables_path, &tables);
    let runtime_line = line("onnxruntime", &lib_path, &runtime);
    let todos_line = if todos.is_empty() {
        "todo pins: none".to_string()
    } else {
        format!("todo pins: {}", todos.join(", "))
    };
    output::success(
        "models",
        ctx,
        json!({
            "models": {
                "layout": {
                    "path": layout_path.to_string_lossy(),
                    "state": layout.state.as_str(),
                    "size": layout.size,
                },
                "tables": {
                    "path": tables_path.to_string_lossy(),
                    "state": tables.state.as_str(),
                    "size": tables.size,
                },
            },
            "runtime": {
                "path": lib_path.to_string_lossy(),
                "state": runtime.state.as_str(),
                "size": runtime.size,
            },
            "todo_pins": todos,
        }),
        Vec::new(),
        |ctx| {
            output::note(ctx, &layout_line);
            output::note(ctx, &tables_line);
            output::note(ctx, &runtime_line);
            output::note(ctx, &todos_line);
        },
    )
}

#[cfg(not(feature = "ort-layout"))]
fn run_install(ctx: &Ctx, _args: &InstallArgs) -> u8 {
    output::error(
        "models",
        ctx,
        "MODEL_NOT_AVAILABLE",
        "this `pdfluent` was built without the `ort-layout` cargo feature, so it cannot install the layout and tables models; rebuild with `--features ort-layout`",
    )
}

#[cfg(not(feature = "ort-layout"))]
fn run_status(ctx: &Ctx, _args: &StatusArgs) -> u8 {
    output::error(
        "models",
        ctx,
        "MODEL_NOT_AVAILABLE",
        "this `pdfluent` was built without the `ort-layout` cargo feature, so it cannot inspect the model dir; rebuild with `--features ort-layout`",
    )
}

#[cfg(feature = "ort-layout")]
#[cfg(test)]
mod tests {
    use super::*;

    fn sha256_hex(bytes: &[u8]) -> String {
        let mut hasher = sha2::Sha256::new();
        hasher.update(bytes);
        hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    }

    fn scratch_dir(tag: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("pdfluent-models-test-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a scratch dir");
        dir
    }

    /// The install path with the socket replaced by bytes: `place_stream`
    /// takes the download as a reader, so the fake release is a cursor.
    /// No network at all; the ureq handshake above it is the same three
    /// lines the paddle fetch uses.
    fn place_bytes(
        body: &[u8],
        expected_sha256: &str,
        expected_size: Option<u64>,
        dest: &std::path::Path,
    ) -> Result<(), (String, String)> {
        let expected = decode_pin(expected_sha256).expect("the test pins a sha256");
        let tmp = dest.with_extension("part");
        place_stream(
            std::io::Cursor::new(body.to_vec()),
            &expected,
            expected_size,
            &tmp,
            dest,
            "https://example.invalid/file",
            expected_sha256,
        )
    }

    /// A fake pin table: one filled asset and one TODO, so the refusal and
    /// override logic is pinned without depending on the release's state.
    fn fake_table(filled_hex: &'static str) -> Vec<ReleaseAsset> {
        vec![
            ReleaseAsset {
                remote: "filled.onnx",
                sha256: filled_hex,
                size: Some(3),
            },
            ReleaseAsset {
                remote: "todo.onnx",
                sha256: TODO_PIN,
                size: None,
            },
        ]
    }

    #[test]
    fn a_correct_file_installs_and_verifies() {
        let body = vec![7u8; 200_000];
        let dir = scratch_dir("ok");
        let dest = dir.join("file.onnx");
        place_bytes(&body, &sha256_hex(&body), None, &dest).expect("a correct file installs");
        assert_eq!(std::fs::read(&dest).expect("the file reads"), body);
        assert!(
            !dest.with_extension("part").exists(),
            "no .part file is left behind"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_with_its_pinned_size_installs() {
        let body = vec![7u8; 10_000];
        let dir = scratch_dir("size-ok");
        let dest = dir.join("file.onnx");
        place_bytes(&body, &sha256_hex(&body), Some(10_000), &dest)
            .expect("the pinned size installs");
        assert_eq!(std::fs::read(&dest).expect("the file reads"), body);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_wrong_size_refuses_and_leaves_nothing_behind() {
        let body = vec![7u8; 10_000];
        let dir = scratch_dir("size-wrong");
        let dest = dir.join("file.onnx");
        let Err((code, message)) = place_bytes(&body, &sha256_hex(&body), Some(9_999), &dest)
        else {
            panic!("a wrong size must refuse");
        };
        assert_eq!(code, "MODEL_NOT_AVAILABLE");
        assert!(
            message.contains("expected 9999 bytes, got 10000"),
            "{message}"
        );
        assert!(message.contains("nothing was installed"), "{message}");
        assert!(!dest.exists(), "the target is not written");
        assert!(
            !dest.with_extension("part").exists(),
            "the .part file is removed"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_wrong_sha_refuses_and_leaves_nothing_behind() {
        let body = vec![9u8; 100_000];
        let dir = scratch_dir("wrong-sha");
        let dest = dir.join("file.onnx");
        let wrong = sha256_hex(b"something else entirely");
        let Err((code, message)) = place_bytes(&body, &wrong, None, &dest) else {
            panic!("a wrong sha must refuse");
        };
        assert_eq!(code, "MODEL_NOT_AVAILABLE");
        assert!(message.contains("checksum"), "{message}");
        assert!(message.contains("nothing was installed"), "{message}");
        assert!(!dest.exists(), "the target is not written");
        assert!(
            !dest.with_extension("part").exists(),
            "the .part file is removed"
        );
        assert!(
            std::fs::read_dir(&dir)
                .expect("the dir reads")
                .next()
                .is_none(),
            "nothing is left behind"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_unpinned_file_refuses_before_any_download() {
        // A TODO pin never reaches the network: the URL is unroutable, so a
        // fetch attempt would fail with a download error rather than this.
        let dir = scratch_dir("todo-pin");
        let dest = dir.join("file.onnx");
        let pin = ResolvedPin {
            hex: TODO_PIN.to_string(),
            size: None,
        };
        let Err((code, message)) = fetch_verified("http://127.0.0.1:1/file", &pin, &dest) else {
            panic!("a TODO pin must refuse");
        };
        assert_eq!(code, "MODEL_NOT_AVAILABLE");
        assert!(message.contains("no sha256 is pinned"), "{message}");
        assert!(!dest.exists(), "the target is not written");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_windows_drive_letter_file_url_names_the_drive_path() {
        // 1.1.0 stripped `file://` and opened `/C:/x/`: os error 123 on
        // Windows, for the RFC 8089 form its own docs ask for.
        for (url, want) in [
            ("file:///C:/x/", "C:/x/"),
            ("file://localhost/C:/x/", "C:/x/"),
            ("file://LOCALHOST/C:/x/", "C:/x/"),
            ("file:///c:/x/heron_ep03.onnx", "c:/x/heron_ep03.onnx"),
            ("file:///C|/x/", "C:/x/"),
            ("file:///C:/with%20space/", "C:/with space/"),
            // The legacy two-slash forms 1.1.0 did read keep working.
            ("file://E:/x/", "E:/x/"),
            ("file://C:\\x\\y/", "C:\\x\\y/"),
            // UNC.
            ("file://server/share/x", "\\\\server\\share\\x"),
        ] {
            assert_eq!(file_url_to_path(url, true).as_deref(), Ok(want), "{url}");
        }
        for bad in ["file:///x/", "file://localhost/x/", "file:///", "file://"] {
            assert!(
                file_url_to_path(bad, true).is_err(),
                "{bad} must refuse on Windows"
            );
        }
        assert_eq!(
            file_url_to_path("file:///x/", true),
            Err(
                "\"file:///x/\" names no drive letter; on Windows put one after file:///"
                    .to_string()
            )
        );
    }

    #[test]
    fn a_posix_file_url_names_the_absolute_path() {
        for (url, want) in [
            ("file:///tmp/x/", "/tmp/x/"),
            ("file://localhost/tmp/x/", "/tmp/x/"),
            ("file:///tmp/a%20b/c", "/tmp/a b/c"),
            // On POSIX `/C:/x` is an ordinary absolute path.
            ("file:///C:/x/", "/C:/x/"),
        ] {
            assert_eq!(file_url_to_path(url, false).as_deref(), Ok(want), "{url}");
        }
        for bad in [
            "file://server/share/x",
            "file:///",
            "file://",
            "file:///tmp/%zz",
            "file:///tmp/%ff",
            "https:///tmp/x",
        ] {
            assert!(file_url_to_path(bad, false).is_err(), "{bad} must refuse");
        }
    }

    #[test]
    fn a_standard_file_url_of_this_host_installs() {
        // The RFC 8089 spelling of a real local dir on THIS host:
        // `file:///C:/...` on Windows, `file:///tmp/...` elsewhere.
        let body = vec![7u8; 4_096];
        let dir = scratch_dir("std-file-url");
        let source = dir.join("source");
        std::fs::create_dir(&source).expect("the source dir");
        std::fs::write(source.join("fake.onnx"), &body).expect("the source file");
        let pin = ResolvedPin {
            hex: sha256_hex(&body),
            size: Some(body.len() as u64),
        };
        let mut path = source.display().to_string().replace('\\', "/");
        if !path.starts_with('/') {
            path.insert(0, '/');
        }
        let url = format!("file://{path}/fake.onnx");
        assert!(url.starts_with("file:///"), "{url}");
        let dest = dir.join("root").join("fake.onnx");
        fetch_verified(&url, &pin, &dest).expect("a standard file URL installs");
        assert_eq!(std::fs::read(&dest).expect("the file reads"), body);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_url_installs_from_the_local_dir() {
        // The release smoke and the mirrors: `file://` reads from disk, and
        // the pins are enforced identically.
        let body = vec![11u8; 50_000];
        let dir = scratch_dir("file-url");
        let source = dir.join("source");
        std::fs::create_dir(&source).expect("the source dir");
        std::fs::write(source.join("fake.onnx"), &body).expect("the source file");
        let hex = sha256_hex(&body);
        let pin = ResolvedPin {
            hex: hex.clone(),
            size: Some(body.len() as u64),
        };
        let dest = dir.join("root").join("fake.onnx");
        let url = format!("file://{}/fake.onnx", source.display());
        fetch_verified(&url, &pin, &dest).expect("a correct local file installs");
        assert_eq!(std::fs::read(&dest).expect("the file reads"), body);
        // And a file the source does not carry refuses, leaving nothing.
        let missing = dir.join("root").join("missing.onnx");
        let missing_url = format!("file://{}/missing.onnx", source.display());
        let Err((code, message)) = fetch_verified(&missing_url, &pin, &missing) else {
            panic!("a missing source file must refuse");
        };
        assert_eq!(code, "MODEL_NOT_AVAILABLE");
        assert!(message.contains("could not download"), "{message}");
        assert!(!missing.exists(), "the target is not written");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_table_names_every_release_asset() {
        let remotes: Vec<&str> = RELEASE_ASSETS.iter().map(|asset| asset.remote).collect();
        assert_eq!(
            remotes,
            vec![
                "heron_ep03.onnx",
                "tatr-v1.1-all.onnx",
                "onnxruntime-macos-arm64-1.23.2.dylib",
                "onnxruntime-macos-x86_64-1.23.2.dylib",
                "onnxruntime-linux-x86_64-1.23.2.so",
                "onnxruntime-linux-arm64-1.23.2.so",
                "onnxruntime-windows-x86_64-1.23.2.dll",
                "onnxruntime-LICENSE.txt",
                "onnxruntime-ThirdPartyNotices.txt",
                "MODELS-NOTICE.txt",
                "LICENSE-Apache-2.0.txt",
                "LICENSE",
                "LICENSE-COMMERCIAL",
            ]
        );
        // The model digests are the specs' digests, not a second copy: the
        // loader and the installer verify against one definition.
        let heron = &RELEASE_ASSETS[0];
        assert_eq!(heron.sha256, pdfluent::model::LAYOUT_EP03.sha256);
        assert_eq!(heron.size, Some(306_654_422));
        let tatr = &RELEASE_ASSETS[1];
        assert_eq!(tatr.sha256, pdfluent::model::TABLES_TATR.sha256);
        assert_eq!(tatr.size, Some(118_206_817));
        // The runtime pins, byte for byte what the release pins: the sha256
        // of the EXTRACTED files from the official v1.23.2 archives, with no
        // sizes (the sha is the authority).
        for (asset, expected) in [
            (&RELEASE_ASSETS[2], ORT_MACOS_ARM64_SHA256),
            (&RELEASE_ASSETS[3], ORT_MACOS_X86_64_SHA256),
            (&RELEASE_ASSETS[4], ORT_LINUX_X86_64_SHA256),
            (&RELEASE_ASSETS[5], ORT_LINUX_AARCH64_SHA256),
            (&RELEASE_ASSETS[6], ORT_WINDOWS_X86_64_SHA256),
            (&RELEASE_ASSETS[7], ORT_LICENSE_SHA256),
            (&RELEASE_ASSETS[8], ORT_NOTICES_SHA256),
        ] {
            assert_eq!(asset.sha256, expected);
            assert_eq!(asset.size, None, "{} carries no size", asset.remote);
        }
        assert_eq!(
            ORT_MACOS_ARM64_SHA256,
            "d306d2bc768540766c7ed8a1e0ff05d2870c77a934ebeee4a7bafa1b732ef299"
        );
        assert_eq!(
            ORT_MACOS_X86_64_SHA256,
            "8c9c78de65ea3786f987c0d980e9c1b13a3a5fbc6b3e2965ba05b450e6e4c054"
        );
        assert_eq!(
            ORT_LINUX_X86_64_SHA256,
            "13ab8084954fa4a47c777880180b90810d6020f021441395712b48a75b74c68b"
        );
        assert_eq!(
            ORT_LINUX_AARCH64_SHA256,
            "648ffa64fbe027ae27139109410900cf776a030dec2dbbac51053318cc44c286"
        );
        assert_eq!(
            ORT_WINDOWS_X86_64_SHA256,
            "dec964ab1ee36cc9b0ae247d13b376627992fc57dec0454354017ab8fd84f1ea"
        );
        assert_eq!(
            ORT_LICENSE_SHA256,
            "2f07c72751aed99790b8a4869cf2311df85a860b22ded05fa22803587a48922c"
        );
        assert_eq!(
            ORT_NOTICES_SHA256,
            "e9e90971a8e75a9a8ac0c6412e29c1202d079998389915aa485f46c816c3b4cc"
        );
        // The notice and the two SDK licence texts are pinned to the tree
        // files' final bytes, beside the final Apache attribution.
        for (asset, sha256, size) in [
            (
                &RELEASE_ASSETS[9],
                "5eb1f80e9689fedb88c3e1f6c415c8341283e0e37ead0e12f0862e6191ac4b8b",
                3_471,
            ),
            (
                &RELEASE_ASSETS[11],
                "0d96a4ff68ad6d4b6f1f30f713b18d5184912ba8dd389f86aa7710db079abcb0",
                34_523,
            ),
            (
                &RELEASE_ASSETS[12],
                "ca54d9ab46d99e81d0dd2ea6ed112f3effa7f87cd3b65a876b1740c9efe599ed",
                10_026,
            ),
        ] {
            assert_eq!(asset.sha256, sha256, "{} is pinned", asset.remote);
            assert_eq!(asset.size, Some(size), "{} carries its size", asset.remote);
        }
        let apache = &RELEASE_ASSETS[10];
        assert_eq!(
            apache.sha256,
            "cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30"
        );
        assert_eq!(apache.size, Some(11_358));
        // Every platform library row resolves to a table entry.
        for library in ORT_LIBRARIES {
            assert!(
                RELEASE_ASSETS
                    .iter()
                    .any(|asset| asset.remote == library.remote),
                "{} has a table entry",
                library.remote
            );
        }
        // Every installed text resolves to a table entry too.
        for text in ROOT_TEXTS.iter().chain(ORT_TEXTS.iter()) {
            assert!(
                RELEASE_ASSETS.iter().any(|asset| asset.remote == *text),
                "{text} has a table entry"
            );
        }
    }

    #[test]
    fn release_table_carries_no_todo_pins() {
        // The final bytes are cut and pinned: the models, the runtime
        // libraries and texts, the notice and the licence texts. Every
        // asset resolves without an override.
        assert!(
            todo_assets(RELEASE_ASSETS).is_empty(),
            "still TODO: {}",
            todo_assets(RELEASE_ASSETS).join(", ")
        );
        for remote in [
            "heron_ep03.onnx",
            "tatr-v1.1-all.onnx",
            "onnxruntime-macos-arm64-1.23.2.dylib",
            "onnxruntime-macos-x86_64-1.23.2.dylib",
            "onnxruntime-linux-x86_64-1.23.2.so",
            "onnxruntime-linux-arm64-1.23.2.so",
            "onnxruntime-windows-x86_64-1.23.2.dll",
            "onnxruntime-LICENSE.txt",
            "onnxruntime-ThirdPartyNotices.txt",
            "MODELS-NOTICE.txt",
            "LICENSE-Apache-2.0.txt",
            "LICENSE",
            "LICENSE-COMMERCIAL",
        ] {
            assert!(
                !todo_assets(RELEASE_ASSETS).contains(&remote),
                "{remote} is pinned"
            );
        }
    }

    #[test]
    fn todo_assets_finds_only_unfilled_pins() {
        // The predicate itself, on a fake table: independent of what the
        // release has filled, so it holds after the pins land too.
        let hex = "9cccf3aa4cc7df7ed0333594fbd6bf2df121ccbbbfeb6f53571cd1fa3e275917";
        let table = fake_table(hex);
        assert_eq!(todo_assets(&table), vec!["todo.onnx"]);
        let filled = vec![ReleaseAsset {
            remote: "filled.onnx",
            sha256: hex,
            size: Some(3),
        }];
        assert!(todo_assets(&filled).is_empty());
    }

    #[test]
    fn a_todo_pin_refuses_naming_the_asset() {
        let hex = "9cccf3aa4cc7df7ed0333594fbd6bf2df121ccbbbfeb6f53571cd1fa3e275917";
        let table = fake_table(hex);
        let empty = HashMap::new();
        let Err(message) = resolve_pin(&table, "todo.onnx", &empty) else {
            panic!("a TODO pin must refuse");
        };
        assert!(message.contains("todo.onnx"), "{message}");
        assert!(message.contains("no sha256 is pinned"), "{message}");
        // The filled row resolves, with its size.
        let pin = resolve_pin(&table, "filled.onnx", &empty).expect("a filled pin resolves");
        assert_eq!(pin.hex, hex);
        assert_eq!(pin.size, Some(3));
    }

    #[test]
    fn a_test_override_fills_a_todo() {
        let hex = "9cccf3aa4cc7df7ed0333594fbd6bf2df121ccbbbfeb6f53571cd1fa3e275917";
        let table = fake_table(hex);
        let filler = sha256_hex(b"test bytes");
        let overrides = HashMap::from([("todo.onnx".to_string(), filler.clone())]);
        check_overrides(&table, &overrides).expect("a TODO fill vets clean");
        let pin = resolve_pin(&table, "todo.onnx", &overrides).expect("the TODO fills");
        assert_eq!(pin.hex, filler);
        assert_eq!(pin.size, None, "a filled TODO carries no size");
    }

    #[test]
    fn a_test_override_cannot_weaken_a_filled_pin() {
        let hex = "9cccf3aa4cc7df7ed0333594fbd6bf2df121ccbbbfeb6f53571cd1fa3e275917";
        let table = fake_table(hex);
        let other = sha256_hex(b"something else entirely");
        let overrides = HashMap::from([("filled.onnx".to_string(), other)]);
        let Err(message) = check_overrides(&table, &overrides) else {
            panic!("a weakened pin must refuse");
        };
        assert!(message.contains("filled.onnx"), "{message}");
        assert!(message.contains("must not weaken"), "{message}");
        // An equal digest is a no-op, not a weakening.
        let same = HashMap::from([("filled.onnx".to_string(), hex.to_string())]);
        check_overrides(&table, &same).expect("an equal digest vets clean");
    }

    #[test]
    fn a_test_override_for_an_unknown_asset_refuses() {
        let hex = "9cccf3aa4cc7df7ed0333594fbd6bf2df121ccbbbfeb6f53571cd1fa3e275917";
        let table = fake_table(hex);
        let overrides = HashMap::from([("no-such-asset".to_string(), hex.to_string())]);
        let Err(message) = check_overrides(&table, &overrides) else {
            panic!("an unknown asset must refuse");
        };
        assert!(message.contains("no-such-asset"), "{message}");
        // And a malformed digest refuses naming the asset.
        let bad = HashMap::from([("todo.onnx".to_string(), "not-a-digest".to_string())]);
        let Err(message) = check_overrides(&table, &bad) else {
            panic!("a malformed digest must refuse");
        };
        assert!(message.contains("todo.onnx"), "{message}");
    }

    #[test]
    fn the_base_override_accepts_https_and_file() {
        assert_eq!(
            resolve_base(None).expect("the default base resolves"),
            MODELS_RELEASE_BASE
        );
        assert_eq!(
            resolve_base(Some("https://mirror.example/models")).expect("https resolves"),
            "https://mirror.example/models/"
        );
        assert_eq!(
            resolve_base(Some("file:///tmp/mirror/")).expect("file resolves"),
            "file:///tmp/mirror/"
        );
        assert_eq!(
            resolve_base(Some("file:///tmp/mirror")).expect("the slash is added"),
            "file:///tmp/mirror/"
        );
        for base in [
            "http://example.invalid/models/",
            "ftp://example.invalid/",
            "tmp/mirror",
        ] {
            let Err((code, message)) = resolve_base(Some(base)) else {
                panic!("{base:?} must refuse");
            };
            assert_eq!(code, "MODEL_NOT_AVAILABLE");
            assert!(message.contains(MODELS_BASE_ENV), "{message}");
        }
    }

    #[test]
    fn test_pins_parse_as_asset_lines() {
        let parsed = parse_test_pins(
            "# a comment\n\ntodo.onnx=9CCCF3AA4CC7DF7ED0333594FBD6BF2DF121CCBBBFEB6F53571CD1FA3E275917\n",
        )
        .expect("comments and blanks skip");
        assert_eq!(
            parsed.get("todo.onnx").map(String::as_str),
            Some("9cccf3aa4cc7df7ed0333594fbd6bf2df121ccbbbfeb6f53571cd1fa3e275917"),
            "the digest lowercases"
        );
        for (text, line) in [
            ("no-equals-here\n", 1),
            ("todo.onnx=\n", 1),
            ("ok=ab\nbroken line\n", 2),
        ] {
            let Err(message) = parse_test_pins(text) else {
                panic!("{text:?} must refuse");
            };
            assert!(message.contains(&format!("line {line}")), "{message}");
        }
    }

    #[test]
    fn the_plan_installs_models_then_runtime_then_texts() {
        let dir = scratch_dir("plan");
        let config = pdfluent::model::RuntimeConfig::new().with_model_dir(dir.clone());
        let plan = build_plan(&[ModelPackage::Layout], &config).expect("layout plans");
        let remotes: Vec<&str> = plan.iter().map(|file| file.remote).collect();
        let library = library_for_this_platform().expect("this platform has a library");
        assert_eq!(
            remotes,
            vec![
                "heron_ep03.onnx",
                library.remote,
                "onnxruntime-LICENSE.txt",
                "onnxruntime-ThirdPartyNotices.txt",
                "MODELS-NOTICE.txt",
                "LICENSE-Apache-2.0.txt",
                "LICENSE",
                "LICENSE-COMMERCIAL",
            ]
        );
        // The destinations are what the loader resolves: the model through
        // its spec, the library through the shared resolution, the texts
        // beside them.
        assert_eq!(
            plan[0].dest,
            pdfluent::model::resolve_model_file(&pdfluent::model::LAYOUT_EP03, &config)
        );
        assert_eq!(plan[1].dest, pdfluent::model::resolve_ort_dylib(&config));
        let ort_dir = plan[1].dest.parent().expect("the library has a parent");
        assert_eq!(plan[2].dest, ort_dir.join("onnxruntime-LICENSE.txt"));
        assert_eq!(
            plan[3].dest,
            ort_dir.join("onnxruntime-ThirdPartyNotices.txt")
        );
        assert_eq!(plan[4].dest, dir.join("MODELS-NOTICE.txt"));
        assert_eq!(plan[5].dest, dir.join("LICENSE-Apache-2.0.txt"));
        assert_eq!(plan[6].dest, dir.join("LICENSE"));
        assert_eq!(plan[7].dest, dir.join("LICENSE-COMMERCIAL"));
        // Tables resolves through its own spec, into its own package dir.
        let tables = build_plan(&[ModelPackage::Tables], &config).expect("tables plans");
        assert_eq!(tables[0].remote, "tatr-v1.1-all.onnx");
        assert_eq!(
            tables[0].dest,
            pdfluent::model::resolve_model_file(&pdfluent::model::TABLES_TATR, &config)
        );
        assert!(
            tables[0].dest.to_string_lossy().contains("tatr-v1.1-all"),
            "the installed layout is the package dir the TATR stage resolves: {}",
            tables[0].dest.display()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn installing_both_packages_fetches_the_runtime_once() {
        let dir = scratch_dir("plan-both");
        let config = pdfluent::model::RuntimeConfig::new().with_model_dir(dir.clone());
        let library = library_for_this_platform().expect("this platform has a library");
        let plan =
            build_plan(&[ModelPackage::Layout, ModelPackage::Tables], &config).expect("both plan");
        let remotes: Vec<&str> = plan.iter().map(|file| file.remote).collect();
        assert_eq!(
            remotes,
            vec![
                "heron_ep03.onnx",
                "tatr-v1.1-all.onnx",
                library.remote,
                "onnxruntime-LICENSE.txt",
                "onnxruntime-ThirdPartyNotices.txt",
                "MODELS-NOTICE.txt",
                "LICENSE-Apache-2.0.txt",
                "LICENSE",
                "LICENSE-COMMERCIAL",
            ]
        );
        // A repeated package installs once.
        let plan = build_plan(&[ModelPackage::Layout, ModelPackage::Layout], &config)
            .expect("a repeat plans");
        assert_eq!(
            plan.iter()
                .filter(|file| file.remote == "heron_ep03.onnx")
                .count(),
            1
        );
        // Nothing requested, nothing planned.
        assert!(build_plan(&[], &config).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn installed_lines_match_the_release_contract() {
        // Exactly one line per model plus one for the runtime, word for word
        // what the owner fixed: the em dashes, the licences, the notices.
        let dir = scratch_dir("lines");
        let model = PlannedFile {
            remote: "heron_ep03.onnx",
            dest: dir.join("heron_ep03.onnx"),
            kind: PlannedKind::Model {
                package: "layout",
                license: "PDFluent SDK licence (AGPL-3.0 or commercial)",
                notice: "MODELS-NOTICE.txt",
            },
        };
        assert_eq!(
            installed_line(&model).as_deref(),
            Some(
                "installed heron_ep03.onnx (layout) — PDFluent SDK licence \
                 (AGPL-3.0 or commercial) — see MODELS-NOTICE.txt"
            )
        );
        let tables = PlannedFile {
            remote: "tatr-v1.1-all.onnx",
            dest: dir.join("tatr-v1.1-all.onnx"),
            kind: PlannedKind::Model {
                package: "tables",
                license: "MIT",
                notice: "MODELS-NOTICE.txt",
            },
        };
        assert_eq!(
            installed_line(&tables).as_deref(),
            Some("installed tatr-v1.1-all.onnx (tables) — MIT — see MODELS-NOTICE.txt")
        );
        let runtime = PlannedFile {
            remote: "onnxruntime-macos-arm64-1.23.2.dylib",
            dest: dir.join("libonnxruntime.dylib"),
            kind: PlannedKind::Runtime {
                platform: "macos-arm64",
            },
        };
        assert_eq!(
            installed_line(&runtime).as_deref(),
            Some("installed onnxruntime 1.23.2 (macos-arm64) — MIT — see onnxruntime-LICENSE.txt")
        );
        let text = PlannedFile {
            remote: "MODELS-NOTICE.txt",
            dest: dir.join("MODELS-NOTICE.txt"),
            kind: PlannedKind::Text,
        };
        assert_eq!(installed_line(&text), None, "texts print no line");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn package_licences_match_the_release_contract() {
        // What each install line claims: Heron under the SDK dual licence,
        // TATR MIT, both pointing at the notice; the runtime line stays MIT.
        let layout = package_spec(ModelPackage::Layout);
        assert_eq!(layout.name, "layout");
        assert_eq!(layout.model, "heron_ep03.onnx");
        assert_eq!(
            layout.license,
            "PDFluent SDK licence (AGPL-3.0 or commercial)"
        );
        assert_eq!(layout.notice, "MODELS-NOTICE.txt");
        let tables = package_spec(ModelPackage::Tables);
        assert_eq!(tables.name, "tables");
        assert_eq!(tables.model, "tatr-v1.1-all.onnx");
        assert_eq!(tables.license, "MIT");
        assert_eq!(tables.notice, "MODELS-NOTICE.txt");
        assert_eq!(ORT_LICENSE, "MIT");
        assert_eq!(ORT_NOTICE, "onnxruntime-LICENSE.txt");
    }

    #[test]
    fn install_json_carries_the_line_fields() {
        let dir = scratch_dir("install-json");
        let model = PlannedFile {
            remote: "heron_ep03.onnx",
            dest: dir.join("layout-heron-ep03").join("heron_ep03.onnx"),
            kind: PlannedKind::Model {
                package: "layout",
                license: "PDFluent SDK licence (AGPL-3.0 or commercial)",
                notice: "MODELS-NOTICE.txt",
            },
        };
        let runtime = PlannedFile {
            remote: "lib",
            dest: dir.join("lib"),
            kind: PlannedKind::Runtime {
                platform: "macos-arm64",
            },
        };
        let data = install_json(&[&model, &runtime], &["MODELS-NOTICE.txt"]);
        assert_eq!(data["installed"][0]["package"], "layout");
        assert_eq!(data["installed"][0]["file"], "heron_ep03.onnx");
        assert_eq!(
            data["installed"][0]["license"],
            "PDFluent SDK licence (AGPL-3.0 or commercial)"
        );
        assert_eq!(data["installed"][0]["notice"], "MODELS-NOTICE.txt");
        assert!(data["installed"][0]["path"]
            .as_str()
            .expect("a path")
            .contains("heron_ep03.onnx"));
        assert_eq!(data["installed"][1]["package"], "onnxruntime");
        assert_eq!(data["installed"][1]["version"], "1.23.2");
        assert_eq!(data["installed"][1]["platform"], "macos-arm64");
        assert_eq!(data["skipped"][0], "MODELS-NOTICE.txt");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn release_pins_required_fails_while_the_table_has_todos() {
        // The release workflow's self-check: silent unless the workflow opts
        // in with `PDFLUENT_RELEASE_PINS_REQUIRED=1`, and then a failure
        // while ANY entry is still a TODO. Run it opted in to see it fail
        // before the pins land, and pass after.
        if std::env::var(RELEASE_PINS_REQUIRED_ENV).as_deref() != Ok("1") {
            return;
        }
        assert!(
            todo_assets(RELEASE_ASSETS).is_empty(),
            "a release must pin every asset; still TODO: {}",
            todo_assets(RELEASE_ASSETS).join(", ")
        );
    }

    #[test]
    fn status_reads_missing_verified_and_mismatch() {
        let dir = scratch_dir("status");
        let missing = dir.join("missing.onnx");
        assert_eq!(state_of(&missing, &sha256_hex(b"x")), FileState::Missing);
        let body = b"present bytes".to_vec();
        let present = dir.join("present.onnx");
        std::fs::write(&present, &body).expect("the file writes");
        assert_eq!(state_of(&present, &sha256_hex(&body)), FileState::Verified);
        assert_eq!(
            state_of(&present, &sha256_hex(b"other")),
            FileState::Mismatch
        );
        assert_eq!(state_of(&present, TODO_PIN), FileState::Unpinned);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
