//! DRIFT GATE: every advertised capability names its CLI command, every
//! named command has a test, and every test marker proves its call.
//!
//! Reads `docs/CAPABILITY_REGISTER.md` (the generated register — the `CLI`
//! row of each capability), then requires, per capability:
//!
//! 1. the register names at least one `pdfluent <command>`;
//! 2. that command exists in `pdfluent --help`;
//! 3. some test file in this crate carries `// CLI covers: <command>`;
//! 4. that file invokes the command through the binary (this gate).
//!
//! A marker without a call is decoration: the file could carry it while
//! testing nothing. Requirement 4 parses each marked file's text and fails
//! with the file, the marker and the command.
//!
//! Accepted call shapes (all through `tests/common`):
//!
//! - `run(&["<cmd>", ...])`, `run_with_stdin`, `run_with_env`, `run_in`
//!   (qualified or not): the FIRST string literal of the argv slice is the
//!   command word. The slice may span lines; `//` and `/* */` comments
//!   inside it are skipped.
//! - `Command::new(common::bin())` / `common::alias_bin()` (qualified or
//!   not): the first `.arg("...")` after the constructor is the command
//!   word; later `.arg("...")` literals in the same statement follow in
//!   order.
//!
//! The marker's leading words name the command path: `merge` matches a first
//! argument `merge`; `xfa flatten` matches the first TWO. Remaining words
//! (`convert --to docx`) must appear as literals later in the same argv, in
//! order. A global flag first (`--json`, ...) does not count: the command
//! word must be argv[0]. Anything fancier — a format held in a variable, an
//! argv built by `.args(&[...])` — is invisible to this scan by design:
//! spell the proof call literally.
//!
//! A failure names the capability, or the file, the marker and the command.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

mod common;

use std::collections::BTreeMap;
use std::path::PathBuf;

/// Capability -> CLI command names from the register's `CLI` rows.
fn register_cli() -> BTreeMap<String, Vec<String>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/CAPABILITY_REGISTER.md");
    let text = std::fs::read_to_string(&path).expect("register readable from the crate");
    let mut map: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut in_advertised = false;
    let mut current: Option<String> = None;
    for line in text.lines() {
        if line.starts_with("## ") {
            in_advertised = line == "## Advertised capabilities";
            current = None;
            continue;
        }
        if !in_advertised {
            continue;
        }
        if let Some(name) = line.strip_prefix("### ") {
            let capability = name.split(" — ").next().unwrap_or(name).trim().to_string();
            current = Some(capability.clone());
            map.insert(capability, Vec::new());
            continue;
        }
        if line.starts_with("| **CLI** |") {
            let Some(capability) = current.clone() else {
                panic!("CLI row outside a capability section");
            };
            let mut commands = Vec::new();
            let mut rest = line;
            while let Some(start) = rest.find("`pdfluent ") {
                let span = &rest[start + 1..];
                let end = span.find('`').expect("closing backtick");
                commands.push(span["pdfluent ".len()..end].to_string());
                rest = &span[end + 1..];
            }
            map.insert(capability, commands);
        }
    }
    assert!(!map.is_empty(), "register has advertised capabilities");
    map
}

/// Every test file in this crate (name + text), minus this gate itself.
fn test_files() -> Vec<(String, String)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("tests dir")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "rs"))
        .collect();
    files.sort();
    files
        .into_iter()
        .filter(|f| f.file_name().is_none_or(|n| n != "cli_drift.rs"))
        .map(|f| {
            let name = f
                .file_name()
                .expect("file name")
                .to_string_lossy()
                .to_string();
            let text = std::fs::read_to_string(&f).expect("test file readable");
            (name, text)
        })
        .collect()
}

/// All `// CLI covers: <command>` markers in this crate's tests.
fn test_markers() -> Vec<String> {
    test_files()
        .iter()
        .flat_map(|(_, text)| markers_in(text))
        .collect()
}

/// The markers one file carries.
fn markers_in(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| line.trim().strip_prefix("// CLI covers:"))
        .map(|marker| marker.trim().to_string())
        .collect()
}

/// Check one test file: every marker it carries must prove its call in that
/// same file. Returns one error per unproven marker (empty means covered).
fn check_file_markers(file_name: &str, text: &str) -> Vec<String> {
    let mut errors = Vec::new();
    for marker in markers_in(text) {
        if !file_invokes(text, &marker) {
            errors.push(format!(
                "`// CLI covers: {marker}` in {file_name}, but no call to `pdfluent {marker}` in that file"
            ));
        }
    }
    errors
}

