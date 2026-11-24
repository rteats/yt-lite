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
