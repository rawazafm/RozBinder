use std::path::Path;

const MAX_ICON_BYTES: usize = 8 * 1024 * 1024;
const MAX_IMAGES: usize = 32;

struct IconImage {
    id: u16,
    group_entry: [u8; 14],
    data: Vec<u8>,
}

pub fn parse_ico(bytes: &[u8]) -> Result<(), String> {
    let _ = read_images(bytes)?;
    Ok(())
}

pub fn apply_icon(exe: &Path, ico: &Path) -> Result<(), String> {
    let bytes = std::fs::read(ico).map_err(|err| format!("Could not read the icon: {err}"))?;
    let images = read_images(&bytes)?;
    write_icon_resources(exe, &images)
}

fn read_images(bytes: &[u8]) -> Result<Vec<IconImage>, String> {
    if bytes.len() < 6 || bytes.len() > MAX_ICON_BYTES {
        return Err("Choose a valid .ico file.".into());
    }
    let reserved = u16::from_le_bytes([bytes[0], bytes[1]]);
    let kind = u16::from_le_bytes([bytes[2], bytes[3]]);
    let count = u16::from_le_bytes([bytes[4], bytes[5]]) as usize;
    if reserved != 0 || kind != 1 || count == 0 || count > MAX_IMAGES {
        return Err("Choose a valid .ico file.".into());
    }
    let directory_end = 6 + count * 16;
    if bytes.len() < directory_end {
        return Err("The icon file is incomplete.".into());
    }

    let mut images = Vec::with_capacity(count);
    for index in 0..count {
        let entry = &bytes[6 + index * 16..6 + (index + 1) * 16];
        let size = u32::from_le_bytes(entry[8..12].try_into().unwrap()) as usize;
        let offset = u32::from_le_bytes(entry[12..16].try_into().unwrap()) as usize;
        let end = offset
            .checked_add(size)
            .filter(|end| *end <= bytes.len() && size > 0)
            .ok_or_else(|| "The icon file is incomplete.".to_string())?;
        let mut group_entry = [0u8; 14];
        group_entry[..12].copy_from_slice(&entry[..12]);
        let id = (index + 1) as u16;
        group_entry[12..14].copy_from_slice(&id.to_le_bytes());
        images.push(IconImage {
            id,
            group_entry,
            data: bytes[offset..end].to_vec(),
        });
    }
    Ok(images)
}

fn write_icon_resources(exe: &Path, images: &[IconImage]) -> Result<(), String> {
    let mut group = Vec::with_capacity(6 + images.len() * 14);
    group.extend_from_slice(&0u16.to_le_bytes());
    group.extend_from_slice(&1u16.to_le_bytes());
    group.extend_from_slice(&(images.len() as u16).to_le_bytes());
    for image in images {
        group.extend_from_slice(&image.group_entry);
    }

    let path = wide_path(exe);
    let handle = unsafe { BeginUpdateResourceW(path.as_ptr(), 0) };
    if handle.is_null() {
        return Err(last_error("Could not update the launcher icon"));
    }

    let written = write_resources(handle, images, &group);
    let discard = if written.is_ok() { 0 } else { 1 };
    let saved = unsafe { EndUpdateResourceW(handle, discard) };
    written?;
    if saved == 0 {
        return Err(last_error("Could not save the launcher icon"));
    }
    Ok(())
}

fn write_resources(
    handle: *mut std::ffi::c_void,
    images: &[IconImage],
    group: &[u8],
) -> Result<(), String> {
    for image in images {
        update_resource(handle, RT_ICON, image.id as usize, &image.data)?;
    }
    update_resource(handle, RT_GROUP_ICON, 1, group)
}

fn update_resource(
    handle: *mut std::ffi::c_void,
    kind: usize,
    id: usize,
    data: &[u8],
) -> Result<(), String> {
    let size = u32::try_from(data.len()).map_err(|_| "The icon is too large.".to_string())?;
    let ok = unsafe {
        UpdateResourceW(
            handle,
            kind as *const u16,
            id as *const u16,
            0x0409,
            data.as_ptr(),
            size,
        )
    };
    if ok == 0 {
        Err(last_error("Could not write the icon"))
    } else {
        Ok(())
    }
}

fn wide_path(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn last_error(action: &str) -> String {
    let code = unsafe { GetLastError() };
    format!("{action} (Windows error {code}).")
}

const RT_ICON: usize = 3;
const RT_GROUP_ICON: usize = 14;

#[link(name = "kernel32")]
unsafe extern "system" {
    fn BeginUpdateResourceW(file_name: *const u16, delete_existing: i32) -> *mut std::ffi::c_void;
    fn UpdateResourceW(
        handle: *mut std::ffi::c_void,
        resource_type: *const u16,
        name: *const u16,
        language: u16,
        data: *const u8,
        size: u32,
    ) -> i32;
    fn EndUpdateResourceW(handle: *mut std::ffi::c_void, discard: i32) -> i32;
    fn GetLastError() -> u32;
}

#[doc(hidden)]
pub fn basic_icon(marker: &[u8]) -> Vec<u8> {
    const WIDTH: usize = 16;
    let xor_size = WIDTH * WIDTH * 4;
    let mask_stride = 4;
    let mask_size = mask_stride * WIDTH;
    let mut image = vec![0u8; 40 + xor_size + mask_size];
    image[0..4].copy_from_slice(&40u32.to_le_bytes());
    image[4..8].copy_from_slice(&16i32.to_le_bytes());
    image[8..12].copy_from_slice(&32i32.to_le_bytes());
    image[12..14].copy_from_slice(&1u16.to_le_bytes());
    image[14..16].copy_from_slice(&32u16.to_le_bytes());
    image[20..24].copy_from_slice(&(xor_size as u32).to_le_bytes());
    let copy = marker.len().min(xor_size);
    image[40..40 + copy].copy_from_slice(&marker[..copy]);

    let mut file = Vec::with_capacity(22 + image.len());
    file.extend_from_slice(&0u16.to_le_bytes());
    file.extend_from_slice(&1u16.to_le_bytes());
    file.extend_from_slice(&1u16.to_le_bytes());
    file.push(WIDTH as u8);
    file.push(WIDTH as u8);
    file.push(0);
    file.push(0);
    file.extend_from_slice(&1u16.to_le_bytes());
    file.extend_from_slice(&32u16.to_le_bytes());
    file.extend_from_slice(&(image.len() as u32).to_le_bytes());
    file.extend_from_slice(&22u32.to_le_bytes());
    file.extend_from_slice(&image);
    file
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_generated_icon() {
        parse_ico(&basic_icon(b"BNDRICON")).unwrap();
    }

    #[test]
    fn rejects_a_file_that_is_not_an_icon() {
        assert!(parse_ico(b"not an icon").is_err());
    }
}
