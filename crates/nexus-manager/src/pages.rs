use std::{fs, path::PathBuf};

use eframe::egui::{self, Color32, RichText, Ui};
use serde::{Deserialize, Serialize};

use crate::{Panel, design::*, storage::{Disk, Partition}};

const MIB: u64 = 1024 * 1024;

pub fn dashboard(ui: &mut Ui, app: &Panel) {
    let parts = app.disks.iter().flat_map(|disk| &disk.partitions);
    let partition_count = parts.clone().count();
    let esp_count = parts.filter(|part| part.is_esp()).count();
    let value = |count: usize| {
        if app.error.is_some() || app.checked.is_none() { "—".into() } else { count.to_string() }
    };
    ui.columns(4, |columns| {
        for (column, (label, total)) in columns.iter_mut().zip([
            ("Discos", value(app.disks.len())), ("Partições", value(partition_count)),
            ("Partições EFI", value(esp_count)), ("Perfis cadastrados", app.sections.preferences.systems.len().to_string()),
        ]) {
            card(column, |ui| {
                hint(ui, label);
                ui.label(RichText::new(total).size(30.0).color(ACCENT).strong());
            });
        }
    });
    card(ui, |ui| {
        title(ui, "Armazenamento");
        if app.disks.is_empty() { hint(ui, "Nenhum disco listado."); }
        for disk in &app.disks {
            ui.horizontal(|ui| {
                ui.strong(disk.model.as_deref().filter(|name| !name.trim().is_empty()).unwrap_or(&disk.id));
                hint(ui, bytes(disk.size));
            });
            map(ui, disk, None);
        }
    });
    card(ui, |ui| {
        title(ui, "Estado de inicialização");
        let default = app.sections.preferences.systems.iter().find(|os| os.default);
        hint(ui, format!("Perfil padrão local    {}", default.map_or("Nenhum definido", |os| &os.name)));
        hint(ui, match &app.firmware {
            Some(Ok(_)) => "Firmware    Entradas consultadas",
            Some(Err(_)) => "Firmware    Consulta indisponível",
            None => "Firmware    Aguardando verificação",
        });
        hint(ui, "Proteção de boot    Recurso pendente");
        if let Some(age) = app.checked.and_then(|time| time.elapsed().ok()) {
            hint(ui, format!("Última verificação    há {} min", age.as_secs() / 60));
        }
        if let Some(Err(error)) = &app.firmware {
            ui.colored_label(VIOLET, error);
        }
    });
}

fn map(ui: &mut Ui, disk: &Disk, selected: Option<usize>) -> Option<usize> {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 24.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 5, LINE);
    if disk.size == 0 { return None; }
    let mut next = 0;
    let mut clicked = None;
    for (i, part) in disk.partitions.iter().enumerate() {
        let start = part.offset.unwrap_or(next);
        let end = start.saturating_add(part.size);
        next = end;
        let position = |offset: u64| rect.left() + rect.width() * (offset.min(disk.size) as f64 / disk.size as f64) as f32;
        let segment = egui::Rect::from_min_max(egui::pos2(position(start), rect.top()), egui::pos2(position(end), rect.bottom()));
        if segment.width() <= 0.0 { continue; }
        let color = if selected == Some(i) { TEXT } else if part.is_esp() { ACCENT } else { VIOLET };
        ui.painter().rect_filled(segment, 3, color);
        let response = ui.interact(segment, ui.id().with((&disk.id, "segment", i)), egui::Sense::click())
            .on_hover_text(format!("{} · {}", part.id, bytes(part.size)));
        if response.clicked() { clicked = Some(i); }
    }
    clicked
}

