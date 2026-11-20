; The Windows installer: Pina Studio, the `pina` command beside it, a Start menu entry, and (if
; asked) `pina` on the PATH for agents and scripts. Per user, so it needs no administrator.
;
; Built by .github/workflows/windows.yml:
;   iscc /DVersion=<x.y.z> /DSource=<folder with "Pina Studio.exe" and pina.exe> scripts\windows\pina.iss

#ifndef Version
  #define Version "0.0.0"
#endif
#ifndef Source
  #define Source "..\..\target\release"
#endif

[Setup]
AppId={{9E486707-A013-4C3C-A154-B9E3C2F96256}
AppName=Pina Studio
AppVersion={#Version}
AppPublisher=Manyeya Khutso Siema
AppCopyright=Copyright (c) 2026 Manyeya Khutso Siema
AppPublisherURL=https://manyeya.github.io/pina-studio/
AppSupportURL=https://github.com/manyeya/pina-studio/issues
AppUpdatesURL=https://github.com/manyeya/pina-studio/releases
VersionInfoCompany=Manyeya Khutso Siema
VersionInfoCopyright=Copyright (c) 2026 Manyeya Khutso Siema
VersionInfoDescription=Pina Studio installer
VersionInfoProductName=Pina Studio
VersionInfoVersion={#Version}
; Which licence covers what (MIT source, GPL-3.0-or-later build), shown before installing.
LicenseFile={#Source}\NOTICE.txt
DefaultDirName={autopf}\Pina Studio
DefaultGroupName=Pina Studio
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
OutputDir=..\..\dist
OutputBaseFilename=PinaStudio-{#Version}-windows-x64-setup
SetupIconFile=..\..\crates\app\icon\pina.ico
UninstallDisplayIcon={app}\pina.ico
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ChangesEnvironment=yes

[Tasks]
Name: "desktopicon"; Description: "A shortcut on the desktop"; Flags: unchecked
Name: "path"; Description: "The pina command on the PATH, for agents and scripts"

[Files]
Source: "{#Source}\Pina Studio.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Source}\pina.exe"; DestDir: "{app}"; Flags: ignoreversion
; DirectML, newer than Windows' own, for ONNX Runtime (stem separation); and the C runtime's DLLs,
; Visual C++'s redistributable, so it runs on a PC without it installed.
Source: "{#Source}\*.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\crates\app\icon\pina.ico"; DestDir: "{app}"; Flags: ignoreversion
; Its licences, and every open-source component's.
Source: "{#Source}\LICENSE.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Source}\COPYING.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Source}\NOTICE.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Source}\THIRD-PARTY-NOTICES.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Pina Studio"; Filename: "{app}\Pina Studio.exe"; IconFilename: "{app}\pina.ico"
Name: "{autodesktop}\Pina Studio"; Filename: "{app}\Pina Studio.exe"; IconFilename: "{app}\pina.ico"; Tasks: desktopicon

[Registry]
Root: HKCU; Subkey: "Environment"; ValueType: expandsz; ValueName: "Path"; ValueData: "{olddata};{app}"; Tasks: path; Check: NotOnPath(ExpandConstant('{app}'))

[Run]
Filename: "{app}\Pina Studio.exe"; Description: "Open Pina Studio"; Flags: nowait postinstall skipifsilent

[Code]
{ Whether the folder isn't on the person's PATH yet. }
function NotOnPath(Dir: string): Boolean;
var
  Path: string;
begin
  if not RegQueryStringValue(HKCU, 'Environment', 'Path', Path) then
  begin
    Result := True;
    exit;
  end;
  Result := Pos(';' + Uppercase(Dir) + ';', ';' + Uppercase(Path) + ';') = 0;
end;
