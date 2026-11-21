set -euo pipefail
export LC_ALL=C
export PATH=/usr/sbin:/usr/bin:/sbin:/bin

action=$1
disk=$2
[[ -d /sys/firmware/efi ]] || { echo "Inicie o sistema em modo UEFI." >&2; exit 1; }
[[ "$disk" == /dev/* && -b "$disk" ]] || exit 1
[[ "$(lsblk -dnro TYPE "$disk")" == disk ]] || exit 1
for tool in sfdisk mkfs.fat udevadm wipefs efibootmgr partx blockdev awk grep blkid; do
    command -v "$tool" >/dev/null || { echo "Dependência ausente: $tool" >&2; exit 1; }
done
efibootmgr >/dev/null

if [[ "$action" == create ]]; then
    start=$5 size=$6 label=$7 original_table=$8
    [[ "$start" =~ ^[0-9]{1,17}$ && "$size" =~ ^[0-9]{1,17}$ ]] || exit 1
    [[ "$label" =~ ^[A-Za-z0-9_.\ -]{1,36}$ ]] || { echo "Rótulo inválido." >&2; exit 1; }
    sector=$(blockdev --getss "$disk")
    (( start >= 1048576 && size >= 67108864 && start % sector == 0 && size % sector == 0 )) || exit 1
    uuid=$(cat /proc/sys/kernel/random/uuid)
    spec="start=$((start / sector)), size=$((size / sector)), type=U, uuid=$uuid, name=\"$label\""
    scheme=$(lsblk -dnro PTTYPE "$disk")
    if [[ "$scheme" == gpt ]]; then
        [[ -n "$original_table" && "$(lsblk -dnro PTUUID "$disk")" == "$original_table" ]] || {
            echo "A tabela GPT mudou. Atualize o inventário antes de tentar novamente." >&2; exit 1;
        }
        append=(sfdisk --lock=yes --no-reread --no-tell-kernel --wipe never --wipe-partitions never --append "$disk")
        printf '%s\n' "$spec" | "${append[@]}" --no-act >&2
        umask 077
        install -d -m 700 /var/backups/nexus-boot
        backup=$(mktemp /var/backups/nexus-boot/gpt.XXXXXXXX)
        sfdisk --dump "$disk" > "$backup"
        printf 'Backup da tabela GPT: %s\n' "$backup" >&2
        write_status=0
        printf '%s\n' "$spec" | "${append[@]}" >&2 || write_status=$?
        sfdisk --verify "$disk" >&2 || {
            echo "A tabela GPT não passou na verificação. Nenhuma partição foi formatada." >&2
            exit 1
        }
        if [[ "$(lsblk -dnro PTUUID "$disk")" != "$original_table" ]]; then
            echo "O identificador GPT mudou durante a escrita. Nenhuma partição foi formatada." >&2
            exit 1
        fi
        row=$(partx --show --bytes --noheadings --output NR,START,SIZE,UUID,TYPE "$disk" |
            awk -v uuid="$uuid" 'tolower($4) == tolower(uuid) {print $1, $2, $3, tolower($5)}')
        read -r number gpt_start gpt_size gpt_type <<< "$row"
        if [[ ! "$number" =~ ^[0-9]+$ || "$gpt_start" != "$((start / 512))" ||
            "$gpt_size" != "$size" || "$gpt_type" != c12a7328-f81f-11d2-ba4b-00a0c93ec93b ]]; then
            echo "A nova ESP não foi confirmada na GPT (retorno do sfdisk: $write_status). Nenhuma partição foi formatada." >&2
            exit 1
        fi
        if (( write_status != 0 )); then
            echo "O sfdisk retornou $write_status após a gravação. A entrada GPT foi verificada por UUID, início, tamanho e tipo." >&2
        fi
        if ! lsblk -lno PARTN "$disk" | grep -Eq "^[[:space:]]*$number[[:space:]]*$"; then
            if ! partx --add --nr "$number" "$disk"; then
                udevadm settle --timeout=10 || true
                if ! lsblk -lno PARTN "$disk" | grep -Eq "^[[:space:]]*$number[[:space:]]*$"; then
                    echo "ESP gravada na GPT, mas o kernel não publicou a partição $number. Nenhuma partição foi formatada. Reinicie e confira o disco." >&2
                    exit 1
                fi
            fi
        fi
    elif [[ -z "$scheme" && -z "$original_table" ]]; then
        [[ "$(lsblk -nr -o TYPE "$disk")" == disk ]] || { echo "O disco já tem partições." >&2; exit 1; }
        [[ -z "$(wipefs --noheadings --output TYPE "$disk")" ]] || { echo "O disco contém assinaturas existentes." >&2; exit 1; }
        printf 'label: gpt\n%s\n' "$spec" | sfdisk --lock=yes --wipe never --wipe-partitions never "$disk" >&2
    else
        echo "Para instalar neste disco, use GPT ou um disco sem tabela de partições." >&2; exit 1
    fi
    udevadm settle --timeout=10 || true
else
    uuid=$3
fi

[[ "$uuid" =~ ^[0-9a-fA-F-]{36}$ ]] || exit 1
part=$(readlink -e "/dev/disk/by-partuuid/$uuid" || true)
if [[ ! -b "$part" ]]; then
    if [[ -z "${number:-}" ]]; then
        number=$(partx --show --noheadings --output NR,UUID "$disk" |
            awk -v uuid="$uuid" 'tolower($2) == tolower(uuid) {print $1}')
    fi
    if [[ "$number" =~ ^[0-9]+$ ]]; then
        part=$(lsblk -lnpo PATH,PARTN "$disk" | awk -v number="$number" '$2 == number {print $1}')
    fi
fi
if [[ ! -b "$part" || "/dev/$(lsblk -dnro PKNAME "$part")" != "$disk" ]]; then
    echo "O dispositivo da ESP $uuid não ficou disponível. Nenhuma partição foi formatada." >&2
    exit 1
fi
number=$(lsblk -dnro PARTN "$part")
[[ "$number" =~ ^[0-9]+$ ]] || exit 1
part_uuid=$(sfdisk --part-uuid "$disk" "$number")
part_type=$(sfdisk --part-type "$disk" "$number")
if [[ "${part_uuid,,}" != "${uuid,,}" || "${part_type,,}" != c12a7328-f81f-11d2-ba4b-00a0c93ec93b ]]; then
    echo "O dispositivo encontrado não corresponde à ESP esperada. Operação cancelada." >&2
    exit 1
fi
if [[ "$action" == create ]]; then
    new_start=$(cat "/sys/class/block/$(lsblk -dnro KNAME "$part")/start")
    [[ $((new_start * 512)) == "$start" && "$(blockdev --getsize64 "$part")" == "$size" ]] || {
        echo "A partição criada não corresponde ao plano. Ela não foi formatada." >&2; exit 1;
    }
    [[ -z "$(wipefs --noheadings --output TYPE "$part")" ]] || {
        echo "A nova partição contém assinaturas antigas. Ela não foi formatada." >&2; exit 1;
    }
    mkfs.fat -F 32 -n NEXUSBOOT "$part" >&2
    printf '%s\n' "$uuid"
    exit
fi

[[ "$action" == install || "$action" == upgrade || "$action" == bootnext || "$action" == register || "$action" == primary || "$action" == last || "$action" == inspect || "$action" == backup ]] || exit 1
[[ "$(blkid -p -s LABEL -o value "$part")" == NEXUSBOOT ]] || {
    echo "A ESP selecionada não tem o rótulo NEXUSBOOT." >&2; exit 1;
}
loader='\EFI\NexusBoot\NexusBoot.efi'
find_entry() {
    efibootmgr -v | awk -v uuid="$uuid" '
        /^Boot[[:xdigit:]][[:xdigit:]][[:xdigit:]][[:xdigit:]]\*[[:space:]]/ {
            line=tolower($0)
            separator=sprintf("%c", 92)
            loader=separator "efi" separator "nexusboot" separator "nexusboot.efi"
            if (index(line, ",gpt," tolower(uuid) ",") &&
                (index(line, "/file(" loader ")") || index(line, "/" loader) &&
                substr(line, index(line, "/" loader) + length(loader) + 1) ~ /^([[:space:]]|$)/))
                print substr($0, 5, 4)
        }'
}
ensure_entry() {
    local scan
    scan=$(find_entry)
    if [[ -z "$scan" ]]; then
        efibootmgr --create-only --disk "$disk" --part "$number" --label 'Nexus Boot' --loader "$loader" >&2
        scan=$(find_entry)
    fi
    entry=${scan%%$'\n'*}
    [[ "$entry" =~ ^[0-9a-fA-F]{4}$ ]] || { echo "O firmware não retornou uma entrada Nexus válida para esta ESP." >&2; exit 1; }
}
stage=$(mktemp -d)
source_stage=
cleanup() {
    if [[ -n "$source_stage" ]]; then
        if mountpoint -q "$source_stage"; then umount "$source_stage" || true; fi
        rmdir "$source_stage" || true
    fi
    if mountpoint -q "$stage"; then
        umount "$stage" || return
    fi
    rmdir "$stage"
}
trap cleanup EXIT
mount_backup_source() {
    local source=$1 target=$2 mounted encoded fsroot
    mounted=$(findmnt --noheadings --raw --source "$source" --output TARGET,FSROOT || true)
    while read -r encoded fsroot; do
        [[ -n "$encoded" && "$fsroot" == / ]] || continue
        mounted=$(printf '%b' "$encoded")
        mount --bind "$mounted" "$target" || return 1
        if mount -o remount,bind,ro,nodev,nosuid,noexec "$target"; then
            return 0
        fi
        umount "$target" || return 1
        return 1
    done <<< "$mounted"
    mount -t vfat -o ro,nodev,nosuid,noexec "$source" "$target"
}

backup_systems() {
    local source type id fs vendor dir relative name dest temp bytes checksum
    local saved=0 existing=0 skipped=0
    local root="$stage/EFI/NexusBoot/Backups"
    command -v sha256sum >/dev/null || { echo "sha256sum indisponível." >&2; return 1; }
    command -v findmnt >/dev/null || { echo "findmnt indisponível." >&2; return 1; }
    mkdir -p "$root" || return 1
    while read -r source type id fs; do
        [[ "${type,,}" == c12a7328-f81f-11d2-ba4b-00a0c93ec93b && "$fs" == vfat && "${id,,}" != "${uuid,,}" ]] || continue
        [[ "$id" =~ ^[[:xdigit:]]{8}-[[:xdigit:]]{4}-[[:xdigit:]]{4}-[[:xdigit:]]{4}-[[:xdigit:]]{12}$ && -b "$source" ]] || continue
        source_stage=$(mktemp -d) || return 1
        if ! mount_backup_source "$source" "$source_stage"; then
            if mountpoint -q "$source_stage"; then umount "$source_stage" || return 1; fi
            rmdir "$source_stage" || return 1
            source_stage=
            printf 'Origem ignorada: %s (não foi possível abrir em modo somente leitura).\n' "$source"
            skipped=$((skipped + 1))
            continue
        fi
        for dir in "$source_stage"/EFI/*; do
            [[ -d "$dir" ]] || continue
            vendor=${dir##*/}
            [[ "$vendor" =~ ^[a-zA-Z0-9][a-zA-Z0-9_-]{0,47}$ ]] || continue
            case "${vendor,,}" in boot|nexusboot) continue ;; esac
            if [[ "${vendor,,}" == microsoft ]]; then
                paths=(Boot/bootmgfw.efi Boot/BCD)
            else
                paths=(shimx64.efi grubx64.efi grub.cfg systemd-bootx64.efi liminex64.efi mmx64.efi)
            fi
            dest="$root/${id,,}/$vendor"
            if [[ -f "$dest/ready" ]]; then existing=$((existing + 1)); continue; fi
            [[ ! -e "$dest" ]] || { skipped=$((skipped + 1)); continue; }
            mkdir -p "$root/${id,,}" || return 1
            temp=$(mktemp -d "$root/${id,,}/.capture.XXXXXXXX") || return 1
            local has_loader=0 total=0 complete=1
            for relative in "${paths[@]}"; do
                [[ -f "$dir/$relative" ]] || continue
                name=${relative##*/}
                bytes=$(stat -c %s "$dir/$relative") || { complete=0; break; }
                total=$((total + bytes))
                if (( bytes == 0 || bytes > 33554432 || total > 67108864 )); then complete=0; break; fi
                cp "$dir/$relative" "$temp/$name" || { complete=0; break; }
                checksum=$(sha256sum "$temp/$name") || { complete=0; break; }
                printf '%s\n' "${checksum%% *}" > "$temp/$name.sha256" || { complete=0; break; }
                [[ "$name" != *.efi ]] || has_loader=1
            done
            if (( complete && has_loader )); then
                if touch "$temp/ready" && sync -f "$stage" && mv -T "$temp" "$dest"; then
                    printf '%s: %s copiado para %s.\n' "$source" "$vendor" "$part"
                    saved=$((saved + 1))
                    continue
                fi
                complete=0
            fi
            rm -rf -- "$temp"
            if (( !complete )); then skipped=$((skipped + 1)); fi
        done
        umount "$source_stage" || return 1
        rmdir "$source_stage" || return 1
        source_stage=
    done < <(lsblk -lnpo PATH,PARTTYPE,PARTUUID,FSTYPE)
    sync -f "$stage" || return 1
    printf 'Destino %s: %s novos, %s preservados, %s ignorados.\n' "$part" "$saved" "$existing" "$skipped"
    (( saved + existing > 0 )) || printf 'Nenhum backup de sistema disponível após esta operação.\n'
}
if [[ "$action" == backup ]]; then
    mount -t vfat -o nodev,nosuid,noexec "$part" "$stage"
    [[ -s "$stage/EFI/NexusBoot/NexusBoot.efi" ]] || { echo "Instale o Nexus Boot nesta ESP primeiro." >&2; exit 1; }
    backup_systems
    exit
