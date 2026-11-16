"""API mode: the loader reads its chunks (and, if asked, embeddings) from the hosted PDFluent API.

Standard library only. The PDFluent key is sent in the ``x-api-key`` header, comes from the caller
or the ``PDFLUENT_API_KEY`` environment variable, and never appears in a repr or an error.

Your own embedding provider key (BYOK) is passed through: given per call (the constructor argument)
or read from ``PDFLUENT_BYOK_EMBEDDING_KEY`` (and ``PDFLUENT_BYOK_EMBEDDING_BASE_URL``), sent in the
``x-byok-embedding-key`` header of the embeddings request only, and used by the service for that one
call. It is never written to disk, never logged, and scrubbed out of any error message; its repr
says nothing about it.
"""

# Copyright (c) 2026 Innovation Trigger B.V.
#
# Licensed under the MIT License. See the LICENSE file in this repository.

from __future__ import annotations

import json
import os
import urllib.error
import urllib.request
import uuid
from typing import Any, Dict, List, Mapping, Optional, Sequence, Tuple

DEFAULT_URL = "https://api.pdfluent.com"
CHUNKS_PATH = "/v1/extract/chunks"
EMBEDDINGS_PATH = "/v1/extract/embeddings"
BYOK_KEY_ENV = "PDFLUENT_BYOK_EMBEDDING_KEY"
BYOK_URL_ENV = "PDFLUENT_BYOK_EMBEDDING_BASE_URL"


class ApiError(RuntimeError):
    """The hosted API refused or failed. The message names the cause and never carries the key."""


def resolve_key(api_key: Optional[str]) -> Optional[str]:
    """The key to use: the argument, else the environment, else None (local mode)."""
    return api_key or os.environ.get("PDFLUENT_API_KEY") or None


class Byok:
    """Your own embedding provider key and, optionally, its OpenAI-compatible base URL.

    Held for the embeddings request only. ``repr`` and ``str`` print nothing but that it is set.
    """

    __slots__ = ("_key", "_base_url")

    def __init__(self, key: str, base_url: Optional[str] = None) -> None:
        self._key = key
        self._base_url = base_url

    def __repr__(self) -> str:
        return "Byok(set)"

    @classmethod
    def resolve(cls, key: Optional[str], base_url: Optional[str]) -> Optional["Byok"]:
        """The explicit key, else the environment's, else None. A base URL alone is a mistake."""
        key = key or os.environ.get(BYOK_KEY_ENV) or None
        base_url = base_url or os.environ.get(BYOK_URL_ENV) or None
        if key is None:
            if base_url is not None:
                raise ValueError("a BYOK base URL needs a BYOK embedding key")
            return None
        return cls(key, base_url)

    def headers(self) -> Dict[str, str]:
        headers = {"x-byok-embedding-key": self._key}
        if self._base_url:
            headers["x-byok-embedding-base-url"] = self._base_url
        return headers

    def secrets(self) -> Tuple[str, ...]:
        return tuple(s for s in (self._key, self._base_url) if s)


def _scrub(text: str, secrets: Sequence[str]) -> str:
    for secret in secrets:
        if secret:
            text = text.replace(secret, "[redacted]")
    return text


def _multipart(pdf: bytes, options: Mapping[str, Any]) -> "tuple[bytes, str]":
    boundary = "----pdfluent" + uuid.uuid4().hex
    body = (
        f'--{boundary}\r\nContent-Disposition: form-data; name="file"; filename="in.pdf"\r\n'
        "Content-Type: application/octet-stream\r\n\r\n"
    ).encode() + pdf + b"\r\n"
    body += f'--{boundary}\r\nContent-Disposition: form-data; name="options"\r\n\r\n'.encode()
    body += json.dumps(options).encode() + b"\r\n" + f"--{boundary}--\r\n".encode()
    return body, f"multipart/form-data; boundary={boundary}"


def _post(
    path: str,
    pdf: bytes,
    *,
    api_key: str,
    api_url: Optional[str],
    options: Mapping[str, Any],
    byok: Optional[Byok] = None,
    timeout: float = 300.0,
) -> Dict[str, Any]:
    body, content_type = _multipart(pdf, options)
    headers = {"content-type": content_type, "x-api-key": api_key}
    secrets: Tuple[str, ...] = ()
    if byok is not None:
        headers.update(byok.headers())
        secrets = byok.secrets() + (api_key,)
    else:
        secrets = (api_key,)
    request = urllib.request.Request(
        (api_url or os.environ.get("PDFLUENT_API_URL") or DEFAULT_URL).rstrip("/") + path,
        data=body,
        method="POST",
        headers=headers,
    )
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:
            return dict(json.loads(response.read()))
    except urllib.error.HTTPError as e:
        try:
            err = json.loads(e.read())["error"]
            message = f"{e.code} {err.get('code', '')}: {err.get('message', '')}"
        except (ValueError, KeyError, TypeError):
            message = f"the hosted API answered {e.code}"
        # A provider can echo what it received; nothing of ours or yours is passed on.
        raise ApiError(_scrub(message, secrets)) from None
    except (urllib.error.URLError, TimeoutError, OSError):
        raise ApiError("the hosted API is not reachable") from None


