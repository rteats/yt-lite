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

#[cfg(unix)]
#[tokio::test]
async fn traced_helper_collects_stdout_and_stderr_and_keeps_json_contract() {
    use std::{fs, os::unix::fs::PermissionsExt, time::{SystemTime, UNIX_EPOCH}};
    use yt_lite_extraction_proof::{extract_playback_traced, trace_id, Provider};
    let dir = std::env::temp_dir().join(format!("yt-lite-trace-{}-{}", std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
    fs::create_dir(&dir).unwrap();
    let helper = dir.join("helper");
    fs::write(&helper, r#"#!/bin/sh
found=no
for arg in "$@"; do
    [ "$arg" = "--verbose" ] && found=yes
    [ "$arg" = "--no-warnings" ] && exit 9
done
[ "$found" = yes ] || exit 8
printf '%s\n' '[youtube] id: Downloading webpage' 'WARNING: HTTP Error 503. Retrying (1/3)' >&2
printf '%s\n' '{"id":"dQw4w9WgXcQ","title":"trace fixture","formats":[]}'
"#).unwrap();
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o700)).unwrap();
    let result = extract_playback_traced("https://www.youtube.com/watch?v=dQw4w9WgXcQ",
        Provider::YtDlp, &helper, false, None, Some(trace_id())).await;
    fs::remove_dir_all(dir).unwrap();
    assert_eq!(result.unwrap().title, "trace fixture");
}
