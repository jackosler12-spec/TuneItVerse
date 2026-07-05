[Setup]
AppName=TuneItVerse
AppVersion=0.2.0
AppPublisher=JRTuners
DefaultDirName={userappdata}\TuneItVerse
DefaultGroupName=TuneItVerse
DisableProgramGroupPage=yes
OutputDir=..\..\
OutputBaseFilename=TuneItVerse-Setup
Compression=lzma2/max
SolidCompression=yes
SetupIconFile=..\src-tauri\icons\icon.ico
UninstallDisplayIcon={app}\tuneitverse.exe
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "..\src-tauri\target\release\tuneitverse.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\src-tauri\target\release\*.dll"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs
Source: "..\src-tauri\target\release\resources\*"; DestDir: "{app}\resources"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\TuneItVerse"; Filename: "{app}\tuneitverse.exe"
Name: "{autodesktop}\TuneItVerse"; Filename: "{app}\tuneitverse.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\tuneitverse.exe"; Description: "{cm:LaunchProgram,TuneItVerse}"; Flags: nowait postinstall skipifsilent