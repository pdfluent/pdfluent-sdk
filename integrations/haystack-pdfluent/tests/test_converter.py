"""The Haystack converter: API mode against a local HTTP server standing in for the hosted API, and
local mode on the structured fixture (needs the pdfluent engine)."""

import http.server
import json
import os
import threading

import pytest
from haystack import Document, Pipeline
from haystack.dataclasses import ByteStream

from pdfluent_haystack import PDFluentConverter

STRUCTURED_PDF = os.path.join(os.path.dirname(__file__), "..", "..", "..", "fixtures", "structured.pdf")
CHUNKS = {"chunks": [
    {"element_id": "chunk-1", "type": "composite", "text": "Safety rules\n\nWear boots.", "page_number": 1,
     "bbox": [1, 2, 3, 4], "parent_id": None, "language": "en", "source_element_ids": ["a", "b"]},
    {"element_id": "chunk-2", "type": "table", "text": "Band | Units", "page_number": 2,
     "bbox": [1, 2, 3, 4], "parent_id": "a", "language": "en", "source_element_ids": ["t"]},
]}


class Api:
    def __init__(self, status=200, body=None):
        self.seen = []
        outer, payload = self, json.dumps(body if body is not None else CHUNKS).encode()

        class H(http.server.BaseHTTPRequestHandler):
            def log_message(self, *a):
                pass

            def do_POST(self):
                n = int(self.headers.get("content-length") or 0)
                outer.seen.append((self.path, {k.lower(): v for k, v in self.headers.items()}, self.rfile.read(n)))
                self.send_response(status)
                self.send_header("content-length", str(len(payload)))
                self.end_headers()
                self.wfile.write(payload)

        self.server = http.server.HTTPServer(("127.0.0.1", 0), H)
        self.url = f"http://127.0.0.1:{self.server.server_address[1]}"
        threading.Thread(target=self.server.serve_forever, daemon=True).start()


@pytest.fixture
def api():
    a = Api()
    yield a
    a.server.shutdown()


def test_api_mode_returns_one_document_per_hosted_chunk(api):
    docs = PDFluentConverter(api_key="sk-test-key", api_url=api.url).run(sources=[STRUCTURED_PDF])["documents"]
    assert all(isinstance(d, Document) for d in docs)
    assert [d.content for d in docs] == ["Safety rules\n\nWear boots.", "Band | Units"]
    assert [d.meta["page"] for d in docs] == [0, 1]
    assert docs[1].meta["kind"] == "table" and docs[1].meta["parent_id"] == "a"
    path, headers, body = api.seen[0]
    assert path == "/v1/extract/chunks" and headers["x-api-key"] == "sk-test-key"
    assert b'"chunking_strategy": "by_title"' in body and len(body) > 1000


def test_options_pages_and_meta_are_passed_on(api):
    out = PDFluentConverter(api_key="k", api_url=api.url, pages=[0, 1], chunking_strategy="by_page",
                            max_characters=300).run(sources=[STRUCTURED_PDF], meta={"tenant": "a"})
    body = api.seen[0][2].decode("latin-1")
    assert '"pages": [1, 2]' in body and '"chunking_strategy": "by_page"' in body and '"max_characters": 300' in body
    assert all(d.meta["tenant"] == "a" for d in out["documents"])


def test_a_byte_stream_and_a_meta_list_per_source(api):
    with open(STRUCTURED_PDF, "rb") as handle:
        stream = ByteStream(handle.read(), meta={"file_path": "inline.pdf"})
    out = PDFluentConverter(api_key="k", api_url=api.url).run(
        sources=[stream, STRUCTURED_PDF], meta=[{"n": 1}, {"n": 2}])
    metas = [(d.meta["source"], d.meta["n"]) for d in out["documents"]]
    assert metas[:2] == [("inline.pdf", 1)] * 2 and metas[2:] == [(STRUCTURED_PDF, 2)] * 2
    with pytest.raises(ValueError):
        PDFluentConverter(api_key="k", api_url=api.url).run(sources=[STRUCTURED_PDF], meta=[{}, {}])


def test_it_runs_inside_a_pipeline(api):
    pipe = Pipeline()
    pipe.add_component("convert", PDFluentConverter(api_key="k", api_url=api.url))
    result = pipe.run({"convert": {"sources": [STRUCTURED_PDF]}})
    assert len(result["convert"]["documents"]) == 2


