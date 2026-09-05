Unicode True

!include "LogicLib.nsh"
!include "WinMessages.nsh"
!include "x64.nsh"

!ifndef APP_FILENAME
    !error "APP_FILENAME must name the application executable"
!endif
!ifndef FILE_VERSION
    !error "FILE_VERSION must use the four-part Windows version format"
!endif
!ifndef OUTPUT_FILE
    !error "OUTPUT_FILE must name the installer output"
!endif
!ifndef PRODUCT_VERSION
    !error "PRODUCT_VERSION must name the Cargo package version"
!endif
!ifndef RELEASE_LABEL
    !error "RELEASE_LABEL must name the user-facing release"
!endif
!ifndef STAGE_DIR
    !error "STAGE_DIR must point to the assembled Windows package"
!endif

!define PRODUCT_NAME "NoiseHoiHoi"
!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\NoiseHoiHoi"

Name "${PRODUCT_NAME} ${RELEASE_LABEL}"
OutFile "${OUTPUT_FILE}"
InstallDir "$PROGRAMFILES64\${PRODUCT_NAME}"
RequestExecutionLevel admin
CRCCheck force
SetCompressor /SOLID lzma
ShowInstDetails show
ShowUninstDetails show
BrandingText "${PRODUCT_NAME} ${RELEASE_LABEL}"

VIProductVersion "${FILE_VERSION}"
VIAddVersionKey /LANG=1033 "ProductName" "${PRODUCT_NAME}"
VIAddVersionKey /LANG=1033 "FileDescription" "${PRODUCT_NAME} ${RELEASE_LABEL} Windows installer"
VIAddVersionKey /LANG=1033 "FileVersion" "${FILE_VERSION}"
VIAddVersionKey /LANG=1033 "ProductVersion" "${PRODUCT_VERSION}"
VIAddVersionKey /LANG=1033 "LegalCopyright" "Copyright (c) NoiseHoiHoi contributors"

LicenseText "NoiseHoiHoi uses the donationware VB-CABLE package under the following notice."
LicenseData "VB-CABLE-NOTICE.txt"
Page license
Page instfiles
UninstPage uninstConfirm
UninstPage instfiles

Var VBCablePreexisting
Var CloseAttempts

!macro DefineEnsureClosed FUNCTION_PREFIX LABEL_PREFIX
Function ${FUNCTION_PREFIX}EnsureNoiseHoiHoiClosed
    StrCpy $CloseAttempts 0

${LABEL_PREFIX}_check_app:
    FindWindow $0 "" "${PRODUCT_NAME}"
    ${If} $0 == 0
        Return
    ${EndIf}

    ${If} $CloseAttempts == 0
        MessageBox MB_ICONEXCLAMATION|MB_OKCANCEL \
            "${PRODUCT_NAME} is currently running. It must be closed before continuing.$\r$\n$\r$\nClick OK to close it." \
            /SD IDOK \
            IDOK ${LABEL_PREFIX}_close_app IDCANCEL ${LABEL_PREFIX}_cancel
    ${EndIf}

${LABEL_PREFIX}_close_app:
    SendMessage $0 ${WM_CLOSE} 0 0 /TIMEOUT=2000
    Sleep 200
    IntOp $CloseAttempts $CloseAttempts + 1
    ${If} $CloseAttempts < 25
        Goto ${LABEL_PREFIX}_check_app
    ${EndIf}

    MessageBox MB_ICONSTOP|MB_OK \
        "${PRODUCT_NAME} could not be closed. Close it using Task Manager, then try again." \
        /SD IDOK

${LABEL_PREFIX}_cancel:
    Abort
FunctionEnd
!macroend

!insertmacro DefineEnsureClosed "" "install"
!insertmacro DefineEnsureClosed "un." "uninstall"

!macro RemoveStartMenuShortcuts
    Delete "$SMPROGRAMS\${PRODUCT_NAME}\${PRODUCT_NAME}.lnk"
    Delete "$SMPROGRAMS\${PRODUCT_NAME}\Uninstall ${PRODUCT_NAME}.lnk"
    RMDir "$SMPROGRAMS\${PRODUCT_NAME}"
!macroend

Function .onInit
    SetRegView 64
    SetShellVarContext all
    ${IfNot} ${RunningX64}
        MessageBox MB_ICONSTOP|MB_OK "${PRODUCT_NAME} ${RELEASE_LABEL} requires 64-bit Windows."
        Abort
    ${EndIf}

    Call EnsureNoiseHoiHoiClosed

    StrCpy $VBCablePreexisting "0"
    ClearErrors
    ReadRegStr $0 HKLM "SYSTEM\CurrentControlSet\Services\VBAudioVACMME" "ImagePath"
    ${IfNot} ${Errors}
        StrCpy $VBCablePreexisting "1"
    ${EndIf}
