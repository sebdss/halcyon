//! Compte Microsoft officiel → Minecraft Java Edition.
//!
//! Connexion par code (« device code ») : l'utilisateur saisit un code sur le site de
//! Microsoft ; Halcyon ne voit jamais son mot de passe. Chaîne : Microsoft (OAuth 2) →
//! Xbox Live → XSTS → `api.minecraftservices.com` → vérification de la licence → profil.
//!
//! Le jeton de renouvellement est chiffré par DPAPI (lié au compte Windows) ; sous Linux il
//! est écrit dans un fichier lisible par l'utilisateur seul (`0600`). Il n'est jamais
//! journalisé ni exporté.

use crate::http::{Error, Http, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

const MS: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0";
const SCOPE: &str = "XboxLive.signin offline_access";
const XBL: &str = "https://user.auth.xboxlive.com/user/authenticate";
const XSTS: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";
const MC: &str = "https://api.minecraftservices.com";

/// Code à saisir sur le site de Microsoft.
#[derive(Clone, Debug, PartialEq)]
pub struct DeviceCode {
    pub user_code: String,
    pub verification_uri: String,
    pub device_code: String,
    pub interval: u64,
    pub expires_in: u64,
}

/// Compte connecté (sans secret) ; le jeton Minecraft n'est gardé qu'en mémoire.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Account {
    pub uuid: String,
    pub name: String,
    #[serde(default)]
    pub xuid: String,
    #[serde(default)]
    pub skin_url: Option<String>,
    #[serde(skip)]
    pub access_token: String,
    /// Expiration du jeton Minecraft (secondes Unix).
    #[serde(skip)]
    pub expires_at: i64,
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Message clair pour les erreurs connues de Xbox Live / Minecraft.
pub fn explain(e: &Error) -> String {
    let text = e.to_string();
    if text.contains("2148916233") {
        return tr!("ce compte Microsoft n'a pas de profil Xbox : créez-le sur xbox.com, puis réessayez").into();
    }
    if text.contains("2148916235") {
        return tr!("Xbox Live n'est pas disponible dans le pays de ce compte").into();
    }
    if text.contains("2148916236") || text.contains("2148916237") {
        return tr!("ce compte doit d'abord confirmer l'âge de son titulaire sur xbox.com").into();
    }
    if text.contains("2148916238") {
        return tr!("compte d'un mineur : il doit être ajouté à une famille Microsoft (account.microsoft.com/family)").into();
    }
    if text.contains("Invalid app registration") || text.contains("AppRegInfo") {
        return tr!("Mojang n'a pas encore autorisé l'identifiant d'application de Halcyon (aka.ms/mce-reviewappid)").into();
    }
    if text.contains("AADSTS70002") || text.contains("AADSTS7000218") {
        return tr!("l'application Azure doit autoriser les « flux de clients publics » (Authentification)").into();
    }
    if text.contains("AADSTS700016") {
        return tr!("identifiant d'application Azure inconnu : vérifiez-le dans les paramètres Minecraft").into();
    }
    text
}

pub struct Auth<'a> {
    pub http: &'a Http,
    pub client_id: &'a str,
}

