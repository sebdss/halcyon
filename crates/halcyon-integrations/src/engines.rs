//! Reconnaissance du moteur d'un jeu d'après les fichiers de son dossier
//! d'installation (lecture seule). Sert à proposer les options de lancement
//! réellement prises en charge (résolution, mode d'affichage…).

use halcyon_core::profiles::Engine;
use std::path::Path;

pub fn detect(install_dir: &Path) -> Engine {
    if !install_dir.is_dir() {
        return Engine::Unknown;
    }
    let has = |rel: &str| install_dir.join(rel).exists();
    if has("UnityPlayer.dll") || has("UnityPlayer.so") || has("UnityPlayer.dylib") {
        return Engine::Unity;
    }
    if has("Engine/Binaries") || has("Engine/Config/BaseEngine.ini") {
        return Engine::Unreal;
    }
    if has("hl2.exe") || has("hl2_linux") || has("hl2.sh") || has("bin/engine.dll") || has("bin/engine.so")
        || has("game/bin/win64/engine2.dll") || has("game/bin/linuxsteamrt64/libengine2.so")
    {
        return Engine::Source;
    }
    let Ok(entries) = std::fs::read_dir(install_dir) else { return Engine::Unknown };
    let mut has_pck = false;
    for e in entries.flatten().take(400) {
        let name = e.file_name().to_string_lossy().to_lowercase();
        let path = e.path();
        if path.is_dir() {
            if name.ends_with("_data")
                && (path.join("globalgamemanagers").exists()
                    || path.join("data.unity3d").exists()
                    || path.join("Managed/UnityEngine.dll").exists())
            {
                return Engine::Unity;
            }
            if path.join("Content/Paks").is_dir() || path.join("Binaries/Win64").is_dir() {
                return Engine::Unreal;
            }
            if path.join("gameinfo.txt").is_file() || path.join("gameinfo.gi").is_file() {
                return Engine::Source;
            }
        } else if name.ends_with(".pck") {
            has_pck = true;
        }
    }
    if has_pck {
        return Engine::Godot;
    }
    Engine::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn recognizes_engines_from_their_files() {
        let tmp = tempfile::tempdir().unwrap();
        let mk = |name: &str, files: &[&str]| {
            let d = tmp.path().join(name);
            for f in files {
                let p = d.join(f);
                fs::create_dir_all(p.parent().unwrap()).unwrap();
                fs::write(p, b"").unwrap();
            }
            d
        };
        assert_eq!(detect(&mk("u", &["Game_Data/globalgamemanagers", "Game.exe"])), Engine::Unity);
        assert_eq!(detect(&mk("u2", &["UnityPlayer.dll"])), Engine::Unity);
        assert_eq!(detect(&mk("ue", &["Game/Content/Paks/Game.pak", "Game.exe"])), Engine::Unreal);
        assert_eq!(detect(&mk("src", &["hl2.exe"])), Engine::Source);
        assert_eq!(detect(&mk("src2", &["portal2/gameinfo.txt"])), Engine::Source);
        assert_eq!(detect(&mk("godot", &["game.pck", "game.x86_64"])), Engine::Godot);
        assert_eq!(detect(&mk("other", &["game.exe", "data.bin"])), Engine::Unknown);
        assert_eq!(detect(Path::new("/nonexistent")), Engine::Unknown);
    }
}
