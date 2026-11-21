use std::{ffi::c_void, io, ptr};

use crate::storage::{Disk, Partition};

const DISK_LAYOUT: u32 = 0x0007_0050;
const DISK_GEOMETRY: u32 = 0x0007_00a0;

type Handle = *mut c_void;

#[link(name = "kernel32")]
unsafe extern "system" {
    fn QueryDosDeviceW(name: *const u16, target: *mut u16, capacity: u32) -> u32;
    fn CreateFileW(name: *const u16, access: u32, share: u32, security: *mut c_void,
        mode: u32, flags: u32, template: Handle) -> Handle;
    fn CloseHandle(handle: Handle) -> i32;
    fn GetLogicalDrives() -> u32;
    fn GetVolumeInformationW(root: *const u16, name: *mut u16, name_len: u32, serial: *mut u32,
        max_component: *mut u32, flags: *mut u32, fs: *mut u16, fs_len: u32) -> i32;
    fn GetDiskFreeSpaceExW(root: *const u16, available: *mut u64, total: *mut u64, free: *mut u64) -> i32;
    fn DeviceIoControl(handle: Handle, code: u32, input: *const c_void, input_size: u32,
        output: *mut u8, capacity: u32, size: *mut u32, overlapped: *mut c_void) -> i32;
}

struct Device(Handle);

impl Drop for Device {
    fn drop(&mut self) { unsafe { CloseHandle(self.0); } }
}

impl Device {
    fn open(name: &str) -> io::Result<Self> {
        let dos: Vec<u16> = format!(r"\\.\{name}").encode_utf16().chain([0]).collect();
        // Zero access is enough for FILE_ANY_ACCESS storage queries. No write handle is opened.
        let handle = unsafe { CreateFileW(dos.as_ptr(), 0, 7, ptr::null_mut(), 3, 0, ptr::null_mut()) };
        if handle as isize == -1 { Err(io::Error::last_os_error()) } else { Ok(Self(handle)) }
    }

    fn query(&self, code: u32, buffer: &mut [u8]) -> io::Result<usize> {
        let mut length = 0;
        let ok = unsafe { DeviceIoControl(self.0, code, ptr::null(), 0, buffer.as_mut_ptr(),
            buffer.len() as u32, &mut length, ptr::null_mut()) };
        if ok == 0 { Err(io::Error::last_os_error()) } else { Ok(length as usize) }
    }
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn u64_at(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}

fn guid(bytes: &[u8]) -> String {
    format!("{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        u32_at(bytes, 0), u16::from_le_bytes([bytes[4], bytes[5]]),
        u16::from_le_bytes([bytes[6], bytes[7]]),
        bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15])
}

fn parse_layout(name: &str, size: u64, bytes: &[u8]) -> io::Result<Disk> {
    const HEADER: usize = 48;
    const RECORD: usize = 144;
    if bytes.len() < HEADER { return Err(io::Error::other("Resposta incompleta da tabela de partições")); }
    let count = u32_at(bytes, 4) as usize;
    if count > (bytes.len() - HEADER) / RECORD {
        return Err(io::Error::other("Tabela de partições truncada"));
    }
    let style = u32_at(bytes, 0);
    let scheme = match style { 0 => "MBR", 1 => "GPT", 2 => "RAW", _ => "Desconhecido" };
    let partitions = bytes[HEADER..].chunks_exact(RECORD).take(count).filter_map(|record| {
        let number = u32_at(record, 24);
        let size = u64_at(record, 16);
        if number == 0 || size == 0 { return None; }
        let gpt = u32_at(record, 0) == 1;
        let label = gpt.then(|| String::from_utf16_lossy(&record[72..144].chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]])).take_while(|&char| char != 0).collect::<Vec<_>>()));
        Some(Partition {
            id: format!("Partição {number}"), size, offset: Some(u64_at(record, 8)),
            part_type: gpt.then(|| guid(&record[32..48])),
            uuid: gpt.then(|| guid(&record[48..64])),
            label, filesystem: None, available: None, used: None, mount: None, volume_label: None,
        })
    }).collect();
    Ok(Disk {
        id: format!("Disco {}", name.trim_start_matches("PhysicalDrive")), size,
        partitions, scheme: Some(scheme.into()), table_uuid: (style == 1).then(|| guid(&bytes[8..24])),
        model: None, transport: None,
    })
}

