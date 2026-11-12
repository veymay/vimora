; Vimora installer for Windows (NSIS 3) — builds on Linux too (`makensis`), no Windows needed.
;
; Two programs, one installer: the teacher program and the student program. Everything goes into
; one folder, C:\Program Files (x86)\Vimora (fixed, no folder page).
;
; Interactive: the installer starts Vimora's own small setup window (`vimora-student.exe
; setup-wizard`): language (with flags) -> teacher or student -> it runs this installer again
; silently and shows the progress. If that window can't open, the plain pages below are used.
;
; Silent (Vimora's updates and the setup window use these):
;   student:  setup.exe /VERYSILENT /SUPPRESSMSGBOXES /NORESTART /TYPE=student /COMPONENTS=agent
;   teacher:  setup.exe /SILENT /SUPPRESSMSGBOXES /NORESTART /TYPE=teacher /COMPONENTS=teacher
;   also:     /LANG=uz|ru|en  (program language)   /LAUNCH  (start the program afterwards)
; Old spellings stay accepted: /COMPONENTS=console (teacher), /COMPONENTS=agent (student).
;
; Up to 0.3.27 Vimora lived in C:\Program Files\Vimora (vimora-client.exe / vimora-server.exe).
; Such a computer is moved here — except a frozen student computer (UWF / Shadow Defender): it
; is updated in place, because a move would be thrown away at its next restart. The student
; service moves it once the freeze is off.
;
; A student computer's Vimora can only be removed by the teacher (uninstall.exe /BYTEACHER).
;
; Build:  makensis -DVERSION=0.3.28 -DBINDIR=target/x86_64-pc-windows-gnu/release installer/windows/vimora.nsi

!ifndef VERSION
  !define VERSION "0.0.0"
!endif
!ifndef BINDIR
  !define BINDIR "..\..\target\release"
!endif
!ifndef OUTFILE
  !define OUTFILE "..\..\dist\vimora-${VERSION}-windows-x64-setup.exe"
!endif

Unicode true
Name "Vimora"
OutFile "${OUTFILE}"
InstallDir "$PROGRAMFILES32\Vimora"
RequestExecutionLevel admin
SetCompressor /SOLID lzma
BrandingText "Vimora ${VERSION}"
ShowInstDetails nevershow
ShowUninstDetails nevershow

VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "Vimora"
VIAddVersionKey "FileDescription" "Vimora setup"
VIAddVersionKey "CompanyName" "VEYMAY"
VIAddVersionKey "LegalCopyright" "(c) 2026 VEYMAY"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "ProductVersion" "${VERSION}"

!include "MUI2.nsh"
!include "nsDialogs.nsh"
!include "LogicLib.nsh"
!include "FileFunc.nsh"
!include "x64.nsh"

!define MUI_ICON "vimora.ico"
!define MUI_UNICON "vimora.ico"
!define MUI_ABORTWARNING

!define RUNKEY "Software\Microsoft\Windows\CurrentVersion\Run"
!define UNKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\Vimora"
!define LEGACY "$PROGRAMFILES64\Vimora"

Var Role        ; "student" | "teacher"
Var Silent
Var Lang        ; "uz" | "ru" | "en" | ""
Var Launch
Var ExeName     ; the student program's file name (old name when updated in place)
Var InPlace
Var ByTeacher
Var RadioStudent
Var RadioTeacher

; --- pages (only when Vimora's own setup window could not open) -------------------------------
Page custom RolePage RolePageLeave
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "Uzbek"
!insertmacro MUI_LANGUAGE "Russian"
!insertmacro MUI_LANGUAGE "English"

LangString RoleTitle ${LANG_UZBEK} "Vimora"
LangString RoleTitle ${LANG_RUSSIAN} "Vimora"
LangString RoleTitle ${LANG_ENGLISH} "Vimora"
LangString RoleSub ${LANG_UZBEK} "Qaysi dastur o'rnatilsin?"
LangString RoleSub ${LANG_RUSSIAN} "Какую программу установить?"
LangString RoleSub ${LANG_ENGLISH} "Which program?"
LangString TypeStudent ${LANG_UZBEK} "O'quvchi — sinfdagi kompyuter"
LangString TypeStudent ${LANG_RUSSIAN} "Ученик — компьютер в классе"
LangString TypeStudent ${LANG_ENGLISH} "Student — a classroom computer"
LangString TypeTeacher ${LANG_UZBEK} "O'qituvchi — sinfni boshqarish"
LangString TypeTeacher ${LANG_RUSSIAN} "Учитель — управление классом"
LangString TypeTeacher ${LANG_ENGLISH} "Teacher — runs the classroom"
LangString UnTeacherOnly ${LANG_UZBEK} "Vimora'ni faqat o'qituvchi o'chira oladi."
LangString UnTeacherOnly ${LANG_RUSSIAN} "Удалить Vimora может только учитель."
LangString UnTeacherOnly ${LANG_ENGLISH} "Only the teacher can remove Vimora."
LangString UnFrozen ${LANG_UZBEK} "Bu kompyuter muzlatilgan. Muzlatish o'chirildi: kompyuter qayta yoqilgach Vimora o'zi o'chiriladi."
LangString UnFrozen ${LANG_RUSSIAN} "Компьютер заморожен. Заморозка отключена: после перезагрузки Vimora удалится сама."
LangString UnFrozen ${LANG_ENGLISH} "This computer is frozen. The freeze is off now: Vimora removes itself after the restart."

; --- start-up: command line, then Vimora's own setup window ------------------------------------
Function .onInit
  StrCpy $Role "student"
  StrCpy $Silent "0"
  StrCpy $Lang ""
  StrCpy $Launch "0"
  ${GetParameters} $R0
  ClearErrors
  ${GetOptions} $R0 "/TYPE=" $R1
  ${IfNot} ${Errors}
    ${If} $R1 == "teacher"
      StrCpy $Role "teacher"
    ${EndIf}
  ${EndIf}
  ClearErrors
  ${GetOptions} $R0 "/COMPONENTS=" $R1
  ${IfNot} ${Errors}
    ${If} $R1 == "teacher"
    ${OrIf} $R1 == "console"
      StrCpy $Role "teacher"
    ${ElseIf} $R1 == "student"
    ${OrIf} $R1 == "agent"
      StrCpy $Role "student"
    ${EndIf}
  ${EndIf}
  ClearErrors
  ${GetOptions} $R0 "/LANG=" $R1
  ${IfNot} ${Errors}
    StrCpy $Lang $R1
  ${EndIf}
  ClearErrors
  ${GetOptions} $R0 "/LAUNCH" $R1
  ${IfNot} ${Errors}
    StrCpy $Launch "1"
  ${EndIf}
  ClearErrors
  ${GetOptions} $R0 "/VERYSILENT" $R1
  ${IfNot} ${Errors}
    StrCpy $Silent "1"
  ${EndIf}
  ClearErrors
  ${GetOptions} $R0 "/SILENT" $R1
  ${IfNot} ${Errors}
    StrCpy $Silent "1"
  ${EndIf}
  ${If} ${Silent}
    StrCpy $Silent "1"
  ${EndIf}
  ${If} $Lang == "ru"
    StrCpy $LANGUAGE ${LANG_RUSSIAN}
  ${ElseIf} $Lang == "en"
    StrCpy $LANGUAGE ${LANG_ENGLISH}
  ${ElseIf} $Lang == "uz"
    StrCpy $LANGUAGE ${LANG_UZBEK}
  ${EndIf}
  ${If} ${RunningX64}
    SetRegView 64
    ${DisableX64FSRedirection}
  ${EndIf}
  ; The student program also serves as the setup window and the freeze check.
  InitPluginsDir
  SetOutPath "$PLUGINSDIR"
  File "${BINDIR}\vimora-student.exe"
  ${If} $Silent == "1"
    SetSilent silent
  ${Else}
    ; 0 = installed, 1 = closed: done. Anything else (no window possible): the pages below.
    ExecWait '"$PLUGINSDIR\vimora-student.exe" setup-wizard "$EXEPATH"' $0
    ${If} $0 == 0
    ${OrIf} $0 == 1
      Quit
    ${EndIf}
  ${EndIf}
FunctionEnd

Function RolePage
  ${If} $Silent == "1"
    Abort
  ${EndIf}
  !insertmacro MUI_HEADER_TEXT "$(RoleTitle)" "$(RoleSub)"
  nsDialogs::Create 1018
  Pop $0
  ${NSD_CreateRadioButton} 10u 18u 280u 14u "$(TypeStudent)"
  Pop $RadioStudent
  ${NSD_CreateRadioButton} 10u 40u 280u 14u "$(TypeTeacher)"
  Pop $RadioTeacher
  ${If} $Role == "teacher"
    ${NSD_Check} $RadioTeacher
  ${Else}
    ${NSD_Check} $RadioStudent
  ${EndIf}
  nsDialogs::Show
FunctionEnd

Function RolePageLeave
  ${NSD_GetState} $RadioTeacher $0
  ${If} $0 == ${BST_CHECKED}
    StrCpy $Role "teacher"
  ${Else}
    StrCpy $Role "student"
  ${EndIf}
  StrCpy $Launch "1"
FunctionEnd

; --- install -------------------------------------------------------------------------------------
Section "Vimora"
  ${If} $Role == "teacher"
    Call InstallTeacher
  ${Else}
    Call InstallStudent
  ${EndIf}
  ; The language both programs start in (chosen in the setup window).
  ${If} $Lang != ""
    WriteRegStr HKLM "Software\Vimora" "Language" $Lang
  ${EndIf}
SectionEnd

; Running programs can't be overwritten but can be renamed: move them aside, the new files go in.
Function InstallStudent
  StrCpy $ExeName "vimora-student.exe"
  StrCpy $InPlace "0"
  ; A student computer still in the old folder: updated there if it is frozen (or if that can't
  ; be told — the safe way), moved here otherwise.
  ${If} ${FileExists} "${LEGACY}\vimora-client.exe"
  ${AndIf} "${LEGACY}" != "$INSTDIR"
    nsExec::Exec '"$PLUGINSDIR\vimora-student.exe" is-frozen'
    Pop $0
    ${If} $0 != "1"
      StrCpy $INSTDIR "${LEGACY}"
      StrCpy $ExeName "vimora-client.exe"
      StrCpy $InPlace "1"
    ${EndIf}
  ${EndIf}
  SetOutPath "$INSTDIR"
  ; Keep the watchdog from restarting the old service while files are swapped.
  nsExec::Exec '"$SYSDIR\schtasks.exe" /change /tn "Vimora Service Watchdog" /disable'
  Pop $0
  nsExec::Exec '"$SYSDIR\schtasks.exe" /end /tn "Vimora Service"'
  Pop $0
  Sleep 1500
  Call DeleteOldFiles
  System::Call 'kernel32::GetTickCount() i .r9'
  ${If} ${FileExists} "$INSTDIR\$ExeName"
    Rename "$INSTDIR\$ExeName" "$INSTDIR\vimora-student.old-$9.exe"
  ${EndIf}
  File "/oname=$ExeName" "${BINDIR}\vimora-student.exe"
  File "/oname=vimora.ico" "vimora.ico"
  WriteUninstaller "$INSTDIR\uninstall.exe"
  ; The program registers itself: service, sign-in start, firewall, update checks, the files
  ; folder and its (hidden) entry in the program list.
  nsExec::Exec /TIMEOUT=180000 '"$INSTDIR\$ExeName" setup'
  Pop $0
  ${If} $InPlace != "1"
    Call RemoveLegacyStudent
  ${EndIf}
  nsExec::Exec '"$SYSDIR\schtasks.exe" /change /tn "Vimora Service Watchdog" /enable'
  Pop $0
  ${If} $Launch == "1"
    Exec '"$WINDIR\explorer.exe" "$INSTDIR\$ExeName"'
  ${EndIf}
FunctionEnd

; The 0.3.27 student program in C:\Program Files\Vimora (this computer was just moved here).
Function RemoveLegacyStudent
  ${If} "${LEGACY}" == "$INSTDIR"
    Return
  ${EndIf}
  ${IfNot} ${FileExists} "${LEGACY}\vimora-client.exe"
    Return
  ${EndIf}
  nsExec::Exec '"$SYSDIR\taskkill.exe" /F /IM vimora-client.exe'
  Pop $0
  Sleep 1000
  ${If} ${FileExists} "${LEGACY}\vimora-server.exe"
    ; A teacher program lives there too: only the student's files go.
    Delete /REBOOTOK "${LEGACY}\vimora-client.exe"
  ${Else}
    RMDir /r /REBOOTOK "${LEGACY}"
  ${EndIf}
FunctionEnd

Function InstallTeacher
  SetOutPath "$INSTDIR"
  System::Call 'kernel32::GetTickCount() i .r9'
  ${If} ${FileExists} "$INSTDIR\vimora-teacher.exe"
    Rename "$INSTDIR\vimora-teacher.exe" "$INSTDIR\vimora-teacher.old-$9.exe"
  ${EndIf}
  File "${BINDIR}\vimora-teacher.exe"
  File "/oname=vimora.ico" "vimora.ico"
  WriteUninstaller "$INSTDIR\uninstall.exe"
  ; Student computers connect on TCP 47801.
  nsExec::Exec '"$SYSDIR\netsh.exe" advfirewall firewall delete rule name="Vimora Console"'
  Pop $0
  nsExec::Exec '"$SYSDIR\netsh.exe" advfirewall firewall delete rule name="Vimora Teacher"'
  Pop $0
  nsExec::Exec '"$SYSDIR\netsh.exe" advfirewall firewall add rule name="Vimora Teacher" dir=in action=allow protocol=TCP localport=47801 program="$INSTDIR\vimora-teacher.exe"'
  Pop $0
  CreateShortcut "$SMPROGRAMS\Vimora.lnk" "$INSTDIR\vimora-teacher.exe"
  CreateShortcut "$DESKTOP\Vimora.lnk" "$INSTDIR\vimora-teacher.exe"
  ; Starts with Windows and keeps running (in the taskbar / tray).
  DeleteRegValue HKLM "${RUNKEY}" "VimoraConsole"
  WriteRegStr HKLM "${RUNKEY}" "VimoraTeacher" '"$INSTDIR\vimora-teacher.exe"'
  WriteRegStr HKLM "${UNKEY}" "DisplayName" "Vimora"
  WriteRegStr HKLM "${UNKEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKLM "${UNKEY}" "Publisher" "VEYMAY"
  WriteRegStr HKLM "${UNKEY}" "DisplayIcon" "$INSTDIR\vimora.ico"
  WriteRegStr HKLM "${UNKEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKLM "${UNKEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegStr HKLM "${UNKEY}" "URLInfoAbout" "https://github.com/veymay/vimora"
  WriteRegDWORD HKLM "${UNKEY}" "NoModify" 1
  WriteRegDWORD HKLM "${UNKEY}" "NoRepair" 1
  DeleteRegValue HKLM "${UNKEY}" "NoRemove"
  DeleteRegValue HKLM "${UNKEY}" "SystemComponent"
  ; The 0.3.27 teacher program in C:\Program Files\Vimora.
  ${If} "${LEGACY}" != "$INSTDIR"
  ${AndIf} ${FileExists} "${LEGACY}\vimora-server.exe"
    nsExec::Exec '"$SYSDIR\taskkill.exe" /F /IM vimora-server.exe'
    Pop $0
    Sleep 1000
    ${If} ${FileExists} "${LEGACY}\vimora-client.exe"
      Delete /REBOOTOK "${LEGACY}\vimora-server.exe"
    ${Else}
      RMDir /r /REBOOTOK "${LEGACY}"
    ${EndIf}
  ${EndIf}
  ; After its own update (silent) the program comes back; a new install starts it too.
  ${If} $Silent == "1"
  ${OrIf} $Launch == "1"
    Exec '"$WINDIR\explorer.exe" "$INSTDIR\vimora-teacher.exe"'
  ${EndIf}
FunctionEnd

; Deletes the files earlier updates moved aside (those still running are skipped).
Function DeleteOldFiles
  Delete "$INSTDIR\vimora-client.old.exe"
  Delete "$INSTDIR\vimora-server.old.exe"
  FindFirst $0 $1 "$INSTDIR\vimora-*.old-*.exe"
  ${DoWhile} $1 != ""
    Delete "$INSTDIR\$1"
    FindNext $0 $1
  ${Loop}
  FindClose $0
FunctionEnd

; --- uninstall -----------------------------------------------------------------------------------
Function un.onInit
  StrCpy $ByTeacher "0"
  ${GetParameters} $R0
  ClearErrors
  ${GetOptions} $R0 "/BYTEACHER" $R1
  ${IfNot} ${Errors}
    StrCpy $ByTeacher "1"
  ${EndIf}
  ; On a student computer only the teacher removes Vimora (the service runs this with
  ; /BYTEACHER when the teacher asks for it).
  ${If} ${FileExists} "$INSTDIR\vimora-student.exe"
  ${OrIf} ${FileExists} "$INSTDIR\vimora-client.exe"
    ${If} $ByTeacher != "1"
      MessageBox MB_OK|MB_ICONSTOP "$(UnTeacherOnly)" /SD IDOK
      Abort
    ${EndIf}
  ${EndIf}
FunctionEnd

Section "Uninstall"
  ${If} ${RunningX64}
    SetRegView 64
    ${DisableX64FSRedirection}
  ${EndIf}
  nsExec::Exec '"$SYSDIR\taskkill.exe" /F /IM vimora-teacher.exe'
  Pop $0
  nsExec::Exec '"$SYSDIR\taskkill.exe" /F /IM vimora-server.exe'
  Pop $0
  ; Nothing of Vimora may stay: settings it changed, pairing, every user's files.
  StrCpy $ExeName ""
  ${If} ${FileExists} "$INSTDIR\vimora-student.exe"
    StrCpy $ExeName "vimora-student.exe"
  ${ElseIf} ${FileExists} "$INSTDIR\vimora-client.exe"
    StrCpy $ExeName "vimora-client.exe"
  ${EndIf}
  ${If} $ExeName != ""
    nsExec::Exec '"$SYSDIR\schtasks.exe" /change /tn "Vimora Service Watchdog" /disable'
    Pop $0
    nsExec::Exec '"$SYSDIR\taskkill.exe" /F /IM $ExeName'
    Pop $0
    nsExec::ExecToLog '"$INSTDIR\$ExeName" purge'
    Pop $0
    ${If} $0 == 3
      ; Frozen (UWF / Shadow Defender): deleting now would come back after a restart. The
      ; freeze is switched off; after the restart the service finishes the uninstall.
      nsExec::Exec '"$SYSDIR\schtasks.exe" /change /tn "Vimora Service Watchdog" /enable'
      Pop $0
      nsExec::Exec '"$SYSDIR\schtasks.exe" /run /tn "Vimora Service"'
      Pop $0
      MessageBox MB_OK|MB_ICONINFORMATION "$(UnFrozen)" /SD IDOK
      SetErrorLevel 3
      Quit
    ${EndIf}
  ${EndIf}
  nsExec::Exec '"$SYSDIR\taskkill.exe" /F /IM vimora-student.exe'
  Pop $0
  nsExec::Exec '"$SYSDIR\taskkill.exe" /F /IM vimora-client.exe'
  Pop $0
  nsExec::Exec '"$SYSDIR\netsh.exe" advfirewall firewall delete rule name="Vimora Console"'
  Pop $0
  nsExec::Exec '"$SYSDIR\netsh.exe" advfirewall firewall delete rule name="Vimora Agent"'
  Pop $0
  nsExec::Exec '"$SYSDIR\netsh.exe" advfirewall firewall delete rule name="Vimora Teacher"'
  Pop $0
  nsExec::Exec '"$SYSDIR\netsh.exe" advfirewall firewall delete rule name="Vimora Student"'
  Pop $0
  nsExec::Exec '"$SYSDIR\schtasks.exe" /end /tn "Vimora Service"'
  Pop $0
  nsExec::Exec '"$SYSDIR\schtasks.exe" /delete /tn "Vimora Service" /f'
  Pop $0
  nsExec::Exec '"$SYSDIR\schtasks.exe" /delete /tn "Vimora Service Watchdog" /f'
  Pop $0
  nsExec::Exec '"$SYSDIR\schtasks.exe" /delete /tn "Vimora Update" /f'
  Pop $0
  nsExec::Exec '"$SYSDIR\schtasks.exe" /delete /tn "Vimora Updater" /f'
  Pop $0
  nsExec::Exec '"$SYSDIR\schtasks.exe" /delete /tn "Vimora Updater Boot" /f'
  Pop $0
  nsExec::Exec '"$SYSDIR\schtasks.exe" /delete /tn "Vimora Agent" /f'
  Pop $0
  nsExec::Exec '"$SYSDIR\schtasks.exe" /delete /tn "Vimora Student" /f'
  Pop $0
  nsExec::Exec '"$SYSDIR\schtasks.exe" /delete /tn "Vimora Remove" /f'
  Pop $0
  DeleteRegValue HKLM "${RUNKEY}" "VimoraAgent"
  DeleteRegValue HKLM "${RUNKEY}" "VimoraStudent"
  DeleteRegValue HKLM "${RUNKEY}" "VimoraConsole"
  DeleteRegValue HKLM "${RUNKEY}" "VimoraTeacher"
  DeleteRegKey HKLM "${UNKEY}"
  DeleteRegKey HKLM "Software\Vimora"
  Delete "$SMPROGRAMS\Vimora.lnk"
  Delete "$DESKTOP\Vimora.lnk"
  RMDir /r /REBOOTOK "$INSTDIR"
  ReadEnvStr $0 ProgramData
  ${If} $0 != ""
    RMDir /r "$0\Vimora"
  ${EndIf}
SectionEnd
