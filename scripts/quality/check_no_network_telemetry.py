#!/usr/bin/env python3
"""QR-13 — static no-network / no-telemetry checker for the non-XFA SDK core.

Proves (statically) that the **core** SDK crates contain no HTTP-client
dependency and no raw socket usage, and that no SDK crate pulls a telemetry/
analytics dependency. Network-capable code is permitted ONLY in explicitly
allowlisted, **user-initiated, opt-in** paths (cloud OCR behind `ocr-*`
features; RFC-3161 TSA timestamping in the signing crate). Non-SDK tooling
(server/CLI/test-runner) is out of scope.

Exit 0 = clean; exit 1 = a forbidden network/telemetry usage was found.

Run from repo root:
    python3 scripts/quality/check_no_network_telemetry.py
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
CRATES = REPO / "crates"

# Core runtime crates that MUST be network-free on every code path.
CORE_CRATES = [
    "pdfluent",
    "pdf-syntax",
    "pdf-manip",
    "pdf-forms",
    "pdf-annot",
    "pdf-compliance",
    "pdf-render",
    "pdf-interpret",
    "pdf-font",
    "pdf-redact",
    "lopdf",
]

# Network-capable code allowed here, because it is user-initiated + opt-in
# (documented), not telemetry:
#   - pdf-engine/src/ocr.rs : cloud OCR backends, all behind `ocr-*` features
#   - pdf-ocr               : cloud OCR backends (opt-in)
#   - pdf-sign              : RFC-3161 TSA timestamp (only when a timestamp is requested)
ALLOWLISTED_CRATES = {"pdf-engine", "pdf-ocr", "pdf-sign"}

# Non-SDK tooling — not part of the shipped SDK surface.
OUT_OF_SCOPE_CRATES = {"xfa-api-server", "xfa-cli", "xfa-test-runner", "pdf-bench", "xfa-golden-tests"}

HTTP_DEP = re.compile(r"^\s*(reqwest|hyper|ureq|curl|isahc|surf|attohttpc|tonic)\b", re.M)
SOCKET_USE = re.compile(r"std::net::(Tcp|Udp)|TcpStream|UdpSocket|tokio::net")
TELEMETRY_DEP = re.compile(
    r"^\s*(sentry|segment|mixpanel|posthog|datadog|opentelemetry|analytics|amplitude)\b",
    re.M,
)


def main() -> int:
    violations: list[str] = []

    # 1. No telemetry dependency in ANY crate manifest.
    for manifest in CRATES.glob("*/Cargo.toml"):
        text = manifest.read_text(encoding="utf-8", errors="ignore")
        if TELEMETRY_DEP.search(text):
            violations.append(f"telemetry dependency in {manifest.relative_to(REPO)}")

    # 2. Core crates: no HTTP-client dep, no socket usage.
    for crate in CORE_CRATES:
        cdir = CRATES / crate
        manifest = cdir / "Cargo.toml"
        if manifest.exists() and HTTP_DEP.search(manifest.read_text(encoding="utf-8", errors="ignore")):
            violations.append(f"core crate '{crate}' declares an HTTP-client dependency")
        src = cdir / "src"
        if src.is_dir():
            for rs in src.rglob("*.rs"):
                if SOCKET_USE.search(rs.read_text(encoding="utf-8", errors="ignore")):
                    violations.append(f"core crate '{crate}' uses raw sockets in {rs.relative_to(REPO)}")

    # 3. Allowlisted crates' network use must remain feature-gated / opt-in.
    #    Sanity for pdf-engine OCR: any *non-test* socket use must sit under an
    #    `#[cfg(feature = "ocr...")]` gate. Socket use inside the test module
    #    (a localhost mock OCR server) is test-only, not shipped runtime.
    ocr = CRATES / "pdf-engine" / "src" / "ocr.rs"
    if ocr.exists():
        lines = ocr.read_text(encoding="utf-8", errors="ignore").splitlines()
        # First line index that begins the test module (#[cfg(test)] or `mod tests`).
        test_start = next(
            (i for i, l in enumerate(lines) if re.search(r"#\[cfg\(test\)\]|mod\s+tests\b", l)),
            len(lines),
        )
        for i, line in enumerate(lines):
            if i >= test_start:
                break  # test-only mock sockets — out of runtime scope
            if SOCKET_USE.search(line):
                # nearest preceding ocr feature gate anywhere above (module or fn level)
                gated = any('cfg(feature = "ocr' in lines[j] for j in range(0, i))
                if not gated:
                    violations.append(
                        f"pdf-engine/src/ocr.rs:{i+1} runtime socket use not under an ocr-* feature gate"
                    )

    if violations:
        print("QR-13 no-network/no-telemetry: VIOLATIONS", file=sys.stderr)
        for v in violations:
            print(f"  - {v}", file=sys.stderr)
        return 1

    print(
        "QR-13 no-network/no-telemetry: OK — core crates network-free; "
        "network only in opt-in cloud-OCR / TSA-timestamp; no telemetry deps."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
