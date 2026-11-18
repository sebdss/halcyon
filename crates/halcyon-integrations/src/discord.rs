//! Statut Discord (« Joue à … ») par la connexion locale du client Discord (Rich Presence).
//!
//! Discord écoute sur un tube nommé (`\\.\pipe\discord-ipc-N`, Windows) ou une socket Unix
//! (`$XDG_RUNTIME_DIR/discord-ipc-N`, aussi dans les dossiers Flatpak et Snap). Chaque
//! message : code (u32 LE), longueur (u32 LE), JSON. Poignée de main `{"v":1,"client_id"}`,
//! puis `SET_ACTIVITY`. Rien ne passe par Internet depuis Halcyon, aucun jeton n'est demandé :
//! le statut n'est visible que si Discord est ouvert et que le partage d'activité y est actif.
//!
//! Tout tourne sur un fil dédié : une commande en attente ne bloque jamais l'interface, et un
//! Discord absent ou fermé en cours de route est simplement ignoré (nouvel essai à la commande
//! suivante).

use serde_json::{json, Value};
use std::io::{Read, Write};
use std::sync::mpsc::{channel, Sender};

/// Ce que Discord affiche pendant une partie.
#[derive(Clone, Debug, PartialEq)]
pub struct Activity {
    pub game: String,
    /// Deuxième ligne (plateforme, profil…).
    pub state: String,
    /// Début de la partie (horodatage Unix).
    pub started: i64,
    /// Jaquette (adresse https) : Discord la reprend directement.
    pub image: Option<String>,
}

enum Cmd {
    Set(String, Activity),
    Clear,
}

/// Fil de communication avec Discord, créé au premier statut.
#[derive(Default)]
pub struct Presence {
    tx: Option<Sender<Cmd>>,
}

impl Presence {
    pub fn set(&mut self, client_id: &str, activity: Activity) {
        if client_id.is_empty() {
            return;
        }
        self.send(Cmd::Set(client_id.to_string(), activity));
    }

    pub fn clear(&mut self) {
        if self.tx.is_some() {
            self.send(Cmd::Clear);
        }
    }

    fn send(&mut self, cmd: Cmd) {
        if self.tx.is_none() {
            let (tx, rx) = channel::<Cmd>();
            let spawned = std::thread::Builder::new().name("halcyon-discord".into()).spawn(move || {
                let mut conn: Option<(Conn, String)> = None;
                while let Ok(cmd) = rx.recv() {
                    match cmd {
                        Cmd::Set(id, activity) => {
                            // Autre application Discord demandée : nouvelle connexion.
                            if conn.as_ref().is_some_and(|(_, cid)| *cid != id) {
                                conn = None;
                            }
                            for _ in 0..2 {
                                if conn.is_none() {
                                    conn = Conn::open(&id).map(|c| (c, id.clone()));
                                }
                                let Some((c, _)) = conn.as_mut() else { break };
                                match c.set_activity(Some(&activity)) {
                                    Ok(()) => break,
                                    Err(e) => {
                                        log::debug!("statut Discord : {e}");
                                        conn = None;
                                    }
                                }
                            }
                        }
                        Cmd::Clear => {
                            if let Some((mut c, _)) = conn.take() {
                                let _ = c.set_activity(None);
                            }
                        }
                    }
                }
            });
            match spawned {
                Ok(_) => self.tx = Some(tx),
                Err(e) => return log::warn!("statut Discord indisponible : {e}"),
            }
        }
        if let Some(tx) = &self.tx {
            if tx.send(cmd).is_err() {
                self.tx = None;
            }
        }
    }
}

/// Corps de la commande `SET_ACTIVITY` (`None` : efface le statut).
pub fn activity_payload(activity: Option<&Activity>, pid: u32, nonce: u64) -> Value {
    let activity = activity.map(|a| {
        let mut v = json!({
            "details": truncate(&a.game, 128),
            "timestamps": { "start": a.started },
            "assets": { "small_image": "halcyon", "small_text": "Halcyon" },
            "instance": false,
        });
        if !a.state.is_empty() {
            v["state"] = json!(truncate(&a.state, 128));
        }
        if let Some(img) = a.image.as_deref().filter(|u| u.starts_with("https://") && u.len() <= 256) {
            v["assets"]["large_image"] = json!(img);
            v["assets"]["large_text"] = json!(truncate(&a.game, 128));
        }
        v
    });
    json!({ "cmd": "SET_ACTIVITY", "args": { "pid": pid, "activity": activity }, "nonce": nonce.to_string() })
}

/// Discord refuse les champs de plus de 128 caractères.
fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max - 1).collect();
    out.push('…');
    out
}

/// Trame : code, longueur, JSON.
pub fn frame(op: u32, payload: &Value) -> Vec<u8> {
    let body = payload.to_string().into_bytes();
    let mut out = Vec::with_capacity(body.len() + 8);
    out.extend_from_slice(&op.to_le_bytes());
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    out
}

#[cfg(unix)]
type Stream = std::os::unix::net::UnixStream;
#[cfg(windows)]
type Stream = std::fs::File;

struct Conn {
    stream: Stream,
    nonce: u64,
}

impl Conn {
    fn open(client_id: &str) -> Option<Self> {
        let mut stream = candidates().into_iter().find_map(|p| connect(&p))?;
        stream.write_all(&frame(0, &json!({ "v": 1, "client_id": client_id }))).ok()?;
        let (op, reply) = read_frame(&mut stream).ok()?;
        if op != 1 || reply.get("evt").and_then(Value::as_str) != Some("READY") {
            log::info!("Discord a refusé la connexion : {reply}");
            return None;
        }
        log::info!("connecté à Discord (statut de jeu)");
        Some(Self { stream, nonce: 1 })
    }

