#!/usr/bin/env python3
"""
XFA-QF1-C: FormCalc residual scan
=================================

First automated FormCalc-only residual scan for the Quality Factory V1
programme. Runs `pdfluent flatten` over a corpus with
`XFA_FORMCALC_DEBUG=1` so the `crates/pdf-xfa/src/dynamic.rs`
`execute_formcalc_script` path emits a structured `XFA_FORMCALC_DEBUG
stage=... activity=... message=...` line for every failed FormCalc
script. Combined with the existing `formcalc_run` / `formcalc_errors`
counters in the `XFA script metadata:` summary line this produces:

  * per-doc `formcalc_run`, `formcalc_errors`, `formcalc_skip_rate`
  * clusters by stage      (lexer / parser / interpreter)
  * clusters by error_type (UnknownFunction / DivisionByZero / TypeError /
                            RuntimeError / ParseError / LexerError /
                            ArityError / CallDepthExceeded)
  * clusters by function   (parsed from `Unknown function: <name>` and
                            `Wrong number of arguments for <name>: ...`)
  * Wave-2 candidate map ordered by impact-per-LOC

Outputs:

  * benchmarks/runs/xfa_enterprise_plan/quality_factory_v1/
        QF1_C_FORMCALC_RESIDUAL_MAP.json
  * benchmarks/runs/xfa_enterprise_plan/quality_factory_v1/
        QF1_C_FORMCALC_RESIDUAL_REPORT.md

Scope guarantees (per QF1-C plan):
  * Measurement only. No engine semantic change in formcalc-interpreter.
  * No paid API calls. No GitHub operations. No corpus mutation.
  * Output paths SHA256-only — never include `/Users/...` etc.

Usage:
    python3 scripts/xfa_formcalc_residual_scan.py \\
        --binary ./target/release/pdfluent \\
        --corpus-dir crates/xfa-golden-tests/golden \\
        --output-dir benchmarks/runs/xfa_enterprise_plan/quality_factory_v1 \\
        [--extra-corpus-dir corpus] \\
        [--max-docs 50]
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
import tempfile
import time
from collections import defaultdict
from pathlib import Path
from typing import Any


# ---------------------------------------------------------------------------
# Stderr parsing
# ---------------------------------------------------------------------------

RE_FORMCALC_DEBUG = re.compile(
    r'XFA_FORMCALC_DEBUG stage=(?P<stage>\w+) '
    r'activity="(?P<activity>[^"]*)" '
    r'message="(?P<message>[^"]*)"'
)
RE_METADATA = re.compile(r'XFA script metadata: (.+)')
RE_UNKNOWN_FN = re.compile(r'Unknown function:\s*([A-Za-z_][A-Za-z0-9_]*)')
RE_ARITY_FN = re.compile(r'Wrong number of arguments for\s+([A-Za-z_][A-Za-z0-9_]*)')

# Map a raw error message prefix to a coarse error-type bucket.
# These are mutually exclusive — first match wins.
ERROR_TYPE_RULES: list[tuple[str, re.Pattern[str]]] = [
    ('UnknownFunction',     re.compile(r'^Unknown function:')),
    ('ArityError',          re.compile(r'^Wrong number of arguments')),
    ('CallDepthExceeded',   re.compile(r'^call depth limit exceeded')),
    ('DivisionByZero',      re.compile(r'^Division by zero')),
    ('TypeError',           re.compile(r'^Type error:')),
    ('RuntimeError',        re.compile(r'^Runtime error:')),
    ('ParseError',          re.compile(r'^Parse error')),
    ('LexerError',          re.compile(r'^Lexer error')),
]


def classify_error_type(message: str) -> str:
    """Return the coarse error-type bucket for a FormCalc error message."""
    for name, rx in ERROR_TYPE_RULES:
        if rx.match(message):
            return name
    return 'Other'


def extract_function_name(message: str) -> str | None:
    """Extract the function name from UnknownFunction / ArityError messages.

    Returns ``None`` if the error has no function identifier (e.g. TypeError,
    RuntimeError without a function context).
    """
    m = RE_UNKNOWN_FN.search(message)
    if m:
        return m.group(1)
    m = RE_ARITY_FN.search(message)
    if m:
        return m.group(1)
    return None


def parse_doc_stderr(stderr: str) -> dict[str, Any]:
    """Parse a single document's flatten stderr for FormCalc signals."""
    events: list[dict[str, str]] = []
    for m in RE_FORMCALC_DEBUG.finditer(stderr):
        events.append({
            'stage': m.group('stage'),
            'activity': m.group('activity'),
            'message': m.group('message'),
            'error_type': classify_error_type(m.group('message')),
            'function': extract_function_name(m.group('message')) or '',
        })

    metadata: dict[str, str] = {}
    meta_match = RE_METADATA.search(stderr)
    if meta_match:
        for kv in meta_match.group(1).split():
            k, _, v = kv.partition('=')
            metadata[k] = v

    fc_run = int(metadata.get('formcalc_run', 0))
    fc_errors = int(metadata.get('formcalc_errors', 0))
    skip_rate = (fc_errors / fc_run) if fc_run else 0.0

    return {
        'events': events,
        'metadata': metadata,
        'formcalc_run': fc_run,
        'formcalc_errors': fc_errors,
        'formcalc_skip_rate': round(skip_rate, 4),
    }


