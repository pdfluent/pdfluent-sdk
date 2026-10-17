// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

using System;
using System.Runtime.InteropServices;
using System.Text;

namespace PDFluent
{
    /// <summary>
    /// Shared plumbing for the compatible API (lane L5): status checks, error
    /// mapping, and native memory readers. Internal; the public classes
    /// delegate here so the P/Invoke call shape stays uniform.
    /// </summary>
    internal static class CompatSupport
    {
        /// <summary>Throw the mapped exception when a status code signals failure.</summary>
        internal static void Check(PdfStatus status, string what)
        {
            if (status != PdfStatus.Ok)
                throw Error(status, what);
        }

        /// <summary>Build the most specific exception for a (status, kind) pair.</summary>
        internal static PdfluentException Error(PdfStatus status, string what)
        {
            string? message = Marshal.PtrToStringUTF8(NativeMethods.pdf_get_last_error());
            if (string.IsNullOrEmpty(message))
                message = what + " failed";
            switch (NativeMethods.pdf_compat_last_error_kind())
            {
                case 7: return new EmptyFileError(message);
                case 6: return new FileDataError(message);
                case 5: return new FileNotFoundError(message);
                case 10: return new FitzDeprecation(message);
                default: break;
            }
            return PdfluentException.FromStatus(status, message);
        }

        /// <summary>Read a heap C string and free it.</summary>
        internal static string ReadString(IntPtr ptr)
        {
            if (ptr == IntPtr.Zero)
                return "";
            try
            {
                return Marshal.PtrToStringUTF8(ptr) ?? "";
            }
            finally
            {
                NativeMethods.pdf_string_free(ptr);
            }
        }

        /// <summary>Read heap bytes and free them.</summary>
        internal static byte[] ReadBytes(IntPtr ptr, UIntPtr len)
        {
            long count = (long)len;
            if (ptr == IntPtr.Zero || count == 0)
            {
                if (ptr != IntPtr.Zero)
                    NativeMethods.pdf_bytes_free(ptr, UIntPtr.Zero);
                return Array.Empty<byte>();
            }
            try
            {
                var buf = new byte[count];
                Marshal.Copy(ptr, buf, 0, (int)count);
                return buf;
            }
            finally
            {
                NativeMethods.pdf_bytes_free(ptr, len);
            }
        }

        /// <summary>Read heap ints and free them.</summary>
        internal static int[] ReadInts(IntPtr ptr, UIntPtr len)
        {
            long count = (long)len;
            if (ptr == IntPtr.Zero || count == 0)
            {
                if (ptr != IntPtr.Zero)
                    NativeMethods.pdf_bytes_free(ptr, UIntPtr.Zero);
                return Array.Empty<int>();
            }
            try
            {
                var buf = new int[count];
                Marshal.Copy(ptr, buf, 0, (int)count);
                return buf;
            }
            finally
            {
                NativeMethods.pdf_bytes_free(ptr, (UIntPtr)(count * 4));
            }
        }

        /// <summary>Read heap doubles and free them.</summary>
        internal static double[] ReadDoubles(IntPtr ptr, UIntPtr len)
        {
            long count = (long)len;
            if (ptr == IntPtr.Zero || count == 0)
            {
                if (ptr != IntPtr.Zero)
                    NativeMethods.pdf_bytes_free(ptr, UIntPtr.Zero);
                return Array.Empty<double>();
            }
            try
            {
                var buf = new double[count];
                Marshal.Copy(ptr, buf, 0, (int)count);
                return buf;
            }
            finally
            {
                NativeMethods.pdf_bytes_free(ptr, (UIntPtr)(count * 8));
            }
        }

        internal static int Flag(bool value) => value ? 1 : 0;

        internal static UIntPtr Count(Array? values) =>
            values == null ? UIntPtr.Zero : (UIntPtr)values.Length;

        /// <summary>
        /// Copy strings to native UTF-8 plus a pointer array. Free with
        /// <see cref="FreeStringArray"/>.
        /// </summary>
        internal static IntPtr AllocStringArray(string[]? values, out UIntPtr len)
        {
            if (values == null || values.Length == 0)
            {
                len = UIntPtr.Zero;
                return IntPtr.Zero;
            }
            IntPtr array = Marshal.AllocHGlobal(IntPtr.Size * values.Length);
            try
            {
                for (int i = 0; i < values.Length; i++)
                {
                    IntPtr s = Marshal.StringToCoTaskMemUTF8(values[i] ?? "");
                    Marshal.WriteIntPtr(array, i * IntPtr.Size, s);
                }
            }
            catch
            {
                len = (UIntPtr)values.Length;
                FreeStringArray(array, len);
                throw;
            }
            len = (UIntPtr)values.Length;
            return array;
        }

        internal static void FreeStringArray(IntPtr array, UIntPtr len)
        {
            if (array == IntPtr.Zero)
                return;
            for (long i = 0, n = (long)len; i < n; i++)
                Marshal.FreeCoTaskMem(Marshal.ReadIntPtr(array, (int)(i * IntPtr.Size)));
            Marshal.FreeHGlobal(array);
        }

        /// <summary>Quote a string for the options JSON wire format.</summary>
        internal static string JsonQuote(string value)
        {
            var out_ = new StringBuilder(value.Length + 2);
            out_.Append('"');
            foreach (char c in value)
            {
                switch (c)
                {
                    case '"': out_.Append("\\\""); break;
                    case '\\': out_.Append("\\\\"); break;
                    case '\n': out_.Append("\\n"); break;
                    case '\r': out_.Append("\\r"); break;
                    case '\t': out_.Append("\\t"); break;
                    default:
                        if (c < 0x20)
                            out_.Append("\\u" + ((int)c).ToString("x4"));
                        else
                            out_.Append(c);
                        break;
                }
            }
            out_.Append('"');
            return out_.ToString();
        }

        /// <summary>Render doubles for the options JSON wire format.</summary>
        internal static string JsonDoubles(double[] values)
        {
            var out_ = new StringBuilder("[");
            for (int i = 0; i < values.Length; i++)
            {
                if (i > 0)
                    out_.Append(',');
                out_.Append(values[i].ToString(System.Globalization.CultureInfo.InvariantCulture));
            }
            return out_.Append(']').ToString();
        }
    }
}
