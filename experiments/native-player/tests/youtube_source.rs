use yt_lite_extraction_proof::{parse_provider_json, Provider};
use yt_lite_native_player_proof::select_muxed_stream;

const ID: &str = "dQw4w9WgXcQ";

#[test]
fn selects_highest_resolution_muxed_video_and_not_video_only() {
    let json = r#"{
      "id":"dQw4w9WgXcQ","title":"Fixture",
      "formats":[
        {"format_id":"137","url":"https://example.test/1080p-silent",
          "vcodec":"avc1","acodec":"none","height":1080},
        {"format_id":"18","url":"https://example.test/360p-muxed",
          "vcodec":"avc1","acodec":"mp4a","height":360},
        {"format_id":"22","url":"https://example.test/720p-muxed",
          "vcodec":"avc1","acodec":"mp4a","height":720},
        {"format_id":"140","url":"https://example.test/audio-only",
          "vcodec":"none","acodec":"mp4a"}
      ]
    }"#;
    let extracted = parse_provider_json(Provider::YtDlp, ID, json).unwrap();
    let selected = select_muxed_stream(&extracted).expect("a combined audio-video stream");
    assert_eq!(selected.format_id, "22");
    assert_eq!(selected.height, Some(720));
}

#[test]
fn no_combined_audio_video_stream_means_no_single_stream_playback() {
    let json = r#"{
      "id":"dQw4w9WgXcQ","title":"Fixture",
      "formats":[
        {"format_id":"137","url":"https://example.test/silent-video",
          "vcodec":"avc1","acodec":"none","height":1080},
        {"format_id":"140","url":"https://example.test/audio",
          "vcodec":"none","acodec":"mp4a"}
      ]
    }"#;
    let extracted = parse_provider_json(Provider::YtDlp, ID, json).unwrap();
    assert!(select_muxed_stream(&extracted).is_none());
}


#[test]
fn automatic_playback_falls_back_once_after_primary_load_failure() {
    use yt_lite_native_player_proof::{fallback_after_playback_error, preferred_playback_provider};
    assert_eq!(preferred_playback_provider(Provider::Auto), Provider::YtDlp);
    assert_eq!(preferred_playback_provider(Provider::Rust), Provider::Rust);
    assert_eq!(preferred_playback_provider(Provider::YtDlp), Provider::YtDlp);
    assert_eq!(fallback_after_playback_error(Provider::YtDlp, Provider::Auto), Some(Provider::Rust));
    assert_eq!(fallback_after_playback_error(Provider::Rust, Provider::Auto), None);
    assert_eq!(fallback_after_playback_error(Provider::Rust, Provider::Rust), None);
    assert_eq!(fallback_after_playback_error(Provider::YtDlp, Provider::YtDlp), None);
}

#[test]
fn streaming_cache_is_memory_bounded_without_disk_downloads() {
    use yt_lite_native_player_proof::NETWORK_STREAM_OPTIONS;
    let read = |key| NETWORK_STREAM_OPTIONS.iter().find(|(k, _)| *k == key).map(|(_, v)| *v);
    assert_eq!(read("cache"), Some("yes"));
    assert_eq!(read("cache-on-disk"), Some("no"));
    assert_eq!(read("demuxer-max-bytes"), Some("16MiB"));
    assert_eq!(read("demuxer-max-back-bytes"), Some("4MiB"));
    assert!(read("cache-secs").is_some());
}


#[test]
fn falls_back_to_separate_video_and_audio_when_no_muxed_stream_exists() {
    use yt_lite_native_player_proof::{select_playback_streams, PlaybackStreamSelection};
    let json = r#"{
        "id":"dQw4w9WgXcQ", "title":"No muxed fixture",
        "formats":[
            {"format_id":"137","url":"https://example.test/video",
             "vcodec":"avc1","acodec":"none","height":1080},
            {"format_id":"140","url":"https://example.test/audio",
             "vcodec":"none","acodec":"mp4a"},
            {"format_id":"134","url":"https://example.test/low-video",
             "vcodec":"avc1","acodec":"none","height":360}
        ]
    }"#;
    let info = parse_provider_json(Provider::YtDlp, ID, json).unwrap();
    match select_playback_streams(&info) {
        Some(PlaybackStreamSelection::Separate { video, audio }) => {
            assert_eq!(video.format_id, "137");
            assert_eq!(audio.format_id, "140");
        }
        _ => panic!("expected separate audio and video"),
    }
}

#[test]
fn higher_quality_split_stream_is_preferred_but_incomplete_pair_is_rejected() {
    use yt_lite_native_player_proof::{select_playback_streams, PlaybackStreamSelection};
    let fixture = |formats: &str| {
        parse_provider_json(Provider::YtDlp, ID, &format!(
            r#"{{"id":"dQw4w9WgXcQ","title":"fixture","formats":[{formats}]}}"#
        )).unwrap()
    };
    let muxed = fixture(
        r#"{"format_id":"18","url":"https://example.test/muxed","vcodec":"avc1","acodec":"mp4a","height":360},
           {"format_id":"137","url":"https://example.test/video","vcodec":"avc1","acodec":"none","height":1080},
           {"format_id":"140","url":"https://example.test/audio","vcodec":"none","acodec":"mp4a"}"#
    );
    assert!(matches!(select_playback_streams(&muxed),
        Some(PlaybackStreamSelection::Separate { video, audio })
        if video.format_id == "137" && audio.format_id == "140"
    ));
    let no_audio = fixture(
        r#"{"format_id":"137","url":"https://example.test/video","vcodec":"avc1","acodec":"none"}"#
    );
    assert!(select_playback_streams(&no_audio).is_none());
}