pub fn disk(ui: &mut Ui, disk: &Disk, selected: &mut Option<usize>) {
    card(ui, |ui| {
        title(ui, disk.model.as_deref().filter(|model| !model.trim().is_empty()).unwrap_or(&disk.id));
        hint(ui, format!("{}   ·   {}   ·   {}   ·   {}", disk.id, bytes(disk.size),
            disk.transport.as_deref().unwrap_or("Conexão indisponível"),
            disk.scheme.as_deref().unwrap_or("Esquema indisponível").to_uppercase()));
        if let Some(index) = map(ui, disk, *selected) { *selected = Some(index); }
        hint(ui, "Verde: EFI   ·   Violeta: outras partições   ·   Cinza: área sem partição representada");
        if disk.partitions.iter().any(|part| part.offset.is_none()) {
            hint(ui, "Partições sem posição informada são exibidas em sequência.");
        }
    });
    ui.columns(2, |columns| {
        card(&mut columns[0], |ui| {
            title(ui, "Partições");
            if disk.partitions.is_empty() { hint(ui, "Nenhuma partição detectada."); }
            egui::Grid::new("partition-table").striped(true).min_col_width(55.0).show(ui, |ui| {
                ui.strong("Partição"); ui.strong("Formato"); ui.strong("Capacidade"); ui.end_row();
                for (index, part) in disk.partitions.iter().enumerate() {
                    if ui.selectable_label(*selected == Some(index), &part.id).clicked() { *selected = Some(index); }
                    ui.label(part.filesystem.as_deref().unwrap_or("—"));
                    ui.label(bytes(part.size)); ui.end_row();
                }
            });
        });
        card(&mut columns[1], |ui| {
            title(ui, "Propriedades");
            if let Some(part) = selected.and_then(|index| disk.partitions.get(index)) {
                properties(ui, part);
            } else { hint(ui, "Selecione uma partição na tabela ou no mapa."); }
        });
    });
}

fn properties(ui: &mut Ui, part: &Partition) {
    ui.strong(&part.id);
    if let Some(label) = &part.label { hint(ui, format!("Nome GPT    {label}")); }
    let display = |value: Option<u64>| value.map_or("Indisponível".into(), bytes);
    hint(ui, if part.is_esp() { "Tipo    Partição de sistema EFI" } else { "Tipo    Partição de disco" });
    hint(ui, format!("Montagem    {}", part.mount.as_deref().unwrap_or("Sem ponto informado")));
    hint(ui, format!("Capacidade    {}", bytes(part.size)));
    hint(ui, format!("Usado no volume    {}", display(part.used)));
    hint(ui, format!("Livre no volume    {}", display(part.available)));
}

pub fn uefi(ui: &mut Ui, result: Option<&Result<String, String>>) {
    card(ui, |ui| {
        title(ui, "Entradas do firmware");
        match result {
            Some(Ok(report)) => {
                hint(ui, "Relatório de leitura do sistema. Inclui a ordem e caminhos quando informados pelo firmware.");
                if ui.button("Copiar relatório").clicked() { ui.ctx().copy_text(report.clone()); }
                egui::ScrollArea::horizontal().show(ui, |ui| { ui.monospace(report); });
            }
            Some(Err(error)) => hint(ui, error),
            None => hint(ui, "Aguardando consulta."),
        }
    });
    hint(ui, "O relatório lista as entradas disponíveis. A restauração das entradas UEFI ainda está pendente.");
}

#[derive(Clone, Serialize, Deserialize)]
struct Theme {
    name: String,
    background: [u8; 3],
    accent: [u8; 3],
    text: [u8; 3],
    radius: u8,
    size: f32,
    horizontal: bool,
}

impl Theme {
    fn preset(index: usize) -> Self {
        let (name, background, accent, text) = [
            ("Nexus Dark", [13, 18, 28], [91, 231, 207], [238, 245, 253]),
            ("Nexus Light", [234, 240, 248], [6, 100, 124], [20, 31, 52]),
            ("AMOLED", [0, 0, 0], [101, 240, 189], [245, 245, 245]),
            ("Midnight", [19, 20, 53], [151, 168, 255], [240, 238, 255]),
            ("Cyberpunk", [34, 13, 39], [255, 216, 74], [254, 234, 254]),
            ("Minimal", [29, 33, 38], [181, 205, 221], [240, 242, 245]),
        ][index];
        Self { name: name.into(), background, accent, text, radius: 12, size: 22.0, horizontal: false }
    }

