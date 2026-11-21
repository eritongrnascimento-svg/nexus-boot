param(
    [Parameter(Mandatory)]
    [string]$OvmfCode,

    [Parameter(Mandatory)]
    [string]$OvmfVars,

    [string]$Qemu = 'qemu-system-x86_64'
)

$ErrorActionPreference = 'Stop'
$project = Split-Path -Parent $PSScriptRoot
$binary = Join-Path $project 'target/x86_64-unknown-uefi/release/nexus-efi.efi'
$binary = (Resolve-Path -LiteralPath $binary).Path
$OvmfCode = (Resolve-Path -LiteralPath $OvmfCode).Path
$OvmfVars = (Resolve-Path -LiteralPath $OvmfVars).Path
$null = Get-Command $Qemu -CommandType Application

$run = Join-Path $project ("target/qemu-" + [Guid]::NewGuid().ToString('N'))
$esp = Join-Path $run 'esp'
$boot = Join-Path $esp 'EFI/BOOT'
$varsCopy = Join-Path $run 'OVMF_VARS.fd'
$null = New-Item -ItemType Directory -Path $boot -Force
Copy-Item -LiteralPath $binary -Destination (Join-Path $boot 'BOOTX64.EFI')
Copy-Item -LiteralPath $OvmfVars -Destination $varsCopy
(Get-Item -LiteralPath $varsCopy).IsReadOnly = $false

function Escape-QemuDriveValue([string]$path) {
    $path.Replace('\', '/').Replace(',', ',,')
}

$codeArg = Escape-QemuDriveValue $OvmfCode
$varsArg = Escape-QemuDriveValue $varsCopy
$espArg = Escape-QemuDriveValue $esp
& $Qemu -machine q35,accel=tcg -m 512 -nic none `
    -drive "if=pflash,format=raw,unit=0,readonly=on,file=$codeArg" `
    -drive "if=pflash,format=raw,unit=1,file=$varsArg" `
    -drive "format=raw,file=fat:rw:$espArg"
if ($LASTEXITCODE -ne 0) {
    throw "QEMU terminou com código $LASTEXITCODE."
}
