#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""One offer, in every document that states it (#227, built in #349).

WHY THIS IS A GATE AND NOT A PROOFREAD

The commercial offer is written down in four places in this repository --
`LICENSE-COMMERCIAL` §2a, `docs/licensing.md`, `docs/licensing/order-form.md`
and `NOTICE` -- and in a fifth outside it, `src/config/plans.ts` in the website
repository, which is what the checkout, the pricing page and the licence
document all read.

That is five copies of four numbers. The previous offer was in nineteen, and
when it changed on 6 September 2026 not one of them moved: the site was still
selling a EUR 699 perpetual licence with a 30-day evaluation and a licence key
weeks after all three had been withdrawn. Nothing failed, because nothing was
comparing them.

So this file holds the offer once, in `docs/licensing/offer.toml`, and reads
the four documents against it. It cannot reach the website repository; what it
can do is make the copy on this side single, so that the two repositories have
one seam instead of five.

WHAT IT CHECKS

  every price     each button's yearly price appears in `LICENSE-COMMERCIAL`
                  §2a and in `docs/licensing.md`, written the way the offer
                  file writes it. A price in one document and not the other is
                  the failure this exists for.
  no retired price
                  no amount from the retired model (699, 1,099, 2,199, 4,399,
                  1,999, 1,499) appears in any of them. Those were real prices;
                  a reader has no way to tell a stale one from a current one.
  the OEM rule    the sentence that decides Commercial from OEM appears
                  verbatim in `LICENSE-COMMERCIAL` and in `docs/licensing.md`.
                  It is the sentence a licensee is most likely to be judged on,
                  and a paraphrase of it is a different rule.
  the term        no document still calls a commercial licence perpetual,
                  except where it says what buyers before 6 September 2026
                  keep. Both statements are true; only one is about what is
                  being sold today.
  no key          no document offers a licence key, an activation or an
                  evaluation period. `no_licence_key_in_a_binding.py` holds the
                  binary to this; nothing held the prose to it.

FLOORS, because a scan that reads nothing approves everything

The offer file must parse to exactly the buttons it declares, and every
document must be non-empty and mention the currency at least once.

Usage:
    python3 scripts/ci/one_offer_in_every_document.py

Exit codes:
    0  the documents state one offer
    1  they do not
