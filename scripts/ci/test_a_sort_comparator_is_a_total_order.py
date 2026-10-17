#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The comparator guard refuses a tolerance and nothing else."""
import pathlib
import tempfile
import unittest

import a_sort_comparator_is_a_total_order as guard

OLD = """
fn f(mine: &mut Vec<usize>, words: &[W]) {
    mine.sort_by(|&a, &b| {
        let (ya, yb) = (words[a].bbox[3], words[b].bbox[3]);
        if (ya - yb).abs() > 0.5 * (words[a].bbox[3] - words[a].bbox[1]) {
            yb.partial_cmp(&ya).unwrap_or(std::cmp::Ordering::Equal)
        } else {
            words[a].bbox[0].partial_cmp(&words[b].bbox[0]).unwrap_or(std::cmp::Ordering::Equal)
        }
    });
}
"""
FINE = """
fn g(v: &mut Vec<(f64, usize)>) -> Vec<usize> {
    v.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    v.sort_unstable_by(|a, b| {
        // not (a - b).abs() > tolerance: sorted first, grouped after
        b.0.total_cmp(&a.0)
    });
    v.sort_by_key(|x| x.1);
    Vec::<usize>::new()
}
"""
IF_ONLY = """
fn h(v: &mut Vec<f64>) {
    v.sort_by(|a, b| if a - b > 1.0 { std::cmp::Ordering::Less } else { a.total_cmp(b) });
}
"""


class GuardTests(unittest.TestCase):
    def run_on(self, text):
        with tempfile.TemporaryDirectory() as d:
            root = pathlib.Path(d)
            (root / "crates/pdf-engine/src").mkdir(parents=True)
            (root / "crates/pdf-engine/src/lib.rs").write_text(text)
            return guard.offenders(root)

    def test_the_comparator_that_aborted_five_pages_is_refused(self):
        self.assertEqual(len(self.run_on(OLD)), 1)

    def test_total_keys_generics_arrows_and_comments_pass(self):
        self.assertEqual(self.run_on(FINE), [])

    def test_a_threshold_in_an_if_is_refused_without_abs(self):
        self.assertEqual(len(self.run_on(IF_ONLY)), 1)

    def test_the_real_tree_holds_none(self):
        self.assertEqual(guard.offenders(guard.ROOT), [])


if __name__ == "__main__":
    unittest.main()
