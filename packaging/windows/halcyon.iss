; Installeur Windows de Halcyon (Inno Setup 6).
; Compilation : iscc /DAppVersion=0.4.0 /DSourceExe=..\..\target\release\halcyon.exe halcyon.iss

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef SourceExe
  #define SourceExe "..\..\target\release\halcyon.exe"
#endif

[Setup]
AppId={{6B0E3B4F-2D7C-4E1A-9C5B-8F1A7D2E4C90}
AppName=Halcyon
AppVersion={#AppVersion}
AppPublisher=Sebi
AppComments=Launcher universel de jeux vidéo
DefaultDirName={autopf}\Halcyon
DefaultGroupName=Halcyon
DisableProgramGroupPage=yes
; Installation pour l'utilisateur seul (sans droits administrateur) ou pour tous.
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
OutputDir=..\..\dist
OutputBaseFilename=Halcyon-{#AppVersion}-setup
SetupIconFile=halcyon.ico
UninstallDisplayIcon={app}\Halcyon.exe
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; Ferme Halcyon s'il est ouvert avant de remplacer l'exécutable.
CloseApplications=yes
ChangesAssociations=yes

[Languages]
Name: "french"; MessagesFile: "compiler:Languages\French.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#SourceExe}"; DestDir: "{app}"; DestName: "Halcyon.exe"; Flags: ignoreversion
Source: "..\..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\docs\GUIDE.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\crates\halcyon-app\ui\fonts\LICENSE-Inter.txt"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
; L'AppUserModelID permet aux notifications de succès de s'afficher au nom de Halcyon.
Name: "{group}\Halcyon"; Filename: "{app}\Halcyon.exe"; AppUserModelID: "Halcyon.Launcher"
Name: "{autodesktop}\Halcyon"; Filename: "{app}\Halcyon.exe"; Tasks: desktopicon; AppUserModelID: "Halcyon.Launcher"

[Registry]
; Associations : thèmes (.halcyontheme) et extensions (.halcyonplugin) s'ouvrent dans Halcyon,
; qui demande confirmation avant toute installation.
Root: HKA; Subkey: "Software\Classes\.halcyontheme"; ValueType: string; ValueData: "Halcyon.Theme"; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Halcyon.Theme"; ValueType: string; ValueData: "Thème Halcyon"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Halcyon.Theme\DefaultIcon"; ValueType: string; ValueData: "{app}\Halcyon.exe,0"
Root: HKA; Subkey: "Software\Classes\Halcyon.Theme\shell\open\command"; ValueType: string; ValueData: """{app}\Halcyon.exe"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.halcyonplugin"; ValueType: string; ValueData: "Halcyon.Plugin"; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Halcyon.Plugin"; ValueType: string; ValueData: "Extension Halcyon"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Halcyon.Plugin\DefaultIcon"; ValueType: string; ValueData: "{app}\Halcyon.exe,0"
Root: HKA; Subkey: "Software\Classes\Halcyon.Plugin\shell\open\command"; ValueType: string; ValueData: """{app}\Halcyon.exe"" ""%1"""
; Démarrage avec la session (activé depuis Halcyon) : retiré à la désinstallation.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: none; ValueName: "Halcyon"; Flags: uninsdeletevalue dontcreatekey

[UninstallDelete]
Type: files; Name: "{app}\instance.json"

[Run]
Filename: "{app}\Halcyon.exe"; Description: "{cm:LaunchProgram,Halcyon}"; Flags: nowait postinstall skipifsilent

[Code]
// Désinstallation propre : les données personnelles ne sont supprimées qu'avec l'accord
// explicite de l'utilisateur (elles contiennent les copies de sauvegardes de jeux).
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Data, Local: String;
begin
  if CurUninstallStep = usPostUninstall then
  begin
    Data := ExpandConstant('{userappdata}\Halcyon');
    Local := ExpandConstant('{localappdata}\Halcyon');
    if (DirExists(Data) or DirExists(Local)) and not UninstallSilent then
    begin
      if MsgBox('Supprimer aussi vos données Halcyon ?' + #13#10 + #13#10 +
                'Bibliothèque, paramètres, statistiques, profils, mods importés et copies de sauvegardes de jeux conservées par Halcyon (' + Data + ').' + #13#10 + #13#10 +
                'Vos jeux et leurs sauvegardes d''origine ne sont jamais supprimés. Choisissez « Non » pour pouvoir réinstaller Halcyon sans rien perdre.',
                mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES then
      begin
        DelTree(Data, True, True, True);
        DelTree(Local, True, True, True);
      end;
    end;
  end;
end;
