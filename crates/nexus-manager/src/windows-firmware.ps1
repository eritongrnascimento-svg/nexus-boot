param([string]$previous, [string]$requested)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
try {
    $principal = [Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())
    if (!$principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw 'Abra o Nexus Boot como administrador para consultar ou editar a ordem UEFI.'
    }

    $managerId = '{a5a30fa2-3d06-4e9f-b5f4-a01df9d1fcba}'
    function Open-Store {
        $result = ([wmiclass]'root\WMI:BcdStore').OpenStore('')
        if (!$result.ReturnValue) { throw 'O Windows não abriu o armazenamento BCD.' }
        $result.Store
    }
    function Read-Order($store) {
        $object = $store.OpenObject($managerId)
        if (!$object.ReturnValue) { throw 'O gerenciador de firmware UEFI não está disponível.' }
        $element = $object.Object.GetElement([uint32]0x24000001)
        if (!$element.ReturnValue -or !$element.Element.Ids) { throw 'O firmware não informou a lista de inicialização.' }
        $element.Element.Ids
    }

    $store = Open-Store
    $ids = @(Read-Order $store)
    if ($requested) {
        $before = @($previous.Split(','))
        $after = @($requested.Split(','))
        if (($before -join ',') -ne ($ids -join ',')) {
            throw 'A ordem mudou desde a consulta. Atualize a lista e tente novamente.'
        }
        if ($after.Count -ne $ids.Count -or ($after | Sort-Object -Unique).Count -ne $ids.Count -or
            (Compare-Object -ReferenceObject $ids -DifferenceObject $after)) {
            throw 'A nova ordem deve conter exatamente as entradas atuais.'
        }
        foreach ($id in $after) { $null = [guid]::Parse($id) }
        $folder = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'nexus-boot\firmware-backups'
        New-Item -ItemType Directory -Path $folder -Force | Out-Null
        $backup = Join-Path $folder ("bootorder-" + [guid]::NewGuid().ToString('N') + ".txt")
        [IO.File]::WriteAllLines($backup, [string[]]$ids)
        $output = & "$env:SystemRoot\System32\bcdedit.exe" /set $managerId displayorder @after
        if ($LASTEXITCODE -ne 0) { throw "O Windows recusou a gravação. Backup: $backup. $output" }
        $store = Open-Store
        $ids = @(Read-Order $store)
        if (($ids -join ',') -ne ($after -join ',')) { throw "O firmware não confirmou a nova ordem. Backup: $backup." }
    }

    $entries = @(
        foreach ($id in $ids) {
            $entry = $store.OpenObject($id)
            $name = if ($entry.ReturnValue) {
                $description = $entry.Object.GetElement([uint32]0x12000004)
                if ($description.ReturnValue) { $description.Element.String }
            }
            @{ id = $id; name = if ($name) { $name } else { $id } }
        }
    )
    ConvertTo-Json -InputObject @{ entries = $entries; backup = $backup } -Depth 4 -Compress
} catch {
    [Console]::Error.WriteLine($_.Exception.Message)
    exit 1
}
