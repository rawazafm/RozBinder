use std::fs::{self, File};
use std::path::PathBuf;
use std::process::Command;

#[test]
fn launcher_starts_bundled_exe() {
    let stub = PathBuf::from(env!("CARGO_BIN_EXE_binder-stub"));
    let fixture = PathBuf::from(env!("CARGO_BIN_EXE_binder-fixture"));
    let work = std::env::temp_dir().join(format!("binder-launch-{}", std::process::id()));
    let _ = fs::remove_dir_all(&work);
    fs::create_dir_all(&work).unwrap();

    let merged_path = work.join("merged.exe");
    let stub_bytes = fs::read(&stub).unwrap();
    let mut merged = File::create(&merged_path).unwrap();
    binder_stub::write_bundle(&stub_bytes, &[("fixture.exe".into(), fixture)], &mut merged)
        .unwrap();
    drop(merged);

    let mut merged = File::open(&merged_path).unwrap();
    let (entries, index) = binder_stub::read_bundle(&mut merged).unwrap();
    assert_eq!(entries[0].name, "fixture.exe");
    drop(merged);

    let extract = std::env::temp_dir()
        .join("binder")
        .join(format!("{:016x}", binder_stub::bundle_key(&index)));
    let _ = fs::remove_dir_all(&extract);
    let error_log = std::env::temp_dir().join("binder-error.txt");
    let _ = fs::remove_file(&error_log);

    let status = Command::new(&merged_path)
        .env("BINDER_NO_UI", "1")
        .status()
        .expect("merged launcher should start");
    if !status.success() {
        let detail = fs::read_to_string(&error_log).unwrap_or_default();
        panic!("launcher exited with {status}: {detail}");
    }

    let marker = extract.join("fixture.ran");
    let mut started = marker.is_file();
    for _ in 0..50 {
        if marker.is_file() {
            started = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(started, "bundled executable did not start");

    let _ = fs::remove_dir_all(&extract);
    let _ = fs::remove_dir_all(&work);
}
