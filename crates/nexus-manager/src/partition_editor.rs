use std::sync::mpsc::{self, Receiver, TryRecvError};

use eframe::egui;

use crate::{design::{bytes, hint}, storage::{Disk, Partition}};

#[derive(Clone, Copy)]
enum Operation {
    Rename,
    Resize,
}

struct Target {
    disk: String,
    part: String,
    uuid: String,
    size: u64,
    filesystem: Option<String>,
    mounted: bool,
}

#[derive(Default)]
pub struct Editor {
    target: Option<Target>,
    proposed: u64,
    label: String,
    operation: Option<Operation>,
    pending: Option<Receiver<Result<String, String>>>,
    result: Option<Result<String, String>>,
}

impl Editor {
    pub fn select(&mut self, disk: &Disk, part: &Partition) {
        if self.pending.is_some() { return; }
        let Some(uuid) = &part.uuid else { return };
        self.target = Some(Target {
            disk: disk.id.clone(), part: part.id.clone(), uuid: uuid.clone(),
            size: part.size, filesystem: part.filesystem.clone(), mounted: part.mount.is_some(),
        });
        self.proposed = part.size.div_ceil(1 << 20).max(64);
        self.label = if cfg!(target_os = "windows") { part.volume_label.clone() } else { part.label.clone() }.unwrap_or_default();
        self.result = None;
    }

    pub fn available(disk: &Disk, part: &Partition) -> bool {
        (cfg!(target_os = "linux") || part.filesystem.as_deref() == Some("NTFS")) && disk.scheme.as_deref().is_some_and(|s| s.eq_ignore_ascii_case("gpt"))
            && part.uuid.as_ref().is_some_and(|uuid| !uuid.is_empty()) && (!cfg!(target_os = "windows") || !part.is_esp())
    }

