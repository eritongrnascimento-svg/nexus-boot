#![no_main]
#![no_std]

extern crate alloc;

mod picker;
mod recovery;

use alloc::{format, string::{String, ToString}, vec::Vec};
use uefi::{
    CString16, Handle, Status, boot::{self, LoadImageSource}, cstr16, entry,
    fs::{FileSystem, PathBuf}, println,
    proto::{
        BootPolicy,
        console::text::{Key, ScanCode},
        device_path::{DevicePath, build::{DevicePathBuilder, media::FilePath}},
        media::fs::SimpleFileSystem,
    },
    system,
};

struct Entry {
    name: String,
    volume: Handle,
    path: CString16,
}

#[entry]
fn main() -> Status {
    if let Err(error) = uefi::helpers::init() { return error.status(); }
    let _ = boot::set_watchdog_timer(0, 0x10000, None);
    let mut entries = discover();
    let mut backups = Vec::new();
    let mut repairing = false;
    let mut confirm = false;
    let mut index = 0;
    let mut notice = None;
    loop {
        let mut labels: Vec<&str> = if repairing {
            backups.iter().map(|item: &recovery::Backup| item.name.as_str()).collect()
        } else {
            entries.iter().map(|entry| entry.name.as_str()).collect()
        };
        labels.push(if repairing { "Voltar ao menu" } else { "Reparação de boot" });
        let heading = if confirm { "Confirmar restauração · Enter para aplicar, Esc para cancelar" }
        else if repairing { "Escolha o sistema para reparar" }
        else { "Escolha o sistema que deseja iniciar" };
        let count = labels.len();
        let displayed = if confirm { &labels[index..=index] } else { labels.as_slice() };
        if !picker::draw(displayed, if confirm { 0 } else { index }, heading, repairing, notice.as_deref()) {
            system::with_stdout(|out| { let _ = out.clear(); });
            println!("NEXUS BOOT | {heading}");
            for (item, name) in displayed.iter().enumerate() {
                println!("{} {}", if item == index || confirm { ">" } else { " " }, name);
            }
            if let Some(ref text) = notice { println!("{text}"); }
            println!("Setas: navegar | Enter: selecionar | Esc: voltar");
        }
        let Some(key) = read_key() else { return Status::DEVICE_ERROR };
        match key {
            Key::Special(ScanCode::ESCAPE) if confirm => { confirm = false; }
            Key::Special(ScanCode::ESCAPE) if repairing => {
                repairing = false; index = 0; notice = None;
            }
            Key::Special(ScanCode::ESCAPE) => return Status::SUCCESS,
            Key::Special(ScanCode::LEFT | ScanCode::UP) if !confirm => {
                index = (index + count - 1) % count;
            }
            Key::Special(ScanCode::RIGHT | ScanCode::DOWN) if !confirm => {
                index = (index + 1) % count;
            }
            Key::Printable(key) if u16::from(key) == 13 => {
                if confirm {
                    confirm = false;
                    notice = Some(match recovery::restore(&backups[index]) {
                        Ok(0) => "Arquivos já conferem com o backup.".into(),
                        Ok(count) => format!("{count} arquivo(s) restaurado(s). Esc volta ao menu."),
                        Err(error) => error.into(),
                    });
                    entries = discover();
                } else if repairing && index < backups.len() {
                    confirm = true;
                    notice = Some("Os arquivos existentes serão preservados ao lado dos restaurados.".into());
                } else if repairing {
                    repairing = false; index = 0; notice = None;
                } else if let Some(selected) = entries.get(index) {
                    let status = launch(selected);
                    notice = Some(format!("{} voltou ao Nexus ({status:?}).", selected.name));
                } else {
                    repairing = true;
                    backups = recovery::targets();
                    index = 0;
                    notice = backups.is_empty().then(|| "Nenhuma cópia disponível. Gere um backup no painel Linux.".into());
                }
            }
            _ => {}
        }
    }
}

fn read_key() -> Option<Key> {
    loop {
        let mut event = [system::with_stdin(|input| input.wait_for_key_event()).ok()?];
        boot::wait_for_event(&mut event).ok()?;
        if let Some(key) = system::with_stdin(|input| input.read_key()).ok()? { return Some(key); }
    }
}

fn discover() -> Vec<Entry> {
    let mut entries = Vec::new();
    for volume in boot::find_handles::<SimpleFileSystem>().unwrap_or_default() {
        let Ok(protocol) = boot::open_protocol_exclusive::<SimpleFileSystem>(volume) else { continue };
        let mut fs = FileSystem::new(protocol);
        let efi = PathBuf::from(cstr16!("\\EFI"));
        let Ok(dirs) = fs.read_dir(&efi) else { continue };
        let vendors: Vec<_> = dirs.filter_map(Result::ok).filter(|item| item.is_directory())
            .map(|item| item.file_name().to_string()).take(128).collect();
        for vendor in vendors {
            if entries.len() == 64 { return entries; }
            let (name, files): (&str, &[&str]) = match vendor.to_ascii_lowercase().as_str() {
                "." | ".." | "boot" | "nexusboot" => continue,
                "microsoft" => ("Windows", &["Boot\\bootmgfw.efi"]),
                "cachyos" => ("CachyOS", &["shimx64.efi", "grubx64.efi"]),
                "systemd" => ("systemd-boot", &["systemd-bootx64.efi"]),
                "ubuntu" => ("Ubuntu", &["shimx64.efi", "grubx64.efi"]),
                "fedora" => ("Fedora", &["shimx64.efi", "grubx64.efi"]),
                "debian" => ("Debian", &["shimx64.efi", "grubx64.efi"]),
                _ => (&vendor, &["shimx64.efi", "grubx64.efi", "liminex64.efi"]),
            };
            for file in files {
                let Ok(path) = CString16::try_from(format!("\\EFI\\{vendor}\\{file}").as_str()) else { continue };
                if fs.metadata(PathBuf::from(path.clone())).is_ok_and(|info| !info.is_directory()) {
                    entries.push(Entry { name: name.to_string(), volume, path });
                    break;
                }
            }
        }
    }
    entries
}

fn launch(entry: &Entry) -> Status {
    let result = (|| {
        let device = boot::open_protocol_exclusive::<DevicePath>(entry.volume).map_err(|error| error.status())?;
        let bytes = device.as_bytes();
        let mut file_node = Vec::new();
        let file = DevicePathBuilder::with_vec(&mut file_node)
            .push(&FilePath { path_name: &entry.path }).and_then(DevicePathBuilder::finalize)
            .map_err(|_| Status::INVALID_PARAMETER)?;
        let mut path = bytes[..bytes.len() - 4].to_vec();
        path.extend_from_slice(file.as_bytes());
        let device_path = <&DevicePath>::try_from(path.as_slice()).map_err(|_| Status::INVALID_PARAMETER)?;
        let handle = boot::load_image(boot::image_handle(), LoadImageSource::FromDevicePath {
            device_path, boot_policy: BootPolicy::ExactMatch,
        }).map_err(|error| error.status())?;
        drop(device);
        boot::start_image(handle).map_err(|error| error.status())
    })();
    result.err().unwrap_or(Status::SUCCESS)
}
