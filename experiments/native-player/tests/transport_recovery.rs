use std::time::Duration;
use yt_lite_native_player_proof::transport_recovery::{transport_diagnostic, RecoveryAction, StartupRecovery};

#[test]
fn failed_split_refreshes_once_before_quality_fallback() {
    let mut policy = StartupRecovery::default();
    assert_eq!(policy.next(Duration::from_secs(7), true, true), RecoveryAction::Refresh);
    assert_eq!(policy.next(Duration::from_secs(17), true, true), RecoveryAction::Muxed);
    assert_eq!(policy.next(Duration::from_secs(19), true, true), RecoveryAction::Stop);
}

#[test]
fn budget_expiry_prevents_any_additional_attempt() {
    let mut policy = StartupRecovery::default();
    assert_eq!(policy.next(Duration::from_secs(30), true, true), RecoveryAction::Stop);
    let mut muxed = StartupRecovery::default();
    assert_eq!(muxed.next(Duration::from_secs(3), false, false), RecoveryAction::Stop);
}

#[test]
fn diagnostics_are_fixed_labels_not_urls_or_arbitrary_status_numbers() {
    assert_eq!(transport_diagnostic("HTTP error 403 Forbidden: https://example.invalid/?sig=SECRET"), Some("http_403"));
    assert_eq!(transport_diagnostic("Connection reset by peer"), Some("connection_reset"));
    assert_eq!(transport_diagnostic("Stream ends prematurely at 123"), Some("premature_eof"));
    assert_eq!(transport_diagnostic("https://example.invalid/?range=403-500&sig=SECRET"), None);
    assert_eq!(transport_diagnostic("Error opening demuxer"), Some("demux_error"));
}


#[test]
fn video_only_restart_is_not_ready_and_unknown_flags_are_not_success() {
    use yt_lite_native_player_proof::transport_recovery::startup_av_ready;
    assert!(!startup_av_ready(Some(0.2), None, Some(false), Some(false), Some(false), None));
    assert!(!startup_av_ready(Some(0.2), Some(0.18), Some(false), None, Some(false), Some(48000.0)));
    assert!(!startup_av_ready(Some(0.2), Some(0.18), Some(false), Some(true), Some(false), Some(48000.0)));
    assert!(!startup_av_ready(Some(0.2), Some(0.18), Some(false), Some(false), Some(false), None));
    assert!(startup_av_ready(Some(0.2), Some(0.18), Some(false), Some(false), Some(false), Some(48000.0)));
}

#[test]
fn split_fixture_without_muxed_candidate_stops_after_one_refresh() {
    let mut policy = StartupRecovery::default();
    assert_eq!(policy.next(Duration::from_secs(5), true, false), RecoveryAction::Refresh);
    assert_eq!(policy.next(Duration::from_secs(10), true, false), RecoveryAction::Stop);
}


#[test]
fn only_current_external_audio_open_failure_is_terminal() {
    use yt_lite_native_player_proof::transport_recovery::terminal_audio_open_failure;
    let current = "http://127.0.0.1:8770/attempt/2/audio.m4a";
    assert!(terminal_audio_open_failure("Can not open external file http://127.0.0.1:8770/attempt/2/audio.m4a.\n", current));
    assert!(!terminal_audio_open_failure("Can not open external file http://127.0.0.1:8770/attempt/1/audio.m4a.\n", current));
    assert!(!terminal_audio_open_failure("HTTP error 503", current));
    assert!(!terminal_audio_open_failure("Audio output error", current));
    assert!(!terminal_audio_open_failure("Can not open external file http://127.0.0.1:8770/attempt/2/video.mp4.\n", current));
    assert!(!terminal_audio_open_failure("Can not open external file .", ""));
}

#[test]
fn explicit_open_failure_only_recovers_during_unready_startup() {
    use yt_lite_native_player_proof::transport_recovery::missing_required_audio_after_open_failure as check;
    assert!(check(true, true, true, None, None));
    assert!(!check(false, true, true, None, None));
    assert!(!check(true, false, true, None, None));
    assert!(!check(true, true, false, None, None));
    assert!(!check(true, true, true, Some(0.0), None));
    assert!(!check(true, true, true, None, Some(48000.0)));
}

#[test]
fn edl_experiment_uses_only_two_length_quoted_loopback_sources() {
    use yt_lite_native_player_proof::loopback_edl_source;
    assert_eq!(loopback_edl_source(8766).as_deref(), Some("edl://!new_stream;!no_clip;%31%http://127.0.0.1:8766/video.mp4;!new_stream;!no_clip;%31%http://127.0.0.1:8766/audio.m4a"));
    assert_eq!(loopback_edl_source(0), None);
}
