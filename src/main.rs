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
    let debug_enabled = std::env::var("HUD_DEBUG").as_deref() == Ok("1");

    let stdin_data = match stdin::read_stdin(debug_enabled) {
        Some(d) => d,
        None => {
            println!("{}[HUD] waiting for data...{}", ansi::DIM, ansi::RESET);
            return;
        }
    };

    let config = config::read_config(debug_enabled);

    let no_network = std::env::var("HUD_NO_NETWORK")
        .ok()
        .map(|v| v == "1")
        .unwrap_or(false);

    let transcript_path = stdin_data.transcript_path.clone();

    let usage_handle = thread::spawn(move || {
        if no_network { None } else { api::get_usage(debug_enabled) }
    });

    let transcript_handle = thread::spawn(move || match transcript_path {
        Some(ref p) => transcript::parse_transcript(p),
        None => transcript::TranscriptData {
            session_start: None,
            agents: vec![],
            todos: vec![],
        },
    });

    let version_handle = thread::spawn(move || {
        if no_network { None } else { version::get_latest_version() }
    });

    let usage = match usage_handle.join() {
        Ok(v) => v,
        Err(e) => {
            if debug_enabled {
                let msg = if let Some(s) = e.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = e.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "unknown panic".to_string()
                };
                eprintln!("[hud] usage thread panicked: {}", msg);
            }
            None
        }
    };
    let transcript_data = match transcript_handle.join() {
        Ok(v) => v,
        Err(e) => {
            if debug_enabled {
                let msg = if let Some(s) = e.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = e.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "unknown panic".to_string()
                };
                eprintln!("[hud] transcript thread panicked: {}", msg);
            }
            transcript::TranscriptData {
                session_start: None,
                agents: vec![],
                todos: vec![],
            }
        }
    };
    let latest_version = match version_handle.join() {
        Ok(v) => v,
        Err(e) => {
            if debug_enabled {
                let msg = if let Some(s) = e.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = e.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "unknown panic".to_string()
                };
                eprintln!("[hud] version thread panicked: {}", msg);
            }
            None
        }
    };

    let output = render::render(
        usage.as_ref(),
        &transcript_data,
        &stdin_data,
        latest_version.as_deref(),
        &config,
    );

    print!("{}", output);
}
