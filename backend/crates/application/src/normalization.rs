use unicode_normalization::UnicodeNormalization;

/// Produces the diacritic-preserving exact-search key for a food name.
///
/// The function applies Unicode NFC, Unicode lowercase conversion, and whitespace collapse. It
/// intentionally does not remove Vietnamese diacritics.
#[must_use]
pub fn normalize_vi_search_key(value: &str) -> String {
    value
        .nfc()
        .flat_map(char::to_lowercase)
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::normalize_vi_search_key;

    #[test]
    fn preserves_diacritics_and_collapses_whitespace() {
        assert_eq!(
            normalize_vi_search_key("  TRỨNG   gà LUỘC "),
            "trứng gà luộc"
        );
    }
}
