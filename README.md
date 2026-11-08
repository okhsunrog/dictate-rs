# Dictate

Voice dictation for Linux Wayland desktops. Start recording with a shortcut,
press it again to stop, then review the transcript and its edited version.
You can also send the result straight to the clipboard or paste it into the
focused application.

The application uses **Rust, Qt Bridge for Rust and Qt Quick/QML**. A small
LayerShellQt overlay shows recording time, microphone level and processing status
without taking keyboard focus. The interface is in English.

[User guide](docs/guide.md)

## What it does

- Captures mono, 16 kHz audio through PipeWire.
- Transcribes the completed recording, then asks an editing model to remove
  filler words, fix punctuation and apply spoken corrections.
- Opens an editable preview as soon as recording stops; the raw transcript
  appears before editing finishes.
- Preserves manual edits when a delayed model response arrives.
- Keeps the last ten drafts in memory and retains the latest audio for retry.
- Supports cancellation, retry, clipboard delivery and optional automatic paste.
- Routes repeated launches to the running instance through a private Unix socket.

There is no streaming transcription. Recognition and editing use a configured
remote service; the application does not include a local speech model.

## Requirements

- Linux with PipeWire and a Wayland compositor supporting layer-shell, such as
  Niri or KDE Plasma. An ordinary-window fallback is available on X11.
- `pw-record` and `wl-copy` in `PATH`.
- A service implementing the API contract described below.
- For automatic paste: a working **RemoteDesktop portal with libei/EIS support**.
  This is a separate compositor/portal capability. In particular, standard Niri
  portal setups may not provide it; preview and clipboard modes remain usable.

Build requirements: Rust 1.88+, a C++ toolchain, pkg-config, Qt 6.10+ with private
development headers, Qt Declarative, Qt Wayland, LayerShellQt and the
`qqc2-desktop-style` QML modules, including Kirigami and Sonnet. Qt Bridge 0.4.1
is a Technology Preview. CXX-Qt remains a transitive dependency.

## Build and run

The Nix flake provides the development tools and Qt dependencies on Linux x86_64:

```sh
git clone https://github.com/okhsunrog/dictate-rs.git
cd dictate-rs
nix develop
just run show   # open the window without recording
just settings   # configure the service
just run        # start recording; run again to stop
```

With the dependencies installed by your distribution:

```sh
cargo build --release --locked
./target/release/dictate settings
```

`qmake` must describe the same Qt installation as its headers and QML modules.
The Nix shell assembles those paths automatically. To build from `shell.nix`
against your own Nixpkgs instead, use `nix develop --impure --file shell.nix`.

Install the release binary and desktop entry under `~/.local`:

```sh
just install
# Optional: PREFIX=/your/prefix just install
```

For the development build on NixOS, run through `nix develop` so the executable
can find Qt's shared libraries and QML imports. A standalone wrapped Nix package
is not provided yet.

## Service configuration

Open `dictate settings`, or copy [config.example.toml](config.example.toml) to
`~/.config/dictate/config.toml`. Set the service URL, API key and editing model.
No personal endpoint or model is selected by default. Existing saved settings
continue to work.

