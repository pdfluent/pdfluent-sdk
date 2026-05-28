#!/usr/bin/env bash
# Launch day: publish alle packages
# Voer uit NADAT: npm login, cargo login, PyPI token, Maven credentials klaarstaan
set -e

echo "=== 1/4 crates.io ==="
cargo publish -p xfa-wasm --dry-run  # verwijder --dry-run op launch day
cargo publish -p pdf-engine --dry-run

echo "=== 2/4 npm ==="
cd crates/pdf-node
npm publish --access public --dry-run  # verwijder --dry-run op launch day
cd ../..

echo "=== 3/4 PyPI ==="
cd crates/pdf-python
maturin publish --dry-run  # verwijder --dry-run op launch day
cd ../..

echo "=== 4/4 Maven ==="
# Canonical Maven channel = bindings/java (com.pdfluent:pdfluent, commercial).
# The legacy crates/pdf-java (xfa-pdf, MIT) is DEPRECATED + non-publishable.
bash "$(dirname "$0")/release/maven_channel_guard.sh"
cd bindings/java
mvn -P release deploy -DskipTests --dry-run  # verwijder --dry-run op launch day
cd ../..

echo "✅ Alle packages gepubliceerd"
