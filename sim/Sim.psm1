<#
.SYNOPSIS
    Controls the QEMU simulator for the well pump firmware: images, start/stop and HTTP access.

.DESCRIPTION
    Helper functions used by the simulator tests in sim\tests. Nothing here touches a real
    device: the firmware runs inside Espressif's QEMU with an emulated Ethernet interface whose
    port 80 is forwarded to a port on 127.0.0.1.

    The simulator firmware is the normal firmware built with `--features qemu` (see the `platform`
    module of the firmware crate). It uses its own target directory so the production build is
    never disturbed.
#>

Set-StrictMode -Version Latest

# Repository root (parent of the sim directory).
$script:Root = Split-Path $PSScriptRoot -Parent

# Target directory of the simulator firmware build.
$script:BuildDir = 'C:\bwq'

# Where the ESP-IDF tools (QEMU, toolchain, Python environment) are installed.
$script:Espressif = Join-Path $env:USERPROFILE '.espressif'

function Get-SimTool {
    <#
    .SYNOPSIS
        Locates the external tools the simulator needs.
    .OUTPUTS
        A hashtable with Qemu (qemu-system-xtensa.exe), Objcopy, Python (the ESP-IDF Python
        environment that provides esptool) and DllDir (a directory with libiconv-2.dll, which the
        Windows QEMU build needs but does not ship).
    #>
    $qemu = Get-ChildItem (Join-Path $script:Espressif 'tools\qemu-xtensa') -Recurse -Filter 'qemu-system-xtensa.exe' -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $qemu) { throw 'QEMU not installed: run idf_tools.py install qemu-xtensa' }
    $objcopy = Get-ChildItem (Join-Path $script:Espressif 'tools\xtensa-esp-elf') -Recurse -Filter 'xtensa-esp32-elf-objcopy.exe' | Select-Object -First 1
    $gitRoot = Split-Path (Split-Path (Get-Command git).Source -Parent) -Parent
    $dllDir = @((Join-Path $gitRoot 'mingw64\bin'), 'C:\msys64\mingw64\bin', 'C:\msys64\ucrt64\bin') |
        Where-Object { Test-Path (Join-Path $_ 'libiconv-2.dll') } | Select-Object -First 1
    if (-not $dllDir) { throw 'libiconv-2.dll not found (needed by QEMU); install Git for Windows or MSYS2' }
    @{
        Qemu    = $qemu.FullName
        Objcopy = $objcopy.FullName
        Python  = Join-Path $script:Espressif 'python_env\idf5.5_py3.12_env\Scripts\python.exe'
        DllDir  = $dllDir
    }
}

function Get-SimElf {
    <#
    .SYNOPSIS
        Returns the path of the simulator firmware ELF.
    .OUTPUTS
        The full path; it may not exist yet when the firmware has not been built.
    #>
    Join-Path $script:BuildDir 'xtensa-esp32-espidf\debug\buzsak_well'
}

function Build-SimFirmware {
    <#
    .SYNOPSIS
        Builds the simulator firmware (`--features qemu`) into the simulator target directory.
    .DESCRIPTION
        The first build compiles ESP-IDF and takes several minutes; later builds are incremental.
        Fails when cargo fails.
    #>
    $firmware = Join-Path $script:Root 'firmware'
    $previous = $env:ESP_IDF_SDKCONFIG_DEFAULTS
    Push-Location $firmware
    try {
        . (Join-Path $env:USERPROFILE 'export-esp.ps1')
        $env:ESP_IDF_SDKCONFIG_DEFAULTS = 'sdkconfig.defaults;../sim/sdkconfig.qemu'
        cargo build --features qemu --target-dir $script:BuildDir
        if ($LASTEXITCODE -ne 0) { throw 'simulator firmware build failed' }
    }
    finally {
        $env:ESP_IDF_SDKCONFIG_DEFAULTS = $previous
        Pop-Location
    }
}