fi
if [[ "$action" == inspect ]]; then
    mount -t vfat -o ro,nodev,nosuid,noexec "$part" "$stage"
    primary="$stage/EFI/NexusBoot/NexusBoot.efi"
    fallback="$stage/EFI/BOOT/BOOTX64.EFI"
    printf 'ESP: %s (PARTUUID %s)\n' "$part" "$uuid"
    if [[ ! -s "$primary" ]]; then
        printf 'EFI principal: ausente. Use Instalar arquivos EFI nesta ESP.\n'
    elif cmp -s "$primary" -; then
        printf 'EFI principal: idêntico ao seletor incorporado neste instalador.\n'
    else
        printf 'EFI principal: versão anterior ou arquivo diferente. Use Atualizar seletor EFI nesta ESP.\n'
    fi
    if [[ -s "$primary" && -s "$fallback" ]] && cmp -s "$primary" "$fallback"; then
        printf 'EFI de fallback: igual ao principal.\n'
    elif [[ -s "$fallback" ]]; then
        printf 'EFI de fallback: arquivo diferente do principal.\n'
    else
        printf 'EFI de fallback: ausente.\n'
    fi
    umount "$stage"
    entries=$(find_entry)
    entry=${entries%%$'\n'*}
    report=$(efibootmgr)
    current=$(awk '/^BootCurrent:/ {print $2}' <<< "$report")
    next=$(awk '/^BootNext:/ {print $2}' <<< "$report")
    order=$(awk '/^BootOrder:/ {print $2}' <<< "$report")
    printf 'Entrada Nexus: %s\nBootOrder: %s\nBootCurrent: %s\nBootNext: %s\n' "${entry:-ausente}" "${order:-indisponível}" "${current:-indisponível}" "${next:-nenhum}"
    if [[ -n "$entry" && "${order^^}" == "${entry^^}" || -n "$entry" && "${order^^}" == "${entry^^},"* ]]; then
        printf 'Nexus está em primeiro na ordem permanente.\n'
    else
        printf 'Nexus não está em primeiro na ordem permanente.\n'
    fi
    exit
