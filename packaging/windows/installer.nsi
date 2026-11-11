; Axcenity Editor installer. Built on GitHub Actions with:
;   makensis /DVERSION=... /DSTAGE=... /DOUTFILE=... /DICON=... /DHEADER=... /DSIDEBAR=... installer.nsi
;
; Current-user install (no admin), branded MUI2 wizard, Start Menu + optional desktop shortcut.

Unicode True
SetCompressor /SOLID lzma
ManifestDPIAware true

!ifndef VERSION
  !error "Pass /DVERSION=0.1.0-alpha.1"
!endif
!ifndef STAGE
  !error "Pass /DSTAGE=path-to-staged-payload"
!endif
!ifndef OUTFILE
  !error "Pass /DOUTFILE=path-to-installer.exe"
!endif
!ifndef ICON
  !error "Pass /DICON=path-to-icon.ico"
!endif
!ifndef HEADER
  !error "Pass /DHEADER=path-to-header.bmp"
!endif
!ifndef SIDEBAR
  !error "Pass /DSIDEBAR=path-to-sidebar.bmp"
!endif

!include "MUI2.nsh"
!include "FileFunc.nsh"
!include "LogicLib.nsh"

Name "Axcenity Editor"
BrandingText "Axcenity Engine ${VERSION}"
OutFile "${OUTFILE}"
InstallDir "$LOCALAPPDATA\Programs\Axcenity Engine"
InstallDirRegKey HKCU "Software\Axcenity\Editor" "InstallDir"
RequestExecutionLevel user

!define MUI_ICON "${ICON}"
!define MUI_UNICON "${ICON}"
!define MUI_HEADERIMAGE
!define MUI_HEADERIMAGE_BITMAP "${HEADER}"
!define MUI_HEADERIMAGE_BITMAP_NOSTRETCH
!define MUI_WELCOMEFINISHPAGE_BITMAP "${SIDEBAR}"
!define MUI_WELCOMEFINISHPAGE_BITMAP_NOSTRETCH
!define MUI_UNWELCOMEFINISHPAGE_BITMAP "${SIDEBAR}"
!define MUI_UNWELCOMEFINISHPAGE_BITMAP_NOSTRETCH
!define MUI_ABORTWARNING
!define MUI_UNABORTWARNING

!define MUI_WELCOMEPAGE_TITLE "Axcenity Editor ${VERSION}"
!define MUI_WELCOMEPAGE_TEXT "This setup installs the Axcenity Engine editor on your computer.$\r$\n$\r$\nBuild worlds from parts, script them in Lua 5.4 or with blocks, press Play to test, and export games for Windows, macOS and Linux.$\r$\n$\r$\nThis is an alpha release: expect bugs and breaking changes.$\r$\n$\r$\nClick Next to continue."

!define MUI_LICENSEPAGE_TEXT_TOP "Axcenity Engine is licensed under the Apache License 2.0. Please review the license before installing."
!define MUI_LICENSEPAGE_TEXT_BOTTOM "Click I Agree to accept the license and continue."

!define MUI_DIRECTORYPAGE_TEXT_TOP "Setup will install Axcenity Editor in the following folder. To install in a different folder, click Browse."

!define MUI_FINISHPAGE_TITLE "Axcenity Editor is installed"
!define MUI_FINISHPAGE_TEXT "You can open it from the Start menu, or launch it now.$\r$\n$\r$\nA short install guide (INSTALL.txt) is in the install folder."
!define MUI_FINISHPAGE_RUN "$INSTDIR\axcenity-editor.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Launch Axcenity Editor"
!define MUI_FINISHPAGE_SHOWREADME "$INSTDIR\INSTALL.txt"
!define MUI_FINISHPAGE_SHOWREADME_TEXT "Open the install guide"

!define MUI_UNWELCOMEPAGE_TITLE "Remove Axcenity Editor"
!define MUI_UNWELCOMEPAGE_TEXT "This will remove Axcenity Editor ${VERSION} from your computer.$\r$\n$\r$\nYour project folders are not deleted. Saved editor settings can be removed on the next page.$\r$\n$\r$\nClick Next to continue."

VIProductVersion "0.1.0.0"
VIAddVersionKey "ProductName" "Axcenity Editor"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "FileDescription" "Axcenity Editor setup"
VIAddVersionKey "LegalCopyright" "Copyright 2026 Paul Janota and contributors"
VIAddVersionKey "CompanyName" "Axcenity Engine"

