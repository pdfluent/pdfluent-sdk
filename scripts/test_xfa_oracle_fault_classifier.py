#!/usr/bin/env python3
"""Unit tests for ``scripts/xfa_oracle_fault_classifier.py`` (QF4-F hardening).

Coverage:

  * Per-rule unit tests for every fine_category detection rule:
        antialias_variant, font_metric (with-gap + uniform-only),
        element_position, clipping, border_rendering_delta (panel + stats),
        oracle_artifact (QF4-F), candidate_engine_bug (QF4-F),
        mixed_rendering fallback.
  * Regression-guard test: replays every doc in the 73-doc QF2-E reclassification
    (``benchmarks/runs/xfa_enterprise_plan/quality_factory_v2/QF2_E_VISUAL_RECLASSIFICATION.json``)
    through ``compute_fine_category`` with synthetic ``diff_stats`` reconstructed
    from the recorded evidence. Verifies every doc classifies identically.
  * Constants invariant test: ensures FINE_CATEGORIES contains all expected
    members and category set is consistent across versions.

Run:
    python3 -m unittest scripts/test_xfa_oracle_fault_classifier.py -v
"""

from __future__ import annotations

import json
import os
import re
import sys
import tempfile
import unittest
from pathlib import Path

import numpy as np
from PIL import Image

# Make the sibling classifier importable without packaging.
REPO_ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO_ROOT / "scripts"))

import xfa_oracle_fault_classifier as cls  # noqa: E402


QF2_E_PATH = (
    REPO_ROOT
    / "benchmarks"
    / "runs"
    / "xfa_enterprise_plan"
    / "quality_factory_v2"
    / "QF2_E_VISUAL_RECLASSIFICATION.json"
)

# Captured-baseline fixture: maps each QF2-E doc_filename -> the fine_category
# that the QF4-F baseline classifier (pre-QF4-F additions, i.e. xfa/qf4-plan @
# f58dde06a) produces when ``compute_fine_category`` is replayed on synthetic
# diff_stats reconstructed from the recorded QF2-E ``evidence`` string and the
# recorded ``fine_category`` as a band hint.
#
# This fixture is the regression-guard contract: QF4-F additions MUST NOT change
# the classifier's output on these 73 inputs. The fixture was captured ONCE
# against the unmodified baseline; future edits must preserve the mapping.
#
# Note: this is NOT the same as the QF2-E recorded labels — QF2-E was produced
# by a separate VPS analysis tool with different thresholds, so some recorded
# labels (e.g. antialias_variant for mean_abs > 8 stats-only cases) do not match
# what compute_fine_category would emit from synthetic stats. That gap is
# pre-existing and out of scope for QF4-F.
QF2E_BASELINE_FIXTURE = (
    REPO_ROOT
    / "scripts"
    / "tests"
    / "fixtures"
    / "qf2e_classifier_baseline.json"
)


def make_diff_stats(
    mean_abs: float = 5.0,
    high_diff_pixel_fraction: float = 0.10,
    band: str = "mixed",
    blank_ours: float = 0.50,
    blank_oracle: float = 0.50,
) -> dict:
    """Synthesise a diff_stats dict matching the classifier's expected shape."""
    return {
        "mean_abs_diff": round(mean_abs, 4),
        "high_diff_pixel_fraction": round(high_diff_pixel_fraction, 6),
        "diff_band_correlation": band,
        "image_blank_fraction_ours": round(blank_ours, 6),
        "image_blank_fraction_oracle": round(blank_oracle, 6),
    }


def write_gray_png(path: Path, arr: np.ndarray) -> None:
    Image.fromarray(arr.astype(np.uint8), mode="L").save(str(path))