def test_the_environment_key_turns_api_mode_on(api, monkeypatch):
    monkeypatch.setenv("PDFLUENT_API_KEY", "sk-env")
    monkeypatch.setenv("PDFLUENT_API_URL", api.url)
    PDFluentConverter().run(sources=[STRUCTURED_PDF])
    assert api.seen[0][1]["x-api-key"] == "sk-env"


def test_the_key_stays_out_of_the_repr_and_the_errors():
    converter = PDFluentConverter(api_key="sk-very-secret", api_url="http://127.0.0.1:1")
    assert "sk-very-secret" not in repr(converter)
    with pytest.raises(Exception) as e:
        converter.run(sources=[STRUCTURED_PDF])
    assert "sk-very-secret" not in str(e.value)


def test_an_api_refusal_names_its_code():
    a = Api(status=402, body={"error": {"code": "insufficient_credits", "message": "Buy credits."}})
    try:
        with pytest.raises(RuntimeError, match="402 insufficient_credits"):
            PDFluentConverter(api_key="k", api_url=a.url).run(sources=[STRUCTURED_PDF])
    finally:
        a.server.shutdown()


def test_local_mode_reads_the_fixture_without_the_network(monkeypatch):
    monkeypatch.delenv("PDFLUENT_API_KEY", raising=False)
    try:
        import pdfluent  # noqa: F401
    except ImportError:
        import sys
        print("SKIPPED (not a pass): the pdfluent engine is not installed", file=sys.stderr)
        pytest.skip("the pdfluent engine is not installed")
    docs = PDFluentConverter().run(sources=[STRUCTURED_PDF])["documents"]
    assert docs and docs[0].content == "Safety rules"
    assert docs[0].meta["page"] == 0 and docs[0].meta["kind"] == "heading"


CANARY = "sk-canary-7f3a9c1e-DO-NOT-LEAK"


class EmbApi:
    """A hosted API that answers /v1/extract/embeddings, and can echo the key it received in an error."""

    def __init__(self, echo_error=False):
        self.seen = []
        outer = self

        class H(http.server.BaseHTTPRequestHandler):
            def log_message(self, *a):
                pass

            def do_POST(self):
                n = int(self.headers.get("content-length") or 0)
                headers = {k.lower(): v for k, v in self.headers.items()}
                outer.seen.append((self.path, headers, self.rfile.read(n)))
                if echo_error:
                    payload = json.dumps({"error": {"code": "embedding_upstream_failed",
                                                    "message": "provider said: " + headers.get("x-byok-embedding-key", "")}}).encode()
                    status = 500
                elif self.path.endswith("/embeddings"):
                    payload = json.dumps({"model": "mistralai/mistral-embed-2312", "region": "eu", "data_left_eu": False,
                                          "chunks": [dict(c, embedding=[0.5, 0.25]) for c in CHUNKS["chunks"]]}).encode()
                    status = 200
                else:
                    payload = json.dumps(CHUNKS).encode()
                    status = 200
                self.send_response(status)
                self.send_header("content-length", str(len(payload)))
                self.end_headers()
                self.wfile.write(payload)

        self.server = http.server.HTTPServer(("127.0.0.1", 0), H)
        self.url = f"http://127.0.0.1:{self.server.server_address[1]}"
        threading.Thread(target=self.server.serve_forever, daemon=True).start()


def _writes(monkeypatch):
    """Records every open() for writing: the clients must write nothing."""
    import builtins

    real, writes = builtins.open, []

    def spy(file, mode="r", *a, **k):
        if any(c in str(mode) for c in "wax+"):
            writes.append(str(file))
        return real(file, mode, *a, **k)

    monkeypatch.setattr(builtins, "open", spy)
    return writes


def test_embed_sends_the_vectors_and_the_byok_key_on_the_embeddings_call_only(monkeypatch):
    a = EmbApi()
    monkeypatch.setenv("PDFLUENT_BYOK_EMBEDDING_KEY", CANARY)
    # Not embedding: the key in the environment is not sent with a chunks call.
    PDFluentConverter(api_key='k', api_url=a.url).run(sources=[STRUCTURED_PDF])['documents']
    assert a.seen[0][0] == "/v1/extract/chunks" and "x-byok-embedding-key" not in a.seen[0][1]
    # Embedding: the key goes with this one call, and the vectors and the region come back.
    docs = PDFluentConverter(api_key='k', api_url=a.url, embed=True, embedding_model='codestral-embed', embedding_region='eu').run(sources=[STRUCTURED_PDF])['documents']
    path, headers, body = a.seen[1]
    assert path == "/v1/extract/embeddings" and headers["x-byok-embedding-key"] == CANARY
    assert b'"model": "codestral-embed"' in body and b'"region": "eu"' in body
    assert [d.embedding for d in docs] == [[0.5, 0.25], [0.5, 0.25]]
    assert all(d.meta["embedding_model"] == "mistralai/mistral-embed-2312" and d.meta["data_left_eu"] is False for d in docs)


