Unicode true
!include "MUI2.nsh"
!include "x64.nsh"
!ifndef VERSION
!define VERSION "1.4.5"
!endif
!ifndef ARCH
!define ARCH "x64"
!endif
Name "EditorTool"
OutFile "..\dist\packages\EditorTool-${VERSION}-windows-${ARCH}-setup.exe"
InstallDir "$LOCALAPPDATA\Programs\EditorTool"
RequestExecutionLevel user
SetCompressor /SOLID lzma
BrandingText "EditorTool"
!define MUI_ICON "..\assets\editortool.ico"
!define MUI_UNICON "..\assets\editortool.ico"
!define MUI_FINISHPAGE_RUN "$INSTDIR\EditorTool.exe"
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "..\LICENSE"
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "German"
!insertmacro MUI_LANGUAGE "English"

Function .onInit
  !if "${ARCH}" == "arm64"
    System::Call 'kernel32::IsWow64Process2(p -1, *i .r0, *i .r1)'
    IntCmp $1 43620 arm_ok
      MessageBox MB_ICONSTOP "Dieses Paket benötigt Windows ARM64."
      Abort
    arm_ok:
  !else
    ${IfNot} ${RunningX64}
      MessageBox MB_ICONSTOP "Dieses Paket benötigt Windows x64 oder ARM64 mit x64-Unterstützung."
      Abort
    ${EndIf}
  !endif
FunctionEnd

Section "EditorTool"
  SetShellVarContext current
  SetOutPath "$INSTDIR"
  File "..\dist\windows\EditorTool.exe"
  File /nonfatal "..\dist\windows\*.dll"
  File "..\LICENSE"
  File "..\README.md"
  SetOutPath "$INSTDIR\licenses"
  File /r "..\third-party-licenses\*"
  SetOutPath "$INSTDIR"
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  CreateDirectory "$SMPROGRAMS\EditorTool"
  CreateShortCut "$SMPROGRAMS\EditorTool\EditorTool.lnk" "$INSTDIR\EditorTool.exe"
  CreateShortCut "$SMPROGRAMS\EditorTool\Deinstallieren.lnk" "$INSTDIR\Uninstall.exe"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\EditorTool" "DisplayName" "EditorTool"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\EditorTool" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\EditorTool" "Publisher" "OnlySecci"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\EditorTool" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\EditorTool" "DisplayIcon" "$INSTDIR\EditorTool.exe,0"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\EditorTool" "UninstallString" '$\"$INSTDIR\Uninstall.exe$\"'
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\EditorTool" "NoModify" 1
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\EditorTool" "NoRepair" 1
  WriteRegStr HKCU "Software\Classes\.etdoc\OpenWithProgids" "EditorTool.Document" ""
  WriteRegStr HKCU "Software\Classes\EditorTool.Document" "" "EditorTool-Dokument"
  WriteRegStr HKCU "Software\Classes\EditorTool.Document\DefaultIcon" "" "$INSTDIR\EditorTool.exe,0"
  WriteRegStr HKCU "Software\Classes\EditorTool.Document\shell\open\command" "" '$\"$INSTDIR\EditorTool.exe$\" $\"%1$\"'
  WriteRegStr HKCU "Software\OnlySecci\EditorTool\Capabilities" "ApplicationName" "EditorTool"
  WriteRegStr HKCU "Software\OnlySecci\EditorTool\Capabilities" "ApplicationDescription" "Text und Bilder mit EditorTool bearbeiten"
  WriteRegStr HKCU "Software\OnlySecci\EditorTool\Capabilities\FileAssociations" ".etdoc" "EditorTool.Document"
  WriteRegStr HKCU "Software\RegisteredApplications" "EditorTool" "Software\OnlySecci\EditorTool\Capabilities"
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
SectionEnd

Section "Uninstall"
  SetShellVarContext current
  StrCmp "$INSTDIR" "$LOCALAPPDATA\Programs\EditorTool" +3
    MessageBox MB_ICONSTOP "Unerwarteter Installationspfad. Es wurden keine Dateien entfernt."
    Abort
  Delete "$INSTDIR\EditorTool.exe"
  Delete "$INSTDIR\libstdc++-6.dll"
  Delete "$INSTDIR\libgcc_s_seh-1.dll"
  Delete "$INSTDIR\libwinpthread-1.dll"
  Delete "$INSTDIR\README.md"
  Delete "$INSTDIR\LICENSE"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir /r "$INSTDIR\licenses"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\EditorTool\EditorTool.lnk"
  Delete "$SMPROGRAMS\EditorTool\Deinstallieren.lnk"
  RMDir "$SMPROGRAMS\EditorTool"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\EditorTool"
  DeleteRegValue HKCU "Software\Classes\.etdoc\OpenWithProgids" "EditorTool.Document"
  DeleteRegKey HKCU "Software\Classes\EditorTool.Document"
  DeleteRegKey HKCU "Software\OnlySecci\EditorTool\Capabilities"
  DeleteRegValue HKCU "Software\RegisteredApplications" "EditorTool"
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
SectionEnd
