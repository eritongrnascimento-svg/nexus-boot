use std::process::Command;

#[cfg(target_os = "linux")]
use std::{io::Write, process::Stdio};

#[cfg(target_os = "linux")]
const EFI: &[u8] = include_bytes!("../payload/EFI/NexusBoot/NexusBoot.efi");
#[cfg(target_os = "linux")]
const CONFIG: &[u8] = include_bytes!("../payload/EFI/NexusBoot/nexus.toml");

#[cfg(target_os = "linux")]
pub fn run(action: &str, disk: &str, uuid: &str, plan: Option<&crate::wizard::Plan>) -> Result<String, String> {
    let plan_args = plan.map(|plan| [
        plan.start.to_string(),
        plan.size.to_string(),
        plan.label.clone(),
        plan.table_uuid.clone().unwrap_or_default(),
    ]);
    let mut child = Command::new("pkexec")
        .args([
            "/usr/bin/bash", "-c", include_str!("installer-linux.sh"), "nexus-installer",
            action, disk, uuid, &EFI.len().to_string(),
        ])
        .args(plan_args.iter().flatten())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("Não foi possível solicitar acesso administrativo: {error}"))?;
    let input_result = if matches!(action, "install" | "upgrade" | "primary" | "inspect") {
        let mut input = child.stdin.take().expect("stdin configurado");
        input.write_all(EFI).and_then(|_| if action == "install" { input.write_all(CONFIG) } else { Ok(()) })
    } else {
        drop(child.stdin.take());
        Ok(())
    };
    let output = child.wait_with_output().map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(format!("Operação interrompida ({}). {}", output.status, String::from_utf8_lossy(&output.stderr).trim()));
    }
    if action != "inspect" { input_result.map_err(|error| error.to_string())?; }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

#[cfg(target_os = "windows")]
pub fn run(_action: &str, _disk: &str, _uuid: &str, _plan: Option<&crate::wizard::Plan>) -> Result<String, String> {
    Err("A instalação guiada está disponível no Linux nesta versão.".into())
}

pub fn reboot_to_firmware() -> Result<String, String> {
    #[cfg(target_os = "linux")]
    let output = Command::new("systemctl").args(["reboot", "--firmware-setup"]).output();
    #[cfg(target_os = "windows")]
    let output = Command::new("shutdown.exe").args(["/r", "/fw", "/t", "0"]).output();

    let output = output.map_err(|error| error.to_string())?;
    if output.status.success() {
        Ok(String::new())
    } else {
        Err(format!("O sistema recusou a reinicialização para o firmware ({}). {}", output.status, String::from_utf8_lossy(&output.stderr).trim()))
    }
}
