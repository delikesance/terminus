!include "MUI2.nsh"
!include "x64.nsh"
!include "WinMessages.nsh"

!ifndef VERSION
  !define VERSION "0.5.30"
!endif

!define PRODUCT_NAME "Terminus"
!define PRODUCT_PUBLISHER "Terminus"
!define PRODUCT_WEB_SITE "https://github.com/delikesance/terminus"
!define PRODUCT_DIR_REGKEY "Software\Microsoft\Windows\CurrentVersion\App Paths\terminus.exe"
!define PRODUCT_UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\Terminus"

SetCompressor /SOLID lzma

Name "${PRODUCT_NAME} ${VERSION}"
OutFile "${OUTFILE}"
InstallDir "$PROGRAMFILES64\Terminus"
InstallDirRegKey HKLM "${PRODUCT_DIR_REGKEY}" ""
RequestExecutionLevel admin

; UI settings
!define MUI_ABORTWARNING
!define MUI_ICON "${SRCDIR}\misc\windows\rio.ico"
!define MUI_UNICON "${SRCDIR}\misc\windows\rio.ico"

; Pages
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "${SRCDIR}\LICENSE"
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES

!define MUI_FINISHPAGE_RUN "$INSTDIR\terminus.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Launch Terminus"
!insertmacro MUI_PAGE_FINISH

; Uninstaller pages
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

; Languages
!insertmacro MUI_LANGUAGE "English"
!insertmacro MUI_LANGUAGE "French"

; PATH functions
Function AddToPath
  Exch $0 ; directory to add
  Push $1
  Push $2

  ReadRegStr $1 HKLM "SYSTEM\CurrentControlSet\Control\Session Manager\Environment" "Path"
  StrCmp $1 "" not_found

  ; Check if already in PATH
  Push "$1;"
  Push "$0;"
  Call StrStr
  Pop $2
  StrCmp $2 "" add_to_path
  Goto done

add_to_path:
  WriteRegExpandStr HKLM "SYSTEM\CurrentControlSet\Control\Session Manager\Environment" "Path" "$1;$0"
  SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment" /TIMEOUT=2000
  Goto done

not_found:
  WriteRegExpandStr HKLM "SYSTEM\CurrentControlSet\Control\Session Manager\Environment" "Path" "$0"
  SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment" /TIMEOUT=2000

done:
  Pop $2
  Pop $1
  Pop $0
FunctionEnd

Function un.RemoveFromPath
  Exch $0 ; directory to remove
  Push $1
  Push $2
  Push $3
  Push $4

  ReadRegStr $1 HKLM "SYSTEM\CurrentControlSet\Control\Session Manager\Environment" "Path"
  StrCmp $1 "" un_done

  ; Replace ";$0" with ""
  Push "$1"
  Push ";$0"
  Push ""
  Call un.StrRep
  Pop $1

  ; Replace "$0;" with ""
  Push "$1"
  Push "$0;"
  Push ""
  Call un.StrRep
  Pop $1

  WriteRegExpandStr HKLM "SYSTEM\CurrentControlSet\Control\Session Manager\Environment" "Path" "$1"
  SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment" /TIMEOUT=2000

un_done:
  Pop $4
  Pop $3
  Pop $2
  Pop $1
  Pop $0
FunctionEnd

Function StrStr
  Exch $R1 ; needle
  Exch
  Exch $R2 ; haystack
  Push $R3
  Push $R4
  Push $R5
  StrLen $R3 $R1
  StrCpy $R4 0
loop:
  StrCpy $R5 $R2 $R3 $R4
  StrCmp $R5 $R1 found
  StrCmp $R5 "" notfound
  IntOp $R4 $R4 + 1
  Goto loop
found:
  StrCpy $R1 $R2 "" $R4
  Goto done
notfound:
  StrCpy $R1 ""
done:
  Pop $R5
  Pop $R4
  Pop $R3
  Pop $R2
  Exch $R1
FunctionEnd

Function un.StrRep
  Exch $R2 ; repl
  Exch 1
  Exch $R1 ; needle
  Exch 2
  Exch $R0 ; haystack
  Push $R3
  Push $R4
  Push $R5
  Push $R6
  StrLen $R3 $R1
  StrLen $R4 $R2
  StrCpy $R5 0
un_loop:
  StrCpy $R6 $R0 $R3 $R5
  StrCmp $R6 $R1 un_found
  StrCmp $R6 "" un_done
  IntOp $R5 $R5 + 1
  Goto un_loop
un_found:
  StrCpy $R6 $R0 $R5
  IntOp $R5 $R5 + $R3
  StrCpy $R0 $R0 "" $R5
  StrCpy $R0 "$R6$R2$R0"
  IntOp $R5 $R5 - $R3
  IntOp $R5 $R5 + $R4
  Goto un_loop
un_done:
  Pop $R6
  Pop $R5
  Pop $R4
  Pop $R3
  Pop $R2
  Pop $R1
  Exch $R0
FunctionEnd

