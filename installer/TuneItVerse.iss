; Unsigned unless iscc is run with /DSign=yes and a SignTool named signtool.
; Local and CI builds stay unsigned when no certificate is configured.
#ifndef Arch
  #define Arch "x64"
#endif
#ifndef ExePath
  #define ExePath "../TuneItVerse.exe"
#endif
#ifndef Sign
  #define Sign "no"
#endif

[Setup]
AppName=TuneItVerse
AppVersion=3.58.0
AppPublisher=JRTuners
DefaultDirName={userappdata}\TuneItVerse
DefaultGroupName=TuneItVerse
DisableProgramGroupPage=yes
OutputDir=..
OutputBaseFilename=TuneItVerse-Setup-{#Arch}
Compression=lzma2/max
SolidCompression=yes
SetupIconFile=..\src-tauri\icons\icon.ico
UninstallDisplayIcon={app}\TuneItVerse.exe
PrivilegesRequired=lowest
#if Arch == "x86"
ArchitecturesAllowed=x86 x64
#else
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
#endif
#if Sign == "yes"
SignTool=signtool
SignedUninstaller=yes
#endif

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Files]
Source: "{#ExePath}"; DestDir: "{app}"; DestName: "TuneItVerse.exe"; Flags: ignoreversion

[Icons]
Name: "{group}\TuneItVerse"; Filename: "{app}\TuneItVerse.exe"
Name: "{userdesktop}\TuneItVerse"; Filename: "{app}\TuneItVerse.exe"
