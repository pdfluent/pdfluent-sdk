"""Our DoclingDocument output, loaded by docling-core itself (#629).

A pipeline built on Docling reads a ``DoclingDocument`` through docling-core's
own model, so that model is the judge here and not a reading of its schema by
us. Two questions, over two sources:

- the five golden files of ``crates/pdf-engine/tests/golden/*.docling.json``,
  which the engine's own suite pins byte for byte (the fixture set), and
- ``extract_docling`` on ``fixtures/structured.pdf`` through this binding.

CONFORMANCE: each validates against the JSON schema docling-core derives for
the pinned version, and loads through ``DoclingDocument.model_validate``. The
SERIALIZATION schema, because a file is the serialized form: docling-core
writes a reference as ``cref`` and describes it that way only in that mode,
while its validation-mode schema names the alias ``$ref`` its own files never
carry.

ROUND TRIP: loaded and dumped again by docling-core, it comes back EQUAL to
what we wrote -- every node, reference, provenance, cell and derived grid. A
field we wrote that Docling drops, or one Docling adds that we left out, fails
here.

THE VERSION IS PINNED on both sides. ``DOCLING_CORE_VERSION`` in
``crates/pdf-engine/src/docling.rs`` names the release the mapping was written
against, CI installs exactly that release, and the first test fails if the two
disagree -- so a schema bump is a change somebody makes on purpose.

docling-core is MIT (the Docling project); it is a test dependency only and
never a dependency of the wheel.

Run:
    cd crates/pdf-python
    pip install docling-core==2.98.1 jsonschema
    maturin develop
    pytest tests/test_docling_document.py
"""

import glob
import json
import os
import re
from importlib.metadata import version

import jsonschema
import pytest
from docling_core.types.doc import DoclingDocument

from pdfluent import Document

REPO = os.path.join(os.path.dirname(__file__), "..", "..", "..")
GOLDEN = os.path.join(REPO, "crates", "pdf-engine", "tests", "golden")
STRUCTURED_PDF = os.path.join(REPO, "fixtures", "structured.pdf")
MAPPING = os.path.join(REPO, "crates", "pdf-engine", "src", "docling.rs")


def pinned(name):
    with open(MAPPING, encoding="utf-8") as source:
        found = re.search(rf'pub const {name}: &str = "([^"]+)";', source.read())
    assert found, f"{name} is not in {MAPPING}"
    return found.group(1)


def goldens():
    paths = sorted(glob.glob(os.path.join(GOLDEN, "*.docling.json")))
    assert len(paths) == 5, f"expected the five fixtures, found {paths}"
    return paths


def schema():
    return DoclingDocument.model_json_schema(mode="serialization")


def round_trip(ours):
    """What docling-core's model makes of our JSON, dumped back as JSON."""
    loaded = DoclingDocument.model_validate(ours)
    return loaded, json.loads(loaded.model_dump_json())


def test_the_installed_docling_core_is_the_pinned_one():
    assert version("docling-core") == pinned("DOCLING_CORE_VERSION")
    assert DoclingDocument.model_fields["version"].default == pinned("SCHEMA_VERSION")


@pytest.mark.parametrize("path", goldens(), ids=os.path.basename)
def test_a_fixture_validates_against_the_pinned_schema(path):
    with open(path, encoding="utf-8") as golden:
        ours = json.load(golden)
    jsonschema.validate(ours, schema())
    assert ours["version"] == pinned("SCHEMA_VERSION")


@pytest.mark.parametrize("path", goldens(), ids=os.path.basename)
def test_a_fixture_comes_back_unchanged_through_docling_core(path):
    with open(path, encoding="utf-8") as golden:
        ours = json.load(golden)
    loaded, back = round_trip(ours)
    assert back == ours
    # And Docling's own walk and export see the body, not the furniture.
    assert list(loaded.iterate_items())
    assert loaded.export_to_markdown().strip()


def test_the_binding_writes_what_docling_core_reads_back():
    with Document(STRUCTURED_PDF) as doc:
        text = doc.extract_docling(name="structured")
    ours = json.loads(text)
    jsonschema.validate(ours, schema())
    loaded, back = round_trip(ours)
    assert back == ours
    assert loaded.name == "structured"

    labels = {item.label.value for item, _ in loaded.iterate_items()}
    assert {"section_header", "list_item", "table"} <= labels
    table = loaded.tables[0]
    assert table.data.grid[0][0].text == "Band"
    assert table.data.grid[0][0].column_header
    assert "# Safety rules" in loaded.export_to_markdown()


def test_a_window_lists_only_its_pages():
    with Document(STRUCTURED_PDF) as doc:
        second = json.loads(doc.extract_docling(first_page=1, page_count=1))
        past = json.loads(doc.extract_docling(first_page=9, page_count=4))
    assert list(second["pages"]) == ["2"]
    assert all(t["prov"][0]["page_no"] == 2 for t in second["texts"])
    assert past["pages"] == {} and past["texts"] == [] and past["tables"] == []
