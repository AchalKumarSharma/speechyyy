//! Text formatting, sentence casing, smart spacing, and hallucination cleaner.

const KNOWN_HALLUCINATIONS: &[&str] = &[
    "blank_audio",
    "blank",
    "music",
    "applause",
    "silence",
    "laughter",
    "cheering",
    "cough",
    "coughing",
    "sigh",
    "snort",
    "throat clearing",
    "whisper",
    "groan",
    "chuckle",
    "gasp",
    "pant",
    "sound",
    "noise",
    "talking",
    "ambient",
];

const ACOUSTIC_KEYWORDS: &[&str] = &[
    "blank", "music", "applause", "silence", "laughter", "cheer", "cough", "sigh", "snort",
    "whisper", "groan", "throat", "talking", "ambient", "noise", "sound", "chuckle", "gasp",
    "pant", "audio",
];

fn is_hallucination_tag(opening: char, tag: &str) -> bool {
    let trimmed = tag.trim();
    if trimmed.is_empty() {
        return true;
    }

    // Asterisk-wrapped text like *applause*, *laughter*, *cough* are acoustic actions
    if opening == '*' {
        return trimmed
            .chars()
            .all(|c| c.is_alphabetic() || c.is_whitespace() || c == '_' || c == '-');
    }

    let tag_norm = trimmed.to_lowercase().replace('-', "_");

    // Match exact known hallucinations
    if KNOWN_HALLUCINATIONS.iter().any(|&h| tag_norm == h) {
        return true;
    }

    // Acoustic event keywords (e.g. [silence_extended], (background talking))
    if ACOUSTIC_KEYWORDS.iter().any(|&kw| tag_norm.contains(kw)) {
        return true;
    }

    // Uppercase special tokens with underscore like [BLANK_AUDIO], [SILENCE_EXTENDED]
    if trimmed.len() >= 3
        && trimmed.contains('_')
        && trimmed
            .chars()
            .all(|c| c.is_uppercase() || c == '_' || c.is_whitespace())
    {
        return true;
    }

    false
}

/// Strips hallucinated non-speech bracketed tokens such as `[BLANK_AUDIO]`, `(music)`, `*applause*`.
/// Returns `None` if the text becomes empty after stripping.
pub fn clean_hallucinations(text: &str) -> Option<String> {
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '[' || ch == '(' || ch == '*' {
            let closing = match ch {
                '[' => ']',
                '(' => ')',
                '*' => '*',
                _ => unreachable!(),
            };

            let mut tag = String::new();
            let mut closed = false;

            for inner in chars.by_ref() {
                if inner == closing {
                    closed = true;
                    break;
                }
                tag.push(inner);
            }

            if closed {
                if !is_hallucination_tag(ch, &tag) {
                    // Not a recognized hallucination tag; keep it
                    result.push(ch);
                    result.push_str(&tag);
                    result.push(closing);
                }
            } else {
                // Not closed; keep original characters
                result.push(ch);
                result.push_str(&tag);
            }
        } else {
            result.push(ch);
        }
    }

    // Collapse multiple spaces and trim
    let mut cleaned = String::with_capacity(result.len());
    let mut last_was_space = false;
    for ch in result.trim().chars() {
        if ch.is_whitespace() {
            if !last_was_space {
                cleaned.push(' ');
                last_was_space = true;
            }
        } else {
            cleaned.push(ch);
            last_was_space = false;
        }
    }

    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

/// Capitalizes the first character of the text if it is an alphabetic letter.
pub fn capitalize_sentence(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut leading_ws_done = false;

    for ch in text.chars() {
        if !leading_ws_done {
            if ch.is_whitespace() {
                result.push(ch);
                continue;
            }
            leading_ws_done = true;
            if ch.is_alphabetic() {
                result.extend(ch.to_uppercase());
            } else {
                result.push(ch);
            }
        } else {
            result.push(ch);
        }
    }

    result
}

/// Stateful text formatter that applies hallucination cleaning, sentence casing,
/// and smart spacing between consecutive dictations.
#[derive(Debug, Clone)]
pub struct TextFormatter {
    pub smart_spacing: bool,
    pub sentence_casing: bool,
    pub clean_hallucinations: bool,
    pub last_char: Option<char>,
}

impl Default for TextFormatter {
    fn default() -> Self {
        Self {
            smart_spacing: true,
            sentence_casing: true,
            clean_hallucinations: true,
            last_char: None,
        }
    }
}

