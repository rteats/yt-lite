use yt_lite_extraction_proof::{parse_provider_json, summarize_extraction, Capability, Provider};

#[test]
fn rust_provider_parses_upstream_camel_case_fields() {
    let json = include_str!("fixtures/rusty-ytdl-serde.json");
    let result = parse_provider_json(Provider::Rust, "dQw4w9WgXcQ", json).unwrap();
    assert_eq!(result.duration_seconds, Some(212));
    assert_eq!(result.channel.as_deref(), Some("Schema channel"));
    match &result.streams {
        Capability::Available(streams) => {
            assert_eq!(streams.len(), 3);
            assert!(streams[0].has_video && streams[0].has_audio);
            assert!(!streams[1].has_video && streams[1].has_audio);
            assert!(streams[2].has_video && !streams[2].has_audio);
        }
        _ => panic!("expected three selectable streams"),
    };
    match &result.chapters {
        Capability::Available(chapters) => assert_eq!(chapters[1].start_seconds, 35.0),
        _ => panic!("expected chapters"),
    };
    let summary = summarize_extraction(&result);
    assert_eq!(summary.video_streams, 2);
    assert_eq!(summary.audio_only_streams, 1);
}