Var StartMenuFolder

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "${STAGE}\LICENSE"
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_DIRECTORY
!define MUI_STARTMENUPAGE_REGISTRY_ROOT "HKCU"
!define MUI_STARTMENUPAGE_REGISTRY_KEY "Software\Axcenity\Editor"
!define MUI_STARTMENUPAGE_REGISTRY_VALUENAME "StartMenuFolder"
!define MUI_STARTMENUPAGE_DEFAULTFOLDER "Axcenity"
!insertmacro MUI_PAGE_STARTMENU Application $StartMenuFolder
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_WELCOME
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_UNPAGE_FINISH

!insertmacro MUI_LANGUAGE "English"

Section "Axcenity Editor" SecEditor
  SectionIn RO
  SetOutPath "$INSTDIR"
  File /r "${STAGE}\*.*"
  File "${STAGE}\LICENSE"
  File "${STAGE}\NOTICE"
  File "${STAGE}\INSTALL.txt"

  WriteRegStr HKCU "Software\Axcenity\Editor" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AxcenityEditor" "DisplayName" "Axcenity Editor"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AxcenityEditor" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AxcenityEditor" "Publisher" "Paul Janota and contributors"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AxcenityEditor" "DisplayIcon" "$INSTDIR\axcenity-editor.exe"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AxcenityEditor" "UninstallString" "$INSTDIR\Uninstall.exe"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AxcenityEditor" "QuietUninstallString" "$INSTDIR\Uninstall.exe /S"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AxcenityEditor" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AxcenityEditor" "URLInfoAbout" "https://github.com/MrBearson321/Axcenity-Engine"
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AxcenityEditor" "NoModify" 1
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AxcenityEditor" "NoRepair" 1
  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  IntFmt $0 "0x%08X" $0
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AxcenityEditor" "EstimatedSize" "$0"

  WriteUninstaller "$INSTDIR\Uninstall.exe"

  !insertmacro MUI_STARTMENU_WRITE_BEGIN Application
    CreateDirectory "$SMPROGRAMS\$StartMenuFolder"
    CreateShortcut "$SMPROGRAMS\$StartMenuFolder\Axcenity Editor.lnk" "$INSTDIR\axcenity-editor.exe" "" "$INSTDIR\axcenity-editor.exe" 0
    CreateShortcut "$SMPROGRAMS\$StartMenuFolder\Install guide.lnk" "$INSTDIR\INSTALL.txt"
    CreateShortcut "$SMPROGRAMS\$StartMenuFolder\Uninstall.lnk" "$INSTDIR\Uninstall.exe"
  !insertmacro MUI_STARTMENU_WRITE_END
SectionEnd

Section "Desktop shortcut" SecDesktop
  CreateShortcut "$DESKTOP\Axcenity Editor.lnk" "$INSTDIR\axcenity-editor.exe" "" "$INSTDIR\axcenity-editor.exe" 0
SectionEnd

Section "un.Uninstall"
  Delete "$DESKTOP\Axcenity Editor.lnk"
  !insertmacro MUI_STARTMENU_GETFOLDER Application $StartMenuFolder
  Delete "$SMPROGRAMS\$StartMenuFolder\Axcenity Editor.lnk"
  Delete "$SMPROGRAMS\$StartMenuFolder\Install guide.lnk"
  Delete "$SMPROGRAMS\$StartMenuFolder\Uninstall.lnk"
  RMDir "$SMPROGRAMS\$StartMenuFolder"

  RMDir /r "$INSTDIR"
  ; The version an in-app update replaced, kept for Roll back (crates/updater).
  RMDir /r "$INSTDIR.previous"

  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AxcenityEditor"
  DeleteRegKey HKCU "Software\Axcenity\Editor"
SectionEnd

LangString DESC_SecEditor ${LANG_ENGLISH} "The editor, the player, and players for Windows, macOS and Linux so you can export a game for any of them."
LangString DESC_SecDesktop ${LANG_ENGLISH} "Put a shortcut to Axcenity Editor on your desktop."

!insertmacro MUI_FUNCTION_DESCRIPTION_BEGIN
  !insertmacro MUI_DESCRIPTION_TEXT ${SecEditor} $(DESC_SecEditor)
  !insertmacro MUI_DESCRIPTION_TEXT ${SecDesktop} $(DESC_SecDesktop)
!insertmacro MUI_FUNCTION_DESCRIPTION_END