def test_the_key_can_be_given_per_object_with_its_base_url(monkeypatch):
    monkeypatch.delenv("PDFLUENT_BYOK_EMBEDDING_KEY", raising=False)
    a = EmbApi()
    PDFluentConverter(api_key='k', api_url=a.url, embed=True, embedding_model='text-embedding-3-small', byok_embedding_key=CANARY, byok_embedding_base_url='https://api.example.com/v1').run(sources=[STRUCTURED_PDF])['documents']
    headers = a.seen[0][1]
    assert headers["x-byok-embedding-key"] == CANARY and headers["x-byok-embedding-base-url"] == "https://api.example.com/v1"


def test_the_key_is_never_in_a_repr_a_log_a_message_a_document_or_a_file(monkeypatch, caplog, capsys, tmp_path):
    import logging

    caplog.set_level(logging.DEBUG)
    monkeypatch.delenv("PDFLUENT_BYOK_EMBEDDING_KEY", raising=False)
    monkeypatch.chdir(tmp_path)
    writes = _writes(monkeypatch)
    a = EmbApi()
    obj = PDFluentConverter(api_key='k', api_url=a.url, embed=True, embedding_model='m', byok_embedding_key=CANARY)
    docs = obj.run(sources=[STRUCTURED_PDF])['documents']
    shown = " ".join([repr(obj), str(obj), repr(vars(obj)), str(docs), repr([d.meta for d in docs])])
    assert CANARY not in shown
    out, err = capsys.readouterr()
    assert CANARY not in out + err + caplog.text
    assert writes == [] and list(tmp_path.iterdir()) == [], "nothing is written to disk"
    # A provider that echoes the key in its error does not get it into ours.
    e = EmbApi(echo_error=True)
    bad = PDFluentConverter(api_key='k', api_url=e.url, embed=True, embedding_model='m', byok_embedding_key=CANARY)
    with pytest.raises(RuntimeError) as raised:
        bad.run(sources=[STRUCTURED_PDF])['documents']
    assert CANARY not in str(raised.value) and "[redacted]" in str(raised.value)
    assert CANARY not in caplog.text


def test_a_key_without_embedding_or_without_api_mode_is_refused_without_naming_it(monkeypatch):
    monkeypatch.delenv("PDFLUENT_API_KEY", raising=False)
    monkeypatch.delenv("PDFLUENT_BYOK_EMBEDDING_KEY", raising=False)
    for kw in (dict(api_key="k", byok_embedding_key=CANARY),
               dict(api_key="k", embed=True, byok_embedding_base_url="https://x.example.com")):
        with pytest.raises(ValueError) as e:
            PDFluentConverter(**kw)
        assert CANARY not in str(e.value)
    # Embedding needs API mode; the converter says so when it runs, not when a saved pipeline loads.
    with pytest.raises(ValueError) as e:
        PDFluentConverter(embed=True, byok_embedding_key=CANARY).run(sources=[STRUCTURED_PDF])
    assert CANARY not in str(e.value)


def test_a_saved_pipeline_holds_no_key_and_loads_again():
    converter = PDFluentConverter(api_key="sk-pdfluent-canary", embed=True, embedding_model="m",
                                  byok_embedding_key=CANARY, byok_embedding_base_url="https://x.example.com/CANARY-BASE")
    pipe = Pipeline()
    pipe.add_component("convert", converter)
    dumped = pipe.dumps()
    assert CANARY not in dumped and "sk-pdfluent-canary" not in dumped and "CANARY-BASE" not in dumped
    assert "embedding_model" in dumped
    # Haystack loads only modules on its allowlist; ours is allowed here as a user would allow it.
    loaded = Pipeline.loads(dumped, allowed_modules=["pdfluent_haystack.converter"])
    assert loaded.get_component("convert").embedding_model == "m"
