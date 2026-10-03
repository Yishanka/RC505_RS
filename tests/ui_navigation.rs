//! Exercises real application routing in an isolated, device-free process.
#![cfg(debug_assertions)]
#[test]
fn editor_navigation_and_dialogs_keep_valid_accessibility_focus() {
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
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("UI regression passed"));
    // Keep failures for diagnosis, but do not accumulate successful audio fixtures.
    let root = std::fs::canonicalize(root).unwrap();
    assert_eq!(
        root.parent(),
        Some(std::fs::canonicalize("var").unwrap().as_path())
    );
    std::fs::remove_dir_all(root).unwrap();
}
