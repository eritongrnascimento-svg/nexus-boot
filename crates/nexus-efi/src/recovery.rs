use alloc::{format, string::{String, ToString}, vec::Vec};
use sha2::{Digest, Sha256};
use uefi::{CString16, Handle, boot, cstr16, fs::{FileSystem, PathBuf},
    proto::{device_path::{DevicePath, media::{HardDrive, PartitionSignature}}, media::fs::SimpleFileSystem}};

const MAX_FILE: u64 = 32 * 1024 * 1024;
const BACKUPS: &str = "\\EFI\\NexusBoot\\Backups";

pub struct Backup {
    pub name: String,
    uuid: String,
    vendor: String,
}

fn path(value: &str) -> Result<PathBuf, &'static str> {
    CString16::try_from(value).map(PathBuf::from).map_err(|_| "Caminho inválido")
}

fn vendor_ok(value: &str) -> bool {
    !value.is_empty() && value.len() <= 48 && value.as_bytes()[0].is_ascii_alphanumeric()
        && value.bytes().all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
        && !["boot", "nexusboot"].iter().any(|name| value.eq_ignore_ascii_case(name))
}

fn uuid_ok(value: &str) -> bool {
    value.len() == 36 && value.bytes().enumerate().all(|(i, b)|
        if [8, 13, 18, 23].contains(&i) { b == b'-' } else { b.is_ascii_hexdigit() })
}

pub fn targets() -> Vec<Backup> {
    let Ok(protocol) = boot::get_image_file_system(boot::image_handle()) else { return Vec::new() };
    let mut fs = FileSystem::new(protocol);
    let Ok(ids) = fs.read_dir(cstr16!("\\EFI\\NexusBoot\\Backups")) else { return Vec::new() };
    let mut backups = Vec::new();
    for uuid in ids.filter_map(Result::ok).filter(|info| info.is_directory())
        .map(|info| info.file_name().to_string()).filter(|id| uuid_ok(id)).take(32)
    {
        let Ok(root) = path(&format!("{BACKUPS}\\{uuid}")) else { continue };
        let Ok(vendors) = fs.read_dir(root) else { continue };
        for vendor in vendors.filter_map(Result::ok).filter(|info| info.is_directory())
            .map(|info| info.file_name().to_string()).filter(|name| vendor_ok(name)).take(16)
        {
            if backups.len() == 64 { return backups; }
            let Ok(marker) = path(&format!("{BACKUPS}\\{uuid}\\{vendor}\\ready")) else { continue };
            if !fs.try_exists(marker).unwrap_or(false) { continue; }
            let display = match vendor.to_ascii_lowercase().as_str() {
                "microsoft" => "Windows", "cachyos" => "CachyOS", "ubuntu" => "Ubuntu",
                "debian" => "Debian", "fedora" => "Fedora", "systemd" => "systemd-boot",
                _ => &vendor,
            };
            backups.push(Backup { name: format!("{display} · {}", &uuid[..8]), uuid: uuid.clone(), vendor });
        }
    }
    backups
}

fn partition_uuid(handle: Handle) -> Option<String> {
    let device = boot::open_protocol_exclusive::<DevicePath>(handle).ok()?;
    device.node_iter().find_map(|node| {
        let hd = <&HardDrive>::try_from(node).ok()?;
        match hd.partition_signature() {
            PartitionSignature::Guid(guid) => Some(guid.to_string()),
            _ => None,
        }
    })
}

fn read_limited(fs: &mut FileSystem, path: &PathBuf, limit: u64) -> Result<Vec<u8>, &'static str> {
    let info = fs.metadata(path).map_err(|_| "Arquivo de backup ou destino indisponível")?;
    if info.is_directory() || info.file_size() > limit { return Err("Arquivo fora do limite de recuperação"); }
    fs.read(path).map_err(|_| "Não foi possível ler o arquivo")
}

pub fn restore(backup: &Backup) -> Result<usize, &'static str> {
    let handles = boot::find_handles::<SimpleFileSystem>().map_err(|_| "Volumes EFI indisponíveis")?;
    let mut matching = handles.into_iter().filter(|handle|
        partition_uuid(*handle).is_some_and(|uuid| uuid.eq_ignore_ascii_case(&backup.uuid)));
    let target = matching.next().ok_or("A partição EFI original não foi encontrada")?;
    if matching.next().is_some() { return Err("Há mais de uma partição com o identificador original"); }
    let protocol = boot::get_image_file_system(boot::image_handle()).map_err(|_| "ESP do Nexus indisponível")?;
    let mut source = FileSystem::new(protocol);
    let windows = backup.vendor.eq_ignore_ascii_case("microsoft");
    let files: &[&str] = if windows { &["bootmgfw.efi", "BCD"] }
        else { &["shimx64.efi", "grubx64.efi", "grub.cfg", "systemd-bootx64.efi", "liminex64.efi", "mmx64.efi"] };
    let prefix = format!("{BACKUPS}\\{}\\{}", backup.uuid, backup.vendor);
    let dest = format!("\\EFI\\{}{}", backup.vendor, if windows { "\\Boot" } else { "" });
    let mut verified = Vec::new();
    let mut total = 0;
    for file in files {
        let input = path(&format!("{prefix}\\{file}"))?;
        let checksum = path(&format!("{prefix}\\{file}.sha256"))?;
        let has_file = source.try_exists(&input).map_err(|_| "Erro ao consultar o backup")?;
        let has_checksum = source.try_exists(&checksum).map_err(|_| "Erro ao consultar o checksum")?;
        if !has_file && !has_checksum { continue; }
        let bytes = read_limited(&mut source, &input, MAX_FILE)?;
        total += bytes.len();
        if bytes.is_empty() || total > 64 * 1024 * 1024 { return Err("Cópia vazia ou maior que o limite de recuperação"); }
        let hash = read_limited(&mut source, &checksum, 80)?;
        let hash = core::str::from_utf8(&hash).map_err(|_| "Checksum inválido")?;
        if format!("{:x}", Sha256::digest(&bytes)) != hash.trim() { return Err("Integridade do backup inválida. Nenhum arquivo restaurado"); }
        verified.push((*file, bytes));
    }
    if !verified.iter().any(|(name, _)| name.ends_with(".efi")) { return Err("Nenhum carregador EFI válido no backup"); }
    drop(source);
    let proto = boot::open_protocol_exclusive::<SimpleFileSystem>(target).map_err(|_| "ESP de destino indisponível")?;
    let mut fs = FileSystem::new(proto);
    fs.create_dir_all(path(&dest)?).map_err(|_| "Não foi possível criar o diretório de boot")?;
    let mut restored = 0;
    for (name, bytes) in verified {
        let output = path(&format!("{dest}\\{name}"))?;
        let old = if fs.try_exists(&output).map_err(|_| "Erro ao consultar o destino")? {
            let data = read_limited(&mut fs, &output, MAX_FILE)?;
            if data == bytes { continue; }
            let previous = path(&format!("{dest}\\{name}.nexus-previous"))?;
            if !fs.try_exists(&previous).map_err(|_| "Erro ao consultar a cópia anterior")? {
                fs.write(previous, &data).map_err(|_| "Não foi possível preservar o arquivo anterior")?;
            }
            Some(data)
        } else { None };
        if fs.write(&output, &bytes).is_err() {
            if let Some(old) = old { let _ = fs.write(&output, old); }
            return Err("Erro ao gravar. A reparação pode estar parcial; confira os arquivos preservados");
        }
        restored += 1;
    }
    Ok(restored)
}
