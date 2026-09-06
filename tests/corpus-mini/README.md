# tests/corpus-mini — where these documents come from

Eleven small PDFs. They are small on purpose: they live in the repository, they
run in every test round, and what is in them has to be readable without tools.

**What each document is and is not is in
`crates/pdfluent/tests/every_fixture_is_what_its_name_says.rs`.** That is the
binding description; this file is about provenance and maintenance.

## Why provenance matters here

On 25-08-2026 **eight of the eleven** documents turned out to carry
`<<//Length` — a double slash, so the key is called `//Length` and not
`/Length`. The stream length was therefore unknown and the object did not load.

Nobody noticed, because the engine reads more tolerantly than lopdf. Text came
out all the same. Everything that reached the content stream through lopdf got
zero bytes, and redaction reported "no matches" on documents the term is plainly
in (#203).

One typo, repeated in eight files, invisible for three months — and it was not
eight typos. `scripts/corpus-generate-mini.py` wrote that shape, because every
caller opened the dictionary with `"<</"` and the generator appended `/Length`.
The files were repaired by hand in August and the generator was not, so anybody
regenerating the corpus would have written all eight straight back. Both are
fixed now, and `scripts/ci/test_fixtures_are_wellformed.py` runs the generator
and reads what it writes, so this cannot return through the one door that
repeats itself.

## The rules

**Adding a document?** It belongs in the table in
`every_fixture_is_what_its_name_says.rs`. That test fails on a document that is
not in it — a fixture nobody describes becomes the answer all by itself.

**Build the dictionaries, do not type them out.**
`scripts/corpus-generate-mini.py` computes the `/Length` values and the xref
offsets itself, so `<<//Length` cannot arise along that road.

**A reference is computed, not typed.** `generate_signed` wrote
`/Fields [99 0 R]` for an object that was given number 5, so signed-rsa.pdf
carried a signature no reader could reach and reported zero signatures. The
catalog now receives the numbers the objects actually got.

**Repairing an existing document? Keep the byte count the same.** The xref table
holds absolute offsets. In the 25-08 repair, changing `//Length` to `/Length`
moved everything by one byte per dictionary and the documents stopped loading
altogether; `<< /Length` (with a space) is the same length and just as valid.

## Per document

| document | provenance | rebuildable |
|---|---|---|
| `acroform.pdf` | generator, two form fields | yes |
| `acroform-multiselect.pdf` | by hand, a multi-select choice list; empty page content (`/Length 0`), which is deliberate | no — the only one without a generator |
| `encrypted.pdf` | generator, encrypted; **the password is written down nowhere** and it is usable only to test the refusal | yes |
| `malformed.pdf` | generator, a broken cross-reference table — the defect its name promises | yes |
| `multi-page.pdf` | generator, fifty pages carrying the same line, for pagination | yes |
| `pdfa-2b.pdf` | generator; claims PDF/A-2b in XMP and is **not** conformant; exists to test a false claim | yes |
| `scanned.pdf` | generator, no text layer, for the OCR route | yes |
| `signed-rsa.pdf` | generator, carries a signature dictionary (`/ByteRange`, `adbe.pkcs7`) | yes |
| `simple.pdf` | generator, one page, one line of text | yes |
| `xfa-form.pdf` | generator; a template with three fields and a dataset for one of them | yes |
| `zugferd.pdf` | generator, a ZUGFeRD-like structure | yes |

The committed bytes are not identical to what the generator writes today: the
documents were repaired by hand in August and the generator afterwards, and
regenerating them would move bytes that other manifests refer to. What the
generator guarantees is the shape, and `test_fixtures_are_wellformed.py` holds
it to that on every run.
