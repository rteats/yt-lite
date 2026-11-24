use yt_lite_extraction_proof::extraction_marker;

#[test]
fn marker_never_emits_raw_provider_payload() {
    for (line, expected) in [
        ("[youtube] id: Downloading web player API JSON", Some("player_api")),
        ("[debug] [youtube] [jsc:deno] Running deno: secret-url", Some("runtime_launch")),
        ("WARNING: HTTP Error 403. Retrying (1/3)", Some("retry")),
        ("[youtube] id: Downloading m3u8 information", Some("manifest")),
        ("https://example.test/?secret=abc", None),
    ] { assert_eq!(extraction_marker(line), expected); }
}
