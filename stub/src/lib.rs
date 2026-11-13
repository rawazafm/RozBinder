use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

mod format;
mod icon;

pub use format::{FOOTER_LEN, FileEntry, Footer, bundle_key, validate_entry_name};
pub use icon::{apply_icon, basic_icon, parse_ico};

struct Bundle {
    entries: Vec<FileEntry>,
    index: Vec<u8>,
}

pub fn write_bundle(
    stub: &[u8],
    inputs: &[(String, PathBuf)],
    output: &mut File,
) -> Result<(), String> {
    if stub.is_empty() {
        return Err("The launcher stub is empty.".into());
    }
    if inputs.is_empty() {
        return Err("Add at least one executable.".into());
    }

    output
        .write_all(stub)
        .map_err(|err| format!("Could not write the launcher: {err}"))?;

    let mut entries = Vec::with_capacity(inputs.len());
    let mut used = HashSet::new();
    for (name, source) in inputs {
        format::validate_entry_name(name)?;
        if !used.insert(name.clone()) {
            return Err(format!("{name} is listed more than once."));
        }
        let offset = output
            .stream_position()
            .map_err(|err| format!("Could not write the launcher: {err}"))?;
        let mut file = File::open(source)
            .map_err(|err| format!("Could not read {}: {err}", source.display()))?;
        let size = io::copy(&mut file, output)
            .map_err(|err| format!("Could not copy {}: {err}", source.display()))?;
        entries.push(FileEntry {
            name: name.clone(),
            offset,
            size,
        });
    }

    let index = format::encode_index(&entries)?;
    let index_off = output
        .stream_position()
        .map_err(|err| format!("Could not write the launcher: {err}"))?;
    output
        .write_all(&index)
        .map_err(|err| format!("Could not write the launcher: {err}"))?;
    let footer = format::encode_footer(stub.len() as u64, index_off, index.len() as u64);
    output
        .write_all(&footer)
        .map_err(|err| format!("Could not write the launcher: {err}"))?;
    Ok(())
}

pub fn read_bundle(file: &mut File) -> Result<(Vec<FileEntry>, Vec<u8>), String> {
    let bundle = read_bundle_inner(file)?;
    Ok((bundle.entries, bundle.index))
}

fn read_bundle_inner(file: &mut File) -> Result<Bundle, String> {
    let len = file
        .metadata()
        .map_err(|err| format!("Could not read the launcher: {err}"))?
        .len();
    if len < FOOTER_LEN {
        return Err("This launcher does not contain any programs.".into());
    }

    file.seek(SeekFrom::End(-(FOOTER_LEN as i64)))
        .map_err(|err| format!("Could not read the launcher: {err}"))?;
    let mut footer_buf = [0u8; FOOTER_LEN as usize];
    file.read_exact(&mut footer_buf)
        .map_err(|err| format!("Could not read the launcher: {err}"))?;
    let footer = format::decode_footer(&footer_buf)?;

    let index_end = footer
        .index_off
        .checked_add(footer.index_len)
        .ok_or_else(|| "The merged file is damaged.".to_string())?;
    if footer.stub_len == 0
        || footer.index_off < footer.stub_len
        || index_end.checked_add(FOOTER_LEN) != Some(len)
    {
        return Err("The merged file is damaged.".into());
    }

    file.seek(SeekFrom::Start(footer.index_off))
        .map_err(|err| format!("Could not read the launcher: {err}"))?;
    let mut index = vec![0u8; footer.index_len as usize];
    file.read_exact(&mut index)
        .map_err(|err| format!("Could not read the launcher: {err}"))?;
    let entries = format::decode_index(&index)?;

    for entry in &entries {
        let end = entry
            .offset
            .checked_add(entry.size)
            .ok_or_else(|| "The merged file is damaged.".to_string())?;
        if entry.offset < footer.stub_len || end > footer.index_off {
            return Err("The merged file is damaged.".into());
        }
    }

    Ok(Bundle { entries, index })
}