impl Auth<'_> {
    fn check(&self) -> Result<()> {
        if self.client_id.trim().is_empty() {
            return Err(Error::Invalid(tr!("aucun identifiant d'application Azure configuré (Minecraft › Paramètres)").into()));
        }
        Ok(())
    }

    pub fn start(&self) -> Result<DeviceCode> {
        self.check()?;
        let v = self.http.post_form(&format!("{MS}/devicecode"), &[("client_id", self.client_id.trim()), ("scope", SCOPE)])?;
        let s = |k: &str| v.get(k).and_then(|x| x.as_str()).map(str::to_string);
        Ok(DeviceCode {
            user_code: s("user_code").ok_or_else(|| Error::Invalid(tr!("réponse de Microsoft incomplète").into()))?,
            verification_uri: s("verification_uri").unwrap_or_else(|| "https://www.microsoft.com/link".into()),
            device_code: s("device_code").ok_or_else(|| Error::Invalid(tr!("réponse de Microsoft incomplète").into()))?,
            interval: v.get("interval").and_then(|x| x.as_u64()).unwrap_or(5).max(1),
            expires_in: v.get("expires_in").and_then(|x| x.as_u64()).unwrap_or(900),
        })
    }

    /// Attend la saisie du code ; renvoie le jeton de renouvellement et le jeton Microsoft.
    pub fn wait(&self, code: &DeviceCode, cancel: &AtomicBool) -> Result<(String, String)> {
        let deadline = now() + code.expires_in as i64;
        let mut interval = code.interval;
        loop {
            for _ in 0..interval * 10 {
                if cancel.load(Ordering::Relaxed) {
                    return Err(Error::Cancelled);
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            if now() > deadline {
                return Err(Error::Invalid(tr!("le code a expiré : recommencez la connexion").into()));
            }
            let r = self.http.post_form(
                &format!("{MS}/token"),
                &[("grant_type", "urn:ietf:params:oauth:grant-type:device_code"), ("client_id", self.client_id.trim()), ("device_code", &code.device_code)],
            );
            match r {
                Ok(v) => return tokens(&v),
                Err(Error::Http(_, body)) if body.contains("authorization_pending") => {}
                Err(Error::Http(_, body)) if body.contains("slow_down") => interval += 5,
                Err(Error::Http(_, body)) if body.contains("authorization_declined") => return Err(Error::Invalid(tr!("connexion refusée sur le site de Microsoft").into())),
                Err(Error::Http(_, body)) if body.contains("expired_token") => return Err(Error::Invalid(tr!("le code a expiré : recommencez la connexion").into())),
                Err(e) => return Err(e),
            }
        }
    }

    pub fn refresh(&self, refresh_token: &str) -> Result<(String, String)> {
        self.check()?;
        let v = self.http.post_form(
            &format!("{MS}/token"),
            &[("grant_type", "refresh_token"), ("client_id", self.client_id.trim()), ("refresh_token", refresh_token), ("scope", SCOPE)],
        )?;
        tokens(&v)
    }

    /// Microsoft → Xbox Live → XSTS → Minecraft, puis licence et profil.
    pub fn minecraft(&self, ms_access: &str) -> Result<Account> {
        let xbl = self.http.post_json(
            XBL,
            &serde_json::json!({"Properties": {"AuthMethod": "RPS", "SiteName": "user.auth.xboxlive.com", "RpsTicket": format!("d={ms_access}")},
                                "RelyingParty": "http://auth.xboxlive.com", "TokenType": "JWT"}),
            &[],
        )?;
        let xbl_token = xbl.get("Token").and_then(|t| t.as_str()).ok_or_else(|| Error::Invalid(tr!("Xbox Live : réponse sans jeton").into()))?;
        let xsts = self.http.post_json(
            XSTS,
            &serde_json::json!({"Properties": {"SandboxId": "RETAIL", "UserTokens": [xbl_token]}, "RelyingParty": "rp://api.minecraftservices.com/", "TokenType": "JWT"}),
            &[],
        )?;
        let xsts_token = xsts.get("Token").and_then(|t| t.as_str()).ok_or_else(|| Error::Invalid(tr!("XSTS : réponse sans jeton").into()))?;
        let uhs = xsts.pointer("/DisplayClaims/xui/0/uhs").and_then(|u| u.as_str()).ok_or_else(|| Error::Invalid(tr!("XSTS : identifiant utilisateur absent").into()))?;
        let xuid = xsts.pointer("/DisplayClaims/xui/0/xid").and_then(|u| u.as_str()).unwrap_or_default().to_string();
        let mc = self.http.post_json(&format!("{MC}/authentication/login_with_xbox"), &serde_json::json!({"identityToken": format!("XBL3.0 x={uhs};{xsts_token}")}), &[])?;
        let token = mc.get("access_token").and_then(|t| t.as_str()).ok_or_else(|| Error::Invalid(tr!("Minecraft : réponse sans jeton").into()))?.to_string();
        let expires = mc.get("expires_in").and_then(|e| e.as_i64()).unwrap_or(86_400);
        let bearer = format!("Bearer {token}");
        let ent = self.http.get_json(&format!("{MC}/entitlements/mcstore"), &[("Authorization", &bearer)])?;
        if !owns_java(&ent) {
            return Err(Error::Invalid(tr!("ce compte ne possède pas Minecraft Java Edition (achat ou Xbox Game Pass PC requis)").into()));
        }
        let profile = match self.http.get_json(&format!("{MC}/minecraft/profile"), &[("Authorization", &bearer)]) {
            Ok(p) => p,
            Err(Error::Http(404, _)) => return Err(Error::Invalid(tr!("ce compte n'a pas encore de profil Minecraft : lancez une fois le launcher officiel pour choisir un pseudo").into())),
            Err(e) => return Err(e),
        };
        Ok(Account {
            uuid: profile.get("id").and_then(|i| i.as_str()).unwrap_or_default().to_string(),
            name: profile.get("name").and_then(|i| i.as_str()).unwrap_or_default().to_string(),
            skin_url: profile.pointer("/skins/0/url").and_then(|u| u.as_str()).map(str::to_string),
            xuid,
            access_token: token,
            expires_at: now() + expires - 300,
        })
    }
}

fn tokens(v: &serde_json::Value) -> Result<(String, String)> {
    let refresh = v.get("refresh_token").and_then(|t| t.as_str()).ok_or_else(|| Error::Invalid(tr!("Microsoft : jeton de renouvellement absent").into()))?;
    let access = v.get("access_token").and_then(|t| t.as_str()).ok_or_else(|| Error::Invalid(tr!("Microsoft : jeton d'accès absent").into()))?;
    Ok((refresh.to_string(), access.to_string()))
}

/// Licence Java : `game_minecraft` (achat) ou `product_minecraft` (Game Pass compris).
pub fn owns_java(entitlements: &serde_json::Value) -> bool {
    entitlements
        .get("items")
        .and_then(|i| i.as_array())
        .is_some_and(|items| items.iter().any(|i| matches!(i.get("name").and_then(|n| n.as_str()), Some("game_minecraft") | Some("product_minecraft"))))
}

// ------------------------------------------------------------------ stockage des comptes

/// Comptes enregistrés : `accounts.json` (noms, UUID) et secrets chiffrés à part.
pub struct Store {
    dir: PathBuf,
}

#[derive(Default, Serialize, Deserialize)]
struct Accounts {
    #[serde(default)]
    active: Option<String>,
    #[serde(default)]
    accounts: Vec<Account>,
}

impl Store {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    fn list_path(&self) -> PathBuf {
        self.dir.join("accounts.json")
    }

    fn secret_path(&self, uuid: &str) -> PathBuf {
        self.dir.join("secrets").join(format!("{}.bin", uuid.chars().filter(|c| c.is_ascii_hexdigit()).collect::<String>()))
    }

    fn read(&self) -> Accounts {
        std::fs::read_to_string(self.list_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    fn write(&self, a: &Accounts) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        let tmp = self.list_path().with_extension("json.part");
        std::fs::write(&tmp, serde_json::to_vec_pretty(a).unwrap_or_default())?;
        std::fs::rename(tmp, self.list_path())
    }

    pub fn accounts(&self) -> (Vec<Account>, Option<String>) {
        let a = self.read();
        (a.accounts, a.active)
    }

    /// Enregistre (ou met à jour) un compte et son jeton de renouvellement, et l'active.
    pub fn save(&self, account: &Account, refresh_token: &str) -> std::io::Result<()> {
        if account.uuid.is_empty() {
            return Err(std::io::Error::other(tr!("compte sans identifiant")));
        }
        write_secret(&self.secret_path(&account.uuid), refresh_token.as_bytes())?;
        let mut a = self.read();
        a.accounts.retain(|x| x.uuid != account.uuid);
        a.accounts.push(account.clone());
        a.active = Some(account.uuid.clone());
        self.write(&a)
    }

    pub fn set_active(&self, uuid: &str) -> std::io::Result<()> {
        let mut a = self.read();
        if a.accounts.iter().any(|x| x.uuid == uuid) {
            a.active = Some(uuid.to_string());
        }
        self.write(&a)
    }

    pub fn refresh_token(&self, uuid: &str) -> Option<String> {
        read_secret(&self.secret_path(uuid)).and_then(|b| String::from_utf8(b).ok())
    }

    /// Déconnexion : compte et secret supprimés.
    pub fn remove(&self, uuid: &str) -> std::io::Result<()> {
        let _ = std::fs::remove_file(self.secret_path(uuid));
        let mut a = self.read();
        a.accounts.retain(|x| x.uuid != uuid);
        if a.active.as_deref() == Some(uuid) {
            a.active = a.accounts.first().map(|x| x.uuid.clone());
        }
        self.write(&a)
    }
}

fn write_secret(path: &Path, data: &[u8]) -> std::io::Result<()> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    let blob = protect(data)?;
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
        opts.open(&tmp)?.write_all(&blob)?;
    }
    std::fs::rename(tmp, path)
}