/// True when `text` invokes `marker` through the binary in an accepted shape.
fn file_invokes(text: &str, marker: &str) -> bool {
    let words: Vec<&str> = marker.split_whitespace().collect();
    let path_len = words.iter().take_while(|w| !w.starts_with('-')).count();
    if path_len == 0 {
        return false;
    }
    let (path, rest) = words.split_at(path_len);
    argv_sequences(text).iter().any(|argv| {
        argv.len() >= path_len
            && argv[..path_len]
                .iter()
                .map(String::as_str)
                .eq(path.iter().copied())
            && contains_in_order(&argv[path_len..], rest)
    })
}

/// True when every needle appears in the haystack, in order (not adjacently).
fn contains_in_order(haystack: &[String], needles: &[&str]) -> bool {
    let mut pos = 0;
    for needle in needles {
        match haystack[pos..].iter().position(|h| h == needle) {
            Some(found) => pos += found + 1,
            None => return false,
        }
    }
    true
}

/// Every argv literal sequence in the file: the `&[...]` slice after each
/// runner call, plus each `.arg("...")` chain after a `bin()` constructor.
fn argv_sequences(text: &str) -> Vec<Vec<String>> {
    let mut seqs = Vec::new();
    for name in ["run", "run_with_stdin", "run_with_env", "run_in"] {
        let open = format!("{name}(");
        let mut start = 0;
        while let Some(found) = text[start..].find(open.as_str()) {
            let idx = start + found;
            start = idx + 1;
            if idx > 0 && is_ident(text.as_bytes()[idx - 1]) {
                continue;
            }
            let after = &text[idx + open.len()..];
            if let Some(slice) = after.find("&[") {
                seqs.push(slice_literals(after, slice));
            }
        }
    }
    seqs.extend(command_sequences(text));
    seqs
}

/// `.arg("...")` chains after `Command::new(...bin())` / `...alias_bin()`.
fn command_sequences(text: &str) -> Vec<Vec<String>> {
    const NEW: &str = "Command::new(";
    let mut seqs = Vec::new();
    let mut start = 0;
    while let Some(found) = text[start..].find(NEW) {
        let idx = start + found;
        start = idx + 1;
        if idx > 0 && is_ident(text.as_bytes()[idx - 1]) {
            continue;
        }
        let rest = &text[idx + NEW.len()..];
        let Some(first_arg) = rest.find(".arg(\"") else {
            continue;
        };
        // The constructor must build on bin()/alias_bin(), in the same
        // statement as the first .arg (no `;` between them).
        let head = &rest[..first_arg];
        if head.contains(';') || !head_has_bin(head) {
            continue;
        }
        let end = statement_end(rest, first_arg);
        let mut chain = Vec::new();
        let mut pos = first_arg;
        while let Some(rel) = rest[pos..end].find(".arg(\"") {
            let quote = pos + rel + ".arg(".len();
            let (lit, next) = string_literal(rest, quote);
            chain.push(lit);
            pos = next;
        }
        seqs.push(chain);
    }
    seqs
}

/// True when `head` names `bin()` or `alias_bin()` (qualified or not).
fn head_has_bin(head: &str) -> bool {
    for name in ["alias_bin()", "bin()"] {
        let mut start = 0;
        while let Some(found) = head[start..].find(name) {
            let idx = start + found;
            start = idx + 1;
            if idx == 0 || !is_ident(head.as_bytes()[idx - 1]) {
                return true;
            }
        }
    }
    false
}

/// String literals inside the slice at `open` (the index of `&`), until its
/// closing `]`. Skips `//` and `/* */` comments; `\"` cannot end a literal.
fn slice_literals(text: &str, open: usize) -> Vec<String> {
    let bytes = text.as_bytes();
    let mut lits = Vec::new();
    let mut i = open + 2; // past `&[`
    let mut depth = 1usize;
    while i < bytes.len() && depth > 0 {
        match bytes[i] {
            b'"' => {
                let (lit, next) = string_literal(text, i);
                lits.push(lit);
                i = next;
            }
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                i = (i + 2).min(bytes.len());
            }
            b'[' => {
                depth += 1;
                i += 1;
            }
            b']' => {
                depth -= 1;
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }
    lits
}

/// Parse `"..."` at `i` (the opening quote). Returns the value and the first
/// index past the closing quote; a backslash escapes the next char.
fn string_literal(text: &str, i: usize) -> (String, usize) {
    let bytes = text.as_bytes();
    let mut j = i + 1;
    while j < bytes.len() {
        if bytes[j] == b'"' {
            let raw = &text[i + 1..j];
            let value = raw.replace("\\\"", "\"").replace("\\\\", "\\");
            return (value, j + 1);
        }
        if bytes[j] == b'\\' {
            j += 1;
        }
        j += 1;
    }
    (text[i + 1..].to_string(), bytes.len())
}

/// Index of the `;` ending the statement at `from` (outside strings), or the
/// end of the text when the statement never ends (a fixture fragment).
fn statement_end(text: &str, from: usize) -> usize {
    let bytes = text.as_bytes();
    let mut i = from;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                let (_, next) = string_literal(text, i);
                i = next;
            }
            b';' => return i,
            _ => i += 1,
        }
    }
    bytes.len()
}

