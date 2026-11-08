//! Qt Bridge for Rust adapter between QML and `dictate-core`.
//!
//! Workers post events through `QThreadHandle`; all properties and their
//! notifications are updated on the Qt thread. Toolkit types stay out of the core.

use dictate_core::{
    Config, Engine, Event, OutputMode, Phase, SttProvider, clipboard, ipc,
    paste::{PasteEvent, PasteService},
};
use qtbridge::{QmlObject, qobject};
use std::os::unix::net::UnixListener;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// Socket name for the application instance.
pub const SOCKET: &str = "dictate";
const PHASE_IDLE: i32 = 0;
static INITIALIZED: AtomicBool = AtomicBool::new(false);

pub fn initialized() -> bool {
    INITIALIZED.load(Ordering::Relaxed)
}

/// Command handed to us by `main` before the QML engine starts.
static INITIAL_COMMAND: Mutex<Option<String>> = Mutex::new(None);
pub fn set_initial_command(cmd: &str) {
    *INITIAL_COMMAND.lock().unwrap() = Some(cmd.to_string());
}
fn take_initial_command() -> Option<String> {
    INITIAL_COMMAND.lock().unwrap().take()
}

/// The bound single-instance listener, moved from `main` to `initialize`.
static LISTENER: Mutex<Option<UnixListener>> = Mutex::new(None);
pub fn set_listener(listener: Option<UnixListener>) {
    *LISTENER.lock().unwrap() = listener;
}

/// UI state and Rust services owned by the QML bridge.
pub struct Bridge {
    phase: i32,
    status_text: String,
    seconds: i32,
    level: f64,
    edited_text: String,
    original_text: String,
    showing_original: bool,
    has_result: bool,
    osd_visible: bool,
    window_visible: bool,
    paste_ready: bool,
    paste_saved: bool,
    paste_error: bool,
    paste_status: String,
    paste_connecting: bool,
    cfg_base_url: String,
    cfg_api_key: String,
    cfg_model: String,
    cfg_stt_provider: i32,
    cfg_elevenlabs_api_key: String,
    cfg_language: String,
    cfg_microphone: String,
    microphones_json: String,
    cfg_output: i32,
    cfg_paste_shift: bool,
    settings_status: String,
    can_retry: bool,
    settings_visible: bool,
    error_text: String,
    history_json: String,
    history_index: i32,
    history: dictate_core::history::History,
    audio_available: bool,
    ignoring_results: bool,
    // Rust-only fields.
    initialized: bool,
    config: Config,
    delivery: OutputMode,
    /// One-shot output-mode override for the next recording (from a
    /// `toggle <mode>` invocation); cleared when recording starts.
    override_mode: Option<OutputMode>,
    engine: Option<Engine>,
    paste: Option<PasteService>,
}

impl Default for Bridge {
    fn default() -> Self {
        Self {
            phase: PHASE_IDLE,
            status_text: String::from("Ready to record"),
            seconds: 0,
            level: 0.0,
            edited_text: String::default(),
            original_text: String::default(),
            showing_original: false,
            has_result: false,
            osd_visible: false,
            window_visible: false,
            paste_ready: false,
            paste_saved: PasteService::has_saved_permission(),
            paste_error: false,
            paste_status: String::new(),
            paste_connecting: false,
            cfg_base_url: String::default(),
            cfg_api_key: String::default(),
            cfg_model: String::default(),
            cfg_stt_provider: 0,
            cfg_elevenlabs_api_key: String::new(),
            cfg_language: String::default(),
            cfg_microphone: String::default(),
            microphones_json: String::from(
                r#"{"devices":[{"name":"","label":"System default"}],"error":""}"#,
            ),
            cfg_output: 0,
            cfg_paste_shift: false,
            settings_status: String::default(),
            can_retry: false,
            settings_visible: false,
            error_text: String::default(),
            history_json: String::from("[]"),
            history_index: 0,
            history: Default::default(),
            audio_available: false,
            ignoring_results: false,
            initialized: false,
            config: Config::default(),
            delivery: OutputMode::Preview,
            override_mode: None,
            engine: None,
            paste: None,
        }
    }
}

