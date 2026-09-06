# Licensing

PDFluent has **no licence key**. There is nothing to activate, nothing to
renew, and no call that unlocks a feature. Every capability in this repository
is present in every build, and the output of a build with no licence is
byte-identical to the output of a build with one.

That is not a gap. It is the model, decided on 25 August 2026 (#199) and
carried out in #226: published source means any technical check can be removed
by whoever holds the code, so a check would only ever inconvenience the people
who intended to pay. What a commercial licensee buys is an agreement, not an
unlock.

## The two licences

PDFluent is published under the **GNU AGPLv3** (`LICENSE-AGPL`). That is the
default and it is the complete product.

The AGPL asks something back: if you convey the software, or let users interact
with a modified version over a network, you must offer those users the
corresponding source under the same licence. Where that is impossible — a
closed surrounding product, a customer contract, a legal department that will
not accept copyleft in a shipped binary — the **PDFluent Commercial Licence**
(`LICENSE-COMMERCIAL`) is the alternative. You are not buying features. You are
buying the right not to publish your own source.

## How to buy

The commercial route is a signed order form, and nothing else:

1. Write to **sales@pdfluent.com** with the legal entity, the versions you want
   covered, and which of the four `LICENSE-COMMERCIAL` §2 grants you need.
2. You receive the order form — the template is
   [`docs/licensing/order-form.md`](licensing/order-form.md) — filled in for
   your case.
3. Both parties sign it. `LICENSE-COMMERCIAL` §10 says the agreement is that
   licence text plus this form, and that **the form prevails** where the two
   conflict.
4. You receive a countersigned agreement, an invoice, and an entry in
   PDFluent's licence register. Those three are the deliverables named in
   `LICENSE-COMMERCIAL` §5, and they are all of them.

There is no self-service checkout on pdfluent.com today. A page that offers one
would be describing something that does not exist, which is the failure #226
was opened about: buttons reading "Buy now" that led to a contact form.

## What a licensee receives, and what changes in the software

Nothing changes in the software. Specifically:

| | |
|---|---|
| Features | all of them, before and after; nothing is withheld from an unlicensed build |
| Output | identical; no watermark, no `/Producer` marking, no trial notice |
| Expiry | none. A version you licensed keeps working, because nothing in it can stop |
| Network | nothing contacts a server, so air-gapped deployment needs no permission |
| Attribution | `LICENSE-COMMERCIAL` §4 — you may remove the "PDFluent" `/Producer` string in your build. So may an AGPL licensee |

## What the environment does not do

`PDFLUENT_LICENSE_KEY` was read by every binding until #226. It is read by none
of them now: setting it changes nothing, and no code path in this repository
consults it. `scripts/ci/no_licence_key_in_a_binding.py` refuses a commit that
brings one back.

## Not covered by either licence

Several components are forks of third-party open-source projects and stay under
their own permissive licences. They are not PDFluent's to relicense and need no
licence from PDFluent. `NOTICE` records which they are;
[`docs/licensing/boundary.toml`](licensing/boundary.toml) is the
machine-readable version.
