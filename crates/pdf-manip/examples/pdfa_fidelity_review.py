#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Compare a fixed page sample with both MuPDF and Poppler, without resizing.

Supplement to compare.mjs and pdfa_round2_eval.py, not a replacement for their
unchanged metrics. Save every selected image for inspection; do not publish
corpus PDFs, extracted text or rendered pages without permission.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
from pathlib import Path
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for key in ['source', 'before', 'after', 'list', 'out']:
        parser.add_argument('--' + key, type=Path, required=True)
    parser.add_argument('--dpi', type=int, default=150)
    parser.add_argument('--pages', type=int, default=3)
    parser.add_argument('--workers', type=int, default=2)
    args = parser.parse_args()
    names = [n.strip() for n in args.list.read_text().splitlines() if n.strip()]
    if len(set(names)) != len(names) or any(Path(n).name != n for n in names):
        parser.error('list must contain unique basenames')
    images = args.out.with_suffix('.images')
    images.mkdir(parents=True, exist_ok=True)
    versions = {}
    for tool, command in [('mutool', ['mutool', '-v']), ('poppler', ['pdftoppm', '-v'])]:
        result = subprocess.run(command, capture_output=True, timeout=30)
        versions[tool] = (result.stdout + result.stderr).decode(errors='replace').strip()

    def one(name):
        import numpy as np
        from PIL import Image
        from pypdf import PdfReader
        from skimage.metrics import structural_similarity
        row = {'file': name, 'pages': [], 'errors': [], 'inputs': {}}
        try:
            count = len(PdfReader(args.source / name).pages)
            for label in ['source', 'before', 'after']:
                pdf = getattr(args, label) / name
                row['inputs'][label] = hashlib.sha256(pdf.read_bytes()).hexdigest()
            for tool in versions:
                for page in range(1, min(count, args.pages) + 1):
                    data = {}; gray = {}; result = {'renderer': tool, 'page': page, 'renders': {}}
                    for label in ['source', 'before', 'after']:
                        pdf = getattr(args, label) / name
                        stem = images / f'{name}-{tool}-{page}-{label}'
                        png = Path(str(stem) + '.png')
                        if tool == 'mutool':
                            cmd = ['mutool', 'draw', '-r', str(args.dpi), '-o', str(png), str(pdf), str(page)]
                        else:
                            cmd = ['pdftoppm', '-f', str(page), '-l', str(page), '-r', str(args.dpi),
                                   '-singlefile', '-png', str(pdf), str(stem)]
                        run = subprocess.run(cmd, capture_output=True, timeout=180)
                        result['renders'][label] = {'exit': run.returncode,
                            'stderr_sha256': hashlib.sha256(run.stderr).hexdigest()}
                        if not png.is_file():
                            raise ValueError(f'{tool} page {page} {label}: no image')
                        image = Image.open(png).convert('RGB')
                        data[label] = np.asarray(image)
                        gray[label] = np.asarray(image.convert('L'))
                        result['renders'][label]['shape'] = list(data[label].shape)
                    for label in ['before', 'after']:
                        if data[label].shape != data['source'].shape:
                            result[label] = {'dimension_mismatch': True}
                            continue
                        result[label] = {
                            'rgb_mae': float(np.abs(data['source'].astype(float) - data[label]).mean() / 255),
                            'rgb_ssim': float(structural_similarity(data['source'], data[label], channel_axis=2, data_range=255)),
                            'gray_ssim': float(structural_similarity(gray['source'], gray[label], data_range=255)),
                        }
                    row['pages'].append(result)
        except Exception as error:
            row['errors'].append(f'{type(error).__name__}: {error}')
        return row

    rows = []
    with args.out.with_suffix('.jsonl').open('w') as incremental:
        with ThreadPoolExecutor(max_workers=args.workers) as pool:
            for row in pool.map(one, names):
                rows.append(row)
                incremental.write(json.dumps(row) + '\n'); incremental.flush()
                print(row['file'], len(row['pages']), row['errors'], flush=True)
    args.out.write_text(json.dumps({'dpi': args.dpi, 'pages_per_document': args.pages,
        'versions': versions, 'rows': rows}, indent=2) + '\n')
    if any(row['errors'] for row in rows):
        raise SystemExit(1)


if __name__ == '__main__':
    main()
