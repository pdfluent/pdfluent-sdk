# Channel checklist — npm

Governed by `docs/release/PUBLISH_PROTOCOL.md`. Audit report goes to `benchmarks/runs/prepublish_audits/<package>-<version>.md`.

## Prepublish audit

- [ ] `git status --porcelain` empty.
- [ ] All publish-bound commits are on the publish branch.
- [ ] `package.json` `name`, `version`, `description`, `license`, `repository`, `homepage` populated.
- [ ] `license` field uses a valid SPDX expression (e.g. `MIT`, `Apache-2.0`, `MIT OR Apache-2.0`)
      or `SEE LICENSE IN LICENSE-OFFER`, and every file that pointer leads to is in `files`
      (`LICENSE-OFFER`, `LICENSE`, `LICENSE-COMMERCIAL`). npm rejects a LicenseRef expression,
      so the pointer is the offer's only spelling on this channel — and a pointer to a file
      the tarball omits states nothing.
- [ ] `private` is not `true` unless this is an internal-only package.
- [ ] Version is **new** — `npm view <pkg>@<version>` returns 404.
- [ ] `files` (or `.npmignore`) explicitly enumerates what ships — never rely on default-include for production packages.

## Package build command

```
npm pack --dry-run
```

Capture the file list as the package-list snapshot.

```
npm pack
```

This produces `<pkg>-<version>.tgz` in the package directory.

## Package content inspection

- [ ] Unpack `.tgz`: `tar -xzf <pkg>-<version>.tgz -C /tmp/audit-<pkg>-<version>/`.
- [ ] Top-level directory in the tarball is `package/`.
- [ ] `package/package.json`, `package/LICENSE` (and dual files if applicable), `package/README.md` present.
- [ ] Run leakage scan via `scripts/release/audit_package_tree.py`.
- [ ] Verify no `.git`, `.env`, `node_modules`, `.npmrc`, or test fixtures shipped that shouldn't be.
- [ ] Verify package size ≤ 50 MiB (or waiver). WASM bundles often approach this; record the value.

## Licence verification

- [ ] `package.json.license` matches the actual licence file.
- [ ] LICENSE file(s) present **inside the tarball**.
- [ ] For `MIT OR Apache-2.0`: both `LICENSE-MIT` and `LICENSE-APACHE` inside the tarball.
- [ ] Licence sha256 recorded in audit report.

## Dependency check

- [ ] `npm ls --omit=dev --all` succeeds and reports no unmet peer deps.
- [ ] No `*` / `latest` ranges on production deps.
- [ ] No `git://` / `file:` / `link:` deps in production.
- [ ] No deprecated transitive deps that we own; if upstream is deprecated, note it.

## Dry-run command

```
npm publish --dry-run --access public
```

Must list the same files captured by `npm pack --dry-run`.

## Publish command

```
npm publish --access public
```

For scoped packages on a private registry, configure `publishConfig.registry` in `package.json` instead of CLI args.

## Post-publish verification

- [ ] `npm view <pkg> version` reports the new version.
- [ ] `npm view <pkg> dist.tarball` and download the tarball from the registry CDN.
- [ ] Re-extract; verify LICENSE is present and content matches.
- [ ] `mkdir /tmp/smoke-<pkg> && cd /tmp/smoke-<pkg> && npm init -y && npm install <pkg>@<version>` resolves cleanly.
- [ ] Record PASS in the audit report.

## Rollback / yank / remediation

- npm allows `npm unpublish <pkg>@<version>` only within 72 hours of publish; after that, only `npm deprecate <pkg>@<version> "<reason>"` is available.
- For a licence-compliance defect older than 72 hours, file a request with the npm registry support to remove if absolutely required; otherwise deprecate + ship a fixed version.
- Yank/deprecate is recorded in the remediation report.

## Failure modes specific to npm

- **Scoped package access**: forgetting `--access public` on a scoped publish yields a 402. Re-run with the flag.
- **Two-factor auth**: ensure the publish token / OTP is fresh; expired OTP triggers a 401.
- **Registry mirror drift**: if publishing to a non-default registry, verify `npm config get registry` matches before publish.