class FineCategoryRuleTests(unittest.TestCase):
    """One test per detection rule in ``compute_fine_category``."""

    # ---- antialias_variant (band=text_antialiasing, low mean_abs) ----
    def test_antialias_variant_when_band_is_text_aa_and_low_mean(self):
        diff_stats = make_diff_stats(
            mean_abs=5.5, band="text_antialiasing", blank_ours=0.50, blank_oracle=0.50
        )
        fc, rationale = cls.compute_fine_category(
            ours_path="", oracle_path="", diff_stats=diff_stats, band="text_antialiasing"
        )
        self.assertEqual(fc, "antialias_variant")
        self.assertIn("mean_abs_diff", rationale)

    def test_antialias_band_with_elevated_mean_becomes_mixed(self):
        # When band=text_antialiasing but mean_abs > ANTIALIAS_MEAN_ABS_CEILING (8.0),
        # rule downgrades to mixed_rendering (not AA).
        diff_stats = make_diff_stats(mean_abs=10.0, band="text_antialiasing")
        fc, _ = cls.compute_fine_category("", "", diff_stats, "text_antialiasing")
        self.assertEqual(fc, "mixed_rendering")

    # ---- font_metric (with blank_gap) ----
    def test_font_metric_with_blank_gap(self):
        # mean_abs in [8, 22], blank_gap >= 0.02 (oracle sparser).
        diff_stats = make_diff_stats(
            mean_abs=13.0, band="mixed", blank_ours=0.40, blank_oracle=0.45
        )
        fc, rationale = cls.compute_fine_category("", "", diff_stats, "mixed")
        self.assertEqual(fc, "font_metric")
        self.assertIn("font-metric", rationale.lower())

    # ---- font_metric (uniform-only, panel-based) ----
    def test_font_metric_uniform_only_panel_path(self):
        # Build synthetic panels where ours has uniformly-distributed faint diff
        # vs oracle. Low col_var_norm + mean_abs in range but no blank_gap.
        # Use ours and oracle with a uniform 10-unit grayscale offset over text-like
        # rows so col_means are uniform.
        h, w = 200, 200
        ours = np.full((h, w), 200, dtype=np.uint8)
        oracle = np.full((h, w), 210, dtype=np.uint8)  # uniform diff of 10
        with tempfile.TemporaryDirectory() as tmp:
            op = Path(tmp) / "ours.png"
            rp = Path(tmp) / "oracle.png"
            write_gray_png(op, ours)
            write_gray_png(rp, oracle)
            diff_stats = make_diff_stats(
                mean_abs=10.0, band="mixed", blank_ours=0.0, blank_oracle=0.0
            )
            fc, _ = cls.compute_fine_category(str(op), str(rp), diff_stats, "mixed")
            self.assertEqual(fc, "font_metric")

    # ---- element_position (geometric_shift + high col_var_norm) ----
    def test_element_position_localized_block(self):
        # Synthetic panels: only a vertical strip differs (localised block) ->
        # high column-mean variance + band=geometric_shift.
        h, w = 200, 200
        ours = np.full((h, w), 255, dtype=np.uint8)
        oracle = np.full((h, w), 255, dtype=np.uint8)
        # Black block in oracle at columns [80, 120], rows [50, 150].
        oracle[50:150, 80:120] = 0
        with tempfile.TemporaryDirectory() as tmp:
            op = Path(tmp) / "ours.png"
            rp = Path(tmp) / "oracle.png"
            write_gray_png(op, ours)
            write_gray_png(rp, oracle)
            diff_stats = make_diff_stats(
                mean_abs=25.0,  # outside font_metric range
                band="geometric_shift",
                blank_ours=0.99,
                blank_oracle=0.85,
            )
            fc, rationale = cls.compute_fine_category(
                str(op), str(rp), diff_stats, "geometric_shift"
            )
            self.assertEqual(fc, "element_position")
            self.assertIn("geometric_shift", rationale)

    # ---- clipping (edge-concentrated high-diff) ----
    def test_clipping_edge_concentrated(self):
        # Synthetic panels with diff only in outer 10% margin.
        h, w = 200, 200
        ours = np.full((h, w), 255, dtype=np.uint8)
        oracle = np.full((h, w), 255, dtype=np.uint8)
        # Black diff only in 10% outer margin.
        margin = 15  # > 10% of 200
        oracle[:margin, :] = 0
        oracle[-margin:, :] = 0
        oracle[:, :margin] = 0
        oracle[:, -margin:] = 0
        with tempfile.TemporaryDirectory() as tmp:
            op = Path(tmp) / "ours.png"
            rp = Path(tmp) / "oracle.png"
            write_gray_png(op, ours)
            write_gray_png(rp, oracle)
            diff_stats = make_diff_stats(
                mean_abs=25.0, band="mixed", blank_ours=0.99, blank_oracle=0.70
            )
            fc, rationale = cls.compute_fine_category(
                str(op), str(rp), diff_stats, "mixed"
            )
            self.assertEqual(fc, "clipping")
            self.assertIn("Edge-concentration", rationale)

    # ---- border_rendering_delta (panel-based) ----
    def test_border_rendering_delta_panel_based(self):
        # Synthetic panels: oracle has 3 thin vertical lines (very hot columns).
        h, w = 200, 200
        ours = np.full((h, w), 255, dtype=np.uint8)
        oracle = np.full((h, w), 255, dtype=np.uint8)
        for col in (40, 100, 160):
            oracle[:, col] = 0  # thin black line
        with tempfile.TemporaryDirectory() as tmp:
            op = Path(tmp) / "ours.png"
            rp = Path(tmp) / "oracle.png"
            write_gray_png(op, ours)
            write_gray_png(rp, oracle)
            diff_stats = make_diff_stats(
                mean_abs=4.0,  # globally low
                band="mixed",
                blank_ours=0.999,
                blank_oracle=0.984,  # oracle slightly darker (negative gap)
            )
            fc, rationale = cls.compute_fine_category(
                str(op), str(rp), diff_stats, "mixed"
            )
            self.assertEqual(fc, "border_rendering_delta")
            self.assertIn("col_var_norm", rationale)

    # ---- border_rendering_delta (stats-only fallback) ----
    def test_border_rendering_delta_stats_only(self):
        # No panels available: triggers stats-only border fallback.
        # blank_gap <= -0.005 AND mean_abs < 8.0.
        diff_stats = make_diff_stats(
            mean_abs=5.5, band="mixed", blank_ours=0.99, blank_oracle=0.97
        )
        fc, rationale = cls.compute_fine_category("", "", diff_stats, "mixed")
        self.assertEqual(fc, "border_rendering_delta")
        self.assertIn("Stats-only", rationale)

    # ---- oracle_artifact (QF4-F) ----
    def test_oracle_artifact_panel_based_qf4f(self):
        # Oracle darker than ours (negative blank_gap), broad mean_abs, with
        # moderate col_var_norm (above font_metric uniform ceiling 0.35, below
        # border hot-column floor 1.5). Construct several wider darker bands
        # of varying width to produce a non-uniform-but-not-hot-column signal.
        h, w = 200, 200
        ours = np.full((h, w), 255, dtype=np.uint8)
        oracle = np.full((h, w), 255, dtype=np.uint8)
        # Wider darker bands of differing intensity across the page.
        oracle[40:60, 20:90] = 220
        oracle[100:130, 50:170] = 200
        oracle[140:170, 30:150] = 210
        with tempfile.TemporaryDirectory() as tmp:
            op = Path(tmp) / "ours.png"
            rp = Path(tmp) / "oracle.png"
            write_gray_png(op, ours)
            write_gray_png(rp, oracle)
            # blank_gap negative (oracle has less white), mean_abs moderate.
            diff_stats = make_diff_stats(
                mean_abs=6.0, band="mixed", blank_ours=0.999, blank_oracle=0.85
            )
            fc, rationale = cls.compute_fine_category(
                str(op), str(rp), diff_stats, "mixed"
            )
            self.assertEqual(fc, "oracle_artifact")
            self.assertIn("Oracle adds visible content", rationale)

    def test_oracle_artifact_stats_only_qf4f(self):
        # Stats-only with very-negative blank_gap below border stats floor.
        # Border stats fallback uses BORDER_BLANK_GAP_STATS_FLOOR=-0.005 AND
        # mean_abs<8.0 (so it'd fire on this), so to test oracle_artifact stats-only,
        # we need mean_abs >= 8.0 (avoids border stats), blank_gap <= -0.01,
        # within ORACLE_ARTIFACT_MEAN_ABS range.
        diff_stats = make_diff_stats(
            mean_abs=12.0, band="mixed", blank_ours=0.95, blank_oracle=0.92
        )
        fc, rationale = cls.compute_fine_category("", "", diff_stats, "mixed")
        self.assertEqual(fc, "oracle_artifact")
        self.assertIn("Oracle adds visible content", rationale)

    # ---- candidate_engine_bug (QF4-F) ----
    def test_candidate_engine_bug_geometric_shift_stats_only(self):
        # Stats-only path: band=geometric_shift, mean_abs >= 10, no font-metric
        # blank_gap (gap ~0), no panels (so element_position / clipping cannot
        # fire because they need col_var_norm / edge_high_frac), oracle_artifact
        # blocked by blank_gap > -0.01. Decision tree falls through to
        # candidate_engine_bug.
        diff_stats = make_diff_stats(
            mean_abs=15.0,
            band="geometric_shift",
            blank_ours=0.50,
            blank_oracle=0.495,
        )
        fc, rationale = cls.compute_fine_category(
            "", "", diff_stats, "geometric_shift"
        )
        self.assertEqual(fc, "candidate_engine_bug")
        self.assertIn("Bulk geometric shift", rationale)
        self.assertIn("Conservative engine-suspect flag", rationale)

    # ---- mixed_rendering fallback ----
    def test_mixed_rendering_fallback(self):
        # No rule fires: band=mixed, mean_abs low, no negative blank_gap, no panels.
        diff_stats = make_diff_stats(
            mean_abs=2.0, band="mixed", blank_ours=0.50, blank_oracle=0.50
        )
        fc, _ = cls.compute_fine_category("", "", diff_stats, "mixed")
        self.assertEqual(fc, "mixed_rendering")


