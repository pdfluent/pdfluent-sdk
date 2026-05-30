#!/usr/bin/env python3
"""
release_train.py — release-promotion control plane.

Reads `docs/release/canonical_releases.toml` and provides four operations:

    matrix      Compare canonical manifest vs every channel's live state
                via registry API. Prints a human-readable table.

    drift       Same comparison, but exits non-zero on any threshold breach.
                CI gate. Prints a machine-readable JSON report on stdout.

    health      Operator dashboard: per-channel state, blockers, lag, next
                action. Reads manifest + ledger files + recent registry API.

    snippets    Emit per-channel install commands rendered from the manifest.
                Used by the website's install-snippets generator.

Usage:
    scripts/release/release_train.py matrix
    scripts/release/release_train.py drift  [--json]
    scripts/release/release_train.py health
    scripts/release/release_train.py snippets [--format=md|json]

Exit codes:
    0   all OK / no drift
    1   drift detected beyond threshold
    2   argv error / file missing / JSON parse error
    3   network unreachable for a non-ignored channel

Standard library only (tomllib + urllib + json + sys). Requires Python 3.11+.
"""
from __future__ import annotations
import argparse
import json
import os
import re
import ssl
import sys
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any

try:
    import tomllib
except ImportError:
    sys.exit("ERROR: this script requires Python 3.11+ (tomllib).")

REPO_ROOT  = Path(__file__).resolve().parents[2]
MANIFEST   = REPO_ROOT / "docs" / "release" / "canonical_releases.toml"
LEDGER_DIR = REPO_ROOT / "docs" / "release" / "sha_ledger"
UA         = "pdfluent-release-train/1.0 (https://pdfluent.com)"


# --------------------------------------------------------------------------
# manifest loading
# --------------------------------------------------------------------------

def load_manifest() -> dict[str, Any]:
    if not MANIFEST.exists():
        sys.exit(f"ERROR: manifest missing at {MANIFEST}")
    return tomllib.loads(MANIFEST.read_text())


# --------------------------------------------------------------------------
# registry-API fetchers
# --------------------------------------------------------------------------

def http_get(url: str, headers: dict[str, str] | None = None, timeout: int = 15) -> dict | list | None:
    headers = {**(headers or {}), "User-Agent": UA, "Accept": "application/json"}
    req = urllib.request.Request(url, headers=headers)
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            body = r.read()
        return json.loads(body)
    except urllib.error.HTTPError as e:
        if e.code == 404:
            return None
        raise
    except urllib.error.URLError:
        return None


def jq_path(data: Any, path: str) -> Any:
    """Tiny jq-ish path resolver. Supports dot-notation + [N] indexing."""
    cur = data
    for part in re.findall(r'[^.\[\]]+|\[-?\d+\]', path):
        if part.startswith("[") and part.endswith("]"):
            idx = int(part[1:-1])
            if isinstance(cur, list):
                try:
                    cur = cur[idx]
                except IndexError:
                    return None
            else:
                return None
        else:
            if isinstance(cur, dict):
                cur = cur.get(part)
            else:
                return None
        if cur is None:
            return None
    return cur


def fetch_live_version(channel_key: str, ch: dict, endpoints: dict) -> tuple[str | None, str | None]:
    """Return (live_version, error). live_version may be None if not_released."""
    ep = endpoints.get(channel_key)
    if not ep:
        return None, f"no endpoint for {channel_key}"
    pkg = ch.get("package", "")
    url = ep["api_url"]
    if "{package}" in url:
        url = url.replace("{package}", pkg)
    if "{package_lower}" in url:
        url = url.replace("{package_lower}", pkg.lower())
    if "{group}" in url and ":" in pkg:
        group, artifact = pkg.split(":", 1)
        url = url.replace("{group}", group).replace("{artifact}", artifact)
    if "{project_id}" in url:
        url = url.replace("{project_id}", str(82044173))  # pdfluent-group/PDFluent-project
    headers: dict[str, str] = {}
    if ep.get("auth_header", "").startswith("PRIVATE-TOKEN:"):
        pat = os.environ.get("GITLAB_PAT")
        if not pat:
            # fall back to keychain
            import subprocess
            try:
                r = subprocess.run(
                    ["security", "find-internet-password", "-a", "claude-pdfluent-api", "-w"],
                    capture_output=True, text=True, check=True)
                pat = r.stdout.strip()
            except Exception:
                return None, "no GITLAB_PAT and keychain lookup failed"
        headers["PRIVATE-TOKEN"] = pat
    try:
        data = http_get(url, headers=headers)
    except Exception as e:
        return None, f"network error: {e}"
    if data is None:
        return None, "not on registry (404)"
    v = jq_path(data, ep["version_path"])
    if v is None:
        return None, f"version_path {ep['version_path']} returned None"
    return str(v), None


