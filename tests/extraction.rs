use yt_lite_extraction_proof::{canonical_video_url, parse_provider_json, Capability, Provider};

const ID: &str = "dQw4w9WgXcQ";

#[test]
fn rust_route_resolves_metadata_and_streams_but_does_not_claim_captions() {
    let result = parse_provider_json(Provider::Rust, ID, include_str!("fixtures/rust-video.json")).unwrap();
    assert_eq!(result.provider, Provider::Rust);
    assert_eq!(result.title, "Sample music video");
    assert_eq!(result.channel.as_deref(), Some("Sample channel"));
    assert_eq!(result.duration_seconds, Some(212));
    match result.streams { Capability::Available(streams) => {
        assert_eq!(streams.len(), 2);
        assert_eq!(streams[0].format_id, "18");
        assert!(streams[0].has_video && streams[0].has_audio);
        assert!(!streams[1].has_video && streams[1].has_audio);
    }, _ => panic!("expected selectable streams") }
    assert_eq!(result.captions, Capability::Unknown);
    match result.chapters { Capability::Available(items) => assert_eq!(items[0].start_seconds, 0.0), _ => panic!("expected chapters") }
}

#[test]
fn helper_route_resolves_captions_chapters_and_audio_only_stream() {
    let result = parse_provider_json(Provider::YtDlp, ID, include_str!("fixtures/yt-dlp-video.json")).unwrap();
    assert_eq!(result.thumbnail.as_deref(), Some("https://i.ytimg.com/vi/dQw4w9WgXcQ/hqdefault.jpg"));
    match result.captions { Capability::Available(items) => {
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].language, "en");
        assert_eq!(items[1].language, "pl");
        assert!(items[1].automatic);
    }, _ => panic!("expected captions") }
    match result.streams { Capability::Available(items) => {
        assert_eq!(items[1].format_id, "140");
        assert!(!items[1].has_video && items[1].has_audio);
    }, _ => panic!("expected streams") }
}

#[test]
fn explicit_empty_capabilities_do_not_look_like_provider_errors() {
    let result = parse_provider_json(Provider::YtDlp, ID, include_str!("fixtures/unavailable-video.json")).unwrap();
    assert_eq!(result.streams, Capability::Unavailable);
    assert_eq!(result.captions, Capability::Unavailable);
    assert_eq!(result.chapters, Capability::Unavailable);
}

#[test]
fn malformed_and_mismatched_provider_outputs_are_actionable() {
    let malformed = parse_provider_json(Provider::Rust, ID, "not json").unwrap_err();
    assert_eq!(malformed.code, "malformed_response");
    assert!(!malformed.action.is_empty());
    let mismatched = parse_provider_json(Provider::YtDlp, "12345678901", include_str!("fixtures/yt-dlp-video.json")).unwrap_err();
    assert_eq!(mismatched.code, "id_mismatch");
}

#[test]
fn canonical_url_rejects_untrusted_hosts_and_missing_ids() {
    assert_eq!(canonical_video_url("https://youtu.be/dQw4w9WgXcQ?t=20").unwrap().1,
        "https://www.youtube.com/watch?v=dQw4w9WgXcQ");
    assert_eq!(canonical_video_url("https://www.youtube.com/shorts/dQw4w9WgXcQ").unwrap().0, ID);
    assert_eq!(canonical_video_url("https://youtube.com.evil.test/watch?v=dQw4w9WgXcQ").unwrap_err().code, "invalid_video_id");
    assert_eq!(canonical_video_url("file:///etc/passwd").unwrap_err().code, "invalid_url");
}


#[test]
fn ignores_storyboard_formats_that_do_not_contain_playable_media() {
    // yt-dlp may include storyboard/image formats in its formats array.
    let payload = r#"{
        "id":"dQw4w9WgXcQ", "title":"Public sample",
        "formats":[
            {"format_id":"sb0","url":"https://example.test/storyboard.jpg",
             "vcodec":"none","acodec":"none"},
            {"format_id":"18","url":"https://example.test/muxed",
             "vcodec":"avc1","acodec":"mp4a"},
            {"format_id":"140","url":"https://example.test/audio",
             "vcodec":"none","acodec":"mp4a"}
        ]
    }"#;
    let result = parse_provider_json(Provider::YtDlp, ID, payload).unwrap();
    match result.streams {
        Capability::Available(formats) => {
            assert_eq!(formats.len(), 2);
            assert_eq!(formats[0].format_id, "18");
            assert_eq!(formats[1].format_id, "140");
        },
        _ => panic!("expected two media streams without the storyboard"),
    }
}

#[test]
fn storyboard_only_response_has_no_playable_streams() {
    let payload = r#"{
        "id":"dQw4w9WgXcQ", "title":"Public sample",
        "formats":[
            {"format_id":"sb0","url":"https://example.test/storyboard.jpg",
             "vcodec":"none","acodec":"none"}
        ]
    }"#;
    let result = parse_provider_json(Provider::YtDlp, ID, payload).unwrap();
    assert_eq!(result.streams, Capability::Unavailable);
}
