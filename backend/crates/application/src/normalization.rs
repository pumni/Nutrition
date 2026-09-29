use crate::ApplicationError;
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

/// Ensures every parsed food modifier is represented in the exact food phrase.
///
/// This deliberately uses only normalized token-sequence containment. A modifier that is not
/// represented by the phrase cannot be proven safe for a generic exact identity and must fail
/// closed. Quantity and portion units are not part of this check.
///
/// # Errors
///
/// Returns [`ApplicationError::InsufficientEvidence`] when a modifier is empty or not represented
/// by the food phrase.
pub fn ensure_modifiers_represented(
    food_phrase: &str,
    modifiers: &[String],
) -> Result<(), ApplicationError> {
    let normalized_food_phrase = normalize_vi_search_key(food_phrase);
    if modifiers.iter().all(|modifier| {
        let normalized_modifier = normalize_vi_search_key(modifier);
        contains_token_sequence(&normalized_food_phrase, &normalized_modifier)
    }) {
        Ok(())
    } else {
        Err(ApplicationError::InsufficientEvidence(
            "food specificity is not represented in the exact food phrase".to_owned(),
        ))
    }
}

fn contains_token_sequence(phrase: &str, sequence: &str) -> bool {
    let phrase_tokens = phrase.split_whitespace().collect::<Vec<_>>();
    let sequence_tokens = sequence.split_whitespace().collect::<Vec<_>>();
    !sequence_tokens.is_empty()
        && phrase_tokens
            .windows(sequence_tokens.len())
            .any(|window| window == sequence_tokens)
}

#[cfg(test)]
mod tests {
    use super::{ensure_modifiers_represented, normalize_vi_search_key};
    use crate::ApplicationError;

    fn validate(food_phrase: &str, modifiers: &[&str]) -> Result<(), ApplicationError> {
        let modifiers = modifiers
            .iter()
            .map(|modifier| (*modifier).to_owned())
            .collect::<Vec<_>>();
        ensure_modifiers_represented(food_phrase, &modifiers)
    }

    #[test]
    fn preserves_diacritics_and_collapses_whitespace() {
        assert_eq!(
            normalize_vi_search_key("  TRỨNG   gà LUỘC "),
            "trứng gà luộc"
        );
    }

    #[test]
    fn allows_modifiers_already_represented_in_the_exact_food_phrase() {
        assert!(validate("trứng gà luộc", &["luộc"]).is_ok());
    }

    #[test]
    fn allows_generic_food_phrase_without_additional_specificity() {
        assert!(validate("cơm trắng", &[]).is_ok());
    }

    #[test]
    fn rejects_unrepresented_cultivar_and_preparation_specificity() {
        for modifier in [
            "jasmine",
            "ST25",
            "nếp",
            "glutinous",
            "gạo lứt",
            "brown rice",
            "rang",
            "chiên",
            "fried",
            "thêm dầu",
            "thêm mỡ",
        ] {
            assert!(
                matches!(
                    validate("cơm trắng", &[modifier]),
                    Err(ApplicationError::InsufficientEvidence(_))
                ),
                "modifier {modifier:?} must not collapse to generic cơm trắng"
            );
        }
    }

    #[test]
    fn rejects_empty_or_only_partially_matching_modifiers() {
        assert!(matches!(
            validate("cơm trắng", &["  "]),
            Err(ApplicationError::InsufficientEvidence(_))
        ));
        assert!(matches!(
            validate("cơm trắng", &["ST2"]),
            Err(ApplicationError::InsufficientEvidence(_))
        ));
    }
}