# ---------------------------------------------------------------------------
# Doc runner
# ---------------------------------------------------------------------------

def sha256_file(path: str) -> str:
    h = hashlib.sha256()
    with open(path, 'rb') as f:
        for chunk in iter(lambda: f.read(65536), b''):
            h.update(chunk)
    return h.hexdigest()


def run_doc(binary: str, pdf_path: str, tmpdir: str) -> dict[str, Any]:
    """Flatten one PDF with FormCalc debug mode and parse the results."""
    out_pdf = os.path.join(tmpdir, 'out.pdf')
    env = os.environ.copy()
    env['XFA_FORMCALC_DEBUG'] = '1'
    # XFA_JS_DEBUG remains off — keep stderr noise scoped to FormCalc lines.
    env['XFA_JS_EXECUTION_MODE'] = 'sandboxed'

    start = time.monotonic()
    try:
        result = subprocess.run(
            [binary, 'flatten', pdf_path, '--output', out_pdf],
            capture_output=True,
            text=True,
            env=env,
            timeout=120,
        )
        timed_out = False
        stderr = result.stderr
        exit_code = result.returncode
    except subprocess.TimeoutExpired as exc:
        timed_out = True
        stderr = (exc.stderr.decode('utf-8', errors='replace')
                  if isinstance(exc.stderr, bytes) else (exc.stderr or ''))
        exit_code = -1
    elapsed_ms = int((time.monotonic() - start) * 1000)

    parsed = parse_doc_stderr(stderr)
    parsed['doc_sha256'] = sha256_file(pdf_path)
    parsed['doc_basename'] = Path(pdf_path).name  # filename only — no parent dir
    parsed['elapsed_ms'] = elapsed_ms
    parsed['exit_code'] = exit_code
    parsed['timed_out'] = timed_out
    return parsed


# ---------------------------------------------------------------------------
# Cluster aggregation
# ---------------------------------------------------------------------------

