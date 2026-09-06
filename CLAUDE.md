# XFA-Native-Rust — Project Conventions

## Project Overview
High-performance XFA (XML Forms Architecture) engine in Rust. Goal: full Adobe Reader parity for XFA forms, including dynamic reflow and FormCalc scripting.

## Source of Truth
- **XFA 3.3 Spec:** https://pdfa.org/norm-refs/XFA-3_3.pdf
- **FormCalc Reference:** https://helpx.adobe.com/pdf/aem-forms/6-2/formcalc-reference.pdf
- **Backlog:** See BACKLOG.md for structured epics and implementation order

## Architecture
Cargo workspace with 6 crates:
- `xfa-dom-resolver` — SOM path resolution, Template/Data DOM (XFA §3)
- `formcalc-interpreter` — FormCalc lexer, parser, interpreter (XFA §25)
- `xfa-layout-engine` — Box Model, pagination, reflow (XFA §4, §8)
- `pdfium-ffi-bridge` — PDFium FFI, rendering, UI events
- `xfa-golden-tests` — Visual regression testing pipeline
- `xfa-cli` — CLI entry point

## Tech Stack
- **Rust 2021 edition**, stable toolchain
- `roxmltree` for XML parsing (read-only DOM)
- `pdfium-render` for PDF rendering via PDFium
- `thiserror` for error types

## Step zero: does it already exist?

Before writing a line for a story, ask whether the work is already sitting in an
open pull request:

```
python3 scripts/ci/does_this_already_exist.py <path>
python3 scripts/ci/does_this_already_exist.py --symbol <name>
```

On 25-08-2026 five JNI entry points were wired up again that had been open since
23 August -- half a day, done twice, because the work existed in a request and
not in master. The same day, three infrastructure fixes stood independently on
four branches. That is a measurement problem rather than carelessness: nobody
could see that the work already existed.

Deliberately not a CI job. A job asks the question after the work is done, which
is the one moment at which the answer is worthless.

And the other half of the same rule: infrastructure changes -- CI, gates,
generated documents -- go to master, not onto a feature branch. Those are what
end up rebuilt three times.

## Coding Conventions
- Use `cargo fmt` before every commit
- Use `cargo clippy -- -D warnings` — no warnings allowed
- All public APIs must have doc comments
- Error handling: use `thiserror` + `Result<T, Error>`, no `.unwrap()` in library code
- Tests: `#[cfg(test)]` modules in each file + integration tests in `tests/`

## Autonomy Principle
Claude must be fully self-sufficient:
- Run all tests via `cargo test`
- Render PDFs to PNG and inspect visually (vision) for layout debugging
- Consult the XFA spec PDF directly for architectural decisions
- Never require human intervention for verification

## Communication style

`caveman` (level full) is the default for **status output and messages between
terminals**. The skill is vendored at `.claude/skills/caveman`.

It applies to: status lines, progress reports, and cross-session messages.

It does **not** apply to: PR and issue text, anything presenting evidence or a
measurement, commit messages, code, comments, documentation, and CI output.
Those stay in normal English.

The split is deliberate. Terse is safe where the reader only needs the state.
It is not safe where the reader needs the reasoning: this repository's guards
keep failing in ways that only a full sentence catches -- a check that cannot
fail, a register that outlives its subject, a SKIPPED reported as a pass. The
argument is the deliverable there, and compressing it removes the part that does
the work.

## Repository topology

One source, and a copy that says when it has stopped being one. The roles are
here because `scripts/ci/mirror_has_not_drifted.py` enforces a direction, and a
direction nobody wrote down is a setting somebody can change rather than a
decision that comes past review. `scripts/ci/the_topology_agrees_with_the_mirror_gate.py`
holds this table and that guard to saying the same thing.

| Repository | Git remote | Role |
|---|---|---|
| `github.com/pdfluent/engine` | `origin` | **source** — code, issues, pull requests, releases, and the branch every landing fast-forwards |
| `github.com/pdfluent/PDFluent-project` | `gitlab` | **backup** — a copy of the source and nothing else |
| `github.com/pdfluent/pdfluent-internal` | — | tracker: the issues this work is filed under. It still holds an old copy of the editor source, which is #291's to remove |
| `github.com/pdfluent/pdfluent` | — | the editor, public, its own history since 26-08-2026 (deliberately no common ancestor with the tracker repo) |
| `github.com/pdfluent/pdfluent-playground` | — | the website |

Two things this table exists to stop drifting:

**`origin` is the source.** It pointed at the mirror until 05-09-2026, and a
name that reads primary while resolving to a copy is not untidiness: every guard
that falls back to `origin/master` was comparing against a repository that ran
348 commits behind, so `territories_do_not_overlap.py` reported 62 files as a
branch's own work when none of them were, and `mr_staleness.py` measured how far
a branch had fallen behind the copy (#291). `github` stays as a second name for
the same URL because the landing scripts outside this repository name it;
`scripts/ci/origin_is_the_source_in_every_checkout.py` holds `origin` to the
source row of this table and refuses the backup under any name but `gitlab`.

**One checkout per repository on this machine.** Two checkouts are two answers
to every question, and the one you are standing in is not necessarily the one
that gets pushed — that is how a finished `lopdf` upgrade was found stranded in
a second copy. The same guard counts them: for this repository it refuses, for
the other rows it warns, because a landing here cannot fix a duplicate of the
website.

**GitLab stopped being a CI executor on 28-08-2026.** Two CI systems on one
four-core desktop kept each other busy, so the automatic GitLab pipelines were
switched off and the heavy work moved to ephemeral instances driven from GitHub.
The table said "CI executor + nightly backup" for three days after that was no
longer true, which is the reason a guard now reads this section: prose has no way
to be wrong out loud.

The nightly mirror cron is gone with the machine it ran on. What replaces it is
`scripts/infra/mirror_to_gitlab.sh`, run by hand, in front of a gate that refuses
a landing while the two have come apart — see `docs/ci/mirror.md`.

## Git Workflow
- `master` branch for stable code
- Feature branches: `epic-N/description` (e.g., `epic-1/som-path-resolver`)
- Conventional commit messages in English
- Use `commit-commands` plugin for commits
