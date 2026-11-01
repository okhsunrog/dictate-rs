//! Command-line parsing shared by startup and IPC dispatch.
pub const HELP: &str = "[preview|clipboard|paste|show|settings|demo|demo-overlay|cancel|retry|quit]\nБез аргументов: начать / завершить запись и открыть результат.\nclipboard: результат в буфер; paste: вставить. show/settings/demo не включают микрофон.\nТакже поддерживается toggle [preview|clipboard|paste].";

pub fn command(args: &[String]) -> Result<String, String> {
    let verb = args.first().map(String::as_str).unwrap_or("preview");
    let mode = |value: &str| match crate::OutputMode::from_cli(value) {
        Some(crate::OutputMode::Preview) => Ok("toggle preview".into()),
        Some(crate::OutputMode::Clipboard) => Ok("toggle clipboard".into()),
        Some(crate::OutputMode::Paste) => Ok("toggle paste".into()),
        None => Err(format!("Неизвестный режим: {value}")),
    };
    match (verb, args.len()) {
        ("toggle", 1) => mode("preview"),
        ("toggle", 2) => mode(&args[1]),
        ("preview" | "clipboard" | "paste", 0 | 1) => mode(verb),
        ("show" | "settings" | "demo" | "demo-overlay" | "cancel" | "retry" | "quit", 1) => {
            Ok(verb.into())
        }
        _ => Err(format!("Неизвестная команда или лишние аргументы\n{HELP}")),
    }
}
pub fn starts_instance(command: &str) -> bool {
    !matches!(command, "cancel" | "retry" | "quit")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse(args: &[&str]) -> Result<String, String> {
        command(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }
    #[test]
    fn launch_contract() {
        assert_eq!(parse(&[]).unwrap(), "toggle preview");
        assert_eq!(parse(&["clipboard"]).unwrap(), "toggle clipboard");
        assert_eq!(parse(&["paste"]).unwrap(), "toggle paste");
        assert_eq!(parse(&["toggle"]).unwrap(), "toggle preview");
        assert_eq!(parse(&["toggle", "paste"]).unwrap(), "toggle paste");
        for verb in ["show", "settings", "demo", "demo-overlay"] {
            assert_eq!(parse(&[verb]).unwrap(), verb);
        }
        assert!(parse(&["show", "paste"]).is_err());
        assert!(parse(&["toggle", "paste", "extra"]).is_err());
        for verb in ["cancel", "retry", "quit"] {
            assert!(!starts_instance(verb));
        }
    }
}