def build_clusters(doc_results: list[dict[str, Any]]) -> dict[str, Any]:
    """Aggregate per-document events into clusters along three axes:

      * stage       (lexer / parser / interpreter)
      * error_type  (UnknownFunction / TypeError / ParseError / ...)
      * function    (UnknownFunction / ArityError function names)
    """
    by_stage: dict[str, dict[str, Any]] = defaultdict(
        lambda: {'occurrences': 0, 'docs': set(), 'sample_messages': set()}
    )
    by_error_type: dict[str, dict[str, Any]] = defaultdict(
        lambda: {'occurrences': 0, 'docs': set(), 'sample_messages': set()}
    )
    by_function: dict[str, dict[str, Any]] = defaultdict(
        lambda: {'occurrences': 0, 'docs': set(), 'error_types': set()}
    )

    for doc in doc_results:
        sha = doc['doc_sha256']
        for ev in doc['events']:
            stage = ev['stage']
            etype = ev['error_type']
            msg = ev['message']

            by_stage[stage]['occurrences'] += 1
            by_stage[stage]['docs'].add(sha)
            if len(by_stage[stage]['sample_messages']) < 8:
                by_stage[stage]['sample_messages'].add(msg)

            by_error_type[etype]['occurrences'] += 1
            by_error_type[etype]['docs'].add(sha)
            if len(by_error_type[etype]['sample_messages']) < 8:
                by_error_type[etype]['sample_messages'].add(msg)

            fn = ev['function']
            if fn:
                by_function[fn]['occurrences'] += 1
                by_function[fn]['docs'].add(sha)
                by_function[fn]['error_types'].add(etype)

    def _finalize(bucket: dict[str, dict[str, Any]]) -> list[dict[str, Any]]:
        rows: list[dict[str, Any]] = []
        for name, data in bucket.items():
            rows.append({
                'name': name,
                'occurrences': data['occurrences'],
                'docs': len(data['docs']),
                'sample_messages': sorted(data.get('sample_messages', set()))[:8]
                                   if 'sample_messages' in data else [],
                'error_types': sorted(data.get('error_types', set()))
                               if 'error_types' in data else [],
            })
        rows.sort(key=lambda r: (-r['occurrences'], r['name']))
        return rows

    return {
        'by_stage': _finalize(by_stage),
        'by_error_type': _finalize(by_error_type),
        'by_function': _finalize(by_function),
    }


def wave2_candidates(clusters: dict[str, Any]) -> list[dict[str, Any]]:
    """Derive Wave-2 candidate fixes from the clusters.

    Ranking is by (docs × occurrences) impact score, capped at the top-5.
    Each candidate carries a short action hint.

    Hints are deliberately conservative — they propose *measurement* or
    *minor builtin* additions, never engine semantic redesign. The actual
    Wave-2 plan owns the prioritisation.
    """
    rows: list[dict[str, Any]] = []
    for fn in clusters['by_function']:
        score = fn['docs'] * fn['occurrences']
        action = ('Add builtin to crates/formcalc-interpreter/src/builtins.rs '
                  if 'UnknownFunction' in fn['error_types']
                  else 'Audit ArityError — adjust builtin signature or fix script binding')
        rows.append({
            'function': fn['name'],
            'occurrences': fn['occurrences'],
            'docs': fn['docs'],
            'error_types': fn['error_types'],
            'impact_score': score,
            'wave2_action': action,
        })
    rows.sort(key=lambda r: (-r['impact_score'], r['function']))
    return rows[:5]


# ---------------------------------------------------------------------------
# Output writers
# ---------------------------------------------------------------------------

