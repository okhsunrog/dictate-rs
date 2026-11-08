//! Dictation provider adapter. Transport and protocol handling live in llm-relay.
use crate::{Config, SttProvider};
use anyhow::{Result, bail};
use llm_relay::wire::openai::ChatResponse;
use llm_relay::{
    AudioInput, ClientConfig, LlmError, SpeechClient, SpeechConfig, SpeechOptions, WireClient,
};
use serde_json::json;
use std::{future::Future, time::Duration};

const INSTRUCTIONS: &str = include_str!("../prompts/edit.txt");
const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

pub struct Proxy {
    speech: SpeechClient,
    editor: WireClient,
    config: Config,
    runtime: tokio::runtime::Runtime,
    cancel: tokio_util::sync::CancellationToken,
}

impl Proxy {
    pub fn new(config: Config) -> Result<Self> {
        let speech = match config.stt_provider {
            SttProvider::ChatgptProxy => {
                SpeechConfig::chatgpt_proxy(&config.base_url, &config.api_key)
            }
            SttProvider::Elevenlabs => {
                SpeechConfig::elevenlabs(&config.elevenlabs_api_key, "scribe_v2")
            }
        };
        config.validate()?;
        Ok(Self {
            speech: SpeechClient::new(
                speech
                    .timeout(Duration::from_secs(180))
                    .max_response_bytes(MAX_RESPONSE_BYTES),
            )
            .map_err(relay_error)?,
            editor: WireClient::new(
                ClientConfig::openai_compatible(&config.base_url, &config.api_key, &config.model)
                    .timeout(Duration::from_secs(180))
                    .max_response_bytes(MAX_RESPONSE_BYTES),
            )
            .map_err(relay_error)?,
            config,
            runtime: tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?,
            cancel: tokio_util::sync::CancellationToken::new(),
        })
    }

    fn wait<T>(&self, future: impl Future<Output = std::result::Result<T, LlmError>>) -> Result<T> {
        self.runtime.block_on(async {
            tokio::select! {
                biased;
                _ = self.cancel.cancelled() => bail!("Processing cancelled"),
                result = future => result.map_err(relay_error),
            }
        })
    }

    pub fn transcribe(&self, audio: Vec<u8>, filename: &str) -> Result<String> {
        if audio.is_empty() || audio.len() > 25 * 1024 * 1024 {
            bail!("Recording size must be between 1 byte and 25 MiB");
        }
        let format = std::path::Path::new(filename)
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("wav")
            .to_ascii_lowercase();
        let mime_type = match format.as_str() {
            "wav" => "audio/wav",
            "ogg" => "audio/ogg",
            "mp3" => "audio/mpeg",
            "flac" => "audio/flac",
            "m4a" | "mp4" => "audio/mp4",
            "webm" => "audio/webm",
            "aac" => "audio/aac",
            _ => "application/octet-stream",
        }
        .to_owned();
        let audio = AudioInput {
            bytes: audio,
            filename: filename.to_owned(),
            mime_type,
            format,
        };
        // Old configs can contain BCP-47 tags such as en-US; STT accepts ISO 639.
        let language = self
            .config
            .language
            .split('-')
            .next()
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        let transcript = self.wait(self.speech.transcribe(
            audio,
            SpeechOptions {
                language,
                ..Default::default()
            },
        ))?;
        let text = transcript.text.trim();
        if text.is_empty() {
            bail!("No speech recognized. Try another recording");
        }
        Ok(text.to_owned())
    }

    pub fn edit(&self, text: &str) -> Result<String> {
        let body = json!({
            "model": self.config.model, "stream": false, "store": false,
            "reasoning_effort": "none",
            "messages": [
                {"role":"system", "content": INSTRUCTIONS},
                {"role":"user", "content": text}
            ]
        });
        // One explicit attempt: retry remains a user action, also after billing.
        let response = self.wait(async {
            self.editor
                .chat_openai_attempt(&body)
                .await
                .map_err(|failure| failure.error)
        })?;
        edited_text(&response)
    }
}

impl crate::engine::Backend for Proxy {
    fn set_cancellation(&mut self, cancel: tokio_util::sync::CancellationToken) {
        self.cancel = cancel;
    }
    fn transcribe(&self, audio: Vec<u8>, filename: &str) -> Result<String> {
        Proxy::transcribe(self, audio, filename)
    }
    fn edit(&self, text: &str) -> Result<String> {
        Proxy::edit(self, text)
    }
}

// Library errors can contain response bodies or credential-bearing URLs. Only
// expose a bounded classification to the UI, never the upstream error text.
fn relay_error(error: LlmError) -> anyhow::Error {
    match error {
        LlmError::ApiError { status, .. } | LlmError::TranscriptionUnavailable { status, .. } => {
            anyhow::anyhow!("Service returned HTTP {status}")
        }
        LlmError::Config(_) => anyhow::anyhow!("Invalid service or audio parameters"),
        LlmError::ResponseTooLarge { .. } => anyhow::anyhow!("Service response is too large"),
        LlmError::Request(_) | LlmError::Client(_) => {
            anyhow::anyhow!("Could not reach the service. Recording retained; try again")
        }
        _ => anyhow::anyhow!("Service returned an invalid or empty response"),
    }
}

fn edited_text(response: &ChatResponse) -> Result<String> {
    let Some(choice) = response.choices.first() else {
        bail!("Model returned empty text");
    };
    if choice.finish_reason.as_deref() != Some("stop")
        || choice
            .message
            .tool_calls
            .as_ref()
            .is_some_and(|calls| !calls.is_empty())
    {
        bail!("Model did not complete editing");
    }
    let text = choice.message.content.as_deref().unwrap_or_default().trim();
    if text.is_empty() {
        bail!("Model returned empty text");
    }
    Ok(text.to_owned())
}