    pub fn show(&mut self, ctx: &egui::Context) -> bool {
        let mut refresh = false;
        if let Some(pending) = &self.pending {
            match pending.try_recv() {
                Ok(result) => { self.result = Some(result); self.pending = None; refresh = true; }
                Err(TryRecvError::Disconnected) => {
                    self.result = Some(Err("A operação foi interrompida. Verifique o estado da partição antes de repetir.".into()));
                    self.pending = None;
                    refresh = true;
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        let Some(target) = &self.target else { return refresh };
        let mut open = true;
        let mut execute = None;
        egui::Window::new("Editar partição existente").open(&mut open).collapsible(false)
            .default_width(540.0).show(ctx, |ui| {
                ui.strong(format!("{}   ·   {}", target.part, target.disk));
                hint(ui, format!("Tamanho atual    {}", bytes(target.size)));
                if self.pending.is_some() {
                    ui.spinner(); hint(ui, "Operação em andamento. Mantenha o aplicativo aberto.");
                    return;
                }
                if let Some(result) = &self.result {
                    match result {
                        Ok(message) => { ui.label(message); }
                        Err(message) => { ui.colored_label(crate::design::VIOLET, message); }
                    }
                    hint(ui, "Feche esta janela e selecione a partição novamente para outra operação.");
                    return;
                }
                ui.separator();
                if cfg!(target_os = "linux") && target.mounted {
                    hint(ui, "A partição está montada. Para editar a partição do sistema, inicie por um Linux live.");
                    return;
                }
                ui.label(if cfg!(target_os = "windows") { "Rótulo do volume NTFS" } else { "Nome da partição GPT" });
                ui.text_edit_singleline(&mut self.label);
                let valid_label = (1..=if cfg!(target_os = "windows") { 32 } else { 36 }).contains(&self.label.trim().len())
                    && self.label.bytes().all(|b| b.is_ascii_alphanumeric() || b"_. -".contains(&b));
                if ui.add_enabled(valid_label, egui::Button::new("Revisar alteração de nome")).clicked() {
                    self.operation = Some(Operation::Rename);
                }
                ui.separator();
                if target.filesystem.as_deref() == Some(if cfg!(target_os = "windows") { "NTFS" } else { "ext4" }) {
                    ui.label("Novo tamanho da partição e do sistema de arquivos (MiB)");
                    ui.add(egui::DragValue::new(&mut self.proposed).range(64..=999_999_999).suffix(" MiB"));
                    hint(ui, "O início da partição será preservado. A expansão exige espaço livre contíguo depois dela.");
                    if ui.add_enabled(self.proposed.saturating_mul(1 << 20) != target.size, egui::Button::new("Revisar redimensionamento")).clicked() {
                        self.operation = Some(Operation::Resize);
                    }
                } else {
                    hint(ui, "Redimensionamento disponível para ext4 desmontado. Outros formatos ainda não são suportados.");
                }
            });
        if !open && self.pending.is_none() {
            self.target = None;
            self.operation = None;
            return refresh;
        }
        if let Some(operation) = self.operation {
            egui::Modal::new(egui::Id::new("confirm-partition-edit")).show(ctx, |ui| {
                ui.heading("Confirmar gravação no disco");
                ui.label(match operation {
                    Operation::Rename => format!("Alterar o rótulo de {} para «{}».", target.part, self.label.trim()),
                    Operation::Resize => format!("Alterar {} de {} para {}.", target.part,
                        bytes(target.size), bytes(self.proposed * (1 << 20))),
                });
                hint(ui, if cfg!(target_os = "windows") { "Faça backup dos arquivos importantes antes de continuar." } else { "Faça backup dos arquivos importantes antes de continuar. O Nexus guarda uma cópia da tabela GPT." });
                ui.horizontal(|ui| {
                    if ui.button("Cancelar").clicked() { self.operation = None; }
                    if ui.button("Confirmar e aplicar").clicked() { execute = Some(operation); }
                });
            });
        }
        if let Some(operation) = execute {
            let (action, value) = match operation {
                Operation::Rename => ("rename", self.label.trim().to_owned()),
                Operation::Resize => ("resize", self.proposed.to_string()),
            };
            let args = [action.to_owned(), target.disk.clone(), target.part.clone(),
                target.uuid.clone(), target.size.to_string(), value];
            let ctx = ctx.clone();
            let (sender, receiver) = mpsc::channel();
            self.pending = Some(receiver);
            self.operation = None;
            std::thread::spawn(move || {
                let _ = sender.send(apply(args));
                ctx.request_repaint();
            });
        }
        refresh
    }
}

#[cfg(target_os = "linux")]
fn apply(args: [String; 6]) -> Result<String, String> {
    let output = std::process::Command::new("pkexec")
        .args(["/usr/bin/bash", "-c", include_str!("partition-linux.sh"), "nexus-partition"])
        .args(args).output().map_err(|error| format!("Não foi possível solicitar acesso administrativo: {error}"))?;
    if output.status.success() { Ok(String::from_utf8_lossy(&output.stdout).trim().into()) }
    else { Err(format!("Alteração interrompida ({}). {}", output.status, String::from_utf8_lossy(&output.stderr).trim())) }
}

#[cfg(target_os = "windows")]
fn apply([action, _, _, uuid, size, value]: [String; 6]) -> Result<String, String> {
    use std::os::windows::process::CommandExt;

    // Values appended to the script are constrained before crossing the PowerShell boundary.
    let id = uuid.trim_matches(['{', '}']);
    if id.len() != 36 || !id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')
        || !matches!(action.as_str(), "rename" | "resize")
        || !size.bytes().all(|b| b.is_ascii_digit())
        || !value.bytes().all(|b| b.is_ascii_alphanumeric() || b"_. -".contains(&b)) {
        return Err("Dados da partição inválidos.".into());
    }
    let script = format!("& {{ {} }} '{}' '{}' {} '{}'", include_str!("windows-storage.ps1"), action, id, size, value);
    let output = std::process::Command::new("powershell.exe").creation_flags(0x08000000)
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output().map_err(|error| error.to_string())?;
    if output.status.success() { Ok(String::from_utf8_lossy(&output.stdout).trim().into()) }
    else { Err(String::from_utf8_lossy(&output.stderr).trim().into()) }
}
