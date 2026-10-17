#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Can a zero-shot model leave torch: ONNX export plus a tract load (J3).

    export_probe.py --model tatr-v1.1-all --weights weights/tatr --out out/

Per model: try `torch.onnx` export at opset 17, and load whatever comes out
in `tract`. The report names what exported, what loaded, and which ops tract
refused. A REPORT, NOT A BLOCKER: a model that does not export still gets its
zero-shot numbers; this always exits 0 once its arguments check out.

The known risk is an autoregressive decode loop: TableFormer runs inference
through `TableModel04_rs.predict`, a Python beam search over a variable
number of steps, and UniTable through the same shape of greedy loop, and
there is no module-level `forward(images)` to trace. The probe attempts the
loop anyway (few steps, so the attempt stays cheap) and then the image
encoder alone, so the report says which half is the gap instead of just
saying no.
"""
from __future__ import annotations

import argparse
import json
import pathlib
import re
import shutil
import subprocess
import sys
import traceback

HERE = pathlib.Path(__file__).resolve()
sys.path.insert(0, str(HERE.parent))
import eval_zeroshot as ev  # noqa: E402

OPSET = 17

# Lines of tract's complaint that name an operation it cannot run. Kept
# loose on purpose: the report carries the tails beside the gaps, so a gap
# this misses is still on the page.
GAP = re.compile(r"unsupport|unknown|unimplemented|not implemented|"
                 r"no tract|missing op|cannot lower", re.IGNORECASE)


def attempt(name: str, fn) -> dict:
    """Run one export attempt, and return its report row whatever happens."""
    try:
        out_path = fn()
    except Exception as exc:  # noqa: BLE001 - the failure IS the report
        return {"attempt": name, "ok": False,
                "reason": f"{type(exc).__name__}: {exc}"[-800:],
                "traceback_last": traceback.format_exc(limit=3)[-1500:]}
    size = out_path.stat().st_size if out_path.is_file() else 0
    return {"attempt": name, "ok": True, "reason": "",
            "file": out_path.name, "bytes": size}


def export_onnx(model, dummy, out_path: pathlib.Path, opset: int,
                dynamo: bool) -> None:
    """One ONNX export, classic tracer or dynamo, at the pinned opset."""
    import torch

    names = ("output",)
    if dynamo:
        exported = torch.onnx.export(model, dummy, dynamo=True)
        exported.optimize()
        exported.save(str(out_path))
    else:
        with open(out_path, "wb") as handle:
            torch.onnx.export(model, dummy, handle, opset_version=opset,
                              dynamo=False, output_names=list(names))
    # Whatever came out must at least parse: an export that wrote a broken
    # file is a failure with a nicer message, not a success.
    import onnx

    onnx.load(str(out_path))


def tract_report(onnx_path: pathlib.Path | None, tract_bin: str | None) -> dict:
    """`tract` on an exported file: version, load, and the ops it refused."""
    tract = shutil.which(tract_bin) if tract_bin else shutil.which("tract")
    if onnx_path is None:
        # Nothing to load outranks a missing loader: the export failed
        # first, and the report should say that rather than complain
        # about the tool that never got its input.
        version = ""
        if tract is not None:
            try:
                proc = subprocess.run(
                    [tract, "--version"], capture_output=True, text=True,
                    timeout=60)
                lines = ((proc.stdout or "") + (proc.stderr or "")
                         ).strip().splitlines()
                version = lines[0] if lines else ""
            except (OSError, subprocess.SubprocessError):
                version = ""
        return {"found": tract is not None, "version": version,
                "loaded": False, "reason": "nothing exported to load",
                "op_gaps": []}
    if tract is None:
        claimed = f" ({tract_bin})" if tract_bin else ""
        return {"found": False, "version": "",
                "loaded": False, "reason": f"no `tract` on PATH{claimed}",
                "op_gaps": []}
    try:
        version = subprocess.run(
            [tract, "--version"], capture_output=True, text=True,
            timeout=60)
        version_text = ((version.stdout or "") + (version.stderr or "")
                        ).strip().splitlines()
    except (OSError, subprocess.SubprocessError) as exc:
        return {"found": True, "version": "", "loaded": False,
                "reason": f"`tract --version` would not run: {exc}",
                "op_gaps": []}
    try:
        proc = subprocess.run(
            [tract, str(onnx_path), "dump"], capture_output=True, text=True,
            timeout=600)
    except (OSError, subprocess.SubprocessError) as exc:
        return {"found": True,
                "version": version_text[0] if version_text else "",
                "loaded": False, "reason": f"`tract dump` would not run: {exc}",
                "op_gaps": []}
    output = (proc.stdout or "") + (proc.stderr or "")
    gaps = sorted({line.strip() for line in output.splitlines()
                   if GAP.search(line)})[:30]
    if proc.returncode != 0:
        tail = "\n".join(output.splitlines()[-15:])
        return {"found": True,
                "version": version_text[0] if version_text else "",
                "loaded": False,
                "reason": f"`tract dump` exited {proc.returncode}: {tail}"[-800:],
                "op_gaps": gaps}
    return {"found": True,
            "version": version_text[0] if version_text else "",
            "loaded": True, "reason": "", "op_gaps": gaps}


def build_tableformer(weights_dir: pathlib.Path, config_name: str,
                      device: str):
    """The TableFormer module with pinned weights, plus its config."""
    import torch
    from safetensors.torch import load_model
    from docling_ibm_models.tableformer.models.table04_rs.tablemodel04_rs \
        import TableModel04_rs

    config = json.loads(
        (weights_dir / config_name).read_text(encoding="utf-8"))
    word_map = config["dataset_wordmap"]
    model = TableModel04_rs(config, {"word_map": word_map}, device)
    [weights] = sorted(weights_dir.glob("tableformer_*.safetensors"))
    missing, unexpected = load_model(model, str(weights), device=device)
    if missing or unexpected:
        raise ValueError(f"weights would not load: missing={missing} "
                         f"unexpected={unexpected}")
    model.eval()
    return model, config


def probe_tableformer(weights_dir: pathlib.Path, config_name: str,
                      out_dir: pathlib.Path, opset: int, device: str,
                      tract_bin: str | None) -> dict:
    import torch

    class _Loop(torch.nn.Module):
        """`predict` as a forward: the beam loop, few steps, narrow beam."""

        def __init__(self, inner) -> None:
            super().__init__()
            self.inner = inner

        def forward(self, imgs):
            _seq, _classes, coords = self.inner.predict(imgs, 8, 1)
            if isinstance(coords, torch.Tensor):
                return coords
            return torch.zeros((1, 1, 4))

    rows = []
    try:
        model, config = build_tableformer(weights_dir, config_name, device)
    except Exception as exc:  # noqa: BLE001 - the failure IS the report
        rows.append({"attempt": "load", "ok": False,
                     "reason": f"{type(exc).__name__}: {exc}"[-800:],
                     "traceback_last": traceback.format_exc(limit=3)[-1500:]})
        return {"exports": rows, "tract": tract_report(None, tract_bin),
                "note": "the model would not load; nothing was attempted"}
    size = int(config["dataset"].get("resized_image", 448))
    dummy = torch.zeros((1, 3, size, size))
    rows.append(attempt(
        "full model (decode loop)",
        lambda: (export_onnx(_Loop(model), dummy,
                             out_dir / "tableformer_full.onnx", opset, False),
                 out_dir / "tableformer_full.onnx")[1]))
    rows.append(attempt(
        "image encoder only",
        lambda: (export_onnx(model._encoder, dummy,
                             out_dir / "tableformer_encoder.onnx", opset, False),
                 out_dir / "tableformer_encoder.onnx")[1]))
    first = next((out_dir / r["file"] for r in rows
                  if r["ok"] and (out_dir / r["file"]).is_file()), None)
    return {"exports": rows, "tract": tract_report(first, tract_bin)}


def build_tatr(weights_dir: pathlib.Path, device: str):
    from transformers import TableTransformerForObjectDetection

    # `num_labels` stated as in the eval: the hub config does not carry it.
    model = TableTransformerForObjectDetection.from_pretrained(
        str(weights_dir), num_labels=6).to(device).eval()
    return model


def probe_tatr(weights_dir: pathlib.Path, out_dir: pathlib.Path, opset: int,
               device: str, tract_bin: str | None) -> dict:
    import torch

    rows = []
    try:
        model = build_tatr(weights_dir, device)
    except Exception as exc:  # noqa: BLE001 - the failure IS the report
        rows.append({"attempt": "load", "ok": False,
                     "reason": f"{type(exc).__name__}: {exc}"[-800:],
                     "traceback_last": traceback.format_exc(limit=3)[-1500:]})
        return {"exports": rows, "tract": tract_report(None, tract_bin),
                "note": "the model would not load; nothing was attempted"}
    dummy = torch.zeros((1, 3, 800, 800))
    rows.append(attempt(
        "DETR (classic tracer)",
        lambda: (export_onnx(model, dummy, out_dir / "tatr.onnx", opset, False),
                 out_dir / "tatr.onnx")[1]))
    if not rows[-1]["ok"]:
        rows.append(attempt(
            "DETR (dynamo)",
            lambda: (export_onnx(model, dummy, out_dir / "tatr_dynamo.onnx",
                                 opset, True),
                     out_dir / "tatr_dynamo.onnx")[1]))
    first = next((out_dir / r["file"] for r in rows
                  if r["ok"] and (out_dir / r["file"]).is_file()), None)
    return {"exports": rows, "tract": tract_report(first, tract_bin)}


def build_unitable(weights_dir: pathlib.Path, code_dir: pathlib.Path,
                   device: str):
    """The UniTable structure head with pinned weights, plus its adapter.

    Through the eval's own adapter, so the probe loads what the eval runs.
    An adapter refusal becomes a plain error here, so the probe reports it
    as a load row instead of exiting past the report.
    """
    adapter = ev.UniTableAdapter(weights_dir, code_dir, device)
    try:
        adapter.load()
    except SystemExit as exc:
        raise RuntimeError(str(exc)) from exc
    return adapter


def probe_unitable(weights_dir: pathlib.Path, code_dir: pathlib.Path,
                   out_dir: pathlib.Path, opset: int, device: str,
                   tract_bin: str | None) -> dict:
    import torch

    class _Loop(torch.nn.Module):
        """`_generate` as a forward: the greedy loop, eight steps."""

        def __init__(self, adapter) -> None:
            super().__init__()
            self.adapter = adapter

        def forward(self, imgs):
            vocab, model = self.adapter._heads["structure"]
            ids = self.adapter._generate(
                vocab, model, imgs, vocab.token_to_id("[html]"), 8,
                self.adapter._struct_white)
            return ids.float()

    class _Encoder(torch.nn.Module):
        """The image encoder alone: backbone, positions, encoder, norm."""

        def __init__(self, adapter) -> None:
            super().__init__()
            _vocab, self.model = adapter._heads["structure"]

        def forward(self, imgs):
            return self.model.encode(imgs)

    rows = []
    try:
        adapter = build_unitable(weights_dir, code_dir, device)
    except Exception as exc:  # noqa: BLE001 - the failure IS the report
        rows.append({"attempt": "load", "ok": False,
                     "reason": f"{type(exc).__name__}: {exc}"[-800:],
                     "traceback_last": traceback.format_exc(limit=3)[-1500:]})
        return {"exports": rows, "tract": tract_report(None, tract_bin),
                "note": "the model would not load; nothing was attempted"}
    dummy = torch.zeros((1, 3, ev.UNITABLE_IMAGE, ev.UNITABLE_IMAGE))
    rows.append(attempt(
        "full model (decode loop)",
        lambda: (export_onnx(_Loop(adapter), dummy,
                             out_dir / "unitable_full.onnx", opset, False),
                 out_dir / "unitable_full.onnx")[1]))
    rows.append(attempt(
        "image encoder only",
        lambda: (export_onnx(_Encoder(adapter), dummy,
                             out_dir / "unitable_encoder.onnx", opset, False),
                 out_dir / "unitable_encoder.onnx")[1]))
    first = next((out_dir / r["file"] for r in rows
                  if r["ok"] and (out_dir / r["file"]).is_file()), None)
    return {"exports": rows, "tract": tract_report(first, tract_bin)}


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--model", required=True, choices=sorted(ev.MODEL_ROWS))
    ap.add_argument("--weights", required=True)
    ap.add_argument("--unitable-code", default=None,
                    help="the pinned poloclub/unitable tree; required for "
                         "--model unitable only")
    ap.add_argument("--out", required=True)
    ap.add_argument("--device", default="cpu")
    ap.add_argument("--opset", type=int, default=OPSET)
    ap.add_argument("--tract", default=None,
                    help="the tract binary (default: first on PATH)")
    args = ap.parse_args(argv)

    if args.opset != OPSET:
        raise SystemExit(f"--opset {args.opset} is not this probe's opset: "
                         f"{OPSET} is pinned so two runs compare")
    row = ev.check_weights(args.model, pathlib.Path(args.weights))
    out_dir = pathlib.Path(args.out)
    out_dir.mkdir(parents=True, exist_ok=True)

    if args.model.startswith("tableformer-"):
        _row_key, _weights, config = ev.MODEL_ROWS[args.model]
        report = probe_tableformer(pathlib.Path(args.weights), config or "",
                                   out_dir, args.opset, args.device,
                                   args.tract)
    elif args.model == "tatr-v1.1-all":
        report = probe_tatr(pathlib.Path(args.weights), out_dir, args.opset,
                            args.device, args.tract)
    elif args.model == "unitable":
        if args.unitable_code is None:
            raise SystemExit("unitable needs --unitable-code: the pinned "
                             "poloclub/unitable tree the startup script "
                             "extracts")
        report = probe_unitable(pathlib.Path(args.weights),
                                pathlib.Path(args.unitable_code),
                                out_dir, args.opset, args.device, args.tract)
    else:  # pragma: no cover - argparse refuses these before this
        raise SystemExit(f"no export probe for --model {args.model}")

    report.update({"model": args.model, "opset": args.opset,
                   "weights_revision": row.get("revision"),
                   "weights_repo": row.get("repo_id")})
    (out_dir / "export_report.json").write_text(
        json.dumps(report, indent=2) + "\n", encoding="utf-8")
    for entry in report["exports"]:
        print(f"{entry['attempt']}: "
              f"{'exported' if entry['ok'] else 'FAILED'}"
              f"{'' if entry['ok'] else ' -- ' + entry['reason'][:200]}",
              flush=True)
    tract = report["tract"]
    print(f"tract: {tract['version'] or 'absent'} -- "
          f"{'loaded' if tract['loaded'] else tract['reason'][:200]}",
          flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
