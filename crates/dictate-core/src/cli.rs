//! Normalize launch options before forwarding them to the resident instance.
pub const HELP: &str = "Usage: dictate [toggle] [--overlay|--window] [--review|--copy|--paste]\n\
No arguments: start/finish recording using saved settings.\n\
  --overlay   Keep recording, processing and review in the bottom bar\n\
  --window    Open the full editor after recording\n\
  --review    Wait for Copy & close after processing\n\
  --copy      Copy the final result and hide automatically\n\
  --paste     Paste the final result and hide automatically (requires a portal)\n\
Commands: compact, show, settings, finish, copy, hide, cancel, retry, quit\n\
compact/show/settings never start recording. Other commands control an existing instance.\n\
Legacy aliases: preview = --window --review; clipboard = --overlay --copy; paste = --overlay --paste.\n\
Overrides apply only when starting a recording; finishing preserves its original options.";

pub fn command(args: &[String]) -> Result<String, String> {
    if let Some(verb) = args.first().map(String::as_str)
        && matches!(
            verb,
            "compact"
                | "show"
                | "settings"
                | "finish"
                | "copy"
                | "hide"
                | "cancel"
                | "retry"
                | "quit"
        )
    {
        return if args.len() == 1 {
            Ok(verb.into())
        } else {
            Err(format!("Unexpected arguments\n{HELP}"))
        };
    }
    let mut presentation = None;
    let mut output = None;
    for (index, arg) in args.iter().enumerate() {
        let (next_view, next_output) = match arg.as_str() {
            "toggle" if index == 0 => (None, None),
            "--overlay" | "overlay" => (Some("overlay"), None),
            "--window" | "window" => (Some("window"), None),
            "--review" | "review" => (None, Some("preview")),
            "--copy" => (None, Some("clipboard")),
            "clipboard" => (Some("overlay"), Some("clipboard")),
            "--paste" => (None, Some("paste")),
            "preview" => (Some("window"), Some("preview")),
            "paste" => (Some("overlay"), Some("paste")),
            _ => return Err(format!("Unknown option: {arg}\n{HELP}")),
        };
        if let Some(view) = next_view {
            if presentation.is_some_and(|old| old != view) {
                return Err("Choose either --overlay or --window".into());
            }
            presentation = Some(view);
        }
        if let Some(mode) = next_output {
            if output.is_some_and(|old| old != mode) {
                return Err("Choose one of --review, --copy or --paste".into());
            }
            output = Some(mode);
        }
    }
    let mut command = String::from("toggle");
    for option in [presentation, output].into_iter().flatten() {
        command.push(' ');
        command.push_str(option);
    }
    Ok(command)
}

pub fn starts_instance(command: &str) -> bool {
    command.starts_with("toggle") || matches!(command, "compact" | "show" | "settings")
}
