pub mod dictionary;
pub mod format;

pub use dictionary::{
    handle_learn_command, handle_learn_command_opts, handle_unlearn_command,
    handle_unlearn_command_internal, LearningDictionary,
};
pub use format::{capitalize_sentence, clean_hallucinations, TextFormatter};

/// Unified text processing pipeline coordinating hallucination cleaning,
/// dictionary vocabulary replacement, and typing ergonomics.
#[derive(Debug, Clone, Default)]
pub struct TextPipeline {
    pub formatter: TextFormatter,
    pub dictionary: LearningDictionary,
}

impl TextPipeline {
    pub fn new(formatter: TextFormatter, dictionary: LearningDictionary) -> Self {
        Self {
            formatter,
            dictionary,
        }
    }

    /// Processes a raw model transcript, returning `None` if hallucination cleaning
    /// discards it as pure noise.
    pub fn process(&mut self, raw_text: &str) -> Option<String> {
        // Automatically refresh dictionary rules if updated on disk by CLI commands
        self.dictionary.reload_if_modified();

        let cleaned = if self.formatter.clean_hallucinations {
            clean_hallucinations(raw_text)?
        } else {
            let trimmed = raw_text.trim();
            if trimmed.is_empty() {
                return None;
            }
            trimmed.to_string()
        };

        // 1. Dictionary replacement pass
        let replaced = self.dictionary.apply(&cleaned);

        // 2. Sentence casing and smart spacing pass
        self.formatter.format_cased_and_spaced(&replaced)
    }

    pub fn reset(&mut self) {
        self.formatter.reset();
    }
}