def write_json(
    output_dir: Path,
    doc_results: list[dict[str, Any]],
    clusters: dict[str, Any],
    wave2: list[dict[str, Any]],
    corpus_dirs: list[str],
    baseline_commit: str,
) -> Path:
    """Emit the machine-readable residual map."""
    summary = {
        'docs_scanned': len(doc_results),
        'docs_with_formcalc_run': sum(1 for d in doc_results if d['formcalc_run'] > 0),
        'docs_with_formcalc_errors': sum(1 for d in doc_results if d['formcalc_errors'] > 0),
        'total_formcalc_run':    sum(d['formcalc_run'] for d in doc_results),
        'total_formcalc_errors': sum(d['formcalc_errors'] for d in doc_results),
    }
    payload = {
        'agent': 'QF1-C',
        'measurement': 'FormCalc residual scan (first automated)',
        'baseline_commit': baseline_commit,
        'corpus_dirs': corpus_dirs,
        'summary': summary,
        'clusters': clusters,
        'wave2_candidates': wave2,
        'per_doc': [
            {
                'sha256': d['doc_sha256'],
                'basename': d['doc_basename'],
                'formcalc_run': d['formcalc_run'],
                'formcalc_errors': d['formcalc_errors'],
                'formcalc_skip_rate': d['formcalc_skip_rate'],
                'elapsed_ms': d['elapsed_ms'],
                'exit_code': d['exit_code'],
                'timed_out': d['timed_out'],
                'events_truncated': [
                    {'stage': e['stage'], 'error_type': e['error_type'],
                     'function': e['function'], 'message': e['message'][:200]}
                    for e in d['events'][:20]
                ],
                'events_total': len(d['events']),
                'metadata': d['metadata'],
            }
            for d in sorted(doc_results,
                            key=lambda x: (-x['formcalc_errors'], x['doc_sha256']))
        ],
    }
    out = output_dir / 'QF1_C_FORMCALC_RESIDUAL_MAP.json'
    out.write_text(json.dumps(payload, indent=2, sort_keys=True) + '\n')
    return out