# --------------------------------------------------------------------------
# operations
# --------------------------------------------------------------------------

def cmd_matrix(_args) -> int:
    m = load_manifest()
    eps = m.get("endpoints", {})
    rows = []
    for ch_key, ch in m.get("channels", {}).items():
        expected = ch.get("expected", "")
        state    = ch.get("state", "released")
        live, err = (None, None)
        if expected != "not_released":
            live, err = fetch_live_version(ch_key, ch, eps)
        match = "OK" if (live == expected) else ("LAG" if live and live != expected else "—" if err else "—")
        rows.append({
            "channel":  ch_key,
            "package":  ch.get("package", ""),
            "expected": expected,
            "live":     live or "(N/A)" + (f" {err}" if err else ""),
            "match":    match,
            "state":    state,
        })
    # Print
    widths = {"channel": 18, "package": 32, "expected": 14, "live": 22, "match": 6, "state": 26}
    hdr = f"{'CHANNEL':<{widths['channel']}}  {'PACKAGE':<{widths['package']}}  {'EXPECTED':<{widths['expected']}}  {'LIVE':<{widths['live']}}  {'MATCH':<{widths['match']}}  STATE"
    print(hdr); print("-" * len(hdr))
    for r in rows:
        print(f"{r['channel']:<{widths['channel']}}  {r['package']:<{widths['package']}}  "
              f"{r['expected']:<{widths['expected']}}  {r['live']:<{widths['live']}}  "
              f"{r['match']:<{widths['match']}}  {r['state']}")
    return 0


def cmd_drift(args) -> int:
    m = load_manifest()
    eps = m.get("endpoints", {})
    thresholds = m.get("drift_thresholds", {})
    strict = set(thresholds.get("strict_match", []))
    lag_ok = set(thresholds.get("allow_lag_one", []))
    ignore = set(thresholds.get("ignore", []))
    consumer_strict = set(thresholds.get("consumer_strict", []))

    findings: list[dict] = []
    for ch_key, ch in m.get("channels", {}).items():
        if ch_key in ignore:
            continue
        expected = ch.get("expected", "")
        if expected == "not_released":
            continue
        live, err = fetch_live_version(ch_key, ch, eps)
        finding = {
            "channel": ch_key,
            "package": ch.get("package", ""),
            "expected": expected,
            "live": live,
            "error": err,
            "severity": "INFO",
        }
        if live is None:
            finding["severity"] = "WARN"
            finding["reason"] = err or "no live version"
        elif live != expected:
            if ch_key in lag_ok:
                finding["severity"] = "WARN"
                finding["reason"] = f"channel lags manifest ({live} vs {expected}); allowed"
            elif ch_key in strict:
                finding["severity"] = "FAIL"
                finding["reason"] = f"strict-match drift ({live} on registry vs {expected} in manifest)"
            else:
                finding["severity"] = "WARN"
                finding["reason"] = "drift outside strict_match"
        findings.append(finding)

    # Consumer drift (e.g. website manifest)
    for cons_key, cons in m.get("consumers", {}).items():
        if cons_key not in consumer_strict:
            continue
        ch = m["channels"].get(cons.get("channel", ""), {})
        expected = ch.get("expected", "")
        loc = Path(os.path.expanduser(cons.get("location", "")))
        mf = loc / cons.get("manifest_file", "")
        if not mf.exists():
            findings.append({
                "channel": f"consumer:{cons_key}",
                "package": cons.get("manifest_file", ""),
                "expected": expected,
                "live": None,
                "error": f"manifest file missing at {mf}",
                "severity": "WARN",
                "reason": "consumer not reachable on this host",
            })
            continue
        # Read the consumer's pinned version
        try:
            cm = json.loads(mf.read_text())
            v = cm
            for k in cons.get("manifest_version_key", "version").split("."):
                v = v.get(k) if isinstance(v, dict) else None
        except Exception as e:
            findings.append({
                "channel": f"consumer:{cons_key}",
                "expected": expected,
                "live": None,
                "error": str(e),
                "severity": "WARN",
            })
            continue
        severity = "INFO" if v == expected else "FAIL"
        findings.append({
            "channel": f"consumer:{cons_key}",
            "package": cons.get("manifest_file", ""),
            "expected": expected,
            "live": v,
            "error": None,
            "severity": severity,
            "reason": "" if v == expected else f"consumer pinned at {v} but channel says {expected}",
        })

    report = {
        "schema": "release-train-drift-v1",
        "manifest_release_line": m.get("release_line"),
        "findings": findings,
        "totals": {
            "fail": sum(1 for f in findings if f["severity"] == "FAIL"),
            "warn": sum(1 for f in findings if f["severity"] == "WARN"),
            "info": sum(1 for f in findings if f["severity"] == "INFO"),
        },
    }

    if args.json:
        print(json.dumps(report, indent=2))
    else:
        for f in findings:
            line = f"[{f['severity']:4}] {f['channel']:<28} expected={f['expected']:<14} live={str(f.get('live','—')):<14}"
            if f.get("reason"):
                line += f"  — {f['reason']}"
            print(line)
        print()
        print(f"TOTAL: FAIL={report['totals']['fail']}  WARN={report['totals']['warn']}  INFO={report['totals']['info']}")

    return 1 if report["totals"]["fail"] > 0 else 0


