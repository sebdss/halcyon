//! Corbeille du système (1.0.0-alpha.6) : un dossier de jeu autonome désinstallé par
//! Halcyon y est envoyé, jamais supprimé directement.
//!
//! - Linux : spécification freedesktop.org (`$XDG_DATA_HOME/Trash/files` + `info/*.trashinfo`),
//!   lue par GNOME, KDE, Xfce… Le dossier est **déplacé** (même disque) : rien n'est copié
//!   ni effacé. Sur un autre disque, la corbeille n'est pas disponible : [`TrashError::OtherDisk`].
//! - Windows : Corbeille par `Microsoft.VisualBasic.FileIO.FileSystem.DeleteDirectory`
//!   (option « SendToRecycleBin »), le chemin passant par une variable d'environnement
//!   (jamais interprété comme du code).

use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq, Eq)]
pub enum TrashError {
    /// Le dossier n'est pas sur le même disque que la corbeille.
    OtherDisk,
    Missing,
    Failed(String),
}

impl std::fmt::Display for TrashError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OtherDisk => f.write_str(&tr!("la corbeille n'est pas disponible pour ce disque")),
            Self::Missing => f.write_str(&tr!("dossier introuvable")),
            Self::Failed(e) => f.write_str(e),
        }
    }
}

/// Envoie un dossier à la corbeille de l'utilisateur.
pub fn move_to_trash(dir: &Path) -> Result<(), TrashError> {
    if std::fs::symlink_metadata(dir).is_err() {
        return Err(TrashError::Missing);
    }
    #[cfg(windows)]
    {
        windows_recycle(dir)
    }
    #[cfg(not(windows))]
    {
        let data = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
            .ok_or_else(|| TrashError::Failed("HOME".into()))?;
        move_to_trash_in(dir, &data.join("Trash"), now_iso())
    }
}

/// Corbeille freedesktop dans `trash` (testable) : déplacement + fichier `.trashinfo`.
pub fn move_to_trash_in(dir: &Path, trash: &Path, deleted_at: String) -> Result<(), TrashError> {
    let files = trash.join("files");
    let info = trash.join("info");
    std::fs::create_dir_all(&files).and_then(|_| std::fs::create_dir_all(&info)).map_err(|e| TrashError::Failed(e.to_string()))?;
    let base = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "game".into());
    // Nom libre dans la corbeille (« Jeu », « Jeu.2 »…) ; le .trashinfo est réservé d'abord.
    let mut n = 1;
    let (name, info_path) = loop {
        let name = if n == 1 { base.clone() } else { format!("{base}.{n}") };
        let ip = info.join(format!("{name}.trashinfo"));
        if !files.join(&name).exists() {
            if let Ok(mut f) = std::fs::OpenOptions::new().write(true).create_new(true).open(&ip) {
                use std::io::Write;
                let body = format!("[Trash Info]\nPath={}\nDeletionDate={deleted_at}\n", encode_path(dir));
                f.write_all(body.as_bytes()).map_err(|e| TrashError::Failed(e.to_string()))?;
                break (name, ip);
            }
        }
        n += 1;
        if n > 10_000 {
            return Err(TrashError::Failed(tr!("trop d'éléments du même nom dans la corbeille")));
        }
    };
    match std::fs::rename(dir, files.join(&name)) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&info_path);
            // EXDEV : autre système de fichiers (pas de copie : on le signale).
            if e.raw_os_error() == Some(18) {
                Err(TrashError::OtherDisk)
            } else {
                Err(TrashError::Failed(e.to_string()))
            }
        }
    }
}

/// Chemin encodé pour `.trashinfo` (RFC 2396 : octets hors caractères sûrs en %XX).
fn encode_path(p: &Path) -> String {
    let s = p.to_string_lossy();
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"/-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(not(windows))]
fn now_iso() -> String {
    // Heure locale du système, au format attendu (AAAA-MM-JJTHH:MM:SS).
    std::process::Command::new("date")
        .arg("+%Y-%m-%dT%H:%M:%S")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| s.len() == 19)
        .unwrap_or_else(|| "1970-01-01T00:00:00".into())
}

#[cfg(windows)]
fn windows_recycle(dir: &Path) -> Result<(), TrashError> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let script = "Add-Type -AssemblyName Microsoft.VisualBasic; \
                  [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteDirectory($env:HALCYON_TRASH_PATH, 'OnlyErrorDialogs', 'SendToRecycleBin')";
    let out = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("HALCYON_TRASH_PATH", dir)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| TrashError::Failed(e.to_string()))?;
    if out.status.success() && !dir.exists() {
        Ok(())
    } else {
        Err(TrashError::Failed(String::from_utf8_lossy(&out.stderr).trim().chars().take(300).collect()))
    }
}

/// Taille d'un dossier (octets), liens symboliques non suivis.
pub fn dir_size(dir: &Path) -> u64 {
    let mut total = 0;
    let mut stack: Vec<PathBuf> = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else { continue };
        for e in entries.flatten() {
            let Ok(m) = e.metadata() else { continue };
            if m.file_type().is_symlink() {
                continue;
            }
            if m.is_dir() {
                stack.push(e.path());
            } else {
                total += m.len();
            }
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_to_trash_with_info_and_unique_names() {
        let tmp = tempfile::tempdir().unwrap();
        let trash = tmp.path().join("Trash");
        for round in 0..2 {
            let game = tmp.path().join("Jeux/Mon Jeu");
            std::fs::create_dir_all(game.join("data")).unwrap();
            std::fs::write(game.join("data/a.pak"), b"1234").unwrap();
            assert_eq!(dir_size(&game), 4);
            move_to_trash_in(&game, &trash, "2026-11-19T10:00:00".into()).unwrap();
            assert!(!game.exists());
            let name = if round == 0 { "Mon Jeu".to_string() } else { "Mon Jeu.2".to_string() };
            assert!(trash.join("files").join(&name).join("data/a.pak").is_file());
            let info = std::fs::read_to_string(trash.join("info").join(format!("{name}.trashinfo"))).unwrap();
            assert!(info.starts_with("[Trash Info]\nPath="), "{info}");
            assert!(info.contains("Mon%20Jeu") && info.contains("DeletionDate=2026-11-19T10:00:00"));
        }
        assert_eq!(move_to_trash(&tmp.path().join("absent")), Err(TrashError::Missing));
    }
}
