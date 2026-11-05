use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const DEFAULT_REPLACEMENTS: &[(&str, &str)] = &[
    ("speechy", "speechyyy"),
    ("speechi", "speechyyy"),
    ("spichi", "speechyyy"),
];

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DictionaryFile {
    #[serde(default)]
    pub replacements: HashMap<String, String>,
}

/// Dynamic dictionary performing case-insensitive, word-boundary-aware substitutions.
#[derive(Debug, Clone, Default)]
pub struct LearningDictionary {
    // Stored as (pattern_lowercase, original_pattern, replacement), sorted descending by pattern length
    rules: Vec<(String, String, String)>,
    pub path: Option<PathBuf>,
    pub last_modified: Option<std::time::SystemTime>,
}

impl LearningDictionary {
    pub fn new() -> Self {
        Self {
            rules: Vec::new(),
            path: None,
            last_modified: None,
        }
    }

    pub fn default_path() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        PathBuf::from(home)
            .join(".config")
            .join("speechyyy")
            .join("dictionary.toml")
    }

    pub fn load() -> Self {
        let path = Self::default_path();
        Self::load_from_path(&path).unwrap_or_else(|_| Self {
            rules: Vec::new(),
            path: Some(path),
            last_modified: None,
        })
    }

    pub fn load_from_path(path: &Path) -> Result<Self, String> {
        if !path.exists() {
            return Ok(Self {
                rules: Vec::new(),
                path: Some(path.to_path_buf()),
                last_modified: None,
            });
        }

        let mtime = fs::metadata(path).and_then(|m| m.modified()).ok();
        let content = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read dictionary from {}: {}", path.display(), e))?;

        let file: DictionaryFile = toml::from_str(&content)
            .map_err(|e| format!("Failed to parse dictionary TOML {}: {}", path.display(), e))?;

        let mut dict = Self::from_file(file);
        dict.path = Some(path.to_path_buf());
        dict.last_modified = mtime;
        Ok(dict)
    }

    pub fn from_file(file: DictionaryFile) -> Self {
        let mut rules: Vec<(String, String, String)> = file
            .replacements
            .into_iter()
            .map(|(k, v)| (k.to_lowercase(), k, v))
            .collect();

        // Sort descending by pattern length to match longer phrases first
        rules.sort_by_key(|a| std::cmp::Reverse(a.0.len()));

        Self {
            rules,
            path: None,
            last_modified: None,
        }
    }

    /// Reloads the dictionary from disk if the file has been modified since it was loaded.
    pub fn reload_if_modified(&mut self) {
        if let Some(ref path) = self.path {
            if let Ok(metadata) = fs::metadata(path) {
                if let Ok(modified) = metadata.modified() {
                    if self.last_modified != Some(modified) {
                        if let Ok(loaded) = Self::load_from_path(path) {
                            self.rules = loaded.rules;
                            self.last_modified = Some(modified);
                        }
                    }
                }
            }
        }
    }

    pub fn save(&self) -> Result<(), String> {
        self.save_to_path(&Self::default_path())
    }

    pub fn save_to_path(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                format!(
                    "Failed to create dictionary directory {}: {}",
                    parent.display(),
                    e
                )
            })?;
        }

        let mut replacements = HashMap::new();
        for (_, orig_k, v) in &self.rules {
            replacements.insert(orig_k.clone(), v.clone());
        }

        let file = DictionaryFile { replacements };
        let content = toml::to_string_pretty(&file)
            .map_err(|e| format!("Failed to serialize dictionary TOML: {}", e))?;

        fs::write(path, content)
            .map_err(|e| format!("Failed to write dictionary to {}: {}", path.display(), e))?;

        Ok(())
    }

    /// Adds or updates a replacement rule and keeps rules sorted by length.
    pub fn add_rule(&mut self, wrong: &str, correct: &str) {
        let wrong_trimmed = wrong.trim();
        let correct_trimmed = correct.trim();

        if wrong_trimmed.is_empty() || correct_trimmed.is_empty() {
            return;
        }

        let wrong_lower = wrong_trimmed.to_lowercase();

        // Remove existing rule if present
        self.rules.retain(|(k, _, _)| k != &wrong_lower);

        self.rules.push((
            wrong_lower,
            wrong_trimmed.to_string(),
            correct_trimmed.to_string(),
        ));
        self.rules.sort_by_key(|a| std::cmp::Reverse(a.0.len()));
    }

    /// Removes a replacement rule by key (case-insensitive) and returns true if found.
    pub fn remove_rule(&mut self, key: &str) -> bool {
        let key_trimmed = key.trim();
        if key_trimmed.is_empty() {
            return false;
        }
        let key_lower = key_trimmed.to_lowercase();
        let initial_len = self.rules.len();
        self.rules.retain(|(k, orig_k, _)| k != &key_lower && orig_k != key_trimmed);
        self.rules.len() < initial_len
    }

    /// Removes a replacement rule and persists changes to disk.
    pub fn remove_rule_and_save(&mut self, key: &str) -> Result<bool, String> {
        let removed = self.remove_rule(key);
        if removed {
            if let Some(ref path) = self.path {
                self.save_to_path(path)?;
            } else {
                self.save()?;
            }
        }
        Ok(removed)
    }

    /// Adds or updates a replacement rule and persists it immediately.
    pub fn add_rule_and_save(&mut self, wrong: &str, correct: &str) -> Result<(), String> {
        self.add_rule(wrong, correct);
        if let Some(ref path) = self.path {
            self.save_to_path(path)
        } else {
            self.save()
        }
    }

    /// Number of active replacement rules.
    pub fn len(&self) -> usize {
        self.rules.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Applies all replacement rules against `text` using case-insensitive word boundary checks.
    pub fn apply(&self, text: &str) -> String {
        let mut current = text.to_string();

        for (pattern_lower, _, replacement) in &self.rules {
            current = replace_word_boundary(&current, pattern_lower, replacement);
        }

        for &(pattern, replacement) in DEFAULT_REPLACEMENTS {
            if !self.rules.iter().any(|(k, _, _)| k == pattern) {
                current = replace_word_boundary_with_case(&current, pattern, replacement, true);
            }
        }

        current
    }
}

