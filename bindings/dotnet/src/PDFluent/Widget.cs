// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

using System;
using System.Collections.Generic;

namespace PDFluent
{

    /// <summary>
    /// A compatible form widget: owned field data built blank, then added to a page.
    /// </summary>
    public sealed class Widget : IDisposable
    {
        private IntPtr _handle;

        /// <summary>
        /// Build a blank widget.
        /// </summary>
        public Widget()
        {
            CompatSupport.Check(NativeMethods.pdf_compat_widget_new(out IntPtr hPtr),
                "Widget");
            _handle = hPtr;
        }

        internal Widget(IntPtr handle)
        {
            _handle = handle;
        }


        /// <summary>Widget field name.</summary>
        public string GetFieldName()
        {
            CompatSupport.Check(NativeMethods.pdf_compat_widget_field_name(RequireOpen(),
                out IntPtr s), "GetFieldName");
            return CompatSupport.ReadString(s);
        }

        /// <summary>Set the widget field name.</summary>
        public void SetFieldName(string? value)
        {
            CompatSupport.Check(NativeMethods.pdf_compat_widget_set_field_name(RequireOpen(),
                value), "SetFieldName");
        }

        /// <summary>Widget field-type code.</summary>
        public int GetFieldType()
        {
            CompatSupport.Check(NativeMethods.pdf_compat_widget_field_type(RequireOpen(),
                out int v), "GetFieldType");
            return v;
        }

        /// <summary>Set the widget field-type code.</summary>
        public void SetFieldType(int value)
        {
            CompatSupport.Check(NativeMethods.pdf_compat_widget_set_field_type(RequireOpen(),
                value), "SetFieldType");
        }

        /// <summary>Widget field value text.</summary>
        public string GetFieldValue()
        {
            CompatSupport.Check(NativeMethods.pdf_compat_widget_field_value(RequireOpen(),
                out IntPtr s), "GetFieldValue");
            return CompatSupport.ReadString(s);
        }

        /// <summary>Set the widget field value text.</summary>
        public void SetFieldValue(string? value)
        {
            CompatSupport.Check(NativeMethods.pdf_compat_widget_set_field_value(RequireOpen(),
                value), "SetFieldValue");
        }

        /// <summary>Widget box in points.</summary>
        public unsafe Rect GetRect()
        {
            Rect r = default;
            CompatSupport.Check(NativeMethods.pdf_compat_widget_rect(RequireOpen(), &r),
                "GetRect");
            return r;
        }

        /// <summary>Set the widget box.</summary>
        public unsafe void SetRect(Rect value)
        {
            Rect v = value;
            CompatSupport.Check(NativeMethods.pdf_compat_widget_set_rect(RequireOpen(), &v),
                "SetRect");
        }

        internal static List<Widget> ReadList(IntPtr list)
        {
            CompatSupport.Check(NativeMethods.pdf_compat_widget_list_count(list, out int n),
                "Read");
            var result = new List<Widget>(n);
            for (int i = 0; i < n; i++)
            {
                CompatSupport.Check(NativeMethods.pdf_compat_widget_list_at(list, i,
                    out IntPtr hPtr), "Read");
                result.Add(new Widget(hPtr));
            }
            return result;
        }


        /// <summary>
        /// Close the handle. Idempotent.
        /// </summary>
        public void Dispose()
        {
            if (_handle != IntPtr.Zero)
            {
                NativeMethods.pdf_compat_widget_free(_handle);
                _handle = IntPtr.Zero;
            }
            GC.SuppressFinalize(this);
        }

        /// <summary>Finalize the handle.</summary>
        ~Widget()
        {
            if (_handle != IntPtr.Zero)
                NativeMethods.pdf_compat_widget_free(_handle);
        }

        /// <summary>
        /// The native handle, failing when closed.
        /// </summary>
        /// <returns>The live handle.</returns>
        public IntPtr RequireOpen()
        {
            if (_handle == IntPtr.Zero)
                throw new PdfluentException("widget is closed");
            return _handle;
        }
    }
}
