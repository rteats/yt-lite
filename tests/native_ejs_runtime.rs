//! Offline contract at the existing extraction-provider boundary.
//! The executable is a local fixture, not yt-dlp or a JavaScript solver.
//! On Unix the shell fixture verifies actual argv, including a space in
//! the runtime path. Windows runtime integration is checked separately.
#![cfg(unix)]

use std::{fs, os::unix::fs::PermissionsExt, path::{Path, PathBuf}, time::{SystemTime, UNIX_EPOCH}};
use yt_lite_extraction_proof::{extract_playback, extract_playback_with_ejs, Capability, Provider};

struct TempDir(PathBuf);
impl Drop for TempDir {
    fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
}

#[tokio::test]
async fn opted_in_solver_is_a_single_runtime_argument_and_standard_api_is_unchanged() {
    let dir = TempDir(std::env::temp_dir().join(format!(
        "yt-lite-ejs-fixture-{}-{}", std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
    )));
    fs::create_dir(&dir.0).unwrap();
    let helper = dir.0.join("fixture");
    fs::write(&helper, r#"#!/bin/sh
mode=standard
reset=no
plugins=yes
full=no
while [ "$#" -gt 0 ]; do
    case "$1" in
        --no-js-runtimes) reset=yes ;;
        --no-plugin-dirs) plugins=no ;;
        --dump-single-json) full=yes ;;
        --js-runtimes)
            shift
            [ "$1" = "quickjs:solver with space.exe" ] || exit 7
            mode=native
            ;;
    esac
    shift
done
[ "$full" = yes ] || exit 8
if [ "$mode" = native ]; then
    [ "$reset" = yes ] && [ "$plugins" = no ] || exit 9
else
    [ "$reset" = no ] && [ "$plugins" = yes ] || exit 10
fi
printf '{"id":"dQw4w9WgXcQ","title":"%s","formats":[{"format_id":"18","url":"https://example.test/video","vcodec":"avc1","acodec":"mp4a"}]}\n' "$mode"
"#).unwrap();
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o700)).unwrap();

    let url = "https://www.youtube.com/watch?v=dQw4w9WgXcQ";
    let standard = extract_playback(url, Provider::YtDlp, &helper, false).await.unwrap();
    assert_eq!(standard.title, "standard");
    let native = extract_playback_with_ejs(url, Provider::YtDlp, &helper, false,
        Some(Path::new("solver with space.exe"))).await.unwrap();
    assert_eq!(native.title, "native");
    assert!(matches!(native.streams, Capability::Available(ref streams) if streams.len() == 1));
}
