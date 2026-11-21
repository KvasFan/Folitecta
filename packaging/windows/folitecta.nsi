Unicode true
!include "MUI2.nsh"
!include "LogicLib.nsh"
!ifndef VERSION
  !error "Pass /DVERSION=x.y.z"
!endif
!ifndef PAYLOAD
  !error "Pass /DPAYLOAD=path"
!endif
Name "Folitecta ${VERSION}"
OutFile "..\..\dist\folitecta-${VERSION}-windows-x64-setup.exe"
InstallDir "$LOCALAPPDATA\Programs\Folitecta"
RequestExecutionLevel user
SetCompressor /SOLID lzma
!define MUI_ABORTWARNING
!define MUI_ICON "folitecta.ico"
!define MUI_UNICON "folitecta.ico"
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "..\..\LICENSE"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!define MUI_FINISHPAGE_RUN "$INSTDIR\folitecta.exe"
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"
!insertmacro MUI_LANGUAGE "Italian"
Section "Folitecta"
  SetShellVarContext current
  SetRegView 64
  SetOutPath "$INSTDIR"
  File /r "${PAYLOAD}\*.*"
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  CreateDirectory "$SMPROGRAMS\Folitecta"
  CreateShortcut "$SMPROGRAMS\Folitecta\Folitecta.lnk" "$INSTDIR\folitecta.exe"
  CreateShortcut "$SMPROGRAMS\Folitecta\Uninstall.lnk" "$INSTDIR\Uninstall.exe"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Folitecta" "DisplayName" "Folitecta"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Folitecta" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Folitecta" "Publisher" "KvasFan"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Folitecta" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Folitecta" "DisplayIcon" "$INSTDIR\folitecta.exe"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Folitecta" "UninstallString" '$"$INSTDIR\Uninstall.exe$"'
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Folitecta" "QuietUninstallString" '$"$INSTDIR\Uninstall.exe$" /S'
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Folitecta" "NoModify" 1
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Folitecta" "NoRepair" 1
  WriteRegStr HKCU "Software\Classes\Folitecta.PDF" "" "PDF document"
  WriteRegStr HKCU "Software\Classes\Folitecta.PDF\DefaultIcon" "" "$INSTDIR\folitecta.exe,0"
  WriteRegStr HKCU "Software\Classes\Folitecta.PDF\shell\open\command" "" '$"$INSTDIR\folitecta.exe$" $"%1$"'
  WriteRegStr HKCU "Software\Classes\.pdf\OpenWithProgids" "Folitecta.PDF" ""
SectionEnd
Section "Uninstall"
  SetShellVarContext current
  SetRegView 64
  Delete "$SMPROGRAMS\Folitecta\Folitecta.lnk"
  Delete "$SMPROGRAMS\Folitecta\Uninstall.lnk"
  RMDir "$SMPROGRAMS\Folitecta"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Folitecta"
  DeleteRegKey HKCU "Software\Classes\Folitecta.PDF"
  DeleteRegValue HKCU "Software\Classes\.pdf\OpenWithProgids" "Folitecta.PDF"
  ; Only remove files shipped by the installer, keeping user-created documents/settings.
  Delete "$INSTDIR\folitecta.exe"
  Delete "$INSTDIR\README.md"
  Delete "$INSTDIR\LICENSE"
  Delete "$INSTDIR\CHANGELOG.md"
  Delete "$INSTDIR\THIRD-PARTY-LICENSES.txt"
  RMDir /r "$INSTDIR\docs"
  RMDir /r "$INSTDIR\dictionaries"
  RMDir /r "$INSTDIR\ocr"
  RMDir /r "$INSTDIR\fonts"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"
SectionEnd
