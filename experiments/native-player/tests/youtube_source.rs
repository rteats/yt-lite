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
