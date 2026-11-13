#![windows_subsystem = "windows"]

use std::io::Write;

fn main() {
    let exe = match std::env::current_exe() {
        Ok(path) => path,
        Err(err) => {
            report(&format!("Could not locate this launcher: {err}"));
            std::process::exit(1);
        }
    };

    if let Err(err) = binder_stub::launch_path(&exe) {
        report(&err);
        std::process::exit(1);
    }
}

fn report(message: &str) {
    if std::env::var_os("BINDER_NO_UI").is_some() {
        let _ = writeln!(std::io::stderr(), "{message}");
        let _ = std::fs::write(std::env::temp_dir().join("binder-error.txt"), message);
        return;
    }
    message_box("Binder", message);
}

fn message_box(title: &str, text: &str) {
    use std::ffi::c_void;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn MessageBoxW(hwnd: *mut c_void, text: *const u16, caption: *const u16, utype: u32)
        -> i32;
    }

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    let text = wide(text);
    let title = wide(title);
    unsafe {
        MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), 0x10);
    }
}
