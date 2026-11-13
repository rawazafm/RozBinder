#![windows_subsystem = "windows"]

fn main() {
    let dir = std::env::current_dir().unwrap_or_else(|_| std::env::temp_dir());
    let stem = std::env::current_exe()
        .ok()
        .and_then(|path| {
            path.file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "payload".into());
    let _ = std::fs::write(dir.join(format!("{stem}.ran")), "ok");
}
