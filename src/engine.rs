use crate::audio::vad::EnergyVad;
use crate::audio::AudioCapture;
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

pub struct AudioDictationEngine<C: AudioCapture> {
    pub capture: C,
    pub vad: EnergyVad,
}

impl<C: AudioCapture> AudioDictationEngine<C> {
    pub fn new(capture: C, vad: EnergyVad) -> Self {
        Self { capture, vad }
    }
}

impl<C: AudioCapture> DictationEngine for AudioDictationEngine<C> {
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
                let summary = format!(
                    "[Speech: {:.2}s, 16kHz, RMS {:.3}]",
                    metrics.duration_secs, metrics.rms
                );
                Ok(Some(summary))
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

    pub fn handle_key_event(
        &mut self,
        key: KeyCode,
        is_down: bool,
    ) -> Result<Option<String>, String> {
        match self.tracker.handle_key(key, is_down) {
            ChordEvent::Press => {
                if self.state == DictationState::Idle {
                    self.state = DictationState::Recording;
                    self.engine.on_press()?;
                }
                Ok(None)
            }
            ChordEvent::Release => {
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
            ChordEvent::None => Ok(None),
        }
    }

    pub fn finish_emission(&mut self) {
        if self.state == DictationState::Emitting {
            self.state = DictationState::Idle;
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
    fn test_audio_dictation_engine_speech_detected() {
        use crate::audio::capture::MockAudioCapture;
        use crate::audio::AudioChunk;

        // Valid speech chunk: 500ms at 16kHz with RMS 0.05
        let speech_chunk = AudioChunk::new(vec![0.05; 8000]);
        let capture = MockAudioCapture::with_queued_chunk(speech_chunk);
        let vad = EnergyVad::default();
        let engine = AudioDictationEngine::new(capture, vad);
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
        assert!(output.is_some());
        let text = output.unwrap();
        assert!(text.starts_with("[Speech: 0.50s, 16kHz, RMS 0.050]"));

        controller.finish_emission();
        assert_eq!(controller.state, DictationState::Idle);
    }

    #[test]
    fn test_audio_dictation_engine_silence_discarded() {
        use crate::audio::capture::MockAudioCapture;
        use crate::audio::AudioChunk;

        // Silence chunk: 500ms at 16kHz with RMS 0.0001
        let silence_chunk = AudioChunk::new(vec![0.0001; 8000]);
        let capture = MockAudioCapture::with_queued_chunk(silence_chunk);
        let vad = EnergyVad::default();
        let engine = AudioDictationEngine::new(capture, vad);
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

        // Micro-tap chunk: 50ms at 16kHz (800 samples < 3200 minimum)
        let micro_tap_chunk = AudioChunk::new(vec![0.1; 800]);
        let capture = MockAudioCapture::with_queued_chunk(micro_tap_chunk);
        let vad = EnergyVad::default();
        let engine = AudioDictationEngine::new(capture, vad);
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
}