Choose the recognition provider in Settings: **ChatGPT proxy** (the default for
existing configs) or **ElevenLabs Scribe v2**. Recognition and editing use
[`llm-relay`](https://github.com/okhsunrog/llm-relay), pinned to a Git revision
with file transcription support. The released crate 1.0.1 does not yet include it.

Set `stt_provider = "elevenlabs"` and provide `ELEVENLABS_API_KEY` or the separate
`elevenlabs_api_key` setting to use ElevenLabs. The existing URL, API key and model
continue to configure the editor. Requests are sent once; retry is an explicit
user action, with no automatic fallback between paid providers.

The relevant endpoints are:

| Endpoint | Contract |
| --- | --- |
| `POST /v1/audio/transcriptions` | Bearer authentication; multipart audio file, `model=chatgpt-transcribe`, `response_format=json`, optional `language`; JSON response with a `text` string |
| ElevenLabs `POST https://api.elevenlabs.io/v1/speech-to-text` | Separate `xi-api-key`; multipart audio, `model_id=scribe_v2`, optional `language_code`; no diarization or audio-event tags |
| `POST /v1/chat/completions` | The configured editing model; system prompt and user transcript, `stream=false`, `store=false`, `reasoning_effort=none`; completed choice with `finish_reason=stop` |

This contract is intended for a compatible proxy; it is not a claim that every
OpenAI-compatible service or model supports these requests. The selected editing
model must accept `reasoning_effort=none`. Editing now uses Chat Completions
instead of the previous Responses endpoint; the service must support it.

`DICTATE_API_KEY`, `DICTATE_BASE_URL` and `ELEVENLABS_API_KEY` override saved values without being
written back to the config. Recognition language and the PipeWire microphone
target are optional; empty values use automatic detection and the default input.
Regional language tags such as `en-US` are reduced to the primary ISO language
code (`en`) for recognition.

## Commands and shortcuts

| Command | Action |
| --- | --- |
| `dictate` / `dictate preview` | Start or stop recording, then show the preview |
| `dictate clipboard` | Start recording and copy the final result when ready |
| `dictate paste` | Start recording and paste the final result when ready |
| `dictate show` | Open the window without recording |
| `dictate settings` | Open settings without recording |
| `dictate cancel` | Cancel recording or processing |
| `dictate retry` | Retry the retained audio, or only editing if a transcript exists |
| `dictate quit` | Stop the application |

The output mode is chosen when recording starts. Any subsequent recording command
stops that recording without changing its mode. `toggle [preview|clipboard|paste]`
is also accepted. In the editor, **Ctrl+Enter** copies and hides the window;
**Escape** hides it. Closing the main window exits the application.

For Niri, use an absolute path to the installed binary and choose an unused key:

```kdl
binds {
    Mod+D { spawn "/home/YOUR_USER/.local/bin/dictate"; }
}

window-rule {
    match app-id=r#"^ru\.okhsunrog\.dictate$"#
    open-floating true
    open-maximized false
    open-fullscreen false
    default-column-width { fixed 640; }
    default-window-height { fixed 480; }
}
```

In Plasma, assign the same command in System Settings → Keyboard → Shortcuts.
Do not put the bare `dictate` command in autostart: it starts recording immediately.

For paste mode, connect automatic paste in Settings first. The text is sent to
whichever application has focus **when processing finishes**. A portal restore
token is saved when the portal permits it; if paste fails, the text remains on
the clipboard. Enable terminal compatibility for Ctrl+Shift+V.

## Data and troubleshooting

Recordings and draft history stay in process memory. Only the latest audio is
available for retry; starting another recording replaces it. Exiting the
application discards audio and draft history. Audio and text are sent to the
configured service for processing; its retention policy is outside this project.
Cancelling the HTTP request does not guarantee the server stopped processing it.

Settings are saved with directory mode `0700` and file mode `0600`. An API key
entered in Settings is stored as plaintext in that file. The portal restore
token is stored separately with mode `0600`.

If no speech is recognized, check the hardware microphone switch, input mute,
selected PipeWire source and recording-level indicator. Recognition failures keep
the audio for retry. If editing fails, the original transcript remains available.
If the interface cannot load a required QML module, startup returns a nonzero
exit code; `QT_FORCE_STDERR_LOGGING=1 dictate show` exposes Qt diagnostics.

## Development

```sh
just check           # formatting, Clippy and Rust tests
just ui-smoke        # sequential Xvfb tests; fake audio, HTTP and clipboard
just wayland-smoke   # private nested Niri; requires an existing Wayland session
```

UI test artifacts are written under `TMPDIR` or a supplied `--output` directory.
Tests do not use a physical microphone, API key or paid service. The Wayland test
checks mapped layer-shell surfaces, absence of a keyboard-focus request, preview
opening, instance IPC and shutdown. Live dictation has also been exercised on
Niri; automatic paste still depends on the desktop's portal implementation.

For an explicit test against your configured service, `just e2e path/to/audio.ogg en`
sends that file and prints the transcript and edited result.

- `crates/dictate-core`: recording, state machine, API client, drafts, config and paste.
- `crates/dictate`: Qt Bridge adapter, QML editor and LayerShellQt recording overlay.
- `tests`: isolated desktop regression tests.

Rust setters are generated by a small local macro; QML properties and notify
signals remain explicit. Qt types do not cross into the core.

## License

The repository's original code is available under either [MIT](LICENSE-MIT) or
[Apache-2.0](LICENSE-APACHE), at your option. Dependencies retain their own
licenses, including Qt and Qt Bridge.
