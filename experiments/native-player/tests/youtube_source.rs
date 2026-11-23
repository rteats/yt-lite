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
fn muxed_stream_stays_first_choice_and_incomplete_pair_is_rejected() {
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
    assert!(matches!(select_playback_streams(&muxed), Some(PlaybackStreamSelection::Muxed(_))));
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
