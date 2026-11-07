; Vimora installer for Windows (Inno Setup 6).
; During setup the user chooses the role of this PC:
;   * Teacher computer -> console (vimora-server.exe)
;   * Student computer -> agent (vimora-client.exe), autostart, firewall, privileged service
;
; Build:  iscc installer\windows\vimora.iss                       (expects target\release\*.exe)
;         iscc /DAppVer=0.2.0 /DBinDir=..\..\target\release installer\windows\vimora.iss
;
; Silent self-update (run by Vimora itself):
;   console: setup.exe /SILENT /TYPE=teacher /COMPONENTS=console
;   agent:   setup.exe /VERYSILENT /TYPE=student /COMPONENTS=agent   (as SYSTEM, no prompts)

#ifndef AppVer
  #define AppVer "0.3.2"
#endif
#ifndef BinDir
  #define BinDir "..\..\target\release"
#endif

[Setup]
AppId={{6F3B2C1E-8A4D-4E5F-9B7A-2C1D3E4F5A6B}
AppName=Vimora
AppVersion={#AppVer}
AppVerName=Vimora {#AppVer}
AppPublisher=VEYMAY
AppPublisherURL=https://github.com/veymay/vimora
AppSupportURL=https://github.com/veymay/vimora/issues
AppUpdatesURL=https://github.com/veymay/vimora/releases
DefaultDirName={autopf}\Vimora
DefaultGroupName=Vimora
DisableProgramGroupPage=yes
PrivilegesRequired=admin
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=..\..\dist
OutputBaseFilename=vimora-{#AppVer}-windows-x64-setup
SetupIconFile=vimora.ico
WizardStyle=modern
Compression=lzma2/max
SolidCompression=yes
UninstallDisplayName=Vimora
UninstallDisplayIcon={app}\vimora-server.exe
; Vimora stops its own processes before files are replaced (see PrepareToInstall).
CloseApplications=no

[Languages]
Name: "en"; MessagesFile: "compiler:Default.isl"
Name: "ru"; MessagesFile: "compiler:Languages\Russian.isl"

[CustomMessages]
en.TypeTeacher=Teacher computer — control console
en.TypeStudent=Student computer — Vimora agent
en.CompConsole=Vimora console (teacher)
en.CompAgent=Vimora agent (student)
en.Firewall=Configuring the firewall…
en.Service=Installing the Vimora service…
en.StartAgent=Start the agent now
en.OpenConsole=Open the Vimora console
ru.TypeTeacher=Компьютер учителя — консоль управления
ru.TypeStudent=Компьютер ученика — агент Vimora
ru.CompConsole=Консоль Vimora (учитель)
ru.CompAgent=Агент Vimora (ученик)
ru.Firewall=Настройка брандмауэра…
ru.Service=Установка службы Vimora…
ru.StartAgent=Запустить агент сейчас
ru.OpenConsole=Открыть консоль Vimora

[Types]
Name: "teacher"; Description: "{cm:TypeTeacher}"
Name: "student"; Description: "{cm:TypeStudent}"

[Components]
Name: "console"; Description: "{cm:CompConsole}"; Types: teacher; Flags: exclusive
Name: "agent"; Description: "{cm:CompAgent}"; Types: student; Flags: exclusive

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
Filename: "{sys}\netsh.exe"; Parameters: "advfirewall firewall delete rule name=""Vimora Console"""; Components: console; Flags: runhidden
Filename: "{sys}\netsh.exe"; Parameters: "advfirewall firewall add rule name=""Vimora Console"" dir=in action=allow protocol=TCP localport=47801 program=""{app}\vimora-server.exe"""; \
  Components: console; Flags: runhidden; StatusMsg: "{cm:Firewall}"
; Student: receive discovery beacons.
Filename: "{sys}\netsh.exe"; Parameters: "advfirewall firewall delete rule name=""Vimora Agent"""; Components: agent; Flags: runhidden
Filename: "{sys}\netsh.exe"; Parameters: "advfirewall firewall add rule name=""Vimora Agent"" dir=in action=allow protocol=UDP localport=47800 program=""{app}\vimora-client.exe"""; \
  Components: agent; Flags: runhidden; StatusMsg: "{cm:Firewall}"
; Privileged Vimora service (SYSTEM): pins the paired console, Shadow (UWF), updates, NetControl.
Filename: "{app}\vimora-client.exe"; Parameters: "setup"; Components: agent; Flags: runhidden; StatusMsg: "{cm:Service}"
; Start now, in the user's (non-elevated) session. Not during a silent self-update: that runs as
; SYSTEM, and the running agents restart themselves when their file changes.
Filename: "{app}\vimora-client.exe"; Description: "{cm:StartAgent}"; Components: agent; Flags: nowait postinstall runasoriginaluser skipifsilent
; The console comes back after its own update too (no skipifsilent).
Filename: "{app}\vimora-server.exe"; Description: "{cm:OpenConsole}"; Components: console; Flags: nowait postinstall runasoriginaluser

[UninstallRun]
Filename: "{sys}\taskkill.exe"; Parameters: "/F /IM vimora-client.exe"; Flags: runhidden; RunOnceId: "KillAgent"
Filename: "{sys}\taskkill.exe"; Parameters: "/F /IM vimora-server.exe"; Flags: runhidden; RunOnceId: "KillConsole"
Filename: "{sys}\netsh.exe"; Parameters: "advfirewall firewall delete rule name=""Vimora Console"""; Flags: runhidden; RunOnceId: "FwConsole"
Filename: "{sys}\netsh.exe"; Parameters: "advfirewall firewall delete rule name=""Vimora Agent"""; Flags: runhidden; RunOnceId: "FwAgent"
Filename: "{sys}\schtasks.exe"; Parameters: "/end /tn ""Vimora Service"""; Flags: runhidden; RunOnceId: "StopSvc"
Filename: "{sys}\schtasks.exe"; Parameters: "/delete /tn ""Vimora Service"" /f"; Flags: runhidden; RunOnceId: "DelSvc"
Filename: "{sys}\schtasks.exe"; Parameters: "/delete /tn ""Vimora Service Watchdog"" /f"; Flags: runhidden; RunOnceId: "DelWatchdog"
Filename: "{sys}\schtasks.exe"; Parameters: "/delete /tn ""Vimora Update"" /f"; Flags: runhidden; RunOnceId: "DelUpdate"

[UninstallDelete]
Type: filesandordirs; Name: "{commonappdata}\Vimora"
Type: files; Name: "{app}\*.old.exe"

[Code]
// Running executables can't be overwritten but can be renamed: move them aside so the new files
// can be written. Agents keep running from the old file and restart themselves on the new one;
// the service is stopped here and started again by "vimora-client setup".
procedure MoveAside(Name: String);
var
  Cur, Old: String;
begin
  Cur := ExpandConstant('{app}\') + Name + '.exe';
  Old := ExpandConstant('{app}\') + Name + '.old.exe';
  DeleteFile(Old);
  if FileExists(Cur) then
    RenameFile(Cur, Old);
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  Code: Integer;
begin
  Exec(ExpandConstant('{sys}\schtasks.exe'), '/end /tn "Vimora Service"', '', SW_HIDE, ewWaitUntilTerminated, Code);
  MoveAside('vimora-client');
  MoveAside('vimora-server');
  Result := '';
end;
