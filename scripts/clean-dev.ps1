param([int]$OlderThanDays = 1, [switch]$Apply)
# Only known generated work directories are eligible. Research, videos, PDFs,
# tool installations and arbitrary user files are never selected.
$ErrorActionPreference = 'Stop'
$workspace = [IO.Path]::GetFullPath((Split-Path -Parent $PSScriptRoot))
$scratch = [IO.Path]::GetFullPath((Join-Path $workspace 'var'))
if (!(Test-Path -LiteralPath $scratch)) { return }
$cutoff = [DateTime]::UtcNow.AddDays(-[Math]::Max(0, $OlderThanDays))
$directoryPattern = '^(capture-test-|history-snapshot-|installer-smoke-|package-\d|replay-trash-test-|replay-assets-test-|replay-delta-test-|global-replay-|stream-replay-test-|session-test-|ui-navigation-test-|ui-0\d+|clean-source-check$)'
$filePattern = '^(actions-|ci-|run-|ready-|build[-.]|check[-.]|test[s]?[-.]|release-[0-9].*\.log$|release-.*\.html$|output-.*\.(log|html)$|ui-.*\.log$|package.*\.log$|audio_probe\.(exe|pdb)$|session_probe\.(exe|pdb)$|delay_bench.*\.(exe|pdb)$|clean-source\.(zip|log)$)'
$candidates = @(Get-ChildItem -LiteralPath $scratch -Force | Where-Object {
    $_.LastWriteTimeUtc -lt $cutoff -and
    (($_.PSIsContainer -and $_.Name -match $directoryPattern) -or
     (!$_.PSIsContainer -and $_.Name -match $filePattern -and $_.Extension -in @('.log','.html','.zip','.exe','.pdb')))
})
$screens = Join-Path $scratch 'ui-verification'
if (Test-Path -LiteralPath $screens) {
    if ((Get-Item -LiteralPath $screens).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Refusing linked screenshot directory.' }
    $candidates += @(Get-ChildItem -LiteralPath $screens -File | Where-Object {$_.LastWriteTimeUtc -lt $cutoff -and $_.Extension -in @('.ppm','.png')})
}
$total = 0L
foreach ($candidate in $candidates) {
    $target = [IO.Path]::GetFullPath($candidate.FullName)
    if (!$target.StartsWith($scratch + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw "Outside scratch directory: $target" }
    if ($candidate.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Refusing linked path: $target" }
    $children = if ($candidate.PSIsContainer) { @(Get-ChildItem -LiteralPath $target -Force -Recurse) } else { @($candidate) }
    if ($children | Where-Object {$_.Attributes -band [IO.FileAttributes]::ReparsePoint}) { throw "Refusing directory containing a link: $target" }
    $bytes = ($children | Where-Object {!$_.PSIsContainer} | Measure-Object Length -Sum).Sum
    $total += [long]$bytes
    if ($Apply) { Remove-Item -LiteralPath $target -Recurse -Force }
}
[pscustomobject]@{Mode=$(if($Apply){'Removed'}else{'Preview'});Items=$candidates.Count;Megabytes=[Math]::Round($total/1MB,2);Root=$scratch}
if (!$Apply) { Write-Output 'Add -Apply to remove these generated items. Reference PDFs, videos, research and tools are preserved.' }
