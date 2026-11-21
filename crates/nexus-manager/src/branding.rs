use eframe::egui;

pub fn icon() -> egui::IconData {
    let mut rgba = Vec::with_capacity(64 * 64 * 4);
    for y in 0i32..64 {
        for x in 0i32..64 {
            let dx = (x * 2 - 63).abs();
            let dy = (y * 2 - 63).abs();
            let corners = (dx - 43).max(0).pow(2) + (dy - 43).max(0).pow(2);
            let pixel = if corners > 18 * 18 {
                [0, 0, 0, 0]
            } else if (32..=41).contains(&(dx + dy)) || dx * dx + dy * dy <= 30 {
                [91, 231, 207, 255]
            } else {
                [17, 44, 53, 255]
            };
            rgba.extend(pixel);
        }
    }
    egui::IconData { rgba, width: 64, height: 64 }
}

#[cfg(target_os = "linux")]
pub fn install_menu_entry() -> Result<(), String> {
    use std::{fs, io, os::unix::fs::PermissionsExt, path::PathBuf};

    fn install() -> io::Result<()> {
        let data = std::env::var_os("XDG_DATA_HOME").filter(|path| !path.is_empty()).map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
            .ok_or_else(|| io::Error::other("Diretório local do usuário indisponível."))?;
        let executables = data.join("nexus-boot");
        let executable = executables.join("NexusBoot.run");
        let path = executable.to_str().ok_or_else(|| io::Error::other("Caminho incompatível com atalhos."))?;
        if !executable.is_absolute() || path.chars().any(|c| matches!(c, '\\' | '"' | '$' | '`' | '%') || c.is_control()) {
            return Err(io::Error::other("O caminho contém caracteres incompatíveis com o atalho."));
        }
        let applications = data.join("applications");
        let icons = data.join("icons/hicolor/64x64/apps");
        fs::create_dir_all(&executables)?;
        fs::create_dir_all(&applications)?;
        fs::create_dir_all(&icons)?;
        let running = std::env::current_exe()?;
        if running != executable {
            let staging = executables.join(format!("NexusBoot-{}.tmp", std::process::id()));
            fs::copy(&running, &staging)?;
            fs::set_permissions(&staging, fs::Permissions::from_mode(0o755))?;
            fs::rename(staging, &executable)?;
        }
        fs::write(icons.join("nexus-boot.png"), include_bytes!("nexus-boot.png"))?;
        fs::write(applications.join("nexus-boot.desktop"), format!(
            "[Desktop Entry]\nType=Application\nName=Nexus Boot\nExec=\"{path}\"\nIcon=nexus-boot\nStartupWMClass=nexus-boot\nTerminal=false\nCategories=System;Settings;\n"
        ))
    }

    install().map_err(|error| error.to_string())
}

#[cfg(target_os = "windows")]
pub fn install_menu_entry() -> Result<(), String> {
    use std::{os::windows::process::CommandExt, process::Command};
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let script = r#"
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
try {
    $programs = [Environment]::GetFolderPath('Programs')
    if (!$programs) { throw 'Menu Iniciar do usuário indisponível.' }
    $shell = New-Object -ComObject WScript.Shell
    $link = $shell.CreateShortcut((Join-Path $programs 'Nexus Boot.lnk'))
    $link.TargetPath = $env:NEXUS_SHORTCUT_EXE
    $link.WorkingDirectory = Split-Path -LiteralPath $env:NEXUS_SHORTCUT_EXE
    $link.IconLocation = $env:NEXUS_SHORTCUT_EXE + ',0'
    $link.Description = 'Nexus Boot'
    $link.Save()
} catch {
    [Console]::Error.WriteLine($_.Exception.Message)
    exit 1
}
"#;
    let output = Command::new("powershell.exe").creation_flags(0x08000000)
        .env("NEXUS_SHORTCUT_EXE", executable)
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output().map_err(|error| error.to_string())?;
    if output.status.success() { Ok(()) }
    else { Err(String::from_utf8_lossy(&output.stderr).trim().into()) }
}
