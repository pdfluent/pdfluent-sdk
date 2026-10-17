#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The export probe, with torch mocked and tract stubbed.

Every attempt row (success, failure, unloadable weights), the opset pin, the
tract load and its op-gap parsing, and a full `main()` run per adapter that
exits 0 with a report -- a probe that raised on a failed export would be a
blocker wearing a report's name. Standard library only.
"""
from __future__ import annotations

import contextlib
import json
import pathlib
import sys
import tempfile
import types

HERE = pathlib.Path(__file__).resolve()
sys.path.insert(0, str(HERE.parent))
import eval_zeroshot as ev  # noqa: E402
import export_probe as probe  # noqa: E402

FAILURES: list[str] = []


def check(name: str, got, want, why: str = "") -> None:
    if got != want:
        FAILURES.append(f"{name}: got {got!r}, want {want!r}"
                        + (f" -- {why}" if why else ""))


@contextlib.contextmanager
def stubbed(modules: dict[str, types.ModuleType]):
    saved = {name: sys.modules.get(name) for name in modules}
    sys.modules.update(modules)
    try:
        yield
    finally:
        for name, module in saved.items():
            if module is None:
                sys.modules.pop(name, None)
            else:
                sys.modules[name] = module


def fake_module(name: str, **attrs) -> types.ModuleType:
    module = types.ModuleType(name)
    for key, value in attrs.items():
        setattr(module, key, value)
    return module


class _FakeTensor:
    def __init__(self, shape=(1,)) -> None:
        self.shape = shape


class _FakeModule:
    def __init__(self, *args, **kwargs) -> None:
        pass


def torch_stub(export=None, dynamo_export=None):
    onnx = types.SimpleNamespace()
    if export is not None:
        onnx.export = export
    else:
        def export(model, dummy, *args, **kwargs):
            if kwargs.get("dynamo"):
                return dynamo_export(model, dummy)
            handle = args[0]
            handle.write(b"fake-onnx")
        onnx.export = export
    nn = types.SimpleNamespace(Module=_FakeModule)
    return fake_module("torch", onnx=onnx, nn=nn,
                       zeros=lambda shape: _FakeTensor(shape))


def write_script(path: pathlib.Path, body: str) -> str:
    path.write_text("#!/bin/sh\n" + body, encoding="utf-8")
    path.chmod(0o755)
    return str(path)


def test_attempt() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        target = pathlib.Path(tmp) / "model.onnx"
        row = probe.attempt("works", lambda: (target.write_bytes(b"12345"),
                                              target)[1])
        check("a success names its file",
              (row["ok"], row["file"], row["bytes"]),
              (True, "model.onnx", 5))

        def failing():
            raise RuntimeError("the decode loop is untraceable")

        row = probe.attempt("loop", failing)
        check("a failure is a row, not an exception",
              (row["ok"], "untraceable" in row["reason"]), (False, True))
        check("with the traceback tail beside it",
              "RuntimeError" in row.get("traceback_last", ""), True)


def test_tract_report() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        root = pathlib.Path(tmp)
        model = root / "m.onnx"
        model.write_bytes(b"x")
        missing = probe.tract_report(model, str(root / "no-such-tract"))
        check("a missing tract is reported, not raised",
              (missing["found"], missing["loaded"]), (False, False))

        healthy = write_script(root / "tract-ok",
                               'if [ "$1" = "--version" ]; then echo "tract 0.23.8"; exit 0; fi\n'
                               'echo "model loaded: 12 ops"; exit 0\n')
        loaded = probe.tract_report(model, healthy)
        check("a tract that loads says so",
              (loaded["found"], loaded["loaded"], loaded["version"]),
              (True, True, "tract 0.23.8"))
        check("and no gaps is no gaps", loaded["op_gaps"], [])

        groaning = write_script(
            root / "tract-gaps",
            'if [ "$1" = "--version" ]; then echo "tract 0.23.8"; exit 0; fi\n'
            'echo "error: Unsupported operator Loop (opset 17)"; echo "note: model has 40 ops"; exit 3\n')
        gappy = probe.tract_report(model, groaning)
        check("a failing load is reported", gappy["loaded"], False)
        check("with the exit code in the reason",
              "exited 3" in gappy["reason"], True)
        check("and the op gap parsed", gappy["op_gaps"],
              ["error: Unsupported operator Loop (opset 17)"])

        check("nothing exported is nothing loaded",
              probe.tract_report(None, healthy)["reason"],
              "nothing exported to load")


def transformers_stub(captured: dict):
    class FakeModel:
        @classmethod
        def from_pretrained(cls, directory, **kwargs):
            captured["dir"] = directory
            captured["num_labels"] = kwargs.get("num_labels")
            return cls()

        def to(self, device):
            return self

        def eval(self):
            return self

    return fake_module("transformers",
                       TableTransformerForObjectDetection=FakeModel)


def weights_dir(root: pathlib.Path, model: str, extra: dict) -> pathlib.Path:
    weights = root / "weights"
    weights.mkdir()
    for weights_file in ev.weights_files_of(model):
        (weights / weights_file).write_bytes(b"fake")
    register_row, _spec = ev.model_row(model)
    (weights / "revision.txt").write_text(register_row["revision"] + "\n")
    for name, content in extra.items():
        (weights / name).write_text(content)
    return weights


def test_probe_tatr() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        root = pathlib.Path(tmp)
        weights = weights_dir(root, "tatr-v1.1-all", {})
        out = root / "out"
        out.mkdir()
        captured: dict = {}
        tract = write_script(root / "tract",
                             'if [ "$1" = "--version" ]; then echo "tract 0.23.8"; exit 0; fi\n'
                             'echo ok; exit 0\n')
        modules = {"torch": torch_stub(),
                   "transformers": transformers_stub(captured),
                   "onnx": fake_module("onnx", load=lambda _path: object())}
        with stubbed(modules):
            report = probe.probe_tatr(weights, out, 17, "cpu", tract)
        check("the model loads from the weights", captured.get("dir"),
              str(weights))
        check("one attempt, the classic tracer",
              [(r["attempt"], r["ok"]) for r in report["exports"]],
              [("DETR (classic tracer)", True)])
        check("and tract loads it", report["tract"]["loaded"], True)

        def broken(_path):
            raise ValueError("not an ONNX file")

        modules["onnx"] = fake_module("onnx", load=broken)
        out2 = root / "out2"
        out2.mkdir()
        with stubbed(modules):
            report = probe.probe_tatr(weights, out2, 17, "cpu", tract)
        kinds = [(r["attempt"], r["ok"]) for r in report["exports"]]
        check("a broken file fails the classic attempt",
              kinds[0], ("DETR (classic tracer)", False))
        check("and the dynamo fallback is attempted", len(kinds), 2,
              "one exporter failing is not the report")


def test_probe_tatr_load_failure() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        root = pathlib.Path(tmp)
        weights = weights_dir(root, "tatr-v1.1-all", {})
        out = root / "out"
        out.mkdir()

        class Refusing:
            @classmethod
            def from_pretrained(cls, _directory, **kwargs):
                raise RuntimeError("weights are not a DETR")

        modules = {"torch": torch_stub(),
                   "transformers": fake_module(
                       "transformers",
                       TableTransformerForObjectDetection=Refusing)}
        with stubbed(modules):
            report = probe.probe_tatr(weights, out, 17, "cpu", None)
        check("unloadable weights are a load row",
              [(r["attempt"], r["ok"]) for r in report["exports"]],
              [("load", False)])
        check("and tract has nothing to load",
              report["tract"]["reason"], "nothing exported to load")


def docling_stub(captured: dict, predict_ok=True):
    class FakeEncoder:
        pass

    class FakeModel(_FakeModule):
        def __init__(self, config, init_data, device) -> None:
            super().__init__()
            captured["config"] = config
            self._encoder = FakeEncoder()

        def eval(self):
            return self

        def predict(self, imgs, max_steps, beam):
            if not predict_ok:
                raise RuntimeError("beam search is untraceable")
            return ["fcel"], None, None

    tablemodel = fake_module(
        "docling_ibm_models.tableformer.models.table04_rs.tablemodel04_rs",
        TableModel04_rs=FakeModel)
    safetensors_torch = fake_module(
        "safetensors.torch", load_model=lambda *a, **k: ([], []))
    return {
        "docling_ibm_models": fake_module("docling_ibm_models"),
        "docling_ibm_models.tableformer": fake_module(
            "docling_ibm_models.tableformer"),
        "docling_ibm_models.tableformer.models": fake_module(
            "docling_ibm_models.tableformer.models"),
        "docling_ibm_models.tableformer.models.table04_rs": fake_module(
            "docling_ibm_models.tableformer.models.table04_rs"),
        "docling_ibm_models.tableformer.models.table04_rs.tablemodel04_rs":
            tablemodel,
        "safetensors": fake_module("safetensors"),
        "safetensors.torch": safetensors_torch,
    }


def test_probe_tableformer() -> None:
    config = {"dataset_wordmap": {"word_map_tag": {"<pad>": 0}},
              "dataset": {"resized_image": 448}}
    with tempfile.TemporaryDirectory() as tmp:
        root = pathlib.Path(tmp)
        weights = weights_dir(root, "tableformer-fast",
                              {"tm_config_fast.json": json.dumps(config)})
        out = root / "out"
        out.mkdir()
        captured: dict = {}

        def export(model, dummy, *args, **kwargs):
            raise RuntimeError("cannot trace the decode loop")

        modules = {"torch": torch_stub(export=export),
                   "onnx": fake_module("onnx", load=lambda _p: object())}
        modules.update(docling_stub(captured))
        tract = write_script(root / "tract",
                             'if [ "$1" = "--version" ]; then echo "tract 0.23.8"; exit 0; fi\n'
                             'echo ok; exit 0\n')
        with stubbed(modules):
            report = probe.probe_tableformer(weights, "tm_config_fast.json",
                                             out, 17, "cpu", tract)
        check("the full model is attempted first",
              report["exports"][0]["attempt"],
              "full model (decode loop)")
        check("and its failure does not end the probe",
              [r["attempt"] for r in report["exports"]],
              ["full model (decode loop)", "image encoder only"],
              "the encoder row says which half is the gap")
        check("both fail here", [r["ok"] for r in report["exports"]],
              [False, False])

        def flaky(model, dummy, *args, **kwargs):
            if hasattr(model, "_encoder"):
                raise RuntimeError("cannot trace the decode loop")
            args[0].write(b"fake-onnx")

        modules["torch"] = torch_stub(export=flaky)
        out2 = root / "out2"
        out2.mkdir()
        with stubbed(modules):
            report = probe.probe_tableformer(weights, "tm_config_fast.json",
                                             out2, 17, "cpu", tract)
        check("an exporting encoder still loads in tract",
              (report["exports"][1]["ok"], report["tract"]["loaded"]),
              (True, True))


def test_probe_unitable() -> None:
    class FakeAdapter:
        def __init__(self) -> None:
            self._heads = {"structure": (FakeUnitableVocab(), object())}
            self._struct_white = [1]

        def _generate(self, vocab, model, image, prefix, max_len,
                      whitelist):
            return FakeUnitableIds()

    class FakeUnitableVocab:
        def token_to_id(self, _token):
            return 1

    class FakeUnitableIds:
        def float(self):
            return self

    real_build = probe.build_unitable
    probe.build_unitable = lambda _w, _c, _d: FakeAdapter()
    try:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            weights = weights_dir(root, "unitable", {})
            out = root / "out"
            out.mkdir()
            tract = write_script(
                root / "tract",
                'if [ "$1" = "--version" ]; then echo "tract 0.23.8"; exit 0; fi\n'
                'echo ok; exit 0\n')
            modules = {"torch": torch_stub(),
                       "onnx": fake_module("onnx", load=lambda _p: object())}
            with stubbed(modules):
                report = probe.probe_unitable(weights, root / "code", out,
                                              17, "cpu", tract)
            check("the loop is attempted first, then the encoder",
                  [r["attempt"] for r in report["exports"]],
                  ["full model (decode loop)", "image encoder only"])
            check("both export here",
                  [r["ok"] for r in report["exports"]], [True, True])
            check("and tract loads the first",
                  report["tract"]["loaded"], True)

            def refusing(_weights, _code, _device):
                raise RuntimeError("no code here")

            probe.build_unitable = refusing
            out2 = root / "out2"
            out2.mkdir()
            with stubbed(modules):
                report = probe.probe_unitable(weights, root / "code", out2,
                                              17, "cpu", tract)
            check("an unloadable model is a load row",
                  [(r["attempt"], r["ok"]) for r in report["exports"]],
                  [("load", False)])
    finally:
        probe.build_unitable = real_build

    with tempfile.TemporaryDirectory() as tmp:
        root = pathlib.Path(tmp)
        weights = weights_dir(root, "unitable", {})
        thin = root / "thin"
        (thin / "src").mkdir(parents=True)
        try:
            probe.build_unitable(weights, thin, "cpu")
            FAILURES.append("a partial code extract loaded")
        except RuntimeError as exc:
            check("a refusal becomes a load-row error, not an exit",
                  "holds no" in str(exc), True)


def test_main() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        root = pathlib.Path(tmp)
        weights = weights_dir(root, "tatr-v1.1-all", {})
        out = root / "out"
        captured: dict = {}
        tract = write_script(root / "tract",
                             'if [ "$1" = "--version" ]; then echo "tract 0.23.8"; exit 0; fi\n'
                             'echo ok; exit 0\n')
        modules = {"torch": torch_stub(),
                   "transformers": transformers_stub(captured),
                   "onnx": fake_module("onnx", load=lambda _p: object())}
        argv = ["--model", "tatr-v1.1-all", "--weights", str(weights),
               "--out", str(out), "--tract", tract]
        with stubbed(modules):
            check("main exits 0", probe.main(argv), 0)
        report = json.loads((out / "export_report.json").read_text())
        check("the report names its model and opset",
              (report["model"], report["opset"]), ("tatr-v1.1-all", 17))
        check("and the weights revision",
              report["weights_revision"],
              ev.model_row("tatr-v1.1-all")[0]["revision"])
        check("and the tract verdict", report["tract"]["loaded"], True)

        # Under stubs: a broken pin check must fail as a check, not by
        # reaching for real torch past the refusal.
        with stubbed(modules):
            try:
                probe.main(["--model", "tatr-v1.1-all", "--weights",
                            str(weights), "--out", str(out), "--opset", "16"])
                FAILURES.append("opset 16 was accepted")
            except SystemExit as exc:
                check("the opset is pinned", "17" in str(exc), True)
            try:
                probe.main(["--model", "slanet-plus", "--weights",
                            str(weights), "--out", str(out)])
                FAILURES.append("slanet-plus was probed")
            except SystemExit as exc:
                check("slanet-plus is not a choice here either",
                      exc.code == 2, True,
                      "argparse refuses it before the probe runs")


def main() -> int:
    test_attempt()
    test_tract_report()
    test_probe_tatr()
    test_probe_tatr_load_failure()
    test_probe_tableformer()
    test_probe_unitable()
    test_main()
    if FAILURES:
        print(f"[test-export-probe] {len(FAILURES)} check(s) failed:",
              file=sys.stderr)
        for line in FAILURES:
            print(f"  - {line}", file=sys.stderr)
        return 1
    print("OK export_probe: attempts report, tract gaps parse, the opset is "
          "pinned, all three models probe, and main exits 0 with a report")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
