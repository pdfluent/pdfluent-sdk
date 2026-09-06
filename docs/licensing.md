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

Four prices, all yearly, all excluding VAT, all self-service at
<https://pdfluent.com/sdk/pricing>. Decided on 6 September 2026 (#227) and built
in #349.

| Licence | Price | Covers |
|---|---|---|
| Commercial | € 999 / year | One organisation, unlimited developers. Closed-source use, and SaaS where PDF functionality is a part of your own product. |
| OEM Startup | € 2,499 / year | One named product, for an organisation with revenue below € 1,000,000 (declared at purchase). Redistribution, and offering PDF functionality itself as a service. |
| OEM | € 9,999 / year | One named product. The same rights, every other organisation. |
| Priority support | € 4,999 / year | Not a licence. A reply within one working day, on any paid licence. |

The rule that decides Commercial from OEM:

> If your customers pay you for PDF functionality that PDFluent provides — a
> conversion API, a PDF/A service, an OCR service, hosted PDF tooling — that is
> OEM, not Commercial.

Building PDF handling into your own product is Commercial. Selling PDF handling
to your customers is OEM.

What a buyer receives is the order confirmation and a licence document naming
the licensee, the licence, the product where one applies, and the term. Those
two together are the licence — `LICENSE-COMMERCIAL` §5 and §10 say so. There is
nothing to install and nothing to activate.

The subscription renews yearly and is cancelled in the billing portal, reachable
from the pricing page. Cancelling breaks nothing: no release stops working and
nothing already shipped is affected. What ends is the right to convey or operate
later releases under this licence rather than under the AGPL
(`LICENSE-COMMERCIAL` §8).

A licensee whose procurement requires a signed order form can have one — the
template is [`docs/licensing/order-form.md`](licensing/order-form.md) — but it
is not the ordinary route and it changes none of the rights. Write to
**sales@pdfluent.com**.

Licences bought before 6 September 2026 were perpetual for the versions they
named and stay perpetual. Nothing in the current model withdraws or replaces
them, and there is nothing to migrate.

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