fi
if [[ "$action" == bootnext || "$action" == register || "$action" == primary || "$action" == last ]]; then
    mount -t vfat -o ro,nodev,nosuid,noexec "$part" "$stage"
    [[ -s "$stage/EFI/NexusBoot/NexusBoot.efi" ]] || {
        echo "A partição EFI existe, mas o executável do Nexus Boot não foi instalado nela." >&2; exit 1;
    }
    if [[ "$action" == primary ]] && ! cmp -s "$stage/EFI/NexusBoot/NexusBoot.efi" -; then
        echo "Atualize o seletor EFI nesta partição antes de torná-lo o gerenciador principal." >&2
        exit 1
    fi
    umount "$stage"
    ensure_entry
    if [[ "$action" == register || "$action" == primary || "$action" == last ]]; then
        order=$(efibootmgr | awk '/^BootOrder:/ {print $2}')
        if [[ ! "$order" =~ ^[[:xdigit:]]{4}(,[[:xdigit:]]{4})*$ ]]; then
            echo "O firmware não forneceu uma ordem de boot válida. A ordem permanente foi preservada." >&2
            exit 1
        fi
        expected=$order
        if [[ "$action" == primary || "$action" == last ]]; then
            IFS=, read -r -a ids <<< "$order"
            remaining=()
            for id in "${ids[@]}"; do
                if [[ "${id^^}" != "${entry^^}" ]]; then remaining+=("$id"); fi
            done
            if [[ "$action" == primary ]]; then ids=("$entry" "${remaining[@]}"); else ids=("${remaining[@]}" "$entry"); fi
            expected=$(IFS=,; printf '%s' "${ids[*]}")
        elif [[ ",${order^^}," != *",${entry^^},"* ]]; then
            expected="$order,$entry"
        fi
        if [[ "${order^^}" != "${expected^^}" ]]; then
            umask 077
            install -d -m 700 /var/backups/nexus-boot
            backup=$(mktemp /var/backups/nexus-boot/bootorder.XXXXXXXX)
            printf '%s\n' "$order" > "$backup"
            efibootmgr --bootorder "$expected" >&2
            actual=$(efibootmgr | awk '/^BootOrder:/ {print $2}')
            if [[ "${actual^^}" != "${expected^^}" ]]; then
                echo "O firmware não confirmou a ordem solicitada. Ordem anterior registrada em $backup." >&2
                exit 1
            fi
        fi
        if [[ "$action" == primary ]]; then
            printf 'Nexus Boot é a primeira opção (Boot%s). As outras entradas foram mantidas. Ordem confirmada: %s.\n' "$entry" "$expected"
        elif [[ "$action" == last ]]; then
            printf 'Nexus Boot foi movido para o fim. Ordem confirmada: %s.\n' "$expected"
        else
            printf 'Nexus Boot está no menu permanente como Boot%s.\n' "$entry"
        fi
        exit
    fi
    current=$(efibootmgr | awk '/^BootNext:/ {print $2}')
    if [[ -n "$current" && "${current^^}" != "${entry^^}" ]]; then
        echo "Já há outro boot único agendado ($current). Conclua ou cancele esse agendamento antes de prosseguir." >&2; exit 1
    fi
    efibootmgr --bootnext "$entry" >&2
    current=$(efibootmgr | awk '/^BootNext:/ {print $2}')
    [[ "${current^^}" == "${entry^^}" ]] || { echo "O firmware não confirmou o BootNext." >&2; exit 1; }
    printf 'Teste agendado para Boot%s. Reinicie normalmente para executá-lo uma vez.\n' "$entry"
    exit
