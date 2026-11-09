; Vimora installer for Windows (NSIS 3) — builds on Linux too (`makensis`), no Windows needed.
;
; It takes the same command line as the Inno Setup script it replaces, because Vimora's
; self-update starts it that way:
;   student (service, as SYSTEM):  setup.exe /VERYSILENT /SUPPRESSMSGBOXES /NORESTART /TYPE=student /COMPONENTS=agent
;   console:                       setup.exe /SILENT /SUPPRESSMSGBOXES /NORESTART /TYPE=teacher /COMPONENTS=console
; Interactive: a small window with one choice (student computer is the default).
;
; Build:  makensis -DVERSION=0.3.7 -DBINDIR=target/x86_64-pc-windows-gnu/release installer/windows/vimora.nsi

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
InstallDir "$PROGRAMFILES64\Vimora"
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
!define MUI_FINISHPAGE_NOAUTOCLOSE

Var Role            ; "student" | "teacher"
Var RadioStudent
Var RadioTeacher
Var Silent

; --- pages: role -> install -> finish ---------------------------------------------------------
Page custom RolePage RolePageLeave
!insertmacro MUI_PAGE_INSTFILES
!define MUI_FINISHPAGE_RUN
!define MUI_FINISHPAGE_RUN_FUNCTION LaunchApp
!define MUI_FINISHPAGE_RUN_TEXT "$(RunText)"
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "Uzbek"
!insertmacro MUI_LANGUAGE "Russian"

LangString RoleTitle ${LANG_UZBEK} "Kompyuter turi"
LangString RoleTitle ${LANG_RUSSIAN} "Тип компьютера"
LangString RoleSub ${LANG_UZBEK} "Bu kompyuter qanday vazifada ishlaydi?"
LangString RoleSub ${LANG_RUSSIAN} "Для чего этот компьютер?"
LangString TypeStudent ${LANG_UZBEK} "O'quvchi kompyuteri — Vimora agenti"
LangString TypeStudent ${LANG_RUSSIAN} "Компьютер ученика — агент Vimora"
LangString TypeTeacher ${LANG_UZBEK} "O'qituvchi kompyuteri — boshqaruv konsoli"
LangString TypeTeacher ${LANG_RUSSIAN} "Компьютер учителя — консоль управления"
LangString RunText ${LANG_UZBEK} "Vimora'ni hozir ishga tushirish"
LangString RunText ${LANG_RUSSIAN} "Запустить Vimora сейчас"

