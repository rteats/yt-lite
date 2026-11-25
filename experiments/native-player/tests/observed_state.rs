use yt_lite_native_player_proof::observed_state::{ObservedState, ObservedValue};

#[test]
fn replacement_and_seek_restart_cannot_accept_queued_old_values() {
    let mut state = ObservedState::default();
    let old = state.reset(true).unwrap();
    state.update(old, "time-pos", Some(ObservedValue::Double(52.7)));
    state.update(old, "audio-pts", Some(ObservedValue::Double(52.6)));
    assert_eq!(state.double("time-pos"), Some(52.7));
    assert_eq!(state.reset(false), None);
    state.update(old, "time-pos", Some(ObservedValue::Double(99.0)));
    assert_eq!(state.double("time-pos"), None);
    let new = state.reset(true).unwrap();
    state.update(old, "audio-pts", Some(ObservedValue::Double(52.6)));
    assert_eq!(state.double("audio-pts"), None);
    state.update(new, "time-pos", Some(ObservedValue::Double(23.65)));
    state.update(new, "audio-pts", Some(ObservedValue::Double(23.64)));
    assert_eq!(state.double("time-pos"), Some(23.65));
    assert_eq!(state.double("audio-pts"), Some(23.64));
}

#[test]
fn unavailable_is_not_zero_or_false_and_clears_previous_value() {
    let mut state = ObservedState::default();
    let id = state.reset(true).unwrap();
    assert_eq!(state.flag("pause"), None);
    state.update(id, "pause", Some(ObservedValue::Flag(false)));
    assert_eq!(state.flag("pause"), Some(false));
    state.update(id, "pause", None);
    assert_eq!(state.flag("pause"), None);
    state.update(id, "audio-pts", Some(ObservedValue::Double(0.0)));
    assert_eq!(state.double("audio-pts"), Some(0.0));
    state.update(id, "audio-pts", None);
    assert_eq!(state.double("audio-pts"), None);
    state.update(id, "audio-pts", Some(ObservedValue::Double(f64::NAN)));
    assert_eq!(state.double("audio-pts"), None);
}

#[test]
fn cache_ranges_are_owned_and_invalidated_with_media() {
    let mut state = ObservedState::default();
    let id = state.reset(true).unwrap();
    state.update(id, "demuxer-cache-state", Some(ObservedValue::Ranges(vec![(0.0, 8.0)])));
    let copy = state.ranges().unwrap();
    state.reset(true);
    assert_eq!(copy, vec![(0.0, 8.0)]);
    assert_eq!(state.ranges(), None);
}
