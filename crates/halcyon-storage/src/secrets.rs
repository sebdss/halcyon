//! Coffre des secrets locaux (clés d'API personnelles).
//!
//! Jusqu'à la 0.5.0, les clés d'API (Steam, RetroAchievements, Nexus Mods, SteamGridDB,
//! CurseForge) étaient écrites **en clair** dans `settings.json`, avec les droits par défaut
//! du système (lisible par les autres comptes locaux sous Linux). Depuis la 1.0.0 :
//!
//! - chaque secret est un fichier distinct de `<données>/secrets/`, écrit de manière
//!   atomique ;
//! - **Windows** : contenu chiffré par DPAPI (lié au compte Windows de l'utilisateur) ;
//! - **Linux** : fichier `0600` dans un dossier `0700`. Il n'existe pas de trousseau
//!   universellement disponible (Secret Service absent de nombreux bureaux légers et des
//!   sessions Gamescope) : la protection repose sur les droits du compte, comme pour
//!   `~/.ssh` ;
//! - `settings.json` ne contient plus que des champs vides pour ces clés.
//!
//! Aucun secret n'est jamais journalisé : le journal de sécurité indique seulement quelle
//! clé a été définie ou retirée.

use std::path::{Path, PathBuf};

/// Coffre attaché à un dossier.
#[derive(Clone, Debug)]
pub struct SecretStore {
    dir: PathBuf,
}

impl SecretStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn path(&self, name: &str) -> Option<PathBuf> {
        // Noms limités à [a-z0-9_.-] : aucun chemin ne peut sortir du dossier.
        let ok = !name.is_empty() && name.len() <= 64 && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '-' | '.')) && !name.starts_with('.');
        ok.then(|| self.dir.join(format!("{name}.secret")))
    }

    /// Valeur d'un secret, `None` s'il est absent ou illisible (fichier d'un autre compte
    /// Windows, corrompu…).
    pub fn get(&self, name: &str) -> Option<String> {
        let data = std::fs::read(self.path(name)?).ok()?;
        let plain = unprotect(&data)?;
        String::from_utf8(plain).ok()
    }

    /// Définit (ou retire si `value` est vide) un secret. N'écrit rien si la valeur ne change
    /// pas. Renvoie `true` si le coffre a été modifié.
    pub fn set(&self, name: &str, value: &str) -> std::io::Result<bool> {
        let path = self.path(name).ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "nom de secret invalide"))?;
        let value = value.trim();
        if value.is_empty() {
            return match std::fs::remove_file(&path) {
                Ok(()) => {
                    crate::audit::event("secret.removed", name);
                    Ok(true)
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
                Err(e) => Err(e),
            };
        }
        if self.get(name).as_deref() == Some(value) {
            return Ok(false);
        }
        create_private_dir(&self.dir)?;
        let blob = protect(value.as_bytes())?;
        write_private(&path, &blob)?;
        crate::audit::event("secret.set", name);
        Ok(true)
    }
}

/// Crée le dossier avec des droits réservés à l'utilisateur (Unix : `0700`).
pub fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// Écriture atomique d'un fichier lisible par l'utilisateur seul (Unix : `0600`).
pub fn write_private(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("part");
    {
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        use std::io::Write;
        let mut f = opts.open(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
    }
    #[cfg(unix)]
    {
        // Un fichier temporaire préexistant garderait ses anciens droits.
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
    }
    std::fs::rename(tmp, path)
}

#[cfg(windows)]
fn protect(data: &[u8]) -> std::io::Result<Vec<u8>> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{CryptProtectData, CRYPT_INTEGER_BLOB};
    let input = CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 };
    let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
    // SAFETY : tampons valides pendant l'appel ; la sortie est libérée par LocalFree.
    let ok = unsafe { CryptProtectData(&input, std::ptr::null(), std::ptr::null(), std::ptr::null(), std::ptr::null(), 0, &mut out) };
    if ok == 0 {
        return Err(std::io::Error::last_os_error());
    }
    let v = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec() };
    unsafe { LocalFree(out.pbData as _) };
    Ok(v)
}

#[cfg(windows)]
fn unprotect(data: &[u8]) -> Option<Vec<u8>> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{CryptUnprotectData, CRYPT_INTEGER_BLOB};
    let input = CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 };
    let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
    // SAFETY : idem `protect`.
    let ok = unsafe { CryptUnprotectData(&input, std::ptr::null_mut(), std::ptr::null(), std::ptr::null(), std::ptr::null(), 0, &mut out) };
    if ok == 0 {
        return None;
    }
    let v = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec() };
    unsafe { LocalFree(out.pbData as _) };
    Some(v)
}

#[cfg(not(windows))]
fn protect(data: &[u8]) -> std::io::Result<Vec<u8>> {
    Ok(data.to_vec())
}

#[cfg(not(windows))]
fn unprotect(data: &[u8]) -> Option<Vec<u8>> {
    Some(data.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_get_remove_and_reject_bad_names() {
        let dir = tempfile::tempdir().unwrap();
        let s = SecretStore::new(dir.path().join("secrets"));
        assert_eq!(s.get("steam_api_key"), None);
        assert!(s.set("steam_api_key", "  ABC123 ").unwrap());
        assert_eq!(s.get("steam_api_key").as_deref(), Some("ABC123"));
        assert!(!s.set("steam_api_key", "ABC123").unwrap(), "valeur identique : rien n'est réécrit");
        assert!(s.set("steam_api_key", "").unwrap());
        assert_eq!(s.get("steam_api_key"), None);
        assert!(!s.set("steam_api_key", "").unwrap());
        for bad in ["../x", "a/b", "", ".hidden", "UPPER"] {
            assert!(s.set(bad, "v").is_err(), "{bad}");
            assert_eq!(s.get(bad), None);
        }
    }

    #[cfg(unix)]
    #[test]
    fn files_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let s = SecretStore::new(dir.path().join("secrets"));
        s.set("ra_api_key", "K").unwrap();
        let f = std::fs::metadata(dir.path().join("secrets/ra_api_key.secret")).unwrap();
        assert_eq!(f.permissions().mode() & 0o777, 0o600);
        let d = std::fs::metadata(dir.path().join("secrets")).unwrap();
        assert_eq!(d.permissions().mode() & 0o777, 0o700);
    }
}
