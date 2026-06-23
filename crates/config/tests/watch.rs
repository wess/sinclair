use super::*;
use std::sync::atomic::AtomicUsize;
use std::time::Instant;

fn tempfile(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("promptwatchtest{}{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("config")
}

#[test]
fn fires_on_mtime_change_and_stops_on_drop() {
    let file = tempfile("fires");
    std::fs::write(&file, "font-size = 13\n").unwrap();
    let hits = Arc::new(AtomicUsize::new(0));
    let counter = hits.clone();
    let handle = watch(file.clone(), Duration::from_millis(10), move || {
        counter.fetch_add(1, Ordering::SeqCst);
    });

    // Bump the mtime until the watcher notices (bounded by a deadline).
    let f = std::fs::File::options().write(true).open(&file).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut bump = 10;
    while hits.load(Ordering::SeqCst) == 0 && Instant::now() < deadline {
        f.set_modified(SystemTime::now() + Duration::from_secs(bump))
            .unwrap();
        bump += 10;
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(hits.load(Ordering::SeqCst) >= 1, "watcher never fired");

    // After drop the thread is joined: no further callbacks.
    drop(handle);
    let settled = hits.load(Ordering::SeqCst);
    f.set_modified(SystemTime::now() + Duration::from_secs(bump))
        .unwrap();
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(hits.load(Ordering::SeqCst), settled);
    std::fs::remove_dir_all(file.parent().unwrap()).ok();
}

#[test]
fn unchanged_file_does_not_fire() {
    let file = tempfile("quiet");
    std::fs::write(&file, "a").unwrap();
    let hits = Arc::new(AtomicUsize::new(0));
    let counter = hits.clone();
    let handle = watch(file.clone(), Duration::from_millis(10), move || {
        counter.fetch_add(1, Ordering::SeqCst);
    });
    std::thread::sleep(Duration::from_millis(150));
    drop(handle);
    assert_eq!(hits.load(Ordering::SeqCst), 0);
    std::fs::remove_dir_all(file.parent().unwrap()).ok();
}

#[test]
fn drop_is_prompt_with_long_interval() {
    let file = tempfile("longint");
    std::fs::write(&file, "a").unwrap();
    let handle = watch(file.clone(), Duration::from_secs(60), || {});
    let start = Instant::now();
    drop(handle);
    assert!(start.elapsed() < Duration::from_secs(1));
    std::fs::remove_dir_all(file.parent().unwrap()).ok();
}

#[test]
fn file_appearing_fires() {
    let file = tempfile("appear");
    std::fs::remove_file(&file).ok();
    let hits = Arc::new(AtomicUsize::new(0));
    let counter = hits.clone();
    let handle = watch(file.clone(), Duration::from_millis(10), move || {
        counter.fetch_add(1, Ordering::SeqCst);
    });
    std::thread::sleep(Duration::from_millis(50));
    std::fs::write(&file, "created").unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while hits.load(Ordering::SeqCst) == 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(hits.load(Ordering::SeqCst) >= 1);
    drop(handle);
    std::fs::remove_dir_all(file.parent().unwrap()).ok();
}