pub fn inventory() -> Result<Vec<Disk>, String> {
    let mut names = vec![0u16; 65_536];
    let count = unsafe { QueryDosDeviceW(ptr::null(), names.as_mut_ptr(), names.len() as u32) };
    if count == 0 { return Err(io::Error::last_os_error().to_string()); }
    let mut result = Vec::new();
    let mut failures = Vec::new();
    for name in names[..count as usize].split(|&ch| ch == 0).map(String::from_utf16_lossy) {
        if !name.strip_prefix("PhysicalDrive").is_some_and(|n| n.parse::<u32>().is_ok()) { continue; }
        let disk = (|| {
            let device = Device::open(&name)?;
            let mut geometry = [0; 256];
            let size = device.query(DISK_GEOMETRY, &mut geometry)?;
            if size < 32 { return Err(io::Error::other("Geometria do disco incompleta")); }
            let mut layout = vec![0; 256 * 1024];
            let length = device.query(DISK_LAYOUT, &mut layout)?;
            parse_layout(&name, u64_at(&geometry, 24), &layout[..length])
        })();
        match disk {
            Ok(disk) => result.push(disk),
            Err(error) => failures.push(format!("{name}: {error}")),
        }
    }
    if result.is_empty() && !failures.is_empty() {
        return Err(format!("A leitura direta dos discos também falhou: {}", failures.join("; ")));
    }
    attach_volumes(&mut result);
    Ok(result)
}

fn attach_volumes(disks: &mut [Disk]) {
    let mask = unsafe { GetLogicalDrives() };
    for i in 0..26 {
        if mask & (1 << i) == 0 { continue; }
        let drive = (b'A' + i) as char;
        let Ok(volume) = Device::open(&format!("{drive}:")) else { continue };
        let mut extents = [0; 32];
        let Ok(size) = volume.query(0x0056_0000, &mut extents) else { continue };
        if size < 32 || u32_at(&extents, 0) != 1 { continue; }
        let disk_id = format!("Disco {}", u32_at(&extents, 8));
        let offset = u64_at(&extents, 16);
        let Some(partition) = disks.iter_mut().find(|disk| disk.id == disk_id)
            .and_then(|disk| disk.partitions.iter_mut().find(|part| part.offset == Some(offset))) else { continue };
        let root = format!("{drive}:\\");
        let wide: Vec<u16> = root.encode_utf16().chain([0]).collect();
        let mut label = [0u16; 261];
        let mut fs = [0u16; 80];
        partition.mount = Some(root);
        // These calls only inspect the existing mount point.
        let success = unsafe { GetVolumeInformationW(wide.as_ptr(), label.as_mut_ptr(), label.len() as u32,
            ptr::null_mut(), ptr::null_mut(), ptr::null_mut(), fs.as_mut_ptr(), fs.len() as u32) };
        if success != 0 {
            let as_text = |value: &[u16]| String::from_utf16_lossy(&value[..value.iter().position(|ch| *ch == 0).unwrap_or(value.len())]);
            partition.volume_label = Some(as_text(&label));
            partition.filesystem = Some(as_text(&fs));
        }
        let (mut available, mut total, mut free) = (0, 0, 0);
        if unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut available, &mut total, &mut free) } != 0 {
            partition.available = Some(free);
            partition.used = Some(total.saturating_sub(free));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_gpt_esp_from_native_layout() {
        let mut data = [0; 192];
        data[0] = 1;
        data[4] = 1;
        let record = &mut data[48..];
        record[0] = 1;
        record[8..16].copy_from_slice(&(1u64 << 20).to_le_bytes());
        record[16..24].copy_from_slice(&(512u64 << 20).to_le_bytes());
        record[24] = 1;
        record[32..48].copy_from_slice(&[0x28, 0x73, 0x2a, 0xc1, 0x1f, 0xf8, 0xd2, 0x11,
            0xba, 0x4b, 0x00, 0xa0, 0xc9, 0x3e, 0xc9, 0x3b]);
        let disk = parse_layout("PhysicalDrive4", 1 << 30, &data).unwrap();
        assert_eq!(disk.partitions[0].offset, Some(1 << 20));
        assert!(disk.partitions[0].is_esp());
        assert!(parse_layout("PhysicalDrive4", 1 << 30, &data[..100]).is_err());
    }

    #[test]
    #[ignore = "Inventário real somente leitura desta máquina"]
    fn reads_local_disks() {
        let disks = inventory().unwrap();
        assert!(disks.iter().any(|disk| !disk.partitions.is_empty()));
        println!("Discos: {}; partições: {}", disks.len(), disks.iter().map(|disk| disk.partitions.len()).sum::<usize>());
    }
}
