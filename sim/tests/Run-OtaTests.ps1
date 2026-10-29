#requires -Version 7.0
<#
.SYNOPSIS
    OTA tests that run the real firmware in the QEMU simulator (no fault injection, no hardware).

.DESCRIPTION
    Seven test cases run in order against ONE simulated device, so each case starts from the
    state the previous one left:

      1. Boot            - the firmware boots, the network comes up and the status endpoint answers.
      2. Authentication  - the OTA routes refuse missing and wrong credentials.
      3. Bad SHA-256     - an upload with a wrong hash is refused and the device is unchanged.
      4. Not newer       - an upload of the running version is refused.
      5. Not an image    - an upload of garbage is refused.
      6. Update          - a newer image is accepted, activated, booted from the other slot
                           after a simulated power cycle (QEMU cannot emulate the software reset)
                           and confirmed after the soak.
      7. Second update   - the confirmed image can be updated again, back to the first slot.

    The simulator is always stopped at the end, also when a case fails or throws.
    The exit code is the number of failed cases.

.PARAMETER Build
    Build the simulator firmware first (several minutes the first time).

.PARAMETER HostPort
    Port on 127.0.0.1 that is forwarded to the simulated device's port 80.

.PARAMETER WorkDir
    Directory for the generated images and the serial log.

.EXAMPLE
    powershell -File sim\tests\Run-OtaTests.ps1 -Build
#>
param(
    [switch]$Build,
    [int]$HostPort = 18080,
    [string]$WorkDir = 'C:\bwq\sim'
)
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot '..\Sim.psm1') -Force -DisableNameChecking

# Results of all cases: Name, Passed, Detail.
$results = [System.Collections.Generic.List[object]]::new()

function Assert-That {
    <#
    .SYNOPSIS
        Fails the current test case when a condition is false.
    .PARAMETER Condition
        The condition that must hold.
    .PARAMETER Message
        What was expected; shown in the failure detail.
    #>
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) { throw "expected: $Message" }
}

function Test-Case {
    <#
    .SYNOPSIS
        Runs one test case and records PASS or FAIL; an exception is a failure.
    .PARAMETER Name
        The case name shown in the report.
    .PARAMETER Body
        The script block with the assertions.
    #>
    param([string]$Name, [scriptblock]$Body)
    try {
        & $Body
        $results.Add([pscustomobject]@{ Name = $Name; Passed = $true; Detail = '' })
        Write-Host "PASS  $Name"
    }
    catch {
        $results.Add([pscustomobject]@{ Name = $Name; Passed = $false; Detail = $_.Exception.Message })
        Write-Host "FAIL  $Name -- $($_.Exception.Message)"
    }
}

