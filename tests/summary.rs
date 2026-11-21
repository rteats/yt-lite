use yt_lite_extraction_proof::{parse_provider_json, summarize_extraction, CapabilityStatus, Provider};

const ID: &str = "dQw4w9WgXcQ";

#[test]
fn summarizes_yt_dlp_capabilities_without_exposing_provider_details() {
    let result = parse_provider_json(
        Provider::YtDlp,
        ID,
        include_str!("fixtures/yt-dlp-video.json"),
    ).unwrap();
    let summary = summarize_extraction(&result);
    assert_eq!(summary.provider, Provider::YtDlp);
    assert_eq!(summary.streams.status, CapabilityStatus::Available);
    assert_eq!(summary.streams.count, 2);
    assert_eq!(summary.video_streams, 1);
    assert_eq!(summary.audio_only_streams, 1);
    assert_eq!(summary.captions.status, CapabilityStatus::Available);
    assert_eq!(summary.captions.count, 2);
    assert_eq!(summary.automatic_caption_tracks, 1);
    assert_eq!(summary.chapters.count, 1);
    assert_eq!(summary.duration_seconds, Some(212));

    let json = serde_json::to_string(&summary).unwrap();
    assert!(!json.contains("example.test"));
    assert!(!json.contains("https://"));
    assert!(!json.contains(ID));
    assert!(!json.contains("Sample music video"));
    assert!(!json.contains("Sample channel"));
    assert!(!json.contains("thumbnail"));
}

#[test]
fn summary_preserves_unknown_separately_from_explicitly_unavailable() {
    let rust = parse_provider_json(
        Provider::Rust, ID, include_str!("fixtures/rust-video.json")
    ).unwrap();
    let rust_summary = summarize_extraction(&rust);
    assert_eq!(rust_summary.captions.status, CapabilityStatus::Unknown);
    assert_eq!(rust_summary.captions.count, 0);

    let unavailable = parse_provider_json(
        Provider::YtDlp, ID, include_str!("fixtures/unavailable-video.json")
    ).unwrap();
    let unavailable_summary = summarize_extraction(&unavailable);
    assert_eq!(unavailable_summary.streams.status, CapabilityStatus::Unavailable);
    assert_eq!(unavailable_summary.streams.count, 0);
    assert_eq!(unavailable_summary.captions.status, CapabilityStatus::Unavailable);
    assert_eq!(unavailable_summary.chapters.status, CapabilityStatus::Unavailable);
}
