use yt_lite_native_player_proof::{command_for, PlaybackAction};

#[test]
fn play_pause_is_a_single_command_without_replacing_the_session() {
    assert_eq!(command_for(PlaybackAction::TogglePause), Some(vec!["cycle".into(), "pause".into()]));
}

#[test]
fn relative_seek_is_preserved_in_seconds() {
    assert_eq!(command_for(PlaybackAction::SeekRelative(-10)), Some(vec!["seek".into(), "-10".into(), "relative".into()]));
    assert_eq!(command_for(PlaybackAction::SeekRelative(10)), Some(vec!["seek".into(), "10".into(), "relative".into()]));
}

#[test]
fn scrub_maps_to_absolute_seconds_and_rejects_invalid_values() {
    assert_eq!(command_for(PlaybackAction::SeekFraction { fraction: 0.25, duration_seconds: 80.0 }),
        Some(vec!["seek".into(), "20.000".into(), "absolute".into()]));
    assert_eq!(command_for(PlaybackAction::SeekFraction { fraction: f64::NAN, duration_seconds: 80.0 }), None);
    assert_eq!(command_for(PlaybackAction::SeekFraction { fraction: 1.5, duration_seconds: 80.0 }), None);
    assert_eq!(command_for(PlaybackAction::SeekFraction { fraction: 0.5, duration_seconds: 0.0 }), None);
}
