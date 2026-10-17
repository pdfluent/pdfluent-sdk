#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
#
# The public enums that are new since v1.0.0 stay #[non_exhaustive], per
# defining crate, judged from outside the crate by the same check the semver
# gate runs (scripts/ci/check_new_public_enums.py, real rustdoc JSON on HEAD
# and v1.0.0, defaults plus the gate's feature list).
#
# WHY: in 1.x a new variant on an exhaustive public enum is a major break.
# On 14-10-2026 thirteen enums new since v1.0.0 (one in pdf-engine, twelve in
# pdfluent) were exhaustive; each was sealed instead of allowlisted. Inside
# the defining crate a match needs no wildcard arm, so no test in that crate
# can see the attribute go; only the rustdoc view from outside can.
#
# Each case also requires the crate to report at least as many new enums as
# it has pinned below: a check that found none (a broken walk, an empty
# rustdoc) would otherwise pass vacuously.
#
# Exit 2 from the check (no pinned nightly, no baseline) is SKIPPED, never a
# pass: this script then exits 2 as well.
set -u
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
CHECK="$ROOT/scripts/ci/check_new_public_enums.py"
FEATURES="$ROOT/scripts/ci/semver_features.txt"
fails=0; cases=0; skipped=0
check() { cases=$((cases+1)); if [ "$1" = ok ]; then echo "  ok    $2"; else echo "  FAIL  $2${3:+   [$3]}"; fails=$((fails+1)); fi; }

# crate|minimum new-enum count|the enums sealed on 14-10-2026 (documentation:
# the check names any that loses the attribute).
CRATES=(
  "pdf-engine|1|pdf_engine::document::LinkTarget"
  "pdfluent|12|pdfluent::compat::Value pdfluent::compat::document::EmbItem pdfluent::compat::document::PageSelection pdfluent::compat::edit::AnnotIdent pdfluent::compat::geometry::AreaUnit pdfluent::compat::geometry::CompareOp pdfluent::compat::pixmap::Colorspace pdfluent::compat::text::SearchHits pdfluent::compat::text::TextArg pdfluent::compat::text::TextFormat pdfluent::compat::text::TextOutput pdfluent::images::ImageColorSpace"
)

for entry in "${CRATES[@]}"; do
  IFS='|' read -r crate minimum pinned <<<"$entry"
  out=$(cd "$ROOT" && python3 "$CHECK" --crates "$crate" --features-file "$FEATURES" 2>&1); st=$?
  if [ "$st" -eq 2 ]; then
    echo "SKIPPED (not a pass): $crate: $(printf '%s\n' "$out" | grep 'SKIPPED' | tail -1)" >&2
    skipped=$((skipped+1))
    continue
  fi
  check "$([ "$st" -eq 0 ] && echo ok)" "$crate: every enum new since v1.0.0 is sealed" "exit=$st out=$(printf '%s\n' "$out" | grep -E '^new exhaustive|FAIL' | tr '\n' ' ')"
  for path in $pinned; do
    case "$out" in
      *"new exhaustive public enum: $path "*) check FAIL "$crate: $path is sealed" "named by the check" ;;
    esac
  done
  found=$(printf '%s\n' "$out" | sed -n "s/^\[enums\] OK: .*, \([0-9][0-9]*\) new public enum(s).*/\1/p" | tail -1)
  check "$([ -n "$found" ] && [ "$found" -ge "$minimum" ] && echo ok)" \
    "$crate: the check saw the pinned enums (>= $minimum new)" "found=${found:-none}"
done

echo "cases=$cases fails=$fails skipped=$skipped"
[ "$fails" -eq 0 ] || exit 1
[ "$skipped" -eq 0 ] || exit 2
exit 0
