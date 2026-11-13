use std::ffi::c_void;
use std::fs::{self, File};
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::process::Command;

#[test]
fn icon_survives_in_a_launcher_that_still_starts() {
    let stub = std::path::PathBuf::from(env!("CARGO_BIN_EXE_binder-stub"));
    let fixture = std::path::PathBuf::from(env!("CARGO_BIN_EXE_binder-fixture"));
    let work = std::env::temp_dir().join(format!("binder-icon-{}", std::process::id()));
    let _ = fs::remove_dir_all(&work);
    fs::create_dir_all(&work).unwrap();

    let marked = work.join("marked.exe");
    fs::copy(&stub, &marked).unwrap();
    let icon = work.join("mark.ico");
    let marker = b"BNDRICON";
    fs::write(&icon, binder_stub::basic_icon(marker)).unwrap();
    binder_stub::apply_icon(&marked, &icon).unwrap();

    let stub_bytes = fs::read(&marked).unwrap();
    assert!(
        stub_bytes
            .windows(marker.len())
            .any(|window| window == marker),
        "icon bytes were not written into the executable"
    );
    assert_eq!(extracted_icon_count(&marked), 1);

    let merged_path = work.join("merged.exe");
    let mut merged = File::create(&merged_path).unwrap();
    binder_stub::write_bundle(&stub_bytes, &[("fixture.exe".into(), fixture)], &mut merged)
        .unwrap();
    drop(merged);
    assert_eq!(extracted_icon_count(&merged_path), 1);

    let mut merged = File::open(&merged_path).unwrap();
    let (_, index) = binder_stub::read_bundle(&mut merged).unwrap();
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
        .unwrap();
    if !status.success() {
        let detail = fs::read_to_string(&error_log).unwrap_or_default();
        panic!("launcher exited with {status}: {detail}");
    }
    let marker_file = extract.join("fixture.ran");
    let mut started = marker_file.is_file();
    for _ in 0..50 {
        if marker_file.is_file() {
            started = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(started, "bundled executable did not start");
    let _ = fs::remove_dir_all(&extract);
    let _ = fs::remove_dir_all(&work);
}

fn extracted_icon_count(path: &Path) -> u32 {
    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut icon = std::ptr::null_mut();
    let mut id = 0u32;
    let count = unsafe { PrivateExtractIconsW(wide.as_ptr(), 0, 32, 32, &mut icon, &mut id, 1, 0) };
    if !icon.is_null() {
        unsafe { DestroyIcon(icon) };
    }
    count
}

#[link(name = "user32")]
unsafe extern "system" {
    fn PrivateExtractIconsW(
        file: *const u16,
        index: i32,
        width: i32,
        height: i32,
        icons: *mut *mut c_void,
        ids: *mut u32,
        count: u32,
        flags: u32,
    ) -> u32;
    fn DestroyIcon(icon: *mut c_void) -> i32;
}
