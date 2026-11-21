# Arquitetura

| Componente | Estado na Fase 1 | Responsabilidade |
| --- | --- | --- |
| Nexus Core | Crate no_std com versão do formato compartilhada | Modelo de entradas e regras independentes de plataforma |
| Nexus EFI | Protótipo de console UEFI | Interface pré-boot, integração com protocolos do firmware e execução de carregadores |
| Nexus Storage | Planejado | Leitura, validação e gravação de configurações, inventário GPT/ESP |
| Nexus Theme Engine | Formato preliminar | Validação de temas, layout e uso de tema padrão em caso de falha |
| Nexus Recovery | Planejado | Inventário de alterações, backups e restauração autorizada |
| Nexus Manager | Inventário de discos e ESPs, assistente Linux | Administração no Windows e Linux |

O firmware carrega a aplicação PE32+ da mídia virtual e chama seu ponto de entrada.
O protótipo imprime a versão, aguarda dez segundos e retorna ao firmware. Os crates
Core e EFI usam no_std. O acesso aos protocolos UEFI passa por uefi-rs. Dependências
de GPT, FAT e gráficos serão avaliadas quando seus respectivos módulos forem implementados.

## Formatos preliminares

Os exemplos em examples/ usam TOML e reservam o campo version para migrações.
O arquivo nexus.toml define o tempo em segundos e o identificador de tema.
O tema define nome, layout e cores RGB no formato #RRGGBB. O único layout
exemplificado é vertical. O protótipo ainda não lê esses arquivos.

Para a detecção futura, a identidade de um carregador deve combinar o GUID da
partição GPT e o caminho EFI dentro da ESP. O inventário começa em modo de
leitura. O fluxo de instalação deve apresentar as mudanças propostas antes
de gravar arquivos ou editar variáveis UEFI.

O caminho removível EFI/BOOT/BOOTX64.EFI é usado apenas na mídia de teste da
Fase 1. Para recuperação futura, a preservação de arquivos de terceiros,
o backup da configuração e o registro prévio das entradas do firmware
são requisitos para operações de restauração.

A distribuição para Secure Boot dependerá de uma estratégia de assinatura e
confiança. O protótipo gera uma imagem sem assinatura para a VM de desenvolvimento.

## Instalação e configuração pelo painel

O mesmo painel deve oferecer os fluxos de instalação e manutenção no Windows
e no Linux. A interface apresenta a prévia do disco, da ESP e das mudanças no
firmware. Após confirmação, o backend da plataforma prepara a ESP dedicada e
instala o binário EFI. A configuração do sistema padrão, do tempo de espera,
dos temas, da aparência e das mensagens de inicialização deve chegar ao
firmware por meio de arquivos versionados administrados pelo painel.

A opção de proteção deve preservar o tipo EFI System Partition, evitar uma
montagem persistente de uso cotidiano e guardar cópias verificáveis dos
arquivos e das entradas UEFI. O painel deve detectar mudanças na ESP e na
ordem de boot separadamente, oferecer restauração mediante confirmação e
informar o estado da proteção. A ausência de uma letra de unidade não impede
gravações por software privilegiado. Atributos definidos para partições GPT
de dados não devem ser tratados como garantia de proteção de uma ESP.

O painel implementa o inventário nos dois sistemas e o assistente de
instalação do protótipo no Linux. A instalação no Windows, o carregamento da
configuração pela aplicação EFI e a proteção com recuperação continuam
pendentes.