FunctionEnd

Function un.onInit
    SetRegView 64
    SetShellVarContext all
    Call un.EnsureNoiseHoiHoiClosed
FunctionEnd

Section "${PRODUCT_NAME}" Install
    SectionIn RO
    StrCpy $INSTDIR "$PROGRAMFILES64\${PRODUCT_NAME}"
    SetOutPath "$INSTDIR"
    File /r "${STAGE_DIR}/*"

    ${If} $VBCablePreexisting == "1"
        DetailPrint "VB-CABLE is already installed; keeping the existing installation."
        Goto vbcable_ready
    ${EndIf}

    DetailPrint "Installing the Microsoft-signed VB-CABLE driver..."
    ClearErrors
    ExecWait '"$INSTDIR\third-party\vb-cable\VBCABLE_Setup_x64.exe" -i -h' $0
    ${If} ${Errors}
        MessageBox MB_ICONSTOP|MB_OK \
            "The VB-CABLE setup program could not be started."
        Abort
    ${EndIf}
    ${If} $0 != "0"
        ${If} $0 != "3010"
            ${If} $0 != "1641"
                MessageBox MB_ICONSTOP|MB_OK \
                    "VB-CABLE could not be installed silently (exit code $0)."
                Abort
            ${EndIf}
        ${EndIf}
    ${EndIf}
    SetRebootFlag true

vbcable_ready:
    ClearErrors
    FileOpen $0 "$INSTDIR\.noise-hoihoi-install" w
    ${If} ${Errors}
        MessageBox MB_ICONSTOP|MB_OK \
            "The installation ownership marker could not be created."
        Abort
    ${EndIf}
    FileWrite $0 "${PRODUCT_NAME} ${PRODUCT_VERSION}$\r$\n"
    FileClose $0
    SetFileAttributes "$INSTDIR\.noise-hoihoi-install" HIDDEN

    WriteUninstaller "$INSTDIR\Uninstall.exe"
    WriteRegStr HKLM "${UNINSTALL_KEY}" "DisplayName" "${PRODUCT_NAME} ${RELEASE_LABEL}"
    WriteRegStr HKLM "${UNINSTALL_KEY}" "DisplayVersion" "${PRODUCT_VERSION}"
    WriteRegStr HKLM "${UNINSTALL_KEY}" "Publisher" "NoiseHoiHoi Project"
    WriteRegStr HKLM "${UNINSTALL_KEY}" "InstallLocation" "$INSTDIR"
    WriteRegStr HKLM "${UNINSTALL_KEY}" "DisplayIcon" "$INSTDIR\${APP_FILENAME}"
    WriteRegStr HKLM "${UNINSTALL_KEY}" "UninstallString" '$\"$INSTDIR\Uninstall.exe$\"'
    WriteRegStr HKLM "${UNINSTALL_KEY}" "QuietUninstallString" '$\"$INSTDIR\Uninstall.exe$\" /S'
    WriteRegDWORD HKLM "${UNINSTALL_KEY}" "NoModify" 1
    WriteRegDWORD HKLM "${UNINSTALL_KEY}" "NoRepair" 1
    DeleteRegKey HKLM "Software\NoiseHoiHoi"

    ; Remove shortcuts created by early v0.1 installers for only the current
    ; user, then install the machine-wide shortcuts used by this package.
    SetShellVarContext current
    !insertmacro RemoveStartMenuShortcuts
    SetShellVarContext all
    CreateDirectory "$SMPROGRAMS\${PRODUCT_NAME}"
    CreateShortcut "$SMPROGRAMS\${PRODUCT_NAME}\${PRODUCT_NAME}.lnk" \
        "$INSTDIR\${APP_FILENAME}"
    CreateShortcut "$SMPROGRAMS\${PRODUCT_NAME}\Uninstall ${PRODUCT_NAME}.lnk" \
        "$INSTDIR\Uninstall.exe"
SectionEnd

Section "Uninstall"
    DetailPrint "VB-CABLE is shared with other applications and will remain installed."

    IfFileExists "$INSTDIR\.noise-hoihoi-install" uninstall_owned_path
    MessageBox MB_ICONSTOP|MB_OK \
        "The NoiseHoiHoi installation marker is missing. Files will not be removed from $INSTDIR." \
        /SD IDOK
    Abort

uninstall_owned_path:

    SetShellVarContext current
    !insertmacro RemoveStartMenuShortcuts
    SetShellVarContext all
    !insertmacro RemoveStartMenuShortcuts

    DeleteRegKey HKLM "${UNINSTALL_KEY}"
    DeleteRegKey HKLM "Software\NoiseHoiHoi"
    RMDir /r "$INSTDIR"
SectionEnd