fn output_to_i32(mode: OutputMode) -> i32 {
    match mode {
        OutputMode::Preview => 0,
        OutputMode::Clipboard => 1,
        OutputMode::Paste => 2,
    }
}
fn i32_to_output(value: i32) -> OutputMode {
    match value {
        1 => OutputMode::Clipboard,
        2 => OutputMode::Paste,
        _ => OutputMode::Preview,
    }
}
fn phase_code(phase: Phase) -> i32 {
    match phase {
        Phase::Idle => 0,
        Phase::Recording => 1,
        Phase::Transcribing => 2,
        Phase::Editing => 3,
    }
}

// Keep property declarations and signals explicit; generate only the ordinary
// Rust setters. QML writes use Qt Bridge's member-property implementation.
macro_rules! notifying_setters {
    ($($field:ident: $ty:ty => $setter:ident, $signal:ident;)*) => {
        impl Bridge {
            $(fn $setter(&mut self, value: $ty) {
                if self.$field != value {
                    self.$field = value;
                    self.$signal();
                }
            })*
        }
    };
}

notifying_setters! {
    phase: i32 => set_phase, phase_changed;
    status_text: String => set_status_text, status_text_changed;
    seconds: i32 => set_seconds, seconds_changed;
    level: f64 => set_level, level_changed;
    edited_text: String => set_edited_text, edited_text_changed;
    original_text: String => set_original_text, original_text_changed;
    showing_original: bool => set_showing_original, showing_original_changed;
    has_result: bool => set_has_result, has_result_changed;
    osd_visible: bool => set_osd_visible, osd_visible_changed;
    window_visible: bool => set_window_visible, window_visible_changed;
    paste_ready: bool => set_paste_ready, paste_ready_changed;
    paste_saved: bool => set_paste_saved, paste_saved_changed;
    paste_error: bool => set_paste_error, paste_error_changed;
    paste_status: String => set_paste_status, paste_status_changed;
    paste_connecting: bool => set_paste_connecting, paste_connecting_changed;
    cfg_base_url: String => set_cfg_base_url, cfg_base_url_changed;
    cfg_api_key: String => set_cfg_api_key, cfg_api_key_changed;
    cfg_model: String => set_cfg_model, cfg_model_changed;
    cfg_stt_provider: i32 => set_cfg_stt_provider, cfg_stt_provider_changed;
    cfg_elevenlabs_api_key: String => set_cfg_elevenlabs_api_key, cfg_elevenlabs_api_key_changed;
    cfg_language: String => set_cfg_language, cfg_language_changed;
    cfg_microphone: String => set_cfg_microphone, cfg_microphone_changed;
    microphones_json: String => set_microphones_json, microphones_json_changed;
    cfg_output: i32 => set_cfg_output, cfg_output_changed;
    cfg_paste_shift: bool => set_cfg_paste_shift, cfg_paste_shift_changed;
    settings_status: String => set_settings_status, settings_status_changed;
    can_retry: bool => set_can_retry, can_retry_changed;
    settings_visible: bool => set_settings_visible, settings_visible_changed;
    error_text: String => set_error_text, error_text_changed;
    history_json: String => set_history_json, history_json_changed;
    history_index: i32 => set_history_index, history_index_changed;
}

#[qobject(ConvertToCamelCase)]
impl Bridge {
    qproperty!(
        "cfgSttProvider",
        Member = cfg_stt_provider,
        Notify = cfg_stt_provider_changed
    );
    #[qsignal]
    fn cfg_stt_provider_changed(&mut self);

    qproperty!(
        "cfgElevenlabsApiKey",
        Member = cfg_elevenlabs_api_key,
        Notify = cfg_elevenlabs_api_key_changed
    );
    #[qsignal]
    fn cfg_elevenlabs_api_key_changed(&mut self);

