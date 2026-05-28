# PDFluent Rollback / Deprecate Procedure

**Status:** mandatory · **Owner:** release captain (or on-call operator) · **Started:** 2026-05-28 (Tier-2 R5).

Standalone operator runbook for "we just shipped something defective; what now."
Consolidates the steps that were previously scattered across `PUBLISH_PROTOCOL.md`
§12–13, the per-channel rollback notes in `SDK_NON_XFA_PUBLISH_TRAIN_RUNBOOK.md`,
and the per-channel checklists, into one place an operator can follow under
pressure.

This document does **not** replace those sources — it points at them. The
yanking *policy* (when yanking is allowed at all) lives in
`PUBLISH_PROTOCOL.md` §13 and is the authoritative source. This runbook is
the *procedure* for executing a yank once the policy has decided it is
warranted.

---

## 0. Before you touch anything

> If you are reading this in a hurry: **stop the publish train first**.
> Halt every other publish in the same group. Then read sections 1 and 2
> before running a single yank command. The wrong yank is worse than the
> defect itself: it is a yank you cannot un-do.

| Stop-the-line action | Where |
|---|---|
| Halt the train | Tell every operator in the train to stop. Cancel any in-flight `release_manual` CI jobs. |
| Lock the publish branch | `git push origin --delete release/<train>` if the train branch is local-only; otherwise comment in the train channel that no further pushes are allowed. |
| Open an incident channel / ticket | Use whatever the team uses for ops incidents. The incident has an id of the form `YYYY-MM-DD-shortslug`; this id is the key for every file produced by this procedure. |

## 1. Decide: yank or deprecate?

The decision tree is in `PUBLISH_PROTOCOL.md` §13. Summarised here for
quick reference:

```
defect == hard-rule violation (missing licence, secret leak, corpus leak)?
  ├─ yes ──▶ YANK (where channel allows; deprecate where it does not)
  └─ no
     │
     defect == "known broken" that a fixed version supersedes?
       ├─ yes ──▶ YANK (where channel allows; deprecate where it does not)
       └─ no
          │
          defect == "this version is just old / outdated"?
            └─ yes ──▶ DO NOT YANK. Update release notes only.
```

A wrong-decision example: yanking `1.0.0-beta.7` because `1.0.0-beta.8` is
out is **forbidden** — see §13 "Do not yank to 'tidy up'." The cost of a
yank lands on every consumer who pinned the previous version.

If the answer is "do not yank": skip to §5 (replacement) and §7
(remediation report). The procedure here covers only the yank/deprecate
path.

## 2. Channel-specific yank commands

| channel | command | reversible? | hides from resolution? | notes |
|---|---|---|---|---|
| **crates.io** | `cargo yank --version <V> <CRATE>` | yes (`cargo yank --undo`) — but un-yanking a non-compliance yank is **forbidden** (re-exposes the defect) | new resolves skip it; existing `Cargo.lock`s keep using it | append-only registry; no delete; no re-upload of the same version after yank |
| **npm** | within 72h of publish: `npm unpublish @scope/pkg@<V>`; otherwise: `npm deprecate @scope/pkg@<V> "<reason>"` | unpublish irreversible (re-publish of same version disallowed); deprecate reversible (`npm deprecate @scope/pkg@<V> ""`) | unpublish hides; deprecate shows a warning on `npm install` | unpublish disallowed if any other public package depends on the version |
| **PyPI** | yank a release via the project web UI under "Manage / Releases / Yank" — there is no command-line yank; deletion is discouraged and not equivalent | yanked releases can be un-yanked via the same UI — but again, do not un-yank non-compliance yanks | yanked release stays installable by exact version pin; `pip install pkg` resolves to the highest non-yanked | per PEP 592 |
| **Maven Central** | drop the staging repository **before** release: `mvn nexus-staging:drop` (or via the Sonatype UI); **after** release: Central is immutable, no yank — file a ticket with Sonatype for take-down in a true compliance emergency, otherwise rely on a new patch version | "drop staging" reversible (re-stage); "after release" irreversible | dropped staging never reaches consumers; post-release entries cannot be hidden | the practical lesson is: catch defects in staging |
| **NuGet** | `dotnet nuget delete <PKG> <V> --source <feed> --api-key <KEY>` (UI: "Unlist") | unlisted package can be re-listed via the same UI | hidden from search and `dotnet add package`, but exact-version installs still succeed | NuGet does not allow hard delete |
| **WASM (npm-bundled)** | same as npm — `npm unpublish` or `npm deprecate` | as npm | as npm | wasm-only publish channel inherits npm semantics |
| **Binary release** (GitHub Release / GitLab Release) | UI: delete the release attachment; or via `gh release delete-asset <tag> <file>` / GitLab API equivalent | yes — re-uploadable | hidden | distribution storage; the most reversible of all channels |
| **GitLab Package Registry** | `glab package delete <name>/<version>` or via the GitLab API `DELETE /projects/:id/packages/:package_id` | yes (re-uploadable) | hidden | internal mirror; rollback is cheap |

## 3. Execute the yank

For each affected `(channel, package, version)`:

1. **Verify the policy decision.** Re-read §1 of this runbook and
   `PUBLISH_PROTOCOL.md` §13. If unsure, stop and escalate.
2. **Run the channel yank command from §2.** Save the exact command you
   ran and the registry response (stdout + stderr).
