# Where LICENSE-AGPL came from, and how we know it is unaltered

## Source

`https://www.gnu.org/licenses/agpl-3.0.txt`, fetched 31-08-2026.

```
HTTP 200, 34,523 bytes, 661 lines
sha256 0d96a4ff68ad6d4b6f1f30f713b18d5184912ba8dd389f86aa7710db079abcb0
```

## Cross-checked against a second, independent source

One fetch plus a heading count is not evidence that a licence is verbatim — a
changed sentence inside section 7 passes both. So the text was compared word for
word against SPDX's stored `AGPL-3.0-only`, from
`spdx/license-list-data/json/details/AGPL-3.0-only.json`:

| | |
|---|---|
| words, ours | 5535 |
| words, SPDX | 5535 |
| sequence similarity | 0.99946 |
| substantive differences | **three, all the same one** |

The three:

```
ours  <https://fsf.org/>                    spdx  <http://fsf.org/>
ours  <https://www.gnu.org/licenses/>.      spdx  <http://www.gnu.org/licenses/>.
ours  <https://www.gnu.org/licenses/>.      spdx  <http://www.gnu.org/licenses/>.
```

`http` → `https` in three FSF and GNU URLs. That is the FSF's own migration to
TLS, present in the text they publish today; SPDX stores the older form. Nothing
in the licence's terms differs by a single word.

Ours is the FSF's current publication, which is the copy to ship.

The line-level diff looks much larger and is not: SPDX stores unwrapped
paragraphs (124 lines) against the FSF's wrapped text (661). Comparing lines
reports 24 hunks of pure rewrapping. Comparing words reports the three above.
Anyone re-doing this check should compare word streams, or they will conclude
the texts differ.

## How it is kept unaltered

`scripts/ci/license_boundary.py` pins the sha256. An edited GPL is not the GPL:
it loses compatibility with every other GPL-licensed work and the case law that
gives the text its meaning — and it would not look wrong, because it would still
read like a licence.

A structural check cannot catch this. Two of the mutations below change one word
each and leave the word count at exactly 5535; counting sections or words passes
both. Only the hash fails.

| mutation | result |
|---|---|
| "Additional permissions" → "Additional restrictions" in §7 | exit 1 |
| "You may convey" → "You must convey" | exit 1 |
| the file removed entirely | exit 1, named separately |
| one trailing newline added | exit 1 — bytes are bytes |
| restored | exit 0 |

If this gate ever fails legitimately, the FSF published a new text. Fetch it,
diff it deliberately, and move the pin **in the same commit**. Never let the
constant follow the file: that turns the gate into a record of whatever the file
happens to say.

## Our own terms are not in this file

The §7(b) attribution requirement is in `LICENSE-ADDITIONAL-TERMS`, which is what
section 7 is for. Nothing of ours is inside `LICENSE-AGPL`, and the hash is what
keeps it that way.