    qproperty!("phase", Member = phase, Notify = phase_changed);

    #[qsignal]
    fn phase_changed(&mut self);

    qproperty!(
        "statusText",
        Member = status_text,
        Notify = status_text_changed
    );

    #[qsignal]
    fn status_text_changed(&mut self);

    qproperty!("seconds", Member = seconds, Notify = seconds_changed);

    #[qsignal]
    fn seconds_changed(&mut self);

    qproperty!("level", Member = level, Notify = level_changed);

    #[qsignal]
    fn level_changed(&mut self);

    qproperty!(
        "editedText",
        Member = edited_text,
        Notify = edited_text_changed
    );

    #[qsignal]
    fn edited_text_changed(&mut self);

    qproperty!(
        "originalText",
        Member = original_text,
        Notify = original_text_changed
    );

    #[qsignal]
    fn original_text_changed(&mut self);

    qproperty!(
        "showingOriginal",
        Member = showing_original,
        Notify = showing_original_changed
    );

    #[qsignal]
    fn showing_original_changed(&mut self);

    qproperty!(
        "hasResult",
        Member = has_result,
        Notify = has_result_changed
    );

    #[qsignal]
    fn has_result_changed(&mut self);

    qproperty!(
        "osdVisible",
        Member = osd_visible,
        Notify = osd_visible_changed
    );

    #[qsignal]
    fn osd_visible_changed(&mut self);

    qproperty!(
        "windowVisible",
        Member = window_visible,
        Notify = window_visible_changed
    );

    #[qsignal]
    fn window_visible_changed(&mut self);

    qproperty!(
        "pasteReady",
        Member = paste_ready,
        Notify = paste_ready_changed
    );

    #[qsignal]
    fn paste_ready_changed(&mut self);

    qproperty!(
        "pasteSaved",
        Member = paste_saved,
        Notify = paste_saved_changed
    );

    #[qsignal]
    fn paste_saved_changed(&mut self);

    qproperty!(
        "pasteError",
        Member = paste_error,
        Notify = paste_error_changed
    );

    #[qsignal]
    fn paste_error_changed(&mut self);

    qproperty!(
        "pasteStatus",
        Member = paste_status,
        Notify = paste_status_changed
    );
    #[qsignal]
    fn paste_status_changed(&mut self);

    qproperty!(
        "pasteConnecting",
        Member = paste_connecting,
        Notify = paste_connecting_changed
    );
    #[qsignal]
    fn paste_connecting_changed(&mut self);

    qproperty!(
        "cfgBaseUrl",
        Member = cfg_base_url,
        Notify = cfg_base_url_changed
    );

    #[qsignal]
    fn cfg_base_url_changed(&mut self);

    qproperty!(
        "cfgApiKey",
        Member = cfg_api_key,
        Notify = cfg_api_key_changed
    );

    #[qsignal]
    fn cfg_api_key_changed(&mut self);

    qproperty!("cfgModel", Member = cfg_model, Notify = cfg_model_changed);

    #[qsignal]
    fn cfg_model_changed(&mut self);

    qproperty!(
        "cfgLanguage",
        Member = cfg_language,
        Notify = cfg_language_changed
    );

    #[qsignal]
    fn cfg_language_changed(&mut self);

    qproperty!(
        "cfgMicrophone",
        Member = cfg_microphone,
        Notify = cfg_microphone_changed
    );

    #[qsignal]
    fn cfg_microphone_changed(&mut self);

    qproperty!(
        "microphonesJson",
        Member = microphones_json,
        Notify = microphones_json_changed
    );

    #[qsignal]
    fn microphones_json_changed(&mut self);

    qproperty!(
        "cfgOutput",
        Member = cfg_output,
        Notify = cfg_output_changed
    );

    #[qsignal]
    fn cfg_output_changed(&mut self);

    qproperty!(
        "cfgPasteShift",
        Member = cfg_paste_shift,
        Notify = cfg_paste_shift_changed
    );

