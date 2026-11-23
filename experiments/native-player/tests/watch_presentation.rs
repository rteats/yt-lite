//! Offline tests of the user-facing watch metadata presentation seam.
use yt_lite_extraction_proof::{parse_provider_json, Provider};
use yt_lite_native_player_proof::{present_watch_metadata, public_youtube_thumbnail_url};

const ID: &str = "dQw4w9WgXcQ";

#[test]
fn valid_video_shows_title_channel_and_duration_without_starting_playback() {
    let info = parse_provider_json(Provider::YtDlp, ID, r#"{
        "id":"dQw4w9WgXcQ", "title":"Example video",
        "channel":"Test Channel", "duration":3723,
        "formats":[{"format_id":"18","url":"https://cdn.example.test/video",
                    "vcodec":"avc1","acodec":"mp4a"}]
    }"#).unwrap();
    let shown = present_watch_metadata(&info);
    assert_eq!(shown.title, "Example video");
    assert_eq!(shown.subtitle, "Test Channel · 1:02:03");
    assert_eq!(shown.thumbnail_url, Some("https://i.ytimg.com/vi/dQw4w9WgXcQ/hqdefault.jpg".to_owned()));
}

#[test]
fn unknown_metadata_is_not_presented_as_zero_duration() {
    let info = parse_provider_json(Provider::YtDlp, ID, r#"{
        "id":"dQw4w9WgXcQ", "title":"Live fixture", "formats":[]
    }"#).unwrap();
    let shown = present_watch_metadata(&info);
    assert_eq!(shown.title, "Live fixture");
    assert_eq!(shown.subtitle, "Duration unavailable");
}

#[test]
fn thumbnail_urls_cannot_leave_the_public_youtube_image_origin() {
    assert_eq!(
        public_youtube_thumbnail_url(ID).as_deref(),
        Some("https://i.ytimg.com/vi/dQw4w9WgXcQ/hqdefault.jpg")
    );
    assert_eq!(public_youtube_thumbnail_url("fake;secret"), None);
    assert_eq!(public_youtube_thumbnail_url("../etc/passwd"), None);
    assert_eq!(public_youtube_thumbnail_url("dQw4w9WgXcQ?x"), None);
}