impl TextFormatter {
    pub fn new(smart_spacing: bool, sentence_casing: bool, clean_hallucinations: bool) -> Self {
        Self {
            smart_spacing,
            sentence_casing,
            clean_hallucinations,
            last_char: None,
        }
    }

    /// Resets the inter-utterance spacing context.
    pub fn reset(&mut self) {
        self.last_char = None;
    }

    /// Formats an already-cleaned transcript by applying sentence casing and smart spacing.
    pub fn format_cased_and_spaced(&mut self, text: &str) -> Option<String> {
        let mut text = text.trim().to_string();
        if text.is_empty() {
            return None;
        }

        if self.sentence_casing {
            text = capitalize_sentence(&text);
        }

        if self.smart_spacing {
            if let Some(prev) = self.last_char {
                let first = text.chars().next().unwrap_or(' ');
                // Punctuation after a clause/sentence triggers spacing before a word
                let prev_triggers_space =
                    prev.is_alphanumeric() || matches!(prev, '.' | ',' | '!' | '?' | ';' | ':');
                let first_needs_space = first.is_alphanumeric();
                if prev_triggers_space && first_needs_space {
                    text = format!(" {}", text);
                }
            }
        }

        self.last_char = text.chars().last();
        Some(text)
    }

    /// Formats an incoming transcript from inference, returning `None` if hallucination
    /// cleaning leaves the text completely empty.
    pub fn format(&mut self, text: &str) -> Option<String> {
        let cleaned = if self.clean_hallucinations {
            clean_hallucinations(text)?
        } else {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return None;
            }
            trimmed.to_string()
        };

        self.format_cased_and_spaced(&cleaned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_hallucinations_pure_noise_returns_none() {
        assert_eq!(clean_hallucinations("[BLANK_AUDIO]"), None);
        assert_eq!(clean_hallucinations("(music)"), None);
        assert_eq!(clean_hallucinations("*applause*"), None);
        assert_eq!(clean_hallucinations("  [silence]   "), None);
        assert_eq!(clean_hallucinations("[BLANK_AUDIO] (music)"), None);
        assert_eq!(clean_hallucinations("[silence_extended]"), None);
        assert_eq!(clean_hallucinations("(background talking)"), None);
        assert_eq!(clean_hallucinations("*sigh*"), None);
    }

    #[test]
    fn test_clean_hallucinations_mixed_text() {
        assert_eq!(
            clean_hallucinations("[BLANK_AUDIO] Hello world (music)"),
            Some("Hello world".to_string())
        );
        assert_eq!(
            clean_hallucinations("Here is the speech [applause] in session"),
            Some("Here is the speech in session".to_string())
        );
    }

    #[test]
    fn test_clean_hallucinations_preserves_legitimate_brackets() {
        assert_eq!(
            clean_hallucinations("[TODO] implement feature"),
            Some("[TODO] implement feature".to_string())
        );
    }

    #[test]
    fn test_capitalize_sentence() {
        assert_eq!(capitalize_sentence("hello world"), "Hello world");
        assert_eq!(capitalize_sentence("  hello"), "  Hello");
        assert_eq!(
            capitalize_sentence("Already capitalized"),
            "Already capitalized"
        );
        assert_eq!(capitalize_sentence("123 test"), "123 test");
    }

    #[test]
    fn test_smart_spacing_consecutive_words() {
        let mut formatter = TextFormatter::default();

        let first = formatter.format("hello").unwrap();
        assert_eq!(first, "Hello");

        let second = formatter.format("world").unwrap();
        assert_eq!(second, " World");

        let third = formatter.format("again").unwrap();
        assert_eq!(third, " Again");
    }

    #[test]
    fn test_smart_spacing_leading_punctuation() {
        let mut formatter = TextFormatter::default();

        let first = formatter.format("hello").unwrap();
        assert_eq!(first, "Hello");

        let comma = formatter.format(", how are you").unwrap();
        assert_eq!(comma, ", how are you");
    }

    #[test]
    fn test_smart_spacing_after_punctuation() {
        let mut formatter = TextFormatter::default();

        let first = formatter.format("hello.").unwrap();
        assert_eq!(first, "Hello.");

        let second = formatter.format("how are you").unwrap();
        assert_eq!(second, " How are you");
    }

    #[test]
    fn test_formatter_reset_clears_spacing() {
        let mut formatter = TextFormatter::default();

        let first = formatter.format("hello").unwrap();
        assert_eq!(first, "Hello");

        formatter.reset();

        let second = formatter.format("world").unwrap();
        assert_eq!(second, "World"); // No prepended space
    }
}
