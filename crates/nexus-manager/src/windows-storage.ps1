param([string]$operation, [string]$uuid, [UInt64]$previousSize, [string]$value)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
try {
    $principal = [Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())
    if (!$principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw 'Abra o Nexus Boot como administrador para alterar uma partição.'
    }

    $identity = [guid]::Parse($uuid)
    $parts = @(Get-Partition | Where-Object { $_.Guid -and ([guid]$_.Guid -eq $identity) })
    if ($parts.Count -ne 1 -or $parts[0].Size -ne $previousSize) {
        throw 'A identificação ou o tamanho da partição mudou. Atualize o inventário.'
    }

    $part = $parts[0]
    $disk = Get-Disk -Number $part.DiskNumber
    if ($disk.PartitionStyle -ne 'GPT' -or $disk.IsReadOnly -or $disk.IsOffline -or
        $part.IsReadOnly -or $part.IsOffline -or $part.IsSystem -or $part.IsBoot -or
        ([guid]$part.GptType -ne [guid]'ebd0a0a2-b9e5-4433-87c0-68b6b72699c7')) {
        throw 'Por segurança, esta operação exige uma partição de dados GPT que não esteja em uso como partição do sistema.'
    }

    $volume = Get-Volume -Partition $part
    if ($volume.FileSystem -ne 'NTFS') { throw 'Esta operação no Windows exige um volume NTFS.' }
    if ($operation -eq 'rename') {
        if ($value -notmatch '^[a-zA-Z0-9_. -]{1,32}$') { throw 'Rótulo inválido.' }
        Set-Volume -InputObject $volume -NewFileSystemLabel $value
        if ((Get-Volume -Partition $part).FileSystemLabel -ne $value) { throw 'O Windows não confirmou o rótulo.' }
        Write-Output 'Rótulo do volume NTFS atualizado.'
    } elseif ($operation -eq 'resize') {
        $size = [uint64]::Parse($value) * 1MB
        $limits = Get-PartitionSupportedSize -DiskNumber $part.DiskNumber -PartitionNumber $part.PartitionNumber
        if ($size -lt $limits.SizeMin -or $size -gt $limits.SizeMax) {
            throw "O Windows permite tamanhos entre $([math]::Ceiling($limits.SizeMin / 1MB)) e $([math]::Floor($limits.SizeMax / 1MB)) MiB para este volume."
        }
        Resize-Partition -DiskNumber $part.DiskNumber -PartitionNumber $part.PartitionNumber -Size $size
        if ((Get-Partition -DiskNumber $part.DiskNumber -PartitionNumber $part.PartitionNumber).Size -ne $size) {
            throw 'O Windows não confirmou o tamanho solicitado. Atualize o inventário antes de tentar novamente.'
        }
        Write-Output 'Partição e volume NTFS redimensionados.'
    } else { throw 'Operação desconhecida.' }
} catch {
    [Console]::Error.WriteLine($_.Exception.Message)
    exit 1
}
