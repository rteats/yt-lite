//! Offline contract tests for optional playback-only yt-dlp output.
//! No network access, player DLL, or signed URLs are involved.

use yt_lite_extraction_proof::{
    parse_provider_json, Capability, Provider, YT_DLP_PLAYBACK_FORMAT_ARGS,
};

#[test]
fn lean_output_keeps_all_formats_and_playback_headers() {
    let projected = r#"{
        "id": "dQw4w9WgXcQ",
        "title": "Fixture",
        "duration": 123,
        "http_headers": {"User-Agent": "yt-lite-test", "Referer": "https://www.youtube.com/"},
        "formats": [
            {"format_id":"18","url":"https://cdn.example.test/muxed",
             "vcodec":"avc1","acodec":"mp4a","height":360},
            {"format_id":"137","url":"https://cdn.example.test/video",
             "vcodec":"avc1","acodec":"none","height":1080},
            {"format_id":"140","url":"https://cdn.example.test/audio",
             "vcodec":"none","acodec":"mp4a"}
        ]
    }"#;
    let result = parse_provider_json(Provider::YtDlp, "dQw4w9WgXcQ", projected).unwrap();
    assert_eq!(result.title, "Fixture");
    assert_eq!(result.duration_seconds, Some(123));
    let Capability::Available(formats) = result.streams else { panic!("formats missing"); };
    assert_eq!(formats.len(), 3);
    let muxed = formats.iter().find(|format| format.format_id == "18").unwrap();
    assert!(muxed.has_audio && muxed.has_video);
    assert_eq!(muxed.http_headers.user_agent.as_deref(), Some("yt-lite-test"));
    assert_eq!(muxed.http_headers.referer.as_deref(), Some("https://www.youtube.com/"));
    // Absence of caption/chapter arrays must mean Unknown, not Unavailable.
    assert!(matches!(result.captions, Capability::Unknown));
    assert!(matches!(result.chapters, Capability::Unknown));
}

#[test]
fn lean_output_requests_id_title_all_formats_and_root_headers() {
    assert_eq!(YT_DLP_PLAYBACK_FORMAT_ARGS[0], "--print");
    let template = YT_DLP_PLAYBACK_FORMAT_ARGS[1];
    for field in ["id", "title", "formats", "http_headers", "duration", "thumbnail", "channel"] {
        assert!(template.contains(field), "missing {field}");
    }
    assert!(!template.contains("automatic_captions"));
    assert!(!template.contains("subtitles"));
    assert!(!template.contains("chapters"));
}
