Unicode True
!include "MUI2.nsh"
!ifndef BINARY
!define BINARY "..\target\release\midad.exe"
!endif
!ifndef OUTPUT
!define OUTPUT "..\target\Midad-0.5.0-Windows-x64.exe"
!endif
Name "مِداد | Midad"
OutFile "${OUTPUT}"
InstallDir "$LOCALAPPDATA\Programs\Midad"
RequestExecutionLevel user
SetCompressor /SOLID lzma
Icon "..\assets\Midad.ico"
!define MUI_ABORTWARNING
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "Arabic"
!insertmacro MUI_LANGUAGE "English"

Section "Midad"
  SetOutPath "$INSTDIR"
  File /oname=midad.exe "${BINARY}"
  File "..\assets\Midad.ico"
  File /oname=Amiri-OFL.txt "..\assets\OFL.txt"
  File /oname=ArefRuqaa-OFL.txt "..\assets\arefruqaa\OFL.txt"
  File /oname=ReemKufi-OFL.txt "..\assets\reemkufi\OFL.txt"
  File /oname=Nastaliq-OFL.txt "..\assets\notonastaliqurdu\OFL.txt"
  File "THIRD-PARTY.txt"
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  CreateShortcut "$SMPROGRAMS\Midad.lnk" "$INSTDIR\midad.exe" "" "$INSTDIR\Midad.ico"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Midad" "DisplayName" "مِداد | Midad"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Midad" "DisplayVersion" "0.5.0"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Midad" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegStr HKCU "Software\Classes\Midad.Project\shell\open\command" "" '"$INSTDIR\midad.exe" "%1"'
  WriteRegStr HKCU "Software\Classes\.midad\OpenWithProgids" "Midad.Project" ""
SectionEnd

Section "Uninstall"
  Delete "$INSTDIR\midad.exe"
  Delete "$INSTDIR\Midad.ico"
  Delete "$INSTDIR\Amiri-OFL.txt"
  Delete "$INSTDIR\ArefRuqaa-OFL.txt"
  Delete "$INSTDIR\ReemKufi-OFL.txt"
  Delete "$INSTDIR\Nastaliq-OFL.txt"
  Delete "$INSTDIR\THIRD-PARTY.txt"
  Delete "$INSTDIR\Uninstall.exe"
  Delete "$SMPROGRAMS\Midad.lnk"
  RMDir "$INSTDIR"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Midad"
  DeleteRegValue HKCU "Software\Classes\.midad\OpenWithProgids" "Midad.Project"
  DeleteRegKey HKCU "Software\Classes\Midad.Project"
SectionEnd