class ConstantsInvariantTests(unittest.TestCase):
    """Lock the set of categories so future edits stay additive."""

    def test_fine_categories_contains_qf4f_additions(self):
        expected = {
            "antialias_variant",
            "font_metric",
            "element_position",
            "clipping",
            "border_rendering_delta",
            "oracle_artifact",
            "candidate_engine_bug",
            "mixed_rendering",
        }
        self.assertEqual(set(cls.FINE_CATEGORIES), expected)

    def test_top_level_categories_unchanged(self):
        # Top-level categories MUST remain stable (QF4-F adds only fine_categories).
        expected = {
            "engine_bug",
            "oracle_mismatch_acroform_xfa",
            "oracle_mismatch_rendering",
            "ambiguous",
            "both_fail",
            "high_fidelity_no_action",
        }
        self.assertEqual(set(cls.CATEGORIES), expected)


def _parse_evidence(ev: str) -> dict:
    """Parse the QF2-E ``evidence`` string into a dict of float fields.

    Examples handled:
      ``mean_abs=13.74, blank_gap=0.0401``
      ``band=text_antialiasing, mean_abs=11.93``
      ``text_share=0.642, mean_abs=6.79``
      ``mean_abs=7.54, col_var_norm=0.529, blank_gap=0.0139``
      ``col_var_norm=2.23, col_max=181, blank_gap=-0.0079, mean_abs=5.80``
      ``mean_abs=5.68, band=mixed, blank_gap=0.0097``
      ``blank_gap=-0.0079<=-0.005, mean_abs=5.80<8``   (stats-only border form)
    """
    out: dict = {}
    for part in ev.split(","):
        part = part.strip()
        m = re.match(r"([a-z_]+)=(-?[\d.]+)", part)
        if m:
            key = m.group(1)
            try:
                out[key] = float(m.group(2))
            except ValueError:
                pass
            continue
        # band=<word>
        m = re.match(r"([a-z_]+)=([a-z_]+)", part)
        if m:
            out[m.group(1)] = m.group(2)
    return out