    #[qsignal]
    fn cfg_paste_shift_changed(&mut self);

    qproperty!(
        "settingsStatus",
        Member = settings_status,
        Notify = settings_status_changed
    );

    #[qsignal]
    fn settings_status_changed(&mut self);

    qproperty!("canRetry", Member = can_retry, Notify = can_retry_changed);

    #[qsignal]
    fn can_retry_changed(&mut self);

    qproperty!(
        "settingsVisible",
        Member = settings_visible,
        Notify = settings_visible_changed
    );

    #[qsignal]
    fn settings_visible_changed(&mut self);

    qproperty!(
        "errorText",
        Member = error_text,
        Notify = error_text_changed
    );

    #[qsignal]
    fn error_text_changed(&mut self);

    qproperty!(
        "historyJson",
        Member = history_json,
        Notify = history_json_changed
    );

    #[qsignal]
    fn history_json_changed(&mut self);

    qproperty!(
        "historyIndex",
        Member = history_index,
        Notify = history_index_changed
    );

    #[qsignal]
    fn history_index_changed(&mut self);

    #[qslot]
    pub fn initialize(&mut self) {
        if self.initialized {
            return;
        }
        self.initialized = true;

        // Load the shared config; keep any error as the initial status.
        let (config, error) = match Config::load() {
            Ok(c) => (c, None),
            Err(e) => (Config::default(), Some(e.to_string())),
        };
        self.set_cfg_base_url(String::from(&config.base_url));
        self.set_cfg_api_key(String::from(&config.api_key));
        self.set_cfg_model(String::from(&config.model));
        self.set_cfg_stt_provider(match config.stt_provider {
            SttProvider::ChatgptProxy => 0,
            SttProvider::Elevenlabs => 1,
        });
        self.set_cfg_elevenlabs_api_key(config.elevenlabs_api_key.clone());
        self.set_cfg_language(String::from(&config.language));
        self.set_cfg_microphone(String::from(&config.microphone));
        self.set_cfg_output(output_to_i32(config.output));
        self.set_cfg_paste_shift(config.paste_shift);
        if let Some(e) = &error {
            self.set_status_text(String::from(e));
            self.set_settings_status(String::from(e));
        }
        self.config = config;
        self.refresh_microphones();

        // Engine: events come back on the Qt thread.
        let qt = self.get_thread_handle();
        let engine = Engine::with_defaults(move |event| {
            qt.invoke_mut(move |obj| obj.apply_event(event));
        });
        self.engine = Some(engine);

        // Paste service: portal events likewise return on the Qt thread.
        let qt = self.get_thread_handle();
        let paste = PasteService::start(move |event| {
            qt.invoke_mut(move |obj| obj.apply_paste(event));
        });
        self.paste = Some(paste);

        // Forwarded single-instance commands.
        if let Some(listener) = LISTENER.lock().unwrap().take() {
            let qt = self.get_thread_handle();
            ipc::spawn_accept(listener, move |cmd| {
                qt.invoke_mut(move |obj| obj.dispatch(&cmd));
            });
        }

        if let Some(initial) = take_initial_command() {
            self.dispatch(&initial);
        }
        INITIALIZED.store(true, Ordering::Relaxed);
    }

    /// Route a forwarded command to the matching action. `toggle` may carry a
    /// one-shot output-mode override, e.g. "toggle clipboard".
    pub fn dispatch(&mut self, command: &str) {
        let mut parts = command.split_whitespace();
        match parts.next() {
            Some("toggle") => {
                let over = parts.next().and_then(OutputMode::from_cli);
                if self.phase == PHASE_IDLE {
                    self.override_mode = over;
                }
                self.toggle();
            }
            Some("cancel") => self.cancel(),
            Some("retry") => self.retry(),
            Some("quit") => self.teardown(),
            Some("settings") => {
                self.set_settings_visible(true);
                self.present();
            }
            _ => {
                self.set_settings_visible(false);
                self.present();
            }
        }
    }

