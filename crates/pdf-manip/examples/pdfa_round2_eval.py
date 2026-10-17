#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""All-page, ordered-text supplement to the unchanged four-axis benchmark.

Consumes saved outputs of pdfa_convert_real or the commercial converter being
evaluated. It never changes the conversion route or drops failed inputs. JSONL
is flushed per document; PNG scratch files are removed after measurement.
Requires the same numpy/Pillow/scikit-image environment as the SSIM gate.

ONE CANDIDATE RENDERS TWO SETS, NOT THREE (#454).

`--before` is optional, and a run that does not name a second candidate renders
the source and the candidate -- twice per page, which is what the comparison
needs. It used to be required, so the wave passed the same directory twice:
every page of the corpus was rendered a third time, and the columns that third
render bought were copies. `output_pixels_identical` in particular compared a
file with itself and counted every page of the corpus as unchanged, which is
true for any converter including a broken one. Both are gone from a
single-candidate run: the summary reports a before/after comparison only when
there were two candidates to compare.

A second candidate can also be borrowed rather than rendered. `--baseline-result`
takes the `all-pages.json` a previous run left in its artifact and lifts its
`after_*` columns into this run's `before_*` columns, pinned on the source
digest -- the previous run measured that candidate against the same bytes at the
same dpi, so those columns are already the answer. Two arms of one experiment
then cost one render of the corpus each instead of two.

`--reuse-result` is a different and older thing: it skips a document entirely
when the candidate is object-identical to the one a prior run measured.

TWO CANDIDATES SHARE ONE SOURCE RENDER (#547).

`--also-after` names a second candidate and `--also-out` the file it gets. Both
are measured in one pass, so the source is rendered, read and text-extracted
ONCE for the two of them: three sets for two candidates, where two runs of this
script rendered four. The two arms of the width-fitting experiment were exactly
that, and the source render they paid for twice is the same bytes at the same
dpi both times.

Each candidate keeps its own file, and in that file it is `after`: the axis
(scripts/pdfa/pdfa_fidelity_axis.py) judges `after_*` columns and knows nothing
about a second arm. A file also carries no trace of the candidate it does not
name -- not its columns, not its errors, and not the pages only it has -- so
what either file says is what a run measuring that candidate alone would have
said.

THE EXIT CODE ANSWERS ONE QUESTION: DID THE MEASUREMENT HAPPEN.

A document the converter refused carries errors and no renders, and on a drawn
set there is always one. Reporting that as a failed run makes the step red on a
measurement that succeeded completely; what a refusal means is judged per
document by the ratchet. The count travels in `documents_with_errors`.
"""
import argparse
from collections import Counter
from concurrent.futures import ThreadPoolExecutor
from contextlib import ExitStack
import difflib
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tempfile
import time


def run(args, timeout):
    return subprocess.run(args, capture_output=True, timeout=timeout)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def text_result(pdf, timeout):
    r = run(['mutool', 'draw', '-F', 'txt', '-o', '-', str(pdf)], timeout)
    return {'exit': r.returncode, 'sha256': sha(r.stdout),
            'warning_sha256': sha(r.stderr), 'bytes': len(r.stdout)}, r.stdout.decode('utf-8', 'replace')


def text_scores(source, target):
    a, b = source.split(), target.split()
    ca, cb = Counter(a), Counter(b)
    # SequenceMatcher's popular-token heuristic bounds pathological repeated
    # text. This is an explicitly approximate order check, not edit distance.
    ordered = sum(m.size for m in difflib.SequenceMatcher(a=a, b=b, autojunk=True).get_matching_blocks())
    chars_a = Counter(''.join(a)); chars_b = Counter(''.join(b))
    return {'word_retention': sum((ca & cb).values()) / len(a) if a else None,
            'character_retention': sum((chars_a & chars_b).values()) / sum(chars_a.values()) if chars_a else None,
            'ordered_word_retention_approx': ordered / len(a) if a else None,
            'words_missing': sum((ca - cb).values()),
            'replacement_characters': target.count('\ufffd')}


def rendered_labels(before, after, other=None):
    """Which directories a run renders, in order, one label each.

    A run whose `--before` is absent, or is the directory `--after` already
    names, has ONE candidate. Rendering it twice under two names costs a third
    of the run: the eval renders source, before and after, so the third set is
    a duplicate of the second. What it buys is `before_*` columns that are
    copies of the `after_*` ones and an `output_pixels_identical` that compares
    a file with itself.

    `other` is a second candidate measured in the same pass (#547): it costs
    its own set and not the source again. The caller has already refused the
    spellings that would make it a copy of `after`.
    """
    labels = ['source']
    if before is not None and Path(before).resolve() != Path(after).resolve():
        labels.append('before')
    labels.append('after')
    if other is not None:
        labels.append('other')
    return labels


# The per-document and per-page columns a prior run's candidate answers for.
BORROWED_COLUMNS = ('bytes', 'sha256', 'text', 'text_scores', 'page_count',
                    'info_exit', 'render_exit', 'render_warning_sha256',
                    'rendered_pages')
BORROWED_PAGE_COLUMNS = ('mae', 'ssim', 'missing', 'unreadable',
                         'dimensions_changed')


def borrow_before(previous, source_sha256, dpi):
    """A prior run's candidate as this run's `before`, without rendering it.

    The prior run measured its candidate against the same source at the same
    dpi, so its `after_*` columns ARE this run's `before_*` columns. The pin is
    the source digest: a row measured against different input bytes says
    nothing about this document, and a borrowed column that quietly answers for
    a different file is worse than a column that is not there. A row that
    carried errors is not borrowed from either -- its candidate may never have
    rendered.

    Returns None when nothing can be borrowed, and the caller renders instead.
    """
    if not previous or previous.get('errors') or previous.get('dpi') != dpi:
        return None
    if not previous.get('after_sha256'):
        return None
    if not source_sha256 or previous.get('source_sha256') != source_sha256:
        return None
    columns = {f'before_{key}': previous[f'after_{key}']
               for key in BORROWED_COLUMNS if f'after_{key}' in previous}
    if 'before_sha256' not in columns:
        return None
    pages = {}
    for page in previous.get('pages', []):
        number = page.get('page')
        if number is None:
            continue
        borrowed = {f'before_{key}': page[f'after_{key}']
                    for key in BORROWED_PAGE_COLUMNS if f'after_{key}' in page}
        if borrowed:
            pages[number] = borrowed
    return {'columns': columns, 'pages': pages}


# Every label that names a candidate rather than the input.
CANDIDATES = ('before', 'after', 'other')
# Before-against-after, so they belong to the file that has both.
COMPARISONS = ('output_pixels_identical', 'output_text_identical')


def project(row, drop, candidate):
    """One candidate's row, as a run that measured only that candidate wrote it.

    A pass measuring two candidates holds both in one row; the files it writes
    hold one each. What the other candidate contributed leaves entirely: its
    columns, its errors, and the pages that only it rendered -- a page the
    second arm has and the first does not would otherwise appear in the first
    arm's file as a page whose every column is missing.

    The candidate that stays is renamed to `after`, because that is the one
    name the axis reads and the file has only one candidate to answer for. The
    claim this makes possible is checkable: the file is expected to be what the
    separate run produced, not merely close to it.

    `drop` names the labels, not the columns: a `before` this run BORROWED was
    never rendered and still belongs in the first file, so what leaves is what
    the other file answers for.
    """
    pages = row.get('_pages', {}).get(candidate)
    foreign = tuple(f'{label}_' for label in drop)
    prefix = f'{candidate}_'

    def rename(item):
        return {('after_' + key[len(prefix):] if key.startswith(prefix) else key): value
                for key, value in item.items() if not key.startswith(foreign)}

    projected = {}
    for key, value in row.items():
        if key == '_pages' or key.startswith(foreign):
            continue
        if key in COMPARISONS and 'before' in drop:
            continue
        if key == 'errors':
            value = [error for error in value
                     if error.split(':', 1)[0] not in drop]
        elif key == 'pages':
            value = [item for item in value
                     if pages is None or item['page'] in pages]
            value = [{k: v for k, v in item.items()
                      if k not in COMPARISONS or 'before' not in drop}
                     for item in value]
        projected[key] = value
    if candidate == 'after':
        return projected

    def renamed_error(error):
        label, colon, rest = error.partition(':')
        return 'after' + colon + rest if label == candidate else error

    projected = rename(projected)
    projected['pages'] = [rename(item) for item in projected['pages']]
    projected['errors'] = [renamed_error(error) for error in projected['errors']]
    return projected


def colliding_outputs(named):
    """The files two outputs would both write, as (paths, the flags that name it).

    A summary path names two more files by suffix: the JSONL the rows stream
    into and the inputs manifest. So two summaries that differ only in their
    suffix -- `--out result.json --also-out result.txt` -- are two different
    files and one JSONL, opened twice and written by both candidates, with an
    exit code that finds a file there and calls the measurement done. Comparing
    only the summaries misses it; so this compares everything either output
    writes, a summary against its own derived names included (`--out x.jsonl`
    is a summary that overwrites its own rows).
    """
    seen, collisions = {}, []
    for flag, path in named:
        for derived in (path, path.with_suffix('.jsonl'),
                        path.with_suffix('.inputs.json')):
            resolved = Path(derived).resolve()
            if resolved in seen:
                collisions.append((resolved, seen[resolved], flag))
            else:
                seen[resolved] = flag
    return collisions


def summarise(rows, labels, dpi, seconds, run_labels=None):
    """The totals, with a comparison reported only when there was one.

    `run_labels` is what the pass rendered when it measured a second candidate
    alongside this one; `labels` stays what THIS file's columns rest on.

    `output_pixels_identical` and `output_text_identical` are before-against-
    after: they exist per row only when both candidates were measured. Summing
    a key that is not there would report nought identical pages for a
    single-candidate run, and reporting the count of a run that compared a
    directory with itself is worse still -- it reads as "the conversion changed
    no pixels" and is a tautology (#454). Both are derived from what the rows
    actually carry.
    """
    pages = [p for r in rows for p in r['pages']]
    summary = {
        'documents': len(rows),
        'candidates': len([label for label in labels if label != 'source']),
        'rendered_sets_per_document': len(labels),
        'rendered': list(labels),
        'documents_reused_after_identity_check': sum('reused_from' in r for r in rows),
        'documents_with_borrowed_before': sum('before_borrowed_from' in r for r in rows),
        'documents_with_errors': sum(bool(r['errors']) for r in rows),
        'pages': len(pages),
    }
    # WHAT THIS FILE RESTS ON, AND WHAT THE RUN PAID (#547). The columns here
    # answer for two sets; the pass that produced them rendered three and wrote
    # a second file from the third. Reporting only the two would understate the
    # run, and reporting only the three would attribute to this candidate a set
    # it does not name.
    if run_labels is not None and len(run_labels) != len(labels):
        summary['shared_source_render'] = True
        summary['run_rendered_sets_per_document'] = len(run_labels)
        summary['run_rendered'] = list(run_labels)
    compared_pages = [p for p in pages if 'output_pixels_identical' in p]
    if compared_pages:
        summary['output_pixels_identical'] = sum(
            p['output_pixels_identical'] for p in compared_pages)
    compared_rows = [r for r in rows if 'output_text_identical' in r]
    if compared_rows:
        summary['output_text_identical'] = sum(
            r['output_text_identical'] for r in compared_rows)
    summary['metric'] = ('all pages; RGB MAE and grayscale SSIM; approximate '
                         'ordered word retention')
    summary['dpi'] = dpi
    summary['seconds'] = seconds
    return summary


def exit_code(rows, jsonl):
    """A measurement that ran is not a failure because a document refused.

    This used to return 1 whenever any document carried an error, and on a
    drawn set there is always one: a document the converter refuses has no
    output PDF to render, so it carries three errors and the whole step went
    red on a measurement that had completely succeeded. The tuning1000 run
    34344290509 produced every file -- compare.json, all-pages.json, the
    artifact -- and reported failure. What a refusal MEANS is a judgement, and
    it is made per document by scripts/ci/loop/pdfa_ratchet.py, where a
    document that converted last round and refuses this one is red.

    So the count travels in the summary (`documents_with_errors`) and the exit
    code answers one question only: did the measurement happen. A run that
    wrote no rows, or lost its JSONL, measured nothing and says so.
    """
    if not rows:
        return 1
    return 0 if Path(jsonl).is_file() else 1


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--source', type=Path, required=True)
    p.add_argument('--before', type=Path,
                   help='Second candidate; omit for a single-candidate run')
    p.add_argument('--after', type=Path, required=True)
    p.add_argument('--list', type=Path, required=True)
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--dpi', type=int, default=72)
    p.add_argument('--workers', type=int, default=2)
    p.add_argument('--timeout', type=int, default=600)
    p.add_argument('--baseline-result', type=Path,
                   help="A prior run's all-pages.json; its after_* columns "
                        "become this run's before_* columns where the source "
                        'digest and dpi match')
    p.add_argument('--also-after', type=Path,
                   help='A second candidate, measured against the same source '
                        'renders in the same pass; it gets its own file')
    p.add_argument('--also-out', type=Path,
                   help='Where the second candidate\'s result goes; its '
                        'columns are after_* there, as in any single-candidate run')
    p.add_argument('--reuse-result', type=Path, help='Prior result; reuse only after strict decoded-object identity')
    p.add_argument('--reuse-after', type=Path, help='PDFs measured as after in the prior result')
    a = p.parse_args()
    names = [n.strip() for n in a.list.read_text().splitlines() if n.strip()]
    if len(set(names)) != len(names) or any(Path(n).name != n for n in names):
        p.error('the list must contain unique basenames')
    if bool(a.reuse_result) != bool(a.reuse_after):
        p.error('both reuse arguments are required together')
    if bool(a.also_after) != bool(a.also_out):
        p.error('both second-candidate arguments are required together')
    if a.also_after:
        # THE SAME DIRECTORY TWICE IS THE #454 BUG IN ANOTHER SPELLING. It
        # would render a third set for a copy of the second and write two files
        # that only look like two measurements. `--source` is in the list for a
        # worse reason: a second candidate that IS the input renders the input
        # twice and files the second render under `after_*`, where zero MAE and
        # perfect SSIM read as a flawless conversion and are a tautology.
        named = {'--source': a.source, '--after': a.after, '--before': a.before}
        for flag, other in named.items():
            if other is not None and Path(other).resolve() == Path(a.also_after).resolve():
                p.error(f'--also-after names the directory {flag} already names; '
                        'the same directory twice is not a second candidate')
        if a.reuse_result:
            p.error('--reuse-result is pinned to one candidate; measure the '
                    'second candidate in its own run')
    labels = rendered_labels(a.before, a.after, a.also_after)
    if a.before is not None and 'before' not in labels:
        print('note: --before names the directory --after already names; '
              'measuring one candidate and rendering two sets', flush=True)
    if a.baseline_result and 'before' in labels:
        p.error('--baseline-result borrows the before columns; a run that '
                'renders its own before does not need them')
    prior = {}
    if a.reuse_result:
        prior = {r['file']: r for r in json.loads(a.reuse_result.read_text())['rows']}
    baseline = {}
    if a.baseline_result:
        baseline = {r['file']: r for r in json.loads(a.baseline_result.read_text())['rows']}
        baseline_digest = sha(a.baseline_result.read_bytes())
    # (file, the labels its columns rest on, the labels it drops, the candidate
    # its after_* columns are).
    outputs = [(a.out, [label for label in labels if label != 'other'],
                ('other',), 'after')]
    if 'other' in labels:
        outputs.append((a.also_out, ['source', 'after'],
                        ('before', 'after'), 'other'))
    collisions = colliding_outputs(
        [('--out', a.out)] + ([('--also-out', a.also_out)] if a.also_out else []))
    if collisions:
        path, first, second = collisions[0]
        if first == second:
            p.error(f'{first} names {path}, which is where this run streams its '
                    'rows; the summary would overwrite them')
        p.error(f'{first} and {second} both write {path}; two candidates '
                'writing one file are not two measurements')
    manifest = [{'file': n, 'sha256': sha((a.source / n).read_bytes())} for n in names]
    for path, *_ in outputs:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.with_suffix('.inputs.json').write_text(json.dumps(manifest, indent=2) + '\n')

    def one(name):
        if name in prior and not prior[name]['errors'] and prior[name]['dpi'] == a.dpi:
            previous = prior[name]
            try:
                from pdfa_round2_identity import fingerprint
                paths = {'source': a.source / name, 'after': a.reuse_after / name}
                if 'before' in labels:
                    paths['before'] = a.before / name
                if all(sha(path.read_bytes()) == previous.get(label + '_sha256')
                       for label, path in paths.items()):
                    old_identity = fingerprint(paths['after'])
                    if old_identity == fingerprint(a.after / name):
                        row = json.loads(json.dumps(previous))
                        if 'before' not in labels:
                            # A row measured with three sets carries before_*
                            # columns this run did not measure. Carrying them
                            # forward would put a column in the file that no
                            # candidate of this run answers for.
                            row = {k: v for k, v in row.items()
                                   if not k.startswith('before_')}
                            row['pages'] = [{k: v for k, v in page.items()
                                             if not k.startswith('before_')
                                             and k != 'output_pixels_identical'}
                                            for page in row['pages']]
                            row.pop('output_text_identical', None)
                        row['reused_from'] = {'result_sha256': sha(a.reuse_result.read_bytes()),
                                              'prior_after_sha256': previous['after_sha256'],
                                              'prior_reuse': previous.get('reused_from'),
                                              'object_identity_sha256': old_identity,
                                              'note': 'Prior metrics and diagnostic exit codes; complete Root/Info graph verified identical'}
                        row['after_sha256'] = sha((a.after / name).read_bytes())
                        row['after_bytes'] = (a.after / name).stat().st_size
                        return row
            except Exception:
                # Parse or identity uncertainty requires fresh measurements.
                pass
        import numpy as np
        from PIL import Image
        from skimage.metrics import structural_similarity
        row = {'file': name, 'pages': [], 'dpi': a.dpi, 'errors': []}
        texts = {}; images = {}; counts = {}
        folders = {'source': a.source, 'before': a.before, 'after': a.after,
                   'other': a.also_after}
        with tempfile.TemporaryDirectory(prefix='pdfa-pages-', dir=a.out.parent) as tmp:
            tmp = Path(tmp)
            for label in labels:
                pdf = folders[label] / name
                if not pdf.is_file():
                    row['errors'].append(f'{label}: missing PDF'); continue
                row[f'{label}_bytes'] = pdf.stat().st_size
                row[f'{label}_sha256'] = sha(pdf.read_bytes())
                try:
                    t, texts[label] = text_result(pdf, a.timeout)
                    row[f'{label}_text'] = t
                    # Independent declared page count catches partial renders.
                    info = run(['pdfinfo', str(pdf)], a.timeout)
                    found = re.search(rb'^Pages:\s+(\d+)', info.stdout, re.M)
                    counts[label] = int(found[1]) if found else None
                    row[f'{label}_page_count'] = counts[label]
                    row[f'{label}_info_exit'] = info.returncode
                    render = run(['mutool', 'draw', '-r', str(a.dpi), '-o', str(tmp / f'{label}-%d.png'), str(pdf)], a.timeout)
                    row[f'{label}_render_exit'] = render.returncode
                    row[f'{label}_render_warning_sha256'] = sha(render.stderr)
                    images[label] = {int(f.stem.split('-')[-1]): f for f in tmp.glob(f'{label}-*.png')}
                    row[f'{label}_rendered_pages'] = len(images[label])
                    if counts[label] is None or len(images[label]) != counts[label]:
                        row['errors'].append(f'{label}: missing page count or partial render')
                except (OSError, subprocess.SubprocessError, ValueError) as error:
                    row['errors'].append(f'{label}: {type(error).__name__}')
            borrowed = None
            if 'before' not in labels and baseline:
                borrowed = borrow_before(baseline.get(name),
                                         row.get('source_sha256'), a.dpi)
            if borrowed:
                row.update(borrowed['columns'])
                row['before_borrowed_from'] = {
                    'result_sha256': baseline_digest,
                    'source_sha256': row['source_sha256'],
                    'note': 'before_* columns are the prior run\'s after_* '
                            'columns for this document, measured against the '
                            'same source bytes at the same dpi; no pixels of '
                            'that candidate exist in this run, so there is no '
                            'before-against-after pixel comparison'}
            # The gate is per file, not per pass: a reader that failed on the
            # second candidate must not remove the first candidate's scores,
            # which a run measuring it alone would have had.
            first = [label for label in labels if label != 'other']
            if len(set(texts) & set(first)) == len(first):
                if 'before' in texts:
                    row['before_text_scores'] = text_scores(texts['source'], texts['before'])
                row['after_text_scores'] = text_scores(texts['source'], texts['after'])
                if 'before_text' in row and 'after_text' in row:
                    row['output_text_identical'] = row['before_text']['sha256'] == row['after_text']['sha256']
            if 'other' in labels and {'source', 'other'} <= set(texts):
                row['other_text_scores'] = text_scores(texts['source'], texts['other'])
            numbers = set().union(*(set(v) for v in images.values())) if images else set()
            if borrowed:
                numbers |= set(borrowed['pages'])
            for page in sorted(numbers):
                result = {'page': page}
                arrays = {}
                for label in labels:
                    path = images.get(label, {}).get(page)
                    if path is None:
                        result[f'{label}_missing'] = True; continue
                    try:
                        with Image.open(path) as im:
                            arrays[label] = np.asarray(im.convert('RGB'))
                    except (OSError, ValueError):
                        result[f'{label}_unreadable'] = True
                for label in labels:
                    if label == 'source':
                        continue
                    x, y = arrays.get('source'), arrays.get(label)
                    if x is None or y is None: continue
                    if x.shape != y.shape:
                        result[f'{label}_dimensions_changed'] = True; continue
                    result[f'{label}_mae'] = float(np.abs(x.astype(np.float32) - y).mean() / 255)
                    # Gate-style grayscale SSIM; RGB MAE remains alongside it.
                    gx, gy = x.mean(axis=2), y.mean(axis=2)
                    window = min(7, min(gx.shape))
                    if window % 2 == 0: window -= 1
                    if window >= 3:
                        result[f'{label}_ssim'] = float(structural_similarity(gx, gy, data_range=255, win_size=window))
                if borrowed:
                    result.update(borrowed['pages'].get(page, {}))
                b, c = arrays.get('before'), arrays.get('after')
                if b is not None and c is not None:
                    result['output_pixels_identical'] = b.shape == c.shape and bool(np.array_equal(b, c))
                row['pages'].append(result)
            # Which pages belong to which file. Rendered together, the page
            # numbers are the union over both candidates; separately they were
            # not, and a page only the second arm has is not a page the first
            # arm's file says anything about.
            own = {label: set(images.get(label, ())) for label in labels}
            first = own.get('source', set()) | own.get('before', set()) | own.get('after', set())
            if borrowed:
                first |= set(borrowed['pages'])
            row['_pages'] = {'after': first}
            if 'other' in labels:
                row['_pages']['other'] = own.get('source', set()) | own['other']
        return row

    written = [[] for _ in outputs]
    started = time.monotonic()
    with ExitStack() as scope:
        streams = [scope.enter_context(path.with_suffix('.jsonl').open('w'))
                   for path, *_ in outputs]
        pool = scope.enter_context(ThreadPoolExecutor(max_workers=a.workers))
        for row in pool.map(one, names):
            for index, (_, _, drop, candidate) in enumerate(outputs):
                one_candidate = project(row, drop, candidate)
                written[index].append(one_candidate)
                streams[index].write(json.dumps(one_candidate) + '\n')
                streams[index].flush()
            print(row['file'], 'pages', len(row['pages']), 'errors', row['errors'], flush=True)
    seconds = round(time.monotonic() - started, 3)
    status = 0
    for index, (path, keep, _, _) in enumerate(outputs):
        summary = summarise(written[index], keep, a.dpi, seconds, labels)
        summary['rows'] = written[index]
        path.write_text(json.dumps(summary, indent=2) + '\n')
        print(json.dumps({k: v for k, v in summary.items() if k != 'rows'}))
        # A file that was not written is a measurement that did not happen,
        # whichever of the two it is.
        status = max(status, exit_code(written[index], path.with_suffix('.jsonl')))
    return status


if __name__ == '__main__':
    raise SystemExit(main())
