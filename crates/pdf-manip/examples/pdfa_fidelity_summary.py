#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Summarize saved four-axis runs with explicit per-document regressions.

This does not convert, validate, or change a threshold. Source reports remain
necessary evidence. Invoke once per matched corpus, never pool unrelated sets.
"""
import argparse
import hashlib
import json
from pathlib import Path


KEYS = ['documents', 'conversion_failed', 'judged', 'conformant', 'without_verdict',
        'vera_batches_failed',
        'word_retention_median', 'word_retention_p5', 'word_below_95',
        'word_below_50', 'char_retention_median', 'char_retention_p5',
        'separator_broken', 'render_diff_median',
        'render_diff_p95', 'render_pages_changed', 'render_not_compared',
        'bytes_in', 'bytes_out', 'size_ratio_total']


def summarize(before, after):
    def index(report):
        rows = report['rows']
        result = {r['name']: r for r in rows}
        if len(rows) != len(result) or len(rows) != report['documents']:
            raise ValueError('duplicate/missing document rows')
        return result
    a, b = index(before), index(after)
    if a.keys() != b.keys():
        raise ValueError('the corpus names differ')
    regressions = []; joint = []
    for name, old in a.items():
        new = b[name]
        if old['bytesIn'] != new['bytesIn']:
            raise ValueError('input sizes differ: ' + name)
        changes = {}
        if old['converted'] and not new['converted']:
            changes['conversion'] = [True, False]
        if old.get('conformant') is True and new.get('conformant') is not True:
            changes['conformance'] = [True, new.get('conformant')]
        for key in ['wordRetention', 'charRetention']:
            x, y = old.get(key), new.get(key)
            if x is not None and (y is None or y < x):
                changes[key] = [x, y]
        x, y = old.get('renderMean'), new.get('renderMean')
        if x is not None and (y is None or y > x + 0.001):
            changes['sampled_render_mae_increase_over_0_001'] = [x, y]
        if changes:
            regressions.append({'file': name, 'changes': changes})
        if old['converted'] and new['converted']:
            joint.append((old['bytesOut'], new['bytesOut']))
    return {'before': {k: before[k] for k in KEYS},
            'after': {k: after[k] for k in KEYS},
            'jointly_converted': {'documents': len(joint),
                'before_bytes': sum(x for x, _ in joint),
                'after_bytes': sum(y for _, y in joint)},
            'regressions': regressions,
            'identity_limit': 'Names and input lengths checked here; verify source SHA-256 with the pinned manifests.'}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    for key in ['before', 'after', 'out']:
        p.add_argument('--' + key, type=Path, required=True)
    a = p.parse_args()
    result = summarize(json.loads(a.before.read_text()), json.loads(a.after.read_text()))
    result['reports_sha256'] = {key: hashlib.sha256(getattr(a, key).read_bytes()).hexdigest()
                               for key in ['before', 'after']}
    a.out.write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
