// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

using System;
using System.Collections.Generic;

namespace PDFluent
{
    /// <summary>One annotation listing.</summary>
    public sealed class AnnotEntry
    {
        /// <summary>Annotation object number.</summary>
        public int Xref { get; }

        /// <summary>Annotation type code.</summary>
        public int TypeCode { get; }

        /// <summary>Annotation type name.</summary>
        public string TypeName { get; }

        /// <summary>Annotation box in points.</summary>
        public Rect Rect { get; }

        /// <summary>Build a annotEntry.</summary>
        public AnnotEntry(int xref, int typeCode, string typeName, Rect rect)
        {
            Xref = xref;
            TypeCode = typeCode;
            TypeName = typeName;
            Rect = rect;
        }

        internal static unsafe List<AnnotEntry> ReadList(IntPtr list)
        {
            CompatSupport.Check(
                NativeMethods.pdf_compat_annot_entry_list_count(list, out int n), "Read");
            var result = new List<AnnotEntry>(n);
            for (int i = 0; i < n; i++)
            {
                CompatSupport.Check(NativeMethods.pdf_compat_annot_entry_list_xref(list, i, out int vXref), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_annot_entry_list_type_code(list, i, out int vTypeCode), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_annot_entry_list_type_name(list, i, out IntPtr vTypeName), "Read");
                Rect vRect = default;
                CompatSupport.Check(NativeMethods.pdf_compat_annot_entry_list_rect(list, i, &vRect), "Read");
                result.Add(new AnnotEntry(vXref, vTypeCode, CompatSupport.ReadString(vTypeName), vRect));
            }
            return result;
        }
    }
}
