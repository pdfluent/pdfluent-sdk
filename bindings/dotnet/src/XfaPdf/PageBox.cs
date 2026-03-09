namespace XfaPdf
{
    /// <summary>
    /// A PDF page boundary box (MediaBox, CropBox, etc.) in PDF points.
    /// </summary>
    public readonly struct PageBox
    {
        public double X0 { get; }
        public double Y0 { get; }
        public double X1 { get; }
        public double Y1 { get; }

        public double Width => X1 - X0;
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
