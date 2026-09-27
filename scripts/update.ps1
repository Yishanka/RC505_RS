param(
    [ValidateSet('Check','Download','Install','Cleanup')][string]$Mode = 'Check',
    [string]$DownloadDir,
    [string]$Installer,
    [string]$InstallDir,
    [string]$DataDir,
    [int]$WaitForProcess = 0
)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$releaseRoot = 'https://github.com/Yishanka/RC505_RS/releases/'
function Read-Release {
    $release = Invoke-RestMethod -Uri ($releaseRoot + 'latest/download/update.json') -Headers @{'User-Agent'='RC505-RS-Updater'} -TimeoutSec 30
    if ($release.schema -ne 1 -or $release.version -notmatch '^\d+\.\d+\.\d+$') { throw 'Invalid update manifest version.' }
    if ($release.file -ne "RC505-RS-$($release.version)-windows-x64-setup.exe") { throw 'Invalid update asset name.' }
    $expected = $releaseRoot + "download/v$($release.version)/$($release.file)"
    if ($release.url -cne $expected -or $release.sha256 -notmatch '^[a-fA-F0-9]{64}$') { throw 'Update URL or checksum is invalid.' }
    return $release
}
function Complete-InstallerCache {
    param([string]$SourceInstaller)
    $root = [IO.Path]::GetFullPath($DownloadDir).TrimEnd([char[]]'\/')
    $prefix = $root + [IO.Path]::DirectorySeparatorChar
    $source = [IO.Path]::GetFullPath($SourceInstaller)
    $latest = Join-Path $root 'RC505-RS-setup.exe'
    $sidecar = $source + '.verified.json'
    $metadata = Get-Content -LiteralPath $sidecar -Raw -Encoding UTF8 | ConvertFrom-Json
    if ($metadata.url -cne ($releaseRoot + "download/v$($metadata.version)/$($metadata.file)")) { throw 'Untrusted cached installer origin.' }
    if ((Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash -ine $metadata.sha256) { throw 'Cached installer checksum mismatch.' }
    if ($source -ine $latest) {
        # Moves stay inside the configured download folder. Manual installers
        # outside that folder are copied and their originals remain untouched.
        if ($source.StartsWith($prefix,[StringComparison]::OrdinalIgnoreCase)) {
            Move-Item -LiteralPath $source -Destination $latest -Force
            Move-Item -LiteralPath $sidecar -Destination ($latest+'.verified.json') -Force
        } else {
            Copy-Item -LiteralPath $source -Destination $latest -Force
            Copy-Item -LiteralPath $sidecar -Destination ($latest+'.verified.json') -Force
        }
    }
    if ((Get-FileHash -LiteralPath $latest -Algorithm SHA256).Hash -ine $metadata.sha256) { throw 'Latest cache verification failed; old caches retained.' }
    foreach ($old in Get-ChildItem -LiteralPath $root -File) {
        if ($old.Name -match '^RC505-RS-\d+\.\d+\.\d+-windows-x64-setup\.exe(?:\.verified\.json|\.partial)?$') {
            $target = [IO.Path]::GetFullPath($old.FullName)
            if (!$target.StartsWith($prefix,[StringComparison]::OrdinalIgnoreCase)) { throw 'Cache path escaped download folder.' }
            Remove-Item -LiteralPath $target
        }
    }
    $errorLog=Join-Path $root 'update-error.log'
    if (Test-Path -LiteralPath $errorLog) {Remove-Item -LiteralPath $errorLog}
}
try {
    if ($Mode -eq 'Check') { Read-Release | ConvertTo-Json -Compress; exit 0 }
    if ($Mode -eq 'Cleanup') {Complete-InstallerCache -SourceInstaller $Installer; exit 0}
    if ($Mode -eq 'Download') {
        $release = Read-Release
        [IO.Directory]::CreateDirectory($DownloadDir) | Out-Null
        $destination = Join-Path $DownloadDir $release.file
        $temporary = $destination + '.partial'
        Invoke-WebRequest -UseBasicParsing -Uri $release.url -OutFile $temporary -Headers @{'User-Agent'='RC505-RS-Updater'} -TimeoutSec 180
        if ((Get-FileHash -LiteralPath $temporary -Algorithm SHA256).Hash -ine $release.sha256) { throw 'Downloaded installer checksum mismatch.' }
        Move-Item -LiteralPath $temporary -Destination $destination -Force
        $release | ConvertTo-Json | Set-Content -LiteralPath ($destination + '.verified.json') -Encoding UTF8
        @{installer=$destination;version=$release.version} | ConvertTo-Json -Compress
        exit 0
    }
    # A helper waits outside the main process. It never terminates the app.
    if ($WaitForProcess -gt 0) { Wait-Process -Id $WaitForProcess -ErrorAction SilentlyContinue }
    $manifest = Get-Content -LiteralPath ($Installer + '.verified.json') -Raw -Encoding UTF8 | ConvertFrom-Json
    if ($manifest.url -cne ($releaseRoot + "download/v$($manifest.version)/$($manifest.file)")) { throw 'Untrusted installer origin.' }
    if ((Get-FileHash -LiteralPath $Installer -Algorithm SHA256).Hash -ine $manifest.sha256) { throw 'Installer changed after verification.' }
    $arguments = @('/SILENT','/SUPPRESSMSGBOXES','/NORESTART',('/DIR="'+$InstallDir+'"'),('/DATADIR="'+$DataDir+'"'),('/DOWNLOADDIR="'+$DownloadDir+'"'))
    $setup = Start-Process -FilePath $Installer -ArgumentList $arguments -PassThru -WindowStyle Hidden
    $setup.WaitForExit()
    if ($setup.ExitCode -ne 0) { throw "Installer failed with code $($setup.ExitCode). Previous data remains at $DataDir" }
    try {Complete-InstallerCache -SourceInstaller $Installer} catch {
        ('Update installed; cache cleanup needs a retry: '+$_.Exception.Message) | Set-Content -LiteralPath (Join-Path $DownloadDir 'update-error.log') -Encoding UTF8
    }
    $restartInfo = [Diagnostics.ProcessStartInfo]::new()
    $restartInfo.FileName = Join-Path $InstallDir 'rc505_rs.exe'
    $restartInfo.WorkingDirectory = $InstallDir
    $restartInfo.UseShellExecute = $false
    $restartInfo.CreateNoWindow = $true
    [Diagnostics.Process]::Start($restartInfo) | Out-Null
} catch {
    [Console]::Error.WriteLine($_.Exception.Message)
    if ($DownloadDir -and (Test-Path -LiteralPath $DownloadDir)) {
        $_.Exception.Message | Set-Content -LiteralPath (Join-Path $DownloadDir 'update-error.log') -Encoding UTF8
    }
    exit 1
}