    fn valid(&self) -> bool {
        (12.0..=40.0).contains(&self.size) && self.radius <= 24 && !self.name.trim().is_empty()
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
struct SystemProfile {
    name: String,
    loader: String,
    partition: String,
    icon: String,
    hidden: bool,
    default: bool,
}

#[derive(Serialize, Deserialize)]
pub struct Preferences {
    theme: Theme,
    systems: Vec<SystemProfile>,
    pub scale: f32,
    timeout: u16,
    automatic: bool,
    message: String,
}

impl Default for Preferences {
    fn default() -> Self {
        Self { theme: Theme::preset(0), systems: Vec::new(), scale: 1.0, timeout: 5,
            automatic: false, message: "Selecione um sistema para iniciar.".into() }
    }
}

impl Preferences {
    fn valid(&self) -> bool {
        self.theme.valid() && (0.85..=1.25).contains(&self.scale) && self.timeout <= 300
            && self.systems.iter().filter(|profile| profile.default).count() <= 1
    }
}

#[derive(Default)]
struct Planner {
    preset: usize,
    sizes: [u32; 3],
    gap: usize,
    alignment: u32,
    custom_kind: usize,
    custom_format: usize,
    label: String,
}

pub struct Sections {
    pub preferences: Preferences,
    notice: Option<String>,
    theme_path: String,
    backup_path: String,
    editor: Option<(Option<usize>, SystemProfile)>,
    remove: Option<usize>,
    planner: Planner,
}

impl Default for Sections {
    fn default() -> Self {
        let loaded = config_path().and_then(|path| {
            if path.exists() { read_json(&path).and_then(validate_preferences) } else { Ok(Preferences::default()) }
        });
        let (preferences, notice) = match loaded {
            Ok(preferences) => (preferences, None),
            Err(error) => (Preferences::default(), Some(error)),
        };
        Self { preferences, notice, theme_path: String::new(), backup_path: String::new(), editor: None,
            remove: None, planner: Planner { sizes: [512; 3], alignment: 1, label: "Nexus Boot".into(), ..Default::default() } }
    }
}

fn config_path() -> Result<PathBuf, String> {
    let base = if cfg!(target_os = "windows") {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
    };
    base.map(|path| path.join("nexus-boot").join("preferences.json"))
        .ok_or_else(|| "Diretório de preferências indisponível.".into())
}

fn read_json<T: serde::de::DeserializeOwned>(path: &std::path::Path) -> Result<T, String> {
    let data = fs::read(path).map_err(|error| error.to_string())?;
    serde_json::from_slice(&data).map_err(|error| error.to_string())
}

fn save_json<T: Serialize>(path: &std::path::Path, value: &T, overwrite: bool) -> Result<(), String> {
    use std::io::Write;
    let data = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    let mut file = fs::OpenOptions::new().write(true).create(overwrite).truncate(overwrite)
        .create_new(!overwrite).open(path).map_err(|error| error.to_string())?;
    file.write_all(&data).and_then(|_| file.sync_all()).map_err(|error| error.to_string())
}

fn validate_preferences(preferences: Preferences) -> Result<Preferences, String> {
    if preferences.valid() { Ok(preferences) } else { Err("Preferências fora dos limites suportados.".into()) }
}

impl Sections {
    fn report(&mut self, result: Result<(), String>, success: &str) {
        self.notice = Some(result.map_or_else(|error| format!("Falha: {error}"), |_| success.into()));
    }

    fn save(&mut self) {
        let result = config_path().and_then(|path| {
            let parent = path.parent().ok_or("Diretório de preferências inválido.")?;
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            save_json(&path, &self.preferences, true)
        });
        self.report(result, "Preferências salvas neste computador.");
    }

    fn feedback(&self, ui: &mut Ui) {
        if let Some(notice) = &self.notice { ui.label(RichText::new(notice).color(ACCENT)); }
    }

    pub fn systems(&mut self, ui: &mut Ui) {
        card(ui, |ui| {
            title(ui, "Perfis de boot");
            hint(ui, "Registros locais. Detecção automática e uso pelo executável EFI ainda estão pendentes.");
            if ui.button("+  Adicionar sistema").clicked() {
                self.editor = Some((None, SystemProfile::default()));
            }
        });
        let mut move_to = None;
        let mut default = None;
        let count = self.preferences.systems.len();
        for (i, profile) in self.preferences.systems.iter_mut().enumerate() {
            card(ui, |ui| {
                title(ui, &format!("{}  {}", profile.icon, profile.name));
                hint(ui, format!("{}   ·   {}", profile.partition, profile.loader));
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Editar").clicked() { self.editor = Some((Some(i), profile.clone())); }
                    if ui.button(if profile.default { "✓ Padrão" } else { "Definir padrão" }).clicked() { default = Some(i); }
                    ui.checkbox(&mut profile.hidden, "Ocultar");
                    if ui.add_enabled(i > 0, egui::Button::new("Subir")).clicked() { move_to = Some((i, i - 1)); }
                    if ui.add_enabled(i + 1 < count, egui::Button::new("Descer")).clicked() { move_to = Some((i, i + 1)); }
                    if ui.button("Remover").clicked() { self.remove = Some(i); }
                });
            });
        }
        if let Some(index) = default {
            for (i, profile) in self.preferences.systems.iter_mut().enumerate() { profile.default = i == index; }
        }
        if let Some((from, to)) = move_to { self.preferences.systems.swap(from, to); }
        if let Some(index) = self.remove.take() {
            if let Some(profile) = self.preferences.systems.get(index) {
                let mut confirm = false;
                let mut cancel = false;
                egui::Window::new("Remover perfil").collapsible(false).show(ui.ctx(), |ui| {
                    ui.label(format!("Remover o registro local «{}»?", profile.name));
                    ui.horizontal(|ui| {
                        confirm = ui.button("Confirmar remoção").clicked();
                        cancel = ui.button("Cancelar").clicked();
                    });
                });
                if confirm { self.preferences.systems.remove(index); }
                else if !cancel { self.remove = Some(index); }
            }
        }
        if let Some((index, mut profile)) = self.editor.take() {
            let mut commit = false;
            let mut cancel = false;
            egui::Window::new("Editar perfil de boot").collapsible(false).show(ui.ctx(), |ui| {
                editor_field(ui, "Nome", &mut profile.name);
                editor_field(ui, "Ícone ou símbolo", &mut profile.icon);
                editor_field(ui, "Partição ou PARTUUID", &mut profile.partition);
                editor_field(ui, "Caminho EFI", &mut profile.loader);
                let valid = !profile.name.trim().is_empty() && !profile.partition.trim().is_empty()
                    && profile.loader.to_ascii_lowercase().ends_with(".efi");
                ui.horizontal(|ui| {
                    commit = ui.add_enabled(valid, egui::Button::new("Aplicar")).clicked();
                    cancel = ui.button("Cancelar").clicked();
                });
            });
            if commit {
                if let Some(index) = index {
                    if let Some(target) = self.preferences.systems.get_mut(index) {
                        profile.default = target.default;
                        *target = profile;
                    }
                } else { self.preferences.systems.push(profile); }
            } else if !cancel { self.editor = Some((index, profile)); }
        }
        if ui.button("Salvar perfis").clicked() { self.save(); }
        self.feedback(ui);
    }

