# pdfluent-mcp

An [MCP](https://modelcontextprotocol.io) server for
[PDFluent](https://pdfluent.com). It lets an agent such as Claude Code or Codex
read a local PDF as markdown or as structured blocks, and ask which pages need
a vision model.

It runs over stdio, one process per client, and reads only files that you can
already read. It opens no network listener and uploads nothing.

## Tools

| Tool | Returns |
|---|---|
| `extract_markdown(path, first_page=0, page_count=None)` | The document as markdown, in reading order: headings, list items and tables. Running heads and page numbers are left out. |
| `extract_blocks(path, first_page=0, page_count=None, include_furniture=False)` | `blocks`: each with `kind`, `text`, `page`, `bbox` and `confidence`, plus `level`, `ordinal` or table-cell position where they apply. `pages_needing_vision`: the pages whose text layer is missing. |
| `vision_advice(path)` | One entry per page with `needs_vision` and its reasons. |

`confidence` is the probability of the learned model that decided a block's
kind. It is `null` where no model decided it. `null` means unknown, not low.

`vision_advice` measures one reason today: `no_text_layer`, meaning the page
draws ink but yields no text, as a scan does. `low_confidence_table` and
`figure` are always `false` until the engine can measure them.

## Install

Not on PyPI yet. It needs `pdfluent` 1.1.0 or later and Python 3.10 or later.
From a checkout of this repository:

```bash
pip install ./integrations/pdfluent-mcp
```

This puts a `pdfluent-mcp` command on your path.

### Claude Code

```bash
claude mcp add pdfluent -- pdfluent-mcp
```

To add the skill that tells Claude when to use these tools, copy
`skill/pdfluent` to `~/.claude/skills/pdfluent` (for every project) or to
`.claude/skills/pdfluent` in one project.

### Codex

Add the server to `~/.codex/config.toml`:

```toml
[mcp_servers.pdfluent]
command = "pdfluent-mcp"
```

For the same guidance the skill gives Claude, paste the "When to use which
tool" section of `skill/pdfluent/SKILL.md` into the project's `AGENTS.md`.

## Licence

AGPL-3.0-only, or the PDFluent Commercial Licence: see `LICENSE` and
`LICENSE-COMMERCIAL`. It depends on `mcp` (MIT) and `pdfluent`.
