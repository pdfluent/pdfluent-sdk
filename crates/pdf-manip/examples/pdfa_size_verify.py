#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Recheck saved shipping-route outputs and the pinned SSIM corpus.

ssim: convert through two pdfa_convert_real binaries; render page 1 at the
existing gate's 150 dpi, use its compute_ssim and feed check_ssim_gate.py.
The source-to-output scores and before-to-after scores are separate.

convert-one: adapter for text_retention_gate.py's runner-shaped interface.
Copies an already measured output from PDFA_SIZE_OUTPUT; never substitutes a
second conversion implementation. No external repair is used.
"""
import argparse
import concurrent.futures
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[3]


def load_script(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def adapter(args):
    p = argparse.ArgumentParser()
    p.add_argument('-p', required=True)
    p.add_argument('-o', required=True)
    a = p.parse_args(args)
    directory = Path(os.environ['PDFA_SIZE_OUTPUT'])
    shutil.copyfile(directory / Path(a.p).name, a.o)


def run(command, timeout=600):
    return subprocess.run(command, capture_output=True, check=True, timeout=timeout)


def ssim(args):
    p = argparse.ArgumentParser()
    p.add_argument('--corpus', type=Path, required=True)
    p.add_argument('--before', type=Path, required=True)
    p.add_argument('--after', type=Path, required=True)
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--workers', type=int, default=2)
    p.add_argument('--reuse', action='store_true', help='recheck existing conversions from these same binaries')
    a = p.parse_args(args)
    manifest = json.loads((ROOT / 'corpus/SSIM_GATE_MANIFEST.json').read_text())
    gate = load_script('gate_ssim', ROOT / 'scripts/run_gate_ssim.py')
    a.out.mkdir(parents=True, exist_ok=True)
    # Verify the ENTIRE set before conversion; missing inputs are an error.
    for e in manifest['entries']:
        data = (a.corpus / e['file']).read_bytes()
        if hashlib.sha256(data).hexdigest() != e['sha256']:
            raise ValueError(f"checksum mismatch: {e['file']}")

    def one(entry):
        name = entry['file']
        directory = a.out / Path(name).stem
        directory.mkdir(exist_ok=True)
        source = a.corpus / name
        before, after = directory / 'before.pdf', directory / 'after.pdf'
        row = {'file': name}
        try:
            # Existing outputs may be rechecked with --out pointing to a completed run.
            if not a.reuse or not before.exists():
                run([str(a.before), str(source), str(before)])
            if not a.reuse or not after.exists():
                run([str(a.after), str(source), str(after)])
            images = []
            for label, pdf in [('source', source), ('before', before), ('after', after)]:
                png = directory / f'{label}.png'
                render = subprocess.run(['mutool', 'draw', '-r', '150', '-o', str(png), str(pdf), '1'], capture_output=True, timeout=600)
                row[f'{label}_render_exit'] = render.returncode
                if not png.is_file():
                    raise ValueError('renderer produced no image')
                images.append(png)
            row['before_ssim'] = round(gate.compute_ssim(str(images[0]), str(images[1])), 4)
            row['after_ssim'] = round(gate.compute_ssim(str(images[0]), str(images[2])), 4)
            row['pair_ssim'] = round(gate.compute_ssim(str(images[1]), str(images[2])), 4)
            from PIL import Image
            with Image.open(images[1]) as b, Image.open(images[2]) as c:
                row['pixels_identical'] = b.size == c.size and b.mode == c.mode and b.tobytes() == c.tobytes()
            texts = [subprocess.run(['mutool', 'draw', '-F', 'txt', str(pdf)], capture_output=True, timeout=600) for pdf in (before, after)]
            row['text_exit_codes'] = [t.returncode for t in texts]
            row['text_identical'] = texts[0].stdout == texts[1].stdout and texts[0].returncode == texts[1].returncode
            row['bytes_before'] = before.stat().st_size
            row['bytes_after'] = after.stat().st_size
        except (OSError, subprocess.SubprocessError, ValueError) as e:
            # Do not silently drop crashes or partial renders from the result.
            row['error'] = type(e).__name__
        print(name, row, flush=True)
        return row

    with concurrent.futures.ThreadPoolExecutor(max_workers=a.workers) as pool:
        rows = list(pool.map(one, manifest['entries']))
    for stage in ['before', 'after']:
        result = {'results': [
            {'file': r['file'], 'render': {
                'status': 'pass' if r.get(f'{stage}_ssim', 0) >= 0.95 else 'fail',
                'ssim': r.get(f'{stage}_ssim', 0),
            }} for r in rows
        ]}
        path = a.out / f'{stage}-ssim.json'
        path.write_text(json.dumps(result, indent=2) + '\n')
        verdict = subprocess.run([sys.executable, str(ROOT / 'scripts/check_ssim_gate.py'), '--result', str(path), '--summary-json', str(a.out / f'{stage}-gate.json')], capture_output=True, text=True)
        (a.out / f'{stage}-gate.txt').write_text(verdict.stdout + verdict.stderr)
    summary = {
        'documents': len(rows),
        'errors': sum('error' in r for r in rows),
        'before_changed': sum(r.get('before_ssim', 0) < 0.95 for r in rows),
        'after_changed': sum(r.get('after_ssim', 0) < 0.95 for r in rows),
        'pixels_identical': sum(r.get('pixels_identical', False) for r in rows),
        'text_identical': sum(r.get('text_identical', False) for r in rows),
        'rows': rows,
    }
    (a.out / 'comparison.json').write_text(json.dumps(summary, indent=2) + '\n')
    print(json.dumps({k: v for k, v in summary.items() if k != 'rows'}, indent=2))
    if summary['errors'] or summary['after_changed'] > summary['before_changed'] or summary['text_identical'] != len(rows):
        return 1
    return 0


def compare(args):
    p = argparse.ArgumentParser()
    p.add_argument('--before', type=Path, required=True)
    p.add_argument('--after', type=Path, required=True)
    p.add_argument('--out', type=Path, required=True)
    a = p.parse_args(args)
    before = json.loads((a.before / 'compare.json').read_text())
    after = json.loads((a.after / 'compare.json').read_text())
    br = {r['name']: r for r in before['rows']}
    ar = {r['name']: r for r in after['rows']}
    if br.keys() != ar.keys():
        raise ValueError('different measurement sets')
    regressions, text_changed = [], []
    for name in sorted(br):
        b, c = br[name], ar[name]
        for field in ['converted', 'judged', 'conformant']:
            if b[field] and not c[field]:
                regressions.append([name, field])
        for field in ['wordRetention', 'charRetention']:
            if b[field] is not None and (c[field] is None or c[field] < b[field]):
                regressions.append([name, field])
        if b['converted'] and c['converted']:
            texts = [subprocess.run(['mutool', 'draw', '-F', 'txt', str(directory / name)], capture_output=True, timeout=600) for directory in (a.before, a.after)]
            if texts[0].stdout != texts[1].stdout or texts[0].returncode != texts[1].returncode:
                text_changed.append(name)
    result = {'documents': len(br), 'regressions': regressions, 'text_changed': text_changed,
              'before_changed': before['render_pages_changed'], 'after_changed': after['render_pages_changed'],
              'bytes_before': before['bytes_out'], 'bytes_after': after['bytes_out']}
    a.out.write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2))
    return int(bool(regressions or text_changed or result['after_changed'] > result['before_changed']))


if __name__ == '__main__':
    commands = {'convert-one': adapter, 'ssim': ssim, 'compare': compare}
    if len(sys.argv) < 2 or sys.argv[1] not in commands:
        sys.exit('usage: pdfa_size_verify.py {convert-one|ssim|compare} ...')
    sys.exit(commands[sys.argv[1]](sys.argv[2:]))
