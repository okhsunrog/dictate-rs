//! Toolkit-independent recording/processing engine.
//!
//! A single worker thread owns all mutable state and drives one recording plus
//! one cancellable processing job. The UI only sends
//! [`Command`]s (via [`Engine`]) and reacts to [`Event`]s; it never touches the
//! recorder, the proxy, or any generation bookkeeping.
//!
//! Recording and network access are abstracted behind [`Recorder`] and
//! [`Backend`] so the state machine can be exercised in tests with fakes,
//! without a microphone or paid network calls.

use crate::{Config, Proxy, audio::Recording};
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Sender},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

use crate::audio::MAX_SECONDS;
use tokio_util::sync::CancellationToken;

/// Coarse pipeline stage, shown to the user as a status/indicator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Recording,
    Transcribing,
    Editing,
}

/// Everything the worker reports back to the UI.
#[derive(Debug, Clone)]
pub enum Event {
    Phase(Phase),
    /// The recording is in memory and can be retried even without a result.
    Captured,
    Transcript(String),
    Cancelled {
        recording: bool,
    },
    Meter {
        seconds: u64,
        level: f32,
    },
    Result {
        original: String,
        edited: String,
        warning: Option<String>,
    },
    Error(String),
}

/// Captures a recording and turns it into WAV bytes. Implemented by the real
/// PipeWire recorder and by fakes in tests.
pub trait Recorder: Send {
    /// Latest RMS level in `0.0..=1.0`, for the live meter.
    fn level(&self) -> f32;
    /// Whether the capture process has exited on its own (mic unplugged, etc.).
    fn has_exited(&mut self) -> bool;
    /// Stop capture and encode the collected samples as a WAV file.
    fn finish(self: Box<Self>) -> anyhow::Result<Vec<u8>>;
}

/// Remote transcription + editing. Implemented by [`Proxy`] and by fakes.
pub trait Backend: Send {
    fn set_cancellation(&mut self, _cancel: CancellationToken) {}
    fn transcribe(&self, audio: Vec<u8>, filename: &str) -> anyhow::Result<String>;
    fn edit(&self, text: &str) -> anyhow::Result<String>;
}

/// Builds a recorder for the given PipeWire target (empty = system default).
pub type RecorderFactory =
    Arc<dyn Fn(&str) -> anyhow::Result<Box<dyn Recorder>> + Send + Sync + 'static>;
/// Builds a backend for a given config; called once per processing job.
pub type BackendFactory =
    Arc<dyn Fn(Config) -> anyhow::Result<Box<dyn Backend>> + Send + Sync + 'static>;

/// Handle used by the UI to drive the worker. Cheap to clone.
#[derive(Clone)]
pub struct Engine {
    tx: Sender<Command>,
    worker: Arc<std::sync::Mutex<Option<JoinHandle<()>>>>,
}

enum Command {
    Toggle(Config),
    Cancel,
    Retry(Config),
    Shutdown,
    Progress(u64, Phase),
    Transcript(u64, String),
    Finished(u64, Result<(String, String, Option<String>), String>),
}

impl Engine {
    /// Start with the real PipeWire recorder and HTTP proxy backend.
    ///
    /// `emit` is only ever invoked from the single worker thread, so it needs
    /// `Send` (to move onto that thread) but not `Sync`.
    pub fn with_defaults(emit: impl Fn(Event) + Send + 'static) -> Self {
        let recorder: RecorderFactory =
            Arc::new(|target| Ok(Box::new(Recording::start(target)?) as Box<dyn Recorder>));
        let backend: BackendFactory =
            Arc::new(|config| Ok(Box::new(Proxy::new(config)?) as Box<dyn Backend>));
        Self::start(recorder, backend, emit)
    }

