param([Parameter(Mandatory=$true)][string]$Version,[Parameter(Mandatory=$true)][string]$Iscc,[switch]$KeepArtifacts)
$ErrorActionPreference='Stop'
$workspace=Split-Path -Parent $PSScriptRoot
$testRoot=Join-Path $workspace ('var/installer-smoke-'+[Guid]::NewGuid().ToString('N'))
$program=Join-Path $testRoot 'program'
$data=Join-Path $program 'data'
$downloads=Join-Path $testRoot 'downloads'
$customData=Join-Path $testRoot 'custom music'
$customDownloads=Join-Path $testRoot 'custom installers'
New-Item -ItemType Directory -Force -Path $data,$downloads,$customData,$customDownloads | Out-Null
$marker=Join-Path $data 'preserve-this.txt'
[IO.File]::WriteAllText($marker,'This file must survive installation and update.')
$before=(Get-FileHash -LiteralPath $marker -Algorithm SHA256).Hash
$customMarker=Join-Path $customData 'preserve-custom.txt'
[IO.File]::WriteAllText($customMarker,'Existing custom data must survive explicit updates and subsequent reinstall.')
$customBefore=(Get-FileHash -LiteralPath $customMarker -Algorithm SHA256).Hash
Write-Output "Installer smoke workspace: $testRoot"
# A distinct, non-uninstallable test identity creates no normal product registry
# entries or shortcuts. It must never alter the user's RC505 RS installation.
& $Iscc '/DInstallerSmokeTest=1' "/DAppVersion=$Version" "/DSourceRoot=$workspace" "/O$testRoot" '/Finstaller-smoke' (Join-Path $workspace 'installer/RC505-RS.iss')
if ($LASTEXITCODE -ne 0) {throw 'Smoke installer compilation failed.'}
$cases=@(
    # No path arguments at all: smoke-only defaults resolve beside this setup,
    # inside var. This reproduces ordinary double-click initialization instead
    # of hiding it behind DATADIR. Production defaults remain unchanged.
    @{Name='first-no-path-arguments';Arguments=@();Data=$data;Downloads=$downloads},
    @{Name='explicit-update-overrides';Arguments=@(('/DIR="'+$program+'"'),('/DATADIR="'+$customData+'"'),('/DOWNLOADDIR="'+$customDownloads+'"'));Data=$customData;Downloads=$customDownloads},
    # No registry is created by InstallerSmokeTest. Its fixture INI supplies
    # previously saved values to the identical production selection code.
    @{Name='saved-custom-reinstall';Arguments=@();Data=$customData;Downloads=$customDownloads}
)
foreach ($case in $cases) {
    $log=Join-Path $testRoot ($case.Name+'.log')
    $arguments=@('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART',('/LOG="'+$log+'"'))+$case.Arguments
    $setup=Start-Process -FilePath (Join-Path $testRoot 'installer-smoke.exe') -ArgumentList $arguments -PassThru -WindowStyle Hidden
    $setup.WaitForExit()
    if ($setup.ExitCode -ne 0) {throw "Installer case $($case.Name) failed; see $testRoot"}
    if (!(Select-String -LiteralPath $log -SimpleMatch 'RC505 installer path checks passed:' -Quiet)) {throw 'Wizard path regression checks did not run.'}
    if ((Get-FileHash -LiteralPath $marker -Algorithm SHA256).Hash -ne $before) {throw 'Installer changed user data.'}
    if ((Get-FileHash -LiteralPath $customMarker -Algorithm SHA256).Hash -ne $customBefore) {throw 'Installer changed custom user data.'}
    foreach ($binary in @('rc505_rs.exe','rc505_launcher.exe')) {
        $bytes=[IO.File]::ReadAllBytes((Join-Path $program $binary))
        $peOffset=[BitConverter]::ToInt32($bytes,0x3c)
        if ([BitConverter]::ToUInt16($bytes,$peOffset+24+68) -ne 2) {throw "$binary must use the Windows GUI subsystem, without a startup console."}
    }
    $info=& (Join-Path $program 'rc505_rs.exe') --installation-info | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0 -or $info.version -ne $Version -or $info.data_dir -ne $case.Data -or $info.download_dir -ne $case.Downloads) {throw "Installed version or data paths do not match in $($case.Name)."}
    Write-Output "Passed installer case: $($case.Name)"
}
$data=$customData
$downloads=$customDownloads
$candidateName="RC505-RS-$Version-windows-x64-setup.exe"
$candidate=Join-Path $downloads $candidateName
Copy-Item -LiteralPath (Join-Path $testRoot 'installer-smoke.exe') -Destination $candidate
$metadata=@{schema=1;version=$Version;file=$candidateName;url="https://github.com/Yishanka/RC505_RS/releases/download/v$Version/$candidateName";sha256=(Get-FileHash -LiteralPath $candidate -Algorithm SHA256).Hash.ToLowerInvariant()}
$metadata | ConvertTo-Json | Set-Content -LiteralPath ($candidate+'.verified.json') -Encoding UTF8
$old=Join-Path $downloads 'RC505-RS-0.0.0-windows-x64-setup.exe'
$unrelated=Join-Path $downloads 'another-program.exe'
[IO.File]::WriteAllText($old,'old cache fixture')
[IO.File]::WriteAllText($unrelated,'must remain')
$previousModulePath=$env:PSModulePath
try {
    # Reproduce launching from a shell with an incompatible module environment.
    $env:PSModulePath=Join-Path $testRoot 'foreign-shell-modules'
    & (Join-Path $program 'rc505_rs.exe') ("--cleanup-update-cache="+$candidate) | Out-Null
    if ($LASTEXITCODE -ne 0) {throw 'Installer cache finalization failed.'}
} finally {$env:PSModulePath=$previousModulePath}
$latest=Join-Path $downloads 'RC505-RS-setup.exe'
if (!(Test-Path -LiteralPath $latest) -or (Test-Path -LiteralPath $old) -or (Test-Path -LiteralPath $candidate) -or !(Test-Path -LiteralPath $unrelated)) {throw 'Cache retention policy is incorrect.'}
if ((Get-FileHash -LiteralPath $latest -Algorithm SHA256).Hash -ine $metadata.sha256) {throw 'Latest cached installer changed.'}
Write-Output 'Installer smoke test passed: no-argument paths, wizard edits, explicit update, saved custom paths, data preservation and one-package cache.'
if ($KeepArtifacts) {
    Write-Output "Installer smoke artifacts kept: $testRoot"
    return
}
$resolved = [IO.Path]::GetFullPath($testRoot)
$allowed = [IO.Path]::GetFullPath((Join-Path $workspace 'var')) + [IO.Path]::DirectorySeparatorChar
if (!$resolved.StartsWith($allowed,[StringComparison]::OrdinalIgnoreCase) -or (Split-Path -Leaf $resolved) -notmatch '^installer-smoke-[a-f0-9]{32}$') { throw 'Refusing cleanup outside installer test workspace.' }
if ((Get-Item -LiteralPath $resolved).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Refusing linked installer test workspace.' }
Remove-Item -LiteralPath $resolved -Recurse -Force
