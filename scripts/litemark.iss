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
ChangesAssociations=yes

[Files]
Source: "..\target\release\litemark.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\THIRD_PARTY_NOTICES.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\LiteMark"; Filename: "{app}\litemark.exe"
Name: "{autodesktop}\LiteMark"; Filename: "{app}\litemark.exe"; Tasks: desktopicon

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; Flags: unchecked
Name: "openwith"; Description: "Add LiteMark to the ""Open with"" list for .md and .txt files"

; Windows never lets an installer take over a file type: this only lists LiteMark
; under "Open with", and the user picks it as the default in Settings.
[Registry]
Root: HKA; Subkey: "Software\Classes\LiteMark.md"; ValueType: string; ValueData: "Markdown document"; Flags: uninsdeletekey; Tasks: openwith
Root: HKA; Subkey: "Software\Classes\LiteMark.md\DefaultIcon"; ValueType: string; ValueData: "{app}\litemark.exe,0"; Tasks: openwith
Root: HKA; Subkey: "Software\Classes\LiteMark.md\shell\open\command"; ValueType: string; ValueData: """{app}\litemark.exe"" ""%1"""; Tasks: openwith
Root: HKA; Subkey: "Software\Classes\.md\OpenWithProgids"; ValueType: none; ValueName: "LiteMark.md"; ValueData: ""; Flags: uninsdeletevalue; Tasks: openwith
Root: HKA; Subkey: "Software\Classes\LiteMark.txt"; ValueType: string; ValueData: "Text document"; Flags: uninsdeletekey; Tasks: openwith
Root: HKA; Subkey: "Software\Classes\LiteMark.txt\DefaultIcon"; ValueType: string; ValueData: "{app}\litemark.exe,0"; Tasks: openwith
Root: HKA; Subkey: "Software\Classes\LiteMark.txt\shell\open\command"; ValueType: string; ValueData: """{app}\litemark.exe"" ""%1"""; Tasks: openwith
Root: HKA; Subkey: "Software\Classes\.txt\OpenWithProgids"; ValueType: none; ValueName: "LiteMark.txt"; ValueData: ""; Flags: uninsdeletevalue; Tasks: openwith

[Run]
Filename: "{app}\litemark.exe"; Description: "Launch LiteMark"; Flags: nowait postinstall skipifsilent