3. **Record the yank in the SHA ledger** (R6). This is mandatory and
   non-optional:
   ```bash
   python3 scripts/release/ledger_mark_yanked.py \
     --channel <crates_io|npm|pypi|maven|nuget|wasm|binary|gitlab> \
     --package <name> \
     --version <V> \
     --reason "<short string from §1 decision tree>" \
     --remediation-report "benchmarks/runs/remediation/<incident-id>.md"
   ```
   The ledger entry stays in the ledger forever (R6-3) — yanking is a
   status change, not a delete. Any auditor reading the ledger years
   later sees: "yes, this version was published, and yes, it was
   yanked on this date for this reason."
4. **Push the ledger update on the release branch** with a commit
   message of the form `chore(release): mark <pkg>@<V> yanked (see
   <incident-id>)`. Do NOT amend any prior commit; do NOT bypass any
   pre-push hook. The yank is part of the permanent history.
5. **Confirm the yank propagated** (give the registry 60s, then):
   - crates.io: `cargo search <crate>` shows a `(yanked)` annotation
     on the version.
   - npm: `npm view @scope/pkg@<V>` shows `deprecated` field or
     resolves with a 404 (after unpublish).
   - PyPI: the project's "Manage" UI shows the release as yanked.
   - NuGet: the package is no longer in `dotnet add package` search.
   - others: per the registry's verification endpoint.

## 4. Take-down / deprecation where yank is unavailable

For channels where post-release yank is unavailable (Maven Central
post-release; PyPI's restricted deletion), the operator MUST:

- Mark the version `yanked` in the ledger anyway (using
  `ledger_mark_yanked.py`) — the operator-side record is independent of
  whether the registry-side action was successful.
- Publish a deprecation notice via the registry's communication
  mechanism (Maven deprecation marker, PyPI release notes etc.).
- Ship the replacement version (§5) within 72 hours.

## 5. Prepare the replacement

The yank is containment, not remediation. The replacement is the
remediation. Per `PUBLISH_PROTOCOL.md` §12 steps 4–7:

1. Fix the defect on the source branch.
2. Re-run the full prepublish audit on the fixed version, including
   the check that catches the defect class. If the audit script did
   not previously catch this defect class, **add the check** before
   re-running.
3. Bump version. Choose the SemVer bump per the policy:
   - hard-rule violation (licence, secret, corpus leak): patch bump
     (e.g. `1.0.0-beta.8` → `1.0.0-beta.9`). The defect is *security/
     compliance*, not API.
   - "known broken" runtime bug: patch bump.
   - API regression (rare for a yank): minor or major per SemVer.
4. Republish the fixed version via the train.
5. Re-run post-publish verification on the new version
   (`scripts/release/ledger_verify.py --write …`).
6. Write the verify report under `benchmarks/runs/post_publish_verify/`
   and store its path on the ledger entry.

## 6. Update consumers

A yank does not push to anyone. Consumers find out via warnings or
404s. The release captain MUST:

- Update `CHANGELOG.md` with an entry for the yanked version, its
  reason, and the replacement.
- Post in whichever channel the team uses for downstream announcements
  (mailing list, Discord, GitHub Discussions, etc.).
- If the defect is security/compliance: file an advisory in
  `docs/security/advisories/` using the existing advisory template.

## 7. Write the remediation report

Mandatory per `PUBLISH_PROTOCOL.md` §12 step 3. Copy
`docs/release/templates/REMEDIATION_REPORT_TEMPLATE.md` to
`benchmarks/runs/remediation/<incident-id>.md` and fill every section.
Commit on the same branch as the ledger update (so the ledger entry's
`yanked_reason` reference resolves immediately).

The report MUST include:

- The exact yank command used per channel (so a future
  operator/auditor can reconstruct what was done).
- The sha256 of the defective artefact (already in the ledger; cross-
  reference rather than copy).
- The detection mechanism + whether the audit script caught it.
- The replacement version's audit + verify report links.
- The protocol-change required so the defect class cannot ship again.

## 8. Update the protocol

If the defect class was not previously caught by the audit script
(per `PUBLISH_PROTOCOL.md` §12 step 8), the next audit script run must
fail before reaching publish for an analogous defect. Add the check,
add a regression test for the check, and update this runbook or the
relevant per-channel checklist if the procedure itself needs a change.

The §8 update is what closes the incident. Until the audit script
catches the defect class, the incident is "open" in the sense that the
same defect can re-ship.

## 9. Verification of the closed incident

Before declaring the incident closed:

- [ ] Defective version is yanked on every affected registry (ledger
      entries all have `status=yanked`).
- [ ] Replacement version is published, verified, and ledger-recorded
      with `status=verified`.
- [ ] Audit script catches the defect class (regression test passes).
- [ ] Remediation report is committed under
      `benchmarks/runs/remediation/<incident-id>.md`.
- [ ] CHANGELOG and downstream communication updated.
- [ ] No open follow-up tasks in the incident channel.

A closed incident becomes a permanent reference — the next time a
defect of the same class threatens, the audit script catches it.

## 10. Cross-references

- `docs/release/PUBLISH_PROTOCOL.md` §12, §13 — policy + emergency
  remediation outline this runbook implements.
- `docs/release/SDK_NON_XFA_PUBLISH_TRAIN_RUNBOOK.md` — per-channel
  rollback notes (kept; this runbook supersedes their *procedural*
  guidance but their *credentials / dependencies* notes still apply).
- `docs/release/sha_ledger/README.md` (R6) — yank is recorded via
  `ledger_mark_yanked.py`; ledger entries persist forever.
- `docs/release/templates/REMEDIATION_REPORT_TEMPLATE.md` —
  mandatory report template.
- `docs/release/release_gate_contract.md` — gate definitions; a yank
  re-opens any gate that the defective version had passed.
