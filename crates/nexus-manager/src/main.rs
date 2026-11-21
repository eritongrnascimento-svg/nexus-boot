#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod design;
mod installer;
#[cfg(target_os = "windows")] mod windows_inventory;
mod pages;
mod partition_editor;
mod branding;
mod storage;
mod wizard;
#[cfg(target_os = "windows")] mod windows_firmware;

use std::{sync::mpsc::{self, Receiver, TryRecvError}, time::SystemTime};
use eframe::egui::{self, RichText};
use design::*;
use storage::Disk;

#[derive(Clone, Copy, Default, PartialEq)]
enum Page {
    #[default]
    Dashboard, Systems, Disks, Planner, Uefi, Themes, Settings, Recovery,
}

impl Page {
    const ALL: [Self; 8] = [Self::Dashboard, Self::Systems, Self::Disks, Self::Planner,
        Self::Uefi, Self::Themes, Self::Settings, Self::Recovery];

    fn label(self) -> &'static str {
        match self {
            Self::Dashboard => "Visão geral", Self::Systems => "Sistemas operacionais",
            Self::Disks => "Discos e partições", Self::Planner => "Particionamento",
            Self::Uefi => "Gerenciador UEFI", Self::Themes => "Temas e aparência",
            Self::Settings => "Configurações", Self::Recovery => "Recuperação",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::Dashboard => "Inventário local de armazenamento e inicialização.",
            Self::Systems => "Organize os perfis que você quer usar no menu de inicialização.",
            Self::Disks => "Consulte os dispositivos e inspecione as partições detectadas.",
            Self::Planner => "Planeje uma estrutura EFI e compare a distribuição antes de aplicar.",
            Self::Uefi => "Consulte as entradas de inicialização disponíveis no firmware.",
            Self::Themes => "Personalize a aparência e confira a prévia do menu de boot.",
            Self::Settings => "Preferências locais do painel e configuração do menu de boot.",
            Self::Recovery => "Crie e restaure cópias das preferências do painel.",
        }
    }
}

fn main() -> eframe::Result {
    let initial = std::env::args().nth(1).and_then(|arg| {
        arg.strip_prefix("--page=")?.parse::<usize>().ok().and_then(|i| Page::ALL.get(i).copied())
    }).unwrap_or_default();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1180.0, 880.0])
            .with_min_inner_size([780.0, 560.0]).with_position([0.0, 0.0]).with_icon(branding::icon()).with_app_id("nexus-boot"),
        ..Default::default()
    };
    eframe::run_native("Nexus Boot", options, Box::new(move |creation| {
        setup(&creation.egui_ctx);
        let mut app = Panel { page: initial, ..Default::default() };
        app.refresh(&creation.egui_ctx);
        Ok(Box::new(app))
    }))
}

type Scan = Receiver<(Result<Vec<Disk>, String>, Result<String, String>)>;

#[derive(Default)]
struct Panel {
    page: Page,
    disks: Vec<Disk>,
    scan: Option<Scan>,
    error: Option<String>,
    checked: Option<SystemTime>,
    firmware: Option<Result<String, String>>,
    selected: usize,
    partition: Option<usize>,
    sections: pages::Sections,
    wizard: wizard::Wizard,
    #[cfg(target_os = "windows")] windows_firmware: windows_firmware::Firmware,
    show_installer: bool,
    partition_editor: partition_editor::Editor,
}

impl Panel {
    fn refresh(&mut self, ctx: &egui::Context) {
        let (sender, receiver) = mpsc::channel();
        self.scan = Some(receiver);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = sender.send((storage::inventory(), storage::firmware_entries()));
            ctx.request_repaint();
        });
    }

    fn poll(&mut self) {
        let Some(scan) = &self.scan else { return };
        match scan.try_recv() {
            Ok((disks, firmware)) => {
                self.error = disks.as_ref().err().cloned();
                self.disks = disks.unwrap_or_default();
                self.firmware = Some(firmware);
                self.checked = Some(SystemTime::now());
                self.selected = 0;
                self.partition = None;
            }
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => self.error = Some("Leitura interrompida.".into()),
        }
        self.scan = None;
    }

    fn disk_picker(&mut self, ui: &mut egui::Ui) {
        let old = self.selected;
        egui::ComboBox::from_id_salt("disk-picker")
            .selected_text(self.disks.get(old).map_or("Escolher disco", |d| &d.id))
            .show_ui(ui, |ui| for (i, disk) in self.disks.iter().enumerate() {
                ui.selectable_value(&mut self.selected, i, &disk.id);
            });
        if old != self.selected {
            self.partition = None;
        }
    }
}