    /// Start with injectable recorder/backend factories (used by tests).
    pub fn start(
        recorder_factory: RecorderFactory,
        backend_factory: BackendFactory,
        emit: impl Fn(Event) + Send + 'static,
    ) -> Self {
        let (tx, rx) = mpsc::channel();
        let worker_tx = tx.clone();
        let emit: Box<dyn Fn(Event) + Send> = Box::new(emit);
        // Shared with worker job threads so a cancel/new job can tell an
        // in-flight job to suppress stale events; the token also aborts HTTP I/O.
        let generation = Arc::new(AtomicU64::new(0));
        let handle = std::thread::spawn(move || {
            worker_loop(
                rx,
                worker_tx,
                emit,
                recorder_factory,
                backend_factory,
                generation,
            );
        });
        Self {
            tx,
            worker: Arc::new(std::sync::Mutex::new(Some(handle))),
        }
    }

    pub fn toggle(&self, config: Config) {
        let _ = self.tx.send(Command::Toggle(config));
    }
    pub fn retry(&self, config: Config) {
        let _ = self.tx.send(Command::Retry(config));
    }
    pub fn cancel(&self) {
        let _ = self.tx.send(Command::Cancel);
    }
    /// Ask the worker to stop and block until it (and thus any live recording)
    /// has torn down, so no orphan `pw-record` outlives the process. Safe to
    /// call more than once; only the first call joins.
    pub fn shutdown(&self) {
        let _ = self.tx.send(Command::Shutdown);
        if let Some(handle) = self.worker.lock().ok().and_then(|mut g| g.take()) {
            let _ = handle.join();
        }
    }
}

/// The worker's owned state. Kept small and driven entirely by messages.
struct Worker {
    emit: Box<dyn Fn(Event) + Send>,
    tx: Sender<Command>,
    recorder_factory: RecorderFactory,
    backend_factory: BackendFactory,
    generation: Arc<AtomicU64>,
    recording: Option<Box<dyn Recorder>>,
    started: Instant,
    config: Config,
    audio: Option<Vec<u8>>,
    busy: bool,
    cancellation: CancellationToken,
    transcript: Option<String>,
}

impl Worker {
    /// Invalidate any in-flight job and pending recording result.
    fn bump(&mut self) -> u64 {
        self.cancellation.cancel();
        self.cancellation = CancellationToken::new();
        self.generation.fetch_add(1, Ordering::AcqRel) + 1
    }
    fn current(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }
}

fn worker_loop(
    rx: mpsc::Receiver<Command>,
    tx: Sender<Command>,
    emit: Box<dyn Fn(Event) + Send>,
    recorder_factory: RecorderFactory,
    backend_factory: BackendFactory,
    generation: Arc<AtomicU64>,
) {
    let mut w = Worker {
        emit,
        tx,
        recorder_factory,
        backend_factory,
        generation,
        recording: None,
        started: Instant::now(),
        config: Config::default(),
        audio: None,
        busy: false,
        cancellation: CancellationToken::new(),
        transcript: None,
    };
    loop {
        let message = match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(m) => m,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                tick(&mut w);
                continue;
            }
        };
        match message {
            Command::Shutdown => break,
            Command::Cancel => {
                w.bump();
                let recording = w.recording.is_some();
                w.recording = None; // Drop stops the capture process.
                w.busy = false;
                (w.emit)(Event::Phase(Phase::Idle));
                (w.emit)(Event::Cancelled { recording });
            }
            Command::Progress(id, phase) => {
                if id == w.current() {
                    (w.emit)(Event::Phase(phase));
                }
            }
            Command::Transcript(id, text) => {
                if id == w.current() {
                    w.transcript = Some(text.clone());
                    (w.emit)(Event::Transcript(text));
                }
            }
            Command::Finished(id, result) => {
                if id != w.current() {
                    continue; // Stale job: cancelled or superseded.
                }
                w.busy = false;
                (w.emit)(Event::Phase(Phase::Idle));
                match result {
                    Ok((original, edited, warning)) => (w.emit)(Event::Result {
                        original,
                        edited,
                        warning,
                    }),
                    Err(e) => (w.emit)(Event::Error(e)),
                }
            }
            Command::Toggle(next) => on_toggle(&mut w, next),
            Command::Retry(next) => on_retry(&mut w, next),
        }
    }
    // Break path: dropping the recorder stops any live capture before we return.
    w.cancellation.cancel();
    w.recording = None;
}

