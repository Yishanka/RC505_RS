{ Compiled only into InstallerSmokeTest. These checks manipulate the actual
  controls and page handlers, without creating folders or installing files.
  The script's silent installs additionally exercise Inno's real page order. }

procedure AssertInstallerPath(Condition: Boolean; Description: String);
begin
  if not Condition then RaiseException('Installer path regression: ' + Description);
end;

procedure RunInstallerPathTests;
var
  OriginalProgram, OriginalData, OriginalDownloads, Root, CustomData, CustomDownloads, Memo: String;
  OriginalFollow, OriginalImport, Accepted, NeedsRestart: Boolean;
begin
  OriginalProgram := WizardDirValue;
  OriginalData := PathsPage.Values[0];
  OriginalDownloads := PathsPage.Values[1];
  OriginalFollow := FollowProgramFolder;
  OriginalImport := ImportChoicePage.Values[0];
  Root := ExpandConstant('{src}\path-policy-fixture');
  CustomData := Root + '\my music';
  CustomDownloads := Root + '\my installers';
  try
    InitializePathValues('', '', '', '');
    AssertInstallerPath(FollowProgramFolder and (PathsPage.Values[0] = ''), 'first install defers its default until the destination is ready');
    WizardForm.DirEdit.Text := Root + '\first program';
    Accepted := NextButtonClick(wpSelectDir);
    AssertInstallerPath(Accepted and (PathsPage.Values[0] = Root + '\first program\data'), 'silent Next derives data from the chosen destination');
    WizardForm.DirEdit.Text := Root + '\second program';
    CurPageChanged(PathsPage.ID);
    AssertInstallerPath(PathsPage.Values[0] = Root + '\second program\data', 'Back and destination change updates the default');

    { Setting Text exercises the same OnChange as typing and Browse. }
    PathsPage.Edits[0].Text := CustomData;
    AssertInstallerPath(not FollowProgramFolder, 'manual data edit disables automatic follow');
    WizardForm.DirEdit.Text := Root + '\third program';
    Accepted := NextButtonClick(wpSelectDir);
    CurPageChanged(PathsPage.ID);
    AssertInstallerPath(PathsPage.Values[0] = CustomData, 'manual data choice survives returning through the destination page');

    InitializePathValues('', CustomData, '', CustomDownloads);
    Accepted := NextButtonClick(wpSelectDir);
    AssertInstallerPath((not FollowProgramFolder) and (PathsPage.Values[0] = CustomData) and
      (PathsPage.Values[1] = CustomDownloads), 'reinstall preserves both saved custom paths');
    InitializePathValues(Root + '\update data', CustomData, Root + '\update downloads', CustomDownloads);
    Accepted := NextButtonClick(wpSelectDir);
    AssertInstallerPath((PathsPage.Values[0] = Root + '\update data') and
      (PathsPage.Values[1] = Root + '\update downloads'), 'explicit update arguments override saved paths');

    InitializePathValues('', '', '', '');
    WizardForm.DirEdit.Text := Root + '\final program';
    NeedsRestart := False;
    AssertInstallerPath(PrepareToInstall(NeedsRestart) = '', 'pre-install fallback succeeds even when the path page is skipped');
    AssertInstallerPath(PathsPage.Values[0] = Root + '\final program\data', 'pre-install fallback uses the final destination');
    ImportChoicePage.Values[0] := False;
    Memo := UpdateReadyMemo('  ', #13#10, '', 'Destination location:' + #13#10 + WizardDirValue, '', '', '', '');
    AssertInstallerPath((Pos(WizardDirValue, Memo) > 0) and (Pos(PathsPage.Values[0], Memo) > 0) and
      (Pos(PathsPage.Values[1], Memo) > 0) and (Pos('Import existing data: Skip', Memo) > 0), 'ready summary shows the final paths and skipped import');
    ImportChoicePage.Values[0] := True;
    Memo := UpdateReadyMemo('  ', #13#10, '', '', '', '', '', '');
    AssertInstallerPath(Pos(ImportPage.Values[0], Memo) > 0, 'ready summary includes the selected import source');
    Log('RC505 installer path checks passed: first install, destination changes, manual choice, saved paths, explicit update overrides, prepare fallback and ready summary.');
  finally
    WizardForm.DirEdit.Text := OriginalProgram;
    SetDataPath(OriginalData);
    PathsPage.Values[1] := OriginalDownloads;
    FollowProgramFolder := OriginalFollow;
    ImportChoicePage.Values[0] := OriginalImport;
  end;
end;
