# Contributing to PDFluent

## How a change gets in

Fork, branch, open a pull request against the default branch — `main` on the
public repository, `master` on the private engine this tree is seeded from.
There is no other route: it takes fast-forwards only, force-pushes to it are
refused, and the
protection is set from `docs/PUBLIC_BRANCH_PROTECTION.toml` rather than by hand,
so what is required of your pull request is written down in this repository and
not only in a settings page you cannot see.

**Two checks run on it, and both must pass.**

| Check | What it refuses |
|---|---|
| No commit publishes a personal address | a commit authored from an address that maps to no account, so the work counts towards nobody and publishes somebody's inbox |
| Every commit carries a sign-off | a commit written since the DCO landed without a matching `Signed-off-by` — see below |

Both live in `.github/workflows/public-pull-request.yml`, both run on a hosted
runner, and both run their own tests first: a guard that has not proved it can
go red says nothing when it is green.

**Before you open it**, run what the checks run:

```
cargo fmt --all --check
cargo clippy --workspace -- -D warnings
cargo test -p pdfluent
```

One fix per pull request. A fix without a regression test is not finished — the
rule this repository holds itself to is that nothing is done if no test that
runs in CI covers it.

**A vulnerability does not go in a pull request or a public issue.**
[SECURITY.md](SECURITY.md) says where it goes.


## Sign off every commit

```
git commit -s
```

That adds one line, `Signed-off-by: Your Name <your@address>`, and by adding it
you certify the Developer Certificate of Origin in `docs/contribution/DCO.txt`:
that the work is yours to give.

**This is not policy, and here is what breaks without it.** PDFluent is licensed
twice — AGPLv3 for everyone, and a commercial licence for buyers who cannot
publish their own source (`LICENSE`). That construction rests on one thing: the
rights to both halves sit with one party. A contribution merged without the
right to relicense it does not stop the AGPL half. It stops the *commercial*
half, permanently, because the tree then holds code we may ship under one
licence and not the other.

It breaks **at the merge, not at the build.** No job goes red, no test fails,
nothing appears in a log. The first person to notice is a buyer's lawyer asking
who owns what, and by then the commit is months down with other work on top of
it. Undoing it means finding an author who signed nothing and has no reason to
reply.

Two gates enforce it, and they run where the merge happens rather than only in
CI: `scripts/ci/every_commit_since_the_cutoff_is_signed.py` before a push, and
`scripts/ci/licence_signoff.py` on the pull request. Both apply from the cutoff
recorded in those files — history written before the decision is not signed
retroactively.

**If you wrote it with somebody else,** the `Co-authored-by:` trailer names them
and certifies nothing on their behalf. They add their own `Signed-off-by:` line;
both gates refuse a co-author who signed off on nothing, because a co-author is
recorded in that trailer and in no field git otherwise exposes.

### What a sign-off does not do

It certifies provenance. It does **not** assign us the right to relicense your
contribution commercially. So a contribution to a crate on PDFluent's side of
`docs/licensing/boundary.toml` needs a CLA, and there is no CLA yet — until
there is, such a contribution cannot be merged. The reasoning, and the condition
that ends it, are in `docs/contribution/why-a-dco-and-not-a-cla.md`.


## Fix & Retest Workflow

When fixing bugs discovered by the corpus test runner, follow this process:

### 1. Pick a cluster

Use `xfa-test-runner clusters` to find the highest-impact error cluster. Prioritize panics first, then failures with the most affected PDFs.

### 2. Reproduce locally

```bash
xfa-test-runner download-examples \
  --test parse --category invalid_xref \
  --output tests/regression/fixtures/ --limit 5
```

Run the failing test against the smallest reproducer to confirm the issue.

### 3. Understand root cause

Read the relevant ISO 32000-2 or ISO 19005 spec section. Understand the full feature, not just the subset that triggers the bug.

### 4. Implement a generic fix

- Implement the WHOLE feature, not a special-case hack for one PDF
- No `if pdf_matches_this_specific_pattern { special_case }` workarounds
- No `unwrap()` or `todo!()` in library code
- No fixes without understanding the root cause

### 5. Add regression tests

Add 3-5 example PDFs as regression tests in the relevant crate's `tests/` directory:

```rust
#[test]
fn regression_cl0042_cmap_format6() {
    let pdf_data = include_bytes!("fixtures/cl0042_govdocs_023456.pdf");
    let text = pdf_extract::extract_text(pdf_data).expect("should not panic");
    assert!(!text.is_empty(), "should extract non-empty text");
}
```

Fixture rules:
- Keep PDFs small (< 100KB per fixture, max 5 per cluster)
- Naming: `cl{NNNN}_{source}_{hash_prefix}.pdf`
- Store in `tests/regression/fixtures/` per crate

### 6. Verify

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --all --check
```

### 7. Compare runs

After merging, run the corpus again and compare:

```bash
xfa-test-runner compare --run-a run-before --run-b run-after --db results.sqlite
```

The compare command will:
- Show per-test pass rate deltas
- List resolved and new clusters
- Exit non-zero if a regression is detected (more new clusters than resolved)

### Principles

- One fix per PR — don't mix unrelated changes
- Every fix must have regression tests
- A fix that introduces new failures is a regression
- False positives (we're stricter than the spec) are acceptable
- False negatives (we miss spec violations) are bugs
