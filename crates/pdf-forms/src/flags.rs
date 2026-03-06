//! Field flags (/Ff) bitfield wrapper (B.1).

/// Wrapper around a PDF field-flags integer (/Ff).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FieldFlags(u32);

impl FieldFlags {
    /// Create from raw bits.
    pub fn from_bits(bits: u32) -> Self {
        Self(bits)
    }
    /// Empty (no flags set).
    pub fn empty() -> Self {
        Self(0)
    }
    /// Raw bits.
    pub fn bits(self) -> u32 {
        self.0
    }
    fn has(self, bit: u32) -> bool {
        self.0 & (1 << bit) != 0
    }

    // Common flags
    /// Bit 1: read-only.
    pub fn read_only(self) -> bool {
        self.has(0)
    }
    /// Bit 2: required.
    pub fn required(self) -> bool {
        self.has(1)
    }
    /// Bit 3: no-export.
    pub fn no_export(self) -> bool {
        self.has(2)
    }

    // Text field flags
    /// Bit 13: multiline.
    pub fn multiline(self) -> bool {
        self.has(12)
    }
    /// Bit 14: password.
    pub fn password(self) -> bool {
        self.has(13)
    }
    /// Bit 21: file-select.
    pub fn file_select(self) -> bool {
        self.has(20)
    }
    /// Bit 23: do-not-spell-check.
    pub fn do_not_spell_check(self) -> bool {
        self.has(22)
    }
    /// Bit 24: do-not-scroll.
    pub fn do_not_scroll(self) -> bool {
        self.has(23)
    }
    /// Bit 25: comb.
    pub fn comb(self) -> bool {
        self.has(24)
    }
    /// Bit 26: rich-text.
    pub fn rich_text(self) -> bool {
        self.has(25)
    }

    // Button flags
    /// Bit 15: no-toggle-to-off (radio buttons).
    pub fn no_toggle_to_off(self) -> bool {
        self.has(14)
    }
    /// Bit 16: radio.
    pub fn radio(self) -> bool {
        self.has(15)
    }
    /// Bit 17: push-button.
    pub fn push_button(self) -> bool {
        self.has(16)
    }
    /// Bit 26: radios-in-unison.
    pub fn radios_in_unison(self) -> bool {
        self.has(25)
    }

    // Choice flags
    /// Bit 18: combo.
    pub fn combo(self) -> bool {
        self.has(17)
    }
    /// Bit 19: edit (editable combo).
    pub fn edit(self) -> bool {
        self.has(18)
    }
    /// Bit 20: sort.
    pub fn sort(self) -> bool {
        self.has(19)
    }
    /// Bit 22: multi-select.
    pub fn multi_select(self) -> bool {
        self.has(21)
    }
    /// Bit 27: commit-on-sel-change.
    pub fn commit_on_sel_change(self) -> bool {
        self.has(26)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_flags() {
        assert!(!FieldFlags::empty().read_only());
    }
    #[test]
    fn read_only() {
        assert!(FieldFlags::from_bits(1).read_only());
    }
    #[test]
    fn multiline() {
        assert!(FieldFlags::from_bits(1 << 12).multiline());
    }
    #[test]
    fn combo() {
        assert!(FieldFlags::from_bits(1 << 17).combo());
    }
    #[test]
    fn push_button() {
        assert!(FieldFlags::from_bits(1 << 16).push_button());
    }
    #[test]
    fn radio() {
        assert!(FieldFlags::from_bits(1 << 15).radio());
    }
}
