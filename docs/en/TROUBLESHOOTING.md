# PDFluent — Troubleshooting

Symptom → cause → fix, cross-linked to the [error catalogue](../error_catalogue.md)
and [per-binding quickstart](quickstart-bindings.md). Non-XFA core; XFA
fidelity is a separate beta track.

## Install / build

| Symptom | Cause | Fix |
| --- | --- | --- |
| `cargo` cannot find `pdfluent` from crates.io | Not published yet (beta) | Use the in-tree dev dependency `pdfluent = { path = "crates/pdfluent" }`; registry install lands at GA. |
| `pip install pdfluent` fails / wrong version | PyPI package not published yet | Dev install: `pip install -e crates/pdf-python`. |
| npm package not found | npm package not published yet | Build the node binding locally (napi-rs) and depend on it by path. |
| Build is slow / huge target dir | Full workspace build | Build only the crate/binding you need (`-p pdfluent`). |

## Native library / C ABI load

| Symptom | Cause | Fix |
| --- | --- | --- |
| linker cannot find `libpdfluent` | C-ABI lib not built/on path | Build `crates/pdf-capi`; ensure the shared lib + `pdfluent.h` are on the link/include path (see `pdfluent-examples/c/strict-api/`). |
| segfault calling C ABI | misuse of ownership/free contract | Follow the example's alloc/free pairing; do not free SDK-owned pointers twice (FFI ownership doc is a Quality-milestone item). |

## WASM loading

| Symptom | Cause | Fix |
| --- | --- | --- |
| module fails to instantiate in browser | wrong asset path / MIME | Serve the `.wasm` with `application/wasm`; await `init()` before calls (see `pdfluent-examples/wasm/strict-ts-edit/`). |
| works in Node, not browser | environment difference | Use the browser entry; browser load/init timing is tracked under Performance (PF-8). |

## License activation

| Symptom | Cause | Fix |
| --- | --- | --- |
| output is watermarked/marked | running in Trial mode | Activate with your key (`set_license_key` / `PDFLUENT_LICENSE_KEY`); confirm via `license_info`. |
| activation fails | invalid/expired/wrong-signed key | Check the typed license error code in the catalogue; no silent fallback occurs after a failed activation — the call returns an error. |

## Malformed / encrypted PDFs

| Symptom | Cause | Fix |
| --- | --- | --- |
| open returns a typed error, not a panic | malformed/truncated/garbage input | Expected: the SDK never panics on hostile input; match the typed error (see catalogue). |
| open of encrypted file fails | password required / wrong password | Open with the password option; wrong password yields a typed `DecryptionFailed`-class error. |
| `ResourceLimitExceeded` | input exceeds a configured limit (size/depth/operators/pixels/stream) | Raise the relevant limit if the input is trusted; defaults guard against bombs. |

## Platform / CI

| Symptom | Cause | Fix |
| --- | --- | --- |
| example won't run without arguments | no input path given | Examples fall back to `tests/corpus-mini/multi-page.pdf` during dev; pass a path otherwise. |
| GitHub links in docs | GitLab is canonical | Report it — the docs drift checker forbids GitHub URLs in user-facing docs. |