/// Handler for CLI `learn` command.
pub fn handle_learn_command(wrong: &str, correct: &str) -> Result<(), String> {
    let mut dict = LearningDictionary::load();
    dict.add_rule_and_save(wrong, correct)?;
    println!("Learned replacement: \"{}\" -> \"{}\"", wrong, correct);
    Ok(())
}

/// Handler for CLI `learn` command with optional arguments.
pub fn handle_learn_command_opts(
    wrong: Option<&str>,
    correct: Option<&str>,
) -> Result<(), String> {
    match (wrong, correct) {
        (Some(w), Some(c)) if !w.trim().is_empty() && !c.trim().is_empty() => {
            handle_learn_command(w.trim(), c.trim())
        }
        _ => {
            println!("Usage: speechyyy learn <WRONG> <CORRECT>\nExample: speechyyy learn 'n8n' 'N8N'");
            Ok(())
        }
    }
}

/// Handler for CLI `unlearn` command with path override for testing.
pub fn handle_unlearn_command_internal(
    key: Option<&str>,
    dict_path: Option<&Path>,
) -> Result<(), String> {
    let key_str = match key {
        Some(k) if !k.trim().is_empty() => k.trim(),
        _ => {
            println!("Usage: speechyyy unlearn <KEY>\nExample: speechyyy unlearn 'Speechy kya tum mujhe sun sakte ho'");
            return Ok(());
        }
    };

    let mut dict = match dict_path {
        Some(p) => LearningDictionary::load_from_path(p)?,
        None => LearningDictionary::load(),
    };

    if dict.remove_rule(key_str) {
        if let Some(p) = dict_path {
            dict.save_to_path(p)?;
        } else {
            dict.save()?;
        }
        println!("[✓] Removed replacement rule: \"{}\"", key_str);
    } else {
        println!("[!] Rule \"{}\" not found in dictionary", key_str);
    }

    Ok(())
}

/// Handler for CLI `unlearn` command.
pub fn handle_unlearn_command(key: Option<&str>) -> Result<(), String> {
    handle_unlearn_command_internal(key, None)
}

/// Adapts replacement casing based on the matched original token.
fn adapt_casing(matched: &str, replacement: &str) -> String {
    if matched.chars().all(|c| !c.is_alphabetic() || c.is_uppercase()) {
        return replacement.to_uppercase();
    }
    let mut chars = matched.chars();
    if let Some(first) = chars.next() {
        if first.is_uppercase() {
            let mut rep_chars = replacement.chars();
            if let Some(rep_first) = rep_chars.next() {
                let mut res = rep_first.to_uppercase().to_string();
                res.extend(rep_chars);
                return res;
            }
        }
    }
    replacement.to_string()
}

