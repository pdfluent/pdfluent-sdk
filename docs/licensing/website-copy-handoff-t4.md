# Website copy for #226 and #227 — prepared by t3, to be applied by t4

t3 owns licence terms; the website is t4's repository. This file is the handoff:
the decisions are made, the replacement strings are written, and t4 applies them.
Nothing here has been edited on the site by t3.

Decisions taken by the owner, 31-08-2026. Every "now" string below was read off
the live page the same day with:

```
curl -sL https://pdfluent.com/sdk/pricing/ | sed 's/<[^>]*>/\n/g'
```

## #226 — the four Buy-now buttons

They read "Buy now — €X" and link to `/contact/`. `/buy`, `/checkout` and
`/sdk/buy` all return 404, so the label lied on every click.

| line | now | becomes |
|---|---|---|
| 340 | `Buy now — €699` | `Request a licence — €699` |
| 422 | `Buy now — €1,099` | `Request a licence — €1,099` |
| 500 | `Buy now — €2,199` | `Request a licence — €2,199` |
| 584 | `Buy now — €4,399` | `Request a licence — €4,399` |

The link target stays `/contact/`. The price stays visible — this is not a
"contact us for pricing" page, it is a published price with a manual last mile,
and saying so is the honest version.

There is one more of these outside the tier cards, at line ~1526 in the
Enterprise block: `Buy — €1,099`. Same treatment.

**A measurement warning for whoever re-checks this.** `/api/*` returns **405 for
every path**, including paths that do not exist. An `/api/sdk-checkout` that
answers 405 is not evidence that a checkout was built.

## #227 — air-gapped is included, not an add-on

The €1,499 add-on is withdrawn. Air-gapped deployment is included in every paid
licence. Five places say otherwise:

**1. The add-on card (lines 1296–1298).** Delete the whole card:

```
Air-gapped Deployment
+€1,499
Deploy in environments with no outbound …
```

Leave the OEM Redistribution card at +€1,999 exactly as it is. That one is not
part of this decision.

**2. The price footnote (line 591).**

> now: All prices in EUR, excl. VAT. Perpetual licence — pay once, own that
> version forever. **OEM redistribution and air-gapped deployment available as
> add-ons.** Enterprise from €12,000.

> becomes: All prices in EUR, excl. VAT. Perpetual licence — pay once, own that
> version forever. **Air-gapped deployment is included in every paid licence.
> OEM redistribution is available as an add-on.** Enterprise from €12,000.

**3. The comparison table (line ~834).** The `Air-gapped deployment` row reads
`+add-on` across the paid tiers and `Included` for Enterprise. Every paid cell
becomes `✓`. Community, when it exists, is also `✓` — see below.

**4. The FAQ entry (line ~1586).**

> now: **What is the Air-gapped add-on?** — Air-gapped (+€1,499) covers
> deployments where the production environment has no outbound internet access.
> Without this add-on, the SDK is licensed for environments where outbound
> connections are theoretically possible — even if not actively used.

> becomes: **Can I run the SDK air-gapped?** — Yes, on every paid licence, at no
> extra cost. The SDK makes no outbound connection at any point, so an
> environment without internet access needs nothing special from us and is not
> priced differently.

**5. The Enterprise block (line ~1544).**

> now: … **OEM and air-gapped rights are available as self-serve add-ons for all
> tiers** — see the licence terms.

> becomes: … **OEM rights are available as a self-serve add-on for all tiers**;
> air-gapped deployment is included everywhere — see the licence terms.

### Why the old wording could not survive anyway

The withdrawn add-on charged for a *restriction we cannot deliver*. Under
AGPLv3 (#212) the customer builds from source and runs it wherever they like;
there is no mechanism by which an unpaid air-gapped deployment differs from a
paid one. Charging €1,499 for the absence of a limit that does not exist is the
kind of line an enterprise buyer's technical reviewer finds, and then stops
trusting the rest of the page.

## Not decided here, but it lands on the same page

Two claims on `/sdk/pricing/` describe technical enforcement that the 25-08
decision removes:

- line ~1526: *"On initialisation the SDK verifies the Ed25519 signature against
  the embedded public key."*
- FAQ *"How does the Ed25519 licence file work?"* and *"Can I use the SDK in
  production during the 30-day evaluation?"* — the latter promises *"After 30
  days the SDK returns a clear error on initialisation."*

These are true today, because master still ships the enforcement. They become
false the moment #199 lands. They are **not** part of #226 or #227 and t4 should
not change them yet — but they must change in the same release as the removal,
or the page starts describing a product that no longer exists.

Same for the missing Community/AGPL route: #227 asks for it as a full column,
and it cannot be written until the AGPL text exists (#220). Deliberately left
out of this handoff rather than written against a licence that does not yet
exist.

## Claims

Every euro figure and competitor comparison on the page needs an ID in the
claims register before it changes, per #227's acceptance criteria. The register
lives in the website repository, so that check is t4's too — flagged here so it
is not discovered afterwards.
