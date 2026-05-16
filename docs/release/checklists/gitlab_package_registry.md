# Channel checklist — GitLab package registry

Governed by `docs/release/PUBLISH_PROTOCOL.md`. GitLab's package registry hosts multiple package types (Maven, npm, PyPI, generic, Conan, NuGet, Composer, Debian, Rubygems, Terraform, Helm). For Maven / npm / PyPI uploads to GitLab, **the per-language checklist applies in full** — this one adds GitLab-specific checks.

## Prepublish audit

- [ ] `git status --porcelain` empty.
- [ ] Target GitLab project ID and registry URL are correct: `https://gitlab.com/api/v4/projects/<id>/packages/<format>/…`.
- [ ] CI/CD deploy token or personal-access token has `write_package_registry` scope and **only** that scope (no superuser tokens).
- [ ] Token is loaded from a secrets manager / `glab auth status`, not pasted into shell history.
- [ ] Generic packages: package `name` and `version` are URL-safe, not yet present in the registry.

## Per-format checklists

GitLab is a transport — the per-format rules still apply.

- For **Maven** uploads: see `docs/release/checklists/maven.md`. Set `<distributionManagement>` to the GitLab Maven endpoint.
- For **npm** uploads: see `docs/release/checklists/npm.md`. Set `publishConfig.registry` to the GitLab npm endpoint and ensure `.npmrc` is `${GITLAB_NPM_ENDPOINT}/:_authToken=${CI_JOB_TOKEN}`.
- For **PyPI** uploads: see `docs/release/checklists/pypi.md`. Use the `pypi.gitlab` repository alias in `~/.pypirc` or pass `--repository-url`.
- For **generic** uploads (e.g. binary artifacts): see `docs/release/checklists/binary_release.md` plus the generic-format steps below.

## Generic package upload

Generic packages allow arbitrary file uploads scoped by `name` + `version` + filename:

```
curl --header "PRIVATE-TOKEN: $TOKEN" \
     --upload-file <file> \
     "https://gitlab.com/api/v4/projects/<id>/packages/generic/<name>/<version>/<filename>"
```

For each file:

- [ ] Run `scripts/release/audit_package_tree.py` against the file (if it's a tree archive) or the file's parent directory.
- [ ] Confirm sha256 of the upload payload before issuing the `curl`.
- [ ] Record sha256 in the audit report.
- [ ] After upload, download the file back and re-verify sha256.

## Licence verification

- [ ] If the registry serves a tree archive, the archive contains the licence file(s).
- [ ] If the registry serves single binaries, the licence is published next to it (e.g. `LICENSE` upload alongside `xfa-cli-linux-amd64`).
- [ ] Per-format licence rules apply.

## Dependency check

- [ ] Same as the per-format checklist; GitLab does not relax dependency rules.

## Dry-run / staging

- GitLab does not have a separate staging registry; the closest substitute is:
  - Use a **non-default branch tag** (e.g. `v1.0.0-beta.5-rc.1`) as the package version for the rehearsal upload.
  - Verify the artifact arrives correctly.
  - Then **publish the final version** under the real tag.
- Alternatively, upload to a dedicated `staging/` project first.

## Publish commands

Examples:

```
# Maven
mvn -DskipTests deploy

# npm
npm publish --registry "$GITLAB_NPM_ENDPOINT"

# PyPI
twine upload --repository gitlab-pypi dist/*

# Generic
curl --header "JOB-TOKEN: $CI_JOB_TOKEN" \
     --upload-file dist/installer.dmg \
     "$CI_API_V4_URL/projects/$CI_PROJECT_ID/packages/generic/desktop/$VERSION/installer.dmg"
```

## Post-publish verification

- [ ] `GET /api/v4/projects/<id>/packages` includes the new package.
- [ ] Download the artifact via the registry API and re-verify sha256 + licence content.
- [ ] If a downstream pipeline consumes the package, run it against the new version.
- [ ] Record PASS in the audit report.

## Rollback / yank / remediation

- GitLab supports `DELETE /api/v4/projects/<id>/packages/<package_id>` to remove a package version.
- **Deletion does not retroactively notify consumers** — also publish a fixed version and announce.
- For licence non-compliance, delete + replace, then add a `deprecated_at` field to the next version's metadata if your format supports it.

## Failure modes specific to GitLab

- **Token scope too narrow / wrong project**. The API responds 403; confirm the token has `write_package_registry` and references the correct project ID.
- **Tag–package version mismatch**. CI pipelines that derive `$VERSION` from `$CI_COMMIT_TAG` can publish under the wrong version when the tag is rewritten — pin the value early in the pipeline.
- **Race with pipeline parallelism**. Two jobs uploading the same name/version concurrently can produce 409 conflicts; serialise package-upload jobs.
