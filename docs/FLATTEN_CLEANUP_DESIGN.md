# Flatten Cleanup Design

FSC-03 documents the low-risk cleanup design for BUG 1 in
`crates/pdf-xfa/src/flatten.rs`.

## Problem

`remove_acroform()` currently removes `/AcroForm` from the catalog and removes `/XFA`
from the AcroForm dictionary, but it does not delete the XFA packet stream objects from
`doc.objects`.

That is not sufficient when writing with lopdf:

- `lopdf::Document::save_to()` serializes every object still present in the object table
- unreachable objects are therefore still written into the output file
- raw-byte validators then still find `/XFA` or widget-related sequences inside those orphaned
  objects

The FSC-01 investigation confirmed this on real corpus files: the catalog was clean, but XFA
packet streams remained serialized as orphaned objects.

## Chosen approach

Use a targeted object purge inside `remove_acroform()`.

After the function captures `acroform_id` from the catalog, it should:

1. Re-read the AcroForm dictionary before mutating it.
2. Inspect the `/XFA` entry and collect every referenced object ID.
3. Remove the `/XFA` key from the AcroForm dictionary.
4. Remove each collected XFA object from `doc.objects`.
5. Remove the AcroForm object itself from `doc.objects`.

This matches the actual structure of XFA packets in PDFs:

- `/XFA` is usually an array of alternating packet names and stream references
- example shape: `[ /xdp:xdp, 10 0 R, /template, 11 0 R, /datasets, 12 0 R ]`
- only the indirect references in that array should be collected
- if `/XFA` is a single indirect reference instead of an array, collect that one object ID

## Why this is the right scope

This design is intentionally narrow.

- It removes only objects that were directly referenced by the AcroForm `/XFA` entry.
- It also removes the AcroForm dictionary object that is no longer reachable from the catalog.
- It does not attempt to garbage-collect arbitrary unreferenced objects elsewhere in the file.

That keeps the cleanup aligned with the verified root cause and avoids turning FSC-05 into a
general-purpose PDF reachability pass.

## Safety

The targeted purge is low risk for rendering correctness.

- The objects referenced by `/XFA` are data packets used by interactive XFA processing.
- They are not page content streams and are not needed once flattening has produced static page
  content.
- After `/AcroForm` is removed from the catalog, the AcroForm object and its `/XFA` children are
  intentionally unreachable from the document root.

The purge therefore changes serialized residue, not visible page rendering.

## Expected implementation shape

Inside `remove_acroform()`:

1. Capture `acroform_id` from the catalog as the code already does.
2. Before deleting `/XFA`, inspect the AcroForm dictionary and collect:
   - every `Object::Reference` inside an `/XFA` array
   - or the single `Object::Reference` if `/XFA` is stored directly
3. Remove `/XFA` from the AcroForm dictionary.
4. Remove all collected XFA stream IDs from `doc.objects`.
5. Remove `acroform_id` itself from `doc.objects`.
6. Continue with widget annotation cleanup as before.

## Alternative considered

Full reachability traversal of the PDF object graph was considered and rejected.

Reasons for rejection:

- more complex to implement and review
- higher risk of removing objects outside the proven problem area
- not required for this milestone because FSC-01 isolated the problem to AcroForm/XFA residue

The milestone only needs a targeted purge of AcroForm-owned XFA packet objects, so that is the
design chosen for FSC-05.