    #[qslot]
    pub fn toggle(&mut self) {
        if self.phase == PHASE_IDLE {
            // A `toggle <mode>` invocation overrides the saved output for this
            // recording only.
            let over = self.override_mode.take();
            self.delivery = over.unwrap_or(OutputMode::Preview);
            // Hide the main window (global shortcut preserves the focused app)
            // and show the OSD immediately so a window is on screen at startup.
            self.set_window_visible(false);
            self.set_error_text(String::default());
            self.set_paste_error(false);
        }
        let config = self.config.clone();
        if let Some(engine) = self.engine.as_ref() {
            engine.toggle(config);
        }
    }

    #[qslot]
    pub fn cancel(&mut self) {
        self.ignoring_results = true;
        if let Some(engine) = self.engine.as_ref() {
            engine.cancel();
        }
        self.set_error_text(String::default());
    }

    #[qslot]
    pub fn retry(&mut self) {
        if !self.can_retry || self.phase != PHASE_IDLE {
            return;
        }
        self.set_error_text(String::default());
        self.set_settings_visible(false);
        self.set_window_visible(true);
        self.delivery = OutputMode::Preview;
        let config = self.config.clone();
        if let Some(engine) = self.engine.as_ref() {
            engine.retry(config);
        }
    }

    #[qslot]
    pub fn edit_draft(&mut self, text: String) {
        self.history.edit(text.to_string());
        self.set_edited_text(text);
    }
    #[qslot]
    pub fn select_session(&mut self, index: i32) {
        if self.phase != PHASE_IDLE || index < 0 {
            return;
        }
        self.history.select(index as usize);
        self.refresh_draft();
    }
    fn refresh_draft(&mut self) {
        let draft = self.history.current().cloned().unwrap_or_default();
        let labels = serde_json::to_string(&self.history.labels()).unwrap();
        let index = self.history.selected as i32;
        let can_retry = self.audio_available && self.history.is_latest();
        self.set_original_text(draft.original.clone());
        self.set_edited_text(draft.text.clone());
        self.set_has_result(!draft.text.trim().is_empty());
        self.set_showing_original(false);
        self.set_history_json(labels);
        self.set_history_index(index);
        self.set_can_retry(can_retry);
    }
    fn copy(&mut self) -> bool {
        let text = if self.showing_original {
            &self.original_text
        } else {
            &self.edited_text
        }
        .to_string();
        if text.trim().is_empty() {
            return false;
        }
        match clipboard::set_text(&text) {
            Ok(()) => {
                self.toast("Copied");
                true
            }
            Err(e) => {
                self.set_error_text(e.to_string());
                false
            }
        }
    }
    #[qslot]
    pub fn copy_current(&mut self) {
        self.copy();
    }
    #[qslot]
    pub fn copy_and_close(&mut self) {
        if self.copy() {
            self.set_window_visible(false);
        }
    }
    fn toast(&mut self, text: &str) {
        self.set_error_text(String::default());
        self.set_status_text(String::from(text));
        if self.phase == PHASE_IDLE {
            self.set_osd_visible(true);
        }
    }

    #[qslot]
    pub fn toggle_original(&mut self) {
        let showing = self.showing_original;
        self.set_showing_original(!showing);
        let status = if showing {
            "Edited text"
        } else {
            "Original transcript (read-only)"
        };
        self.set_status_text(String::from(status));
    }

