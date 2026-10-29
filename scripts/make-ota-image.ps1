# Builds the release firmware and writes the OTA image plus its SHA-256. Offline: never touches a device.
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot -Parent)
. "$env:USERPROFILE\export-esp.ps1"

cargo build --release
if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }

$elf = 'C:\bw\xtensa-esp32-espidf\release\buzsak_well'
$out = 'C:\bw\ota'
$bin = Join-Path $out 'buzsak_well.bin'
New-Item -ItemType Directory -Force $out | Out-Null

# Flash parameters must match the C++ image header on the device (dio, 80 MHz, 16 MB).
espflash save-image --chip esp32 --flash-mode dio --flash-freq 80mhz --flash-size 16mb `
    --partition-table partitions.csv --skip-update-check $elf $bin
if ($LASTEXITCODE -ne 0) { throw 'espflash save-image failed' }

$slot = 0x640000
$size = (Get-Item $bin).Length
if ($size -gt $slot) { throw "image is $size bytes, slot is $slot" }

$sha = (Get-FileHash $bin -Algorithm SHA256).Hash.ToLower()
Set-Content -NoNewline -Path "$bin.sha256" -Value $sha
"{0}  {1} bytes ({2:P1} of slot)  sha256 {3}" -f $bin, $size, ($size / $slot), $sha
