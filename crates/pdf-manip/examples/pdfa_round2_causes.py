#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Inventory every original validation, visual and word-retention tail case.

Evidence and hypotheses are separate. Source fonts without programs do not by
themselves prove a particular visual change. Token metrics do not prove that
all visible content survived. Requires pypdf for source resource inspection.
"""
import argparse
from collections import Counter
from concurrent.futures import ThreadPoolExecutor
import json
from pathlib import Path
import subprocess


def source_fonts(path):
    from pypdf import PdfReader
    from pypdf.generic import IndirectObject, DictionaryObject, ArrayObject
    reader = PdfReader(path)
    seen = set(); fonts = {}
    def walk(obj):
        if isinstance(obj, IndirectObject):
            key = (obj.idnum, obj.generation)
            if key in seen: return
            seen.add(key); obj = obj.get_object()
        if isinstance(obj, DictionaryObject):
            if obj.get('/Type') == '/Font' or ('/BaseFont' in obj and '/Subtype' in obj):
                subtype = str(obj.get('/Subtype'))
                if subtype not in ('/Type0', '/Type3'):
                    descriptor = obj.get('/FontDescriptor')
                    fd = descriptor.get_object() if descriptor else {}
                    embedded = any(k in fd for k in ('/FontFile', '/FontFile2', '/FontFile3'))
                    fonts[str(obj.get('/BaseFont', 'unnamed'))] = {'subtype': subtype, 'embedded': embedded}
            for k, value in obj.items():
                if k not in ('/Parent', '/P'): walk(value)
        elif isinstance(obj, ArrayObject):
            for value in obj: walk(value)
    for page in reader.pages: walk(page.get('/Resources', {}))
    return fonts


def main():
    p = argparse.ArgumentParser(description=__doc__)
    for name in ['baseline', 'candidate', 'source', 'before', 'after', 'out']:
        p.add_argument('--' + name, type=Path, required=True)
    a = p.parse_args()
    before = {r['name']: r for r in json.loads(a.baseline.read_text())['rows']}
    after = {r['name']: r for r in json.loads(a.candidate.read_text())['rows']}
    names = [n for n, r in before.items() if not r['conformant'] or (r.get('renderMean') or 0) > .02
             or (r.get('wordRetention') is not None and r['wordRetention'] < 1)]
    def one(n):
        b, c = before[n], after[n]
        row = {'file': n, 'before': b, 'after': c, 'evidence': [], 'hypotheses': [], 'unresolved': [], 'errors': []}
        if not b['conformant']:
            row['evidence'].append('original validator failure; ' + ('now conformant' if c['conformant'] else 'still nonconformant'))
        if (c.get('renderMean') or 0) > .02:
            row['unresolved'].append('original visual threshold still exceeded')
        if c.get('wordRetention') is not None and c['wordRetention'] < 1:
            row['unresolved'].append('word-retention tail remains; inspect source extraction and changed token order/geometry')
        try:
            fonts = source_fonts(a.source / n)
            row['source_resource_fonts'] = fonts
            if any(not f['embedded'] for f in fonts.values()):
                row['evidence'].append('source resource dictionary contains fonts without embedded programs')
                row['hypotheses'].append('font substitution can change geometry; this resource census alone does not attribute each affected page')
        except Exception as e:
            row['errors'].append('source font inspection: ' + type(e).__name__)
        texts = {}
        for label, folder in [('source', a.source), ('before', a.before), ('after', a.after)]:
            try:
                result = subprocess.run(['mutool', 'draw', '-F', 'txt', '-o', '-', str(folder / n)], capture_output=True, timeout=600)
                texts[label] = result.stdout.decode('utf-8', 'replace')
                row[label + '_text_exit'] = result.returncode
                row[label + '_replacement_characters'] = texts[label].count('\ufffd')
            except (OSError, subprocess.SubprocessError) as e:
                row['errors'].append(label + ' extraction: ' + type(e).__name__)
        if len(texts) == 3:
            row['output_text_identical'] = texts['before'] == texts['after']
            missing = Counter(texts['source'].split()) - Counter(texts['after'].split())
            row['missing_token_signatures'] = [{'length': len(x), 'count': count, 'special_codepoints': sorted(set(f'U+{ord(c):04X}' for c in x if not c.isalnum()))} for x, count in missing.most_common(5)]
            if row['source_replacement_characters']:
                row['evidence'].append('source extraction contains Unicode replacement characters; exact-token retention is not a complete text oracle')
        sidecar = (a.after / n).with_suffix('.pdf.report.json')
        if sidecar.exists(): row['conversion_diagnostics'] = json.loads(sidecar.read_text())
        return row
    with ThreadPoolExecutor(max_workers=2) as pool:
        rows = list(pool.map(one, names))
    a.out.write_text(json.dumps({'documents': len(rows), 'scope': 'union of every original validation/visual/word-retention tail',
                               'hypotheses_are_not_verified_causes': True, 'rows': rows}, indent=2) + '\n')
    print(len(rows), 'tail cases inventoried')


if __name__ == '__main__':
    main()
