# Why a DCO, and what it does not do

Decision, 31-08-2026. Contributions are certified with the Developer Certificate
of Origin, signed off per commit with `git commit -s`. There is no CLA yet, and
that is deliberate rather than an oversight.

## What each instrument actually does

| | DCO | CLA |
|---|---|---|
| certifies | the contributor had the right to submit it | that, **plus** a grant to us |
| gives us the right to relicense their code commercially | **no** | yes |
| cost | nothing; one line per commit | a lawyer, and a signing process |
| precedent | the Linux kernel | varied |

The DCO answers "was this yours to give". A CLA additionally answers "may
PDFluent sell it under different terms". Those are different questions and only
the second one needs a lawyer, because it is an assignment of rights.

## Why the second question can wait, but not forever

The whole dual-licence construction rests on one invariant: **a proprietary
editor may run on our own AGPL engine as long as everything we do not own is
permissive.** Merge one outside contribution into the AGPL SDK without the right
to relicense it, and the SDK contains AGPL code that is not ours. From that
moment the commercial half is not offerable, and the editor is not shippable —
not to the stores, not as a download.

That breaks **at the merge, not at the build.** Nothing turns red. The first time
anyone notices is when a buyer's lawyer asks, and by then it is months deep in a
history other work is built on. Removing it means finding the author, and an
author who signed nothing has no reason to reply.

So the risk is real and the exposure today is zero, for one reason that is
measurable: there are **no outside contributors**. Measured 31-08-2026 over ten
identities across five repositories — every one of them the owner, automation he
controls, or a tool that holds no rights — and the four public repositories had
zero forks and zero pull requests.

That measurement was made with a register and a guard,
`scripts/ci/contributors.toml` and `scripts/ci/contribution_rights_are_covered.py`,
written on 31-08 in commit `4e63143f`. **Neither is on master** (checked
05-09-2026): the commit sits on `chore/test-reachability-gate` and on two other
branches, and its guard imports two modules that never landed either. So the ten
identities are a measurement that was taken and not a check that runs. Landing it
is the work `docs/decisions/contribution-rights-coverage.md` describes, and it is
not done.

A CLA costs money we do not have, for a population of zero. A DCO costs nothing
and closes the provenance half now.

## The condition that ends this

**The first outside pull request.** Not a date, not a funding round — that event.
Until it arrives the DCO is enough. When it arrives, either the CLA exists, or
that contribution cannot be merged into an AGPL crate.

This is written down here so that the decision is a decision and not a habit. It
is not "we chose a DCO"; it is "we deferred the CLA, for this reason, until this
event".

## What is deferred, exactly

- The CLA text, legally checked.
- A signing and recording process.
- The gate that refuses a commit whose author is not covered by a signed CLA —
  as opposed to a gate that checks that every identity is *recorded*.
- The register that gate would read, and the guard over it: written, measured,
  and still not on master (see above).

What master does enforce is the sign-off itself, in both places a commit can
arrive: `every_commit_since_the_cutoff_is_signed.py` before a push, and
`licence_signoff.py` on the pull request, the second matching each sign-off
against the commit's own author. Since 05-09-2026 both also refuse a
`Co-authored-by:` address that appears in no sign-off — the case #223 names as
"author or co-author", and the one that was invisible, because a co-author is in
that trailer and in no field git exposes.

It is worth being honest that the identity half **cannot fail for the reason it
exists** while there is one contributor and he is the owner. That is an argument
for having it run before the day it matters, not for calling the day it matters
covered.

## How to contribute under the DCO

Every commit carries a sign-off line matching the author:

```
Signed-off-by: Your Name <your.email@example.com>
```

`git commit -s` adds it. By adding it you certify the DCO in
`docs/contribution/DCO.txt`, which is the Linux Foundation's text, verbatim.
`CONTRIBUTING.md` says the same thing where a contributor will actually look.

A commit written by two people carries a `Co-authored-by:` line for the second
one. That trailer certifies nothing on their behalf, so they add their own
`Signed-off-by:` line beside the author's; both gates refuse a co-author who
signed off on nothing.

Sign-off is about provenance. It is not an assignment, and it does not give
PDFluent the right to relicense your contribution commercially. If a
contribution ever needs to go into a crate on our side of the licence boundary
(`docs/licensing/boundary.toml`), that will need the CLA that does not exist
yet — and until it does, such a contribution cannot be merged.
