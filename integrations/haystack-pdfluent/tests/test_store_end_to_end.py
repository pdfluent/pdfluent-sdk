"""PDF -> Haystack converter -> an in-memory document store -> retrieval that finds the chunk.

No network: ``InMemoryDocumentStore`` with its BM25 retriever, and for the
vector path embeddings set by hand on the documents. What this pins is that the
converter's ``Document`` objects are written to a Haystack store unchanged and
come back with our metadata, and that ``flatten_metadata=True`` leaves scalars.
"""

import os

from haystack import Document, Pipeline
from haystack.components.retrievers.in_memory import InMemoryBM25Retriever, InMemoryEmbeddingRetriever
from haystack.components.writers import DocumentWriter
from haystack.document_stores.in_memory import InMemoryDocumentStore
from haystack.document_stores.types import DuplicatePolicy

from pdfluent_haystack import PDFluentConverter

STRUCTURED_PDF = os.path.join(os.path.dirname(__file__), "..", "..", "..", "fixtures", "structured.pdf")
SCALARS = (str, int, float, bool)


def _written(converter):
    """converter -> DocumentWriter -> store, as a Haystack pipeline."""
    store = InMemoryDocumentStore()
    pipe = Pipeline()
    pipe.add_component("convert", converter)
    pipe.add_component("write", DocumentWriter(store, policy=DuplicatePolicy.OVERWRITE))
    pipe.connect("convert.documents", "write.documents")
    result = pipe.run({"convert": {"sources": [STRUCTURED_PDF]}})
    return store, result["write"]["documents_written"]


def test_a_pipeline_writes_our_documents_to_a_haystack_store():
    store, written = _written(PDFluentConverter())
    assert written == 5 == store.count_documents()
    docs = store.filter_documents()
    assert all(isinstance(d, Document) for d in docs)
    assert sorted(d.meta["kind"] for d in docs) == ["heading", "list_item", "list_item", "list_item", "table"]


def test_bm25_retrieval_returns_the_chunk_with_its_metadata():
    store, _ = _written(PDFluentConverter())
    hit = InMemoryBM25Retriever(store, top_k=1).run(query="Safety rules")["documents"][0]
    assert hit.content == "Safety rules"
    assert hit.meta["kind"] == "heading" and hit.meta["page"] == 0 and len(hit.meta["bbox"]) == 4


def test_a_metadata_filter_selects_the_table():
    store, _ = _written(PDFluentConverter())
    tables = store.filter_documents({"field": "meta.kind", "operator": "==", "value": "table"})
    assert len(tables) == 1 and tables[0].meta["page"] == 1


def test_embedding_retrieval_over_our_documents():
    store, _ = _written(PDFluentConverter())
    docs = store.filter_documents()
    for i, doc in enumerate(docs):
        vec = [0.0] * len(docs)
        vec[i] = 1.0
        doc.embedding = vec
    store.write_documents(docs, policy=DuplicatePolicy.OVERWRITE)
    target = docs[2]
    query = [0.0] * len(docs)
    query[2] = 1.0
    hit = InMemoryEmbeddingRetriever(store, top_k=1).run(query_embedding=query)["documents"][0]
    assert hit.id == target.id and hit.meta["kind"] == target.meta["kind"]


def test_flattened_metadata_is_scalar_only():
    store, _ = _written(PDFluentConverter(flatten_metadata=True))
    for doc in store.filter_documents():
        assert all(isinstance(v, SCALARS) for v in doc.meta.values()), doc.meta
        assert "bbox" not in doc.meta and doc.meta["bbox_x1"] > doc.meta["bbox_x0"]


def test_flatten_metadata_survives_a_saved_pipeline():
    converter = PDFluentConverter(flatten_metadata=True)
    assert PDFluentConverter.from_dict(converter.to_dict()).flatten_metadata is True
