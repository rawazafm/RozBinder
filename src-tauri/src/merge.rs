use std::collections::HashSet;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use binder_stub::{parse_ico, validate_entry_name, write_bundle};

const STUB: &[u8] = include_bytes!("../../stub/target/release/binder-stub.exe");

#[derive(serde::Serialize)]
pub struct ListedFile {
    pub path: String,
    pub name: String,
    pub size: u64,
}

#[derive(serde::Serialize)]
pub struct DescribeResult {
    pub files: Vec<ListedFile>,
    pub errors: Vec<String>,
}

#[derive(serde::Serialize)]
pub struct MergeReport {
    pub output: String,
    pub files: usize,
    pub bytes: u64,
    pub names: Vec<String>,
    pub icon: bool,
}

#[derive(serde::Serialize)]
pub struct IconChoice {
    pub path: String,
    pub name: String,
}

#[tauri::command]
pub fn describe_files(paths: Vec<String>) -> DescribeResult {
    let mut files = Vec::new();
    let mut errors = Vec::new();
    let mut seen = HashSet::new();
    for path in paths {
        match inspect(&path) {
            Ok(file) => {
                if seen.insert(file.path.to_ascii_lowercase()) {
                    files.push(file);
                }
            }
            Err(err) => errors.push(err),
        }
    }
    DescribeResult { files, errors }
}

#[tauri::command]
pub fn inspect_icon(path: String) -> Result<IconChoice, String> {
    let path_buf = PathBuf::from(&path);
    let name = path_buf
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Choose a valid .ico file.".to_string())?
        .to_string();
    if !name.to_ascii_lowercase().ends_with(".ico") {
        return Err(format!("{name} is not an .ico file."));
    }
    let bytes = fs::read(&path_buf).map_err(|err| format!("Could not read {path}: {err}"))?;
    parse_ico(&bytes)?;
    let canonical = fs::canonicalize(&path_buf).unwrap_or(path_buf);
    Ok(IconChoice {
        path: display_path(&canonical),
        name,
    })
}

#[tauri::command]
pub async fn merge_exes(
    inputs: Vec<String>,
    output: String,
    icon: Option<String>,
) -> Result<MergeReport, String> {
    tauri::async_runtime::spawn_blocking(move || assemble(inputs, output, icon))
        .await
        .map_err(|_| "Could not finish the merge.".to_string())?
}

fn inspect(path: &str) -> Result<ListedFile, String> {
    let path_buf = PathBuf::from(path);
    let name = path_buf
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("{path} has a file name Binder cannot store."))?
        .to_string();
    if !is_exe_name(&name) {
        return Err(format!("{name} is not an .exe file."));
    }
    validate_entry_name(&name).map_err(|_| format!("{name} cannot be stored in the launcher."))?;

    let mut file = File::open(&path_buf).map_err(|err| format!("Could not open {path}: {err}"))?;
    let meta = file
        .metadata()
        .map_err(|err| format!("Could not read {path}: {err}"))?;
    if !meta.is_file() {
        return Err(format!("{path} is not a file."));
    }
    if meta.len() < 2 {
        return Err(format!("{name} is not a Windows executable."));
    }
    let mut mz = [0u8; 2];
    file.read_exact(&mut mz)
        .map_err(|err| format!("Could not read {path}: {err}"))?;
    if &mz != b"MZ" {
        return Err(format!("{name} is not a Windows executable."));
    }

    let canonical = fs::canonicalize(&path_buf).unwrap_or(path_buf);
    Ok(ListedFile {
        path: display_path(&canonical),
        name,
        size: meta.len(),
    })
}

