use crate::Config;
use anyhow::{Context, Result, bail};
use reqwest::{Client, Response, multipart};
use serde_json::{Value, json};
use std::time::Duration;

const INSTRUCTIONS: &str = include_str!("../prompts/edit.txt");

pub struct Proxy {
    client: Client,
    config: Config,
    runtime: tokio::runtime::Runtime,
    cancel: tokio_util::sync::CancellationToken,
}
impl Proxy {
    pub fn new(config: Config) -> Result<Self> {
        config.validate()?;
        Ok(Self {
            config,
            runtime: tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?,
            cancel: tokio_util::sync::CancellationToken::new(),
            client: Client::builder()
                .connect_timeout(Duration::from_secs(15))
                .timeout(Duration::from_secs(180))
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
        })
    }
    fn request(&self, request: reqwest::RequestBuilder) -> Result<Value> {
        self.runtime.block_on(async {
            tokio::select! {
                biased;
                _ = self.cancel.cancelled() => bail!("Обработка остановлена"),
                result = async {
                    let response = request.send().await.context("Не удалось связаться с прокси. Запись сохранена; попробуй ещё раз")?;
                    response_json(response).await
                } => result,
            }
        })
    }
    fn url(&self, route: &str) -> String {
        format!("{}/{}", self.config.base_url.trim_end_matches('/'), route)
    }
    pub fn transcribe(&self, audio: Vec<u8>, filename: &str) -> Result<String> {
        if audio.is_empty() || audio.len() > 25 * 1024 * 1024 {
            bail!("Размер записи должен быть от 1 байта до 25 МиБ");
        }
        let mut form = multipart::Form::new()
            .text("model", "chatgpt-transcribe")
            .text("response_format", "json")
            .part(
                "file",
                multipart::Part::bytes(audio).file_name(filename.to_owned()),
            );
        if !self.config.language.is_empty() {
            form = form.text("language", self.config.language.clone());
        }
        let value = self.request(
            self.client
                .post(self.url("audio/transcriptions"))
                .bearer_auth(&self.config.api_key)
                .multipart(form),
        )?;
        let text = value["text"]
            .as_str()
            .context("В ответе распознавания нет текста")?
            .trim();
        if text.is_empty() {
            bail!("Речь не распознана. Попробуй другую запись");
        }
        Ok(text.to_owned())
    }
    pub fn edit(&self, text: &str) -> Result<String> {
        let value = self.request(
            self.client
                .post(self.url("responses"))
                .bearer_auth(&self.config.api_key)
                .json(&json!({
                    "model": self.config.model, "stream": false, "store": false,
                    "reasoning": {"effort":"none"}, "instructions": INSTRUCTIONS,
                    "input": [{"role":"user", "content": [{"type":"input_text","text":text}]}]
                })),
        )?;
        edited_text(&value)
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
async fn response_json(mut response: Response) -> Result<Value> {
    let status = response.status();
    // Never expose response bodies: they can echo credentials or dictation.
    if !status.is_success() {
        bail!("Прокси вернул HTTP {}", status.as_u16());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if bytes.len() + chunk.len() > 4 * 1024 * 1024 {
            bail!("Ответ прокси слишком большой");
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).context("Прокси вернул некорректный JSON")
}
pub fn edited_text(value: &Value) -> Result<String> {
    if value["status"].as_str().is_some_and(|s| s != "completed") {
        bail!("Модель не завершила редактирование");
    }
    let text = value["output"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| item["type"] == "message")
        .flat_map(|item| item["content"].as_array().into_iter().flatten())
        .filter(|part| part["type"] == "output_text")
        .filter_map(|part| part["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    if text.trim().is_empty() {
        bail!("Модель вернула пустой текст");
    }
    Ok(text.trim().to_owned())
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    #[test]
    fn excludes_reasoning_and_rejects_partial_results() {
        let mut value = json!({"status":"completed","output":[{"type":"reasoning","content":[{"type":"output_text","text":"secret"}]},{"type":"message","content":[{"type":"output_text","text":"Привет!"}]}]});
        assert_eq!(edited_text(&value).unwrap(), "Привет!");
        value["status"] = json!("incomplete");
        assert!(edited_text(&value).is_err());
    }

    /// Serve `count` canned responses `(status_line, json_body)` on localhost,
    /// reading each full request first so the client always gets its reply.
    fn mock_server(
        responses: Vec<(&'static str, String)>,
    ) -> (String, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            for (status, body) in responses {
                let (mut stream, _) = listener.accept().unwrap();
                // Read headers, then the body per Content-Length.
                let mut buf = Vec::new();
                let mut tmp = [0u8; 1024];
                let header_end = loop {
                    let n = stream.read(&mut tmp).unwrap();
                    if n == 0 {
                        break buf.len();
                    }
                    buf.extend_from_slice(&tmp[..n]);
                    if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        break pos + 4;
                    }
                };
                let headers = String::from_utf8_lossy(&buf[..header_end]).to_lowercase();
                let content_len = headers
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length:"))
                    .and_then(|v| v.trim().parse::<usize>().ok())
                    .unwrap_or(0);
                while buf.len() < header_end + content_len {
                    let n = stream.read(&mut tmp).unwrap();
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&tmp[..n]);
                }
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
            }
        });
        (format!("http://{addr}/v1"), handle)
    }

    fn config_for(url: String) -> Config {
        Config {
            base_url: url,
            api_key: "test-key".into(),
            model: "test-model".into(),
            ..Config::default()
        }
    }

    #[test]
    fn transcribe_and_edit_against_mock() {
        let (url, handle) = mock_server(vec![
            ("200 OK", json!({"text": "  сырой текст  "}).to_string()),
            ("200 OK", json!({"status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"Готовый текст."}]}]}).to_string()),
        ]);
        let proxy = Proxy::new(config_for(url)).unwrap();
        assert_eq!(
            proxy.transcribe(vec![1, 2, 3, 4], "a.wav").unwrap(),
            "сырой текст"
        );
        assert_eq!(proxy.edit("сырой текст").unwrap(), "Готовый текст.");
        handle.join().unwrap();
    }

    #[test]
    fn http_error_is_reported_without_body() {
        let (url, handle) = mock_server(vec![(
            "500 Internal Server Error",
            "server exploded".into(),
        )]);
        let proxy = Proxy::new(config_for(url)).unwrap();
        let err = proxy
            .transcribe(vec![1, 2, 3], "a.wav")
            .unwrap_err()
            .to_string();
        assert!(err.contains("500"), "surfaces status: {err}");
        assert!(
            !err.contains("exploded"),
            "must not leak upstream body: {err}"
        );
        handle.join().unwrap();
    }

    #[test]
    fn empty_transcript_is_rejected() {
        let (url, handle) = mock_server(vec![("200 OK", json!({"text": "   "}).to_string())]);
        let proxy = Proxy::new(config_for(url)).unwrap();
        assert!(proxy.transcribe(vec![1, 2, 3], "a.wav").is_err());
        handle.join().unwrap();
    }

    #[test]
    fn oversized_audio_rejected_before_network() {
        // No mock: must fail on the size guard, not by contacting anyone.
        let proxy = Proxy::new(config_for("http://127.0.0.1:9/v1".into())).unwrap();
        assert!(
            proxy
                .transcribe(vec![0u8; 26 * 1024 * 1024], "a.wav")
                .is_err()
        );
        assert!(proxy.transcribe(vec![], "a.wav").is_err());
    }
    #[test]
    fn cancellation_aborts_a_pending_http_response() {
        use crate::engine::Backend;
        use std::sync::mpsc;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/v1", listener.local_addr().unwrap());
        let (accepted_tx, accepted_rx) = mpsc::channel();
        let (stop_tx, stop_rx) = mpsc::channel();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0; 4096];
            assert!(stream.read(&mut buf).unwrap() > 0);
            accepted_tx.send(()).unwrap();
            // No response until the test releases the fixture.
            let _ = stop_rx.recv_timeout(Duration::from_secs(5));
        });
        let cancel = tokio_util::sync::CancellationToken::new();
        let token = cancel.clone();
        let (done_tx, done_rx) = mpsc::channel();
        let client = std::thread::spawn(move || {
            let mut proxy = Proxy::new(config_for(url)).unwrap();
            proxy.set_cancellation(token);
            done_tx
                .send(proxy.transcribe(vec![1, 2], "a.wav").is_err())
                .unwrap();
        });
        accepted_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        cancel.cancel();
        let result = done_rx.recv_timeout(Duration::from_secs(2));
        let _ = stop_tx.send(());
        server.join().unwrap();
        client.join().unwrap();
        assert!(
            result.unwrap(),
            "cancel must finish before the HTTP timeout"
        );
    }
}
