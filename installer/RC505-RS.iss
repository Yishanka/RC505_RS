#ifndef AppVersion
  #define AppVersion "0.2.0"
#endif
#ifndef SourceRoot
  #define SourceRoot ".."
#endif

[Setup]
AppId={{D70BC68C-93D0-4FA4-A660-7552396837B5}
AppName=RC505 RS
AppVersion={#AppVersion}
AppPublisher=Yishanka
AppPublisherURL=https://github.com/Yishanka/RC505_RS
AppUpdatesURL=https://github.com/Yishanka/RC505_RS/releases
DefaultDirName={localappdata}\Programs\RC505 RS
DefaultGroupName=RC505 RS
DisableDirPage=no
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir={#SourceRoot}\dist
OutputBaseFilename=RC505-RS-{#AppVersion}-windows-x64-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
WizardSizePercent=115
UninstallDisplayIcon={app}\rc505_rs.exe
CloseApplications=no
RestartApplications=no
SetupLogging=yes
VersionInfoVersion={#AppVersion}

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Shortcuts:"; Flags: unchecked

[Files]
Source: "{#SourceRoot}\target\release\rc505_rs.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceRoot}\target\release\rc505_launcher.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceRoot}\README_CN.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceRoot}\docs\*"; DestDir: "{app}\docs"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "{#SourceRoot}\scripts\update.ps1"; DestDir: "{app}\scripts"; Flags: ignoreversion

[Icons]
Name: "{group}\RC505 RS"; Filename: "{app}\rc505_rs.exe"; WorkingDir: "{app}"
Name: "{group}\Audio setup"; Filename: "{app}\rc505_launcher.exe"; WorkingDir: "{app}"
Name: "{group}\Uninstall RC505 RS"; Filename: "{uninstallexe}"
Name: "{autodesktop}\RC505 RS"; Filename: "{app}\rc505_rs.exe"; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
Filename: "{app}\rc505_rs.exe"; Description: "Open RC505 RS"; Flags: nowait postinstall skipifsilent

[Code]
var
  PathsPage: TInputDirWizardPage;
  ImportPage: TInputDirWizardPage;
  FollowProgramFolder: Boolean;
  SuggestedDataDir: String;

procedure InitializeWizard;
begin
  PathsPage := CreateInputDirPage(wpSelectDir, 'Data and download folders',
    'Keep your music separate from application updates.',
    'Projects, snapshots and replays are never removed by an update or uninstall. Downloaded installers are kept in the second folder.', False, '');
  PathsPage.Add('Project data folder:');
  PathsPage.Add('Installer download folder:');
  PathsPage.Values[0] := ExpandConstant('{param:DATADIR|}') ;
  FollowProgramFolder := (PathsPage.Values[0] = '') and (GetPreviousData('DataDir', '') = '');
  if PathsPage.Values[0] = '' then PathsPage.Values[0] := GetPreviousData('DataDir', ExpandConstant('{app}\data'));
  SuggestedDataDir := PathsPage.Values[0];
  PathsPage.Values[1] := ExpandConstant('{param:DOWNLOADDIR|}');
  if PathsPage.Values[1] = '' then PathsPage.Values[1] := GetPreviousData('DownloadDir', ExpandConstant('{userdocs}\RC505 RS Installers'));
  ImportPage := CreateInputDirPage(PathsPage.ID, 'Import existing data (optional)',
    'Copy your existing RC505 RS projects into the chosen data folder.',
    'Choose the old data folder containing projects and launcher_config.json. Leave empty to skip. Originals remain untouched; conflicting files are never overwritten.', False, '');
  ImportPage.Add('Existing data folder (optional):');
  ImportPage.Values[0] := ExpandConstant('{param:IMPORTDIR|}');
end;

procedure CurPageChanged(CurPageID: Integer);
begin
  if (CurPageID = PathsPage.ID) and FollowProgramFolder and (PathsPage.Values[0] = SuggestedDataDir) then begin
    SuggestedDataDir := AddBackslash(WizardDirValue) + 'data';
    PathsPage.Values[0] := SuggestedDataDir;
  end;
end;

procedure RegisterPreviousData(PreviousDataKey: Integer);
begin
  SetPreviousData(PreviousDataKey, 'DataDir', PathsPage.Values[0]);
  SetPreviousData(PreviousDataKey, 'DownloadDir', PathsPage.Values[1]);
end;

function JsonPath(Value: String): String;
begin
  StringChangeEx(Value, '\', '\\', True);
  StringChangeEx(Value, '"', '\"', True);
  Result := Value;
end;

procedure CurStepChanged(CurStep: TSetupStep);
var
  Settings, Parameters: String;
  ExitCode: Integer;
begin
  if CurStep = ssPostInstall then begin
    if not ForceDirectories(PathsPage.Values[0]) then RaiseException('Cannot create data folder.');
    if not ForceDirectories(PathsPage.Values[1]) then RaiseException('Cannot create download folder.');
    Settings := '{"data_dir":"' + JsonPath(PathsPage.Values[0]) + '","download_dir":"' + JsonPath(PathsPage.Values[1]) + '"}';
    if not SaveStringToFile(ExpandConstant('{app}\install-settings.json'), Utf8Encode(Settings), False) then RaiseException('Cannot save installation settings.');
    if ImportPage.Values[0] <> '' then begin
      Parameters := '--migrate-data="' + ImportPage.Values[0] + '" --data-dir="' + PathsPage.Values[0] + '"';
      if not Exec(ExpandConstant('{app}\rc505_rs.exe'), Parameters, ExpandConstant('{app}'), SW_HIDE, ewWaitUntilTerminated, ExitCode) then RaiseException('Cannot start data import.');
      if ExitCode <> 0 then RaiseException('Data import failed. Source data was preserved. Run rc505_rs.exe --migrate-data=SOURCE --data-dir=DESTINATION to inspect the error.');
    end;
  end;
end;
