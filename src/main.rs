use std::{env, path::PathBuf, process::ExitCode};
use yt_lite_extraction_proof::{extract, Provider};

#[tokio::main]
async fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let Some(url) = args.next() else {
        eprintln!("Usage: yt-lite-extraction-proof <youtube-video-url> [--provider auto|rust|yt-dlp] [--yt-dlp PATH]");
        return ExitCode::from(2);
    };
    let mut provider = Provider::Auto;
    let mut helper = PathBuf::from("yt-dlp");
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--provider" => match args.next().and_then(|value| Provider::parse(&value)) {
                Some(value) => provider = value,
                None => { eprintln!("Invalid --provider value"); return ExitCode::from(2); }
            },
            "--yt-dlp" => match args.next() {
                Some(value) => helper = PathBuf::from(value),
                None => { eprintln!("Missing --yt-dlp path"); return ExitCode::from(2); }
            },
            _ => { eprintln!("Unknown option: {flag}"); return ExitCode::from(2); }
        }
    }
    match extract(&url, provider, &helper).await {
        Ok(info) => { println!("{}", serde_json::to_string_pretty(&info).expect("serializable result")); ExitCode::SUCCESS }
        Err(error) => {
            eprintln!("{}", serde_json::to_string(&error).expect("serializable error"));
            ExitCode::FAILURE
        }
    }
}
