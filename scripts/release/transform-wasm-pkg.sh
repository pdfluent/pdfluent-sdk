#!/usr/bin/env bash
# Rewrite the wasm-pack-emitted package.json so it carries the npm
# release identity for @pdfluent/xfa-wasm.
#
# wasm-pack copies fields from Cargo.toml verbatim. The crate is called
# `xfa-wasm` (no `@scope/` allowed in Rust crate names) and the workspace
# repository field is on the SDK monorepo. None of those are correct for
# the published npm package, so this script rewrites them after wasm-pack.
#
# Usage:
#   ./scripts/release/transform-wasm-pkg.sh <pkg-dir> <target-version>
#
# Exits non-zero if any required field is missing or wrong after rewrite.

set -euo pipefail

PKG_DIR="${1:?usage: $0 <pkg-dir> <target-version>}"
VERSION="${2:?usage: $0 <pkg-dir> <target-version>}"

PKG_JSON="$PKG_DIR/package.json"

if [[ ! -f "$PKG_JSON" ]]; then
    echo "error: $PKG_JSON not found" >&2
    exit 2
fi

python3 - "$PKG_JSON" "$VERSION" <<'PY'
import json, sys, pathlib

pkg_path = pathlib.Path(sys.argv[1])
version  = sys.argv[2]

d = json.loads(pkg_path.read_text(encoding="utf-8"))

# Identity
d["name"]    = "@pdfluent/xfa-wasm"
d["version"] = version
d["license"] = "SEE LICENSE IN LICENSE"
d["homepage"] = "https://pdfluent.com"
d["description"] = "Enterprise PDF SDK — XFA / forms / rendering — pure-Rust core via WASM."
d["author"] = "Innovation Trigger BV <team@pdfluent.com>"

# Strip GitHub or any external repo reference
d.pop("repository", None)
d.pop("bugs", None)

# Cleaner keyword set
d["keywords"] = ["pdf", "xfa", "wasm", "pdfluent", "forms", "render"]

pkg_path.write_text(json.dumps(d, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
print(json.dumps({k: d.get(k) for k in ("name", "version", "license", "homepage", "author")}, indent=2))
PY

# Verify expected fields are present and absent
ok=1
required='"@pdfluent/xfa-wasm" '"$VERSION"' SEE LICENSE IN LICENSE pdfluent.com'
for s in "@pdfluent/xfa-wasm" "$VERSION" "SEE LICENSE IN LICENSE" "https://pdfluent.com"; do
    if ! grep -q -F "$s" "$PKG_JSON"; then
        echo "  MISSING: $s" >&2
        ok=0
    fi
done
for forbidden in "github.com" "/Users/" "/home/"; do
    if grep -q -F "$forbidden" "$PKG_JSON"; then
        echo "  FORBIDDEN (must not appear): $forbidden" >&2
        ok=0
    fi
done
# LICENSE file must be the PDFluent Commercial License
LIC="$PKG_DIR/LICENSE"
if ! grep -q "PDFluent Commercial License" "$LIC" 2>/dev/null; then
    echo "  LICENSE missing or not PDFluent Commercial License: $LIC" >&2
    ok=0
fi

if [[ $ok -ne 1 ]]; then
    echo "transform-wasm-pkg.sh: audit FAILED" >&2
    exit 3
fi

echo "transform-wasm-pkg.sh: audit PASS"
