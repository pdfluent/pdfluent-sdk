# Fase J — Python Bindings — Autonomous Decisions

## Architecture

- **PyO3 0.24** chosen as the Rust → Python binding framework (mature, well-documented, active community)
- **maturin** as the build system — generates Python wheels from PyO3 cdylibs
- `pdf-python` crate produces a `cdylib` named `pdfengine` via `[lib] name = "pdfengine"`
- Inner module `pdfengine._native` contains the Rust classes; `__init__.py` re-exports them
- `PdfDocument` wrapped in `Arc<>` for safe sharing between `PyDocument` and `PyPage` objects

## J1 — PyO3 Bindings + PIL/NumPy Integration

### Classes exposed
- **`Document`** — Main document handle with `open(path, password=None)`, context manager (`with`), iteration, `__getitem__` with negative indexing, `render_all(dpi)`, `search(query)`
- **`Page`** — Per-page handle with `render(dpi, width, height, background)`, `thumbnail(max_dimension)`, `extract_text()`, `extract_text_blocks()`, properties for `width`, `height`, `rotation`, `geometry`
- **`RenderedImage`** — Wraps rendered RGBA pixels with `to_pil()`, `to_numpy()`, `save(path)`, `width`, `height`, `pixels`
- **`TextBlock`**, **`TextSpan`**, **`DocumentInfo`**, **`Bookmark`**, **`PageGeometry`** — Data classes

### Design decisions

1. **Workspace exclusion** — PyO3 cdylibs require a matching Python interpreter at build time. CI runners may have incompatible Python versions (e.g., 3.14 when PyO3 0.24 supports up to 3.13). Excluded `pdf-python` from workspace members with a comment explaining why. Build separately via `cd crates/pdf-python && maturin develop`.

2. **PIL integration via raw bytes** — `to_pil()` calls `PIL.Image.frombytes("RGBA", (w, h), bytes)` on the Python side. This avoids any C-level PIL dependency in the Rust code — only requires Pillow installed in the Python environment.

3. **NumPy integration via frombuffer** — `to_numpy()` calls `numpy.frombuffer(bytes, dtype="uint8").reshape((h, w, 4))`. Same principle: NumPy is imported at call time, not a build dependency.

4. **0-based page indices** — Matches Python convention. `__getitem__` supports negative indices (`doc[-1]` for last page).

5. **Context manager pattern** — `Document` supports `with` for familiar Python resource management, though Rust handles cleanup via `Drop` regardless.

6. **Iterator protocol** — `Document` implements `__iter__` returning a `PageIterator`, enabling `for page in doc:` loops.

7. **Error mapping** — `EngineError` variants are mapped to Python exceptions: `FileNotFoundError` for missing files, `ValueError` for password errors, `RuntimeError` for everything else.

8. **Module structure** — `pdfengine._native` (Rust) re-exported via `pdfengine/__init__.py`. This follows the maturin `python-source` convention and allows adding pure-Python utilities alongside native code in the future.

9. **`save()` on RenderedImage** — Uses `image` crate to encode RGBA pixels to PNG/JPEG/BMP based on file extension. This avoids requiring PIL just to save a file.

## J2 — Type Stubs, PyPI Setup, Documentation

### Package structure
- `pyproject.toml` — maturin build backend, `python-source = "python"`, `module-name = "pdfengine._native"`
- `python/pdfengine/__init__.py` — Re-exports all classes, sets `__version__`
- `python/pdfengine/__init__.pyi` — Full type stubs for IDE autocompletion and mypy
- `README.md` — Quick start, rendering, text extraction, thumbnails, search, NumPy, password-protected PDFs
- `.gitignore` — Excludes `__pycache__/`, `*.so`, `target/`

### Type stubs
Hand-written `.pyi` file covering all public classes and methods with full type annotations. Uses `Optional`, `List`, `Dict`, `bytes` types. PIL and NumPy return types annotated as `Any` since they're optional dependencies.

### Deferred items
- **PyPI publish workflow**: Requires PyPI account and token; `pyproject.toml` is publish-ready for `maturin build --release` + `twine upload`
- **Form field access**: `pdf-forms` crate integration prepared but not exposed — form engine API still evolving
- **Digital signature verification via Python**: `pdf-sign` dependency included but verification API not yet exposed as Python methods
- **Platform wheels**: maturin can produce wheels for manylinux, macOS universal2, Windows — requires CI matrix; deferred to CI setup phase
