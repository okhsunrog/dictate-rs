# Dictate user guide

Dictate records speech through PipeWire, transcribes the completed recording,
and edits the transcript with a language model. The interface uses Qt Quick/QML
and Qt Bridge for Rust. All application text is in English.

## Install and configure

On NixOS:

```sh
git clone https://github.com/okhsunrog/dictate-rs.git
cd dictate-rs
nix develop
just run show
just settings
```

Opening the window or settings does not start recording. Set the service URL
including `/v1`, API key and editing model. No personal endpoint or model is
selected by default. Use `just install` to install under `~/.local`.
See the [README](../README.md#requirements) for build and runtime dependencies.

Choose **ChatGPT proxy** or **ElevenLabs Scribe v2** for transcription. Both use
`llm-relay`. Existing configs default to the proxy. ElevenLabs requires its own
key in Settings or `ELEVENLABS_API_KEY`. The service URL, ordinary API key and
model always configure editing; in proxy mode the same service transcribes audio.
Editing uses `chat/completions` rather than the previous `responses` endpoint.
The model must accept `reasoning_effort=none`. There are no automatic retries or
provider fallbacks. See the [API contract](../README.md#service-configuration).

## Record and review

1. Check the hardware microphone switch and system input device.
2. Run `dictate` or press your configured shortcut.
3. Speak while the overlay displays recording time and input level.
4. Run `dictate` again or click **Finish**.
5. Review the original transcript, followed by the edited text.
6. Make any corrections and click **Copy**.

Spoken corrections such as "Wednesday at three, no, Thursday at two" are applied
by the editor. The editor preserves the dictation language. Transcription starts
after recording stops. Recordings are limited to ten minutes and 25 MiB.

## Window and history

- `dictate show` opens the window; `dictate settings` opens settings.
- Ctrl+Enter copies the text and hides the window. Escape hides it.
- Closing the window exits, stops recording and cancels processing.
- The history button opens recent drafts. **Edited** and **Original** switch
  transcript views; retry actions appear beside the result.
- The original transcript is read-only. Switching views preserves manual edits.
- A delayed model response does not replace manual edits.
- The last ten drafts are kept until exit.

`dictate clipboard` records and copies the result without preview.
`dictate paste` records and pastes automatically. A repeated recording command
stops the current recording while retaining its selected delivery mode.
`cancel`, `retry` and `quit` control an existing instance.

## Shortcuts and window size

For Niri, choose an unused shortcut and use your own absolute binary path:

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

In Plasma, use System Settings → Keyboard → Shortcuts. The bare `dictate`
command starts recording, so do not add it to autostart.

## Automatic paste

Click **Connect automatic paste** in Settings and approve the system dialog.
This requires a RemoteDesktop portal with libei/EIS support. Standard Niri portal
setups may not provide it; use Copy and Ctrl+V in that case.

When permitted, a restore token saves permission for future launches. Text is
pasted into the application focused when processing finishes. Keep the target
application focused until then. Enable terminal compatibility for Ctrl+Shift+V.
If paste fails, the text remains on the clipboard.

## Settings and data

Settings are in `~/.config/dictate/config.toml`; see
[config.example.toml](../config.example.toml). The directory uses mode `0700` and
the file uses `0600`. Keys entered in Settings are stored as plaintext.
`DICTATE_API_KEY`, `DICTATE_BASE_URL` and `ELEVENLABS_API_KEY` override saved
settings without being written back. The ElevenLabs key is separate from the
editor key.

Choose a language by name, or select **Automatic detection**. You can also type
an ISO language code. Choose a microphone from the detected PipeWire devices,
or use **System default**. Click **Refresh** after connecting another microphone.

Audio and history remain in process memory. Only the latest recording is retained
for retry; a new recording replaces it. Exiting discards audio and history.
Audio and text are sent to the configured services, whose retention policies apply.
Cancelling a request does not guarantee server-side processing has stopped.

## Troubleshooting

- No speech recognized: check the microphone switch, mute, input device and level
  indicator. Then retry processing or make another recording.
- Transcription failed: the audio remains available for **Retry**.
- Editing failed: the original transcript remains available. Retry runs only
  editing, without uploading the audio again.
- No window: check required QML modules. Run
  `QT_FORCE_STDERR_LOGGING=1 dictate show` for Qt diagnostics.
- Paste failed: check RemoteDesktop portal support or copy manually.
