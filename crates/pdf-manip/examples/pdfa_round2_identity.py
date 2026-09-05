#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Strict decoded-object identity for reusing already measured render results.

Hash the entire graph reachable from Root and Info, including metadata, every
resource dictionary, original string bytes and decoded stream bytes. Include
object IDs, header version and filters. Only physical stream Length is ignored;
xref/object-stream packing is resolved by the reader. Any uncertainty or changed
object requires a fresh render. This is not a general PDF equivalence validator.
"""
import hashlib
import json
from pypdf import PdfReader
from pypdf.generic import (IndirectObject, StreamObject, ByteStringObject,
                          TextStringObject, BooleanObject)


def fingerprint(path):
    reader = PdfReader(path, strict=True)
    if reader.is_encrypted:
        raise ValueError('encrypted outputs cannot reuse render evidence')
    pending = set()
    visited = set()

    def normalize(obj):
        if isinstance(obj, IndirectObject):
            pending.add((obj.idnum, obj.generation))
            return ['reference', obj.idnum, obj.generation]
        if isinstance(obj, StreamObject):
            return ['stream', normalize({k: v for k, v in obj.items() if k != '/Length'}),
                    hashlib.sha256(obj.get_data()).hexdigest()]
        if isinstance(obj, dict):
            return [[str(k), normalize(v)] for k, v in sorted(obj.items())]
        if isinstance(obj, (list, tuple)):
            return [normalize(v) for v in obj]
        if isinstance(obj, ByteStringObject):
            return ['bytes', bytes(obj).hex()]
        if isinstance(obj, TextStringObject):
            return ['text', obj.original_bytes.hex()]
        if isinstance(obj, BooleanObject):
            return ['boolean', obj.value]
        return [type(obj).__name__, str(obj)]

    roots = normalize({k: reader.trailer.get(k) for k in ['/Root', '/Info']})
    rows = []
    # Traversal order is immaterial: rows are sorted before hashing. Pop work
    # once instead of repeatedly subtracting/sorting an entire large graph.
    while pending:
        key = pending.pop()
        if key in visited:
            continue
        visited.add(key)
        obj = reader.get_object(IndirectObject(*key, reader))
        rows.append([*key, normalize(obj)])
    payload = [reader.pdf_header, roots, sorted(rows)]
    return hashlib.sha256(json.dumps(payload, separators=(',', ':')).encode()).hexdigest()


def self_test():
    import tempfile
    from pathlib import Path
    from pypdf import PdfWriter
    from pypdf.generic import NameObject, DecodedStreamObject
    with tempfile.TemporaryDirectory() as directory:
        paths = [Path(directory) / f'{i}.pdf' for i in range(3)]
        for i, path in enumerate(paths):
            writer = PdfWriter()
            page = writer.add_blank_page(100, 100)
            stream = DecodedStreamObject()
            stream.set_data(b'0 0 10 10 re f' if i < 2 else b'0 0 20 10 re f')
            page[NameObject('/Contents')] = writer._add_object(stream)
            writer.write(path)
        assert fingerprint(paths[0]) == fingerprint(paths[1])
        assert fingerprint(paths[0]) != fingerprint(paths[2])


if __name__ == '__main__':
    self_test()
    print('decoded-object identity positive/negative controls passed')
