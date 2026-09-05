# PDFluent Commercial Licence — Order Form

This is the template of the document `LICENSE-COMMERCIAL` §10 calls "an order
form signed by both parties". The licence text says what the rights are; this
form says who holds them, over which versions, for how long, and for how much.
Neither is the agreement on its own.

**This file is a template.** It is filled in per licensee, signed, and kept with
the invoice and the register entry named in `LICENSE-COMMERCIAL` §5. Nothing in
this repository records an executed order form, and nothing should: an executed
form carries a customer's name.

Licensor: Innovation Trigger B.V., trading as PDFluent, Netherlands.
Licence: PDFluent Commercial Licence, version 2.0 — 31 August 2026 (`LICENSE-COMMERCIAL`).

---

## 1. Parties

| | |
|---|---|
| Licensor | Innovation Trigger B.V., trading as PDFluent, Netherlands. Chamber of Commerce no. ___________ |
| Licensee | _________________________________________ (full legal name) |
| Licensee's registered seat | _________________________________________ |
| Licensee's registration no. | _________________________________________ |
| Notices to licensee | _________________________________________ (address and e-mail) |
| Notices to licensor | sales@pdfluent.com |

The licensee is the legal entity named above. Affiliates are covered only where
section 3 says so, and an affiliate that leaves the licensee's group leaves the
licence with it on the date it leaves.

## 2. Licensed software and versions

| | |
|---|---|
| Software | PDFluent, the components on PDFluent's side of `docs/licensing/boundary.toml` |
| Versions licensed | _________________________________________ |
| Delivery | source and published packages, as released; nothing is withheld from a licensee |

Two ways to fill in "versions licensed", and the difference matters more than
the fee does:

- **A named version and everything released before it.** The rights are perpetual
  over exactly that set. A later release is a new order form.
- **Every version released during the support term in section 5.** The rights
  over each version stay perpetual once released — the term bounds which
  versions arrive, never how long the ones already delivered may be used.

Leave neither blank. A form that does not say which versions it covers is a form
that will be read two ways on the day it matters.

## 3. Licensed scope

Tick what is granted. Each row is one of the rights `LICENSE-COMMERCIAL` §2
grants; a row left unticked is a right the licensee does not buy and does not
receive, and falls back to the AGPL.

| | Grant (`LICENSE-COMMERCIAL` §2) | Granted |
|---|---|---|
| a | Distribution of the software as part of a compiled product, without supplying source to the recipients | ☐ |
| b | Operation as a network service, without the AGPL section 13 obligation | ☐ |
| c | Internal use of any kind | ☐ |
| d | Deployment in environments without network access | ☐ |

| | |
|---|---|
| Affiliates included | ☐ none ☐ majority-owned affiliates ☐ named: _____________ |
| Named products or services | _________________________________________ |
| Sublicensing to the licensee's own customers | granted for the compiled product only, and only where row a is ticked |

Row d needs no permission and costs nothing extra: nothing in the software
contacts a server. It is on the form because buyers ask, not because it is a
restriction.

## 4. Fee

| | |
|---|---|
| Fee | € ___________ , excluding VAT |
| Payment | ☐ one-off ☐ annually in advance, per section 5 |
| Invoice reference | _________________________________________ |
| Payment term | 30 days from the invoice date, unless stated otherwise here: _______ |

## 5. Term

The licence granted in section 3 is **perpetual** for the versions in section 2
and does not end with this section. What this section bounds is the support and
the release stream, not the right to use what was delivered.

| | |
|---|---|
| Support and updates | ☐ none ☐ from _______ to _______ ☐ annual, renewing unless cancelled _______ before renewal |
| Response times | ☐ none agreed ☐ as set out in a separate services agreement dated _______ |

Where support is agreed here, this section prevails over `LICENSE-COMMERCIAL` §6,
which says the licence promises none.

## 6. Precedence

This order form and `LICENSE-COMMERCIAL` together are the agreement. Where they
conflict, the order form prevails. `LICENSE-COMMERCIAL` §10 says the same thing
from the other side; both are written down so that neither document has to be
read to know which one wins.

Terms on a purchase order, in a portal, or in general conditions attached to a
payment do not vary this agreement unless they are written into this form and
signed with it.

## 7. What this form does not change

- **The forked components.** Several components are forks of third-party open
  source under their own permissive licences, recorded in `NOTICE` and in
  `docs/licensing/boundary.toml`. They are not PDFluent's to license and need no
  licence from PDFluent. This form covers PDFluent's side of that boundary only.
- **Warranty and liability.** `LICENSE-COMMERCIAL` §7 applies as written. A
  higher liability cap is a negotiated change and belongs in section 8 below,
  not in an e-mail.
- **Law and forum.** `LICENSE-COMMERCIAL` §9 applies: Netherlands law, the court
  of the licensor's registered seat.

## 8. Agreed variations

Anything the parties agreed that is not in the sections above:

_______________________________________________________________________

_______________________________________________________________________

If this section is empty, nothing was varied. Do not leave a variation in the
correspondence and out of the form.

## 9. Signatures

| | Licensor | Licensee |
|---|---|---|
| Name | | |
| Function | | |
| Date | | |
| Signature | | |

---

*Template version 1.0, 5 September 2026. Drawn against `LICENSE-COMMERCIAL`
version 2.0 of 31 August 2026. `scripts/ci/the_order_form_agrees_with_the_licence.py`
fails when this form and that licence stop agreeing.*