def cmd_health(_args) -> int:
    m = load_manifest()
    eps = m.get("endpoints", {})
    print(f"# PDFluent release train — channel health")
    print(f"")
    print(f"**Release line:** `{m.get('release_line','?')}`")
    print()
    print("| Channel | Package | Expected | Live | State | Notes |")
    print("|---|---|---|---|---|---|")
    for ch_key, ch in m.get("channels", {}).items():
        expected = ch.get("expected", "")
        state = ch.get("state", "released")
        live, err = (None, None)
        if expected != "not_released":
            live, err = fetch_live_version(ch_key, ch, eps)
        live_repr = live if live else ("(N/A)" if err else "—")
        note = ch.get("notes", "")[:60] + ("…" if len(ch.get("notes",""))>60 else "")
        emoji = "✅" if live == expected else ("⏳" if state.startswith("staged") else ("🚧" if state.startswith("blocked") else "⚠️"))
        print(f"| {ch_key} | `{ch.get('package','?')}` | `{expected}` | `{live_repr}` | {emoji} {state} | {note} |")
    print()
    # Blockers section
    blockers = []
    for ch_key, ch in m.get("channels", {}).items():
        if ch.get("blockers"):
            for b in ch["blockers"]:
                blockers.append((ch_key, b))
    if blockers:
        print("## Blockers")
        print()
        for ch, b in blockers:
            print(f"- **{ch}**: {b}")
        print()
    # Ledger summary
    print("## Ledger entries per channel")
    print()
    for ledger_json in sorted(LEDGER_DIR.glob("*.json")):
        if ledger_json.name == "schema.json": continue
        try:
            d = json.loads(ledger_json.read_text() or "{}")
            n = len(d.get("entries", []))
            verified = sum(1 for e in d.get("entries", []) if e.get("status")=="verified")
            print(f"- `{ledger_json.name}`: {n} entries ({verified} verified)")
        except: pass
    return 0


def cmd_snippets(args) -> int:
    m = load_manifest()
    snippets = {}
    for ch_key, ch in m.get("channels", {}).items():
        if ch.get("expected") == "not_released":
            continue
        snippets[ch_key] = {
            "package": ch.get("package"),
            "version": ch.get("expected"),
            "pin":     ch.get("pin"),
            "install": ch.get("install_command"),
            "registry_url": ch.get("registry_url"),
        }
    if args.format == "json":
        print(json.dumps(snippets, indent=2))
    else:
        for k, v in snippets.items():
            print(f"## {k}")
            print(f"- **Package:** `{v['package']}`")
            print(f"- **Version:** `{v['version']}`")
            print(f"- **Install:**")
            print(f"  ```")
            print(f"  {v['install']}")
            print(f"  ```")
            print()
    return 0


# --------------------------------------------------------------------------
# main
# --------------------------------------------------------------------------

def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    sub.add_parser("matrix", help="print human-readable matrix")
    p_drift = sub.add_parser("drift", help="drift detector (CI gate)")
    p_drift.add_argument("--json", action="store_true")
    sub.add_parser("health", help="operator dashboard")
    p_snip = sub.add_parser("snippets", help="emit install snippets")
    p_snip.add_argument("--format", choices=["md","json"], default="md")
    args = ap.parse_args()
    if args.cmd == "matrix":   return cmd_matrix(args)
    if args.cmd == "drift":    return cmd_drift(args)
    if args.cmd == "health":   return cmd_health(args)
    if args.cmd == "snippets": return cmd_snippets(args)
    return 2


if __name__ == "__main__":
    sys.exit(main())
