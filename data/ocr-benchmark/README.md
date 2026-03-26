# OCR benchmark corpus

Local benchmark assets for evaluating OCR engines on public scanned-document datasets.

## Layout

- `funsd/`: FUNSD downloads/extractions live here.
- `sroie/`: SROIE downloads/extractions live here.
- `scans/`: reserved for extra local scans.
- `prepare_corpus.py`: converts supported datasets into `corpus.json`.
- `benchmark.py`: runs OCR engines against `corpus.json`.

The large dataset files, generated corpora, and benchmark results are intentionally ignored by git.

## Download

FUNSD:

```bash
curl -L -o data/ocr-benchmark/funsd/funsd.zip \
  "https://guillaumejaume.github.io/FUNSD/dataset.zip"
unzip -o data/ocr-benchmark/funsd/funsd.zip -d data/ocr-benchmark/funsd
```

SROIE:

```bash
curl -L -o data/ocr-benchmark/sroie/sroie.zip \
  "https://drive.google.com/uc?export=download&id=1ShItNWXyiY1tFDM5W02bceHuJjyeeJl2"
```

The official SROIE Google Drive link often requires a browser-confirmed/manual download. If the file is empty or not a zip, use the official task page and place the extracted files under `data/ocr-benchmark/sroie/`:

- https://rrc.cvc.uab.es/?ch=13

`prepare_corpus.py` also understands the common public mirror layout with `data/img`, `data/box`, and `data/key` subdirectories.

## API keys

Create a repo-local `.env` file if you want to benchmark cloud OCR:

```bash
MISTRAL_API_KEY=...
OPENROUTER_API_KEY=...
OPENROUTER_MODEL=...
```

`benchmark.py` loads `.env` automatically from the repository root.

## Run

```bash
python3 data/ocr-benchmark/prepare_corpus.py
python3 data/ocr-benchmark/benchmark.py --engines ocrs,tesseract
```

`benchmark.py` forces `CARGO_TARGET_DIR=/tmp/codex-ocr-corpus-target` for the `ocrs` engine subprocess.
