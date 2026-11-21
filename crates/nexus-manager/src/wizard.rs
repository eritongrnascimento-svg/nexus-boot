use std::sync::mpsc::{self, Receiver, TryRecvError};

use eframe::egui;

use crate::{installer, storage::Disk};

#[derive(Default)]
enum Step {
    #[default]
    Welcome,
    Disk,
    Install(String),
    Finished(String),
}

#[derive(Clone)]
pub struct Plan {
    pub start: u64,
    pub size: u64,
    pub label: String,
    pub table_uuid: Option<String>,
}

#[derive(Default)]
pub struct Wizard {
    step: Step,
    target: String,
    pending: Option<Receiver<(&'static str, Result<String, String>)>>,
    error: Option<String>,
    status: Option<String>,
}

impl Wizard {
    pub fn show(&mut self, ui: &mut egui::Ui, disk: Option<&Disk>, plan: Option<&Plan>) {
        self.poll();
        ui.separator();
        if let Some(error) = &self.error {
            ui.label(error);
        }
        if self.pending.is_some() {
            ui.spinner();
            ui.label("Aguarde a conclusão da operação.");
            return;
        }

        match &self.step {
            Step::Welcome => {
                ui.heading("Bem-vindo ao Nexus Boot");
                ui.label("O assistente cria uma partição EFI dedicada, copia os arquivos do Nexus e prepara uma opção para o firmware.");
                ui.label("O seletor EFI mostra os carregadores compatíveis detectados. Setas selecionam, Enter inicia e Esc retorna ao firmware.");
                if cfg!(target_os = "windows") {
                    ui.label("Neste sistema, o painel oferece o inventário de discos. O assistente de instalação está disponível no Linux.");
                } else if ui.button("Começar instalação").clicked() {
                    self.step = Step::Disk;
                }
            }
            Step::Disk => {
                ui.heading("Criar a partição EFI");
                ui.label("Use o perfil EFI exclusiva do Nexus Boot e selecione espaço não alocado na prévia.");
                if let Some(disk) = disk {
                    ui.label(format!("Destino: {}", disk.id));
                    if let Some(plan) = plan {
                        ui.label(format!("{} em FAT32, a partir de {}. Rótulo: {}.",
                            crate::design::bytes(plan.size), crate::design::bytes(plan.start), plan.label));
                        ui.label("A operação cria somente a nova ESP e preserva as partições existentes.");
                        if ui.button("Confirmar criação da partição").clicked() {
                            self.target.clone_from(&disk.id);
                            self.start(ui.ctx(), "create", String::new(), Some(plan.clone()));
                        }
                    } else {
                        ui.label("Não há um plano aplicável. Confira o tamanho, o intervalo, o rótulo e o esquema do disco.");
                    }
                }
            }
            Step::Install(uuid) => {
                ui.heading("Instalar o Nexus Boot?");
                ui.label(format!("A partição EFI foi criada em {}. Você pode fechar o painel e deixar a partição vazia.", self.target));
                if ui.button("Instalar nesta partição").clicked() {
                    let uuid = uuid.clone();
                    self.start(ui.ctx(), "install", uuid, None);
                }
            }
            Step::Finished(uuid) => {
                ui.heading("Nexus Boot instalado");
                ui.label("Ordem de boot UEFI");
                ui.label("Escolha a prioridade aqui e reinicie normalmente. O teste de próximo boot vale apenas uma vez.");
                if ui.button("Definir Nexus como primeira opção de boot").clicked() {
                    self.start(ui.ctx(), "primary", uuid.clone(), None);
                    return;
                }
                if ui.button("Mover Nexus para o fim da ordem de boot").clicked() {
                    self.start(ui.ctx(), "last", uuid.clone(), None);
                    return;
                }
                if ui.button("Adicionar ao menu UEFI permanente").on_hover_text("Inclui o Nexus no fim da ordem de boot.").clicked() {
                    self.start(ui.ctx(), "register", uuid.clone(), None);
                } else if ui.button("Agendar teste no próximo boot").clicked() {
                    self.start(ui.ctx(), "bootnext", uuid.clone(), None);
                }
                if let Some(message) = &self.status { ui.label(message); }
                if ui.button("Reiniciar agora no setup UEFI").clicked() {
                    self.start(ui.ctx(), "reboot", String::new(), None);
                }
            }
        }
    }

