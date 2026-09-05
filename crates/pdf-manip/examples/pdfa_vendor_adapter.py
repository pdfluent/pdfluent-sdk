#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Local PDF/A-2b vendor adapters for the unchanged four-axis benchmark.

Each invocation writes OUTPUT.measurement.json, including errors. No corpus
upload to hosted services, license acquisition, external repair, or extra
lossy optimization is performed. Vendor results require external validation.
See pdfa_vendor_comparison.md for installation/access and exact settings.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import resource
import sys
import time
import urllib.parse
import urllib.request
import uuid


def convert_apryse(source, output, row):
    from PDFNetPython3 import PDFNet, PDFACompliance
    key = os.environ.get('APRYSE_LICENSE_KEY')
    if not key:
        raise RuntimeError('APRYSE_LICENSE_KEY is unavailable')
    PDFNet.Initialize(key)
    try:
        row['sdk_version'] = str(PDFNet.GetVersion())
        row['settings'] = {'convert': True, 'conformance': '2b', 'max_reference_objects': 10,
                           'linearized': False, 'separate_optimizer': False}
        pdfa = PDFACompliance(True, str(source), None, PDFACompliance.e_Level2B, 0, 10)
        pdfa.SaveAs(str(output), False)
    finally:
        PDFNet.Terminate()


def convert_nutrient(source, output, row):
    endpoint = os.environ.get('NUTRIENT_LOCAL_URL', 'http://127.0.0.1:5000')
    parsed = urllib.parse.urlparse(endpoint)
    if parsed.scheme not in ('http', 'https') or parsed.hostname not in ('localhost', '127.0.0.1', '::1'):
        raise RuntimeError('NUTRIENT_LOCAL_URL must be a loopback Document Engine endpoint')
    token = os.environ.get('NUTRIENT_API_TOKEN')
    version = os.environ.get('NUTRIENT_ENGINE_VERSION')
    if not token or not version:
        raise RuntimeError('NUTRIENT_API_TOKEN and NUTRIENT_ENGINE_VERSION are required')
    instructions = {'parts': [{'file': 'document'}], 'output': {
        'type': 'pdfa', 'conformance': 'pdfa-2b', 'vectorization': True, 'rasterization': False}}
    row['sdk_version'] = version
    row['settings'] = instructions
    row['peak_rss_scope'] = 'client only; server peak memory must be measured separately'
    boundary = 'pdfa-benchmark-' + uuid.uuid4().hex
    body = (f'--{boundary}\r\nContent-Disposition: form-data; name="instructions"\r\n\r\n'
            + json.dumps(instructions) + f'\r\n--{boundary}\r\nContent-Disposition: form-data; '
            'name="document"; filename="input.pdf"\r\nContent-Type: application/pdf\r\n\r\n').encode()
    body += source.read_bytes() + f'\r\n--{boundary}--\r\n'.encode()
    request = urllib.request.Request(endpoint.rstrip('/') + '/api/build', data=body, headers={
        'Authorization': 'Token token=' + token,
        'Content-Type': 'multipart/form-data; boundary=' + boundary})
    # Redirects cannot move local corpus content to a hosted endpoint.
    class NoRedirect(urllib.request.HTTPRedirectHandler):
        def redirect_request(self, req, fp, code, msg, headers, newurl):
            return None
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), NoRedirect())
    with opener.open(request, timeout=600) as response:
        data = response.read()
    if not data.startswith(b'%PDF-'):
        raise RuntimeError('Document Engine returned a non-PDF response')
    output.write_bytes(data)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('vendor', choices=['apryse', 'nutrient'])
    p.add_argument('source', type=Path)
    p.add_argument('output', type=Path)
    a = p.parse_args()
    if a.output.exists() or a.output.with_suffix(a.output.suffix + '.measurement.json').exists():
        p.error('output or measurement already exists; use a fresh directory')
    a.output.parent.mkdir(parents=True, exist_ok=True)
    row = {'vendor': a.vendor, 'input_sha256': hashlib.sha256(a.source.read_bytes()).hexdigest(),
           'target': 'PDF/A-2b', 'license_and_watermark': os.environ.get('PDFA_VENDOR_LICENSE_DESCRIPTION', 'unspecified'),
           'peak_rss_scope': 'adapter and in-process SDK', 'status': 'failed'}
    start = time.perf_counter()
    try:
        (convert_apryse if a.vendor == 'apryse' else convert_nutrient)(a.source, a.output, row)
        row['output_bytes'] = a.output.stat().st_size
        row['output_sha256'] = hashlib.sha256(a.output.read_bytes()).hexdigest()
        row['status'] = 'converted; external validation pending'
    except Exception as error:
        # SDK/network exceptions can contain credentials or response bodies.
        row['error_type'] = type(error).__name__
        print(f'{a.vendor} conversion unavailable or failed: {type(error).__name__}', file=sys.stderr)
    finally:
        row['wall_seconds'] = time.perf_counter() - start
        rss = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
        row['peak_rss_bytes'] = rss if sys.platform == 'darwin' else rss * 1024
        a.output.with_suffix(a.output.suffix + '.measurement.json').write_text(json.dumps(row, indent=2) + '\n')
    return int(row['status'] == 'failed')


if __name__ == '__main__':
    raise SystemExit(main())
