//! Exercises real application routing in an isolated, device-free process.
#![cfg(debug_assertions)]
#[test]
fn editor_navigation_and_dialogs_keep_valid_accessibility_focus() {
    use std::io::Read;
    let root = format!("var/ui-navigation-test-{}", std::process::id());
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_rc505_rs"))
        .args([
            "--offline",
            &format!("--data-dir={root}"),
            "--ui-regression",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    // Drain while the child runs: a detailed assertion can fill the pipe before
    // exit, otherwise the parent misreports a real regression as a UI hang.
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let stdout = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).unwrap();
        bytes
    });
    let stderr = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr.read_to_end(&mut bytes).unwrap();
        bytes
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(45);
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("UI navigation process hung");
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let status = child.wait().unwrap();
    let stdout = stdout.join().unwrap();
    let stderr = stderr.join().unwrap();
    assert!(status.success(), "{}", String::from_utf8_lossy(&stderr));
    assert!(String::from_utf8_lossy(&stdout).contains("UI regression passed"));
    // Keep failures for diagnosis, but do not accumulate successful audio fixtures.
    let root = std::fs::canonicalize(root).unwrap();
    assert_eq!(
        root.parent(),
        Some(std::fs::canonicalize("var").unwrap().as_path())
    );
    std::fs::remove_dir_all(root).unwrap();
}
