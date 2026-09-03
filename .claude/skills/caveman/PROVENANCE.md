# caveman — vendored, and where it came from

| | |
|---|---|
| Author | Julius Brussee |
| Source | https://github.com/JuliusBrussee/caveman |
| Path upstream | `skills/caveman/` |
| Commit | `9911e5fd0de1b77fe41a157214a4e83b92aadfa4` |
| Licence | MIT (see `LICENSE` beside this file) |

## Why MIT is the right answer here, and how it was established

The upstream repository is **dual-licensed** and the root `LICENSE` says so in a
scope note: MIT covers the repository *except* the Engine-linked directories,
which are Business Source License 1.1, and "New Engine-linked runtime modules
default to BSL-1.1 unless explicitly classified as MIT."

So "the repo is MIT" would have been the wrong reading. `LICENSING.md` settles
it per directory, and `skills/` is named there explicitly:

> | `skills/` | MIT | Existing Caveman skill stays MIT and untouched. |

Only `SKILL.md` and `README.md` from that directory are vendored here. Nothing
from `engine/`, `proxy/`, `rewriter/`, `browse/`, `mcp/`, `shrink/`, the cavemem
Go core or `shared/platform/` is present, and those are the parts that carry
BSL-1.1.

## The commit is verifiable, not asserted

`SKILL.md` here is byte-identical to `skills/caveman/SKILL.md` at the commit
above: 6518 bytes, sha256 `fec5718a391dd9d8c89746d39e0cc0fa7aa5b50447669a15e44c3244167828c0`.

That check also settles something recorded when this was vendored. The
installer's `skills-lock.json` claimed `1a41e7f3543c4a969c040c34a96925f8a3f82a6179a3f1002692b37957f7140b`
for this file, which matched nothing on disk under any normalisation. Fetching
the file from upstream gives `fec5718a...` — the same value as the vendored
copy. The lock file's hash was simply wrong, and dropping it rather than
committing an unverifiable provenance record was the right call.
