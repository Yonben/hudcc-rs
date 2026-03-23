mod json;
mod ansi;
mod time;
mod cache;
mod config;
mod stdin;
mod api;
mod version;
mod transcript;
mod render;

use std::thread;

fn main() {
    let result = std::panic::catch_unwind(run);
    match result {
        Ok(()) => {}
        Err(e) => {
            let msg = if let Some(s) = e.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = e.downcast_ref::<String>() {
                s.clone()
            } else {
                "unknown error".to_string()
            };
            println!("[HUD] error: {}", msg);
        }
    }
}

fn run() {
    let stdin_data = match stdin::read_stdin() {
        Some(d) => d,
        None => {
            println!("{}[HUD] waiting for data...{}", ansi::DIM, ansi::RESET);
            return;
        }
    };

    let config = config::read_config();

    let debug_enabled = std::env::var("DEBUG_USAGE")
        .ok()
        .map(|v| v == "1")
        .unwrap_or(false);

    let transcript_path = stdin_data.transcript_path.clone();

    let usage_handle = thread::spawn(move || api::get_usage(debug_enabled));

    let transcript_handle = thread::spawn(move || match transcript_path {
        Some(ref p) => transcript::parse_transcript(p),
        None => transcript::TranscriptData {
            session_start: None,
            agents: vec![],
            todos: vec![],
        },
    });

    let version_handle = thread::spawn(|| version::get_latest_version());

    let usage = usage_handle.join().unwrap_or(None);
    let transcript_data =
        transcript_handle
            .join()
            .unwrap_or_else(|_| transcript::TranscriptData {
                session_start: None,
                agents: vec![],
                todos: vec![],
            });
    let latest_version = version_handle.join().unwrap_or(None);

    let output = render::render(
        usage.as_ref(),
        &transcript_data,
        &stdin_data,
        latest_version.as_deref(),
        &config,
    );

    print!("{}", output);
}
