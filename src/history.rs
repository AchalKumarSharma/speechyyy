use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HistoryRecord {
    pub timestamp: String,
    pub raw_text: String,
    pub final_text: String,
    pub provider: String,
    pub duration_secs: f32,
}

#[derive(Debug, Clone)]
pub struct HistoryLogger {
    pub path: PathBuf,
}

impl Default for HistoryLogger {
    fn default() -> Self {
        Self::new()
    }
}

impl HistoryLogger {
    pub fn default_path() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("speechyyy")
            .join("history.jsonl")
    }

    pub fn new() -> Self {
        Self {
            path: Self::default_path(),
        }
    }

    pub fn with_path(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn append(&self, record: &HistoryRecord) -> Result<(), std::io::Error> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;

        let line = serde_json::to_string(record)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;

        writeln!(file, "{}", line)?;
        Ok(())
    }

    pub fn get_recent_entries(&self, limit: usize) -> Result<Vec<HistoryRecord>, std::io::Error> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(&self.path)?;
        let mut records = Vec::new();

        for line in content.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                if let Ok(record) = serde_json::from_str::<HistoryRecord>(trimmed) {
                    records.push(record);
                }
            }
        }

        if records.len() > limit {
            Ok(records.split_off(records.len() - limit))
        } else {
            Ok(records)
        }
    }

    pub fn get_last_entry(&self) -> Result<Option<HistoryRecord>, std::io::Error> {
        let recent = self.get_recent_entries(1)?;
        Ok(recent.into_iter().next())
    }

    /// Records a finalized dictation into the history log.
    pub fn log(
        &self,
        raw_text: &str,
        final_text: &str,
        provider: &str,
        duration_secs: f32,
    ) -> Result<(), std::io::Error> {
        let record = HistoryRecord {
            timestamp: current_iso_timestamp(),
            raw_text: raw_text.to_string(),
            final_text: final_text.to_string(),
            provider: provider.to_string(),
            duration_secs,
        };
        self.append(&record)
    }
}

/// Generates an ISO 8601 UTC timestamp string without external time dependencies.
pub fn current_iso_timestamp() -> String {
    let now = std::time::SystemTime::now();
    let duration = now
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let total_secs = duration.as_secs();

    let mut days = (total_secs / 86400) as i64;
    let day_secs = (total_secs % 86400) as u32;

    let hour = day_secs / 3600;
    let min = (day_secs % 3600) / 60;
    let sec = day_secs % 60;

    days += 719468;
    let era = if days >= 0 { days } else { days - 146096 } / 146097;
    let doe = (days - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };

    format!("{year:04}-{m:02}-{d:02}T{hour:02}:{min:02}:{sec:02}Z")
}

/// Computes the word-level diff between `original` and `corrected`, extracting the
/// differing segment as `(wrong, correct)`.
pub fn extract_correction(original: &str, corrected: &str) -> Option<(String, String)> {
    let orig = original.trim();
    let corr = corrected.trim();

    if orig.is_empty() || corr.is_empty() || orig == corr {
        return None;
    }

    let orig_words: Vec<&str> = orig.split_whitespace().collect();
    let corr_words: Vec<&str> = corr.split_whitespace().collect();

    // 1. Common prefix length
    let mut prefix_len = 0;
    while prefix_len < orig_words.len()
        && prefix_len < corr_words.len()
        && orig_words[prefix_len].eq_ignore_ascii_case(corr_words[prefix_len])
    {
        prefix_len += 1;
    }

    // 2. Common suffix length
    let mut orig_end = orig_words.len();
    let mut corr_end = corr_words.len();

    while orig_end > prefix_len
        && corr_end > prefix_len
        && orig_words[orig_end - 1].eq_ignore_ascii_case(corr_words[corr_end - 1])
    {
        orig_end -= 1;
        corr_end -= 1;
    }

    let wrong = orig_words[prefix_len..orig_end].join(" ");
    let correct = corr_words[prefix_len..corr_end].join(" ");

    let wrong_trimmed = wrong
        .trim_matches(|c: char| c.is_ascii_punctuation() || c.is_whitespace())
        .to_string();
    let correct_trimmed = correct
        .trim_matches(|c: char| c.is_ascii_punctuation() || c.is_whitespace())
        .to_string();

    if wrong_trimmed.is_empty()
        || correct_trimmed.is_empty()
        || wrong_trimmed.eq_ignore_ascii_case(&correct_trimmed)
    {
        None
    } else {
        Some((wrong_trimmed, correct_trimmed))
    }
}

