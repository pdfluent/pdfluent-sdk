> **Superseded, 01-09-2026.** The state this page describes is resolved. `LICENSE`
> now holds the dual notice rather than the v1.0 commercial licence (the v1.0 text
> is archived at `docs/licensing/archive/LICENSE-COMMERCIAL-v1.0-2026-05-02.txt`),
> the crates declare `license = "AGPL-3.0-only OR LicenseRef-PDFluent-Commercial"`
> instead of `license-file`, and
> `LICENSE-ADDITIONAL-TERMS` no longer exists — the owner dropped the §7(b) term on
> 01-09-2026. Kept because it records why two disagreeing files were tolerated for
> a week, which is the part that would otherwise look like an accident.

# `LICENSE` and `LICENSE-COMMERCIAL` both exist, and disagree. On purpose, briefly.

As of 31-08-2026 the repository root holds:

| file | what it is |
|---|---|
| `LICENSE` | the commercial licence **v1.0, 2 May 2026** — proprietary-only, references a licence key file and published tiers |
| `LICENSE-COMMERCIAL` | the commercial licence **v2.0, 31 Aug 2026** — the current model |
| `LICENSE-AGPL` | the FSF text, verbatim |
| `LICENSE-ADDITIONAL-TERMS` | the §7(b) attribution term |

`LICENSE` is superseded and has not been deleted. That is deliberate: 29 crates
declare `license-file = "LICENSE"`, and crates.io renders that file for every
published version. Replacing it is the manifest flip, which is **LC9 (#221)**,
not this issue.

Doing it here would change what published crates declare as a side effect of
writing a text — the exact move #220 says it is not making.

## What LC9 has to do

1. Point the manifests at the pair, or make `LICENSE` a short notice that names
   both halves and refers to the other three files.
2. Delete or archive the v1.0 text in the same commit, so there is never a
   version of the tree where two commercial licences both look current.
3. Re-run `scripts/ci/license_boundary.py`, which asserts that every crate on
   our side declares `license-file` — a check that will need updating in the
   same breath if the field changes.

## Until then

Anyone reading the repository sees v1.0 at the canonical path. If a buyer asks
which applies, the answer is `LICENSE-COMMERCIAL` v2.0 — and the fact that this
needs saying out loud is the reason LC9 should not sit for long.
