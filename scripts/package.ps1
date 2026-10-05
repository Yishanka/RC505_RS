param([string]$Version,[string]$Iscc,[switch]$SkipBuild)
$ErrorActionPreference='Stop'
$workspace = Split-Path -Parent $PSScriptRoot
$portable = $null
Push-Location $workspace
try {
    $cargoVersion = ((Get-Content Cargo.toml | Where-Object {$_ -match '^version\s*='}) -replace '^version\s*=\s*"([^" ]+)".*$','$1').Trim()
    if (!$Version) {$Version=$cargoVersion}
    if ($Version -notmatch '^\d+\.\d+\.\d+$') {throw 'Release version must be major.minor.patch.'}
    if ($Version -ne $cargoVersion) {throw 'Tag/version does not match Cargo.toml.'}
    if (!$SkipBuild) {cargo build --release --bins --locked; if ($LASTEXITCODE -ne 0) {throw 'Release build failed.'}}
    if (!$Iscc) {
        $candidates=@("${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe", "$env:ProgramFiles\Inno Setup 6\ISCC.exe", "$workspace\var\tools\InnoSetup\ISCC.exe")
        $Iscc=$candidates | Where-Object {Test-Path -LiteralPath $_} | Select-Object -First 1
    }
    if (!$Iscc) {throw 'Install Inno Setup 6 or pass -Iscc.'}
    New-Item -ItemType Directory -Force -Path dist | Out-Null
    & $Iscc "/DAppVersion=$Version" "/DSourceRoot=$workspace" installer/RC505-RS.iss
    if ($LASTEXITCODE -ne 0) {throw 'Installer compilation failed.'}
    & (Join-Path $PSScriptRoot 'test-installer.ps1') -Version $Version -Iscc $Iscc
    & (Join-Path $PSScriptRoot 'test-uninstall-data.ps1')
    $portable=Join-Path $workspace ("var/package-$Version-" + [Guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Force -Path $portable | Out-Null
    Copy-Item -LiteralPath target/release/rc505_rs.exe,target/release/rc505_launcher.exe,README_CN.md -Destination $portable -Force
    Copy-Item -LiteralPath docs -Destination $portable -Recurse -Force
    [IO.File]::WriteAllText((Join-Path $portable 'install-settings.json'),'{"data_dir":"data","download_dir":"downloads"}',[Text.UTF8Encoding]::new($false))
    Compress-Archive -Path "$portable/*" -DestinationPath "dist/RC505-RS-$Version-windows-x64-portable.zip" -Force
    $asset="RC505-RS-$Version-windows-x64-setup.exe"
    $hash=(Get-FileHash -LiteralPath (Join-Path dist $asset) -Algorithm SHA256).Hash.ToLowerInvariant()
    $manifest=@{schema=1;version=$Version;file=$asset;url="https://github.com/Yishanka/RC505_RS/releases/download/v$Version/$asset";sha256=$hash}
    [IO.File]::WriteAllText((Join-Path $workspace 'dist/update.json'),($manifest|ConvertTo-Json),[Text.UTF8Encoding]::new($false))
    $sums=Get-ChildItem dist -File | Where-Object {$_.Name -like "*$Version*"} | ForEach-Object {"$((Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant())  $($_.Name)"}
    [IO.File]::WriteAllLines((Join-Path $workspace 'dist/SHA256SUMS.txt'),$sums,[Text.UTF8Encoding]::new($false))
    # The current installer has passed smoke tests and the portable ZIP exists.
    # Keep one local release in dist; historical releases remain on GitHub.
    $artifactRoot = [IO.Path]::GetFullPath((Join-Path $workspace 'dist'))
    if ((Get-Item -LiteralPath $artifactRoot).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Refusing cleanup in a linked artifact directory.' }
    foreach ($oldArtifact in Get-ChildItem -LiteralPath $artifactRoot -File) {
        if ($oldArtifact.Name -match '^RC505-RS-(\d+\.\d+\.\d+)-windows-x64-(setup\.exe|portable\.zip)$' -and $Matches[1] -ne $Version) {
            Remove-Item -LiteralPath $oldArtifact.FullName -Force
        }
    }
} finally {
    if ($portable -and (Test-Path -LiteralPath $portable)) {
        $resolved = [IO.Path]::GetFullPath($portable)
        $allowed = [IO.Path]::GetFullPath((Join-Path $workspace 'var')) + [IO.Path]::DirectorySeparatorChar
        if (!$resolved.StartsWith($allowed, [StringComparison]::OrdinalIgnoreCase) -or (Split-Path -Leaf $resolved) -notmatch '^package-\d') { throw 'Refusing cleanup outside package workspace.' }
        if ((Get-Item -LiteralPath $resolved).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Refusing linked package workspace.' }
        Remove-Item -LiteralPath $resolved -Recurse -Force
    }
    Pop-Location
}
