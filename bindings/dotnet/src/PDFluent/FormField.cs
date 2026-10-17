// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

namespace PDFluent
{
    /// <summary>
    /// One terminal AcroForm field, as returned by
    /// <see cref="PdfDocument.GetFormFields"/>.
    /// </summary>
    public sealed class FormField
    {
        /// <summary>Fully qualified field name.</summary>
        public string Name { get; }

        /// <summary>
        /// Field type: <c>"text"</c>, <c>"button"</c>, <c>"choice"</c>,
        /// <c>"signature"</c>, or <c>"unknown"</c>.
        /// </summary>
        public string FieldType { get; }

        /// <summary>
        /// Current value, or <see langword="null"/> when the field
        /// carries none.
        /// </summary>
        public string? Value { get; }

        internal FormField(string name, string fieldType, string? value)
        {
            Name = name;
            FieldType = fieldType;
            Value = value;
        }
    }
}
