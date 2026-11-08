use speechyyy::dock::DockModel;
use speechyyy::events::DictationState;

#[test]
fn emitting_collapses_to_zero() {
    let mut model = DockModel::new();

    // 1. Recording
    model.transition_to(DictationState::Recording);
    assert_eq!(model.target_width, 180.0);
    assert_eq!(model.target_opacity, 1.0);

    // 2. Processing
    model.transition_to(DictationState::Processing);
    assert_eq!(model.target_width, 140.0);
    assert_eq!(model.target_opacity, 1.0);

    // 3. Emitting
    model.transition_to(DictationState::Emitting);
    // Exit spring must be initiated immediately upon Emitting
    assert_eq!(model.target_width, 0.0);
    assert_eq!(model.target_opacity, 0.0);
}

#[test]
fn idle_stops_animating() {
    let mut model = DockModel::new();

    // Start from active recording
    model.transition_to(DictationState::Recording);
    for _ in 0..30 {
        model.tick(1.0 / 60.0);
        model.settle();
    }
    assert!(model.is_animating());
    assert!(model.current_width > 100.0);

    // Transition to Idle
    model.transition_to(DictationState::Idle);
    assert!(model.is_animating());

    // Step the spring with a fixed dt, calling settle() each step
    let mut iterations = 0;
    while model.is_animating() && iterations < 200 {
        model.tick(1.0 / 60.0);
        model.settle();
        iterations += 1;
    }

    assert!(!model.is_animating());
    assert!(model.is_settled_hidden());
    assert_eq!(model.current_width, 0.0);
    assert_eq!(model.current_opacity, 0.0);
}

#[test]
fn error_keeps_auto_dismiss() {
    let mut model = DockModel::new();

    // Recording does not arm dismiss timer
    model.transition_to(DictationState::Recording);
    assert!(model.error_started_at.is_none());

    // Processing does not arm dismiss timer
    model.transition_to(DictationState::Processing);
    assert!(model.error_started_at.is_none());

    // Emitting does not arm dismiss timer
    model.transition_to(DictationState::Emitting);
    assert!(model.error_started_at.is_none());

    // Error arms the 2.0s dismiss timer
    model.transition_to(DictationState::Error);
    assert!(model.error_started_at.is_some());
}

