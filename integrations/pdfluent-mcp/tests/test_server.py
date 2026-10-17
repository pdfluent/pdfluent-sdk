"""The MCP server over stdio, called the way Claude Code and Codex call it (#625).

A real client starts the server as a subprocess and speaks the protocol to it:
the tools are listed, then each is called on fixtures/structured.pdf (a heading
and a bulleted list on page 0, a ruled table on page 1, pinned by
crates/pdf-engine/tests/the_api_over_the_block_model.rs) and on one scanned
page.
"""

import json
import os
import sys

import anyio
from mcp import ClientSession
from mcp.client.stdio import StdioServerParameters, stdio_client

FIXTURES = os.path.join(os.path.dirname(__file__), "..", "..", "..", "fixtures")
STRUCTURED_PDF = os.path.abspath(os.path.join(FIXTURES, "structured.pdf"))
SCANNED_PDF = os.path.abspath(os.path.join(FIXTURES, "scanned", "scan_01_sample.pdf"))


def _session(body):
    """Run ``body(session)`` against a freshly started server."""

    async def run():
        params = StdioServerParameters(
            command=sys.executable, args=["-m", "pdfluent_mcp.server"]
        )
        async with stdio_client(params) as (read, write):
            async with ClientSession(read, write) as session:
                await session.initialize()
                return await body(session)

    return anyio.run(run)


def _payload(result):
    """A tool result as the value the tool returned."""
    assert not result.is_error, result.content
    if result.structured_content is not None:
        content = result.structured_content
        # A tool that returns a list or a string is wrapped as {"result": ...}.
        return content.get("result", content) if set(content) == {"result"} else content
    return json.loads(result.content[0].text)


def test_the_server_lists_its_three_tools():
    async def body(session):
        return await session.list_tools()

    names = sorted(t.name for t in _session(body).tools)
    assert names == ["extract_blocks", "extract_markdown", "vision_advice"]


def test_extract_markdown_reads_a_heading_a_list_item_and_a_table():
    async def body(session):
        return await session.call_tool("extract_markdown", {"path": STRUCTURED_PDF})

    result = _session(body)
    assert not result.is_error, result.content
    markdown = result.content[0].text
    assert "# Safety rules" in markdown
    assert "- Wear the issued boots on the floor." in markdown
    assert "| Band | Units | Rate |" in markdown


def test_extract_blocks_carries_kind_page_box_and_confidence():
    async def body(session):
        return await session.call_tool("extract_blocks", {"path": STRUCTURED_PDF})

    out = _payload(_session(body))
    blocks = out["blocks"]
    heading = next(b for b in blocks if b["kind"] == "heading")
    assert heading["text"] == "Safety rules"
    assert heading["level"] == 1
    assert heading["page"] == 0
    assert len(heading["bbox"]) == 4
    # Present on every block, null where no model decided the kind.
    assert all("confidence" in b for b in blocks)
    cell = next(b for b in blocks if b["kind"] == "table_cell" and b["text"] == "Band")
    assert (cell["row"], cell["column"], cell["header"]) == (0, 0, True)
    assert out["pages_needing_vision"] == []


def test_a_scanned_page_is_named_as_needing_vision():
    async def body(session):
        advice = await session.call_tool("vision_advice", {"path": SCANNED_PDF})
        blocks = await session.call_tool("extract_blocks", {"path": SCANNED_PDF})
        return advice, blocks

    advice, blocks = _session(body)
    (page,) = _payload(advice)
    assert page["needs_vision"] is True
    assert page["no_text_layer"] is True
    assert _payload(blocks)["pages_needing_vision"] == [0]


def test_a_missing_file_is_an_error_the_agent_can_read():
    async def body(session):
        return await session.call_tool("extract_markdown", {"path": "/no/such/file.pdf"})

    result = _session(body)
    assert result.is_error
    assert "no file at" in result.content[0].text


def test_a_file_that_is_not_a_pdf_is_an_error_the_agent_can_read(tmp_path):
    bogus = tmp_path / "notes.pdf"
    bogus.write_text("this is not a PDF")

    async def body(session):
        return await session.call_tool("extract_blocks", {"path": str(bogus)})

    result = _session(body)
    assert result.is_error
    assert "Pdfluent" in result.content[0].text
