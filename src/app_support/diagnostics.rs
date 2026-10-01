//! Preserve panic evidence for GUI builds without a console. No audio/config dump.
pub fn install_panic_log() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Some(root) = super::paths::appdata_root() {
            let directory = root.join("logs");
            if std::fs::create_dir_all(&directory).is_ok() {
                let message = format!(
                    "RC505 RS {}\n{}\nThread: {:?}\n{info}\n{}\n",
                    env!("CARGO_PKG_VERSION"),
                    chrono::Utc::now(),
                    std::thread::current().name(),
                    std::backtrace::Backtrace::force_capture()
                );
                let _ = std::fs::write(directory.join("last-panic.log"), message);
            }
        }
        previous(info);
    }));
}
