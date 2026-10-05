param(
    [ValidateSet('Check','Delete')][string]$Mode='Check',
    [Parameter(Mandatory=$true)][string]$InstallDir,
    [switch]$ConfirmDelete,
    [string]$ExpectedDataDir,
    [string]$ReportFile
)
$ErrorActionPreference='Stop'
$ProgressPreference='SilentlyContinue'
function Full-Path([string]$Value) {
    $full=[IO.Path]::GetFullPath($Value)
    if ($full -ieq [IO.Path]::GetPathRoot($full)) {return $full}
    return $full.TrimEnd([char[]]'\/')
}
function Data-Path($Settings,[string]$Program) {
    if ([string]::IsNullOrWhiteSpace($Settings.data_dir)) {throw 'No configured data directory.'}
    $path=$Settings.data_dir
    if (![IO.Path]::IsPathRooted($path)) {$path=Join-Path $Program $path}
    return Full-Path $path
}
function Assert-PlainAncestors([string]$Path) {
    $current=Get-Item -LiteralPath $Path -Force
    while ($null -ne $current) {
        if ($current.Attributes -band [IO.FileAttributes]::ReparsePoint) {throw 'Linked data paths are preserved; remove them manually if intended.'}
        $current=$current.Parent
    }
}
function Assert-PlainTree([string]$Path,[string]$Prefix) {
    $pending=[Collections.Generic.Stack[string]]::new();$pending.Push($Path)
    while ($pending.Count -gt 0) {
        $item=Get-Item -LiteralPath $pending.Pop() -Force
        if (!(Full-Path $item.FullName).StartsWith($Prefix,[StringComparison]::OrdinalIgnoreCase)) {throw 'Data path escaped the configured folder.'}
        if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) {throw 'Linked data files are preserved; remove them manually if intended.'}
        if ($item.PSIsContainer) {foreach($child in Get-ChildItem -LiteralPath $item.FullName -Force) {$pending.Push($child.FullName)}}
    }
}
try {
    $program=Full-Path $InstallDir
    foreach($name in @('rc505_rs','rc505_launcher')) {
        foreach($process in [Diagnostics.Process]::GetProcessesByName($name)) {
            $executable=$null
            try {$executable=$process.MainModule.FileName} catch {}
            if ($executable -and (Full-Path $executable) -ieq (Join-Path $program ($name+'.exe'))) {
                throw 'Close RC505 RS and its audio launcher before uninstalling. Save your work first.'
            }
        }
    }
    if ($Mode -eq 'Check') {
        # The uninstaller must display this live JSON-derived path, rather than
        # an install-time copy. Invalid settings disable data removal, but must
        # not prevent a normal program-only uninstall.
        $confirmed=''
        try {
            $settings=Get-Content -LiteralPath (Join-Path $program 'install-settings.json') -Raw -Encoding UTF8 | ConvertFrom-Json
            $confirmed=Data-Path $settings $program
        } catch {}
        if ($ReportFile) {[IO.File]::WriteAllText($ReportFile,$confirmed)}
        exit 0
    }
    if (!$ConfirmDelete) {throw 'Deleting application data requires explicit confirmation.'}
    if ([string]::IsNullOrWhiteSpace($ExpectedDataDir)) {throw 'No confirmed data directory; application data preserved.'}
    $settings=Get-Content -LiteralPath (Join-Path $program 'install-settings.json') -Raw -Encoding UTF8 | ConvertFrom-Json
    $root=Data-Path $settings $program
    if ((Full-Path $ExpectedDataDir) -ine $root) {throw 'The data directory changed after confirmation. Application data was preserved; restart uninstall and confirm the current location.'}
    if ([string]::IsNullOrWhiteSpace($settings.download_dir)) {throw 'No configured download directory; data preserved because download ownership cannot be checked.'}
    $downloads=$settings.download_dir
    if (![IO.Path]::IsPathRooted($downloads)) {$downloads=Join-Path $program $downloads}
    $downloads=Full-Path $downloads
    $protected=@([IO.Path]::GetPathRoot($root),$env:SystemRoot,$env:USERPROFILE,[Environment]::GetFolderPath('Desktop'),[Environment]::GetFolderPath('MyDocuments'),[Environment]::GetFolderPath('ApplicationData'),[Environment]::GetFolderPath('LocalApplicationData'))
    foreach($path in $protected) {if ($path -and $root -ieq (Full-Path $path)) {throw 'A drive, system, or personal root directory cannot be deleted as application data.'}}
    if (!(Test-Path -LiteralPath $root)) {exit 0}
    Assert-PlainAncestors $root
    $marker=Get-Content -LiteralPath (Join-Path $root '.rc505-rs-data.json') -Raw -Encoding UTF8 | ConvertFrom-Json
    if ($marker.product -cne 'RC505 RS' -or $marker.schema -ne 1 -or (Full-Path $marker.data_dir) -ine $root) {throw 'Data ownership marker does not match; data preserved.'}
    $prefix=$root+[IO.Path]::DirectorySeparatorChar
    $targets=@()
    foreach($name in @('projects','presets','clips','replays','logs','launcher_config.json','keyboard.json','migration.json','.rc505-rs-data.json')) {
        $target=Join-Path $root $name
        if (Test-Path -LiteralPath $target) {
            $target=Full-Path $target
            foreach($retained in @($program,$downloads)) {
                if ($retained -ieq $target -or $retained.StartsWith($target+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)) {
                    throw 'Data folders overlap the installation or installer download directory. Data removal was skipped. Uninstall while keeping data, then organize/remove the overlapping folders manually.'
                }
            }
            Assert-PlainTree $target $prefix;$targets+=$target
        }
    }
    # No recursive deletion occurs before every target and ancestor is checked.
    # Remove only RC505-owned entries, never the configured root or download folder.
    $lock=Join-Path $root 'projects/editor.lock'
    if (Test-Path -LiteralPath $lock) {
        $handle=[IO.File]::Open($lock,[IO.FileMode]::Open,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)
        $handle.Dispose()
    }
    foreach($target in $targets) {Remove-Item -LiteralPath $target -Recurse -Force}
    if ($ReportFile) {[IO.File]::WriteAllText($ReportFile,'RC505 RS data removed. Other files and downloaded installers were preserved.')}
} catch {
    if ($ReportFile) {[IO.File]::WriteAllText($ReportFile,$_.Exception.Message)}
    [Console]::Error.WriteLine($_.Exception.Message)
    exit 1
}