; --- start-up: command line (silent flags, role) ----------------------------------------------
Function .onInit
  StrCpy $Role "student"
  StrCpy $Silent "0"
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
    ${If} $R1 == "console"
      StrCpy $Role "teacher"
    ${ElseIf} $R1 == "agent"
      StrCpy $Role "student"
    ${EndIf}
  ${EndIf}
  ; Inno-style silent switches (what Vimora's self-update passes).
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
  ${If} $Silent == "1"
    SetSilent silent
  ${EndIf}
  ${If} ${RunningX64}
    SetRegView 64
    ${DisableX64FSRedirection}
  ${EndIf}
FunctionEnd

; --- the one choice ----------------------------------------------------------------------------
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
FunctionEnd

; --- install -------------------------------------------------------------------------------------
; Running programs can't be overwritten but can be renamed: move them aside, the new files go in.
; Agents keep running from the old file and restart themselves on the new one; the service is
; stopped here and started again by "vimora-client setup".
Section "Vimora"
  SetOutPath "$INSTDIR"
  nsExec::Exec '"$SYSDIR\schtasks.exe" /end /tn "Vimora Service"'
  Pop $0
  Delete "$INSTDIR\vimora-client.old.exe"
  Delete "$INSTDIR\vimora-server.old.exe"
  IfFileExists "$INSTDIR\vimora-client.exe" 0 +2
    Rename "$INSTDIR\vimora-client.exe" "$INSTDIR\vimora-client.old.exe"
  IfFileExists "$INSTDIR\vimora-server.exe" 0 +2
    Rename "$INSTDIR\vimora-server.exe" "$INSTDIR\vimora-server.old.exe"

  ${If} $Role == "teacher"
    File "${BINDIR}\vimora-server.exe"
    ; Accept student connections.
    nsExec::Exec '"$SYSDIR\netsh.exe" advfirewall firewall delete rule name="Vimora Console"'
    Pop $0
    nsExec::Exec '"$SYSDIR\netsh.exe" advfirewall firewall add rule name="Vimora Console" dir=in action=allow protocol=TCP localport=47801 program="$INSTDIR\vimora-server.exe"'
    Pop $0
    CreateShortcut "$SMPROGRAMS\Vimora.lnk" "$INSTDIR\vimora-server.exe"
    CreateShortcut "$DESKTOP\Vimora.lnk" "$INSTDIR\vimora-server.exe"
  ${Else}
    File "${BINDIR}\vimora-client.exe"
    ; The agent starts in every user's session at logon (it needs the desktop).
    WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Run" "VimoraAgent" '"$INSTDIR\vimora-client.exe"'
    ; Receive discovery beacons.
    nsExec::Exec '"$SYSDIR\netsh.exe" advfirewall firewall delete rule name="Vimora Agent"'
    Pop $0
    nsExec::Exec '"$SYSDIR\netsh.exe" advfirewall firewall add rule name="Vimora Agent" dir=in action=allow protocol=UDP localport=47800 program="$INSTDIR\vimora-client.exe"'
    Pop $0
    ; Privileged service (SYSTEM), logon task, Shadow control, updates.
    nsExec::Exec '"$INSTDIR\vimora-client.exe" setup'
    Pop $0
  ${EndIf}

  WriteUninstaller "$INSTDIR\uninstall.exe"
  File "/oname=vimora.ico" "vimora.ico"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\Vimora" "DisplayName" "Vimora"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\Vimora" "DisplayVersion" "${VERSION}"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\Vimora" "Publisher" "VEYMAY"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\Vimora" "DisplayIcon" "$INSTDIR\vimora.ico"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\Vimora" "InstallLocation" "$INSTDIR"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\Vimora" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\Vimora" "URLInfoAbout" "https://github.com/veymay/vimora"
  WriteRegDWORD HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\Vimora" "NoModify" 1
  WriteRegDWORD HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\Vimora" "NoRepair" 1

  ; A silent update is run as SYSTEM: the console comes back after its own update (it was
  ; started by the user), agents restart themselves when their file changes.
  ${If} $Silent == "1"
  ${AndIf} $Role == "teacher"
    Call LaunchApp
  ${EndIf}
SectionEnd

; Starts the program in the user's (non-elevated) session.
Function LaunchApp
  ${If} $Role == "teacher"
    Exec '"$WINDIR\explorer.exe" "$INSTDIR\vimora-server.exe"'
  ${Else}
    Exec '"$WINDIR\explorer.exe" "$INSTDIR\vimora-client.exe"'
  ${EndIf}
FunctionEnd

; --- uninstall -----------------------------------------------------------------------------------
Section "Uninstall"
  ${If} ${RunningX64}
    SetRegView 64
    ${DisableX64FSRedirection}
  ${EndIf}
  nsExec::Exec '"$SYSDIR\taskkill.exe" /F /IM vimora-client.exe'
  Pop $0
  nsExec::Exec '"$SYSDIR\taskkill.exe" /F /IM vimora-server.exe'
  Pop $0
  nsExec::Exec '"$SYSDIR\netsh.exe" advfirewall firewall delete rule name="Vimora Console"'
  Pop $0
  nsExec::Exec '"$SYSDIR\netsh.exe" advfirewall firewall delete rule name="Vimora Agent"'
  Pop $0
  nsExec::Exec '"$SYSDIR\schtasks.exe" /end /tn "Vimora Service"'
  Pop $0
  nsExec::Exec '"$SYSDIR\schtasks.exe" /delete /tn "Vimora Service" /f'
  Pop $0
  nsExec::Exec '"$SYSDIR\schtasks.exe" /delete /tn "Vimora Service Watchdog" /f'
  Pop $0
  nsExec::Exec '"$SYSDIR\schtasks.exe" /delete /tn "Vimora Update" /f'
  Pop $0
  nsExec::Exec '"$SYSDIR\schtasks.exe" /delete /tn "Vimora Agent" /f'
  Pop $0
  DeleteRegValue HKLM "Software\Microsoft\Windows\CurrentVersion\Run" "VimoraAgent"
  DeleteRegKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\Vimora"
  Delete "$SMPROGRAMS\Vimora.lnk"
  Delete "$DESKTOP\Vimora.lnk"
  Delete "$INSTDIR\vimora-client.exe"
  Delete "$INSTDIR\vimora-server.exe"
  Delete "$INSTDIR\vimora-client.old.exe"
  Delete "$INSTDIR\vimora-server.old.exe"
  Delete "$INSTDIR\vimora.ico"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"
  ReadEnvStr $0 ProgramData
  ${If} $0 != ""
    RMDir /r "$0\Vimora"
  ${EndIf}
SectionEnd