fn assemble(
    inputs: Vec<String>,
    output: String,
    icon: Option<String>,
) -> Result<MergeReport, String> {
    if inputs.is_empty() {
        return Err("Add at least one executable.".into());
    }
    if inputs.len() > 100 {
        return Err("Merge 100 executables or fewer at a time.".into());
    }

    let output = normalize_output(PathBuf::from(output))?;
    let output = if output.is_absolute() {
        output
    } else {
        std::env::current_dir()
            .map_err(|err| format!("Could not resolve the output folder: {err}"))?
            .join(output)
    };
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() && !parent.is_dir() {
            return Err(format!("Folder does not exist: {}", parent.display()));
        }
    }

    let mut payloads = Vec::with_capacity(inputs.len());
    let mut used_names = HashSet::new();
    let mut seen_paths = HashSet::new();
    let mut bytes = 0u64;
    for input in &inputs {
        let listed = inspect(input)?;
        if !seen_paths.insert(listed.path.to_ascii_lowercase()) {
            return Err(format!("{} is listed more than once.", listed.name));
        }
        if paths_match(Path::new(&listed.path), &output) {
            return Err(
                "Choose an output file that is not one of the executables being merged.".into(),
            );
        }
        let name = unique_name(&used_names, &listed.name);
        validate_entry_name(&name)?;
        used_names.insert(name.clone());
        bytes = bytes.saturating_add(listed.size);
        payloads.push((name, PathBuf::from(listed.path)));
    }

    let has_icon = icon.as_deref().is_some_and(|path| !path.trim().is_empty());
    let stub = if has_icon {
        stub_with_icon(Path::new(icon.as_deref().unwrap().trim()))?
    } else {
        STUB.to_vec()
    };
    let partial = partial_path(&output);
    let result: Result<Vec<String>, String> = (|| {
        let mut file = File::create(&partial)
            .map_err(|err| format!("Could not create {}: {err}", partial.display()))?;
        let stored: Vec<(String, PathBuf)> = payloads
            .iter()
            .map(|(name, path)| (name.clone(), path.clone()))
            .collect();
        write_bundle(&stub, &stored, &mut file)?;
        file.sync_all()
            .map_err(|err| format!("Could not save the merged file: {err}"))?;
        drop(file);
        replace_file(&partial, &output)?;
        Ok(stored.into_iter().map(|(name, _)| name).collect::<Vec<_>>())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&partial);
    }
    let names: Vec<String> = result?;
    Ok(MergeReport {
        output: display_path(&output),
        files: names.len(),
        bytes,
        names,
        icon: has_icon,
    })
}

fn stub_with_icon(icon: &Path) -> Result<Vec<u8>, String> {
    let bytes = fs::read(icon).map_err(|err| format!("Could not read the icon: {err}"))?;
    parse_ico(&bytes)?;
    let tmp = std::env::temp_dir().join(format!(
        "binder-icon-{}-{}.exe",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0)
    ));
    if let Err(err) = fs::write(&tmp, STUB) {
        return Err(format!("Could not prepare the launcher: {err}"));
    }
    let applied = binder_stub::apply_icon(&tmp, icon)
        .and_then(|_| fs::read(&tmp).map_err(|err| format!("Could not read the launcher: {err}")));
    let _ = fs::remove_file(&tmp);
    applied
}

fn normalize_output(path: PathBuf) -> Result<PathBuf, String> {
    if path.as_os_str().is_empty() {
        return Err("Choose where to save the merged executable.".into());
    }
    let path = match path.extension().and_then(|ext| ext.to_str()) {
        Some(ext) if ext.eq_ignore_ascii_case("exe") => path,
        _ => path.with_extension("exe"),
    };
    if path.file_name().is_none() {
        return Err("Choose a file name for the merged executable.".into());
    }
    Ok(path)
}

fn unique_name(used: &HashSet<String>, file_name: &str) -> String {
    if !used.contains(file_name) {
        return file_name.to_string();
    }
    let path = Path::new(file_name);
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(file_name);
    let ext = path.extension().and_then(|ext| ext.to_str());
    let mut index = 2u32;
    loop {
        let candidate = match ext {
            Some(ext) => format!("{stem} ({index}).{ext}"),
            None => format!("{stem} ({index})"),
        };
        if !used.contains(&candidate) {
            return candidate;
        }
        index += 1;
    }
}