function New-SimImage {
    <#
    .SYNOPSIS
        Creates an app image (and optionally a merged 16 MB flash image) from the simulator ELF.
    .DESCRIPTION
        The app image is what an OTA upload sends. The merged flash image (bootloader, partition
        table and app) is what QEMU boots from. With -Version the version in the app descriptor
        is replaced first, which is how test images get distinct versions without a rebuild.
    .PARAMETER Elf
        The firmware ELF to convert.
    .PARAMETER AppOut
        Where to write the app image.
    .PARAMETER FlashOut
        Where to write the merged flash image; omitted means no flash image.
    .PARAMETER Version
        Version text to stamp into the image (shorter than 32 characters), e.g. 0.1.1-test.
    .OUTPUTS
        The SHA-256 of the app image as lowercase hex.
    #>
    param(
        [Parameter(Mandatory)][string]$Elf,
        [Parameter(Mandatory)][string]$AppOut,
        [string]$FlashOut,
        [string]$Version
    )
    $tools = Get-SimTool
    if ($Version) {
        if ($Version.Length -ge 32) { throw 'Version must be shorter than 32 characters' }
        $desc = Join-Path $env:TEMP 'sim-appdesc.bin'
        & $tools.Objcopy --dump-section ".flash.appdesc=$desc" $Elf
        $bytes = [IO.File]::ReadAllBytes($desc)
        $text = [Text.Encoding]::ASCII.GetBytes($Version)
        # esp_app_desc_t: the version is the 32 bytes at offset 16.
        for ($i = 0; $i -lt 32; $i++) { $bytes[16 + $i] = if ($i -lt $text.Length) { $text[$i] } else { 0 } }
        [IO.File]::WriteAllBytes($desc, $bytes)
        $stamped = Join-Path $env:TEMP 'sim-stamped.elf'
        & $tools.Objcopy --update-section ".flash.appdesc=$desc" $Elf $stamped
        if ($LASTEXITCODE -ne 0) { throw 'objcopy failed' }
        $Elf = $stamped
    }

    # Flash parameters match the C++ image header on the device (dio, 80 MHz, 16 MB).
    $null = espflash save-image --chip esp32 --flash-mode dio --flash-freq 80mhz --flash-size 16mb `
        --partition-table (Join-Path $script:Root 'firmware\partitions.csv') --skip-update-check $Elf $AppOut
    if ($LASTEXITCODE -ne 0) { throw 'espflash save-image failed' }

    if ($FlashOut) {
        $build = Get-ChildItem (Join-Path $script:BuildDir 'xtensa-esp32-espidf\debug\build') -Directory -Filter 'esp-idf-sys-*' |
            Where-Object { Test-Path "$($_.FullName)\out\build\bootloader\bootloader.bin" } | Select-Object -First 1
        $null = & $tools.Python -m esptool --chip esp32 merge_bin -o $FlashOut --fill-flash-size 16MB `
            --flash_mode dio --flash_freq 80m --flash_size 16MB `
            0x1000 "$($build.FullName)\out\build\bootloader\bootloader.bin" `
            0x8000 "$($build.FullName)\out\build\partition_table\partition-table.bin" `
            0x10000 $AppOut
        if ($LASTEXITCODE -ne 0) { throw 'esptool merge_bin failed' }
    }
    (Get-FileHash $AppOut -Algorithm SHA256).Hash.ToLower()
}

function Start-Sim {
    <#
    .SYNOPSIS
        Starts QEMU in the background with a flash image and a forwarded HTTP port.
    .PARAMETER Flash
        The merged 16 MB flash image to boot. QEMU writes to it, so pass a copy.
    .PARAMETER HostPort
        Port on 127.0.0.1 that is forwarded to port 80 of the simulated device.
    .PARAMETER LogPath
        File that receives the serial console output.
    .OUTPUTS
        A simulator handle (Process, Log, HostPort, Flash) for the other Sim functions.
    #>
    param(
        [Parameter(Mandatory)][string]$Flash,
        [int]$HostPort = 18080,
        [Parameter(Mandatory)][string]$LogPath
    )
    $tools = Get-SimTool
    Remove-Item $LogPath, "$LogPath.err" -ErrorAction SilentlyContinue
    $arguments = @(
        '-nographic', '-machine', 'esp32',
        '-drive', "file=$Flash,if=mtd,format=raw",
        # QEMU's timing makes the emulated task watchdog unreliable.
        '-global', 'driver=timer.esp32.timg,property=wdt_disable,value=true',
        '-nic', "user,model=open_eth,hostfwd=tcp:127.0.0.1:${HostPort}-:80"
    )
    $previousPath = $env:PATH
    $env:PATH = "$($tools.DllDir);$env:PATH"
    try {
        $process = Start-Process -FilePath $tools.Qemu -ArgumentList $arguments -PassThru -WindowStyle Hidden `
            -RedirectStandardOutput $LogPath -RedirectStandardError "$LogPath.err"
    }
    finally { $env:PATH = $previousPath }
    [pscustomobject]@{ Process = $process; Log = $LogPath; HostPort = $HostPort; Flash = $Flash }
}

