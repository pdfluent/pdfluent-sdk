// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

using System;
using System.Globalization;
using System.Text;

namespace PDFluent
{
    /// <summary>Save and serialise options.</summary>
    ///
    /// <remarks>Mutable DTO with recorded defaults; serialises to the JSON
    /// the native layer overlays onto its own defaults.</remarks>
    public sealed class SaveOptions
    {
        /// <summary>Garbage-collection level.</summary>
        public int Garbage = 0;

        /// <summary>Clean content streams.</summary>
        public bool Clean = false;

        /// <summary>Deflate streams.</summary>
        public bool Deflate = false;

        /// <summary>Deflate images.</summary>
        public bool DeflateImages = false;

        /// <summary>Deflate fonts.</summary>
        public bool DeflateFonts = false;

        /// <summary>Append an incremental update.</summary>
        public bool Incremental = false;

        /// <summary>Use ASCII filters.</summary>
        public bool Ascii = false;

        /// <summary>Expand compressed objects.</summary>
        public bool Expand = false;

        /// <summary>Linearize the output.</summary>
        public bool Linear = false;

        /// <summary>Keep the file identifier.</summary>
        public bool NoNewId = false;

        /// <summary>Regenerate appearances.</summary>
        public bool Appearance = false;

        /// <summary>Pretty-print objects.</summary>
        public bool Pretty = false;

        /// <summary>Encryption selector.</summary>
        public int Encryption = 1;

        /// <summary>Permission bits.</summary>
        public int Permissions = 4095;

        /// <summary>Owner password, if any.</summary>
        public string? OwnerPassword = null;

        /// <summary>User password, if any.</summary>
        public string? UserPassword = null;

        /// <summary>Keep XMP metadata.</summary>
        public bool PreserveMetadata = true;

        /// <summary>Use object streams.</summary>
        public bool UseObjstms = false;

        /// <summary>Compression effort.</summary>
        public int CompressionEffort = 0;

        /// <summary>Fail on repair.</summary>
        public bool RaiseOnRepair = false;

        /// <summary>Byte-stable output.</summary>
        public bool Reproducible = false;

        /// <summary>Serialise to the native JSON overlay.</summary>
        public string ToJson()
        {
            var out_ = new StringBuilder("{");
            bool first = true;
            first = Append(out_, first, "garbage", Garbage.ToString(CultureInfo.InvariantCulture));
            first = Append(out_, first, "clean", (Clean ? "true" : "false"));
            first = Append(out_, first, "deflate", (Deflate ? "true" : "false"));
            first = Append(out_, first, "deflate_images", (DeflateImages ? "true" : "false"));
            first = Append(out_, first, "deflate_fonts", (DeflateFonts ? "true" : "false"));
            first = Append(out_, first, "incremental", (Incremental ? "true" : "false"));
            first = Append(out_, first, "ascii", (Ascii ? "true" : "false"));
            first = Append(out_, first, "expand", (Expand ? "true" : "false"));
            first = Append(out_, first, "linear", (Linear ? "true" : "false"));
            first = Append(out_, first, "no_new_id", (NoNewId ? "true" : "false"));
            first = Append(out_, first, "appearance", (Appearance ? "true" : "false"));
            first = Append(out_, first, "pretty", (Pretty ? "true" : "false"));
            first = Append(out_, first, "encryption", Encryption.ToString(CultureInfo.InvariantCulture));
            first = Append(out_, first, "permissions", Permissions.ToString(CultureInfo.InvariantCulture));
            first = Append(out_, first, "owner_password", (OwnerPassword == null ? null : CompatSupport.JsonQuote(OwnerPassword)));
            first = Append(out_, first, "user_password", (UserPassword == null ? null : CompatSupport.JsonQuote(UserPassword)));
            first = Append(out_, first, "preserve_metadata", (PreserveMetadata ? "true" : "false"));
            first = Append(out_, first, "use_objstms", (UseObjstms ? "true" : "false"));
            first = Append(out_, first, "compression_effort", CompressionEffort.ToString(CultureInfo.InvariantCulture));
            first = Append(out_, first, "raise_on_repair", (RaiseOnRepair ? "true" : "false"));
            first = Append(out_, first, "reproducible", (Reproducible ? "true" : "false"));
            return out_.Append("}").ToString();
        }

        private static bool Append(StringBuilder out_, bool first, string key, string? value)
        {
            if (value == null)
                return first;
            if (!first)
                out_.Append(",");
            out_.Append(CompatSupport.JsonQuote(key)).Append(":").Append(value);
            return false;
        }
    }
}
