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
measurable: there are **no outside contributors**. Verified 31-08-2026 —
`scripts/ci/contributors.toml` records ten identities across five repositories,
every one of them the owner, automation he controls, or a tool that holds no
rights; and the four public repositories have zero forks and zero pull requests.

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
  as opposed to today's gate, which checks that every identity is *recorded*.

`scripts/ci/contribution_rights_are_covered.py` already fails on an identity it
has never been told about, which is the mechanism the CLA gate would reuse. It
is worth being honest that today that gate **cannot fail for the reason it
exists**: there is one contributor and he is in the register. It is a gate that
will matter later, running now so that it is not new on the day it matters.

## How to contribute under the DCO

Every commit carries a sign-off line matching the author:

```
Signed-off-by: Your Name <your.email@example.com>
```

`git commit -s` adds it. By adding it you certify the DCO in
`docs/contribution/DCO.txt`, which is the Linux Foundation's text, verbatim.

Sign-off is about provenance. It is not an assignment, and it does not give
PDFluent the right to relicense your contribution commercially. If a
contribution ever needs to go into a crate on our side of the licence boundary
(`docs/licensing/boundary.toml`), that will need the CLA that does not exist
yet — and until it does, such a contribution cannot be merged.
