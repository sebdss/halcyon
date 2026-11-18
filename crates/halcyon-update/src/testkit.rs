//! Outils de test (compilés uniquement avec la fonctionnalité `fault-injection`) : serveur
//! HTTP local (ETag, Range, coupures, erreurs d'API), publications signées avec une clé de
//! test déterministe. Jamais présents dans les binaires livrés.

use crate::core::{Channel, PackageFormat, Platform};
use crate::manifest::{Manifest, Package, MANIFEST_NAME, SCHEMA, SIGNATURE_NAME};
use crate::security::{self, PublicKey};
use crate::version::Version;
use base64::Engine;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Default)]
pub struct Route {
    pub status: u16,
    pub body: Vec<u8>,
    pub etag: Option<String>,
    /// Prise en charge de `Range: bytes=N-`.
    pub range: bool,
    /// Coupe la connexion après ce nombre d'octets du corps…
    pub cut_after: Option<usize>,
    /// … pour ce nombre de requêtes (ensuite, réponse complète).
    pub cuts: usize,
    pub headers: Vec<(String, String)>,
}

impl Route {
    pub fn ok(body: impl Into<Vec<u8>>) -> Route {
        Route { status: 200, body: body.into(), range: true, ..Default::default() }
    }
}

#[derive(Clone, Debug)]
pub struct Request {
    pub path: String,
    pub headers: HashMap<String, String>,
}

pub struct Server {
    pub base: String,
    routes: Arc<Mutex<HashMap<String, Route>>>,
    log: Arc<Mutex<Vec<Request>>>,
}

impl Server {
    pub fn start() -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").expect("port local");
        let base = format!("http://{}", listener.local_addr().unwrap());
        let routes: Arc<Mutex<HashMap<String, Route>>> = Arc::default();
        let log: Arc<Mutex<Vec<Request>>> = Arc::default();
        let (r, l) = (routes.clone(), log.clone());
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let (r, l) = (r.clone(), l.clone());
                std::thread::spawn(move || {
                    let _ = serve(stream, &r, &l);
                });
            }
        });
        Server { base, routes, log }
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    pub fn set(&self, path: &str, route: Route) {
        self.routes.lock().unwrap().insert(path.to_string(), route);
    }

    pub fn requests(&self, path: &str) -> Vec<Request> {
        self.log.lock().unwrap().iter().filter(|r| r.path == path).cloned().collect()
    }
}