/// Handler for CLI `history` command.
pub fn handle_history_command(limit: usize) -> Result<(), String> {
    let logger = HistoryLogger::new();
    let records = logger
        .get_recent_entries(limit)
        .map_err(|e| format!("Error reading history: {}", e))?;
    if records.is_empty() {
        println!("No transcription history found.");
    } else {
        println!(
            "=== Recent Speechyyy Utterances (last {}) ===",
            records.len()
        );
        for (i, rec) in records.iter().enumerate() {
            println!(
                "[{}] {} ({:.2}s, provider: {})",
                i + 1,
                rec.timestamp,
                rec.duration_secs,
                rec.provider
            );
            println!("     Final: \"{}\"", rec.final_text);
            if rec.raw_text != rec.final_text {
                println!("     Raw:   \"{}\"", rec.raw_text);
            }
        }
    }
    Ok(())
}

/// Handler for CLI `fix` command.
pub fn handle_fix_command(corrected_phrase: &str) -> Result<(), String> {
    let logger = HistoryLogger::new();
    match logger
        .get_last_entry()
        .map_err(|e| format!("Error reading history: {}", e))?
    {
        Some(last) => match extract_correction(&last.final_text, corrected_phrase) {
            Some((wrong, correct)) => {
                let mut dict = crate::text::LearningDictionary::load();
                dict.add_rule_and_save(&wrong, &correct)?;
                println!("Previous:  \"{}\"", last.final_text);
                println!("Corrected: \"{}\"", corrected_phrase);
                println!("Learned replacement: \"{}\" -> \"{}\"", wrong, correct);
                Ok(())
            }
            None => {
                println!("No difference detected between previous utterance and corrected phrase.");
                Ok(())
            }
        },
        None => Err("No previous dictation history found to fix.".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_history_append_and_read_recent() {
        let temp_dir = std::env::temp_dir().join(format!("hist_test_{}", std::process::id()));
        let hist_path = temp_dir.join("history.jsonl");
        let logger = HistoryLogger::with_path(hist_path);

        assert_eq!(logger.get_last_entry().unwrap(), None);

        let rec1 = HistoryRecord {
            timestamp: "2026-10-30T23:00:00Z".into(),
            raw_text: "hello world".into(),
            final_text: "Hello world".into(),
            provider: "mock".into(),
            duration_secs: 1.0,
        };

        let rec2 = HistoryRecord {
            timestamp: "2026-10-30T23:01:00Z".into(),
            raw_text: "post gress".into(),
            final_text: "PostgreSQL".into(),
            provider: "mock".into(),
            duration_secs: 1.2,
        };

        logger.append(&rec1).unwrap();
        logger.append(&rec2).unwrap();

        let last = logger.get_last_entry().unwrap();
        assert_eq!(last, Some(rec2.clone()));

        let recent = logger.get_recent_entries(10).unwrap();
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0], rec1);
        assert_eq!(recent[1], rec2);

        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_extract_correction_multi_word() {
        let orig = "I use post gress in production";
        let corr = "I use PostgreSQL in production";

        let extracted = extract_correction(orig, corr);
        assert_eq!(
            extracted,
            Some(("post gress".to_string(), "PostgreSQL".to_string()))
        );
    }

    #[test]
    fn test_extract_correction_single_word() {
        let orig = "Let us test wispr now";
        let corr = "Let us test WisprFlow now";

        let extracted = extract_correction(orig, corr);
        assert_eq!(
            extracted,
            Some(("wispr".to_string(), "WisprFlow".to_string()))
        );
    }

    #[test]
    fn test_extract_correction_identical_returns_none() {
        assert_eq!(extract_correction("same text", "same text"), None);
        assert_eq!(extract_correction("   ", "   "), None);
    }

    #[test]
    fn test_extract_correction_trims_punctuation() {
        let orig = "I use post gress.";
        let corr = "I use PostgreSQL.";
        assert_eq!(
            extract_correction(orig, corr),
            Some(("post gress".to_string(), "PostgreSQL".to_string()))
        );

        let orig_comma = "We like wispr flow, absolutely";
        let corr_comma = "We like WisprFlow, absolutely";
        assert_eq!(
            extract_correction(orig_comma, corr_comma),
            Some(("wispr flow".to_string(), "WisprFlow".to_string()))
        );
    }

    #[test]
    fn test_current_iso_timestamp_format() {
        let ts = current_iso_timestamp();
        assert_eq!(ts.len(), 20); // YYYY-MM-DDTHH:MM:SSZ
        assert!(ts.ends_with('Z'));
        assert!(ts.contains('T'));
    }
}