fn is_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

#[test]
fn every_capability_names_a_tested_cli_command() {
    let map = register_cli();
    assert_eq!(map.len(), 14, "capabilities in the register: {}", map.len());

    let help = {
        let out = common::run(&["--help"]);
        assert!(out.status.success());
        String::from_utf8_lossy(&out.stdout).to_string()
    };
    let markers = test_markers();

    for (capability, commands) in &map {
        assert!(
            !commands.is_empty(),
            "capability `{capability}` names no CLI command in docs/CAPABILITY_REGISTER.md (add it to CLI_COMMANDS in scripts/ci/feature_promises.py)"
        );
        for command in commands {
            let base = command.split_whitespace().next().unwrap_or(command);
            assert!(
                help.contains(&format!("  {base}")),
                "capability `{capability}` names `pdfluent {command}`, but `{base}` is not in `pdfluent --help`"
            );
            assert!(
                markers.iter().any(|m| m == command),
                "capability `{capability}` names `pdfluent {command}`, but no test carries `// CLI covers: {command}`"
            );
        }
    }

    // Markers must point somewhere real, or they rot into decoration.
    let named: Vec<&String> = map.values().flatten().collect();
    for marker in &markers {
        assert!(
            named.iter().any(|n| n.as_str() == marker),
            "stray `// CLI covers: {marker}`: the register names no such command"
        );
    }

    // And each marked file must prove its marker with a call of its own.
    let mut unproven = Vec::new();
    for (name, text) in test_files() {
        unproven.extend(check_file_markers(&name, &text));
    }
    assert!(
        unproven.is_empty(),
        "markers without a call:\n{}",
        unproven.join("\n")
    );
}

#[test]
fn a_marker_without_a_call_fails_the_drift_gate() {
    let text =
        "// CLI covers: merge\n\n#[test]\nfn something_else() {\n    assert_eq!(1 + 1, 2);\n}\n";
    let errors = check_file_markers("cmd_manip.rs", text);
    assert_eq!(errors.len(), 1);
    assert_eq!(
        errors[0],
        "`// CLI covers: merge` in cmd_manip.rs, but no call to `pdfluent merge` in that file"
    );
}

#[test]
fn a_marker_with_a_call_passes() {
    let text = "// CLI covers: merge\n\n#[test]\nfn merge_combines() {\n    let out = run(&[\"merge\", \"a.pdf\", \"-o\", \"out.pdf\"]);\n    assert!(out.status.success());\n}\n";
    assert!(check_file_markers("cmd_manip.rs", text).is_empty());
}

#[test]
fn a_nested_command_needs_both_words() {
    let partial =
        "// CLI covers: xfa flatten\n\n#[test]\nfn x() {\n    let out = run(&[\"xfa\"]);\n}\n";
    let errors = check_file_markers("cmd_xfa.rs", partial);
    assert_eq!(errors.len(), 1);
    assert_eq!(
        errors[0],
        "`// CLI covers: xfa flatten` in cmd_xfa.rs, but no call to `pdfluent xfa flatten` in that file"
    );
    let full = "// CLI covers: xfa flatten\n\n#[test]\nfn x() {\n    let out = run(&[\"xfa\", \"flatten\", \"in.pdf\"]);\n}\n";
    assert!(check_file_markers("cmd_xfa.rs", full).is_empty());
}

#[test]
fn a_flag_marker_needs_the_flag_in_the_same_call() {
    // `convert --to docx` is proven by one argv holding all three words: the
    // command first, the flag and its value later in the same call.
    let good = "// CLI covers: convert --to docx\n\n#[test]\nfn x() {\n    let out = run(&[\n        \"convert\",\n        \"in.pdf\",\n        \"--to\",\n        \"docx\",\n    ]);\n}\n";
    assert!(check_file_markers("cmd_process.rs", good).is_empty());
    let missing_value = "// CLI covers: convert --to docx\n\n#[test]\nfn x() {\n    let out = run(&[\"convert\", \"in.pdf\"]);\n}\n";
    assert_eq!(check_file_markers("cmd_process.rs", missing_value).len(), 1);
}

#[test]
fn a_command_builder_call_counts_as_proof() {
    let text = "// CLI covers: merge\n\n#[test]\nfn x() {\n    let out = std::process::Command::new(common::bin())\n        .arg(\"merge\")\n        .arg(\"a.pdf\")\n        .output()\n        .expect(\"spawn\");\n}\n";
    assert!(check_file_markers("cmd_manip.rs", text).is_empty());
    let bare = "// CLI covers: merge\n\n#[test]\nfn x() {\n    let out = std::process::Command::new(common::bin())\n        .output()\n        .expect(\"spawn\");\n}\n";
    assert_eq!(check_file_markers("cmd_manip.rs", bare).len(), 1);
}
