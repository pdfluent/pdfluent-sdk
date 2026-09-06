# PDFluent Commercial Licence — Order Form

This is the template of the document `LICENSE-COMMERCIAL` §10 calls "an order
form signed by both parties". The licence text says what the rights are; this
form says who holds them, over which versions, for how long, and for how much.
Neither is the agreement on its own.

**This is not the ordinary way to buy a licence.** Since 6 September 2026 the
four commercial licences are self-service: they are bought at
https://pdfluent.com/sdk/pricing, and what a buyer receives is the order
confirmation and the licence document named in `LICENSE-COMMERCIAL` §5. That is
the agreement, and no form is signed. This template exists for the licensee
whose procurement requires a signed order form, and it changes nothing about the
rights — the prices and scopes below are the ones on the page.

**This file is a template.** It is filled in per licensee, signed, and kept with
the invoice. Nothing in this repository records an executed order form, and
nothing should: an executed form carries a customer's name.

Licensor: Innovation Trigger B.V., trading as PDFluent, Netherlands.
Licence: PDFluent Commercial Licence, version 3.0 — 6 September 2026 (`LICENSE-COMMERCIAL`).

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
| Versions licensed | every version released during the term in section 5 |
| Delivery | source and published packages, as released; nothing is withheld from a licensee |

The term bounds which releases the licence covers, not how long the software may
be run: nothing in it checks a date or reads this form. What ends with the term
is the right to convey or operate later releases under this licence rather than
under the AGPL, and anything shipped during the term is unaffected
(`LICENSE-COMMERCIAL` §8).

A licence granted under version 2.0 or earlier of the licence was perpetual for
the versions it named and stays so. Do not rewrite one onto this form.

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

Tick the licence bought. The prices are the published ones
(`LICENSE-COMMERCIAL` §2a) and this form does not vary them.

| | Licence | Yearly fee, excl. VAT | Bought |
|---|---|---|---|
| | Commercial — one organisation, unlimited developers | € 999 | ☐ |
| | OEM Startup — one product, organisation revenue below € 1,000,000 | € 2,499 | ☐ |
| | OEM — one product | € 9,999 | ☐ |
| | Priority support — add-on, not a licence | € 4,999 | ☐ |

| | |
|---|---|
| Named product (OEM only) | _________________________________________ |
| Revenue declaration (OEM Startup only) | the licensee declares its organisation's revenue is below € 1,000,000 ☐ |
| Invoice reference | _________________________________________ |
| Payment | annually in advance |
| Payment term | 30 days from the invoice date, unless stated otherwise here: _______ |

`LICENSE-COMMERCIAL` §2a decides which of the first three applies: if the
licensee's customers pay it for PDF functionality that PDFluent provides — a
conversion API, a PDF/A service, an OCR service, hosted PDF tooling — that is
OEM, not Commercial.

## 5. Term

The licence runs for one year and renews yearly until cancelled
(`LICENSE-COMMERCIAL` §8).

| | |
|---|---|
| Term begins | _______ |
| Renewal | yearly, unless cancelled before the end of the current term |
| Support | ☐ none beyond `LICENSE-COMMERCIAL` §6 ☐ priority support, per section 4 |
| Response times | ☐ none agreed ☐ within one working day, where priority support is bought |

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

*Template version 2.0, 6 September 2026. Drawn against `LICENSE-COMMERCIAL`
version 3.0 of 6 September 2026. `scripts/ci/the_order_form_agrees_with_the_licence.py`
fails when this form and that licence stop agreeing.*
