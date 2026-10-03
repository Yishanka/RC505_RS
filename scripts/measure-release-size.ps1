param([string]$Output = 'var/release-size.json')
$ErrorActionPreference = 'Stop'
$workspace = Split-Path -Parent $PSScriptRoot
Push-Location $workspace
$names = @('CARGO_PROFILE_RELEASE_LTO','CARGO_PROFILE_RELEASE_CODEGEN_UNITS','CARGO_PROFILE_RELEASE_STRIP')
$previousProfile = @{}
foreach ($name in $names) { $previousProfile[$name] = [Environment]::GetEnvironmentVariable($name,'Process') }
function Source-Fingerprint {
    $files = @(Get-ChildItem -LiteralPath src,assets -Recurse -File)
    $files += @(Get-Item -LiteralPath Cargo.toml,Cargo.lock,build.rs,.cargo/config.toml)
    $lines = $files | Sort-Object FullName | ForEach-Object { $_.FullName + ':' + (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash }
    $hash = [Security.Cryptography.SHA256]::Create()
    try { [BitConverter]::ToString($hash.ComputeHash([Text.Encoding]::UTF8.GetBytes(($lines -join "`n")))).Replace('-','') }
    finally { $hash.Dispose() }
}
function Binary-Sizes {
    @('rc505_rs.exe','rc505_launcher.exe') | ForEach-Object {
        $item = Get-Item -LiteralPath (Join-Path 'target/release' $_)
        [pscustomobject]@{Binary=$_.ToString();Bytes=$item.Length}
    }
}
try {
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Output) | Out-Null
    $source = Source-Fingerprint
    # Cargo's former defaults: local thin LTO over 16 codegen units, no stripping.
    $env:CARGO_PROFILE_RELEASE_LTO = 'false'
    $env:CARGO_PROFILE_RELEASE_CODEGEN_UNITS = '16'
    $env:CARGO_PROFILE_RELEASE_STRIP = 'none'
    cargo build --release --bins --locked *> var/release-baseline.log
    if ($LASTEXITCODE -ne 0) { throw 'Baseline build failed; see var/release-baseline.log' }
    $baseline = @(Binary-Sizes)
    foreach ($name in $names) { Remove-Item -LiteralPath ("Env:" + $name) -ErrorAction SilentlyContinue }
    cargo build --release --bins --locked *> var/release-optimized.log
    if ($LASTEXITCODE -ne 0) { throw 'Optimized build failed; see var/release-optimized.log' }
    if ((Source-Fingerprint) -ne $source) { throw 'Source changed during comparison; rerun after edits finish.' }
    $optimized = @(Binary-Sizes)
    $results = for ($i=0;$i -lt $baseline.Count;$i++) {
        [pscustomobject]@{Binary=$baseline[$i].Binary;BaselineBytes=$baseline[$i].Bytes;OptimizedBytes=$optimized[$i].Bytes;ReductionPercent=[math]::Round(100*(1-$optimized[$i].Bytes/$baseline[$i].Bytes),2)}
    }
    [pscustomobject]@{SourceSHA256=$source;Compiler=(rustc --version);Results=@($results)} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $Output -Encoding utf8
    $results | Format-Table
} finally {
    foreach ($name in $names) {
        if ($null -eq $previousProfile[$name]) { Remove-Item -LiteralPath ("Env:" + $name) -ErrorAction SilentlyContinue }
        else { [Environment]::SetEnvironmentVariable($name,$previousProfile[$name],'Process') }
    }
    Pop-Location
}
