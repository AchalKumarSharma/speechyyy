use crate::audio::vad::EnergyVad;
use crate::audio::AudioCapture;
use crate::history::HistoryLogger;
use crate::inference::SpeechInference;
use crate::text::TextPipeline;
use evdev::KeyCode;
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Modifier {
    Ctrl,
    Alt,
    Shift,
    Super,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetChord {
    SingleKey(KeyCode),
    Compound {
        modifiers: Vec<Modifier>,
        trigger_key: KeyCode,
    },
}

impl TargetChord {
    pub fn right_alt() -> Self {
        Self::SingleKey(KeyCode::KEY_RIGHTALT)
    }

    pub fn ctrl_space() -> Self {
        Self::Compound {
            modifiers: vec![Modifier::Ctrl],
            trigger_key: KeyCode::KEY_SPACE,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChordEvent {
    None,
    Press,
    Release,
}

pub struct KeyChordTracker {
    target: TargetChord,
    pressed_keys: HashSet<KeyCode>,
    is_chord_active: bool,
}

impl KeyChordTracker {
    pub fn new(target: TargetChord) -> Self {
        Self {
            target,
            pressed_keys: HashSet::new(),
            is_chord_active: false,
        }
    }

    fn is_ctrl_pressed(&self) -> bool {
        self.pressed_keys.contains(&KeyCode::KEY_LEFTCTRL)
            || self.pressed_keys.contains(&KeyCode::KEY_RIGHTCTRL)
    }

    fn is_alt_pressed(&self) -> bool {
        self.pressed_keys.contains(&KeyCode::KEY_LEFTALT)
            || self.pressed_keys.contains(&KeyCode::KEY_RIGHTALT)
    }

    fn is_shift_pressed(&self) -> bool {
        self.pressed_keys.contains(&KeyCode::KEY_LEFTSHIFT)
            || self.pressed_keys.contains(&KeyCode::KEY_RIGHTSHIFT)
    }

    fn is_super_pressed(&self) -> bool {
        self.pressed_keys.contains(&KeyCode::KEY_LEFTMETA)
            || self.pressed_keys.contains(&KeyCode::KEY_RIGHTMETA)
    }

    fn check_modifiers(&self, modifiers: &[Modifier]) -> bool {
        modifiers.iter().all(|m| match m {
            Modifier::Ctrl => self.is_ctrl_pressed(),
            Modifier::Alt => self.is_alt_pressed(),
            Modifier::Shift => self.is_shift_pressed(),
            Modifier::Super => self.is_super_pressed(),
        })
    }

    pub fn handle_key(&mut self, key: KeyCode, is_down: bool) -> ChordEvent {
        if is_down {
            self.pressed_keys.insert(key);
        } else {
            self.pressed_keys.remove(&key);
        }

        match &self.target {
            TargetChord::SingleKey(target_key) => {
                let target = *target_key;
                if key == target {
                    if is_down && !self.is_chord_active {
                        self.is_chord_active = true;
                        ChordEvent::Press
                    } else if !is_down && self.is_chord_active {
                        self.is_chord_active = false;
                        ChordEvent::Release
                    } else {
                        ChordEvent::None
                    }
                } else {
                    ChordEvent::None
                }
            }
            TargetChord::Compound {
                modifiers,
                trigger_key,
            } => {
                let trigger = *trigger_key;
                if key == trigger {
                    if is_down && self.check_modifiers(modifiers) && !self.is_chord_active {
                        self.is_chord_active = true;
                        ChordEvent::Press
                    } else if !is_down && self.is_chord_active {
                        self.is_chord_active = false;
                        ChordEvent::Release
                    } else {
                        ChordEvent::None
                    }
                } else if !is_down && self.is_chord_active && !self.check_modifiers(modifiers) {
                    self.is_chord_active = false;
                    ChordEvent::Release
                } else {
                    ChordEvent::None
                }
            }
        }
    }

    pub fn is_chord_release(&self, key: KeyCode, is_down: bool) -> bool {
        if is_down || !self.is_chord_active {
            return false;
        }
        match &self.target {
            TargetChord::SingleKey(target_key) => key == *target_key,
            TargetChord::Compound {
                modifiers,
                trigger_key,
            } => {
                if key == *trigger_key {
                    true
                } else {
                    match key {
                        KeyCode::KEY_LEFTCTRL | KeyCode::KEY_RIGHTCTRL => {
                            modifiers.contains(&Modifier::Ctrl)
                        }
                        KeyCode::KEY_LEFTALT | KeyCode::KEY_RIGHTALT => {
                            modifiers.contains(&Modifier::Alt)
                        }
                        KeyCode::KEY_LEFTSHIFT | KeyCode::KEY_RIGHTSHIFT => {
                            modifiers.contains(&Modifier::Shift)
                        }
                        KeyCode::KEY_LEFTMETA | KeyCode::KEY_RIGHTMETA => {
                            modifiers.contains(&Modifier::Super)
                        }
                        _ => false,
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DictationState {
    Idle,
    Recording,
    Processing,
    Emitting,
}

impl DictationState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Recording => "Recording",
            Self::Processing => "Processing",
            Self::Emitting => "Emitting",
        }
    }
}

pub trait DictationEngine {
    fn on_press(&mut self) -> Result<(), String>;
    fn on_release(&mut self) -> Result<Option<String>, String>;
}

pub struct MockDictationEngine {
    payload: String,
}

impl MockDictationEngine {
    pub fn new() -> Self {
        Self {
            payload: "Hello from Speechyyy".to_string(),
        }
    }

    pub fn with_payload(payload: &str) -> Self {
        Self {
            payload: payload.to_string(),
        }
    }
}

impl Default for MockDictationEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl DictationEngine for MockDictationEngine {
    fn on_press(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn on_release(&mut self) -> Result<Option<String>, String> {
        Ok(Some(self.payload.clone()))
    }
}

pub struct AudioDictationEngine<C: AudioCapture, I: SpeechInference> {
    pub capture: C,
    pub vad: EnergyVad,
    pub inference: I,
    pub pipeline: TextPipeline,
    pub history: Option<HistoryLogger>,
}

impl<C: AudioCapture, I: SpeechInference> AudioDictationEngine<C, I> {
    pub fn new(capture: C, vad: EnergyVad, inference: I) -> Self {
        Self {
            capture,
            vad,
            inference,
            pipeline: TextPipeline::default(),
            history: None,
        }
    }

    pub fn with_pipeline_and_history(
        capture: C,
        vad: EnergyVad,
        inference: I,
        pipeline: TextPipeline,
        history: Option<HistoryLogger>,
    ) -> Self {
        Self {
            capture,
            vad,
            inference,
            pipeline,
            history,
        }
    }
}

impl<C: AudioCapture, I: SpeechInference> DictationEngine for AudioDictationEngine<C, I> {
    fn on_press(&mut self) -> Result<(), String> {
        self.capture
            .start_capture()
            .map_err(|e| format!("Audio capture start failed: {}", e))
    }

    fn on_release(&mut self) -> Result<Option<String>, String> {
        let chunk = self
            .capture
            .stop_capture()
            .map_err(|e| format!("Audio capture stop failed: {}", e))?;

        match self.vad.evaluate(&chunk) {
            Some(metrics) => {
                eprintln!(
                    "VAD speech detected: {:.2}s, RMS {:.4} (samples: {})",
                    metrics.duration_secs, metrics.rms, metrics.sample_count
                );
                match self.inference.transcribe(&chunk) {
                    Ok(transcript) => {
                        let trimmed = transcript.trim();
                        if trimmed.is_empty() {
                            eprintln!("Inference returned empty transcript");
                            return Ok(None);
                        }

                        match self.pipeline.process(trimmed) {
                            Some(final_text) => {
                                if let Some(ref logger) = self.history {
                                    let _ = logger.log(
                                        trimmed,
                                        &final_text,
                                        self.inference.provider_name(),
                                        metrics.duration_secs,
                                    );
                                }
                                Ok(Some(final_text))
                            }
                            None => {
                                eprintln!("Transcript cleaned to empty (hallucination discarded)");
                                Ok(None)
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Speech inference failed: {}", e);
                        // Do not inject error string into active caret
                        Ok(None)
                    }
                }
            }
            None => {
                eprintln!("Audio discarded: below VAD threshold or duration < 200ms");
                Ok(None)
            }
        }
    }
}

pub struct StateMachineController<E: DictationEngine> {
    pub tracker: KeyChordTracker,
    pub engine: E,
    pub state: DictationState,
}

impl<E: DictationEngine> StateMachineController<E> {
    pub fn new(target: TargetChord, engine: E) -> Self {
        Self {
            tracker: KeyChordTracker::new(target),
            engine,
            state: DictationState::Idle,
        }
    }

    pub fn start_recording(&mut self) -> Result<(), String> {
        if self.state == DictationState::Idle {
            self.state = DictationState::Recording;
            self.engine.on_press()?;
        }
        Ok(())
    }

    pub fn stop_and_transcribe(&mut self) -> Result<Option<String>, String> {
        if self.state == DictationState::Recording {
            self.state = DictationState::Processing;
            let text_opt = self.engine.on_release()?;
            if text_opt.is_some() {
                self.state = DictationState::Emitting;
            } else {
                self.state = DictationState::Idle;
            }
            Ok(text_opt)
        } else {
            Ok(None)
        }
    }

    pub fn toggle(&mut self) -> Result<Option<String>, String> {
        match self.state {
            DictationState::Idle => {
                self.start_recording()?;
                Ok(None)
            }
            DictationState::Recording => self.stop_and_transcribe(),
            _ => Ok(None),
        }
    }

    pub fn handle_key_event(
        &mut self,
        key: KeyCode,
        is_down: bool,
    ) -> Result<Option<String>, String> {
        match self.tracker.handle_key(key, is_down) {
            ChordEvent::Press => {
                self.start_recording()?;
                Ok(None)
            }
            ChordEvent::Release => self.stop_and_transcribe(),
            ChordEvent::None => Ok(None),
        }
    }

    pub fn finish_emission(&mut self) {
        if self.state == DictationState::Emitting {
            self.state = DictationState::Idle;
        }
    }

    pub fn is_chord_release(&self, key: KeyCode, is_down: bool) -> bool {
        if self.state == DictationState::Recording {
            self.tracker.is_chord_release(key, is_down)
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_key_right_alt_press_and_release() {
        let mut tracker = KeyChordTracker::new(TargetChord::right_alt());

        assert_eq!(
            tracker.handle_key(KeyCode::KEY_RIGHTALT, true),
            ChordEvent::Press
        );
        assert_eq!(
            tracker.handle_key(KeyCode::KEY_RIGHTALT, true),
            ChordEvent::None
        );
        assert_eq!(tracker.handle_key(KeyCode::KEY_A, true), ChordEvent::None);
        assert_eq!(
            tracker.handle_key(KeyCode::KEY_RIGHTALT, false),
            ChordEvent::Release
        );
    }

    #[test]
    fn test_compound_chord_ctrl_space() {
        let mut tracker = KeyChordTracker::new(TargetChord::ctrl_space());

        assert_eq!(
            tracker.handle_key(KeyCode::KEY_SPACE, true),
            ChordEvent::None
        );
        assert_eq!(
            tracker.handle_key(KeyCode::KEY_SPACE, false),
            ChordEvent::None
        );

        assert_eq!(
            tracker.handle_key(KeyCode::KEY_LEFTCTRL, true),
            ChordEvent::None
        );
        assert_eq!(
            tracker.handle_key(KeyCode::KEY_SPACE, true),
            ChordEvent::Press
        );

        assert_eq!(
            tracker.handle_key(KeyCode::KEY_SPACE, false),
            ChordEvent::Release
        );
        assert_eq!(
            tracker.handle_key(KeyCode::KEY_LEFTCTRL, false),
            ChordEvent::None
        );
    }

    #[test]
    fn test_compound_chord_modifier_released_first() {
        let mut tracker = KeyChordTracker::new(TargetChord::ctrl_space());

        assert_eq!(
            tracker.handle_key(KeyCode::KEY_RIGHTCTRL, true),
            ChordEvent::None
        );
        assert_eq!(
            tracker.handle_key(KeyCode::KEY_SPACE, true),
            ChordEvent::Press
        );

        assert_eq!(
            tracker.handle_key(KeyCode::KEY_RIGHTCTRL, false),
            ChordEvent::Release
        );
        assert_eq!(
            tracker.handle_key(KeyCode::KEY_SPACE, false),
            ChordEvent::None
        );
    }

    #[test]
    fn test_state_machine_flow_with_mock_engine() {
        let engine = MockDictationEngine::new();
        let mut controller = StateMachineController::new(TargetChord::right_alt(), engine);

        assert_eq!(controller.state, DictationState::Idle);

        let res = controller
            .handle_key_event(KeyCode::KEY_RIGHTALT, true)
            .unwrap();
        assert_eq!(res, None);
        assert_eq!(controller.state, DictationState::Recording);

        let res = controller
            .handle_key_event(KeyCode::KEY_RIGHTALT, false)
            .unwrap();
        assert_eq!(res, Some("Hello from Speechyyy".to_string()));
        assert_eq!(controller.state, DictationState::Emitting);

        controller.finish_emission();
        assert_eq!(controller.state, DictationState::Idle);
    }

    #[test]
    fn test_controller_direct_recording_and_toggle() {
        let engine = MockDictationEngine::with_payload("Direct IPC Payload");
        let mut controller = StateMachineController::new(TargetChord::right_alt(), engine);

        assert_eq!(controller.state, DictationState::Idle);

        // 1. Direct start_recording
        controller.start_recording().unwrap();
        assert_eq!(controller.state, DictationState::Recording);

        // 2. Direct stop_and_transcribe
        let text = controller.stop_and_transcribe().unwrap();
        assert_eq!(text, Some("Direct IPC Payload".to_string()));
        assert_eq!(controller.state, DictationState::Emitting);
        controller.finish_emission();
        assert_eq!(controller.state, DictationState::Idle);

        // 3. Toggle from Idle -> Recording
        let toggle_res1 = controller.toggle().unwrap();
        assert_eq!(toggle_res1, None);
        assert_eq!(controller.state, DictationState::Recording);

        // 4. Toggle from Recording -> Processing/Emitting
        let toggle_res2 = controller.toggle().unwrap();
        assert_eq!(toggle_res2, Some("Direct IPC Payload".to_string()));
        assert_eq!(controller.state, DictationState::Emitting);
        controller.finish_emission();
        assert_eq!(controller.state, DictationState::Idle);
    }

    #[test]
    fn test_audio_dictation_engine_speech_detected() {
        use crate::audio::capture::MockAudioCapture;
        use crate::audio::AudioChunk;
        use crate::inference::mock::MockSpeechInference;

        // Valid speech chunk: 500ms at 16kHz with RMS 0.05
        let speech_chunk = AudioChunk::new(vec![0.05; 8000]);
        let capture = MockAudioCapture::with_queued_chunk(speech_chunk);
        let vad = EnergyVad::default();
        let inference = MockSpeechInference::with_transcript("Real transcribed speech");
        let engine = AudioDictationEngine::new(capture, vad, inference);
        let mut controller = StateMachineController::new(TargetChord::right_alt(), engine);

        // Press hotkey
        controller
            .handle_key_event(KeyCode::KEY_RIGHTALT, true)
            .unwrap();
        assert_eq!(controller.state, DictationState::Recording);
        assert!(controller.engine.capture.is_capturing());

        // Release hotkey
        let output = controller
            .handle_key_event(KeyCode::KEY_RIGHTALT, false)
            .unwrap();
        assert_eq!(controller.state, DictationState::Emitting);
        assert!(!controller.engine.capture.is_capturing());
        assert_eq!(output, Some("Real transcribed speech".to_string()));

        controller.finish_emission();
        assert_eq!(controller.state, DictationState::Idle);
    }

    #[test]
    fn test_audio_dictation_engine_silence_discarded() {
        use crate::audio::capture::MockAudioCapture;
        use crate::audio::AudioChunk;
        use crate::inference::mock::MockSpeechInference;

        // Silence chunk: 500ms at 16kHz with RMS 0.0001
        let silence_chunk = AudioChunk::new(vec![0.0001; 8000]);
        let capture = MockAudioCapture::with_queued_chunk(silence_chunk);
        let vad = EnergyVad::default();
        let inference = MockSpeechInference::with_transcript("Should not be called");
        let engine = AudioDictationEngine::new(capture, vad, inference);
        let mut controller = StateMachineController::new(TargetChord::right_alt(), engine);

        // Press
        controller
            .handle_key_event(KeyCode::KEY_RIGHTALT, true)
            .unwrap();
        assert_eq!(controller.state, DictationState::Recording);

        // Release: silence should evaluate to None and state should return directly to Idle
        let output = controller
            .handle_key_event(KeyCode::KEY_RIGHTALT, false)
            .unwrap();
        assert_eq!(output, None);
        assert_eq!(controller.state, DictationState::Idle);
    }

    #[test]
    fn test_audio_dictation_engine_micro_tap_discarded() {
        use crate::audio::capture::MockAudioCapture;
        use crate::audio::AudioChunk;
        use crate::inference::mock::MockSpeechInference;

        // Micro-tap chunk: 50ms at 16kHz (800 samples < 3200 minimum)
        let micro_tap_chunk = AudioChunk::new(vec![0.1; 800]);
        let capture = MockAudioCapture::with_queued_chunk(micro_tap_chunk);
        let vad = EnergyVad::default();
        let inference = MockSpeechInference::new();
        let engine = AudioDictationEngine::new(capture, vad, inference);
        let mut controller = StateMachineController::new(TargetChord::right_alt(), engine);

        controller
            .handle_key_event(KeyCode::KEY_RIGHTALT, true)
            .unwrap();
        let output = controller
            .handle_key_event(KeyCode::KEY_RIGHTALT, false)
            .unwrap();
        assert_eq!(output, None);
        assert_eq!(controller.state, DictationState::Idle);
    }

    #[test]
    fn test_audio_dictation_engine_empty_transcript_returns_none() {
        use crate::audio::capture::MockAudioCapture;
        use crate::audio::AudioChunk;
        use crate::inference::mock::MockSpeechInference;

        let speech_chunk = AudioChunk::new(vec![0.05; 8000]);
        let capture = MockAudioCapture::with_queued_chunk(speech_chunk);
        let vad = EnergyVad::default();
        let inference = MockSpeechInference::with_transcript("   ");
        let engine = AudioDictationEngine::new(capture, vad, inference);
        let mut controller = StateMachineController::new(TargetChord::right_alt(), engine);

        controller
            .handle_key_event(KeyCode::KEY_RIGHTALT, true)
            .unwrap();
        let output = controller
            .handle_key_event(KeyCode::KEY_RIGHTALT, false)
            .unwrap();
        assert_eq!(output, None);
        assert_eq!(controller.state, DictationState::Idle);
    }

    #[test]
    fn test_audio_dictation_engine_inference_error_returns_none() {
        use crate::audio::capture::MockAudioCapture;
        use crate::audio::AudioChunk;
        use crate::inference::{InferenceError, SpeechInference};

        struct FailingInference;
        impl SpeechInference for FailingInference {
            fn transcribe(&mut self, _chunk: &AudioChunk) -> Result<String, InferenceError> {
                Err(InferenceError::Network("Server down".into()))
            }
            fn provider_name(&self) -> &'static str {
                "failing"
            }
        }

        let speech_chunk = AudioChunk::new(vec![0.05; 8000]);
        let capture = MockAudioCapture::with_queued_chunk(speech_chunk);
        let vad = EnergyVad::default();
        let engine = AudioDictationEngine::new(capture, vad, FailingInference);
        let mut controller = StateMachineController::new(TargetChord::right_alt(), engine);

        controller
            .handle_key_event(KeyCode::KEY_RIGHTALT, true)
            .unwrap();
        // Failure shouldn't cause an unwrap or error propagation, just Ok(None) to stay resilient
        let output = controller
            .handle_key_event(KeyCode::KEY_RIGHTALT, false)
            .unwrap();
        assert_eq!(output, None);
        assert_eq!(controller.state, DictationState::Idle);
    }

    #[test]
    fn test_audio_dictation_engine_hallucination_discarded() {
        use crate::audio::capture::MockAudioCapture;
        use crate::audio::AudioChunk;
        use crate::inference::mock::MockSpeechInference;

        let speech_chunk = AudioChunk::new(vec![0.05; 8000]);
        let capture = MockAudioCapture::with_queued_chunk(speech_chunk);
        let vad = EnergyVad::default();
        let inference = MockSpeechInference::with_transcript("[BLANK_AUDIO]");
        let engine = AudioDictationEngine::new(capture, vad, inference);
        let mut controller = StateMachineController::new(TargetChord::right_alt(), engine);

        controller
            .handle_key_event(KeyCode::KEY_RIGHTALT, true)
            .unwrap();
        let output = controller
            .handle_key_event(KeyCode::KEY_RIGHTALT, false)
            .unwrap();
        assert_eq!(output, None);
        assert_eq!(controller.state, DictationState::Idle);
    }

    #[test]
    fn test_audio_dictation_engine_dictionary_and_history_logging() {
        use crate::audio::capture::MockAudioCapture;
        use crate::audio::AudioChunk;
        use crate::inference::mock::MockSpeechInference;
        use crate::text::{LearningDictionary, TextFormatter, TextPipeline};

        let temp_dir =
            std::env::temp_dir().join(format!("engine_hist_test_{}", std::process::id()));
        let hist_path = temp_dir.join("history.jsonl");
        let history = HistoryLogger::with_path(hist_path.clone());

        let mut dict = LearningDictionary::new();
        dict.add_rule("post gress", "PostgreSQL");

        let pipeline = TextPipeline::new(TextFormatter::default(), dict);

        let speech_chunk = AudioChunk::new(vec![0.05; 8000]);
        let capture = MockAudioCapture::with_queued_chunk(speech_chunk);
        let vad = EnergyVad::default();
        let inference = MockSpeechInference::with_transcript("i love post gress");

        let engine = AudioDictationEngine::with_pipeline_and_history(
            capture,
            vad,
            inference,
            pipeline,
            Some(history.clone()),
        );
        let mut controller = StateMachineController::new(TargetChord::right_alt(), engine);

        controller
            .handle_key_event(KeyCode::KEY_RIGHTALT, true)
            .unwrap();
        let output = controller
            .handle_key_event(KeyCode::KEY_RIGHTALT, false)
            .unwrap();

        assert_eq!(output, Some("I love PostgreSQL".to_string()));
        assert_eq!(controller.state, DictationState::Emitting);

        // Verify history log was appended
        let last = history.get_last_entry().unwrap().unwrap();
        assert_eq!(last.raw_text, "i love post gress");
        assert_eq!(last.final_text, "I love PostgreSQL");

        let _ = std::fs::remove_dir_all(temp_dir);
    }
}
