use yt_lite_extraction_proof::{parse_search_json, validated_search_query, SearchHit};

#[test]
fn search_results_only_include_valid_watchable_youtube_ids() {
    let json = r#"{
      "_type":"playlist",
      "entries":[
        {"id":"dQw4w9WgXcQ","title":"First result","channel":"Example Channel","duration":213},
        {"id":"fq-Xn7ZOZtg","title":"Second result","uploader":"Example Uploader","duration":null},
        {"id":"not-valid","title":"Invalid short ID"},
        {"id":"xxxxxxxxxxx","title":"Playlist result","_type":"playlist"},
        {"id":"yyyyyyyyyyy","title":""}
      ]
    }"#;
    assert_eq!(
        parse_search_json(json).unwrap(),
        vec![
            SearchHit {
                video_id: "dQw4w9WgXcQ".into(), title: "First result".into(),
                channel: Some("Example Channel".into()), duration_seconds: Some(213),
            },
            SearchHit {
                video_id: "fq-Xn7ZOZtg".into(), title: "Second result".into(),
                channel: Some("Example Uploader".into()), duration_seconds: None,
            },
        ]
    );
}

#[test]
fn empty_or_bad_searches_are_rejected_before_network_io() {
    assert!(validated_search_query("").is_err());
    assert!(validated_search_query(" \t\n ").is_err());
    assert!(validated_search_query(&"x".repeat(121)).is_err());
    assert!(validated_search_query("hello\nworld").is_err());
    assert_eq!(validated_search_query("  Rust desktop player  ").unwrap(), "Rust desktop player");
}

#[test]
fn empty_results_are_not_an_error_and_invalid_documents_are() {
    assert!(parse_search_json(r#"{"entries":[]}"#).unwrap().is_empty());
    assert!(parse_search_json(r#"{"title":"not search results"}"#).is_err());
    assert!(parse_search_json("[]").is_err());
    assert!(parse_search_json("this is not json").is_err());
}

#[test]
fn search_can_request_more_than_eight_results_and_limits_work() {
    use yt_lite_extraction_proof::{search_target, SEARCH_BATCH_SIZE, SEARCH_MAX_RESULTS};
    assert_eq!(SEARCH_BATCH_SIZE, 8);
    assert_eq!(search_target("  rust tutorials  ", 16).unwrap(), "ytsearch16:rust tutorials");
    assert!(search_target("test", 0).is_err());
    assert!(search_target("test", SEARCH_MAX_RESULTS + 1).is_err());
    let entries = (0..50)
        .map(|i| format!(r#"{{"id":"v{i:010}","title":"Video {i}"}}"#))
        .collect::<Vec<_>>().join(",");
    let json = format!(r#"{{"entries":[{entries}]}}"#);
    let parsed = parse_search_json(&json).unwrap();
    assert_eq!(parsed.len(), SEARCH_MAX_RESULTS);
    assert_eq!(parsed[15].title, "Video 15");
}