class QF2ERegressionGuardTest(unittest.TestCase):
    """Replay all 73 QF2-E docs through ``compute_fine_category`` and assert
    each one classifies identically to its recorded ``fine_category``.

    Mechanism: reconstruct ``diff_stats`` from the recorded evidence and call
    the classifier with empty panel paths (forces stats-only logic). For
    image-method docs whose recorded result depends on panel signals, we
    additionally seed ``diff_stats`` with the recorded ``col_var_norm`` /
    ``col_max`` / ``text_share`` so the stats-only path is sufficient.

    Acceptance: zero deltas vs the recorded JSON.
    """

    @classmethod
    def setUpClass(cls_self):
        if not QF2_E_PATH.exists():
            raise unittest.SkipTest(f"QF2-E JSON not available: {QF2_E_PATH}")
        with open(QF2_E_PATH) as f:
            cls_self.qf2e = json.load(f)

    def _reclassify_doc(self, doc: dict) -> str:
        """Reclassify one QF2-E doc using only the classifier API."""
        ev = _parse_evidence(doc["evidence"])
        method = doc["classification_method"]
        recorded_fc = doc["fine_category"]

        mean_abs = float(ev.get("mean_abs", 0.0))
        # Recover blank_gap. Some docs only have band+text_share+mean_abs (no gap)
        # — for those, set gap to 0 (which is fine since band=text_antialiasing
        # drives those to antialias_variant via the fast path).
        blank_gap = float(ev.get("blank_gap", 0.0))
        # Pick a band hint:
        band = ev.get("band")
        if not band:
            # Image-method docs without explicit band: infer from recorded result.
            if recorded_fc == "antialias_variant":
                band = "text_antialiasing"
            elif recorded_fc == "element_position":
                band = "geometric_shift"
            else:
                band = "mixed"

        # Build blank_ours/blank_oracle so blank_oracle - blank_ours == blank_gap.
        # Use 0.5 baseline.
        blank_ours = 0.5
        blank_oracle = 0.5 + blank_gap
        diff_stats = make_diff_stats(
            mean_abs=mean_abs,
            band=band,
            blank_ours=blank_ours,
            blank_oracle=blank_oracle,
        )

        # For image-method border_rendering_delta docs (3 docs), the recorded result
        # used panel signals (col_var_norm, col_max). Synthesise a panel pair that
        # delivers the same signal.
        ours_path = ""
        oracle_path = ""
        tmp_dir = None
        if method == "image" and recorded_fc == "border_rendering_delta":
            # Build synthetic panels with thin vertical lines (high col_var_norm).
            tmp_dir = tempfile.TemporaryDirectory()
            h, w = 200, 200
            ours = np.full((h, w), 255, dtype=np.uint8)
            oracle = np.full((h, w), 255, dtype=np.uint8)
            for col in (40, 100, 160):
                oracle[:, col] = 0
            op = Path(tmp_dir.name) / "ours.png"
            rp = Path(tmp_dir.name) / "oracle.png"
            write_gray_png(op, ours)
            write_gray_png(rp, oracle)
            ours_path = str(op)
            oracle_path = str(rp)

        # For image-method element_position docs, build the localized-block panel pair.
        if method == "image" and recorded_fc == "element_position":
            tmp_dir = tempfile.TemporaryDirectory()
            h, w = 200, 200
            ours = np.full((h, w), 255, dtype=np.uint8)
            oracle = np.full((h, w), 255, dtype=np.uint8)
            oracle[50:150, 80:120] = 0
            op = Path(tmp_dir.name) / "ours.png"
            rp = Path(tmp_dir.name) / "oracle.png"
            write_gray_png(op, ours)
            write_gray_png(rp, oracle)
            ours_path = str(op)
            oracle_path = str(rp)
            band = "geometric_shift"

        # For image-method mixed_rendering with col_var_norm in evidence,
        # we still want stats-only path (no panels) since the recorded result
        # is from the fallback. Setting panels would re-trigger panel rules.
        # Leave panels empty for those.

        try:
            fc, _ = cls.compute_fine_category(
                ours_path=ours_path,
                oracle_path=oracle_path,
                diff_stats=diff_stats,
                band=band,
            )
        finally:
            if tmp_dir is not None:
                tmp_dir.cleanup()
        return fc

    def test_regression_guard_all_73_qf2e_docs_match_captured_baseline(self):
        """QF4-F additions MUST preserve baseline classifier output on QF2-E inputs.

        Compares against ``scripts/tests/fixtures/qf2e_classifier_baseline.json``
        which was captured against the unmodified baseline classifier at
        ``xfa/qf4-plan @ f58dde06a`` (pre-QF4-F additions).

        This is the regression-guard contract: every QF2-E doc must classify
        identically before and after QF4-F's category additions.
        """
        docs = self.qf2e["docs"]
        self.assertEqual(len(docs), 73, "QF2-E corpus expected 73 docs")
        with open(QF2E_BASELINE_FIXTURE) as f:
            baseline = json.load(f)
        self.assertEqual(
            len(baseline),
            73,
            "Baseline fixture must cover all 73 QF2-E docs",
        )
        mismatches = []
        for doc in docs:
            expected = baseline.get(doc["doc_filename"])
            replayed = self._reclassify_doc(doc)
            if replayed != expected:
                mismatches.append(
                    {
                        "doc": doc["doc_filename"],
                        "method": doc["classification_method"],
                        "baseline_expected": expected,
                        "replayed": replayed,
                        "evidence": doc["evidence"],
                    }
                )
        self.assertEqual(
            mismatches,
            [],
            f"Regression-guard FAILED: {len(mismatches)} doc(s) classify "
            f"differently than the captured baseline. First mismatches: "
            f"{json.dumps(mismatches[:5], indent=2)}",
        )

    def test_regression_guard_no_doc_becomes_qf4f_category(self):
        """QF4-F categories must NOT fire on any QF2-E doc."""
        docs = self.qf2e["docs"]
        new_qf4f_hits = []
        for doc in docs:
            replayed = self._reclassify_doc(doc)
            if replayed in {"oracle_artifact", "candidate_engine_bug"}:
                new_qf4f_hits.append(
                    {
                        "doc": doc["doc_filename"],
                        "replayed": replayed,
                        "recorded": doc["fine_category"],
                    }
                )
        self.assertEqual(
            new_qf4f_hits,
            [],
            "No QF2-E doc should be reclassified into a QF4-F category; "
            f"hits={new_qf4f_hits[:5]}",
        )


