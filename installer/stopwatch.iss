#ifndef MyAppVersion
  #define MyAppVersion "0.1.0"
#endif

[Setup]
AppId={{3CF4F53D-23E6-4FA8-89E3-0BB86D5B14A8}
AppName=Stopwatch
AppVersion={#MyAppVersion}
AppPublisher=Stopwatch
DefaultDirName={autopf}\Stopwatch
DefaultGroupName=Stopwatch
OutputDir=..\dist
OutputBaseFilename=stopwatch-setup-{#MyAppVersion}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=lowest
SetupLogging=yes
CloseApplications=force
UninstallDisplayIcon={app}\stopwatch.exe
SetupIconFile=..\assets\icon.ico

[Files]
Source: "..\target\x86_64-pc-windows-msvc\release\stopwatch.exe"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\Stopwatch"; Filename: "{app}\stopwatch.exe"
Name: "{autodesktop}\Stopwatch"; Filename: "{app}\stopwatch.exe"; Tasks: desktopicon

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Additional icons:"

[Run]
Filename: "{app}\stopwatch.exe"; Description: "Launch Stopwatch"; Flags: nowait postinstall skipifsilent
Filename: "{app}\stopwatch.exe"; Flags: nowait skipifnotsilent

[UninstallDelete]
Type: files; Name: "{app}\stopwatch.log"

