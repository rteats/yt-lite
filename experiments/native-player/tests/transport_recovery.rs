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
