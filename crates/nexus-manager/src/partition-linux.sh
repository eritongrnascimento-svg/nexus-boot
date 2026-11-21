set -euo pipefail
export LC_ALL=C PATH=/usr/sbin:/usr/bin:/sbin:/bin
fail() { echo "$*" >&2; exit 1; }
action=$1 disk=$2 part=$3 identity=$4 old=$5 value=$6
[[ "$disk" == /dev/* && "$part" == /dev/* && -b "$disk" && -b "$part" ]] || fail "Dispositivo indisponível."
[[ "$(lsblk -dnro PTTYPE "$disk")" == gpt ]] || fail "O disco precisa usar GPT."
[[ "$(lsblk -dnro TYPE "$part")" == part && "/dev/$(lsblk -dnro PKNAME "$part")" == "$disk" ]] || fail "A partição não pertence ao disco selecionado."
[[ "$old" =~ ^[0-9]+$ && "$(blockdev --getsize64 "$part")" == "$old" ]] || fail "O tamanho da partição mudou. Atualize o inventário."
[[ "$(blockdev --getro "$disk")" == 0 && "$(blockdev --getro "$part")" == 0 ]] || fail "Dispositivo somente leitura."
number=$(lsblk -dnro PARTN "$part")
[[ "$number" =~ ^[0-9]+$ ]] || fail "Número da partição indisponível."
current_uuid=$(sfdisk --part-uuid "$disk" "$number")
[[ -n "$identity" && "${current_uuid,,}" == "${identity,,}" ]] ||
    fail "O identificador da partição mudou. Atualize o inventário."

require_idle() {
    [[ -z "$(lsblk -nr -o MOUNTPOINTS "$part")" ]] || fail "Desmonte a partição antes de continuar."
    local sys
    sys=$(lsblk -dnro KNAME "$part")
    [[ -z "$(ls -A "/sys/class/block/$sys/holders")" ]] || fail "A partição está em uso por outro dispositivo."
    local major_minor
    major_minor=$(lsblk -dnro MAJ:MIN "$part")
    if awk -v device="$major_minor" '$3 == device {found=1} END {exit !found}' /proc/[0-9]*/mountinfo 2>/dev/null; then
        fail "A partição está montada em outro namespace."
    fi
}
require_idle

case "$action" in
    rename)
        [[ "$value" =~ ^[A-Za-z0-9_.\ -]{1,36}$ ]] || fail "Use de 1 a 36 letras sem acento, números, espaços, pontos, traços ou sublinhados."
        ;;
    resize)
        [[ "$value" =~ ^[0-9]{1,9}$ ]] && (( value >= 64 )) || fail "Informe um tamanho a partir de 64 MiB."
        [[ "$(blkid -p -s TYPE -o value "$part")" == ext4 ]] || fail "O redimensionamento exige uma partição ext4."
        for tool in e2fsck resize2fs dumpe2fs partx awk udevadm; do
            command -v "$tool" >/dev/null || fail "Dependência ausente: $tool"
        done
        new=$((value * 1048576))
        (( new != old )) || fail "Escolha um tamanho diferente."
        sector=$(blockdev --getss "$disk")
        (( new % sector == 0 )) || fail "O tamanho não respeita os setores do disco."
        start=$(cat "/sys/class/block/$(lsblk -dnro KNAME "$part")/start")
        (( start * 512 % sector == 0 )) || fail "O início não respeita os setores do disco."
        specification="start=$((start * 512 / sector)), size=$((new / sector))"
        write=(sfdisk --lock=yes --no-reread --no-tell-kernel --wipe never --wipe-partitions never -N "$number" "$disk")
        printf '%s\n' "$specification" | "${write[@]}" --no-act >/dev/null
        e2fsck -fn "$part" >&2 || fail "A verificação encontrou erros. Corrija o volume antes de redimensionar."
        ;;
    *) fail "Operação desconhecida." ;;
esac

umask 077
install -d -m 700 /var/backups/nexus-boot
backup=$(mktemp /var/backups/nexus-boot/gpt.XXXXXXXX)
sfdisk --dump "$disk" > "$backup"
echo "Backup da tabela GPT: $backup" >&2
require_idle

if [[ "$action" == rename ]]; then
    sfdisk --lock=yes --part-label "$disk" "$number" "$value"
    printf 'Nome GPT atualizado. Backup: %s\n' "$backup"
    exit
fi

if (( new < old )); then
    resize2fs "$part" "${value}M" >&2
    fs=$(dumpe2fs -h "$part" 2>/dev/null)
    blocks=$(awk -F: '$1 == "Block count" {print $2+0}' <<< "$fs")
    blocksize=$(awk -F: '$1 == "Block size" {print $2+0}' <<< "$fs")
    [[ "$blocks" =~ ^[0-9]+$ && "$blocksize" =~ ^[0-9]+$ ]] || fail "Não foi possível verificar o novo tamanho do ext4. A tabela GPT não foi alterada."
    (( blocks > 0 && blocksize > 0 && blocks * blocksize <= new )) || fail "O ext4 ainda é maior que a partição planejada."
fi

require_idle
printf '%s\n' "$specification" | "${write[@]}" >&2
partx --update --nr "$number" "$disk" || fail "A tabela GPT foi atualizada, mas o kernel recusou o novo tamanho. Reinicie e verifique a partição antes de tentar novamente."
[[ "$(blockdev --getsize64 "$part")" == "$new" ]] || fail "O kernel não publicou o novo tamanho. Reinicie antes de continuar."
if (( new > old )); then
    resize2fs "$part" >&2 || fail "A partição cresceu, mas o ext4 ainda precisa ser expandido. O backup da tabela foi preservado."
fi
udevadm trigger --action=change --subsystem-match=block --sysname-match="$(lsblk -dnro KNAME "$part")" || true
udevadm settle || true
printf 'Partição e ext4 redimensionados. Backup: %s\n' "$backup"