pub fn launch_path(exe: &Path) -> Result<(), String> {
    let mut file = File::open(exe).map_err(|err| format!("Could not open this launcher: {err}"))?;
    let bundle = read_bundle_inner(&mut file)?;
    let dir = std::env::temp_dir()
        .join("binder")
        .join(format!("{:016x}", bundle_key(&bundle.index)));
    fs::create_dir_all(&dir)
        .map_err(|err| format!("Could not create a temporary folder: {err}"))?;

    let mut errors = Vec::new();
    for entry in &bundle.entries {
        let dest = safe_destination(&dir, &entry.name)?;
        if let Err(err) = extract_entry(&mut file, entry, &dest) {
            let reusable = dest
                .metadata()
                .map(|meta| meta.is_file() && meta.len() == entry.size)
                .unwrap_or(false);
            if !reusable {
                return Err(err);
            }
        }
        if !entry.name.to_ascii_lowercase().ends_with(".exe") {
            errors.push(format!("{} is not an executable.", entry.name));
            continue;
        }
        match Command::new(&dest).current_dir(&dir).spawn() {
            Ok(_) => {}
            Err(err) => errors.push(format!("{} failed to start: {err}", entry.name)),
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("\n"))
    }
}

fn safe_destination(dir: &Path, name: &str) -> Result<PathBuf, String> {
    format::validate_entry_name(name)?;
    let dest = dir.join(name);
    if dest.parent() != Some(dir) {
        return Err("The merged file is damaged.".into());
    }
    Ok(dest)
}

fn extract_entry(file: &mut File, entry: &FileEntry, dest: &Path) -> Result<(), String> {
    file.seek(SeekFrom::Start(entry.offset))
        .map_err(|err| format!("Could not read {}: {err}", entry.name))?;
    let part = dest.with_file_name(format!(".{}.part", entry.name));
    let copied = (|| -> Result<(), String> {
        let mut out = File::create(&part)
            .map_err(|err| format!("Could not extract {}: {err}", entry.name))?;
        let copied = io::copy(&mut Read::take(file, entry.size), &mut out)
            .map_err(|err| format!("Could not extract {}: {err}", entry.name))?;
        if copied != entry.size {
            return Err(format!("{} is incomplete in the merged file.", entry.name));
        }
        out.sync_all()
            .map_err(|err| format!("Could not extract {}: {err}", entry.name))?;
        Ok(())
    })();
    if let Err(err) = copied {
        let _ = fs::remove_file(&part);
        return Err(err);
    }
    if dest.exists() {
        fs::remove_file(dest)
            .map_err(|err| format!("Could not replace {}: {err}", dest.display()))?;
    }
    fs::rename(&part, dest).map_err(|err| format!("Could not extract {}: {err}", entry.name))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_and_read_preserves_payloads() {
        let dir = std::env::temp_dir().join(format!("binder-roundtrip-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let first = dir.join("a.exe");
        let second = dir.join("b.exe");
        fs::write(&first, b"MZ-first").unwrap();
        fs::write(&second, b"MZ-second-payload").unwrap();
        let merged_path = dir.join("merged.exe");
        let mut merged = File::create(&merged_path).unwrap();
        write_bundle(
            b"STUBDATA",
            &[
                ("a.exe".into(), first.clone()),
                ("b.exe".into(), second.clone()),
            ],
            &mut merged,
        )
        .unwrap();
        drop(merged);

        let mut merged = File::open(&merged_path).unwrap();
        let (entries, _) = read_bundle(&mut merged).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(read_entry(&mut merged, &entries[0]).unwrap(), b"MZ-first");
        assert_eq!(
            read_entry(&mut merged, &entries[1]).unwrap(),
            b"MZ-second-payload"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    fn read_entry(file: &mut File, entry: &FileEntry) -> io::Result<Vec<u8>> {
        file.seek(SeekFrom::Start(entry.offset))?;
        let mut buf = vec![0u8; entry.size as usize];
        file.read_exact(&mut buf)?;
        Ok(buf)
    }
}