    #[qslot]
    pub fn refresh_microphones(&mut self) {
        let qt = self.get_thread_handle();
        std::thread::spawn(move || {
            let mut devices = vec![serde_json::json!({"name": "", "label": "System default"})];
            let result = (|| -> Result<(), Box<dyn std::error::Error>> {
                let output = std::process::Command::new("pw-dump")
                    .stdin(std::process::Stdio::null())
                    .output()?;
                if !output.status.success() {
                    return Err("PipeWire is unavailable".into());
                }
                let objects: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout)?;
                for object in objects {
                    let props = &object["info"]["props"];
                    if props["media.class"].as_str() != Some("Audio/Source") {
                        continue;
                    }
                    if let Some(name) = props["node.name"].as_str() {
                        let label = props["node.description"]
                            .as_str()
                            .or_else(|| props["node.nick"].as_str())
                            .unwrap_or(name);
                        devices.push(serde_json::json!({"name": name, "label": label}));
                    }
                }
                devices[1..].sort_by(|a, b| a["label"].as_str().cmp(&b["label"].as_str()));
                Ok(())
            })();
            let error = if result.is_err() {
                "Could not list microphones. Check that PipeWire is running and pw-dump is installed."
            } else {
                ""
            };
            let json = serde_json::json!({"devices": devices, "error": error}).to_string();
            qt.invoke_mut(move |obj| obj.set_microphones_json(json));
        });
    }

    #[qslot]
    pub fn save_settings(&mut self) {
        if self.phase != PHASE_IDLE {
            self.set_settings_status(String::from("Finish recording and processing first"));
            return;
        }
        let candidate = Config {
            stt_provider: if self.cfg_stt_provider == 1 {
                SttProvider::Elevenlabs
            } else {
                SttProvider::ChatgptProxy
            },
            elevenlabs_api_key: self.cfg_elevenlabs_api_key.trim().to_owned(),
            base_url: self.cfg_base_url.trim().to_owned(),
            api_key: self.cfg_api_key.trim().to_owned(),
            model: self.cfg_model.trim().to_owned(),
            language: self.cfg_language.trim().to_owned(),
            microphone: self.cfg_microphone.trim().to_owned(),
            output: i32_to_output(self.cfg_output),
            paste_shift: self.cfg_paste_shift,
        };
        match candidate.save() {
            Ok(()) => {
                self.config = candidate;
                self.set_settings_status(String::from("Settings saved"));
            }
            Err(e) => self.set_settings_status(e.to_string()),
        }
    }

    #[qslot]
    pub fn dismiss_error(&mut self) {
        self.set_error_text(String::new());
        self.set_paste_error(false);
    }

    #[qslot]
    pub fn connect_paste(&mut self) {
        self.set_paste_status(String::from("Waiting for the desktop portal…"));
        self.set_paste_connecting(true);
        if let Some(paste) = self.paste.as_ref() {
            paste.connect();
        }
    }

    #[qslot]
    pub fn present(&mut self) {
        self.set_window_visible(true);
    }

    #[qslot]
    pub fn teardown(&mut self) {
        if let Some(engine) = self.engine.as_ref() {
            engine.shutdown();
        }
        if let Some(paste) = self.paste.as_ref() {
            paste.shutdown();
        }
        ipc::cleanup(SOCKET);
        // Workers are stopped and the socket is gone; leave the process now.
        // (A QML Qt.quit() via the signal proved unreliable from this path.)
        std::process::exit(0);
    }

    /// Apply one engine event to the QML-visible state.
    pub fn apply_event(&mut self, event: Event) {
        if self.ignoring_results
            && matches!(
                event,
                Event::Result { .. } | Event::Transcript(_) | Event::Error(_)
            )
        {
            return;
        }
        match event {
            Event::Phase(phase) => {
                self.set_phase(phase_code(phase));
                let silent = self.delivery != OutputMode::Preview;
                self.set_osd_visible(phase == Phase::Recording || (silent && phase != Phase::Idle));
                match phase {
                    Phase::Recording => {
                        self.history.begin();
                        self.audio_available = false;
                        self.refresh_draft();
                        self.set_seconds(0);
                        self.set_level(0.0);
                        self.set_error_text(String::default());
                        self.set_window_visible(false);
                        self.set_status_text(String::from("Recording"));
                    }
                    Phase::Transcribing => {
                        self.set_status_text(String::from("Transcribing recording…"))
                    }
                    Phase::Editing => self.set_status_text(String::from(
                        "Editing text… Original transcript available.",
                    )),
                    Phase::Idle => {} // Terminal messages survive the phase transition.
                }
            }
            Event::Captured => {
                self.history.captured();
                self.set_status_text(String::from("Recording retained"));
                self.audio_available = true;
                self.refresh_draft();
                self.set_settings_visible(false);
                if self.delivery == OutputMode::Preview {
                    self.set_window_visible(true);
                }
            }
            Event::Transcript(text) => {
                self.history.transcript(text);
                self.refresh_draft();
            }
            Event::Cancelled { recording } => {
                if recording {
                    self.history.cancel_recording();
                    self.refresh_draft();
                }
                self.ignoring_results = false;
                self.set_osd_visible(false);
                self.set_status_text(String::from("Processing cancelled"));
            }
            Event::Meter { seconds, level } => {
                self.set_seconds(seconds as i32);
                self.set_level((level as f64 * 5.0).clamp(0.0, 1.0));
            }
            Event::Error(e) => {
                self.set_status_text(String::from("Action failed"));
                self.set_error_text(e);
                if self.audio_available {
                    self.set_settings_visible(false);
                    self.set_window_visible(true);
                    self.set_osd_visible(false);
                } else {
                    self.set_osd_visible(true);
                }
            }
            Event::Result {
                original,
                edited,
                warning,
            } => {
                let accepted = self.history.result(original, edited.clone());
                self.refresh_draft();
                if let Some(w) = warning {
                    self.set_status_text(String::from("Original text retained"));
                    self.set_error_text(w);
                    if self.delivery != OutputMode::Preview {
                        self.set_window_visible(true);
                    }
                    return;
                }
                if !accepted {
                    self.set_status_text(String::from("Your edits have been saved"));
                    return;
                }
                self.set_status_text(String::from("Text ready"));
                match self.delivery {
                    OutputMode::Preview => self.set_status_text(String::from("Text ready")),
                    mode => {
                        if let Err(e) = clipboard::set_text(&edited) {
                            self.set_error_text(e.to_string());
                            self.set_window_visible(true);
                        } else if mode == OutputMode::Clipboard {
                            self.toast("Copied");
                        } else if self.paste_ready || self.paste_saved {
                            self.set_status_text(String::from("Text ready. Pasting…"));
                            if let Some(paste) = self.paste.as_ref() {
                                paste.paste(self.config.paste_shift);
                            }
                        } else {
                            self.set_paste_error(true);
                            self.set_error_text(String::from("Text copied. Connect automatic paste in settings or paste manually."));
                            self.set_window_visible(true);
                        }
                    }
                }
            }
        }
    }

    /// Apply one paste-service event.
    pub fn apply_paste(&mut self, event: PasteEvent) {
        match event {
            PasteEvent::Connected => {
                self.set_paste_connecting(false);
                self.set_paste_ready(true);
                if self.paste_error {
                    self.set_error_text(String::default());
                    self.set_paste_error(false);
                }
                self.set_paste_saved(PasteService::has_saved_permission());
                self.set_paste_status(String::from(if PasteService::has_saved_permission() {
                    "Automatic paste connected. Permission saved for future launches."
                } else {
                    "Automatic paste connected, but the portal did not grant permission for future launches."
                }));
            }
            PasteEvent::ConnectionFailed(e) => {
                self.set_paste_connecting(false);
                self.set_paste_ready(false);
                self.set_paste_saved(PasteService::has_saved_permission());
                self.set_paste_status(e);
            }
            PasteEvent::Done => self.toast("Pasted"),
            PasteEvent::Error(e) => {
                self.set_paste_ready(false);
                self.set_paste_error(true);
                self.set_paste_saved(PasteService::has_saved_permission());
                if self.has_result {
                    self.set_status_text(String::from("Text ready"));
                }
                self.set_paste_status(String::from(
                    "Automatic paste disconnected. You can copy text manually.",
                ));
                self.set_error_text(e);
                self.set_window_visible(true);
            }
        }
    }
}
