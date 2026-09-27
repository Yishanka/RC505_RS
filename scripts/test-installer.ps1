param([Parameter(Mandatory=$true)][string]$Version,[Parameter(Mandatory=$true)][string]$Iscc)
$ErrorActionPreference='Stop'
$workspace=Split-Path -Parent $PSScriptRoot
$testRoot=Join-Path $workspace ('var/installer-smoke-'+[Guid]::NewGuid().ToString('N'))
$program=Join-Path $testRoot 'program'
$data=Join-Path $program 'data'
$downloads=Join-Path $testRoot 'downloads'
New-Item -ItemType Directory -Force -Path $data,$downloads | Out-Null
$marker=Join-Path $data 'preserve-this.txt'
[IO.File]::WriteAllText($marker,'This file must survive installation and update.')
$before=(Get-FileHash -LiteralPath $marker -Algorithm SHA256).Hash
# A distinct, non-uninstallable test identity creates no normal product registry
# entries or shortcuts. It must never alter the user's RC505 RS installation.
& $Iscc '/DInstallerSmokeTest=1' "/DAppVersion=$Version" "/DSourceRoot=$workspace" "/O$testRoot" '/Finstaller-smoke' (Join-Path $workspace 'installer/RC505-RS.iss')
if ($LASTEXITCODE -ne 0) {throw 'Smoke installer compilation failed.'}
foreach ($attempt in 1..2) {
    $arguments=@('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART',('/DIR="'+$program+'"'),('/DATADIR="'+$data+'"'),('/DOWNLOADDIR="'+$downloads+'"'),('/LOG="'+(Join-Path $testRoot "install-$attempt.log")+'"'))
    $setup=Start-Process -FilePath (Join-Path $testRoot 'installer-smoke.exe') -ArgumentList $arguments -PassThru -WindowStyle Hidden
    $setup.WaitForExit()
    if ($setup.ExitCode -ne 0) {throw "Silent install without import failed (attempt $attempt); see $testRoot"}
    if ((Get-FileHash -LiteralPath $marker -Algorithm SHA256).Hash -ne $before) {throw 'Installer changed user data.'}
    $info=& (Join-Path $program 'rc505_rs.exe') --installation-info | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0 -or $info.version -ne $Version -or $info.data_dir -ne $data -or $info.download_dir -ne $downloads) {throw 'Installed version or data paths do not match.'}
}
$candidateName="RC505-RS-$Version-windows-x64-setup.exe"
$candidate=Join-Path $downloads $candidateName
Copy-Item -LiteralPath (Join-Path $testRoot 'installer-smoke.exe') -Destination $candidate
$metadata=@{schema=1;version=$Version;file=$candidateName;url="https://github.com/Yishanka/RC505_RS/releases/download/v$Version/$candidateName";sha256=(Get-FileHash -LiteralPath $candidate -Algorithm SHA256).Hash.ToLowerInvariant()}
$metadata | ConvertTo-Json | Set-Content -LiteralPath ($candidate+'.verified.json') -Encoding UTF8
$old=Join-Path $downloads 'RC505-RS-0.0.0-windows-x64-setup.exe'
$unrelated=Join-Path $downloads 'another-program.exe'
[IO.File]::WriteAllText($old,'old cache fixture')
[IO.File]::WriteAllText($unrelated,'must remain')
& (Join-Path $program 'rc505_rs.exe') ("--cleanup-update-cache="+$candidate)
if ($LASTEXITCODE -ne 0) {throw 'Installer cache finalization failed.'}
$latest=Join-Path $downloads 'RC505-RS-setup.exe'
if (!(Test-Path -LiteralPath $latest) -or (Test-Path -LiteralPath $old) -or (Test-Path -LiteralPath $candidate) -or !(Test-Path -LiteralPath $unrelated)) {throw 'Cache retention policy is incorrect.'}
if ((Get-FileHash -LiteralPath $latest -Algorithm SHA256).Hash -ine $metadata.sha256) {throw 'Latest cached installer changed.'}
Write-Output 'Installer smoke test passed: no-import install, reinstall, data preservation and one-package cache.'
