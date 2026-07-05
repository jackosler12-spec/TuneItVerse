[Setup]
AppName=TuneItVerse
AppVersion=0.2.0
AppPublisher=JRTuners
AppPublisherURL=https://github.com/jackosler12-spec/TuneItVerse
AppSupportURL=https://github.com/jackosler12-spec/TuneItVerse
AppUpdatesURL=https://github.com/jackosler12-spec/TuneItVerse
DefaultDirName={userappdata}\TuneItVerse
DefaultGroupName=TuneItVerse
DisableProgramGroupPage=yes
OutputDir=..\..\
OutputBaseFilename=TuneItVerse-Setup
Compression=lzma2
SolidCompression=yes
SetupIconFile=..\src-tauri\icons\icon.ico
UninstallDisplayIcon={app}\TuneItVerse.exe
PrivilegesRequired=lowest

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "..\src-tauri\target\release\bundle\appimage\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs
; Note: Adjust source path after confirming Tauri output structure

[Icons]
Name: "{group}\TuneItVerse"; Filename: "{app}\TuneItVerse.exe"
Name: "{autodesktop}\TuneItVerse"; Filename: "{app}\TuneItVerse.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\TuneItVerse.exe"; Description: "{cm:LaunchProgram,TuneItVerse}"; Flags: nowait postinstall skipifsilent