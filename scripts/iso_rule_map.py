#!/usr/bin/env python3
"""Map veraPDF rule IDs to ISO 19005-2 section references.

Used by the dashboard and agent workflow to provide ISO context
for each veraPDF rule failure. Based on the veraPDF PDFA-2B.xml
validation profile.

Usage: import iso_rule_map; iso_rule_map.get_iso_ref("6.2.11.5:1")
"""

# Maps "clause:testNumber" -> (ISO section, short description, spec reference)
RULES = {
    "6.1.2:1": ("6.1.2", "File header format", "ISO 19005-2, S6.1.2"),
    "6.1.2:2": ("6.1.2", "Binary comment", "ISO 19005-2, S6.1.2"),
    "6.1.3:1": ("6.1.3", "Trailer ID keyword", "ISO 19005-2, S6.1.3"),
    "6.1.3:2": ("6.1.3", "No Encrypt in trailer", "ISO 19005-2, S6.1.3"),
    "6.1.6:1": ("6.1.6", "Hex string even chars", "ISO 19005-2, S6.1.6"),
    "6.1.7.1:1": ("6.1.7.1", "Stream Length key", "ISO 19005-2, S6.1.7; ISO 32000-1, S7.3.8"),
    "6.1.7.2:1": ("6.1.7.2", "Allowed stream filters only", "ISO 19005-2, S6.1.7; ISO 32000-1, S7.4 Table 6"),
    "6.1.12:1": ("6.1.12", "Permissions key restriction", "ISO 19005-2, S6.1.12"),
    "6.1.13:4": ("6.1.13", "Name max 127 bytes", "ISO 19005-2, S6.1.13; ISO 32000-1, S7.3.5"),
    "6.2.2:1": ("6.2.2", "No undefined operators", "ISO 19005-2, S6.2.2"),
    "6.2.2:2": ("6.2.2", "Content stream needs Resources", "ISO 19005-2, S6.2.2; ISO 32000-1, S7.8.3"),
    "6.2.3:3": ("6.2.3", "OutputIntent profile", "ISO 19005-2, S6.2.3"),
    "6.2.4.2:1": ("6.2.4.2", "ICC profile conformance", "ISO 19005-2, S6.2.4; ICC.1:2004-10"),
    "6.2.4.3:2": ("6.2.4.3", "DeviceRGB needs OutputIntent", "ISO 19005-2, S6.2.4.3"),
    "6.2.4.3:3": ("6.2.4.3", "DeviceCMYK needs OutputIntent", "ISO 19005-2, S6.2.4.3"),
    "6.2.4.3:4": ("6.2.4.3", "DeviceGray needs OutputIntent", "ISO 19005-2, S6.2.4.3"),
    "6.2.4.4:1": ("6.2.4.4", "DeviceN colorants dict", "ISO 19005-2, S6.2.4.4"),
    "6.2.5:3": ("6.2.5", "No HTP in ExtGState", "ISO 19005-2, S6.2.5"),
    "6.2.8:3": ("6.2.8", "Image Interpolate=false", "ISO 19005-2, S6.2.8; ISO 32000-1, S8.9.5.3"),
    "6.2.9:1": ("6.2.9", "Form XObject restrictions", "ISO 19005-2, S6.2.9"),
    "6.2.10:1": ("6.2.10", "Blend mode compliance", "ISO 19005-2, S6.2.10"),
    "6.2.11.2:1": ("6.2.11.2", "Font dict Type entry", "ISO 19005-2, S6.2.11; ISO 32000-1, S9.6"),
    "6.2.11.3.1:1": ("6.2.11.3.1", "CIDSystemInfo consistency", "ISO 19005-2, S6.2.11.3; ISO 32000-1, S9.7.3"),
    "6.2.11.3.2:1": ("6.2.11.3.2", "CIDToGIDMap required", "ISO 19005-2, S6.2.11.3; ISO 32000-1, S9.7.4 Table 117"),
    "6.2.11.3.3:1": ("6.2.11.3.3", "Non-standard CMap embedded", "ISO 19005-2, S6.2.11.3; ISO 32000-1, S9.7.5"),
    "6.2.11.3.3:2": ("6.2.11.3.3", "CMap WMode consistency", "ISO 19005-2, S6.2.11.3; ISO 32000-1, S9.7.5"),
    "6.2.11.4.1:1": ("6.2.11.4.1", "Font program must be embedded", "ISO 19005-2, S6.2.11.4; ISO 32000-1, S9.9"),
    "6.2.11.4.1:2": ("6.2.11.4.1", "All rendered glyphs present in font", "ISO 19005-2, S6.2.11.4; ISO 32000-1, S9.6.6.4"),
    "6.2.11.4.2:1": ("6.2.11.4.2", "Type1 CharSet lists all glyphs", "ISO 19005-2, S6.2.11.4; ISO 32000-1, S9.8.1 Table 122"),
    "6.2.11.4.2:2": ("6.2.11.4.2", "CIDFont CIDSet completeness", "ISO 19005-2, S6.2.11.4"),
    "6.2.11.5:1": ("6.2.11.5", "Font width dict matches program", "ISO 19005-2, S6.2.11.5; ISO 32000-1, S9.6.1/S9.7.4.3"),
    "6.2.11.6:1": ("6.2.11.6", "Non-symbolic TT cmap entry", "ISO 19005-2, S6.2.11.6; ISO 32000-1, S9.6.6.4"),
    "6.2.11.6:2": ("6.2.11.6", "Non-symbolic TT encoding", "ISO 19005-2, S6.2.11.6; ISO 32000-1, S9.6.6.4"),
    "6.2.11.6:3": ("6.2.11.6", "Symbolic TT no Encoding key", "ISO 19005-2, S6.2.11.6"),
    "6.2.11.6:4": ("6.2.11.6", "Symbolic TT cmap table", "ISO 19005-2, S6.2.11.6"),
    "6.2.11.8:1": ("6.2.11.8", ".notdef glyph reference", "ISO 19005-2, S6.2.11.8"),
    "6.3.1:1": ("6.3.1", "Allowed annotation types", "ISO 19005-2, S6.3.1; ISO 32000-1, S12.5.6"),
    "6.3.2:1": ("6.3.2", "Annotation F key required", "ISO 19005-2, S6.3.2"),
    "6.3.2:2": ("6.3.2", "Annotation flags Print=1", "ISO 19005-2, S6.3.2"),
    "6.3.3:1": ("6.3.3", "Annotation AP dict required", "ISO 19005-2, S6.3.3; ISO 32000-1, S12.5.5"),
    "6.3.3:2": ("6.3.3", "AP dict only N key", "ISO 19005-2, S6.3.3"),
    "6.3.3:3": ("6.3.3", "Widget/Btn AP/N subdictionary", "ISO 19005-2, S6.3.3"),
    "6.5.1:1": ("6.5.1", "Action type restriction", "ISO 19005-2, S6.5.1"),
    "6.5.1:2": ("6.5.1", "Named action restriction", "ISO 19005-2, S6.5.1"),
    "6.5.2:1": ("6.5.2", "No Catalog AA key", "ISO 19005-2, S6.5.2"),
    "6.5.2:2": ("6.5.2", "No Page AA key", "ISO 19005-2, S6.5.2"),
    "6.7.3:1": ("6.7.3", "XMP metadata validation", "ISO 19005-2, S6.7.3"),
    "6.8:2": ("6.8", "File specification EF", "ISO 19005-2, S6.8"),
    "6.9:1": ("6.9", "OCG config Name key", "ISO 19005-2, S6.9"),
}


def get_iso_ref(rule_id: str) -> tuple:
    """Get ISO reference for a veraPDF rule ID like '6.2.11.5:1'.

    Returns (iso_section, short_description, full_spec_reference) or None.
    """
    return RULES.get(rule_id)


def get_iso_section(rule_id: str) -> str:
    """Get just the ISO section number, e.g. '6.2.11.5'."""
    ref = RULES.get(rule_id)
    return ref[0] if ref else rule_id.split(":")[0]


def get_spec_reference(rule_id: str) -> str:
    """Get the full spec reference string."""
    ref = RULES.get(rule_id)
    return ref[2] if ref else f"ISO 19005-2, S{rule_id.split(':')[0]}"
