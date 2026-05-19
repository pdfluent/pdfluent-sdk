namespace PDFluent
{
    /// <summary>
    /// A single structured text block extracted from a PDF page.
    /// </summary>
    /// <remarks>
    /// <para>
    /// Coordinates are in PDF user-space points (1/72 inch). The origin is
    /// the bottom-left of the page (PDF convention). <see cref="Width"/>
    /// and <see cref="Height"/> are always non-negative; empty blocks
    /// have all four geometric fields set to zero.
    /// </para>
    /// <para>
    /// Returned by <see cref="PdfDocument.ExtractTextBlocks(int)"/>.
    /// </para>
    /// </remarks>
    public readonly struct TextBlock : System.IEquatable<TextBlock>
    {
        /// <summary>PDF user-space X of the block's bottom-left corner.</summary>
        public double X { get; }

        /// <summary>PDF user-space Y of the block's bottom-left corner.</summary>
        public double Y { get; }

        /// <summary>Block width in PDF points. Always &gt;= 0.</summary>
        public double Width { get; }

        /// <summary>Block height in PDF points. Always &gt;= 0.</summary>
        public double Height { get; }

        /// <summary>
        /// Concatenated UTF-8 text of all spans in this block, joined in
        /// reading order. Never <c>null</c>; may be empty.
        /// </summary>
        public string Text { get; }

        /// <summary>Initializes a new <see cref="TextBlock"/>.</summary>
        public TextBlock(double x, double y, double width, double height, string text)
        {
            X = x;
            Y = y;
            Width = width;
            Height = height;
            Text = text ?? string.Empty;
        }

        /// <inheritdoc/>
        public bool Equals(TextBlock other) =>
            X == other.X && Y == other.Y &&
            Width == other.Width && Height == other.Height &&
            Text == other.Text;

        /// <inheritdoc/>
        public override bool Equals(object? obj) => obj is TextBlock other && Equals(other);

        /// <inheritdoc/>
        public override int GetHashCode() =>
            System.HashCode.Combine(X, Y, Width, Height, Text);

        /// <inheritdoc/>
        public override string ToString() =>
            $"TextBlock(x={X}, y={Y}, w={Width}, h={Height}, text=\"{Text}\")";

        /// <summary>Equality operator.</summary>
        public static bool operator ==(TextBlock left, TextBlock right) => left.Equals(right);

        /// <summary>Inequality operator.</summary>
        public static bool operator !=(TextBlock left, TextBlock right) => !left.Equals(right);
    }
}
