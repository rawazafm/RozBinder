use std::path::PathBuf;
use std::process::Command;

fn main() {
    build_stub();
    tauri_build::build();
}

fn build_stub() {
    let manifest_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let stub_dir = manifest_dir.join("../stub");
    println!("cargo:rerun-if-changed=../stub/Cargo.toml");
    println!("cargo:rerun-if-changed=../stub/src/lib.rs");
    println!("cargo:rerun-if-changed=../stub/src/main.rs");
    println!("cargo:rerun-if-changed=../stub/src/format.rs");

    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let mut cmd = Command::new(cargo);
    cmd.current_dir(&stub_dir)
        .arg("build")
        .arg("--release")
        .arg("--bin")
        .arg("binder-stub")
        .env("CARGO_TARGET_DIR", stub_dir.join("target"));

    for (key, _) in std::env::vars() {
        let remove = (key.starts_with("CARGO_") && key != "CARGO" && key != "CARGO_HOME")
            || key == "MAKEFLAGS"
            || key == "MFLAGS";
        if remove {
            cmd.env_remove(&key);
        }
    }
    cmd.env("CARGO_TARGET_DIR", stub_dir.join("target"));

    let status = cmd.status().unwrap_or_else(|err| {
        panic!("could not start cargo to build the launcher stub: {err}");
    });
    if !status.success() {
        panic!("could not build the launcher stub");
    }
}
