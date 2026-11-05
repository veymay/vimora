; Vimora installer for Windows (Inno Setup 6).
; During setup the user chooses the role of this PC:
;   * O'qituvchi kompyuteri  -> teacher console (vimora-server.exe)
;   * O'quvchi kompyuteri    -> student agent   (vimora-client.exe), autostart, firewall, NetControl rights
;
; Build:  iscc installer\windows\vimora.iss            (expects target\release\*.exe)
;         iscc /DBinDir=..\..\target\x86_64-pc-windows-gnu\release installer\windows\vimora.iss

#define AppVer "0.1.0"
#ifndef BinDir
  #define BinDir "..\..\target\release"
#endif

[Setup]
AppId={{6F3B2C1E-8A4D-4E5F-9B7A-2C1D3E4F5A6B}
AppName=Vimora
AppVersion={#AppVer}
AppPublisher=Vimora
AppComments=Kompyuter xonalarini nazorat qilish
DefaultDirName={autopf}\Vimora
DefaultGroupName=Vimora
DisableProgramGroupPage=yes
PrivilegesRequired=admin
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=..\..\dist
OutputBaseFilename=vimora-{#AppVer}-windows-x64-setup
WizardStyle=modern
Compression=lzma2/max
SolidCompression=yes
UninstallDisplayName=Vimora
UninstallDisplayIcon={app}\vimora-server.exe

[Languages]
Name: "en"; MessagesFile: "compiler:Default.isl"
Name: "ru"; MessagesFile: "compiler:Languages\Russian.isl"

[Types]
Name: "teacher"; Description: "O'qituvchi kompyuteri — nazorat konsoli"
Name: "student"; Description: "O'quvchi kompyuteri — Vimora agenti"

[Components]
Name: "console"; Description: "Vimora konsoli (o'qituvchi)"; Types: teacher; Flags: exclusive
Name: "agent"; Description: "Vimora agenti (o'quvchi)"; Types: student; Flags: exclusive

[Files]
Source: "{#BinDir}\vimora-server.exe"; DestDir: "{app}"; Components: console; Flags: ignoreversion
Source: "{#BinDir}\vimora-client.exe"; DestDir: "{app}"; Components: agent; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Vimora"; Filename: "{app}\vimora-server.exe"; Components: console
Name: "{autodesktop}\Vimora"; Filename: "{app}\vimora-server.exe"; Components: console

[Registry]
; Agent starts in every user's session at logon (it needs the desktop for capture and lock screen).
Root: HKLM; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "VimoraAgent"; \
  ValueData: """{app}\vimora-client.exe"""; Components: agent; Flags: uninsdeletevalue

[Run]
; Teacher: accept student connections.
Filename: "{sys}\netsh.exe"; Parameters: "advfirewall firewall add rule name=""Vimora Console"" dir=in action=allow protocol=TCP localport=47801 program=""{app}\vimora-server.exe"""; \
  Components: console; Flags: runhidden; StatusMsg: "Firewall sozlanmoqda…"
; Student: receive discovery beacons, allow hosts-file NetControl, write config.
Filename: "{sys}\netsh.exe"; Parameters: "advfirewall firewall add rule name=""Vimora Agent"" dir=in action=allow protocol=UDP localport=47800 program=""{app}\vimora-client.exe"""; \
  Components: agent; Flags: runhidden; StatusMsg: "Firewall sozlanmoqda…"
; Privileged Vimora service (SYSTEM): holds the key, freeze (UWF), updates, NetControl.
Filename: "{app}\vimora-client.exe"; Parameters: "{code:SetupArgs}"; Components: agent; Flags: runhidden; StatusMsg: "Vimora xizmati o'rnatilmoqda…"
; Start now, in the user's (non-elevated) session.
Filename: "{app}\vimora-client.exe"; Description: "Agentni hozir ishga tushirish"; Components: agent; Flags: nowait postinstall runasoriginaluser
Filename: "{app}\vimora-server.exe"; Description: "Vimora konsolini ochish"; Components: console; Flags: nowait postinstall skipifsilent runasoriginaluser

[UninstallRun]
Filename: "{sys}\taskkill.exe"; Parameters: "/F /IM vimora-client.exe"; Flags: runhidden; RunOnceId: "KillAgent"
Filename: "{sys}\taskkill.exe"; Parameters: "/F /IM vimora-server.exe"; Flags: runhidden; RunOnceId: "KillConsole"
Filename: "{sys}\netsh.exe"; Parameters: "advfirewall firewall delete rule name=""Vimora Console"""; Flags: runhidden; RunOnceId: "FwConsole"
Filename: "{sys}\netsh.exe"; Parameters: "advfirewall firewall delete rule name=""Vimora Agent"""; Flags: runhidden; RunOnceId: "FwAgent"
Filename: "{sys}\schtasks.exe"; Parameters: "/end /tn ""Vimora Service"""; Flags: runhidden; RunOnceId: "StopSvc"
Filename: "{sys}\schtasks.exe"; Parameters: "/delete /tn ""Vimora Service"" /f"; Flags: runhidden; RunOnceId: "DelSvc"
Filename: "{sys}\schtasks.exe"; Parameters: "/delete /tn ""Vimora Service Watchdog"" /f"; Flags: runhidden; RunOnceId: "DelWatchdog"

[UninstallDelete]
Type: filesandordirs; Name: "{commonappdata}\Vimora"

[Code]
var
  ClassPage: TInputQueryWizardPage;

procedure InitializeWizard;
begin
  ClassPage := CreateInputQueryPage(wpSelectComponents,
    'Ulanish sozlamalari',
    'Bu kompyuter o''qituvchi konsoliga qanday ulanadi?',
    'Ulanish kaliti o''qituvchi konsolidagi (Sozlamalar) bilan bir xil bo''lishi kerak. Xonaga biriktirish keyin konsolda qilinadi.');
  ClassPage.Add('Ulanish kaliti:', True);
  ClassPage.Add('Konsol manzili (ixtiyoriy, masalan 192.168.1.10:47801):', False);
  ClassPage.Add('Kompyuter nomi (ixtiyoriy, masalan PC-12):', False);
end;

function ShouldSkipPage(PageID: Integer): Boolean;
begin
  Result := (PageID = ClassPage.ID) and not WizardIsComponentSelected('agent');
end;

function NextButtonClick(CurPageID: Integer): Boolean;
begin
  Result := True;
  if (CurPageID = ClassPage.ID) and (Trim(ClassPage.Values[0]) = '') then
  begin
    MsgBox('Ulanish kalitini kiriting.', mbError, MB_OK);
    Result := False;
  end;
end;

function Quote(S: String): String;
begin
  StringChangeEx(S, '"', '', True);
  Result := '"' + S + '"';
end;

function SetupArgs(Param: String): String;
begin
  Result := '--key ' + Quote(ClassPage.Values[0]);
  if Trim(ClassPage.Values[1]) <> '' then
    Result := Result + ' --server ' + Quote(Trim(ClassPage.Values[1]));
  if Trim(ClassPage.Values[2]) <> '' then
    Result := Result + ' --name ' + Quote(Trim(ClassPage.Values[2]));
  Result := Result + ' setup';
end;
