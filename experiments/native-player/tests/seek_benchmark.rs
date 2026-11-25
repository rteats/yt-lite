use yt_lite_native_player_proof::{main_cache_membership, SeekBenchmarkMode};

#[test]
fn target_in_a_disjoint_range_is_cached_but_gap_is_not() {
    let ranges = [(0.0, 8.0), (20.0, 26.0)];
    assert_eq!(main_cache_membership(Some(&ranges), 23.0), "inside");
    assert_eq!(main_cache_membership(Some(&ranges), 15.0), "outside");
    assert_eq!(main_cache_membership(Some(&[]), 15.0), "outside");
    assert_eq!(main_cache_membership(None, 15.0), "unknown");
}

#[test]
fn compare_explicit_seek_precision_without_changing_target() {
    assert_eq!(SeekBenchmarkMode::Exact.command(20.586).unwrap(), vec!["seek", "20.586", "absolute+exact"]);
    assert_eq!(SeekBenchmarkMode::Keyframes.command(20.586).unwrap(), vec!["seek", "20.586", "absolute+keyframes"]);
    assert!(SeekBenchmarkMode::Exact.command(f64::NAN).is_none());
    assert!(SeekBenchmarkMode::parse("other").is_none());
}


#[test]
fn exact_seek_rejects_wrong_position_even_when_playback_recovers() {
    use yt_lite_native_player_proof::exact_seek_landing_ok;
    assert!(!exact_seek_landing_ok(23.638, Some(52.733)));
    assert!(!exact_seek_landing_ok(70.913, Some(76.383)));
    assert!(exact_seek_landing_ok(52.003, Some(52.083)));
    assert!(!exact_seek_landing_ok(52.003, None));
    assert!(!exact_seek_landing_ok(52.003, Some(f64::NAN)));
}

#[test]
fn stream_classification_does_not_expose_url_or_guess_unknown() {
    use yt_lite_native_player_proof::diagnostic_stream_hint;
    assert_eq!(diagnostic_stream_hint("https://example.invalid/?mime=audio%2Fwebm&sig=SECRET"), "audio");
    assert_eq!(diagnostic_stream_hint("mime=video%2Fmp4"), "video");
    assert_eq!(diagnostic_stream_hint("HTTP error 403"), "unknown");
    assert_eq!(diagnostic_stream_hint("mime=audio mime=video"), "unknown");
}
