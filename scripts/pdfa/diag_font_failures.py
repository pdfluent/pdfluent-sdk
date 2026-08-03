#!/usr/bin/env python3
"""Per failing document, report which font and which character code veraPDF
objected to.

The rule number alone ("6.2.11.5:1") says nothing actionable. What you need is
the font name and the code, because that is what tells you whether the encoding,
the width or the glyph is wrong. The trailing shape of veraPDF's usedGlyphs
context is `<code> <renderMode> <?> <bool>`, so the code is the 4th token from
the end -- not a fixed column, since the leading font-name tokens vary.

Usage:
    python3 scripts/pdfa/diag_font_failures.py a.pdf b.pdf ...

Requires a release build of xfa-test-runner and veraPDF. On macOS the
/usr/local/bin/verapdf launcher has a broken shebang that execve rejects, hence
the direct path below.
"""
import json, re, subprocess, sys, os
import xml.etree.ElementTree as ET

RUNNER = os.environ.get("XFA_RUNNER", "./target/release/xfa-test-runner")
VERAPDF = os.environ.get("VERAPDF", "/usr/local/bin/verapdf")
OUT = os.environ.get("PDFA_DIAG_OUT", "/tmp/pdfa-diag")
os.makedirs(OUT, exist_ok=True)

docs = sys.argv[1:]
results = {}
for doc in docs:
    name = os.path.basename(doc)
    conv = os.path.join(OUT, name)
    if not os.path.exists(conv):
        r = subprocess.run([RUNNER, "convert-one", "-p", doc, "-o", conv],
                           capture_output=True, text=True)
        if r.returncode != 0:
            results[name] = {"error": "convert failed"}
            continue
    xml = subprocess.run([VERAPDF, "-f", "2b", "--format", "xml", conv],
                         capture_output=True, text=True, env={**os.environ, "JAVA_HOME": "/usr/local/opt/openjdk@17"}).stdout
    with open(os.path.join(OUT, name + ".xml"), "w") as f:
        f.write(xml)
    try:
        root = ET.fromstring(xml)
    except ET.ParseError as e:
        results[name] = {"error": f"xml parse: {e}"}
        continue
    fails = []
    for rule in root.iter("rule"):
        if rule.get("status") == "failed":
            clause = rule.get("clause")
            test = rule.get("testNumber")
            for chk in rule.iter("check"):
                ctx = chk.findtext("context") or ""
                fails.append({"rule": f"{clause}:{test}", "context": ctx})
    results[name] = {"failures": fails}

print(json.dumps(results, indent=1))
