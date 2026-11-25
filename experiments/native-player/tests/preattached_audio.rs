use yt_lite_native_player_proof::preattached_audio_command;

#[test]
fn signed_audio_url_is_one_file_local_option() {
    let video = "https://example.com/v";
    let audio = "https://example.com/a?x=1,2%20";
    let args = preattached_audio_command(video, audio).unwrap();
    assert_eq!(args, vec!["loadfile", video, "replace", "-1",
        "audio-files-append=%30%https://example.com/a?x=1,2%20"]);
}

#[test]
fn refuses_non_http_or_embedded_nul_streams() {
    assert!(preattached_audio_command("file:///video", "https://example.com/a").is_none());
    assert!(preattached_audio_command("https://example.com/v", "https://example.com/a\0").is_none());
}
