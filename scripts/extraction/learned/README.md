# Learned decisions — training (#598)

Offline, on the Mac, never on a rented machine.

1. `bash scripts/extraction/learned/setup_venv.sh` — builds `~/.cache/pdfluent/learned-venv` (outside the
   tree: the push gate scans scripts/) from
   `requirements.txt` and runs `smoke_threads.py`. Use **this** interpreter for everything below,
   never the anaconda base: its MKL numpy loads a second OpenMP runtime beside the libomp the
   LightGBM wheel links, and multi-threaded training then crashes with no traceback. Do not set
   `KMP_DUPLICATE_LIB_OK`; the setup refuses it.

   **The failure is silent.** Two OpenMP runtimes do not raise: the process is simply gone —
   no exception, no telling exit status, not reproducible on the same input — once in `fit`,
   twice in `predict` on 23-09 (T4), easily read as running out of memory. If a training run
   vanishes, check the interpreter first: `~/.cache/pdfluent/learned-venv/bin/python smoke_threads.py`.
2. Dump features: `cargo build --release -p pdf-learned --bin feature_dump`, then either
   `target/release/feature_dump <pdf> [--page-id ID]` or the streaming driver
   `dump_doclaynet.py --split train --out <file>.jsonl` (one shard on disk at a time).
   Schema `pdfluent.features/1`, described on #598.
3. Train: `$VENV/bin/python train.py --task <task> --features <jsonl> --out models/<name> --rust <path>`.
   The model, a manifest naming every training document, and a parity fixture land in `models/<name>`.
4. Export: `$VENV/bin/python export_rust.py --model models/<name>/model.json --out <path>.rs` — the
   path is the lane's; a decision's model goes beside the code that makes it.

What CI then holds you to: `learned_training_uses_no_rig_document.py` (no rig document, by document),
`a_learned_model_stays_in_budget.py` (≤ 5 MB and ≤ 5000 comparisons per prediction per model, ≤ 25 MB for all of them together),
`test_learned_export_is_reproducible.py` (the .rs is what the exporter makes of model.json), and a
parity test beside the generated code (margins within 1e-6 of LightGBM's; see
`crates/pdf-learned/tests/model_parity.rs`).
