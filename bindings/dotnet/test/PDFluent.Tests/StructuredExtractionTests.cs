// Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
//
// This software is proprietary. The PDFluent application is free to use,
// including for commercial purposes. Redistribution, or extraction or reuse
// of its components (including the embedded PDF engine), requires a licence.
// See https://pdfluent.com/license for terms.

using System;
using System.IO;
using System.Linq;
using System.Text.Json;
using Xunit;

namespace PDFluent.Tests
{
    /// <summary>
    /// The structured-extraction API through the .NET binding (#573).
    ///
    /// Two methods, one document. <c>fixtures/structured.pdf</c> is built and
    /// pinned by <c>crates/pdf-engine/tests/the_api_over_the_block_model.rs</c>:
    /// a heading and a bulleted list on page 0, a ruled table on page 1. Every
    /// binding suite reads that same file and asserts the same three things, so
    /// a fault in one crossing stands next to five that still work.
    ///
    /// The blocks cross the boundary as JSON, which is what this package does
    /// with every structured result (see <see cref="TextEditor"/>): it carries
    /// no dependencies of its own. The test project targets net8.0, so it reads
    /// the JSON back with the framework's own reader and asserts on values
    /// rather than on substrings.
    /// </summary>
    public class StructuredExtractionTests
    {
        /// <summary>
        /// The repository's <c>fixtures</c> directory, found by walking up from
        /// the test assembly. The assembly sits several directories deep under
        /// <c>bin/</c>, and how deep depends on the configuration.
        /// </summary>
        private static string FixturePath()
        {
            var at = new DirectoryInfo(AppContext.BaseDirectory);
            while (at != null)
            {
                string candidate = Path.Combine(at.FullName, "fixtures", "structured.pdf");
                if (File.Exists(candidate))
                    return candidate;
                at = at.Parent;
            }
            throw new InvalidOperationException(
                "fixtures/structured.pdf not found above " + AppContext.BaseDirectory
                + "; it is built by crates/pdf-engine/tests/the_api_over_the_block_model.rs");
        }

        private static PdfDocument Open() => PdfDocument.Open(File.ReadAllBytes(FixturePath()));

        [Fact]
        public void MarkdownCarriesAHeadingAListItemAndATableCell()
        {
            using var doc = Open();

            string markdown = doc.ExtractMarkdown();

            Assert.Contains("# Safety rules", markdown);
            Assert.Contains("- Wear the issued boots on the floor.", markdown);
            Assert.Contains("| Band | Units | Rate |", markdown);
        }

        [Fact]
        public void BlocksCarryAHeadingAListItemAndATableCell()
        {
            using var doc = Open();

            using var parsed = JsonDocument.Parse(doc.ExtractBlocks());
            var blocks = parsed.RootElement.GetProperty("blocks").EnumerateArray().ToList();

            Assert.Contains(blocks, b =>
                b.GetProperty("kind").GetString() == "heading"
                && b.GetProperty("text").GetString() == "Safety rules"
                && b.GetProperty("level").GetInt32() == 1);
            Assert.Contains(blocks, b =>
                b.GetProperty("kind").GetString() == "listItem"
                && b.GetProperty("text").GetString() == "Wear the issued boots on the floor.");
            Assert.Contains(blocks, b =>
                b.GetProperty("kind").GetString() == "tableCell"
                && b.GetProperty("text").GetString() == "Band"
                && b.GetProperty("row").GetInt32() == 0
                && b.GetProperty("column").GetInt32() == 0
                && b.GetProperty("header").GetBoolean());
        }

        [Fact]
        public void ABlockCarriesItsPageAndItsBox()
        {
            using var doc = Open();

            using var parsed = JsonDocument.Parse(doc.ExtractBlocks());
            var first = parsed.RootElement.GetProperty("blocks").EnumerateArray().First();

            Assert.Equal(0, first.GetProperty("page").GetInt32());
            Assert.Equal(0, first.GetProperty("index").GetInt32());
            Assert.False(first.GetProperty("continuesPrevious").GetBoolean());
            Assert.Equal(4, first.GetProperty("bbox").GetArrayLength());
        }

        [Fact]
        public void DoclingCarriesAHeadingAListAndATable()
        {
            using var doc = Open();

            using var parsed = JsonDocument.Parse(doc.ExtractDocling(name: "structured"));
            var root = parsed.RootElement;
            var labels = root.GetProperty("texts").EnumerateArray()
                .Select(t => t.GetProperty("label").GetString()).ToList();

            Assert.Equal("DoclingDocument", root.GetProperty("schema_name").GetString());
            Assert.Equal("structured", root.GetProperty("name").GetString());
            Assert.Contains("section_header", labels);
            Assert.Contains("list_item", labels);
            Assert.Equal(1, root.GetProperty("tables").GetArrayLength());
            Assert.Equal("document",
                JsonDocument.Parse(doc.ExtractDocling()).RootElement.GetProperty("name").GetString());
        }

        [Fact]
        public void AWindowHoldsThePagesItAsksFor()
        {
            using var doc = Open();

            using var parsed = JsonDocument.Parse(doc.ExtractBlocks(1, 1));
            var blocks = parsed.RootElement.GetProperty("blocks").EnumerateArray().ToList();

            Assert.NotEmpty(blocks);
            Assert.All(blocks, b => Assert.Equal(1, b.GetProperty("page").GetInt32()));
            Assert.DoesNotContain(blocks, b => b.GetProperty("text").GetString() == "Safety rules");
            Assert.Contains("| Band | Units | Rate |", doc.ExtractMarkdown(1, 1));
        }

        [Fact]
        public void AWindowPastTheEndIsEmptyRatherThanAnError()
        {
            using var doc = Open();

            using var parsed = JsonDocument.Parse(doc.ExtractBlocks(9, 4));
            Assert.Empty(parsed.RootElement.GetProperty("blocks").EnumerateArray());
            Assert.Equal("", doc.ExtractMarkdown(9, 4));
        }
    }
}