function Restart-Sim {
    <#
    .SYNOPSIS
        Power-cycles the simulated device on the same flash image.
    .DESCRIPTION
        QEMU does not reliably emulate the firmware's own software reset (the rebooted firmware
        crashes during early init), so after an update the harness stops QEMU and boots it again.
        The flash content (otadata, NVS, both slots) is kept, so the bootloader still picks the
        slot and applies its rollback rules. The previous serial log is kept as `<log>.previous`.
        The handle is updated in place.
    .PARAMETER Sim
        The handle returned by Start-Sim.
    #>
    param([Parameter(Mandatory)]$Sim)
    Stop-Sim -Sim $Sim
    Move-Item $Sim.Log "$($Sim.Log).previous" -Force
    $next = Start-Sim -Flash $Sim.Flash -HostPort $Sim.HostPort -LogPath $Sim.Log
    $Sim.Process = $next.Process
}

function Stop-Sim {
    <#
    .SYNOPSIS
        Stops the simulator process; safe to call when it already exited.
    .PARAMETER Sim
        The handle returned by Start-Sim.
    #>
    param([Parameter(Mandatory)]$Sim)
    if ($Sim -and -not $Sim.Process.HasExited) {
        Stop-Process -Id $Sim.Process.Id -Force -ErrorAction SilentlyContinue
        $null = $Sim.Process.WaitForExit(5000)
    }
}

function Wait-SimLog {
    <#
    .SYNOPSIS
        Waits until the serial log contains a pattern.
    .PARAMETER Sim
        The handle returned by Start-Sim.
    .PARAMETER Pattern
        Regular expression to look for.
    .PARAMETER TimeoutSec
        How long to wait.
    .OUTPUTS
        $true when the pattern appeared, $false on timeout or when QEMU exited first.
    #>
    param(
        [Parameter(Mandatory)]$Sim,
        [Parameter(Mandatory)][string]$Pattern,
        [int]$TimeoutSec = 90
    )
    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    while ((Get-Date) -lt $deadline) {
        if ((Test-Path $Sim.Log) -and (Select-String -Path $Sim.Log -Pattern $Pattern -Quiet)) { return $true }
        if ($Sim.Process.HasExited) { return $false }
        Start-Sleep -Milliseconds 500
    }
    $false
}

function Get-SimCredentials {
    <#
    .SYNOPSIS
        Reads the OTA user and password from firmware\src\secrets.rs (never printed).
    .OUTPUTS
        A Basic `Authorization` header value.
    #>
    $secrets = Get-Content (Join-Path $script:Root 'firmware\src\secrets.rs') -Raw
    $user = [regex]::Match($secrets, 'OTA_USER: &str = "([^"]*)"').Groups[1].Value
    $password = [regex]::Match($secrets, 'OTA_PASSWORD: &str = "([^"]*)"').Groups[1].Value
    'Basic ' + [Convert]::ToBase64String([Text.Encoding]::ASCII.GetBytes("${user}:$password"))
}