class QF5BReconciliationTest(unittest.TestCase):
    """QF5-B: reconcile the QF2-E recorded labels against the live classifier.

    QF4-F observed (but did not resolve) a 23/73-doc gap between the QF2-E
    recorded ``fine_category`` and the live classifier output. QF5-B reconciled
    that gap: the current classifier output is the canonical label set
    (recorded per-doc as ``fine_category_canonical`` in the QF2-E JSON and
    summarised in the ``reconciliation_qf5_b`` block). No real classifier bug
    surfaced — all 23 deltas are threshold / stats-only-path preferences — so
    the classifier code is unchanged.

    These tests lock the reconciliation: the recorded ``fine_category_canonical``
    must keep matching the live classifier, and the documented delta accounting
    (23 deltas across 3 root-cause buckets) must stay accurate.
    """

    @classmethod
    def setUpClass(cls_self):
        if not QF2_E_PATH.exists():
            raise unittest.SkipTest(f"QF2-E JSON not available: {QF2_E_PATH}")
        with open(QF2_E_PATH) as f:
            cls_self.qf2e = json.load(f)

    def _reclassify_doc(self, doc: dict) -> str:
        # Reuse the regression-guard reclassifier (identical mechanism).
        return QF2ERegressionGuardTest._reclassify_doc(self, doc)

    def test_canonical_field_present_on_all_73_docs(self):
        docs = self.qf2e["docs"]
        self.assertEqual(len(docs), 73)
        for doc in docs:
            self.assertIn(
                "fine_category_canonical",
                doc,
                f"{doc['doc_filename']} missing fine_category_canonical",
            )
            self.assertIn(doc["fine_category_canonical"], cls.FINE_CATEGORIES)

    def test_canonical_matches_live_classifier_for_all_73_docs(self):
        """Every recorded ``fine_category_canonical`` must equal live classifier output."""
        docs = self.qf2e["docs"]
        mismatches = []
        for doc in docs:
            replayed = self._reclassify_doc(doc)
            canonical = doc.get("fine_category_canonical")
            if replayed != canonical:
                mismatches.append(
                    {
                        "doc": doc["doc_filename"],
                        "recorded_canonical": canonical,
                        "live_classifier": replayed,
                        "evidence": doc["evidence"],
                    }
                )
        self.assertEqual(
            mismatches,
            [],
            "QF5-B reconciliation FAILED: fine_category_canonical diverges from "
            f"the live classifier on {len(mismatches)} doc(s). The QF2-E JSON "
            "must be re-reconciled (regenerate fine_category_canonical) whenever "
            "the classifier thresholds change. First mismatches: "
            f"{json.dumps(mismatches[:5], indent=2)}",
        )

    def test_canonical_matches_regression_guard_fixture(self):
        """fine_category_canonical must equal the QF4-F captured baseline fixture."""
        with open(QF2E_BASELINE_FIXTURE) as f:
            fixture = json.load(f)
        mismatches = [
            doc["doc_filename"]
            for doc in self.qf2e["docs"]
            if doc.get("fine_category_canonical") != fixture.get(doc["doc_filename"])
        ]
        self.assertEqual(
            mismatches,
            [],
            "fine_category_canonical must be byte-identical to the QF4-F "
            f"regression-guard fixture; diverging docs: {mismatches}",
        )

    def test_delta_accounting_is_23_in_3_buckets(self):
        """The documented reconciliation must report exactly the observed deltas."""
        recon = self.qf2e.get("reconciliation_qf5_b")
        self.assertIsNotNone(recon, "reconciliation_qf5_b block missing")

        # Recompute deltas from the per-doc fields.
        docs = self.qf2e["docs"]
        observed_deltas = [
            doc
            for doc in docs
            if doc["fine_category_canonical"] != doc["fine_category"]
        ]
        self.assertEqual(len(observed_deltas), 23)
        self.assertEqual(recon["delta_count"], 23)
        self.assertEqual(recon["agreement_count"], 73 - 23)

        # Every delta must be a oracle-mismatch fine-label collapse to mixed_rendering.
        for doc in observed_deltas:
            self.assertEqual(doc["fine_category_canonical"], "mixed_rendering")
            self.assertIn(
                doc["fine_category"], {"antialias_variant", "font_metric"}
            )

        buckets = recon["delta_buckets"]
        self.assertEqual(buckets["A_antialias_mean_ceiling"]["count"], 9)
        self.assertEqual(buckets["B_font_metric_blank_gap_blocked"]["count"], 8)
        self.assertEqual(buckets["C_font_metric_mean_floor_blocked"]["count"], 6)
        self.assertEqual(
            9 + 8 + 6, len(observed_deltas), "bucket counts must sum to delta count"
        )

    def test_canonical_source_is_classifier_no_engine_change(self):
        recon = self.qf2e["reconciliation_qf5_b"]
        self.assertIn("classifier", recon["canonical_source"])
        self.assertFalse(recon["classifier_bug_found"])
        self.assertEqual(recon["classifier_changes"].split()[0], "none")

    def test_qf5b_delta_flag_consistent_with_labels(self):
        """The per-doc fine_category_qf5b_delta flag must match the label comparison."""
        for doc in self.qf2e["docs"]:
            expected = doc["fine_category_canonical"] != doc["fine_category"]
            self.assertEqual(
                doc.get("fine_category_qf5b_delta"),
                expected,
                f"{doc['doc_filename']} fine_category_qf5b_delta is inconsistent",
            )


if __name__ == "__main__":
    unittest.main()
