//! Instance unique : une seconde exécution (raccourci de jeu, Sunshine, association de
//! fichier) transmet sa demande à Halcyon déjà ouvert puis se ferme.
//!
//! Canal local : TCP sur 127.0.0.1 (port choisi par le système) protégé par un jeton
//! aléatoire, écrits dans `instance.json` (lisible par l'utilisateur seul sous Unix).

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Show,
    Quick,
    BigPicture,
    Launch(i64),
    Open(PathBuf),
}

impl Command {
    fn encode(&self) -> String {
        match self {
            Self::Show => "show".into(),
            Self::Quick => "quick".into(),
            Self::BigPicture => "bigpicture".into(),
            Self::Launch(id) => format!("launch {id}"),
            Self::Open(p) => format!("open {}", p.display()),
        }
    }

    fn decode(s: &str) -> Option<Self> {
        let (verb, arg) = s.split_once(' ').unwrap_or((s, ""));
        match verb {
            "show" => Some(Self::Show),
            "quick" => Some(Self::Quick),
            "bigpicture" => Some(Self::BigPicture),
            "launch" => arg.trim().parse().ok().map(Self::Launch),
            "open" if !arg.is_empty() => Some(Self::Open(PathBuf::from(arg))),
            _ => None,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct Lock {
    port: u16,
    token: String,
    pid: u32,
}

fn lock_path(data_dir: &Path) -> PathBuf {
    data_dir.join("instance.json")
}

fn new_token() -> String {
    // `RandomState` est initialisé par l'aléa du système d'exploitation.
    use std::hash::BuildHasher;
    let mut h = Sha256::new();
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    for i in 0..4u64 {
        h.update(std::collections::hash_map::RandomState::new().hash_one((i, now.as_nanos(), std::process::id())).to_le_bytes());
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Transmet la demande à une instance déjà ouverte. `true` si elle l'a acceptée.
pub fn forward(data_dir: &Path, cmd: &Command) -> bool {
    let Ok(text) = std::fs::read_to_string(lock_path(data_dir)) else { return false };
    let Ok(lock) = serde_json::from_str::<Lock>(&text) else { return false };
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, lock.port));
    let Ok(mut stream) = TcpStream::connect_timeout(&addr, Duration::from_millis(600)) else { return false };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    if writeln!(stream, "{} {}", lock.token, cmd.encode()).is_err() {
        return false;
    }
    let mut reply = String::new();
    BufReader::new(stream).read_line(&mut reply).is_ok() && reply.trim() == "ok"
}

/// Écoute les demandes des instances suivantes (thread dédié).
pub fn listen(data_dir: &Path, on: impl Fn(Command) + Send + 'static) {
    let listener = match TcpListener::bind((Ipv4Addr::LOCALHOST, 0)) {
        Ok(l) => l,
        Err(e) => {
            log::warn!("instance unique indisponible : {e}");
            return;
        }
    };
    let Ok(port) = listener.local_addr().map(|a| a.port()) else { return };
    let token = new_token();
    let lock = Lock { port, token: token.clone(), pid: std::process::id() };
    let path = lock_path(data_dir);
    let tmp = path.with_extension("tmp");
    let written = std::fs::write(&tmp, serde_json::to_string(&lock).unwrap_or_default()).and_then(|_| {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
        }
        std::fs::rename(&tmp, &path)
    });
    if let Err(e) = written {
        log::warn!("instance unique : {e}");
        return;
    }
    std::thread::Builder::new()
        .name("halcyon-instance".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                let mut line = String::new();
                let mut reader = BufReader::new(&stream);
                if reader.read_line(&mut line).is_err() || line.len() > 4096 {
                    continue;
                }
                let line = line.trim_end();
                let Some((t, rest)) = line.split_once(' ') else { continue };
                let mut out = &stream;
                if t != token {
                    let _ = writeln!(out, "refusé");
                    continue;
                }
                match Command::decode(rest) {
                    Some(cmd) => {
                        let _ = writeln!(out, "ok");
                        on(cmd);
                    }
                    None => {
                        let _ = writeln!(out, "inconnu");
                    }
                }
            }
        })
        .ok();
}

/// Supprime le verrou à la fermeture (seulement s'il appartient à ce processus).
pub fn release(data_dir: &Path) {
    let path = lock_path(data_dir);
    if let Ok(lock) = std::fs::read_to_string(&path).map_err(|_| ()).and_then(|t| serde_json::from_str::<Lock>(&t).map_err(|_| ())) {
        if lock.pid == std::process::id() {
            let _ = std::fs::remove_file(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_roundtrip() {
        for c in [Command::Show, Command::Quick, Command::BigPicture, Command::Launch(42), Command::Open(PathBuf::from("/tmp/a b.json"))] {
            assert_eq!(Command::decode(&c.encode()), Some(c));
        }
        assert_eq!(Command::decode("rm -rf"), None);
    }

    #[test]
    fn forwards_to_listener_with_token() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        listen(dir.path(), move |c| {
            let _ = tx.send(c);
        });
        assert!(forward(dir.path(), &Command::Launch(7)));
        assert_eq!(rx.recv_timeout(Duration::from_secs(2)).unwrap(), Command::Launch(7));
        // Jeton falsifié : refusé.
        let mut lock: Lock = serde_json::from_str(&std::fs::read_to_string(lock_path(dir.path())).unwrap()).unwrap();
        lock.token = "faux".into();
        std::fs::write(lock_path(dir.path()), serde_json::to_string(&lock).unwrap()).unwrap();
        assert!(!forward(dir.path(), &Command::Show));
    }
}