fn serve(mut stream: TcpStream, routes: &Mutex<HashMap<String, Route>>, log: &Mutex<Vec<Request>>) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
    let mut headers = HashMap::new();
    loop {
        let mut h = String::new();
        if reader.read_line(&mut h)? == 0 || h == "\r\n" {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
        }
    }
    log.lock().unwrap().push(Request { path: path.clone(), headers: headers.clone() });
    let route = {
        let mut map = routes.lock().unwrap();
        match map.get_mut(&path) {
            Some(r) => {
                let snapshot = r.clone();
                if r.cuts > 0 {
                    r.cuts -= 1;
                }
                Some(snapshot)
            }
            None => None,
        }
    };
    let Some(route) = route else {
        return write!(stream, "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    };
    let mut extra = String::new();
    for (k, v) in &route.headers {
        extra.push_str(&format!("{k}: {v}\r\n"));
    }
    if let Some(tag) = &route.etag {
        if headers.get("if-none-match") == Some(tag) {
            return write!(stream, "HTTP/1.1 304 Not Modified\r\nETag: {tag}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        }
        extra.push_str(&format!("ETag: {tag}\r\n"));
    }
    let len = route.body.len();
    let start = headers
        .get("range")
        .filter(|_| route.range)
        .and_then(|r| r.strip_prefix("bytes="))
        .and_then(|r| r.strip_suffix('-'))
        .and_then(|n| n.parse::<usize>().ok());
    let (status, body) = match start {
        Some(s) if s >= len => {
            return write!(stream, "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */{len}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        }
        Some(s) => {
            extra.push_str(&format!("Content-Range: bytes {s}-{}/{len}\r\n", len - 1));
            ("206 Partial Content".to_string(), &route.body[s..])
        }
        None => (format!("{} X", route.status), &route.body[..]),
    };
    write!(stream, "HTTP/1.1 {status}\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\n{extra}Connection: close\r\n\r\n", body.len())?;
    match route.cut_after.filter(|_| route.cuts > 0) {
        Some(n) => {
            stream.write_all(&body[..n.min(body.len())])?;
            stream.flush()?;
            let _ = stream.shutdown(std::net::Shutdown::Both);
        }
        None => stream.write_all(body)?,
    }
    let mut sink = [0u8; 1];
    let _ = stream.read(&mut sink);
    Ok(())
}

/// Clé de test déterministe (graine fixe).
pub fn test_key(seed: u8) -> ed25519_compact::KeyPair {
    ed25519_compact::KeyPair::from_seed(ed25519_compact::Seed::new([seed; 32]))
}

pub fn pubkey_b64(kp: &ed25519_compact::KeyPair) -> String {
    base64::engine::general_purpose::STANDARD.encode(kp.pk.as_ref())
}

pub fn public(kp: &ed25519_compact::KeyPair) -> PublicKey {
    kp.pk
}

pub fn sign(kp: &ed25519_compact::KeyPair, bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(kp.sk.sign(bytes, None).as_ref())
}

/// Manifeste pour `version`, avec le paquet `payload` de la plateforme donnée.
pub fn manifest_for(version: &str, platform: Platform, payload: &[u8]) -> Manifest {
    let v: Version = version.parse().unwrap();
    Manifest {
        schema: SCHEMA,
        product: "Halcyon".into(),
        channel: Channel::of(&v, false),
        version: v.to_string(),
        published: "2026-11-16T14:00:00Z".into(),
        min_updater: "1.0.0-alpha.1".into(),
        security_floor: None,
        packages: vec![Package {
            platform,
            format: if platform == Platform::WindowsX64 { PackageFormat::Zip } else { PackageFormat::Appimage },
            name: platform.package_name(&v),
            size: payload.len() as u64,
            sha256: security::sha256_bytes(payload),
        }],
        notes: tr!("Notes de la version {v}", v = v),
    }
}

/// Publie une version sur le serveur de test (manifeste signé par `kp`, paquet) et renvoie
/// l'entrée JSON de la liste des publications.
pub fn publish(server: &Server, kp: &ed25519_compact::KeyPair, manifest: &Manifest, payload: &[u8]) -> serde_json::Value {
    let v = &manifest.version;
    let bytes = serde_json::to_vec_pretty(manifest).unwrap();
    let pkg = &manifest.packages[0];
    server.set(&format!("/dl/{v}/{MANIFEST_NAME}"), Route::ok(bytes.clone()));
    server.set(&format!("/dl/{v}/{SIGNATURE_NAME}"), Route::ok(sign(kp, &bytes)));
    server.set(&format!("/dl/{v}/{}", pkg.name), Route::ok(payload.to_vec()));
    let asset = |name: &str, size: usize| serde_json::json!({"name": name, "browser_download_url": server.url(&format!("/dl/{v}/{name}")), "size": size});
    serde_json::json!({
        "tag_name": format!("v{v}"),
        "prerelease": manifest.channel == Channel::Beta,
        "draft": false,
        "html_url": server.url(&format!("/releases/{v}")),
        "assets": [asset(MANIFEST_NAME, bytes.len()), asset(SIGNATURE_NAME, 88), asset(&pkg.name, payload.len())]
    })
}

/// Expose la liste des publications pour `repo`.
pub fn set_feed(server: &Server, repo: &str, releases: &[serde_json::Value], etag: &str) {
    server.set(
        &format!("/repos/{repo}/releases?per_page=30"),
        Route { status: 200, body: serde_json::to_vec(releases).unwrap(), etag: Some(etag.into()), ..Default::default() },
    );
}

/// Exécutable ELF réel transformé en « AppImage » de test : signature `AI\x02` dans les
/// octets de bourrage de l'en-tête (ignorés par le chargeur Linux, comme pour une vraie
/// AppImage) et image SquashFS factice à la fin de l'exécutable.
pub fn appimage_from_elf(elf: &[u8]) -> Vec<u8> {
    let mut b = elf.to_vec();
    assert_eq!(&b[0..4], b"\x7fELF", "exécutable ELF attendu");
    b[8..11].copy_from_slice(b"AI\x02");
    let rd = |at: usize, n: usize| -> u64 {
        let mut a = [0u8; 8];
        a[..n].copy_from_slice(&b[at..at + n]);
        u64::from_le_bytes(a)
    };
    let end = (rd(0x28, 8) + rd(0x3a, 2) * rd(0x3c, 2)) as usize;
    assert!(end <= b.len());
    b.truncate(end);
    b.extend_from_slice(b"hsqs");
    b.extend_from_slice(&[0u8; 92]);
    b
}
