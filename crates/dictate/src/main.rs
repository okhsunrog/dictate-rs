//! Qt Quick application backed by the toolkit-independent dictation engine.

pub mod bridge;

use dictate_core::ipc;
use qtbridge::QApp;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args
        .get(1)
        .is_some_and(|s| matches!(s.as_str(), "--help" | "-h"))
    {
        println!("{}", dictate_core::cli::HELP);
        return;
    }
    let command = dictate_core::cli::command(&args[1..]).unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(2);
    });
    let starts_instance = dictate_core::cli::starts_instance(&command);
    let listener = match ipc::start(bridge::SOCKET, &command, starts_instance) {
        ipc::Startup::Resident(listener) => listener,
        ipc::Startup::Forwarded | ipc::Startup::Nothing => return,
    };
    // We are the resident instance.
    bridge::set_listener(Some(listener));
    bridge::set_initial_command(&command);

    // Prefer the native KDE (Breeze) Controls style unless the user overrode it.
    // Safe here: set before any threads or the Qt app are created.
    if std::env::var_os("QT_QUICK_CONTROLS_STYLE").is_none() {
        unsafe { std::env::set_var("QT_QUICK_CONTROLS_STYLE", "org.kde.desktop") };
    }

    let exit_code = QApp::new()
        .application_name("Dictate")
        .register::<bridge::Bridge>()
        .load_qml(include_bytes!("../qml/main.qml"))
        .run();
    ipc::cleanup(bridge::SOCKET);
    // QApp may return zero even when a QML import failed before initialization.
    if !bridge::initialized() {
        eprintln!("Failed to load the Dictate QML interface.");
        std::process::exit(1);
    }
    std::process::exit(exit_code);
}
