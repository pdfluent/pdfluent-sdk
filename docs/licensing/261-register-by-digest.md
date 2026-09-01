# #261 — the register records people by digest

Decision, 31-08-2026: `scripts/ci/contributors.toml` carries only the noreply
alias in plain text. The owner's personal and work addresses are recorded as
`sha256:<hex>`.

## Why this is not covered by the 27-08 decision

That decision said: fix going forward, leave the history, treat
the owner's personal address as already public. It was about **commit metadata that
already exists and cannot be retracted.** `contributors.toml` was written after
it, and this tree becomes public under #222 — so writing the address there is a
*new* publication of it, as fresh harvestable plain text, arriving through the
one file whose entire purpose is being careful about rights. "Already out there,
so no point rewriting" does not extend to "so we may as well write it again."

## The mechanism

`registersleutel()` in `contribution_rights_are_covered.py` translates an address
found in the history to the key it is registered under: itself, or its digest.
Only `oordeel()` compares, and both sides now speak the same alphabet, so the
gate is **exactly as strict as before**. A digest entry covers the one address
that hashes to it and nothing else.

This is not secrecy, and the file says so in its own header. A digest of a
guessable address is guessable, and the address is in 3555 commit headers anyone
may read. What it stops is the address being *scraped from a published file*,
which is the thing that actually happens automatically and at scale.

Organisation and automation addresses stay in plain text: `noreply@github.com`
and `gate-ci@pdfluent.com` identify a machine, and hiding them costs legibility
for nothing.

## A second leak, found by mutating

Removing the digest entry made the gate fail — correctly — with a message naming
the address it could not place. **That message goes to a CI log, and on a public
repository the log is public.** So the gate would have republished the address
in full, on the day something went wrong, which is the worst day for it.

Failures now print the address masked to its first character and domain,
with the digest beside it. The operator
can still act — the digest is what the register wants anyway — and the plaintext
is never printed.

Found by breaking the gate rather than by reading it. It is not visible in the
passing path at all.

## Proof

Run on the real `chore/test-reachability-gate` tree, with the branch's own
dependency modules present, patch applied:

```
[rights] 10 identit(y/ies) over 5 population(s): 3868 local commit(s) and 69 published
  ... all OK, exit=0
grep -c 'jasper@' scripts/ci/contributors.toml  →  0
```

Three mutations, each restored from a copy:

| mutation | result |
|---|---|
| digest entry for the personal address deleted | exit 1 — "writes in … and is named nowhere" |
| one hex character wrong in the digest | exit 1 — both directions, unlisted **and** covers-nothing |
| a digest for an address nobody ever used | exit 1 — "appears in no history" |
| restored | exit 0 |

The second is the one that matters: a typo in a digest is invisible to a reader
and would silently uncover 3555 commits. It fails loudly, twice.
