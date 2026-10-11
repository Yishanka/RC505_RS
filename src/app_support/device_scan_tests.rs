use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

fn names(input: &str, output: &str) -> DeviceList {
    DeviceList {
        inputs: vec![input.into()],
        outputs: vec![output.into()],
        default_input: Some(input.into()),
        default_output: Some(output.into()),
        warnings: Vec::new(),
    }
}
fn completed(scan: &mut DeviceScan) -> ScanResult {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(result) = scan.poll() {
            return result;
        }
        assert!(Instant::now() < deadline, "Mock scan did not finish");
        std::thread::sleep(Duration::from_millis(1));
    }
}
#[test]
fn device_scan_timeout_does_not_queue_or_spawn_replacements_and_accepts_late_completion() {
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let mut scan = DeviceScan::with_scan(move || {
        count.fetch_add(1, Ordering::SeqCst);
        entered_tx.send(std::thread::current().id()).unwrap();
        release_rx.recv().unwrap();
        Ok(names("Microphone", "Headphones"))
    });
    let start = Instant::now();
    assert!(scan.request_at(start));
    let first_thread = entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    for _ in 0..1000 {
        assert!(!scan.request());
    }
    assert!(scan.poll_at(start + Duration::from_secs(4)).is_none());
    assert!(
        scan.poll_at(start + TIMEOUT)
            .unwrap()
            .unwrap_err()
            .contains("timed out")
    );
    assert!(scan.pending() && scan.timed_out());
    for _ in 0..1000 {
        assert!(scan.poll_at(start + Duration::from_secs(100)).is_none());
        assert!(!scan.request());
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    release_tx.send(()).unwrap();
    assert_eq!(
        completed(&mut scan).unwrap(),
        names("Microphone", "Headphones")
    );
    assert!(!scan.pending() && !scan.timed_out());
    assert!(scan.poll().is_none());
    assert!(scan.request());
    assert_eq!(
        entered_rx.recv_timeout(Duration::from_secs(2)).unwrap(),
        first_thread
    );
    release_tx.send(()).unwrap();
    completed(&mut scan).unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}
#[test]
fn device_scan_errors_and_driver_panics_are_reported_once_on_the_same_worker() {
    let mut call = 0;
    let (thread_tx, thread_rx) = mpsc::channel();
    let mut scan = DeviceScan::with_scan(move || {
        call += 1;
        thread_tx.send(std::thread::current().id()).unwrap();
        match call {
            1 => Err("Mock enumeration error".into()),
            2 => panic!("Mock driver panic"),
            _ => Ok(DeviceList::default()),
        }
    });
    let mut threads = Vec::new();
    for attempt in 0..3 {
        assert!(scan.request());
        threads.push(thread_rx.recv_timeout(Duration::from_secs(2)).unwrap());
        let result = completed(&mut scan);
        if attempt < 2 {
            assert!(result.is_err());
        } else {
            assert_eq!(result.unwrap(), DeviceList::default());
        }
        assert!(!scan.pending());
        assert!(scan.poll().is_none());
    }
    assert!(threads.iter().all(|id| *id == threads[0]));
}
#[test]
fn device_scan_drop_never_waits_on_a_blocked_driver() {
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let (finished_tx, finished_rx) = mpsc::channel();
    let mut scan = DeviceScan::with_scan(move || {
        entered_tx.send(()).unwrap();
        release_rx.recv().unwrap();
        finished_tx.send(()).unwrap();
        Ok(DeviceList::default())
    });
    assert!(scan.request());
    entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let (dropped_tx, dropped_rx) = mpsc::channel();
    let owner = std::thread::spawn(move || {
        drop(scan);
        dropped_tx.send(()).unwrap();
    });
    let dropped = dropped_rx.recv_timeout(Duration::from_secs(1));
    release_tx.send(()).unwrap();
    finished_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    owner.join().unwrap();
    assert!(
        dropped.is_ok(),
        "Closing the UI must not join a blocked device scan"
    );
}

#[test]
fn device_scan_payload_contains_only_sendable_names() {
    fn assert_send<T: Send>() {}
    assert_send::<DeviceList>();
}