/// Periodic housekeeping while idle or recording.
fn tick(w: &mut Worker) {
    let Some(r) = &mut w.recording else { return };
    if r.has_exited() {
        w.recording = None;
        // Idle first, then the error, so the UI's Idle status doesn't clobber it.
        (w.emit)(Event::Phase(Phase::Idle));
        (w.emit)(Event::Error(
            "Microphone disconnected or PipeWire stopped recording".into(),
        ));
    } else if w.started.elapsed().as_secs() >= MAX_SECONDS {
        // Hit the length cap: stop and process what we have.
        let config = w.config.clone();
        let _ = w.tx.send(Command::Toggle(config));
    } else {
        (w.emit)(Event::Meter {
            seconds: w.started.elapsed().as_secs(),
            level: r.level(),
        });
    }
}

fn on_toggle(w: &mut Worker, next: Config) {
    if w.busy {
        return;
    }
    if let Some(r) = w.recording.take() {
        // Second press: stop recording and start processing.
        match r.finish() {
            Ok(bytes) => w.audio = Some(bytes),
            Err(e) => {
                // Idle first, then the error, so the UI keeps the message.
                (w.emit)(Event::Phase(Phase::Idle));
                (w.emit)(Event::Error(e.to_string()));
                return;
            }
        }
    } else {
        // Capture is local: configuration/network errors must never prevent
        // the user from recording. Validate only after audio is safely kept.
        w.config = next.effective();
        match (w.recorder_factory)(&w.config.microphone) {
            Ok(r) => {
                w.bump();
                w.audio = None;
                w.transcript = None;
                w.started = Instant::now();
                w.recording = Some(r);
                (w.emit)(Event::Phase(Phase::Recording));
            }
            Err(e) => (w.emit)(Event::Error(e.to_string())),
        }
        return;
    }
    (w.emit)(Event::Captured);
    on_retry(w, w.config.clone());
}

fn on_retry(w: &mut Worker, next: Config) {
    if w.busy || w.recording.is_some() {
        return;
    }
    match w.audio.clone() {
        Some(bytes) => {
            let effective = next.effective();
            if let Err(e) = effective.validate() {
                (w.emit)(Event::Phase(Phase::Idle));
                (w.emit)(Event::Error(format!(
                    "{e}. Recording retained; configure the connection and retry"
                )));
                return;
            }
            w.config = effective.clone();
            let id = w.bump();
            w.busy = true;
            spawn_job(w, id, effective, bytes);
        }
        None => (w.emit)(Event::Error("No recording available to retry".into())),
    }
}