fn read_secret(path: &Path) -> Option<Vec<u8>> {
    unprotect(&std::fs::read(path).ok()?)
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
    let bytes = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec() };
    unsafe { LocalFree(out.pbData as _) };
    Ok(bytes)
}

#[cfg(windows)]
fn unprotect(data: &[u8]) -> Option<Vec<u8>> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{CryptUnprotectData, CRYPT_INTEGER_BLOB};
    let input = CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 };
    let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
    // SAFETY : comme ci-dessus.
    let ok = unsafe { CryptUnprotectData(&input, std::ptr::null_mut(), std::ptr::null(), std::ptr::null(), std::ptr::null(), 0, &mut out) };
    if ok == 0 {
        return None;
    }
    let bytes = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec() };
    unsafe { LocalFree(out.pbData as _) };
    Some(bytes)
}

/// Linux : pas de trousseau universel ; le fichier n'est lisible que par l'utilisateur.
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
    use serde_json::json;

    #[test]
    fn licence_and_errors() {
        assert!(owns_java(&json!({"items":[{"name":"product_minecraft","signature":"x"},{"name":"game_minecraft"}]})));
        assert!(owns_java(&json!({"items":[{"name":"product_minecraft"}]})));
        assert!(!owns_java(&json!({"items":[{"name":"product_minecraft_bedrock"}]})));
        assert!(!owns_java(&json!({"items":[]})));
        assert!(explain(&Error::Http(401, "{\"XErr\":2148916238}".into())).contains("famille"));
        assert!(explain(&Error::Http(403, "Invalid app registration, see https://aka.ms/AppRegInfo".into())).contains("Mojang"));
        let auth = Auth { http: &Http::new(), client_id: " " };
        assert!(auth.start().is_err(), "pas de requête sans identifiant");
    }

    #[test]
    fn store_keeps_secret_apart() {
        let t = tempfile::tempdir().unwrap();
        let s = Store::new(t.path());
        let a = Account { uuid: "0123456789abcdef0123456789abcdef".into(), name: "Sebi".into(), xuid: "1".into(), skin_url: None, access_token: "MCTOKEN".into(), expires_at: 1 };
        s.save(&a, "REFRESH-SECRET").unwrap();
        let list = std::fs::read_to_string(t.path().join("accounts.json")).unwrap();
        assert!(list.contains("Sebi") && !list.contains("REFRESH-SECRET") && !list.contains("MCTOKEN"), "aucun secret dans la liste");
        assert_eq!(s.refresh_token(&a.uuid).as_deref(), Some("REFRESH-SECRET"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(t.path().join("secrets").join(format!("{}.bin", a.uuid))).unwrap().permissions().mode();
            assert_eq!(mode & 0o077, 0, "fichier lisible par l'utilisateur seul");
        }
        let (accounts, active) = s.accounts();
        assert_eq!(accounts.len(), 1);
        assert_eq!(active.as_deref(), Some(a.uuid.as_str()));
        s.remove(&a.uuid).unwrap();
        assert!(s.refresh_token(&a.uuid).is_none());
        assert!(s.accounts().0.is_empty());
    }
}
