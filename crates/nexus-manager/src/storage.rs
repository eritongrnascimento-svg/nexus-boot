use std::process::Command;

use serde::Deserialize;

const ESP_TYPE: &str = "c12a7328-f81f-11d2-ba4b-00a0c93ec93b";

#[derive(Deserialize)]
pub struct Disk {
    pub id: String,
    pub size: u64,
    pub partitions: Vec<Partition>,
    pub model: Option<String>,
    pub scheme: Option<String>,
    pub transport: Option<String>,
    pub table_uuid: Option<String>,
}

#[derive(Deserialize)]
pub struct Partition {
    pub id: String,
    pub size: u64,
    pub part_type: Option<String>,
    pub filesystem: Option<String>,
    pub available: Option<u64>,
    pub used: Option<u64>,
    pub mount: Option<String>,
    pub offset: Option<u64>,
    pub uuid: Option<String>,
    pub label: Option<String>,
    pub volume_label: Option<String>,
}

impl Partition {
    pub fn is_esp(&self) -> bool {
        self.part_type
            .as_deref()
            .is_some_and(|kind| kind.trim_matches(['{', '}']).eq_ignore_ascii_case(ESP_TYPE))
    }
}

fn read_json<T: for<'de> Deserialize<'de>>(command: &mut Command) -> Result<T, String> {
    let output = command.output().map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }
    serde_json::from_slice(&output.stdout).map_err(|error| format!("Inventário inválido: {error}"))
}

#[cfg(target_os = "linux")]
pub fn inventory() -> Result<Vec<Disk>, String> {
    #[derive(Deserialize)]
    struct Devices {
        blockdevices: Vec<Device>,
    }

    #[derive(Deserialize)]
    struct Device {
        path: String,
        size: u64,
        #[serde(rename = "type")]
        kind: String,
        parttype: Option<String>,
        model: Option<String>, pttype: Option<String>, tran: Option<String>, ptuuid: Option<String>,
        fstype: Option<String>, fsavail: Option<u64>, fsused: Option<u64>,
        mountpoint: Option<String>, start: Option<u64>, partuuid: Option<String>, partlabel: Option<String>, label: Option<String>,
        #[serde(default)]
        children: Vec<Device>,
    }

    let devices: Devices = read_json(Command::new("lsblk").args([
        "--json", "--bytes", "--tree", "--output", "PATH,SIZE,TYPE,PARTTYPE,MODEL,PTTYPE,TRAN,FSTYPE,FSAVAIL,FSUSED,MOUNTPOINT,START,PARTUUID,PARTLABEL,PTUUID,LABEL",
    ]))?;
    Ok(devices.blockdevices.into_iter().filter(|device| device.kind == "disk").map(|disk| Disk {
        id: disk.path,
        model: disk.model, scheme: disk.pttype, transport: disk.tran, table_uuid: disk.ptuuid,
        size: disk.size,
        partitions: disk.children.into_iter().filter(|child| child.kind == "part").map(|part| Partition {
            id: part.path,
            size: part.size,
            part_type: part.parttype,
            filesystem: part.fstype, available: part.fsavail, used: part.fsused, mount: part.mountpoint, offset: part.start.and_then(|s| s.checked_mul(512)), uuid: part.partuuid, label: part.partlabel, volume_label: part.label,
        }).collect(),
    }).collect())
}

#[cfg(target_os = "windows")]
pub fn inventory() -> Result<Vec<Disk>, String> {
    use std::os::windows::process::CommandExt;

    let script = r#"
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
$disks = @(Get-Disk | ForEach-Object {
    $disk = $_
    $parts = if ($disk.NumberOfPartitions -gt 0) {
        Get-Partition -DiskNumber $disk.Number | ForEach-Object {
            $part = $_
            $volume = Get-Volume -Partition $part -ErrorAction SilentlyContinue
            $used = if ($null -ne $volume) { $volume.Size - $volume.SizeRemaining }
            @{id = "Partição $($part.PartitionNumber)"; size = $part.Size; part_type = $part.GptType; offset = $part.Offset; uuid = [string]$part.Guid; volume_label = $volume.FileSystemLabel; filesystem = $volume.FileSystem; available = $volume.SizeRemaining; used = $used; mount = if ($part.DriveLetter -match '^[A-Z]$') { "$($part.DriveLetter):\" } }
        }
    }
    @{id = "Disco $($disk.Number) · $($disk.FriendlyName)"; model = $disk.FriendlyName; scheme = [string]$disk.PartitionStyle; transport = [string]$disk.BusType; size = $disk.Size; partitions = @($parts | Where-Object { $null -ne $_ })}
})
ConvertTo-Json -InputObject $disks -Depth 5 -Compress
"#;
    let cim = read_json(Command::new("powershell.exe")
        .creation_flags(0x08000000)
        .args(["-NoProfile", "-NonInteractive", "-Command", script]));
    cim.or_else(|error| crate::windows_inventory::inventory().map_err(|fallback| format!("{error}
{fallback}")))
}

pub fn firmware_entries() -> Result<String, String> {
    #[cfg(target_os = "linux")]
    {
        if !std::path::Path::new("/sys/firmware/efi").is_dir() {
            return Err("O ambiente atual não expõe firmware UEFI.".into());
        }
    }
    #[cfg(target_os = "windows")]
    use std::os::windows::process::CommandExt;
    #[cfg(target_os = "windows")]
    let output = Command::new("bcdedit.exe").creation_flags(0x08000000).args(["/enum", "firmware", "/v"]).output();
    #[cfg(target_os = "linux")]
    let output = Command::new("efibootmgr").arg("-v").output();
    let output = output.map_err(|error| format!("Consulta ao firmware indisponível ({error})."))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(format!("O firmware recusou a consulta ({}). {}", output.status,
            String::from_utf8_lossy(&output.stderr).trim()))
    }
}