"""
from __future__ import annotations

import pathlib
import re
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]
OFFER = "docs/licensing/offer.toml"
LICENCE = "LICENSE-COMMERCIAL"
DOCS = "docs/licensing.md"
FORM = "docs/licensing/order-form.md"
NOTICE = "NOTICE"

# Amounts the repository sold before 06-09-2026. Not "wrong numbers": prices
# that were true, which is exactly why a reader cannot spot a stale one.
RETIRED = ("699", "1,099", "2,199", "4,399", "1,999", "1,499", "12,000")

# Wording of the retired model. The second element is why it may not be there.
RETIRED_WORDS = (
    (r"\blicence key\b", "no code reads a key (#226); only a denial may mention one"),
    (r"\b30[- ]day (evaluation|trial)\b", "there is no evaluation period -- the AGPL build is complete"),
    (r"\bpay once\b", "nothing is sold as a one-off payment"),
    (r"\bOEM Redistribution\b", "OEM is a licence of its own, not an add-on to another one"),
    (r"\bAir-gapped Deployment\b", "air-gapped use is included and was never withholdable"),
)

# Every one of the above is allowed to appear in a sentence that denies it, and
# most of these documents exist partly to deny them. What is forbidden is the
# offer, not the word.
DENIAL = re.compile(
    r"\b(no|not|never|nothing|none|without|removed|withdrawn|retired|neither|"
    r"does not|do not|is not|are not|stopped|ceased)\b",
    re.I,
)

# A sentence saying a *past* licence was perpetual is the one true use of the
# word left, and every document needs it. These are the forms it may take.
PERPETUAL_OK = (
    "were perpetual",
    "was perpetual",
    "stay perpetual",
    "stays so",
    "remain so",
    "sold as a perpetual licence",
)

problems: list[str] = []


def complain(line: str) -> None:
    problems.append(line)


def read(rel: str) -> str | None:
    path = REPO / rel
    if not path.is_file():
        complain(f"{rel} is missing; the offer is stated in it.")
        return None
    text = path.read_text(encoding="utf-8")
    if not text.strip():
        complain(f"{rel} is empty. An empty document agrees with every offer.")
        return None
    return text


def flat(text: str) -> str:
    """One space between words, and no Markdown quote markers.

    A sentence inside a blockquote is the same sentence. Comparing the rendered
    prose to the offer file has to ignore the `> ` a Markdown quote adds, or
    quoting the OEM rule -- which is exactly how a document should present it --
    would read as not carrying it.
    """
    unquoted = re.sub(r"^\s*>\s?", "", text, flags=re.M)
    return " ".join(unquoted.split())


def sentence_around(text: str, index: int) -> str:
    """The sentence a match sits in, so a denial can be told from an offer."""
    start = max(text.rfind(".", 0, index), text.rfind("\n\n", 0, index)) + 1
    end = text.find(".", index)
    return flat(text[start : end + 1 if end > 0 else len(text)])


def parse_offer(text: str) -> tuple[list[dict[str, str]], str]:
    """The buttons and the OEM rule, out of the TOML, without a TOML library.

    The file is written for this parser and the shape is checked below: a
    parser that silently returns nothing is the failure mode this whole guard
    exists to prevent, so it is not allowed to be quiet about it.
    """
    buttons = []
    for block in re.findall(r"\[\[button\]\](.*?)(?=\n\[\[|\n\[|\Z)", text, re.S):
        entry = dict(re.findall(r"^\s*(\w+)\s*=\s*\"([^\"]*)\"", block, re.M))
        if entry.get("id"):
            buttons.append(entry)
    rule = re.search(r'^oem_rule\s*=\s*"""(.*?)"""', text, re.S | re.M)
    return buttons, flat(rule.group(1)) if rule else ""


def main() -> int:
    offer_text = read(OFFER)
    licence = read(LICENCE)
    docs = read(DOCS)
    form = read(FORM)
    notice = read(NOTICE)
    if not all((offer_text, licence, docs, form, notice)):
        return finish()

    buttons, rule = parse_offer(offer_text)
    declared = re.search(r"^buttons\s*=\s*(\d+)", offer_text, re.M)
    expected = int(declared.group(1)) if declared else -1

    if expected < 0:
        complain(f"{OFFER} declares no `buttons = <n>` count, so nothing says how many the parser should find.")
        return finish()
    if len(buttons) != expected:
        complain(
            f"{OFFER} declares {expected} button(s) and parsed to {len(buttons)}. "
            "A parser that finds nothing agrees with every document, which is "
            "the one verdict this guard must never print."
        )
        return finish()
    if not rule:
        complain(f"{OFFER} carries no `oem_rule`, and it is the sentence the rest is checked against.")
        return finish()

    stated = {LICENCE: licence, DOCS: docs}

    # --- every price, in the documents that state prices
    for button in buttons:
        price = button.get("price_eur_written", "")
        if not price:
            complain(f"{OFFER}: button {button.get('id')} has no `price_eur_written`.")
            continue
        # Bounded, not as a substring: "999" is inside "9,999", so a plain
        # `in` test finds the Commercial price in a document that only states
        # the OEM one. That is exactly the drift this check exists to catch,
        # and it passed until the self-test planted it.
        wanted = re.compile(rf"(?<![\d,]){re.escape(price)}(?![\d,])")
        for rel, text in stated.items():
            if not wanted.search(flat(text)):
                complain(
                    f"{rel} does not state {price} for {button.get('name')}. "
                    f"{OFFER} says it is the price, and a document that omits one "
                    "of four prices is read as though that licence is not sold."
                )

    # --- no retired price anywhere
    for rel, text in {**stated, FORM: form, NOTICE: notice}.items():
        flattened = flat(text)
        for amount in RETIRED:
            if any(amount == b.get("price_eur_written", "").lstrip("€ ") for b in buttons):
                continue
            for form_of in (f"EUR {amount}", f"€ {amount}", f"€{amount}"):
                if re.search(rf"{re.escape(form_of)}(?![\d,])", flattened):
                    complain(
                        f"{rel} still states {form_of}, a price from the model retired on "
                        "6 September 2026. It was true once, which is why nobody spots it."
                    )

    # --- the rule, verbatim, where the rights are stated
    for rel, text in stated.items():
        if rule not in flat(text):
            complain(
                f"{rel} does not carry the OEM rule verbatim. It is the sentence that "
                "decides which licence a buyer needed, and a paraphrase of it is a "
                f"different rule. The words are in {OFFER}."
            )

    # --- the term, and the one honest use of "perpetual"
    for rel, text in {**stated, FORM: form}.items():
        for match in re.finditer(r"[^.\n]*\bperpetual\b[^.\n]*", text, re.I):
            sentence = flat(match.group(0))
            if any(ok in sentence for ok in PERPETUAL_OK):
                continue
            complain(
                f"{rel} calls a licence perpetual outside a statement about what "
                f"earlier buyers keep: {sentence[:110]!r}. Licences sold today are yearly."
            )

    # --- the vocabulary of the retired model
    for rel, text in {**stated, FORM: form, NOTICE: notice}.items():
        for pattern, why in RETIRED_WORDS:
            for hit in re.finditer(pattern, text, re.I):
                sentence = sentence_around(text, hit.start())
                if DENIAL.search(sentence):
                    continue
                complain(f"{rel} offers {hit.group(0)!r} -- {why}. In: {sentence[:110]!r}")

    # --- floors
    for rel, text in {**stated, FORM: form, NOTICE: notice}.items():
        if "EUR" not in text and "€" not in text:
            complain(f"{rel} names no amount at all, so every price check above passed over nothing.")

    return finish()


def finish() -> int:
    if problems:
        print(f"[one-offer] {len(problems)} problem(s):", file=sys.stderr)
        for p in problems:
            print(f"  - {p}", file=sys.stderr)
        return 1
    print(
        "[one-offer] LICENSE-COMMERCIAL, docs/licensing.md, the order form and NOTICE "
        "state one offer, the one in docs/licensing/offer.toml"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