    pub fn install_plan(&self, disk: &Disk) -> Option<crate::wizard::Plan> {
        let plan = &self.planner;
        if plan.preset != 0 { return None; }
        let gaps = free_extents(disk)?;
        let (start, end) = *gaps.get(plan.gap.min(gaps.len().saturating_sub(1)))?;
        let align = u64::from(plan.alignment) * MIB;
        let start = start.div_ceil(align) * align;
        let size = u64::from(plan.sizes[2]) * MIB;
        let label = plan.label.trim();
        if size < 64 * MIB || start.checked_add(size)? > end || label.is_empty() || label.len() > 36
            || !label.bytes().all(|b| b.is_ascii_alphanumeric() || b"_. -".contains(&b)) {
            return None;
        }
        Some(crate::wizard::Plan { start, size, label: label.into(), table_uuid: disk.table_uuid.clone() })
    }

    pub fn plan(&mut self, ui: &mut Ui, disk: Option<&Disk>) {
        const PRESETS: [&str; 5] = ["EFI exclusiva do Nexus Boot", "EFI independente do Windows",
            "EFI independente do Linux", "Três ESPs independentes", "Personalizado"];
        const FORMATS: [&str; 4] = ["Sem formatação", "FAT32", "ext4", "NTFS"];
        const NAMES: [&str; 3] = ["EFI Windows", "EFI Linux", "EFI Nexus Boot"];
        let planner = &mut self.planner;
        card(ui, |ui| {
            title(ui, "Perfil e dimensionamento");
            egui::ComboBox::from_id_salt("profile").selected_text(PRESETS[planner.preset]).show_ui(ui, |ui| {
                for (i, label) in PRESETS.iter().enumerate() { ui.selectable_value(&mut planner.preset, i, *label); }
            });
            let rows = match planner.preset { 0 => &[2][..], 1 => &[0][..], 2 => &[1][..], _ => &[0, 1, 2][..] };
            for &index in rows {
                ui.horizontal(|ui| {
                    ui.label(NAMES[index]);
                    let minimum = if planner.preset == 4 { 0 } else { 64 };
                    ui.add(egui::DragValue::new(&mut planner.sizes[index]).range(minimum..=32768).suffix(" MiB"));
                });
            }
            ui.horizontal(|ui| {
                ui.label("Alinhamento");
                egui::ComboBox::from_id_salt("alignment").selected_text(format!("{} MiB", planner.alignment))
                    .show_ui(ui, |ui| for value in [1, 2, 4, 8] {
                        ui.selectable_value(&mut planner.alignment, value, format!("{value} MiB"));
                    });
            });
            editor_field(ui, "Rótulo da ESP Nexus", &mut planner.label);
            if planner.preset == 4 {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Tipo da terceira partição");
                    ui.selectable_value(&mut planner.custom_kind, 0, "ESP");
                    ui.selectable_value(&mut planner.custom_kind, 1, "Dados");
                });
                if planner.custom_kind == 1 {
                    egui::ComboBox::from_id_salt("custom-format").selected_text(FORMATS[planner.custom_format])
                        .show_ui(ui, |ui| {
                            for (index, format) in FORMATS.iter().enumerate() {
                                ui.selectable_value(&mut planner.custom_format, index, *format);
                            }
                        });
                }
            } else {
                hint(ui, "Tipo EFI System Partition   ·   FAT32");
            }
        });
        let rows: &[usize] = match planner.preset { 0 => &[2], 1 => &[0], 2 => &[1], _ => &[0, 1, 2] };
        card(ui, |ui| {
            title(ui, "Prévia · Dry Run");
            hint(ui, "A prévia não toca no disco. O instalador pode criar a ESP no intervalo selecionado.");
            let Some(disk) = disk else { hint(ui, "Selecione um disco inventariado."); return };
            hint(ui, "Distribuição atual");
            map(ui, disk, None);
            let Some(gaps) = free_extents(disk) else {
                hint(ui, "Mapa de espaço livre indisponível para este esquema ou inventário.");
                return;
            };
            if gaps.is_empty() { hint(ui, "Nenhum intervalo livre identificado."); return; }
            planner.gap = planner.gap.min(gaps.len() - 1);
            let gap_label = |(start, end)| format!("{} livres a partir de {}", bytes(end - start), bytes(start));
            egui::ComboBox::from_id_salt("gap").selected_text(gap_label(gaps[planner.gap])).show_ui(ui, |ui| {
                for (index, &gap) in gaps.iter().enumerate() { ui.selectable_value(&mut planner.gap, index, gap_label(gap)); }
            });
            let (start, end) = gaps[planner.gap];
            let alignment = u64::from(planner.alignment) * MIB;
            let mut cursor = start;
            let mut count = 0;
            let mut planned = Vec::new();
            for &index in rows {
                let size = u64::from(planner.sizes[index]) * MIB;
                if size == 0 { continue; }
                cursor = cursor.div_ceil(alignment) * alignment;
                let label = if index == 2 { &planner.label } else { NAMES[index] };
                let format = if planner.preset == 4 && index == 2 && planner.custom_kind == 1 {
                    FORMATS[planner.custom_format]
                } else { "ESP / FAT32" };
                hint(ui, format!("Criar {label}   ·   {format}   ·   {}   ·   início em {}", bytes(size), bytes(cursor)));
                planned.push((cursor, cursor + size));
                cursor += size;
                count += 1;
            }
            hint(ui, format!("{count} a criar   ·   0 a modificar   ·   0 a remover"));
            if cursor > end || rows.iter().any(|&index| (1..64).contains(&planner.sizes[index]) && !(planner.preset == 4 && index == 2 && planner.custom_kind == 1)) {
                ui.colored_label(VIOLET, "Ajuste os tamanhos. Cada ESP planejada deve ter no mínimo 64 MiB e caber no intervalo.");
            } else {
                hint(ui, "Distribuição planejada no intervalo selecionado");
                let (bar, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 24.0), egui::Sense::hover());
                ui.painter().rect_filled(bar, 5, LINE);
                let position = |offset| bar.left() + bar.width() * ((offset - start) as f64 / (end - start) as f64) as f32;
                for (begin, finish) in planned {
                    let segment = egui::Rect::from_min_max(egui::pos2(position(begin), bar.top()), egui::pos2(position(finish), bar.bottom()));
                    ui.painter().rect_filled(segment, 3, ACCENT);
                }
                hint(ui, format!("Espaço restante após o plano    {}", bytes(end - cursor)));
            }
        });
        hint(ui, "Para renomear ou redimensionar uma partição existente, abra Discos e partições. Excluir, mover e formatar ainda estão indisponíveis.");
    }
    pub fn themes(&mut self, ui: &mut Ui) {
        card(ui, |ui| {
            title(ui, "Biblioteca de temas");
            ui.horizontal_wrapped(|ui| {
                for index in 0..6 {
                    let preset = Theme::preset(index);
                    if ui.selectable_label(self.preferences.theme.name == preset.name, &preset.name).clicked() {
                        self.preferences.theme = preset;
                    }
                }
            });
        });
        {
            let theme = &mut self.preferences.theme;
            card(ui, |ui| {
                title(ui, "Editor visual");
                editor_field(ui, "Nome", &mut theme.name);
                ui.horizontal_wrapped(|ui| {
                    for (label, color) in [("Fundo", &mut theme.background), ("Destaque", &mut theme.accent), ("Texto", &mut theme.text)] {
                        ui.label(label);
                        ui.color_edit_button_srgb(color);
                    }
                });
                ui.add(egui::Slider::new(&mut theme.size, 12.0..=40.0).text("Tipografia"));
                ui.add(egui::Slider::new(&mut theme.radius, 0..=24).text("Arredondamento"));
                ui.checkbox(&mut theme.horizontal, "Cards lado a lado");
            });
            card(ui, |ui| {
                title(ui, "Prévia do menu EFI");
                let rgb = |color: [u8; 3]| Color32::from_rgb(color[0], color[1], color[2]);
                frame(rgb(theme.background), 22).corner_radius(theme.radius).show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.label(RichText::new("NEXUS BOOT").size(theme.size).strong().color(rgb(theme.accent)));
                    ui.label(RichText::new(&self.preferences.message).color(rgb(theme.text)));
                    let profiles = |ui: &mut Ui| {
                        for profile in self.preferences.systems.iter().filter(|profile| !profile.hidden) {
                            ui.label(RichText::new(format!("{} {}", profile.icon, profile.name))
                                .size(theme.size * 0.75).color(rgb(theme.text)));
                        }
                    };
                    if theme.horizontal { ui.horizontal_wrapped(profiles); } else { profiles(ui); }
                });
                hint(ui, "Prévia visual no painel. O executável EFI atual ainda não usa estes temas.");
            });
        }
        card(ui, |ui| {
            title(ui, "Importar e exportar tema JSON");
            editor_field(ui, "Caminho do arquivo", &mut self.theme_path);
            ui.horizontal_wrapped(|ui| {
                let path = PathBuf::from(self.theme_path.trim());
                if ui.button("Importar tema").clicked() {
                    match read_json::<Theme>(&path) {
                        Ok(theme) if theme.valid() => { self.preferences.theme = theme; self.notice = Some("Tema importado.".into()); }
                        Ok(_) => self.notice = Some("Tema fora dos limites suportados.".into()),
                        Err(error) => self.notice = Some(error),
                    }
                }
                if ui.add_enabled(self.preferences.theme.valid(), egui::Button::new("Exportar novo arquivo")).clicked() {
                    self.report(save_json(&path, &self.preferences.theme, false), "Tema exportado.");
                }
                if ui.add_enabled(self.preferences.theme.valid(), egui::Button::new("Salvar preferência local")).clicked() { self.save(); }
            });
        });
        self.feedback(ui);
    }

    pub fn settings(&mut self, ui: &mut Ui) {
        card(ui, |ui| {
            title(ui, "Inicialização");
            ui.add(egui::Slider::new(&mut self.preferences.timeout, 0..=300).text("Espera em segundos"));
            ui.checkbox(&mut self.preferences.automatic, "Inicialização automática");
            editor_field(ui, "Mensagem de inicialização", &mut self.preferences.message);
            hint(ui, "Aplique o sistema padrão em Sistemas operacionais. Estas preferências são locais.");
        });
        card(ui, |ui| {
            title(ui, "Interface");
            ui.add(egui::Slider::new(&mut self.preferences.scale, 0.85..=1.25).text("Escala do painel"));
            hint(ui, "Idioma atual    Português");
        });
        card(ui, |ui| {
            title(ui, "Segurança e diagnóstico");
            hint(ui, format!("Ambiente    {} / {}", std::env::consts::OS, std::env::consts::ARCH));
            hint(ui, "Backups manuais das preferências estão em Recuperação. Proteção da ESP e verificação de integridade ainda estão pendentes.");
            if let Ok(path) = config_path() { hint(ui, format!("Arquivo local    {}", path.display())); }
        });
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        if ui.button("Adicionar Nexus Boot ao menu do sistema").clicked() {
            self.report(crate::branding::install_menu_entry(), "Atalho do Nexus Boot instalado para este usuário.");
        }
        if ui.button("Salvar preferências").clicked() { self.save(); }
        self.feedback(ui);
    }

    pub fn recovery(&mut self, ui: &mut Ui) {
        card(ui, |ui| {
            title(ui, "Backup local");
            hint(ui, "Inclui perfis, tema e preferências do painel. A importação substitui as preferências em memória. Salve para torná-las persistentes.");
            editor_field(ui, "Caminho do arquivo JSON (opcional para criar)", &mut self.backup_path);
            ui.horizontal_wrapped(|ui| {
                let path = PathBuf::from(self.backup_path.trim());
                if ui.add_enabled(self.preferences.valid(), egui::Button::new("Criar novo backup")).clicked() {
                    let result = if self.backup_path.trim().is_empty() {
                        config_path().and_then(|path| {
                            let directory = path.with_file_name("backups");
                            fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
                            let stamp = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH).map_err(|error| error.to_string())?.as_millis();
                            let target = directory.join(format!("nexus-boot-{stamp}.json"));
                            save_json(&target, &self.preferences, false)?;
                            self.backup_path = target.to_string_lossy().into_owned();
                            Ok(())
                        })
                    } else { save_json(&path, &self.preferences, false) };
                    self.report(result, "Backup criado.");
                }
                if ui.add_enabled(!self.backup_path.trim().is_empty(), egui::Button::new("Carregar backup")).clicked() {
                    match read_json(&path).and_then(validate_preferences) {
                        Ok(preferences) => { self.preferences = preferences; self.notice = Some("Backup carregado em memória.".into()); }
                        Err(error) => self.notice = Some(format!("Falha: {error}")),
                    }
                }
                if ui.button("Salvar versão carregada").clicked() { self.save(); }
            });
            self.feedback(ui);
        });
        card(ui, |ui| {
            title(ui, "Recuperação do boot");
            hint(ui, "A opção abaixo localiza o protótipo instalado em uma ESP Nexus e registra a entrada permanente ou agenda um teste de inicialização. O backup JSON acima guarda apenas preferências.");
        });
    }
}