    fn set_activity(&mut self, activity: Option<&Activity>) -> std::io::Result<()> {
        self.nonce += 1;
        self.stream.write_all(&frame(1, &activity_payload(activity, std::process::id(), self.nonce)))?;
        let (op, reply) = read_frame(&mut self.stream)?;
        if op == 2 {
            return Err(std::io::Error::other(tr!("Discord a fermé la connexion : {reply}", reply = reply.to_string())));
        }
        if reply.get("evt").and_then(Value::as_str) == Some("ERROR") {
            log::info!("statut Discord refusé : {reply}");
        }
        Ok(())
    }
}

fn read_frame(stream: &mut Stream) -> std::io::Result<(u32, Value)> {
    let mut head = [0u8; 8];
    stream.read_exact(&mut head)?;
    let op = u32::from_le_bytes([head[0], head[1], head[2], head[3]]);
    let len = u32::from_le_bytes([head[4], head[5], head[6], head[7]]) as usize;
    if len > 1 << 20 {
        return Err(std::io::Error::other(tr!("Réponse de Discord trop longue")));
    }
    let mut body = vec![0u8; len];
    stream.read_exact(&mut body)?;
    Ok((op, serde_json::from_slice(&body).unwrap_or(Value::Null)))
}

#[cfg(unix)]
fn candidates() -> Vec<std::path::PathBuf> {
    let mut dirs: Vec<std::path::PathBuf> = ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"].iter().filter_map(std::env::var_os).map(Into::into).collect();
    dirs.push("/tmp".into());
    let subs = ["", "app/com.discordapp.Discord", "app/com.discordapp.DiscordCanary", "snap.discord", ".flatpak/dev.vencord.Vesktop/xdg-run"];
    let mut out = Vec::new();
    for d in &dirs {
        for s in subs {
            for i in 0..10 {
                out.push(d.join(s).join(format!("discord-ipc-{i}")));
            }
        }
    }
    out
}

#[cfg(windows)]
fn candidates() -> Vec<std::path::PathBuf> {
    (0..10).map(|i| format!(r"\\.\pipe\discord-ipc-{i}").into()).collect()
}

#[cfg(unix)]
fn connect(path: &std::path::Path) -> Option<Stream> {
    let s = Stream::connect(path).ok()?;
    // Discord répond en quelques millisecondes ; un client figé ne bloque pas le fil.
    s.set_read_timeout(Some(std::time::Duration::from_secs(5))).ok()?;
    s.set_write_timeout(Some(std::time::Duration::from_secs(5))).ok()?;
    Some(s)
}

#[cfg(windows)]
fn connect(path: &std::path::Path) -> Option<Stream> {
    std::fs::OpenOptions::new().read(true).write(true).open(path).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_and_payloads() {
        let f = frame(0, &json!({"v": 1}));
        assert_eq!(&f[..4], &0u32.to_le_bytes());
        assert_eq!(u32::from_le_bytes([f[4], f[5], f[6], f[7]]) as usize, f.len() - 8);

        let a = Activity { game: "Celeste".into(), state: "itch.io".into(), started: 1_700_000_000, image: Some("https://cdn.example/c.jpg".into()) };
        let p = activity_payload(Some(&a), 42, 3);
        assert_eq!(p["cmd"], "SET_ACTIVITY");
        assert_eq!(p["args"]["pid"], 42);
        assert_eq!(p["args"]["activity"]["details"], "Celeste");
        assert_eq!(p["args"]["activity"]["assets"]["large_image"], "https://cdn.example/c.jpg");
        assert_eq!(p["nonce"], "3");
        let p = activity_payload(Some(&Activity { image: Some("http://x".into()), state: String::new(), ..a }), 1, 1);
        assert!(p["args"]["activity"]["assets"].get("large_image").is_none());
        assert!(p["args"]["activity"].get("state").is_none());
        assert!(activity_payload(None, 1, 1)["args"]["activity"].is_null());
        assert_eq!(truncate(&"é".repeat(200), 128).chars().count(), 128);
    }

    /// Faux Discord : vérifie la poignée de main et la commande envoyées.
    #[cfg(unix)]
    #[test]
    fn talks_to_a_local_socket() {
        let dir = std::env::temp_dir().join(format!("halcyon-discord-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("discord-ipc-0");
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        let server = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let (op, hello) = read_frame(&mut s).unwrap();
            assert_eq!(op, 0);
            assert_eq!(hello["client_id"], "123");
            s.write_all(&frame(1, &json!({"evt": "READY", "cmd": "DISPATCH"}))).unwrap();
            let (op, cmd) = read_frame(&mut s).unwrap();
            assert_eq!(op, 1);
            assert_eq!(cmd["args"]["activity"]["details"], "Hades");
            s.write_all(&frame(1, &json!({"cmd": "SET_ACTIVITY", "evt": null}))).unwrap();
        });
        let mut c = Conn { stream: connect(&path).unwrap(), nonce: 0 };
        c.stream.write_all(&frame(0, &json!({ "v": 1, "client_id": "123" }))).unwrap();
        assert_eq!(read_frame(&mut c.stream).unwrap().1["evt"], "READY");
        c.set_activity(Some(&Activity { game: "Hades".into(), state: String::new(), started: 1, image: None })).unwrap();
        server.join().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
