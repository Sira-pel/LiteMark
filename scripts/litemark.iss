; Inno Setup script. Built by scripts/build-windows.ps1, which passes /DAppVersion.
#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif

[Setup]
AppId={{6B0F7A52-3E1D-4C0E-9C55-4B8D6E2A71F3}
AppName=LiteMark
AppVersion={#AppVersion}
AppPublisher=LiteMark Contributors
DefaultDirName={autopf}\LiteMark
DefaultGroupName=LiteMark
UninstallDisplayIcon={app}\litemark.exe
OutputDir=..\build
OutputBaseFilename=LiteMark-{#AppVersion}-windows-x64-setup
SetupIconFile=..\assets\windows\litemark.ico
Compression=lzma2
SolidCompression=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
WizardStyle=modern

[Files]
Source: "..\target\release\litemark.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\THIRD_PARTY_NOTICES.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\LiteMark"; Filename: "{app}\litemark.exe"
Name: "{autodesktop}\LiteMark"; Filename: "{app}\litemark.exe"; Tasks: desktopicon

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; Flags: unchecked

[Run]
Filename: "{app}\litemark.exe"; Description: "Launch LiteMark"; Flags: nowait postinstall skipifsilent