fn editor_field(ui: &mut Ui, label: &str, value: &mut String) {
    ui.label(label);
    ui.add(egui::TextEdit::singleline(value).desired_width(f32::INFINITY));
}

fn free_extents(disk: &Disk) -> Option<Vec<(u64, u64)>> {
    let supported = disk.scheme.as_deref().is_some_and(|scheme| scheme.eq_ignore_ascii_case("gpt")
        || scheme.eq_ignore_ascii_case("raw"));
    if !supported && !(disk.scheme.is_none() && disk.partitions.is_empty()) { return None; }
    let mut occupied: Vec<_> = disk.partitions.iter().map(|part| {
        let start = part.offset?;
        Some((start, start.checked_add(part.size)?))
    }).collect::<Option<_>>()?;
    occupied.sort_unstable();
    let limit = disk.size.saturating_sub(MIB);
    let mut cursor = MIB;
    let mut gaps = Vec::new();
    for (start, end) in occupied {
        let bound = start.min(limit);
        if bound > cursor { gaps.push((cursor, bound)); }
        cursor = cursor.max(end);
    }
    if limit > cursor { gaps.push((cursor, limit)); }
    Some(gaps)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gaps_respect_offsets_and_metadata_reserve() {
        let disk = Disk { id: "test".into(), size: 100 * MIB, model: None,
            scheme: Some("gpt".into()), transport: None, table_uuid: None,
            partitions: vec![Partition { id: "p1".into(), size: 40 * MIB, part_type: None,
                filesystem: None, available: None, used: None, mount: None, offset: Some(10 * MIB), uuid: None, label: None, volume_label: None }] };
        assert_eq!(free_extents(&disk), Some(vec![(MIB, 10 * MIB), (50 * MIB, 99 * MIB)]));
    }
}