fn partial_path(output: &Path) -> PathBuf {
    let mut name = output.file_name().unwrap_or_default().to_os_string();
    name.push(".partial");
    output.with_file_name(name)
}

fn replace_file(partial: &Path, output: &Path) -> Result<(), String> {
    if output.exists() {
        fs::remove_file(output)
            .map_err(|err| format!("Could not replace {}: {err}", output.display()))?;
    }
    fs::rename(partial, output)
        .map_err(|err| format!("Could not save {}: {err}", output.display()))?;
    Ok(())
}

fn paths_match(input: &Path, output: &Path) -> bool {
    match (fs::canonicalize(input), fs::canonicalize(output)) {
        (Ok(input), Ok(output)) => input == output,
        _ => input == output,
    }
}

fn is_exe_name(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
}

fn display_path(path: &Path) -> String {
    let text = path.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{rest}");
    }
    text.strip_prefix(r"\\?\").unwrap_or(&text).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_executables_into_a_readable_launcher() {
        let dir = std::env::temp_dir().join(format!("binder-merge-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let first = dir.join("alpha.exe");
        let second = dir.join("beta.exe");
        fs::write(&first, b"MZ-alpha").unwrap();
        fs::write(&second, b"MZ-beta-data").unwrap();

        let report = assemble(
            vec![
                first.to_string_lossy().into_owned(),
                second.to_string_lossy().into_owned(),
            ],
            dir.join("packed.exe").to_string_lossy().into_owned(),
            None,
        )
        .unwrap();

        assert_eq!(report.files, 2);
        assert_eq!(report.names, vec!["alpha.exe", "beta.exe"]);
        let merged = fs::read(&report.output).unwrap();
        assert!(merged.starts_with(STUB));
        let mut file = File::open(&report.output).unwrap();
        let (entries, _) = binder_stub::read_bundle(&mut file).unwrap();
        assert_eq!(entries[0].name, "alpha.exe");
        assert_eq!(entries[1].name, "beta.exe");
        assert_eq!(read_at(&mut file, &entries[0]).unwrap(), b"MZ-alpha");
        assert_eq!(read_at(&mut file, &entries[1]).unwrap(), b"MZ-beta-data");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn merge_embeds_the_selected_icon() {
        let dir = std::env::temp_dir().join(format!("binder-icon-merge-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let payload = dir.join("alpha.exe");
        fs::write(&payload, b"MZ-icon").unwrap();
        let icon = dir.join("app.ico");
        fs::write(&icon, binder_stub::basic_icon(b"BNDRICON")).unwrap();

        let report = assemble(
            vec![payload.to_string_lossy().into_owned()],
            dir.join("packed.exe").to_string_lossy().into_owned(),
            Some(icon.to_string_lossy().into_owned()),
        )
        .unwrap();

        assert!(report.icon);
        let merged = fs::read(&report.output).unwrap();
        assert!(merged.windows(8).any(|window| window == b"BNDRICON"));
        let mut file = File::open(&report.output).unwrap();
        let (entries, _) = binder_stub::read_bundle(&mut file).unwrap();
        assert_eq!(read_at(&mut file, &entries[0]).unwrap(), b"MZ-icon");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_a_file_that_is_not_a_windows_executable() {
        let dir = std::env::temp_dir().join(format!("binder-reject-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let text = dir.join("notes.exe");
        fs::write(&text, b"hello").unwrap();
        let described = describe_files(vec![text.to_string_lossy().into_owned()]);
        assert!(described.files.is_empty());
        assert!(!described.errors.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    fn read_at(file: &mut File, entry: &binder_stub::FileEntry) -> std::io::Result<Vec<u8>> {
        use std::io::{Seek, SeekFrom};
        file.seek(SeekFrom::Start(entry.offset))?;
        let mut buf = vec![0u8; entry.size as usize];
        file.read_exact(&mut buf)?;
        Ok(buf)
    }
}
