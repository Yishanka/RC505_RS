#ifndef AppVersion
  #define AppVersion "0.2.0"
#endif
#ifndef SourceRoot
  #define SourceRoot ".."
#endif

[Setup]
#ifdef InstallerSmokeTest
AppId=RC505-RS-Installer-Smoke
Uninstallable=no
#else
AppId={{D70BC68C-93D0-4FA4-A660-7552396837B5}
Uninstallable=yes
CreateUninstallRegKey=yes
UninstallDisplayName=RC505 RS
#endif
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
SetupIconFile={#SourceRoot}\assets\rc505-rs-icon-v1.ico
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
Source: "{#SourceRoot}\scripts\uninstall-data.ps1"; DestDir: "{app}\scripts"; Flags: ignoreversion

[UninstallDelete]
Type: files; Name: "{app}\install-settings.json"
Type: files; Name: "{app}\uninstall-paths.ini"

[Icons]
#ifndef InstallerSmokeTest
Name: "{group}\RC505 RS"; Filename: "{app}\rc505_rs.exe"; WorkingDir: "{app}"
Name: "{group}\Audio setup"; Filename: "{app}\rc505_launcher.exe"; WorkingDir: "{app}"
Name: "{group}\Uninstall RC505 RS"; Filename: "{uninstallexe}"
Name: "{autodesktop}\RC505 RS"; Filename: "{app}\rc505_rs.exe"; WorkingDir: "{app}"; Tasks: desktopicon
#endif

[Run]
Filename: "{app}\rc505_rs.exe"; Description: "Open RC505 RS"; Flags: nowait postinstall skipifsilent

[Code]
var
  PathsPage: TInputDirWizardPage;
  ImportPage: TInputDirWizardPage;
  ImportChoicePage: TInputOptionWizardPage;
  FollowProgramFolder: Boolean;
  SuggestedDataDir: String;
  DeleteDataOnUninstall: Boolean;
  ConfirmedUninstallDataDir: String;

procedure InitializeWizard;
begin
  PathsPage := CreateInputDirPage(wpSelectDir, 'Data and download folders',
    'Keep your music separate from application updates.',
    'Updates preserve your data. Uninstall also preserves it unless you explicitly choose to delete application data. Downloaded installers stay in the second folder.', False, '');
  PathsPage.Add('Project data folder:');
  PathsPage.Add('Installer download folder:');
  PathsPage.Values[0] := ExpandConstant('{param:DATADIR|}') ;
  FollowProgramFolder := (PathsPage.Values[0] = '') and (GetPreviousData('DataDir', '') = '');
  if PathsPage.Values[0] = '' then PathsPage.Values[0] := GetPreviousData('DataDir', ExpandConstant('{app}\data'));
  SuggestedDataDir := PathsPage.Values[0];
  PathsPage.Values[1] := ExpandConstant('{param:DOWNLOADDIR|}');
  if PathsPage.Values[1] = '' then PathsPage.Values[1] := GetPreviousData('DownloadDir', ExpandConstant('{userdocs}\RC505 RS Installers'));
  ImportChoicePage := CreateInputOptionPage(PathsPage.ID, 'Import existing data (optional)',
    'Would you like to copy projects from a previous installation?',
    'Leave this option unchecked for a new installation or a normal update. Existing data in your chosen data folder is preserved.', False, False);
  ImportChoicePage.Add('Copy existing RC505 RS projects and settings');
  ImportChoicePage.Values[0] := ExpandConstant('{param:IMPORTDIR|}') <> '';
  ImportPage := CreateInputDirPage(ImportChoicePage.ID, 'Choose existing data',
    'Copy your existing RC505 RS projects into the chosen data folder.',
    'Choose the old data folder containing projects and launcher_config.json. Originals remain untouched; conflicting files are never overwritten.', False, '');
  ImportPage.Add('Existing data folder:');
  ImportPage.Values[0] := ExpandConstant('{param:IMPORTDIR|}');
  if ImportPage.Values[0] = '' then ImportPage.Values[0] := ExpandConstant('{userappdata}\rc505_rs');
end;

function ShouldSkipPage(PageID: Integer): Boolean;
begin
  Result := (PageID = ImportPage.ID) and not ImportChoicePage.Values[0];
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
    Settings := '{"product":"RC505 RS","schema":1,"data_dir":"' + JsonPath(ExpandFileName(PathsPage.Values[0])) + '"}';
    if not SaveStringToFile(AddBackslash(PathsPage.Values[0]) + '.rc505-rs-data.json', Utf8Encode(Settings), False) then RaiseException('Cannot mark the application data directory.');
    if ImportChoicePage.Values[0] then begin
      Parameters := '--migrate-data="' + ImportPage.Values[0] + '" --data-dir="' + PathsPage.Values[0] + '"';
      if not Exec(ExpandConstant('{app}\rc505_rs.exe'), Parameters, ExpandConstant('{app}'), SW_HIDE, ewWaitUntilTerminated, ExitCode) then RaiseException('Cannot start data import.');
      if ExitCode <> 0 then RaiseException('Data import failed. Source data was preserved. Run rc505_rs.exe --migrate-data=SOURCE --data-dir=DESTINATION to inspect the error.');
    end;
  end;
end;

function RunUninstallData(Mode: String; ConfirmDelete: Boolean): Boolean;
var
  Params, Report, MessageText: String;
  RawMessage: AnsiString;
  ExitCode: Integer;
begin
  Report := ExpandConstant('{tmp}\rc505-uninstall-result.txt');
  DeleteFile(Report);
  Params := '-NoProfile -NonInteractive -ExecutionPolicy Bypass -File "' + ExpandConstant('{app}\scripts\uninstall-data.ps1') + '" -Mode ' + Mode + ' -InstallDir "' + ExpandConstant('{app}') + '" -ReportFile "' + Report + '"';
  if ConfirmDelete then Params := Params + ' -ConfirmDelete -ExpectedDataDir "' + ConfirmedUninstallDataDir + '"';
  Result := Exec(ExpandConstant('{sys}\WindowsPowerShell\v1.0\powershell.exe'), Params, ExpandConstant('{app}'), SW_HIDE, ewWaitUntilTerminated, ExitCode) and (ExitCode = 0);
  if Result and (Mode = 'Check') then begin
    ConfirmedUninstallDataDir := '';
    if LoadStringFromFile(Report, RawMessage) then ConfirmedUninstallDataDir := Utf8Decode(RawMessage);
  end;
  if not Result then begin
    if LoadStringFromFile(Report, RawMessage) then MessageText := Utf8Decode(RawMessage)
    else MessageText := 'Unable to verify or remove application data. Close RC505 RS and try again.';
    Log(MessageText);
    SuppressibleMsgBox(MessageText, mbError, MB_OK, IDOK);
  end;
end;

function InitializeUninstall: Boolean;
var
  DataDir: String;
  DeleteData: Boolean;
begin
  Result := RunUninstallData('Check', False);
  if not Result then Exit;
  DataDir := ConfirmedUninstallDataDir;
  DeleteData := ExpandConstant('{param:DELETEDATA|0}') = '1';
  if (DataDir <> '') and not UninstallSilent and not DeleteData then
    DeleteData := SuppressibleMsgBox('Delete RC505 RS projects, snapshots, replays, sounds and settings in:' + #13#10 + DataDir + #13#10#13#10 + 'Choose No to keep your music (recommended). Yes permanently deletes the application-owned folders, including data shared with other installations. Other files and downloaded installers are kept.', mbConfirmation, MB_YESNO or MB_DEFBUTTON2, IDNO) = IDYES;
  DeleteDataOnUninstall := DeleteData and (DataDir <> '');
  if DataDir = '' then Log('Cannot resolve current data directory. Application data will be preserved.');
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if (CurUninstallStep = usUninstall) and DeleteDataOnUninstall then
    if not RunUninstallData('Delete', True) then RaiseException('Application data removal failed. Uninstall stopped.');
end;
