use std::path::Path;

pub const MAGIC: [u8; 8] = *b"EXEMRG01";
pub const FOOTER_LEN: u64 = 32;
const MAX_FILES: u32 = 10_000;
const MAX_NAME_LEN: u32 = 200;
const MAX_INDEX_LEN: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    pub name: String,
    pub offset: u64,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Footer {
    pub stub_len: u64,
    pub index_off: u64,
    pub index_len: u64,
}

pub fn validate_entry_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name.len() > MAX_NAME_LEN as usize {
        return Err("A bundled file has an unusable name.".into());
    }
    if name == "." || name == ".." || name.ends_with('.') || name.ends_with(' ') {
        return Err(format!("Cannot bundle a file named {name}."));
    }
    if name.contains(['/', '\\', ':', '\0']) || name.chars().any(|ch| ch.is_control()) {
        return Err(format!("Cannot bundle a file named {name}."));
    }
    if Path::new(name).components().count() != 1 {
        return Err(format!("Cannot bundle a file named {name}."));
    }
    Ok(())
}

pub fn encode_index(entries: &[FileEntry]) -> Result<Vec<u8>, String> {
    if entries.is_empty() || entries.len() > MAX_FILES as usize {
        return Err("Add between 1 and 10000 executables.".into());
    }
    let mut buf = Vec::new();
    buf.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    for entry in entries {
        validate_entry_name(&entry.name)?;
        let name = entry.name.as_bytes();
        buf.extend_from_slice(&(name.len() as u32).to_le_bytes());
        buf.extend_from_slice(name);
        buf.extend_from_slice(&entry.offset.to_le_bytes());
        buf.extend_from_slice(&entry.size.to_le_bytes());
    }
    Ok(buf)
}

pub fn decode_index(data: &[u8]) -> Result<Vec<FileEntry>, String> {
    let mut pos = 0;
    let count = read_u32(data, &mut pos)?;
    if count == 0 || count > MAX_FILES {
        return Err("The merged file is damaged.".into());
    }
    let mut entries = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let name_len = read_u32(data, &mut pos)?;
        if name_len == 0 || name_len > MAX_NAME_LEN {
            return Err("The merged file is damaged.".into());
        }
        let name_bytes = read_bytes(data, &mut pos, name_len as usize)?;
        let name = std::str::from_utf8(name_bytes)
            .map_err(|_| "The merged file is damaged.".to_string())?;
        validate_entry_name(name)?;
        entries.push(FileEntry {
            name: name.to_string(),
            offset: read_u64(data, &mut pos)?,
            size: read_u64(data, &mut pos)?,
        });
    }
    if pos != data.len() {
        return Err("The merged file is damaged.".into());
    }
    Ok(entries)
}

pub fn encode_footer(stub_len: u64, index_off: u64, index_len: u64) -> [u8; 32] {
    let mut buf = [0u8; 32];
    buf[..8].copy_from_slice(&MAGIC);
    buf[8..16].copy_from_slice(&stub_len.to_le_bytes());
    buf[16..24].copy_from_slice(&index_off.to_le_bytes());
    buf[24..32].copy_from_slice(&index_len.to_le_bytes());
    buf
}

pub fn decode_footer(buf: &[u8]) -> Result<Footer, String> {
    if buf.len() != FOOTER_LEN as usize || buf[..8] != MAGIC {
        return Err("This launcher does not contain any programs.".into());
    }
    if u64::from_le_bytes(buf[24..32].try_into().unwrap()) > MAX_INDEX_LEN {
        return Err("The merged file is damaged.".into());
    }
    Ok(Footer {
        stub_len: u64::from_le_bytes(buf[8..16].try_into().unwrap()),
        index_off: u64::from_le_bytes(buf[16..24].try_into().unwrap()),
        index_len: u64::from_le_bytes(buf[24..32].try_into().unwrap()),
    })
}

pub fn bundle_key(index: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in index {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn read_u32(data: &[u8], pos: &mut usize) -> Result<u32, String> {
    let bytes = read_bytes(data, pos, 4)?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn read_u64(data: &[u8], pos: &mut usize) -> Result<u64, String> {
    let bytes = read_bytes(data, pos, 8)?;
    Ok(u64::from_le_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ]))
}

fn read_bytes<'a>(data: &'a [u8], pos: &mut usize, len: usize) -> Result<&'a [u8], String> {
    let end = pos
        .checked_add(len)
        .filter(|end| *end <= data.len())
        .ok_or_else(|| "The merged file is damaged.".to_string())?;
    let bytes = &data[*pos..end];
    *pos = end;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_roundtrip() {
        let entries = vec![
            FileEntry {
                name: "setup.exe".into(),
                offset: 120,
                size: 4,
            },
            FileEntry {
                name: "helper.exe".into(),
                offset: 124,
                size: 8,
            },
        ];
        let decoded = decode_index(&encode_index(&entries).unwrap()).unwrap();
        assert_eq!(decoded, entries);
    }

    #[test]
    fn footer_roundtrip() {
        let encoded = encode_footer(40, 90, 12);
        let footer = decode_footer(&encoded).unwrap();
        assert_eq!(
            footer,
            Footer {
                stub_len: 40,
                index_off: 90,
                index_len: 12,
            }
        );
    }

    #[test]
    fn rejects_path_names() {
        for name in ["../a.exe", r"..\a.exe", r"dir\a.exe", "", ".", ".."] {
            assert!(validate_entry_name(name).is_err(), "{name}");
        }
    }

    #[test]
    fn rejects_parent_directory_in_index() {
        let mut index = Vec::new();
        index.extend_from_slice(&1u32.to_le_bytes());
        let name = br"..\a.exe";
        index.extend_from_slice(&(name.len() as u32).to_le_bytes());
        index.extend_from_slice(name);
        index.extend_from_slice(&0u64.to_le_bytes());
        index.extend_from_slice(&1u64.to_le_bytes());
        assert!(decode_index(&index).is_err());
    }
}
