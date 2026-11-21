use std::{os::windows::process::CommandExt, process::Command, sync::mpsc::{self, Receiver, TryRecvError}};

use eframe::egui;
use serde::Deserialize;

#[derive(Clone, Deserialize)]
struct Entry {
    id: String,
    name: String,
}

#[derive(Deserialize)]
struct Report {
    entries: Vec<Entry>,
    backup: Option<String>,
}

#[derive(Default)]
pub struct Firmware {
    current: Vec<Entry>,
    proposed: Vec<Entry>,
    work: Option<Receiver<Result<Report, String>>>,
    message: Option<String>,
    confirm: bool,
}

impl Firmware {
    pub fn show(&mut self, ui: &mut egui::Ui) {
        if let Some(work) = &self.work {
            match work.try_recv() {
                Ok(result) => {
                    self.work = None;
                    match result {
                        Ok(report) => {
                            self.current = report.entries;
                            self.proposed.clone_from(&self.current);
                            self.message = report.backup.map(|path| format!("Ordem confirmada. Backup em {path}."));
                        }
                        Err(error) => self.message = Some(error),
                    }
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    self.work = None;
                    self.message = Some("Operação interrompida. Consulte o firmware novamente.".into());
                }
            }
        }
        crate::design::card(ui, |ui| {
            crate::design::title(ui, "Ordem do firmware UEFI");
            if let Some(message) = &self.message { ui.label(message); }
            if self.work.is_some() { ui.spinner(); return; }
            if ui.button("Consultar ordem UEFI").clicked() { self.start(ui.ctx(), false); }
            let mut change = None;
            let count = self.proposed.len();
            for (i, entry) in self.proposed.iter().enumerate() {
                ui.horizontal_wrapped(|ui| {
                    ui.strong(format!("{}. {}", i + 1, entry.name));
                    ui.push_id(i, |ui| {
                        if ui.add_enabled(i > 0, egui::Button::new("Subir")).clicked() { change = Some((i, i - 1)); }
                        if ui.add_enabled(i + 1 < count, egui::Button::new("Descer")).clicked() { change = Some((i, i + 1)); }
                    });
                });
            }
            if let Some((a, b)) = change { self.proposed.swap(a, b); }
            let changed = !self.current.iter().map(|entry| &entry.id).eq(self.proposed.iter().map(|entry| &entry.id));
            if ui.add_enabled(changed, egui::Button::new("Revisar nova ordem")).clicked() { self.confirm = true; }
        });
        if self.confirm {
            egui::Modal::new(egui::Id::new("windows-order-confirm")).show(ui.ctx(), |ui| {
                ui.heading("Gravar ordem UEFI");
                for entry in &self.proposed { ui.label(&entry.name); }
                ui.label("A ordem anterior será guardada antes da alteração.");
                if ui.button("Confirmar e aplicar").clicked() { self.confirm = false; self.start(ui.ctx(), true); }
                if ui.button("Cancelar").clicked() { self.confirm = false; }
            });
        }
    }

    fn start(&mut self, ctx: &egui::Context, apply: bool) {
        let (sender, receiver) = mpsc::channel();
        let ctx = ctx.clone();
        let serialize = |entries: &[Entry]| entries.iter().map(|entry| entry.id.as_str()).collect::<Vec<_>>().join(",");
        let before = if apply { serialize(&self.current) } else { String::new() };
        let after = if apply { serialize(&self.proposed) } else { String::new() };
        self.work = Some(receiver);
        self.message = None;
        std::thread::spawn(move || {
            let _ = sender.send(run(&before, &after));
            ctx.request_repaint();
        });
    }
}

fn run(before: &str, after: &str) -> Result<Report, String> {
    for ids in [before, after] {
        if !ids.bytes().all(|b| b.is_ascii_hexdigit() || b"{},-".contains(&b)) {
            return Err("Identificador UEFI inválido.".into());
        }
    }
    let script = format!("& {{ {} }} '{before}' '{after}'", include_str!("windows-firmware.ps1"));
    let output = Command::new("powershell.exe").creation_flags(0x08000000)
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output().map_err(|error| error.to_string())?;
    if output.status.success() {
        serde_json::from_slice(&output.stdout).map_err(|error| format!("Relatório inválido: {error}"))
    } else { Err(String::from_utf8_lossy(&output.stderr).trim().into()) }
}
