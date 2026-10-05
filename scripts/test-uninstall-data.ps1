# No installer, uninstaller, registry mutation, or user process is executed here.
# All removal targets and the junction destination are disposable var fixtures.
$ErrorActionPreference='Stop'
$workspace=Split-Path -Parent $PSScriptRoot
$testRoot=Join-Path $workspace ('var/uninstall-data-test-'+[Guid]::NewGuid().ToString('N'))
$script=Join-Path $PSScriptRoot 'uninstall-data.ps1'
$powershell=Join-Path $env:SystemRoot 'System32/WindowsPowerShell/v1.0/powershell.exe'
$junction=$null
function Fixture([string]$Name) {
    $program=Join-Path $testRoot ($Name+' program');$data=Join-Path $testRoot ($Name+' data')
    New-Item -ItemType Directory -Path $program,$data,(Join-Path $data 'projects'),(Join-Path $data 'presets'),(Join-Path $data 'clips'),(Join-Path $data 'replays'),(Join-Path $data 'logs'),(Join-Path $data 'downloads') -Force | Out-Null
    [IO.File]::WriteAllText((Join-Path $program 'install-settings.json'),(@{data_dir=$data;download_dir=(Join-Path $data 'downloads')}|ConvertTo-Json))
    [IO.File]::WriteAllText((Join-Path $data '.rc505-rs-data.json'),(@{product='RC505 RS';schema=1;data_dir=$data}|ConvertTo-Json))
    foreach($name in @('projects/audio.wav','presets/sound.json','clips/melody.json','replays/input.wav','logs/log.txt','launcher_config.json','keyboard.json','migration.json','unrelated.txt','downloads/setup.exe')){[IO.File]::WriteAllText((Join-Path $data $name),'fixture')}
    return @{program=$program;data=$data}
}
function Run-Helper($Fixture,[string]$Mode,[bool]$Confirm,[bool]$Success,[string]$ExpectedDataDir=$Fixture.data) {
    $args=@('-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',$script,'-Mode',$Mode,'-InstallDir',$Fixture.program,'-ReportFile',(Join-Path $testRoot 'result.txt'))
    if($Confirm){$args+='-ConfirmDelete'}
    if($Mode -eq 'Delete' -and $ExpectedDataDir){$args+=@('-ExpectedDataDir',$ExpectedDataDir)}
    $previous=$ErrorActionPreference;$ErrorActionPreference='Continue'
    try {& $powershell @args 2>&1 | Out-Null;$code=$LASTEXITCODE} finally {$ErrorActionPreference=$previous}
    if(($code -eq 0) -ne $Success){throw "Unexpected helper exit $code for $Mode (confirmation=$Confirm); fixtures at $testRoot"}
}
try {
    $keep=Fixture 'keep'
    Run-Helper $keep 'Check' $false $true
    if((Get-Content -LiteralPath (Join-Path $testRoot 'result.txt') -Raw) -ine $keep.data){throw 'Check must report the current absolute JSON data path.'}
    Run-Helper $keep 'Delete' $false $false
    if(!(Test-Path -LiteralPath (Join-Path $keep.data 'projects/audio.wav'))){throw 'Default/check/no-confirm path removed data.'}
    $moving=Fixture 'moving-settings';$destination=Fixture 'moved-data'
    # An obsolete install-time INI must have no influence on the prompt path.
    [IO.File]::WriteAllText((Join-Path $moving.program 'uninstall-paths.ini'),("[Paths]`r`nDataDir="+$moving.data))
    [IO.File]::WriteAllText((Join-Path $moving.program 'install-settings.json'),(@{data_dir=$destination.data;download_dir=(Join-Path $destination.data 'downloads')}|ConvertTo-Json))
    Run-Helper $moving 'Check' $false $true
    $confirmed=Get-Content -LiteralPath (Join-Path $testRoot 'result.txt') -Raw
    if($confirmed -ine $destination.data){throw 'Check used a stale install-time path after settings moved.'}
    # Both A and B are real marked RC505 data folders: detecting missing marker
    # alone cannot protect this confirmation-to-deletion race.
    [IO.File]::WriteAllText((Join-Path $moving.program 'install-settings.json'),(@{data_dir=$moving.data;download_dir=(Join-Path $moving.data 'downloads')}|ConvertTo-Json))
    Run-Helper $moving 'Delete' $true $false $confirmed
    if(!(Test-Path -LiteralPath (Join-Path $moving.data 'projects/audio.wav')) -or !(Test-Path -LiteralPath (Join-Path $destination.data 'projects/audio.wav'))){throw 'Changing settings after Check must preserve both locations.'}
    if((Get-Content -LiteralPath (Join-Path $testRoot 'result.txt') -Raw) -notmatch 'changed after confirmation'){throw 'Changed-path rejection must explain why data was kept.'}
    $invalid=Fixture 'invalid-settings'
    [IO.File]::WriteAllText((Join-Path $invalid.program 'install-settings.json'),'{invalid json')
    Run-Helper $invalid 'Check' $false $true
    if((Get-Item -LiteralPath (Join-Path $testRoot 'result.txt')).Length -ne 0){throw 'Invalid settings must allow program-only uninstall without a data deletion target.'}
    Run-Helper $keep 'Delete' $true $false ''
    $missing=Fixture 'missing-marker'
    Remove-Item -LiteralPath (Join-Path $missing.data '.rc505-rs-data.json')
    Run-Helper $missing 'Delete' $true $false
    if(!(Test-Path -LiteralPath (Join-Path $missing.data 'projects/audio.wav'))){throw 'Missing marker must preserve data.'}
    $mismatch=Fixture 'mismatching-marker'
    [IO.File]::WriteAllText((Join-Path $mismatch.data '.rc505-rs-data.json'),(@{product='RC505 RS';schema=1;data_dir=$keep.data}|ConvertTo-Json))
    Run-Helper $mismatch 'Delete' $true $false
    if(!(Test-Path -LiteralPath (Join-Path $mismatch.data 'projects/audio.wav'))){throw 'Mismatching marker must preserve all data.'}
    $downloadOverlap=Fixture 'download-overlap'
    $nestedDownload=Join-Path $downloadOverlap.data 'replays/downloads'
    New-Item -ItemType Directory -Path $nestedDownload | Out-Null
    [IO.File]::WriteAllText((Join-Path $nestedDownload 'setup.exe'),'download fixture must survive')
    [IO.File]::WriteAllText((Join-Path $downloadOverlap.program 'install-settings.json'),(@{data_dir=$downloadOverlap.data;download_dir=$nestedDownload}|ConvertTo-Json))
    Run-Helper $downloadOverlap 'Check' $false $true
    Run-Helper $downloadOverlap 'Delete' $true $false
    if(!(Test-Path -LiteralPath (Join-Path $nestedDownload 'setup.exe')) -or !(Test-Path -LiteralPath (Join-Path $downloadOverlap.data 'projects/audio.wav'))){throw 'Nested download path must reject the entire deletion before any target is removed.'}
    if((Get-Content -LiteralPath (Join-Path $testRoot 'result.txt') -Raw) -notmatch 'overlap'){throw 'Overlap rejection must explain the manual cleanup path.'}
    [IO.File]::WriteAllText((Join-Path $downloadOverlap.program 'install-settings.json'),(@{data_dir=$downloadOverlap.data;download_dir=(Join-Path $downloadOverlap.data 'replays')}|ConvertTo-Json))
    Run-Helper $downloadOverlap 'Delete' $true $false
    if(!(Test-Path -LiteralPath (Join-Path $nestedDownload 'setup.exe'))){throw 'A download directory equal to a deletion target must also be preserved.'}
    $programOverlap=Fixture 'program-overlap'
    $nestedProgram=Join-Path $programOverlap.data 'projects/app'
    New-Item -ItemType Directory -Path $nestedProgram | Out-Null
    Copy-Item -LiteralPath (Join-Path $programOverlap.program 'install-settings.json') -Destination (Join-Path $nestedProgram 'install-settings.json')
    [IO.File]::WriteAllText((Join-Path $nestedProgram 'rc505_rs.exe'),'not executable; installation fixture must survive')
    $programOverlap.program=$nestedProgram
    Run-Helper $programOverlap 'Check' $false $true
    Run-Helper $programOverlap 'Delete' $true $false
    if(!(Test-Path -LiteralPath (Join-Path $nestedProgram 'rc505_rs.exe')) -or !(Test-Path -LiteralPath (Join-Path $programOverlap.data 'presets/sound.json'))){throw 'Nested installation must reject the entire deletion before any target is removed.'}
    $linked=Fixture 'linked';$outside=Join-Path $testRoot 'outside'
    New-Item -ItemType Directory -Path $outside | Out-Null
    [IO.File]::WriteAllText((Join-Path $outside 'keep.txt'),'must survive')
    $junction=Join-Path $linked.data 'projects/linked'
    New-Item -ItemType Junction -Path $junction -Target $outside | Out-Null
    Run-Helper $linked 'Delete' $true $false
    if(!(Test-Path -LiteralPath (Join-Path $outside 'keep.txt')) -or !(Test-Path -LiteralPath (Join-Path $linked.data 'projects/audio.wav'))){throw 'Linked path guard failed.'}
    [IO.Directory]::Delete($junction);$junction=$null
    $locked=Fixture 'locked'
    $handle=[IO.File]::Open((Join-Path $locked.data 'projects/editor.lock'),[IO.FileMode]::Create,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)
    try {Run-Helper $locked 'Delete' $true $false} finally {$handle.Dispose()}
    if(!(Test-Path -LiteralPath (Join-Path $locked.data 'projects/audio.wav'))){throw 'Open editor guard removed data.'}
    $remove=Fixture 'remove'
    Run-Helper $remove 'Delete' $true $true
    foreach($name in @('projects','presets','clips','replays','logs','launcher_config.json','keyboard.json','migration.json','.rc505-rs-data.json')){if(Test-Path -LiteralPath (Join-Path $remove.data $name)){throw "Application data was not removed: $name"}}
    foreach($name in @('unrelated.txt','downloads/setup.exe')){if(!(Test-Path -LiteralPath (Join-Path $remove.data $name))){throw "Unrelated/download data removed: $name"}}
    Write-Output 'Uninstall data helper passed: current-path confirmation, changed/invalid settings preservation, keep-by-default, explicit deletion, missing/mismatching markers, overlapping installation/download refusal, junction, open editor, unrelated files/download preservation.'
} finally {
    if($junction -and (Test-Path -LiteralPath $junction)){[IO.Directory]::Delete($junction)}
    if(Test-Path -LiteralPath $testRoot){
        $resolved=[IO.Path]::GetFullPath($testRoot);$allowed=[IO.Path]::GetFullPath((Join-Path $workspace 'var'))+[IO.Path]::DirectorySeparatorChar
        if(!$resolved.StartsWith($allowed,[StringComparison]::OrdinalIgnoreCase) -or (Split-Path -Leaf $resolved) -notmatch '^uninstall-data-test-[a-f0-9]{32}$'){throw 'Refusing cleanup outside isolated test workspace.'}
        if((Get-Item -LiteralPath $resolved).Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Refusing linked fixture root.'}
        Remove-Item -LiteralPath $resolved -Recurse -Force
    }
}
