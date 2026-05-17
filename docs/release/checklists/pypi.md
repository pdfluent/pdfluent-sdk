# Channel checklist — PyPI

Governed by `docs/release/PUBLISH_PROTOCOL.md`. Audit report goes to `benchmarks/runs/prepublish_audits/<package>-<version>.md`.

## Prepublish audit

- [ ] `git status --porcelain` empty.
- [ ] `pyproject.toml` `[project]` has `name`, `version`, `description`, `license`, `readme`, `requires-python`, `authors`, `urls`.
- [ ] `[project].license` is set as `{file = "LICENSE"}` for proprietary, or `{text = "MIT"}` for SPDX.
- [ ] `classifiers` list does not contain stale "Development Status :: 1 - Planning" if the release is beta or stable.
- [ ] Version is **new** — `pip index versions <pkg>` does not list it.
- [ ] PyO3 packages: confirm `maturin` config and target wheels match (sdist + wheels for each supported abi).

## Package build commands

For pure-Python packages:

```
python -m build
```

For PyO3 / Rust-backed packages:

```
maturin build --release
maturin sdist
```

Outputs land in `dist/`.

## Package content inspection

- [ ] Inspect sdist: `tar -tzf dist/<pkg>-<version>.tar.gz | head -50` and `tar -xzf dist/<pkg>-<version>.tar.gz -C /tmp/audit-sdist/`.
- [ ] Inspect each wheel: `unzip -l dist/<pkg>-<version>-<abi>.whl`; extract to `/tmp/audit-wheel-<abi>/`.
- [ ] Top-level wheel contains `<pkg>-<version>.dist-info/` with `METADATA`, `WHEEL`, `LICENSE` (or `licenses/LICENSE`), `RECORD`.
- [ ] Run `scripts/release/audit_package_tree.py` on each unpacked artifact.
- [ ] Verify no `.pyc` / `__pycache__` / test data shipped that shouldn't be.
- [ ] Wheel size ≤ 50 MiB per artifact (PyO3 wheels with bundled binaries may need a waiver).

## Licence verification

- [ ] `LICENSE` (or dual `LICENSE-APACHE` / `LICENSE-MIT`) inside the sdist `.tar.gz`.
- [ ] `LICENSE` inside every wheel's `.dist-info/` directory.
- [ ] `pyproject.toml` `license` matches the file content.
- [ ] Licence sha256 recorded in audit report for each artifact.

## Dependency check

- [ ] `[project].dependencies` has no `*` / unbounded version specifiers.
- [ ] No yanked dep on PyPI (`pip index versions` shows yanked status).
- [ ] No `extras_require` ranges that pull yanked transitive deps for any extras combination.

## Dry-run command

```
python -m twine check dist/*
```

Validates metadata. Also use TestPyPI for a real upload dry-run:

```
python -m twine upload --repository testpypi dist/*
pip install --index-url https://test.pypi.org/simple/ <pkg>==<version>
```

## Publish command

```
python -m twine upload dist/*
```

Use an API token scoped to this project. **Never** use the legacy username/password.

## Post-publish verification

- [ ] `pip index versions <pkg>` lists the new version.
- [ ] `curl https://pypi.org/pypi/<pkg>/<version>/json` returns 200.
- [ ] Download the sdist and one wheel from `pypi.org/simple/<pkg>/` and re-verify LICENSE inside.
- [ ] `python -m venv /tmp/smoke-<pkg> && /tmp/smoke-<pkg>/bin/pip install <pkg>==<version>` resolves and installs.
- [ ] If a CLI entry point ships, run `/tmp/smoke-<pkg>/bin/<entry> --version` and confirm it matches.
- [ ] Record PASS in the audit report.

## Rollback / yank / remediation

- PyPI supports yanking via the web UI or `twine yank` (project-specific API token required).
- A yanked version disappears from the default `pip install <pkg>` resolution but remains downloadable with explicit `==<version>`.
- For licence non-compliance, yank + ship fixed version.
- Record in remediation report.

## Failure modes specific to PyPI

- **TestPyPI ≠ PyPI**: a successful TestPyPI upload does not guarantee PyPI acceptance; metadata is validated more strictly on prod.
- **Manylinux wheel tag mismatch**: wheels built outside `manylinux` containers may upload but not install on common Linux distributions. Use `auditwheel repair` for compiled wheels.
- **Trove classifier validation**: invalid `classifiers` strings produce a 400 on upload.
