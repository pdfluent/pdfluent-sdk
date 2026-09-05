#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Measure shipping conversion in fresh processes; report failures and timeouts.

Use the same list for each binary. This is a secondary engineering check:
record host load separately and do not present contended runs as latency SLAs.
Peak RSS covers the entire converter process, including loading and saving.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys
import time


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    for key in ['binary', 'source', 'list', 'out']:
        p.add_argument('--' + key, type=Path, required=True)
    a = p.parse_args()
    names = [n.strip() for n in a.list.read_text().splitlines() if n.strip()]
    if len(set(names)) != len(names) or any(Path(n).name != n for n in names):
        p.error('list requires unique basenames')
    a.out.mkdir(parents=True, exist_ok=True)
    rows = []
    for n in names:
        output = a.out / n
        row = {'file': n, 'input_sha256': sha(a.source / n)}
        start = time.monotonic()
        try:
            flags = ['-l'] if sys.platform == 'darwin' else ['-v']
            result = subprocess.run(['/usr/bin/time', *flags, str(a.binary.resolve()),
                                     str(a.source / n), str(output)], capture_output=True, timeout=600)
            text = result.stderr.decode('utf8', 'replace')
            peak = re.search(r'(\d+)\s+maximum resident set size', text) if sys.platform == 'darwin' else re.search(r'Maximum resident set size \(kbytes\):\s*(\d+)', text)
            row.update(exit=result.returncode, peak_rss_bytes=int(peak[1]) * (1 if sys.platform == 'darwin' else 1024) if peak else None,
                       stderr_sha256=hashlib.sha256(result.stderr).hexdigest())
            if result.returncode == 0 and output.exists():
                row.update(output_bytes=output.stat().st_size, output_sha256=sha(output))
        except subprocess.TimeoutExpired:
            row.update(exit=None, timeout_seconds=600, peak_rss_bytes=None)
        row['seconds'] = time.monotonic() - start
        rows.append(row)
        with (a.out / 'measurement.jsonl').open('a') as f:
            f.write(json.dumps(row) + '\n')
    (a.out / 'measurement.json').write_text(json.dumps({'binary_sha256': sha(a.binary), 'platform': sys.platform,
        'scope': 'fresh converter process, wall time and peak RSS; host contention not controlled', 'rows': rows}, indent=2) + '\n')


if __name__ == '__main__':
    main()
