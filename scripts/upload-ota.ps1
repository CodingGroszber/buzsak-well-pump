<#
.SYNOPSIS
    Uploads an OTA image over the LAN and waits until the new image has confirmed itself.

.DESCRIPTION
    Never run by the build or the tests; it talks to a real device. Before uploading it checks that
    the device runs the same project, is confirmed (state valid) and that the image version is newer.
    After the upload it polls /ota/status until the new version reports state valid, and fails when
    the device falls back to the old version (rollback) or the timeout expires.

.PARAMETER Device
    Host name or IP address of the device.

.PARAMETER Image
    The OTA image to upload (made by make-ota-image.ps1).

.PARAMETER User
    OTA user name; defaults to the OTA_USER environment variable.

.PARAMETER Password
    OTA password; defaults to the OTA_PASSWORD environment variable.

.PARAMETER TimeoutSeconds
    How long to wait for the new image to confirm after the upload.

.EXAMPLE
    powershell -File scripts\upload-ota.ps1 -Device 192.168.1.50
#>
param(
    [Parameter(Mandatory)][string]$Device,
    [string]$Image = 'C:\bw\ota\buzsak_well.bin',
    [string]$User = $env:OTA_USER,
    [string]$Password = $env:OTA_PASSWORD,
    [int]$TimeoutSeconds = 900
)
$ErrorActionPreference = 'Stop'
if (-not $User -or -not $Password) { throw 'Set OTA_USER and OTA_PASSWORD or pass -User and -Password' }

$bytes = [IO.File]::ReadAllBytes($Image)
function Get-ImageField([int]$offset) {
    [Text.Encoding]::ASCII.GetString($bytes[$offset..($offset + 31)]).TrimEnd([char]0)
}
# App descriptor: version at 48, project name at 80 (after the 24-byte header and 8-byte segment header).
$newVersion = Get-ImageField 48
$project = Get-ImageField 80
$sha = (Get-FileHash $Image -Algorithm SHA256).Hash.ToLower()

$auth = 'Basic ' + [Convert]::ToBase64String([Text.Encoding]::ASCII.GetBytes("${User}:$Password"))
$base = "http://$Device"
function Get-OtaStatus {
    Invoke-RestMethod -Uri "$base/ota/status" -Headers @{ Authorization = $auth } -TimeoutSec 5
}

$before = Get-OtaStatus
"Device: project $($before.project), version $($before.version), slot $($before.slot), state $($before.state)"
if ($before.project -ne $project) { throw "Image is for '$project', device runs '$($before.project)'" }
if ($before.state -ne 'valid') { throw "Running image is '$($before.state)', not confirmed yet" }
if ([version]$newVersion -le [version]$before.version) { throw "Image $newVersion is not newer than $($before.version)" }

"Uploading $Image ($($bytes.Length) bytes, version $newVersion, sha256 $sha)"
Add-Type -AssemblyName System.Net.Http
$client = [Net.Http.HttpClient]::new()
$client.Timeout = [TimeSpan]::FromMinutes(5)
$content = [Net.Http.ByteArrayContent]::new($bytes)
$content.Headers.ContentType = [Net.Http.Headers.MediaTypeHeaderValue]::Parse('application/octet-stream')
$request = [Net.Http.HttpRequestMessage]::new([Net.Http.HttpMethod]::Post, "$base/ota")
$request.Content = $content
$null = $request.Headers.TryAddWithoutValidation('Authorization', $auth)
$null = $request.Headers.TryAddWithoutValidation('X-Sha256', $sha)
$response = $client.SendAsync($request).GetAwaiter().GetResult()
$body = $response.Content.ReadAsStringAsync().GetAwaiter().GetResult()
if (-not $response.IsSuccessStatusCode) { throw "Upload rejected: $([int]$response.StatusCode) $body" }
"Accepted: $body"

$deadline = (Get-Date).AddSeconds($TimeoutSeconds)
$sawNew = $false
while ((Get-Date) -lt $deadline) {
    Start-Sleep -Seconds 3
    try { $s = Get-OtaStatus } catch { Write-Host '.' -NoNewline; continue }
    if ($s.version -eq $newVersion) {
        $sawNew = $true
        if ($s.state -eq 'valid') { "`nNew image $newVersion is running and confirmed."; return }
    } elseif ($sawNew) {
        throw "Device went back to $($s.version): the new image was rolled back"
    }
}
throw "Timed out after $TimeoutSeconds s waiting for $newVersion to confirm"
