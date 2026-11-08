//! Manual end-to-end check against the real proxy.
//!
//! Usage:
//!   cargo run -p dictate-core --example e2e -- /path/to/audio.ogg [ru|en]
//! Uses the saved configuration and DICTATE_BASE_URL / DICTATE_API_KEY overrides.
//!
//! Prints the raw transcript and the edited result. It never prints the key.

use dictate_core::{Config, Proxy};
use std::time::Instant;

fn main() -> anyhow::Result<()> {
    let path = std::env::args()
        .nth(1)
        .expect("provide an audio file path as the first argument");
    let audio = std::fs::read(&path)?;
    let filename = std::path::Path::new(&path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("audio.ogg")
        .to_owned();

    let mut config = Config::load()?.effective();
    if let Some(language) = std::env::args().nth(2) {
        config.language = language;
    }
    let proxy = Proxy::new(config)?;

    let t0 = Instant::now();
    let original = proxy.transcribe(audio, &filename)?;
    let t1 = Instant::now();
    println!("--- ORIGINAL ---\n{original}");
    println!("[transcribe: {:.2}s]", (t1 - t0).as_secs_f32());

    let edited = proxy.edit(&original)?;
    let t2 = Instant::now();
    println!("--- EDITED ---\n{edited}");
    println!("[edit: {:.2}s]", (t2 - t1).as_secs_f32());
    Ok(())
}
