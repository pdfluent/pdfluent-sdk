The CMYK profile and its CC0-1.0 license are copied byte-for-byte from the
existing `pdf-interpret/assets` profile used by the renderer. Keeping a local
copy makes the `pdf-manip` package self-contained.

Upstream: [Compact ICC Profiles, CMYK](https://github.com/saucecontrol/Compact-ICC-Profiles#cmyk).

The profile supplies the full lookup tables needed for a CMYK-to-PCS color
transform. It replaces a generated placeholder that declared LUTs without
including their samples. Existing document profiles are preserved.