impl eframe::App for Panel {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.poll();
        ctx.set_pixels_per_point(self.sections.preferences.scale);
        egui::SidePanel::left("navigation").exact_width(218.0).resizable(false)
            .frame(frame(SIDEBAR, 18)).show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let (badge, _) = ui.allocate_exact_size(egui::vec2(28.0, 28.0), egui::Sense::hover());
                    ui.painter().rect_filled(badge, 8, egui::Color32::from_rgb(29, 72, 76));
                    draw_icon(ui, badge.center(), 4, ACCENT);
                    ui.label(RichText::new("NEXUS BOOT").size(20.0).strong().color(TEXT));
                });
                hint(ui, "Centro de administração");
                ui.add_space(20.0);
                for (index, page) in Page::ALL.into_iter().enumerate() {
                    if navigation_row(ui, page.label(), index, self.page == page).clicked() {
                        self.page = page;
                    }
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    hint(ui, format!("v{}   ·   {}", env!("CARGO_PKG_VERSION"), std::env::consts::OS));
                });
            });
        egui::CentralPanel::default().frame(frame(BG, 28)).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(self.page.label()).size(30.0).strong().color(TEXT));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add_enabled(self.scan.is_none(), egui::Button::new("↻  Verificar")).clicked() {
                        self.refresh(ctx);
                    }
                    if self.scan.is_some() { ui.spinner(); }
                });
            });
            hint(ui, self.page.description());
            ui.add_space(12.0);

            if let Some(error) = &self.error {
                card(ui, |ui| { title(ui, "Falha no inventário"); hint(ui, error); });
            }

            egui::ScrollArea::vertical().id_salt(self.page.label()).show(ui, |ui| {
                match self.page {
                    Page::Dashboard => pages::dashboard(ui, self),
                    Page::Systems => self.sections.systems(ui),
                    Page::Disks => {
                        self.disk_picker(ui);
                        if let Some(disk) = self.disks.get(self.selected) {
                            pages::disk(ui, disk, &mut self.partition);
                            if let Some(part) = self.partition.and_then(|index| disk.partitions.get(index)) {
                                if partition_editor::Editor::available(disk, part) {
                                    if ui.button("Editar ou redimensionar partição").clicked() {
                                        self.partition_editor.select(disk, part);
                                    }
                                } else {
                                    hint(ui, "Edição disponível em GPT no Linux e em volumes NTFS de dados no Windows.");
                                }
                            }
                        } else { hint(ui, "Nenhum disco disponível."); }
                    }
                    Page::Planner => {
                        self.disk_picker(ui);
                        self.sections.plan(ui, self.disks.get(self.selected));
                        ui.add_space(8.0);
                        if cfg!(target_os = "linux") && ui.button("Abrir instalador Linux").clicked() {
                            self.show_installer = true;
                        }
                    }
                    Page::Uefi => {
                        #[cfg(target_os = "windows")]
                        self.windows_firmware.show(ui);
                        pages::uefi(ui, self.firmware.as_ref());
                    },
                    Page::Themes => self.sections.themes(ui),
                    Page::Settings => self.sections.settings(ui),
                    Page::Recovery => {
                        self.sections.recovery(ui);
                        self.wizard.recover(ui, &self.disks);
                    },
                }
            });
        });

        if self.partition_editor.show(ctx) { self.refresh(ctx); }

        if self.show_installer {
            egui::Window::new("Instalação do Nexus Boot").collapsible(false)
                .default_width(520.0).open(&mut self.show_installer).show(ctx, |ui| {
                    let disk = self.disks.get(self.selected);
                    let plan = disk.and_then(|disk| self.sections.install_plan(disk));
                    self.wizard.show(ui, disk, plan.as_ref());
                });
        }
    }
}