fi
mount -t vfat -o nodev,nosuid,noexec "$part" "$stage"
if [[ "$action" == upgrade ]]; then
    primary="$stage/EFI/NexusBoot/NexusBoot.efi"
    fallback="$stage/EFI/BOOT/BOOTX64.EFI"
    [[ -s "$primary" ]] || { echo "Instale o Nexus Boot nesta ESP primeiro." >&2; exit 1; }
    incoming=$(mktemp "$stage/EFI/NexusBoot/update.XXXXXXXX")
    cat > "$incoming"
    [[ "$(stat -c %s "$incoming")" == "$4" ]] || {
        rm -f "$incoming"; echo "O binário EFI chegou incompleto. O executável anterior foi preservado." >&2; exit 1;
    }
    if cmp -s "$incoming" "$primary"; then
        rm -f "$incoming"
    else
        previous=$(mktemp "$stage/EFI/NexusBoot/previous.XXXXXXXX")
        cp "$primary" "$previous"
        if [[ -f "$fallback" ]] && cmp -s "$primary" "$fallback"; then
            temp=$(mktemp "$stage/EFI/BOOT/update.XXXXXXXX")
            cp "$incoming" "$temp"
            mv -f "$temp" "$fallback"
        fi
        sync -f "$stage"
        mv -f "$incoming" "$primary"
    fi
    sync -f "$stage"
    umount "$stage"
    ensure_entry
    printf 'Seletor EFI atualizado. Agende um teste ou defina o Nexus como primeira opção.\n'
    exit
fi
for target in EFI/NexusBoot/NexusBoot.efi EFI/NexusBoot/nexus.toml EFI/BOOT/BOOTX64.EFI; do
    [[ ! -e "$stage/$target" ]] || { echo "Já existe um arquivo em $target." >&2; exit 1; }
done
mkdir -p "$stage/EFI/NexusBoot" "$stage/EFI/BOOT"
head -c "$4" > "$stage/EFI/NexusBoot/NexusBoot.efi"
[[ "$(stat -c %s "$stage/EFI/NexusBoot/NexusBoot.efi")" == "$4" ]] || exit 1
cat > "$stage/EFI/NexusBoot/nexus.toml"
cp "$stage/EFI/NexusBoot/NexusBoot.efi" "$stage/EFI/BOOT/BOOTX64.EFI"
backup_systems || echo "Instalação concluída. Gere o backup pelo painel de Recuperação."
sync -f "$stage"
umount "$stage"
number=$(lsblk -dnro PARTN "$part")
ensure_entry