; Installation sections
Section "!Terminus (required)" SEC_CORE
  SectionIn RO
  SetOutPath "$INSTDIR"
  SetOverwrite on
  File "/oname=terminus.exe" "${EXEPATH}"
  File "/oname=terminus.ico" "${SRCDIR}\misc\windows\rio.ico"

  ; App Paths (allows Win+R > terminus)
  WriteRegStr HKLM "${PRODUCT_DIR_REGKEY}" "" "$INSTDIR\terminus.exe"
  WriteRegStr HKLM "${PRODUCT_DIR_REGKEY}" "Path" "$INSTDIR"

  ; Add/Remove Programs entry
  WriteRegStr HKLM "${PRODUCT_UNINST_KEY}" "DisplayName" "${PRODUCT_NAME}"
  WriteRegStr HKLM "${PRODUCT_UNINST_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKLM "${PRODUCT_UNINST_KEY}" "Publisher" "${PRODUCT_PUBLISHER}"
  WriteRegStr HKLM "${PRODUCT_UNINST_KEY}" "URLInfoAbout" "${PRODUCT_WEB_SITE}"
  WriteRegStr HKLM "${PRODUCT_UNINST_KEY}" "DisplayIcon" "$INSTDIR\terminus.exe,0"
  WriteRegStr HKLM "${PRODUCT_UNINST_KEY}" "UninstallString" "$INSTDIR\Uninstall.exe"
  WriteRegStr HKLM "${PRODUCT_UNINST_KEY}" "QuietUninstallString" "$INSTDIR\Uninstall.exe /S"
  WriteRegDWORD HKLM "${PRODUCT_UNINST_KEY}" "NoModify" 1
  WriteRegDWORD HKLM "${PRODUCT_UNINST_KEY}" "NoRepair" 1

  WriteUninstaller "$INSTDIR\Uninstall.exe"
SectionEnd

Section "Start Menu Shortcut" SEC_STARTMENU
  CreateDirectory "$SMPROGRAMS\Terminus"
  CreateShortcut "$SMPROGRAMS\Terminus\Terminus.lnk" "$INSTDIR\terminus.exe" "" "$INSTDIR\terminus.ico"
  CreateShortcut "$SMPROGRAMS\Terminus\Uninstall Terminus.lnk" "$INSTDIR\Uninstall.exe"
SectionEnd

Section "Desktop Shortcut" SEC_DESKTOP
  CreateShortcut "$DESKTOP\Terminus.lnk" "$INSTDIR\terminus.exe" "" "$INSTDIR\terminus.ico"
SectionEnd

Section "Add to PATH" SEC_PATH
  Push "$INSTDIR"
  Call AddToPath
SectionEnd

Section "Open in Terminus context menu" SEC_CONTEXT
  ; Background context menu (right-click inside a folder)
  WriteRegStr HKCR "Directory\Background\shell\Terminus" "" "Open in Terminus"
  WriteRegStr HKCR "Directory\Background\shell\Terminus" "Icon" "$INSTDIR\terminus.exe"
  WriteRegStr HKCR "Directory\Background\shell\Terminus\command" "" '"$INSTDIR\terminus.exe" --working-dir "%V"'

  ; Directory context menu (right-click on a folder)
  WriteRegStr HKCR "Directory\shell\Terminus" "" "Open in Terminus"
  WriteRegStr HKCR "Directory\shell\Terminus" "Icon" "$INSTDIR\terminus.exe"
  WriteRegStr HKCR "Directory\shell\Terminus\command" "" '"$INSTDIR\terminus.exe" --working-dir "%1"'
SectionEnd

; Section descriptions
!insertmacro MUI_FUNCTION_DESCRIPTION_BEGIN
  !insertmacro MUI_DESCRIPTION_TEXT ${SEC_CORE} "Installs the Terminus executable and core files."
  !insertmacro MUI_DESCRIPTION_TEXT ${SEC_STARTMENU} "Creates shortcuts in the Start Menu."
  !insertmacro MUI_DESCRIPTION_TEXT ${SEC_DESKTOP} "Creates a shortcut on the Desktop."
  !insertmacro MUI_DESCRIPTION_TEXT ${SEC_PATH} "Adds Terminus to the system PATH so you can launch it from command prompt."
  !insertmacro MUI_DESCRIPTION_TEXT ${SEC_CONTEXT} "Adds 'Open in Terminus' to the Windows Explorer context menu."
!insertmacro MUI_FUNCTION_DESCRIPTION_END

; Uninstallation section
Section "Uninstall"
  ; Remove context menus
  DeleteRegKey HKCR "Directory\Background\shell\Terminus"
  DeleteRegKey HKCR "Directory\shell\Terminus"

  ; Remove from PATH
  Push "$INSTDIR"
  Call un.RemoveFromPath

  ; Remove App Paths & Uninstall registry
  DeleteRegKey HKLM "${PRODUCT_DIR_REGKEY}"
  DeleteRegKey HKLM "${PRODUCT_UNINST_KEY}"

  ; Remove shortcuts
  Delete "$DESKTOP\Terminus.lnk"
  Delete "$SMPROGRAMS\Terminus\Terminus.lnk"
  Delete "$SMPROGRAMS\Terminus\Uninstall Terminus.lnk"
  RMDir "$SMPROGRAMS\Terminus"

  ; Remove files and directory
  Delete "$INSTDIR\terminus.exe"
  Delete "$INSTDIR\terminus.ico"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"
SectionEnd