/// Replaces whole-word occurrences of `pattern_lower` in `text` with `replacement` (case-insensitive).
fn replace_word_boundary(text: &str, pattern_lower: &str, replacement: &str) -> String {
    replace_word_boundary_with_case(text, pattern_lower, replacement, false)
}

/// Replaces whole-word occurrences with optional casing adaptation.
fn replace_word_boundary_with_case(
    text: &str,
    pattern_lower: &str,
    replacement: &str,
    preserve_case: bool,
) -> String {
    if pattern_lower.is_empty() || text.is_empty() {
        return text.to_string();
    }

    let text_lower = text.to_lowercase();
    let pattern_len = pattern_lower.len();
    let mut result = String::with_capacity(text.len());
    let mut cursor = 0;

    while let Some(rel_idx) = text_lower[cursor..].find(pattern_lower) {
        let match_start = cursor + rel_idx;
        let match_end = match_start + pattern_len;

        // Check word boundaries
        let is_start_boundary = if match_start == 0 {
            true
        } else {
            let prev_char = text[..match_start].chars().last().unwrap();
            !prev_char.is_alphanumeric() && prev_char != '_'
        };

        let is_end_boundary = if match_end >= text.len() {
            true
        } else {
            let next_char = text[match_end..].chars().next().unwrap();
            !next_char.is_alphanumeric() && next_char != '_'
        };

        if is_start_boundary && is_end_boundary {
            result.push_str(&text[cursor..match_start]);
            let rep = if preserve_case {
                adapt_casing(&text[match_start..match_end], replacement)
            } else {
                replacement.to_string()
            };
            result.push_str(&rep);
            cursor = match_end;
        } else {
            let next_cursor = text[match_start..]
                .char_indices()
                .nth(1)
                .map(|(offset, _)| match_start + offset)
                .unwrap_or(text.len());
            result.push_str(&text[cursor..next_cursor]);
            cursor = next_cursor;
        }
    }

    result.push_str(&text[cursor..]);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dictionary_word_boundary_replacement() {
        let mut dict = LearningDictionary::new();
        dict.add_rule("post gress", "PostgreSQL");
        dict.add_rule("wispr flow", "wisprflow");

        let input = "I am using post gress and wispr flow today";
        assert_eq!(
            dict.apply(input),
            "I am using PostgreSQL and wisprflow today"
        );
    }

    #[test]
    fn test_dictionary_case_insensitivity() {
        let mut dict = LearningDictionary::new();
        dict.add_rule("post gress", "PostgreSQL");

        assert_eq!(dict.apply("POST GRESS is great"), "PostgreSQL is great");
        assert_eq!(dict.apply("Post Gress is great"), "PostgreSQL is great");
    }

    #[test]
    fn test_dictionary_substring_protection() {
        let mut dict = LearningDictionary::new();
        dict.add_rule("js", "JavaScript");

        // "js" should be replaced, but NOT inside "json" or "nodejs"
        let input = "Write js code with json parser in nodejs";
        assert_eq!(
            dict.apply(input),
            "Write JavaScript code with json parser in nodejs"
        );
    }

    #[test]
    fn test_dictionary_longer_phrase_precedence() {
        let mut dict = LearningDictionary::new();
        dict.add_rule("type script", "TypeScript");
        dict.add_rule("type script language", "the TypeScript language");

        let input = "I love type script language";
        assert_eq!(dict.apply(input), "I love the TypeScript language");
    }

    #[test]
    fn test_dictionary_save_and_load_roundtrip() {
        let temp_dir = std::env::temp_dir().join(format!("dict_test_{}", std::process::id()));
        let dict_path = temp_dir.join("dictionary.toml");

        let mut dict = LearningDictionary::new();
        dict.add_rule("post gress", "PostgreSQL");
        dict.add_rule("react native", "React Native");

        dict.save_to_path(&dict_path).unwrap();

        let loaded = LearningDictionary::load_from_path(&dict_path).unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(
            loaded.apply("Running post gress and react native"),
            "Running PostgreSQL and React Native"
        );

        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_dictionary_reload_if_modified() {
        let temp_dir =
            std::env::temp_dir().join(format!("dict_reload_test_{}", std::process::id()));
        let dict_path = temp_dir.join("dictionary.toml");

        let mut dict = LearningDictionary::new();
        dict.add_rule("wispr flow", "WisprFlow");
        dict.save_to_path(&dict_path).unwrap();

        let mut loaded = LearningDictionary::load_from_path(&dict_path).unwrap();
        assert_eq!(loaded.apply("testing wispr flow"), "testing WisprFlow");

        // Small delay to ensure mtime timestamp tick on all filesystems
        std::thread::sleep(std::time::Duration::from_millis(50));

        let mut updater = LearningDictionary::load_from_path(&dict_path).unwrap();
        updater
            .add_rule_and_save("post gress", "PostgreSQL")
            .unwrap();

        // Check that reload_if_modified picks up the new rule
        loaded.reload_if_modified();
        assert_eq!(
            loaded.apply("testing post gress and wispr flow"),
            "testing PostgreSQL and WisprFlow"
        );

        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_dictionary_remove_rule_and_unlearn() {
        let temp_dir =
            std::env::temp_dir().join(format!("dict_unlearn_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        let dict_path = temp_dir.join("dictionary.toml");

        let mut dict = LearningDictionary::load_from_path(&dict_path).unwrap();
        dict.add_rule_and_save("n8n", "N8N").unwrap();
        dict.add_rule_and_save("Speechy", "Speechyyy").unwrap();
        assert_eq!(dict.len(), 2);

        // Remove rule via unlearn command
        let res = handle_unlearn_command_internal(Some("n8n"), Some(&dict_path));
        assert!(res.is_ok());

        let reloaded = LearningDictionary::load_from_path(&dict_path).unwrap();
        assert_eq!(reloaded.len(), 1);
        assert_eq!(reloaded.apply("testing n8n"), "testing n8n");
        assert_eq!(reloaded.apply("testing Speechy"), "testing Speechyyy");

        // Remove again -> not found (prints warning, succeeds)
        let res_missing = handle_unlearn_command_internal(Some("n8n"), Some(&dict_path));
        assert!(res_missing.is_ok());

        // Missing argument -> prints usage
        let res_bare = handle_unlearn_command_internal(None, Some(&dict_path));
        assert!(res_bare.is_ok());

        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_handle_learn_command_opts() {
        let res_bare = handle_learn_command_opts(None, None);
        assert!(res_bare.is_ok());
        let res_one = handle_learn_command_opts(Some("wrong"), None);
        assert!(res_one.is_ok());
    }

    #[test]
    fn test_default_speechyyy_replacements() {
        let dict = LearningDictionary::new();

        // Exact test case required by spec:
        let input = "Hey speechi, test speechy spichi";
        assert_eq!(
            dict.apply(input),
            "Hey speechyyy, test speechyyy speechyyy"
        );

        // Case-insensitive adaptation (e.g. Speechi -> Speechyyy)
        assert_eq!(dict.apply("Speechi is ready"), "Speechyyy is ready");
        assert_eq!(dict.apply("I love Speechy"), "I love Speechyyy");
        assert_eq!(dict.apply("Listen Spichi"), "Listen Speechyyy");
        assert_eq!(dict.apply("ALL CAPS SPEECHI AND SPEECHY"), "ALL CAPS SPEECHYYY AND SPEECHYYY");

        // Substring boundary protection - should NOT replace inside other words
        assert_eq!(dict.apply("He is speechless"), "He is speechless");
        assert_eq!(dict.apply("the speechiest speaker"), "the speechiest speaker");
        assert_eq!(dict.apply("conspichi test"), "conspichi test");
    }

    #[test]
    fn test_text_pipeline_speechyyy_normalization() {
        let mut pipeline = crate::text::TextPipeline::new(
            crate::text::TextFormatter::default(),
            LearningDictionary::new(),
        );
        let res = pipeline.process("Hey speechi, test speechy spichi").unwrap();
        assert_eq!(res, "Hey speechyyy, test speechyyy speechyyy");

        let mut pipeline2 = crate::text::TextPipeline::new(
            crate::text::TextFormatter::default(),
            LearningDictionary::new(),
        );
        let res2 = pipeline2.process("speechi is active").unwrap();
        assert_eq!(res2, "Speechyyy is active");
    }
}