def write_report(
    output_dir: Path,
    doc_results: list[dict[str, Any]],
    clusters: dict[str, Any],
    wave2: list[dict[str, Any]],
    corpus_dirs: list[str],
    baseline_commit: str,
) -> tuple[Path, str]:
    """Emit the human-readable markdown report and return (path, verdict)."""
    n_docs = len(doc_results)
    n_with_run = sum(1 for d in doc_results if d['formcalc_run'] > 0)
    n_with_err = sum(1 for d in doc_results if d['formcalc_errors'] > 0)
    total_run = sum(d['formcalc_run'] for d in doc_results)
    total_err = sum(d['formcalc_errors'] for d in doc_results)
    total_events = sum(len(d['events']) for d in doc_results)

    if n_docs < 15:
        verdict = f'XFA_QF1_C_BLOCKED_corpus_too_small ({n_docs} docs)'
    elif total_err == 0 and total_events == 0:
        verdict = 'XFA_QF1_C_NO_RESIDUAL'
    else:
        verdict = 'XFA_QF1_C_FORMCALC_SCAN_READY'

    lines: list[str] = [
        '# XFA-QF1-C — FormCalc Residual Scan Report',
        '',
        '**Agent:** QF1-C  **Cluster:** FC-01 (FormCalc residual scan unmapped)',
        f'**Baseline:** `{baseline_commit}`',
        '**Mode:** `XFA_FORMCALC_DEBUG=1 XFA_JS_EXECUTION_MODE=sandboxed`',
        '',
        '## Context',
        '',
        'Phase 1 (`XFA_RESIDUAL_DEFECT_MAP.md`) flagged FC-01 — FormCalc',
        'residual scan currently un-instrumented. This is the **first**',
        'dedicated FormCalc-only scan. It is built on a small, additive',
        'debug surface in `crates/pdf-xfa/src/dynamic.rs::execute_formcalc_script`',
        'that emits a single stderr line per FormCalc failure when',
        '`XFA_FORMCALC_DEBUG=1` is set:',
        '',
        '```',
        'XFA_FORMCALC_DEBUG stage=<lexer|parser|interpreter> '
        'activity="<initialize|calculate|...>" message="<error display>"',
        '```',
        '',
        'Default off. Zero observable behaviour change otherwise.',
        '',
        '## Corpus',
        '',
    ]
    for d in corpus_dirs:
        lines.append(f'- `{d}`')
    lines += [
        '',
        '## Headline counters',
        '',
        '| Metric | Value |',
        '|--------|------:|',
        f'| Docs scanned | {n_docs} |',
        f'| Docs with `formcalc_run > 0` | {n_with_run} |',
        f'| Docs with `formcalc_errors > 0` | {n_with_err} |',
        f'| Total FormCalc scripts run | {total_run} |',
        f'| Total FormCalc errors | {total_err} |',
        f'| Total debug events captured | {total_events} |',
        '',
        '## Per-document inventory',
        '',
        '| SHA256 (12) | basename | fc_run | fc_errors | skip_rate | exit | ms |',
        '|-------------|----------|-------:|----------:|----------:|-----:|---:|',
    ]
    for d in sorted(doc_results, key=lambda x: (-x['formcalc_errors'], -x['formcalc_run'])):
        lines.append(
            f'| `{d["doc_sha256"][:12]}` | {d["doc_basename"]} | '
            f'{d["formcalc_run"]} | {d["formcalc_errors"]} | '
            f'{d["formcalc_skip_rate"]:.2f} | {d["exit_code"]} | '
            f'{d["elapsed_ms"]} |'
        )

    # --- Clusters ---
    def _cluster_table(title: str, rows: list[dict[str, Any]], note: str = '') -> list[str]:
        out = ['', f'## {title}', '']
        if note:
            out += [note, '']
        if not rows:
            out += ['_No occurrences captured._', '']
            return out
        out += ['| name | docs | occurrences | sample |',
                '|------|-----:|------------:|--------|']
        for r in rows[:20]:
            sample = (r.get('sample_messages') or [''])[0]
            sample = sample.replace('|', '\\|')[:90]
            out.append(f'| `{r["name"]}` | {r["docs"]} | {r["occurrences"]} | `{sample}` |')
        return out

    lines += _cluster_table(
        'Cluster by stage',
        clusters['by_stage'],
        '`lexer` / `parser` errors indicate scripts our front-end refuses; '
        '`interpreter` errors indicate semantically reachable scripts that '
        'fail at runtime.',
    )
    lines += _cluster_table(
        'Cluster by error type',
        clusters['by_error_type'],
        'Buckets follow `FormCalcError` variants from '
        '`crates/formcalc-interpreter/src/error.rs`.',
    )
    lines += _cluster_table(
        'Cluster by function (UnknownFunction + ArityError)',
        clusters['by_function'],
        'Function names parsed out of `Unknown function: NAME` and '
        '`Wrong number of arguments for NAME`. Use this list to size '
        'a Wave-2 builtin closure batch.',
    )

    # --- Wave 2 candidates ---
    lines += ['', '## Wave-2 candidate map', '']
    if not wave2:
        lines += [
            '_No FormCalc function-level residuals found on this corpus._',
            '',
            'Either the FormCalc builtin set is sufficient for the documents',
            'tested, or the scripts that would fail are gated out by a',
            'preceding lexer/parser stage. See cluster-by-stage above.',
            '',
        ]
    else:
        lines += [
            '| rank | function | docs | occurrences | impact | action |',
            '|-----:|----------|-----:|------------:|-------:|--------|',
        ]
        for i, c in enumerate(wave2, 1):
            lines.append(
                f'| {i} | `{c["function"]}` | {c["docs"]} | '
                f'{c["occurrences"]} | {c["impact_score"]} | '
                f'{c["wave2_action"]} |'
            )

    # --- Acceptance + verdict ---
    lines += [
        '',
        '## Acceptance checklist',
        '',
        f'- Scan over ≥ 15 docs: {"PASS" if n_docs >= 15 else "FAIL"} ({n_docs})',
        '- Produces clusters by stage / error_type / function: '
        f'{"PASS" if any(clusters[k] for k in ("by_stage","by_error_type","by_function")) or total_events == 0 else "FAIL"}',
        f'- Top-3 clusters listed when present: {"PASS" if (total_events == 0 or len(clusters["by_error_type"]) > 0) else "FAIL"}',
        '- Clean-state report path when zero errors: '
        f'{"PASS (no residual)" if total_err == 0 and total_events == 0 else "N/A (errors present)"}',
        '',
        '## Verdict',
        '',
        f'**`{verdict}`**',
        '',
        '## Reproducible command',
        '',
        '```bash',
        'python3 scripts/xfa_formcalc_residual_scan.py \\',
        '    --binary ./target/release/pdfluent \\',
        '    --corpus-dir crates/xfa-golden-tests/golden \\',
        '    --output-dir benchmarks/runs/xfa_enterprise_plan/quality_factory_v1',
        '```',
        '',
    ]

    report = output_dir / 'QF1_C_FORMCALC_RESIDUAL_REPORT.md'
    report.write_text('\n'.join(lines))
    return report, verdict


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main() -> int:
    parser = argparse.ArgumentParser(description='XFA-QF1-C FormCalc residual scan')
    parser.add_argument('--binary', default='./target/release/pdfluent')
    parser.add_argument('--corpus-dir', default='crates/xfa-golden-tests/golden')
    parser.add_argument('--extra-corpus-dir', default=None)
    parser.add_argument(
        '--output-dir',
        default='benchmarks/runs/xfa_enterprise_plan/quality_factory_v1',
    )
    parser.add_argument('--max-docs', type=int, default=50)
    parser.add_argument(
        '--baseline-commit',
        default='c70c2a308ad5a81da7b2f98c5bb53c5321c808a7',
        help='Baseline commit hash recorded in the JSON output.',
    )
    args = parser.parse_args()

    output_dir = Path(args.output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)

    binary = os.path.abspath(args.binary)
    if not os.path.isfile(binary):
        print(f'ERROR: binary not found: {binary}', file=sys.stderr)
        return 1

    pdf_paths: list[str] = []
    corpus_dirs: list[str] = []

    corpus_dir = Path(args.corpus_dir)
    if corpus_dir.exists():
        pdfs = sorted(corpus_dir.glob('*.pdf'))
        pdf_paths.extend(str(p) for p in pdfs)
        corpus_dirs.append(args.corpus_dir)

    if args.extra_corpus_dir:
        extra = Path(args.extra_corpus_dir)
        if extra.exists():
            extra_pdfs = sorted(extra.glob('*.pdf'))
            sample = extra_pdfs[: max(0, args.max_docs - len(pdf_paths))]
            pdf_paths.extend(str(p) for p in sample)
            corpus_dirs.append(
                f'{args.extra_corpus_dir} (sampled {len(sample)} of {len(extra_pdfs)})'
            )

    pdf_paths = pdf_paths[: args.max_docs]
    if not pdf_paths:
        print('ERROR: no PDFs found in corpus dirs', file=sys.stderr)
        return 1

    print(f'Scanning {len(pdf_paths)} docs with FormCalc debug enabled...')
    doc_results: list[dict[str, Any]] = []
    started = time.monotonic()
    with tempfile.TemporaryDirectory(prefix='qf1c_formcalc_scan_') as tmpdir:
        for i, pdf in enumerate(pdf_paths, 1):
            print(f'  [{i}/{len(pdf_paths)}] {Path(pdf).name}', end=' ', flush=True)
            try:
                result = run_doc(binary, pdf, tmpdir)
            except Exception as exc:  # noqa: BLE001 - capture and continue
                print(f'EXCEPTION: {exc}')
                continue
            doc_results.append(result)
            print(
                f'fc_run={result["formcalc_run"]} '
                f'fc_err={result["formcalc_errors"]} '
                f'events={len(result["events"])} '
                f'exit={result["exit_code"]} '
                f'{result["elapsed_ms"]}ms'
            )
    elapsed = time.monotonic() - started

    clusters = build_clusters(doc_results)
    wave2 = wave2_candidates(clusters)

    json_path = write_json(
        output_dir, doc_results, clusters, wave2,
        corpus_dirs, args.baseline_commit,
    )
    md_path, verdict = write_report(
        output_dir, doc_results, clusters, wave2,
        corpus_dirs, args.baseline_commit,
    )

    print()
    print(f'Wrote {json_path}')
    print(f'Wrote {md_path}')
    print(f'Verdict: {verdict}  (elapsed {elapsed:.1f}s)')
    return 0


if __name__ == '__main__':
    sys.exit(main())
