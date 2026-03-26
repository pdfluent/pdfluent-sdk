#!/usr/bin/env python3
"""Convert OCR datasets into benchmark-friendly image/text pairs."""

from __future__ import annotations

import json
from pathlib import Path


BENCH_DIR = Path(__file__).resolve().parent


def collapse_whitespace(text: str) -> str:
    return " ".join(text.split())


def prepare_funsd() -> list[dict[str, str]]:
    """FUNSD: extract text from annotation JSONs."""
    base = BENCH_DIR / "funsd" / "dataset"
    if not base.exists():
        print(f"[warn] FUNSD dataset not found at {base}")
        return []

    pairs: list[dict[str, str]] = []
    for split in ("training_data", "testing_data"):
        ann_dir = base / split / "annotations"
        img_dir = base / split / "images"
        if not ann_dir.exists():
            print(f"[warn] Missing FUNSD annotations directory: {ann_dir}")
            continue

        for ann_file in sorted(ann_dir.glob("*.json")):
            data = json.loads(ann_file.read_text())
            texts = []
            for entry in data.get("form", []):
                text = collapse_whitespace(entry.get("text", ""))
                if text:
                    texts.append(text)

            full_text = " ".join(texts).strip()
            img_file = img_dir / ann_file.with_suffix(".png").name
            if img_file.exists() and full_text:
                pairs.append(
                    {
                        "image": str(img_file),
                        "text": full_text,
                        "source": "funsd",
                        "split": split,
                        "id": ann_file.stem,
                    }
                )

    print(f"[info] FUNSD pairs: {len(pairs)}")
    return pairs


def parse_sroie_box_file(path: Path) -> str:
    rows: list[tuple[int, int, str]] = []
    for raw_line in path.read_text(errors="replace").splitlines():
        line = raw_line.strip()
        if not line:
            continue

        parts = line.split(",", 8)
        if len(parts) != 9:
            continue

        try:
            xs = [int(parts[0]), int(parts[2]), int(parts[4]), int(parts[6])]
            ys = [int(parts[1]), int(parts[3]), int(parts[5]), int(parts[7])]
        except ValueError:
            continue

        text = collapse_whitespace(parts[8])
        if text:
            rows.append((min(ys), min(xs), text))

    rows.sort()
    return "\n".join(text for _, _, text in rows)


def parse_sroie_key_file(path: Path) -> str:
    data = json.loads(path.read_text())
    if isinstance(data, dict):
        values = [collapse_whitespace(str(value)) for value in data.values()]
        return "\n".join(value for value in values if value)
    return ""


def prepare_sroie() -> list[dict[str, str]]:
    """SROIE: support common official/manual and mirror layouts."""
    base = BENCH_DIR / "sroie"
    if not base.exists():
        return []

    img_dirs = [
        base / "data" / "img",
        base / "img",
        base / "images",
        base / "train" / "img",
        base / "training" / "img",
    ]
    img_dir = next((path for path in img_dirs if path.exists()), None)
    if img_dir is None:
        print(f"[warn] No SROIE image directory found under {base}")
        return []

    pairs: list[dict[str, str]] = []
    for image_path in sorted(img_dir.glob("*")):
        if image_path.suffix.lower() not in {".jpg", ".jpeg", ".png"}:
            continue

        stem = image_path.stem
        text = ""
        source_path = None

        box_candidates = [
            base / "data" / "box" / f"{stem}.csv",
            base / "box" / f"{stem}.csv",
            base / "annotations" / f"{stem}.csv",
        ]
        for candidate in box_candidates:
            if candidate.exists():
                text = parse_sroie_box_file(candidate)
                source_path = candidate
                break

        if not text:
            text_candidates = [
                base / "data" / "key" / f"{stem}.json",
                base / "key" / f"{stem}.json",
                base / "annotations" / f"{stem}.json",
                base / "ocr_gt" / f"{stem}.txt",
                base / "gt" / f"{stem}.txt",
                image_path.with_suffix(".txt"),
            ]
            for candidate in text_candidates:
                if not candidate.exists():
                    continue
                if candidate.suffix == ".json":
                    text = parse_sroie_key_file(candidate)
                else:
                    text = collapse_whitespace(candidate.read_text(errors="replace"))
                source_path = candidate
                if text:
                    break

        if not text:
            continue

        pairs.append(
            {
                "image": str(image_path),
                "text": text,
                "source": "sroie",
                "id": stem,
                "ground_truth": str(source_path) if source_path else "",
            }
        )

    print(f"[info] SROIE pairs: {len(pairs)}")
    return pairs


def main() -> int:
    pairs = []
    pairs.extend(prepare_funsd())
    pairs.extend(prepare_sroie())

    if not pairs:
        print("[error] No image/text pairs found. Download and extract FUNSD or SROIE first.")
        return 1

    output = BENCH_DIR / "corpus.json"
    output.write_text(json.dumps(pairs, indent=2))

    print(f"[done] Corpus: {len(pairs)} image-text pairs -> {output}")
    for pair in pairs[:5]:
        print(
            f"  {pair['source']}:{pair['id']} -> "
            f"{len(pair['text'])} chars ({Path(pair['image']).name})"
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