function Send-Update {
    <#
    .SYNOPSIS
        Uploads an image to the simulated device.
    .PARAMETER Path
        The image file to send.
    .PARAMETER Sha256
        The SHA-256 to announce in the X-Sha256 header.
    .OUTPUTS
        The HTTP result (Status, Body).
    #>
    param([string]$Path, [string]$Sha256)
    Invoke-SimHttp -Sim $sim -Method POST -Path '/ota' -Body ([IO.File]::ReadAllBytes($Path)) `
        -Headers @{ 'X-Sha256' = $Sha256; 'Content-Type' = 'application/octet-stream' } -TimeoutSec 120
}

function Assert-Unchanged {
    <#
    .SYNOPSIS
        Asserts that the device still runs 0.1.0 from app0 and is valid (a refused upload changed nothing).
    #>
    $status = Get-SimStatus -Sim $sim
    Assert-That ($null -ne $status) 'the device still answers'
    Assert-That ($status.version -eq '0.1.0' -and $status.slot -eq 'app0' -and $status.state -eq 'valid') "still 0.1.0 / app0 / valid, got $($status.version) / $($status.slot) / $($status.state)"
}

New-Item -ItemType Directory -Force $WorkDir | Out-Null
if ($Build) { Build-SimFirmware }
$elf = Get-SimElf
if (-not (Test-Path $elf)) { throw "Simulator firmware not built: $elf (run with -Build)" }

# Images: the base flash image to boot, and test images with newer versions to upload.
$baseApp = Join-Path $WorkDir 'base-app.bin'
$baseFlash = Join-Path $WorkDir 'base-flash.bin'
$baseSha = New-SimImage -Elf $elf -AppOut $baseApp -FlashOut $baseFlash
$update1 = Join-Path $WorkDir 'update-0.1.1-test.bin'
$update1Sha = New-SimImage -Elf $elf -AppOut $update1 -Version '0.1.1-test'
$update2 = Join-Path $WorkDir 'update-0.1.2-test.bin'
$update2Sha = New-SimImage -Elf $elf -AppOut $update2 -Version '0.1.2-test'
$garbage = Join-Path $WorkDir 'garbage.bin'
$garbageBytes = [byte[]](,[byte]0x55 * 4096)
[IO.File]::WriteAllBytes($garbage, $garbageBytes)
$garbageSha = (Get-FileHash $garbage -Algorithm SHA256).Hash.ToLower()

# A fresh copy per run: QEMU writes to the flash image it boots.
$runFlash = Join-Path $WorkDir 'run-flash.bin'
Copy-Item $baseFlash $runFlash -Force

$sim = $null
try {
    $sim = Start-Sim -Flash $runFlash -HostPort $HostPort -LogPath (Join-Path $WorkDir 'serial.log')

    Test-Case '1. Boot: network up and status answers' {
        Assert-That (Wait-SimLog -Sim $sim -Pattern 'network up' -TimeoutSec 90) 'the log reports "network up" within 90 s'
        $status = Get-SimStatus -Sim $sim
        Assert-That ($null -ne $status) 'GET /ota/status answers with valid credentials'
        Assert-That ($status.project -eq 'buzsak_well') "project buzsak_well, got $($status.project)"
        Assert-That ($status.version -eq '0.1.0') "version 0.1.0, got $($status.version)"
        Assert-That ($status.slot -eq 'app0') "slot app0, got $($status.slot)"
        Assert-That ($status.state -eq 'valid') "state valid, got $($status.state)"
    }

    Test-Case '2. Authentication: missing and wrong credentials are refused' {
        $none = Invoke-SimHttp -Sim $sim -Path '/ota/status' -NoAuth
        Assert-That ($none.Status -eq 401) "GET without credentials gives 401, got $($none.Status)"
        $wrong = Invoke-SimHttp -Sim $sim -Path '/ota/status' -Authorization 'Basic d3Jvbmc6d3Jvbmc='
        Assert-That ($wrong.Status -eq 401) "GET with wrong credentials gives 401, got $($wrong.Status)"
        $post = Invoke-SimHttp -Sim $sim -Method POST -Path '/ota' -NoAuth -Body $garbageBytes
        Assert-That ($post.Status -eq 401) "POST without credentials gives 401, got $($post.Status)"
        Assert-Unchanged
    }

    Test-Case '3. Wrong SHA-256: refused, device unchanged' {
        $result = Send-Update -Path $update1 -Sha256 ('0' * 64)
        Assert-That ($result.Status -eq 422) "status 422, got $($result.Status)"
        Assert-That ($result.Body -match 'SHA-256 mismatch') "reason SHA-256 mismatch, got $($result.Body)"
        Assert-Unchanged
    }

    Test-Case '4. Not newer: the running version is refused' {
        $result = Send-Update -Path $baseApp -Sha256 $baseSha
        Assert-That ($result.Status -eq 422) "status 422, got $($result.Status)"
        Assert-That ($result.Body -match 'newer') "reason mentions the version, got $($result.Body)"
        Assert-Unchanged
    }

    Test-Case '5. Not an image: garbage is refused' {
        $result = Send-Update -Path $garbage -Sha256 $garbageSha
        Assert-That ($result.Status -eq 422) "status 422, got $($result.Status)"
        Assert-That ($result.Body -match 'not an ESP image') "reason not an ESP image, got $($result.Body)"
        Assert-Unchanged
    }

    Test-Case '6. Update: accepted, booted from the other slot, confirmed' {
        $result = Send-Update -Path $update1 -Sha256 $update1Sha
        Assert-That ($result.Status -eq 200) "status 200, got $($result.Status): $($result.Body)"
        Assert-That (Wait-SimLog -Sim $sim -Pattern 'OTA activated' -TimeoutSec 30) 'the log reports "OTA activated"'
        Restart-Sim -Sim $sim
        $status = Wait-SimStatus -Sim $sim -Version '0.1.1-test' -Slot 'app1' -State 'valid'
        Assert-That ($null -ne $status) 'the device comes back as 0.1.1-test / app1 / valid within 150 s'
        Assert-That (Wait-SimLog -Sim $sim -Pattern 'new image confirmed' -TimeoutSec 5) 'the log reports "new image confirmed"'
    }

    Test-Case '7. Second update: the confirmed image can be updated again' {
        $result = Send-Update -Path $update2 -Sha256 $update2Sha
        Assert-That ($result.Status -eq 200) "status 200, got $($result.Status): $($result.Body)"
        Assert-That (Wait-SimLog -Sim $sim -Pattern 'OTA activated' -TimeoutSec 30) 'the log reports "OTA activated"'
        Restart-Sim -Sim $sim
        $status = Wait-SimStatus -Sim $sim -Version '0.1.2-test' -Slot 'app0' -State 'valid'
        Assert-That ($null -ne $status) 'the device comes back as 0.1.2-test / app0 / valid within 150 s'
    }
}
finally {
    if ($sim) { Stop-Sim -Sim $sim }
}

$failed = @($results | Where-Object { -not $_.Passed }).Count
Write-Host ''
Write-Host ("{0} of {1} cases passed. Serial log: {2}" -f ($results.Count - $failed), $results.Count, (Join-Path $WorkDir 'serial.log'))
exit $failed
