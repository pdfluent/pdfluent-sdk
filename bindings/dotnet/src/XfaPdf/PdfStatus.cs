namespace XfaPdf
{
    /// <summary>
    /// Status codes returned by native PDF operations. Mirrors the C API PdfStatus enum.
    /// </summary>
    public enum PdfStatus
    {
        Ok = 0,
        ErrorInvalidArgument = 1,
        ErrorFileNotFound = 2,
        ErrorInvalidPassword = 3,
        ErrorCorruptPdf = 4,
        ErrorPageRange = 5,
        ErrorRender = 6,
        ErrorUnknown = 99,
    }
}
