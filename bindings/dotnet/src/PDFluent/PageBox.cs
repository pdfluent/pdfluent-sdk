namespace PDFluent
{
    /// <summary>
    /// A PDF page boundary box (MediaBox, CropBox, etc.) in PDF points (1/72 inch).
    /// </summary>
    public readonly struct PageBox
    {
        /// <summary>Left edge of the box in PDF points.</summary>
        public double X0 { get; }

        /// <summary>Bottom edge of the box in PDF points.</summary>
        public double Y0 { get; }

        /// <summary>Right edge of the box in PDF points.</summary>
        public double X1 { get; }

        /// <summary>Top edge of the box in PDF points.</summary>
        public double Y1 { get; }

        /// <summary>Width of the box (<c>X1 - X0</c>) in PDF points.</summary>
        public double Width => X1 - X0;

        /// <summary>Height of the box (<c>Y1 - Y0</c>) in PDF points.</summary>
        public double Height => Y1 - Y0;

        internal PageBox(double x0, double y0, double x1, double y1)
        {
            X0 = x0;
            Y0 = y0;
            X1 = x1;
            Y1 = y1;
        }
    }
}
