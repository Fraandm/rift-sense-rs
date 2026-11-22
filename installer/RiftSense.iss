#define MyAppName "RiftSense"
#ifndef MyAppVersion
  #define MyAppVersion "0.5.0"
#endif
#define MyAppPublisher "RiftSense"
#define MyAppURL "https://github.com/Fraandm/rift-sense-rs"
#define MyAppExeName "riftsense-desktop.exe"

[Setup]
AppId={{8F3C1E2A-7B4D-4E91-9C6A-2D5F8B1A3C7E}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppVerName={#MyAppName} {#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
AppUpdatesURL={#MyAppURL}/releases
DefaultDirName={localappdata}\Programs\RiftSense
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
OutputDir=..\dist
OutputBaseFilename=RiftSense-Setup-{#MyAppVersion}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64
ArchitecturesInstallIn64BitMode=x64
CloseApplications=yes
RestartApplications=no
UninstallDisplayIcon={app}\RiftSense.ico
SetupIconFile=..\assets\RiftSense.ico
SetupLogging=yes
VersionInfoVersion={#MyAppVersion}
VersionInfoDescription={#MyAppName} installer
VersionInfoProductName={#MyAppName}
VersionInfoProductVersion={#MyAppVersion}

[Languages]
Name: "spanish"; MessagesFile: "compiler:Languages\Spanish.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "payload\*"; DestDir: "{app}"; Excludes: "MicrosoftEdgeWebview2Setup.exe"; Flags: recursesubdirs createallsubdirs ignoreversion
#ifdef WebView2Setup
Source: "payload\MicrosoftEdgeWebview2Setup.exe"; DestDir: "{app}"; Flags: ignoreversion
#endif

[Icons]
Name: "{autoprograms}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\RiftSense.ico"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\RiftSense.ico"; Tasks: desktopicon

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: none; ValueName: "RiftSense"; Flags: uninsdeletevalue

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#MyAppName}}"; Flags: nowait postinstall

[Code]
const
  WebView2Client = '{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}';

function IsWebView2Installed: Boolean;
var
  Version: String;
begin
  Result :=
    (RegQueryStringValue(HKLM, 'SOFTWARE\Microsoft\EdgeUpdate\Clients\' + WebView2Client, 'pv', Version) and (Version <> '')) or
    (RegQueryStringValue(HKLM, 'SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\' + WebView2Client, 'pv', Version) and (Version <> '')) or
    (RegQueryStringValue(HKCU, 'Software\Microsoft\EdgeUpdate\Clients\' + WebView2Client, 'pv', Version) and (Version <> ''));
end;

procedure EnsureWebView2;
var
  Bootstrapper: String;
  ResultCode: Integer;
begin
  if IsWebView2Installed then
    Exit;

  Bootstrapper := ExpandConstant('{app}\MicrosoftEdgeWebview2Setup.exe');
  if FileExists(Bootstrapper) then
  begin
    if MsgBox('RiftSense necesita Microsoft Edge WebView2. Se instalara ahora para que la aplicacion pueda abrirse.', mbConfirmation, MB_YESNO) = IDYES then
    begin
      if (not Exec(Bootstrapper, '/silent /install', '', SW_SHOW, ewWaitUntilTerminated, ResultCode)) or (ResultCode <> 0) then
        MsgBox('No se pudo instalar WebView2 automaticamente. Descargalo en https://go.microsoft.com/fwlink/p/?LinkId=2124703', mbError, MB_OK);
    end;
  end
  else
    MsgBox('RiftSense necesita Microsoft Edge WebView2. Instalalo desde https://go.microsoft.com/fwlink/p/?LinkId=2124703 antes de abrir la aplicacion.', mbInformation, MB_OK);
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then
    EnsureWebView2;
end;