def fetch_chunks_bytes(pdf: bytes, **kw: Any) -> List[Dict[str, Any]]:
    """The ``chunks`` of ``POST /v1/extract/chunks`` for one PDF given as bytes."""
    return list(_post(CHUNKS_PATH, pdf, **kw)["chunks"])


def fetch_embeddings_bytes(pdf: bytes, **kw: Any) -> Dict[str, Any]:
    """The answer of ``POST /v1/extract/embeddings``: ``chunks`` each with an ``embedding``, and the
    ``model``, ``region`` and ``data_left_eu`` fields. Sends the BYOK key, if any, on this call only."""
    return _post(EMBEDDINGS_PATH, pdf, **kw)


def fetch_chunks(pdf_path: str, **kw: Any) -> List[Dict[str, Any]]:
    with open(pdf_path, "rb") as handle:
        return fetch_chunks_bytes(handle.read(), **kw)


def fetch_embeddings(pdf_path: str, **kw: Any) -> Dict[str, Any]:
    with open(pdf_path, "rb") as handle:
        return fetch_embeddings_bytes(handle.read(), **kw)


def api_options(
    *,
    pages: Optional[Sequence[int]],
    quality: str,
    chunking_strategy: str,
    max_characters: int,
    overlap: int,
    new_after_n_chars: Optional[int],
    combine_text_under_n_chars: int,
    embedding_model: Optional[str] = None,
    embedding_region: Optional[str] = None,
) -> Dict[str, Any]:
    """The request options. The hosted API counts pages from 1, the loaders from 0."""
    options: Dict[str, Any] = {
        "quality": quality,
        "chunking_strategy": chunking_strategy,
        "max_characters": max_characters,
        "overlap": overlap,
        "combine_text_under_n_chars": combine_text_under_n_chars,
    }
    if new_after_n_chars is not None:
        options["new_after_n_chars"] = new_after_n_chars
    if pages is not None:
        options["pages"] = [p + 1 for p in pages]
    if embedding_model:
        options["model"] = embedding_model
    if embedding_region:
        options["region"] = embedding_region
    return options


def chunk_metadata(source: str, chunk: Mapping[str, Any]) -> Dict[str, Any]:
    """The metadata of one chunk, named like the local loaders' where there is an equivalent."""
    meta: Dict[str, Any] = {
        "source": source,
        "page": int(chunk.get("page_number", 1)) - 1,
        "kind": chunk.get("type"),
        "bbox": chunk.get("bbox"),
        "element_id": chunk.get("element_id"),
        "source_element_ids": chunk.get("source_element_ids", []),
    }
    if chunk.get("parent_id"):
        meta["parent_id"] = chunk["parent_id"]
    if chunk.get("language"):
        meta["language"] = chunk["language"]
    return meta


def flatten_metadata(meta: Mapping[str, Any]) -> Dict[str, Any]:
    """Metadata a store that takes only scalars (Chroma, Pinecone, Milvus and others) accepts.

    ``bbox`` becomes ``bbox_x0``, ``bbox_y0``, ``bbox_x1``, ``bbox_y1``; ``section_path`` becomes one
    string joined by `` > ``; every other list or dict becomes compact JSON text; ``None`` values are
    dropped. Scalars pass through. ``embedding`` (the vector API mode puts on a document) is left as it
    is: a store that wants it as metadata is not scalar-only.
    """
    out: Dict[str, Any] = {}
    for key, value in meta.items():
        if value is None:
            continue
        if key == "bbox" and isinstance(value, (list, tuple)) and len(value) == 4:
            for name, number in zip(("x0", "y0", "x1", "y1"), value):
                out[f"bbox_{name}"] = number
        elif key == "embedding":
            out[key] = value
        elif key == "section_path" and isinstance(value, (list, tuple)):
            out[key] = " > ".join(str(v) for v in value)
        elif isinstance(value, (list, tuple, dict)):
            out[key] = json.dumps(value, sort_keys=True, separators=(",", ":"))
        else:
            out[key] = value
    return out


def embedding_metadata(answer: Mapping[str, Any]) -> Dict[str, Any]:
    """What an embeddings answer says about where the data went, for every document."""
    return {
        "embedding_model": answer.get("model"),
        "embedding_region": answer.get("region"),
        "data_left_eu": bool(answer.get("data_left_eu")),
    }
