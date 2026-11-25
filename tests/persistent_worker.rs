use std::{path::PathBuf, thread, time::Duration};
use yt_lite_extraction_proof::persistent::PersistentWorker;

#[test]
#[ignore = "requires a local Python interpreter; no network access is used"]
fn persistent_worker_reuses_process_and_recovers_from_stale_or_wrong_id() {
    let python = std::env::var_os("YT_LITE_TEST_PYTHON")
        .expect("Set YT_LITE_TEST_PYTHON to your Python executable");
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/persistent_worker.py");
    let worker = PersistentWorker::start(PathBuf::from(python), script, false);
    let url = |id: &str| format!("https://www.youtube.com/watch?v={id}");
    let first = worker.submit(&url("aaaaaaaaaaa")).resolve().unwrap();
    let second = worker.submit(&url("bbbbbbbbbbb")).resolve().unwrap();
    assert_eq!(first.title, second.title, "worker must survive a successful request");
    assert_eq!(second.video_id, "bbbbbbbbbbb");

    let stale = worker.submit(&url("ccccccccccc"));
    thread::sleep(Duration::from_millis(200));
    let next = worker.submit(&url("bbbbbbbbbbb"));
    assert!(!stale.is_current());
    assert!(stale.resolve().is_err());
    let next_result = next.resolve().unwrap();
    assert_eq!(next_result.video_id, "bbbbbbbbbbb");
    let bad = worker.submit(&url("ddddddddddd")).resolve();
    assert_eq!(bad.err().unwrap().code, "persistent_id_mismatch");
    let recovered = worker.submit(&url("aaaaaaaaaaa")).resolve().unwrap();
    assert_ne!(next_result.title, recovered.title, "wrong reply ID must invalidate the process");
    assert_eq!(recovered.video_id, "aaaaaaaaaaa");
}