/// Run one transcription+editing job on its own thread. `id` is the generation
/// this job belongs to; if the worker's generation moves on (cancel/new job)
/// the job stops between stages instead of doing wasted work, and any late
/// result is dropped by the `id` check in [`worker_loop`].
fn spawn_job(w: &Worker, id: u64, config: Config, audio: Vec<u8>) {
    let tx = w.tx.clone();
    let backend_factory = w.backend_factory.clone();
    let generation = w.generation.clone();
    let cancel = w.cancellation.clone();
    let transcript = w.transcript.clone();
    let phase = if transcript.is_some() {
        Phase::Editing
    } else {
        Phase::Transcribing
    };
    (w.emit)(Event::Phase(phase));
    std::thread::spawn(move || {
        let live = || generation.load(Ordering::Acquire) == id;
        let result = (|| -> anyhow::Result<(String, String, Option<String>)> {
            let mut backend = (backend_factory)(config)?;
            backend.set_cancellation(cancel.clone());
            let original = match transcript {
                Some(text) => text,
                None => backend.transcribe(audio, "dictation.wav")?,
            };
            if !live() {
                anyhow::bail!("__cancelled__");
            }
            let _ = tx.send(Command::Transcript(id, original.clone()));
            let _ = tx.send(Command::Progress(id, Phase::Editing));
            let (edited, warning) = match backend.edit(&original) {
                Ok(text) => (text, None),
                Err(e) => (
                    original.clone(),
                    Some(format!(
                        "Editing failed: {e}. Original transcript retained."
                    )),
                ),
            };
            Ok((original, edited, warning))
        })()
        .map_err(|e| e.to_string());
        // Don't bother reporting a job the worker has already abandoned.
        if live() {
            let _ = tx.send(Command::Finished(id, result));
        } else {
            let _ = tx.send(Command::Finished(id, Err(String::new())));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::{Receiver, RecvTimeoutError};

    /// A recorder that yields preset bytes and never exits on its own.
    struct FakeRecorder {
        level: f32,
        bytes: Vec<u8>,
        fail: bool,
    }
    impl Recorder for FakeRecorder {
        fn level(&self) -> f32 {
            self.level
        }
        fn has_exited(&mut self) -> bool {
            false
        }
        fn finish(self: Box<Self>) -> anyhow::Result<Vec<u8>> {
            if self.fail {
                anyhow::bail!("mic failed");
            }
            Ok(self.bytes)
        }
    }

    struct FakeBackend {
        transcript: String,
        edited: Result<String, String>,
        delay: Duration,
    }
    impl Backend for FakeBackend {
        fn transcribe(&self, _audio: Vec<u8>, _filename: &str) -> anyhow::Result<String> {
            std::thread::sleep(self.delay);
            Ok(self.transcript.clone())
        }
        fn edit(&self, _text: &str) -> anyhow::Result<String> {
            std::thread::sleep(self.delay);
            self.edited.clone().map_err(|e| anyhow::anyhow!(e))
        }
    }

    fn valid_config() -> Config {
        Config {
            base_url: "https://proxy.example.com/v1".into(),
            api_key: "k".into(),
            model: "test-model".into(),
            ..Config::default()
        }
    }

    /// Collect events with a small helper that times out rather than hanging.
    struct Sink(Receiver<Event>);
    impl Sink {
        fn next(&self) -> Option<Event> {
            self.0.recv_timeout(Duration::from_secs(5)).ok()
        }
        /// Wait for the next Phase event, ignoring meters.
        fn next_phase(&self) -> Option<Phase> {
            loop {
                match self.0.recv_timeout(Duration::from_secs(5)) {
                    Ok(Event::Phase(p)) => return Some(p),
                    Ok(_) => continue,
                    Err(_) => return None,
                }
            }
        }
        fn drain_for(&self, dur: Duration) -> Vec<Event> {
            let mut out = Vec::new();
            let deadline = Instant::now() + dur;
            loop {
                let now = Instant::now();
                if now >= deadline {
                    break;
                }
                match self.0.recv_timeout(deadline - now) {
                    Ok(e) => out.push(e),
                    Err(RecvTimeoutError::Timeout) => break,
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }
            out
        }
    }

    fn engine_with(
        recorder: impl Fn(&str) -> anyhow::Result<Box<dyn Recorder>> + Send + Sync + 'static,
        backend: impl Fn(Config) -> anyhow::Result<Box<dyn Backend>> + Send + Sync + 'static,
    ) -> (Engine, Sink) {
        let (etx, erx) = mpsc::channel();
        let engine = Engine::start(Arc::new(recorder), Arc::new(backend), move |e| {
            let _ = etx.send(e);
        });
        (engine, Sink(erx))
    }

    #[test]
    fn happy_path_records_then_reports_result() {
        let (engine, sink) = engine_with(
            |_| {
                Ok(Box::new(FakeRecorder {
                    level: 0.5,
                    bytes: vec![1, 2, 3],
                    fail: false,
                }))
            },
            |_| {
                Ok(Box::new(FakeBackend {
                    transcript: "raw text".into(),
                    edited: Ok("Edited text.".into()),
                    delay: Duration::ZERO,
                }))
            },
        );
        engine.toggle(valid_config());
        assert_eq!(sink.next_phase(), Some(Phase::Recording));
        engine.toggle(valid_config());
        assert_eq!(sink.next_phase(), Some(Phase::Transcribing));
        assert_eq!(sink.next_phase(), Some(Phase::Editing));
        assert_eq!(sink.next_phase(), Some(Phase::Idle));
        match sink.next() {
            Some(Event::Result {
                original,
                edited,
                warning,
            }) => {
                assert_eq!(original, "raw text");
                assert_eq!(edited, "Edited text.");
                assert!(warning.is_none());
            }
            other => panic!("expected result, got {other:?}"),
        }
        engine.shutdown();
    }

    #[test]
    fn edit_failure_keeps_transcript_with_warning() {
        let (engine, sink) = engine_with(
            |_| {
                Ok(Box::new(FakeRecorder {
                    level: 0.1,
                    bytes: vec![9],
                    fail: false,
                }))
            },
            |_| {
                Ok(Box::new(FakeBackend {
                    transcript: "original".into(),
                    edited: Err("HTTP 500".into()),
                    delay: Duration::ZERO,
                }))
            },
        );
        engine.toggle(valid_config());
        assert_eq!(sink.next_phase(), Some(Phase::Recording));
        engine.toggle(valid_config());
        // Skip to the terminal result.
        let mut result = None;
        for _ in 0..10 {
            match sink.next() {
                Some(Event::Result {
                    original,
                    edited,
                    warning,
                }) => {
                    result = Some((original, edited, warning));
                    break;
                }
                Some(_) => continue,
                None => break,
            }
        }
        let (original, edited, warning) = result.expect("result event");
        assert_eq!(original, "original");
        assert_eq!(edited, "original", "falls back to transcript");
        assert!(warning.is_some(), "warns that editing failed");
        engine.shutdown();
    }

    #[test]
    fn cancel_during_processing_suppresses_stale_result() {
        let (engine, sink) = engine_with(
            |_| {
                Ok(Box::new(FakeRecorder {
                    level: 0.1,
                    bytes: vec![1],
                    fail: false,
                }))
            },
            |_| {
                Ok(Box::new(FakeBackend {
                    transcript: "late".into(),
                    edited: Ok("late result".into()),
                    delay: Duration::from_millis(300),
                }))
            },
        );
        engine.toggle(valid_config());
        assert_eq!(sink.next_phase(), Some(Phase::Recording));
        engine.toggle(valid_config());
        assert_eq!(sink.next_phase(), Some(Phase::Transcribing));
        // Cancel while transcription is still sleeping.
        engine.cancel();
        // We should see Idle from the cancel and then NO Result afterwards.
        let events = sink.drain_for(Duration::from_millis(700));
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::Phase(Phase::Idle))),
            "cancel emits Idle"
        );
        assert!(
            !events.iter().any(|e| matches!(e, Event::Result { .. })),
            "stale result must be suppressed, got {events:?}"
        );
        engine.shutdown();
    }

    #[test]
    fn missing_key_does_not_prevent_capture_and_audio_can_be_retried() {
        let (engine, sink) = engine_with(
            |_| {
                Ok(Box::new(FakeRecorder {
                    level: 0.0,
                    bytes: vec![],
                    fail: false,
                }))
            },
            |_| {
                Ok(Box::new(FakeBackend {
                    transcript: String::new(),
                    edited: Ok(String::new()),
                    delay: Duration::ZERO,
                }))
            },
        );
        engine.toggle(Config::default());
        assert_eq!(sink.next_phase(), Some(Phase::Recording));
        engine.toggle(Config::default());
        assert!(matches!(sink.next(), Some(Event::Captured)));
        assert_eq!(sink.next_phase(), Some(Phase::Idle));
        assert!(matches!(sink.next(), Some(Event::Error(_))));
        engine.retry(valid_config());
        assert_eq!(sink.next_phase(), Some(Phase::Transcribing));
        engine.shutdown();
    }

    #[test]
    fn recorder_finish_failure_reports_error() {
        let (engine, sink) = engine_with(
            |_| {
                Ok(Box::new(FakeRecorder {
                    level: 0.0,
                    bytes: vec![],
                    fail: true,
                }))
            },
            |_| {
                Ok(Box::new(FakeBackend {
                    transcript: String::new(),
                    edited: Ok(String::new()),
                    delay: Duration::ZERO,
                }))
            },
        );
        engine.toggle(valid_config());
        assert_eq!(sink.next_phase(), Some(Phase::Recording));
        engine.toggle(valid_config());
        // finish() fails -> Idle then Error, so the UI keeps the message.
        let events = sink.drain_for(Duration::from_millis(300));
        let idle_pos = events
            .iter()
            .position(|e| matches!(e, Event::Phase(Phase::Idle)));
        let err_pos = events.iter().position(|e| matches!(e, Event::Error(_)));
        assert!(err_pos.is_some(), "an error is reported");
        assert!(idle_pos.is_some(), "returns to idle");
        assert!(
            idle_pos < err_pos,
            "Idle must precede Error so the error isn't overwritten: {events:?}"
        );
        engine.shutdown();
    }
    #[test]
    fn retry_after_editor_failure_does_not_transcribe_again() {
        use std::sync::atomic::AtomicUsize;
        struct Counted(Arc<AtomicUsize>);
        impl Backend for Counted {
            fn transcribe(&self, _: Vec<u8>, _: &str) -> anyhow::Result<String> {
                self.0.fetch_add(1, Ordering::SeqCst);
                Ok("original".into())
            }
            fn edit(&self, _: &str) -> anyhow::Result<String> {
                anyhow::bail!("offline")
            }
        }
        let count = Arc::new(AtomicUsize::new(0));
        let c = count.clone();
        let (engine, sink) = engine_with(
            |_| {
                Ok(Box::new(FakeRecorder {
                    level: 0.0,
                    bytes: vec![1],
                    fail: false,
                }))
            },
            move |_| Ok(Box::new(Counted(c.clone()))),
        );
        engine.toggle(valid_config());
        assert_eq!(sink.next_phase(), Some(Phase::Recording));
        engine.toggle(valid_config());
        for _ in 0..2 {
            loop {
                match sink.next().expect("terminal result") {
                    Event::Result { warning, .. } => {
                        assert!(warning.is_some());
                        break;
                    }
                    Event::Error(e) => panic!("unexpected error: {e}"),
                    _ => {}
                }
            }
            engine.retry(valid_config());
        }
        engine.shutdown();
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn repeated_cancel_does_not_block_the_next_recording() {
        let (engine, sink) = engine_with(
            |_| {
                Ok(Box::new(FakeRecorder {
                    level: 0.0,
                    bytes: vec![1],
                    fail: false,
                }))
            },
            |_| {
                Ok(Box::new(FakeBackend {
                    transcript: "text".into(),
                    edited: Ok("Text".into()),
                    delay: Duration::from_millis(300),
                }))
            },
        );
        for _ in 0..3 {
            engine.toggle(valid_config());
            assert_eq!(sink.next_phase(), Some(Phase::Recording));
            engine.toggle(valid_config());
            assert_eq!(sink.next_phase(), Some(Phase::Transcribing));
            engine.cancel();
            assert_eq!(sink.next_phase(), Some(Phase::Idle));
        }
        engine.shutdown();
    }
}