    pub fn recover(&mut self, ui: &mut egui::Ui, disks: &[Disk]) {
        if !cfg!(target_os = "linux") { return; }
        if self.pending.is_some() {
            self.poll();
            if self.pending.is_some() {
                ui.spinner();
                ui.label("Aguardando a operação de firmware.");
                return;
            }
        }
        if let Some(message) = &self.status { ui.label(message); }
        if let Some(error) = &self.error { ui.label(error); }
        let mut found = false;
        for disk in disks {
            for part in &disk.partitions {
                if !part.is_esp() || part.volume_label.as_deref() != Some("NEXUSBOOT") { continue; }
                let Some(uuid) = &part.uuid else { continue };
                found = true;
                ui.label(format!("Destino dos backups: {} (ESP do Nexus). As origens são as outras ESPs de sistemas.", part.id));
                if ui.button("Copiar arquivos EFI dos sistemas para esta ESP do Nexus").clicked() {
                    self.target.clone_from(&disk.id);
                    self.start(ui.ctx(), "backup", uuid.clone(), None);
                    return;
                }
                if ui.button(format!("Diagnosticar boot UEFI · {}", part.id)).clicked() {
                    self.target.clone_from(&disk.id);
                    self.start(ui.ctx(), "inspect", uuid.clone(), None);
                    return;
                }
                if ui.button(format!("Atualizar seletor EFI · {}", part.id)).clicked() {
                    self.target.clone_from(&disk.id);
                    self.start(ui.ctx(), "upgrade", uuid.clone(), None);
                    return;
                }
                if ui.button(format!("Definir Nexus como primeira opção de boot · {}", part.id)).clicked() {
                    self.target.clone_from(&disk.id);
                    self.start(ui.ctx(), "primary", uuid.clone(), None);
                    return;
                }
                if ui.button(format!("Mover Nexus para o fim da ordem de boot · {}", part.id)).clicked() {
                    self.target.clone_from(&disk.id);
                    self.start(ui.ctx(), "last", uuid.clone(), None);
                    return;
                }
                if ui.button(format!("Instalar arquivos EFI em {}", part.id)).clicked() {
                    self.target.clone_from(&disk.id);
                    self.start(ui.ctx(), "install", uuid.clone(), None);
                    return;
                }
                if ui.button(format!("Adicionar ao menu UEFI permanente · {}", part.id))
                    .on_hover_text("Inclui o Nexus no fim da ordem de boot.").clicked() {
                    self.target.clone_from(&disk.id);
                    self.start(ui.ctx(), "register", uuid.clone(), None);
                    return;
                }
                if ui.button(format!("Agendar teste no próximo boot · {}", part.id)).clicked() {
                    self.target.clone_from(&disk.id);
                    self.start(ui.ctx(), "bootnext", uuid.clone(), None);
                }
            }
        }
        if !found { ui.label("Nenhuma ESP com o rótulo NEXUSBOOT encontrada. Use Verificar para atualizar o inventário."); }
    }

    fn start(&mut self, context: &egui::Context, action: &'static str, uuid: String, plan: Option<Plan>) {
        let (sender, receiver) = mpsc::channel();
        let context = context.clone();
        let target = self.target.clone();
        self.error = None;
        self.status = None;
        self.pending = Some(receiver);
        std::thread::spawn(move || {
            let result = if action == "reboot" {
                installer::reboot_to_firmware()
            } else {
                installer::run(action, &target, &uuid, plan.as_ref())
            };
            let _ = sender.send((action, result));
            context.request_repaint();
        });
    }

    fn poll(&mut self) {
        let Some(pending) = &self.pending else { return };
        match pending.try_recv() {
            Ok(("create", Ok(uuid))) => self.step = Step::Install(uuid),
            Ok(("install", Ok(message))) => {
                if let Step::Install(uuid) = &self.step { self.step = Step::Finished(uuid.clone()); }
                self.status = Some(if message.is_empty() { "Arquivos EFI instalados.".into() } else { message });
            }
            Ok(("bootnext" | "register" | "upgrade" | "primary" | "last" | "inspect" | "backup", Ok(message))) => self.status = Some(message),
            Ok((_, Ok(_))) => {}
            Ok((_, Err(error))) => self.error = Some(error),
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => self.error = Some("A operação foi interrompida.".into()),
        }
        self.pending = None;
    }
}
