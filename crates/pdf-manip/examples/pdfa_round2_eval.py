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
"""
import argparse
from collections import Counter
from concurrent.futures import ThreadPoolExecutor
import difflib
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tempfile


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


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--source', type=Path, required=True)
    p.add_argument('--before', type=Path, required=True)
    p.add_argument('--after', type=Path, required=True)
    p.add_argument('--list', type=Path, required=True)
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--dpi', type=int, default=72)
    p.add_argument('--workers', type=int, default=2)
    p.add_argument('--timeout', type=int, default=600)
    p.add_argument('--reuse-result', type=Path, help='Prior result; reuse only after strict decoded-object identity')
    p.add_argument('--reuse-after', type=Path, help='PDFs measured as after in the prior result')
    a = p.parse_args()
    names = [n.strip() for n in a.list.read_text().splitlines() if n.strip()]
    if len(set(names)) != len(names) or any(Path(n).name != n for n in names):
        p.error('the list must contain unique basenames')
    if bool(a.reuse_result) != bool(a.reuse_after):
        p.error('both reuse arguments are required together')
    prior = {}
    if a.reuse_result:
        prior = {r['file']: r for r in json.loads(a.reuse_result.read_text())['rows']}
    a.out.parent.mkdir(parents=True, exist_ok=True)
    manifest = [{'file': n, 'sha256': sha((a.source / n).read_bytes())} for n in names]
    a.out.with_suffix('.inputs.json').write_text(json.dumps(manifest, indent=2) + '\n')

    def one(name):
        if name in prior and not prior[name]['errors'] and prior[name]['dpi'] == a.dpi:
            previous = prior[name]
            try:
                from pdfa_round2_identity import fingerprint
                paths = {'source': a.source / name, 'before': a.before / name,
                         'after': a.reuse_after / name}
                if all(sha(path.read_bytes()) == previous[label + '_sha256'] for label, path in paths.items()):
                    old_identity = fingerprint(paths['after'])
                    if old_identity == fingerprint(a.after / name):
                        row = json.loads(json.dumps(previous))
                        row['reused_from'] = {'result_sha256': sha(a.reuse_result.read_bytes()),
                                              'measured_after_sha256': previous['after_sha256'],
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
        with tempfile.TemporaryDirectory(prefix='pdfa-pages-', dir=a.out.parent) as tmp:
            tmp = Path(tmp)
            for label, folder in [('source', a.source), ('before', a.before), ('after', a.after)]:
                pdf = folder / name
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
            if len(texts) == 3:
                row['before_text_scores'] = text_scores(texts['source'], texts['before'])
                row['after_text_scores'] = text_scores(texts['source'], texts['after'])
                row['output_text_identical'] = row['before_text']['sha256'] == row['after_text']['sha256']
            all_pages = sorted(set().union(*(set(v) for v in images.values())))
            for page in all_pages:
                result = {'page': page}
                arrays = {}
                for label in ['source', 'before', 'after']:
                    path = images.get(label, {}).get(page)
                    if path is None:
                        result[f'{label}_missing'] = True; continue
                    try:
                        with Image.open(path) as im:
                            arrays[label] = np.asarray(im.convert('RGB'))
                    except (OSError, ValueError):
                        result[f'{label}_unreadable'] = True
                for label in ['before', 'after']:
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
                b, c = arrays.get('before'), arrays.get('after')
                if b is not None and c is not None:
                    result['output_pixels_identical'] = b.shape == c.shape and bool(np.array_equal(b, c))
                row['pages'].append(result)
        return row

    rows = []
    with a.out.with_suffix('.jsonl').open('w') as stream, ThreadPoolExecutor(max_workers=a.workers) as pool:
        for row in pool.map(one, names):
            rows.append(row); stream.write(json.dumps(row) + '\n'); stream.flush()
            print(row['file'], 'pages', len(row['pages']), 'errors', row['errors'], flush=True)
    summary = {'documents': len(rows), 'documents_reused_after_identity_check': sum('reused_from' in r for r in rows),
               'documents_with_errors': sum(bool(r['errors']) for r in rows),
               'pages': sum(len(r['pages']) for r in rows),
               'output_pixels_identical': sum(p.get('output_pixels_identical', False) for r in rows for p in r['pages']),
               'output_text_identical': sum(r.get('output_text_identical', False) for r in rows),
               'metric': 'all pages; RGB MAE and grayscale SSIM; approximate ordered word retention',
               'dpi': a.dpi, 'rows': rows}
    a.out.write_text(json.dumps(summary, indent=2) + '\n')
    print(json.dumps({k: v for k, v in summary.items() if k != 'rows'}))
    return int(summary['documents_with_errors'] > 0)


if __name__ == '__main__':
    raise SystemExit(main())