function Invoke-SimHttp {
    <#
    .SYNOPSIS
        Sends an HTTP request to the simulated device and returns the status without throwing.
    .PARAMETER Sim
        The handle returned by Start-Sim.
    .PARAMETER Method
        HTTP method, e.g. GET or POST.
    .PARAMETER Path
        Request path, e.g. /ota/status.
    .PARAMETER Body
        Request body bytes; omitted means no body.
    .PARAMETER Headers
        Extra request headers.
    .PARAMETER Authorization
        Overrides the Authorization header value; by default the OTA credentials are used.
    .PARAMETER NoAuth
        Sends no Authorization header at all.
    .PARAMETER TimeoutSec
        Request timeout.
    .OUTPUTS
        An object with Status (HTTP status, or 0 when the connection failed) and Body (text).
    #>
    param(
        [Parameter(Mandatory)]$Sim,
        [string]$Method = 'GET',
        [Parameter(Mandatory)][string]$Path,
        [byte[]]$Body,
        [hashtable]$Headers = @{},
        [string]$Authorization,
        [switch]$NoAuth,
        [int]$TimeoutSec = 30
    )
    Add-Type -AssemblyName System.Net.Http
    $client = [Net.Http.HttpClient]::new()
    $client.Timeout = [TimeSpan]::FromSeconds($TimeoutSec)
    try {
        $request = [Net.Http.HttpRequestMessage]::new([Net.Http.HttpMethod]::new($Method), "http://127.0.0.1:$($Sim.HostPort)$Path")
        if (-not $NoAuth) {
            if (-not $Authorization) { $Authorization = Get-SimCredentials }
            $null = $request.Headers.TryAddWithoutValidation('Authorization', $Authorization)
        }
        foreach ($name in $Headers.Keys) { $null = $request.Headers.TryAddWithoutValidation($name, [string]$Headers[$name]) }
        if ($null -ne $Body) { $request.Content = [Net.Http.ByteArrayContent]::new($Body) }
        $response = $client.SendAsync($request).GetAwaiter().GetResult()
        [pscustomobject]@{ Status = [int]$response.StatusCode; Body = $response.Content.ReadAsStringAsync().GetAwaiter().GetResult() }
    }
    catch { [pscustomobject]@{ Status = 0; Body = $_.Exception.Message } }
    finally { $client.Dispose() }
}

function Get-SimStatus {
    <#
    .SYNOPSIS
        Reads GET /ota/status from the simulated device.
    .PARAMETER Sim
        The handle returned by Start-Sim.
    .OUTPUTS
        The parsed status (project, version, slot, state), or $null when the device does not answer.
    #>
    param([Parameter(Mandatory)]$Sim)
    $response = Invoke-SimHttp -Sim $Sim -Path '/ota/status' -TimeoutSec 5
    if ($response.Status -ne 200) { return $null }
    $response.Body | ConvertFrom-Json
}

function Wait-SimStatus {
    <#
    .SYNOPSIS
        Waits until the device reports a given version, slot and state (it reboots in between).
    .PARAMETER Sim
        The handle returned by Start-Sim.
    .PARAMETER Version
        Expected running version.
    .PARAMETER Slot
        Expected running slot label, e.g. app1.
    .PARAMETER State
        Expected slot state, e.g. valid.
    .PARAMETER TimeoutSec
        How long to wait.
    .OUTPUTS
        The matching status, or $null on timeout.
    #>
    param(
        [Parameter(Mandatory)]$Sim,
        [Parameter(Mandatory)][string]$Version,
        [Parameter(Mandatory)][string]$Slot,
        [string]$State = 'valid',
        [int]$TimeoutSec = 150
    )
    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    while ((Get-Date) -lt $deadline) {
        $status = Get-SimStatus -Sim $Sim
        if ($status -and $status.version -eq $Version -and $status.slot -eq $Slot -and $status.state -eq $State) { return $status }
        Start-Sleep -Seconds 2
    }
    $null
}

Export-ModuleMember -Function Get-SimTool, Get-SimElf, Build-SimFirmware, New-SimImage, Start-Sim, Restart-Sim, Stop-Sim, Wait-SimLog, Invoke-SimHttp, Get-SimStatus, Wait-SimStatus