#[test]
fn player_video_margins_reserve_logical_chrome_even_at_150_percent_dpi() {
    use yt_lite_native_player_proof::video_margin_ratios;
    let (top, bottom) = video_margin_ratios(900, 1.5, true);
    assert!((top - (118.0 * 1.5 / 900.0)).abs() < 0.001);
    assert!((bottom - (116.0 * 1.5 / 900.0)).abs() < 0.001);
    let (plain_top, _) = video_margin_ratios(900, 1.5, false);
    assert!(plain_top < top);
    let (small_top, small_bottom) = video_margin_ratios(100, 2.0, true);
    assert!(small_top + small_bottom <= 0.9 + f64::EPSILON);
}

#[test]
fn search_sidebar_reserves_right_side_without_obscuring_video() {
    use yt_lite_native_player_proof::search_margin_ratio;
    assert_eq!(search_margin_ratio(1500, 1.5, false), 0.0);
    assert!((search_margin_ratio(1500, 1.5, true) - 0.37).abs() < 0.001);
    assert!(search_margin_ratio(400, 2.0, true) <= 0.65);
}

#[test]
fn auto_quality_prefers_1080p_split_over_360p_but_keeps_muxed_for_equal_quality() {
    use yt_lite_native_player_proof::{select_playback_streams, PlaybackStreamSelection};
    let fixture = |heights: &[u64]| {
        let mut formats = vec![
            r#"{"format_id":"18","url":"https://cdn.example.test/mux","height":360,"vcodec":"avc1","acodec":"mp4a"}"#.to_string(),
            r#"{"format_id":"140","url":"https://cdn.example.test/aud","vcodec":"none","acodec":"mp4a"}"#.to_string(),
        ];
        for h in heights {
            formats.push(format!(r#"{{"format_id":"v{h}","url":"https://cdn.example.test/v{h}","height":{h},"vcodec":"vp9","acodec":"none"}}"#));
        }
        parse_provider_json(Provider::YtDlp, ID, &format!(r#"{{"id":"{ID}","title":"Fixture","formats":[{}]}}"#,formats.join(","))).unwrap()
    };
    let full = fixture(&[720, 1080, 1440, 2160]);
    assert!(matches!(select_playback_streams(&full),
        Some(PlaybackStreamSelection::Separate { video, .. }) if video.height == Some(1080)));
    let equal = fixture(&[240, 360]);
    assert!(matches!(select_playback_streams(&equal),
        Some(PlaybackStreamSelection::Muxed(stream)) if stream.height == Some(360)));
}

#[test]
fn incompatible_headers_on_top_track_do_not_hide_compatible_lower_track() {
    use yt_lite_native_player_proof::{select_playback_streams, PlaybackStreamSelection};
    let doc = r#"{
        "id":"dQw4w9WgXcQ", "title":"Fixture",
        "formats":[
            {"format_id":"18","url":"https://cdn.example.test/mux","height":360,"vcodec":"avc1","acodec":"mp4a"},
            {"format_id":"v1080","url":"https://cdn.example.test/v1080","height":1080,"vcodec":"vp9","acodec":"none","http_headers":{"Referer":"https://other.example/"}},
            {"format_id":"v720","url":"https://cdn.example.test/v720","height":720,"vcodec":"vp9","acodec":"none"},
            {"format_id":"a","url":"https://cdn.example.test/audio","vcodec":"none","acodec":"opus"}
        ]
    }"#;
    let info = parse_provider_json(Provider::YtDlp, ID, doc).unwrap();
    assert!(matches!(select_playback_streams(&info),
        Some(PlaybackStreamSelection::Separate { video, audio })
        if video.format_id == "v720" && audio.format_id == "a"));
}

#[test]
fn unknown_video_height_does_not_outrank_known_hd_video() {
    use yt_lite_native_player_proof::{select_playback_streams, PlaybackStreamSelection};
    let json = r#"{
        "id":"dQw4w9WgXcQ","title":"Fixture",
        "formats":[
            {"format_id":"unknown","url":"https://cdn.example.test/u","vcodec":"vp9","acodec":"none"},
            {"format_id":"hd","url":"https://cdn.example.test/h","height":1440,"vcodec":"vp9","acodec":"none"},
            {"format_id":"audio","url":"https://cdn.example.test/a","vcodec":"none","acodec":"opus"}
        ]
    }"#;
    let info = parse_provider_json(Provider::YtDlp, ID, json).unwrap();
    assert!(matches!(select_playback_streams(&info),
        Some(PlaybackStreamSelection::Separate { video, .. }) if video.format_id == "hd"));
}
