//! Base LaunchBox (1.0.0-alpha.7, **en ligne**, désactivée par défaut) : dos et tranches de
//! boîtiers.
//!
//! 1. `Metadata.zip` (environ 110 Mo) est téléchargé dans le cache, une fois ;
//! 2. son `Metadata.xml` (environ 500 Mo) est lu **en flux** depuis l'archive, sans être
//!    décompressé sur le disque ni chargé en mémoire : seuls les noms des plateformes utiles et
//!    le meilleur dos et la meilleure tranche de chaque jeu sont gardés, dans un petit index
//!    SQLite ([`halcyon_storage::lbindex`]) ; l'archive est ensuite supprimée ;
//! 3. une image n'est téléchargée que pour un jeu de la bibliothèque, depuis
//!    `images.launchbox-app.com` seulement, avec un nom de fichier vérifié, une taille bornée et
//!    un contenu contrôlé (PNG ou JPEG).
//!
//! Lecture du XML : un découpage simple par blocs (`<Game>…</Game>`, `<GameImage>…`), suffisant
//! pour ce fichier plat et sans dépendance supplémentaire.

use halcyon_core::launchbox::{image_rank, looks_like_image, normalize_title, valid_image_file, IMAGE_HOST, METADATA_URL, PLATFORMS};
use halcyon_storage::lbindex::LbWriter;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Taille maximale acceptée pour l'archive (la vraie fait environ 110 Mo).
const MAX_ZIP: u64 = 400 * 1024 * 1024;
/// Taille maximale d'une image (un scan fait rarement plus de 3 Mo).
const MAX_IMAGE: u64 = 15 * 1024 * 1024;

fn agent(timeout: Duration) -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .user_agent(concat!("Halcyon/", env!("CARGO_PKG_VERSION")))
        .build();
    ureq::Agent::new_with_config(config)
}

fn cancelled() -> String {
    tr!("opération annulée")
}

/// Télécharge `Metadata.zip` vers `dest` (fichier `.part` renommé à la fin). `progress` reçoit
/// les octets reçus et la taille annoncée.
pub fn download_metadata(dest: &Path, progress: &mut dyn FnMut(u64, Option<u64>), cancel: &AtomicBool) -> Result<(), String> {
    let mut resp = agent(Duration::from_secs(60 * 60)).get(METADATA_URL).call().map_err(|e| tr!("réseau : {e}", e = e))?;
    let total = resp.headers().get("content-length").and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<u64>().ok());
    if total.is_some_and(|t| t > MAX_ZIP) {
        return Err(tr!("archive trop grande"));
    }
    if let Some(d) = dest.parent() {
        std::fs::create_dir_all(d).map_err(|e| tr!("écriture : {e}", e = e))?;
    }
    let tmp = dest.with_extension("part");
    let mut out = std::fs::File::create(&tmp).map_err(|e| tr!("écriture : {e}", e = e))?;
    let mut reader = resp.body_mut().with_config().limit(MAX_ZIP).reader();
    let mut buf = vec![0u8; 1 << 16];
    let mut done = 0u64;
    loop {
        if cancel.load(Ordering::Relaxed) {
            drop(out);
            let _ = std::fs::remove_file(&tmp);
            return Err(cancelled());
        }
        let n = reader.read(&mut buf).map_err(|e| tr!("réseau : {e}", e = e))?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n]).map_err(|e| tr!("écriture : {e}", e = e))?;
        done += n as u64;
        progress(done, total);
    }
    out.sync_all().map_err(|e| tr!("écriture : {e}", e = e))?;
    drop(out);
    std::fs::rename(&tmp, dest).map_err(|e| tr!("écriture : {e}", e = e))
}

/// Valeur du premier `<tag>…</tag>` d'un bloc, entités XML de base décodées.
fn field(block: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let start = block.find(&open)? + open.len();
    let end = block[start..].find(&format!("</{tag}>"))? + start;
    let v = block[start..end].trim();
    Some(v.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&"))
}

const BLOCKS: [(&str, &str); 3] = [("<Game>", "</Game>"), ("<GameAlternateName>", "</GameAlternateName>"), ("<GameImage>", "</GameImage>")];

/// Lit le XML en flux et remplit l'index. Renvoie le nombre de jeux qui ont un dos ou une
/// tranche. `progress` reçoit les octets lus du XML et sa taille.
pub fn index_xml(xml: impl Read, total: u64, w: &LbWriter, progress: &mut dyn FnMut(u64, u64), cancel: &AtomicBool) -> Result<(), String> {
    let mut reader = std::io::BufReader::with_capacity(1 << 20, xml);
    let mut buf: Vec<u8> = Vec::with_capacity(4 << 20);
    let mut chunk = vec![0u8; 1 << 20];
    let mut read = 0u64;
    // Plateforme de chaque jeu retenu ; noms alternatifs vus avant leur jeu.
    let mut games: HashMap<i64, &'static str> = HashMap::new();
    let mut pending_alt: Vec<(i64, String)> = Vec::new();
    let mut eof = false;
    while !eof {
        if cancel.load(Ordering::Relaxed) {
            return Err(cancelled());
        }
        let n = reader.read(&mut chunk).map_err(|e| tr!("lecture : {e}", e = e))?;
        if n == 0 {
            eof = true;
        } else {
            buf.extend_from_slice(&chunk[..n]);
            read += n as u64;
            progress(read, total);
        }
        // Seulement le préfixe UTF-8 valide (un caractère peut être coupé en fin de morceau) ;
        // un octet réellement invalide est remplacé par « ? ».
        let valid = loop {
            match std::str::from_utf8(&buf) {
                Ok(_) => break buf.len(),
                Err(e) => match e.error_len() {
                    Some(_) => buf[e.valid_up_to()] = b'?',
                    None => break e.valid_up_to(),
                },
            }
        };
        let text = std::str::from_utf8(&buf[..valid]).unwrap_or_default();
        let mut pos = 0usize;
        loop {
            // Prochain bloc utile (le plus proche des trois).
            let next = BLOCKS
                .iter()
                .enumerate()
                .filter_map(|(i, (open, _))| text[pos..].find(open).map(|p| (pos + p, i)))
                .min_by_key(|(p, _)| *p);
            let Some((start, kind)) = next else {
                // Rien d'autre dans ce morceau : garde la fin (un début de balise coupé).
                let mut keep = text.len().saturating_sub(32).max(pos);
                while !text.is_char_boundary(keep) {
                    keep -= 1;
                }
                pos = keep;
                break;
            };
            let close = BLOCKS[kind].1;
            let Some(end) = text[start..].find(close).map(|e| start + e + close.len()) else {
                pos = start;
                break;
            };
            let block = &text[start..end];
            let id = field(block, "DatabaseID").and_then(|v| v.parse::<i64>().ok());
            if let Some(id) = id {
                match kind {
                    0 => {
                        let platform = field(block, "Platform").unwrap_or_default();
                        if let Some(p) = PLATFORMS.iter().find(|p| **p == platform) {
                            games.insert(id, *p);
                            if let Some(name) = field(block, "Name") {
                                w.add_name(id, &normalize_title(&name), p).map_err(|e| e.to_string())?;
                            }
                        }
                    }
                    1 => {
                        if let Some(alt) = field(block, "AlternateName") {
                            match games.get(&id) {
                                Some(p) => w.add_name(id, &normalize_title(&alt), p).map_err(|e| e.to_string())?,
                                None => pending_alt.push((id, alt)),
                            }
                        }
                    }
                    _ => {
                        let file = field(block, "FileName").unwrap_or_default();
                        let rank = image_rank(&field(block, "Type").unwrap_or_default(), &field(block, "Region").unwrap_or_default());
                        if let Some((part, rank)) = rank.filter(|_| valid_image_file(&file)) {
                            w.add_image(id, part.id(), i64::from(rank), &file).map_err(|e| e.to_string())?;
                        }
                    }
                }
            }
            pos = end;
        }
        // Retire ce qui a été lu (le texte est un préfixe de `buf`, mêmes octets).
        buf.drain(..pos);
    }
    for (id, alt) in pending_alt {
        if let Some(p) = games.get(&id) {
            w.add_name(id, &normalize_title(&alt), p).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Construit l'index depuis l'archive téléchargée (lecture en flux de `Metadata.xml`).
pub fn build_index(zip: &Path, dest: &Path, progress: &mut dyn FnMut(u64, u64), cancel: &AtomicBool, now: i64) -> Result<(), String> {
    let file = std::fs::File::open(zip).map_err(|e| tr!("lecture : {e}", e = e))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| tr!("archive illisible : {e}", e = e))?;
    let entry = archive.by_name("Metadata.xml").map_err(|_| tr!("archive incomplète (Metadata.xml absent)"))?;
    let total = entry.size();
    let tmp = dest.with_extension("building");
    let w = LbWriter::create(&tmp).map_err(|e| e.to_string())?;
    let r = index_xml(entry, total, &w, progress, cancel);
    if let Err(e) = r {
        drop(w);
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    w.finish(&tmp, dest, now).map_err(|e| e.to_string())
}

/// Télécharge une image de la base (dos ou tranche) vers `dest`.
pub fn fetch_part(file: &str, dest: &Path) -> Result<(), String> {
    if !valid_image_file(file) {
        return Err(tr!("nom de fichier refusé"));
    }
    let url = format!("{IMAGE_HOST}{}", file.replace(' ', "%20"));
    let mut resp = agent(Duration::from_secs(30)).get(&url).call().map_err(|e| tr!("réseau : {e}", e = e))?;
    let bytes = resp.body_mut().with_config().limit(MAX_IMAGE).read_to_vec().map_err(|e| tr!("réseau : {e}", e = e))?;
    if !looks_like_image(&bytes) {
        return Err(tr!("contenu inattendu (ni PNG ni JPEG)"));
    }
    if let Some(d) = dest.parent() {
        std::fs::create_dir_all(d).map_err(|e| tr!("écriture : {e}", e = e))?;
    }
    let tmp = dest.with_extension("part");
    std::fs::write(&tmp, &bytes).map_err(|e| tr!("écriture : {e}", e = e))?;
    std::fs::rename(&tmp, dest).map_err(|e| tr!("écriture : {e}", e = e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use halcyon_storage::lbindex::{LbArt, LbIndex};

    const XML: &str = "<?xml version=\"1.0\"?>\n<LaunchBox>\n  <Game>\n    <Name>Half-Life 2</Name>\n    <DatabaseID>10</DatabaseID>\n    <Platform>Windows</Platform>\n  </Game>\n  <Game>\n    <Name>Super Mario World</Name>\n    <DatabaseID>11</DatabaseID>\n    <Platform>Super Nintendo Entertainment System</Platform>\n  </Game>\n  <Game>\n    <Name>Ignored</Name>\n    <DatabaseID>12</DatabaseID>\n    <Platform>Commodore 64</Platform>\n  </Game>\n  <GameAlternateName>\n    <AlternateName>Super Mario Bros. 4</AlternateName>\n    <DatabaseID>11</DatabaseID>\n  </GameAlternateName>\n  <GameImage>\n    <DatabaseID>10</DatabaseID>\n    <FileName>aa-back.jpg</FileName>\n    <Type>Box - Back</Type>\n    <Region>Japan</Region>\n  </GameImage>\n  <GameImage>\n    <DatabaseID>10</DatabaseID>\n    <FileName>bb-back.jpg</FileName>\n    <Type>Box - Back</Type>\n    <Region>North America</Region>\n  </GameImage>\n  <GameImage>\n    <DatabaseID>10</DatabaseID>\n    <FileName>../evil.png</FileName>\n    <Type>Box - Spine</Type>\n  </GameImage>\n  <GameImage>\n    <DatabaseID>11</DatabaseID>\n    <FileName>cc-spine.png</FileName>\n    <Type>Box - Spine</Type>\n    <Region>Europe</Region>\n  </GameImage>\n  <GameImage>\n    <DatabaseID>12</DatabaseID>\n    <FileName>dd.png</FileName>\n    <Type>Box - Back</Type>\n  </GameImage>\n</LaunchBox>\n";

    /// Lecteur qui rend le texte par petits morceaux (blocs coupés en plein milieu).
    struct Trickle<'a>(&'a [u8]);
    impl Read for Trickle<'_> {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            let n = self.0.len().min(out.len()).min(7);
            out[..n].copy_from_slice(&self.0[..n]);
            self.0 = &self.0[n..];
            Ok(n)
        }
    }

    #[test]
    fn streams_xml_into_index() {
        let dir = tempfile::tempdir().unwrap();
        let (tmp, dest) = (dir.path().join("i.tmp"), dir.path().join("i.sqlite"));
        let w = LbWriter::create(&tmp).unwrap();
        let mut last = 0;
        index_xml(Trickle(XML.as_bytes()), XML.len() as u64, &w, &mut |r, _| last = r, &AtomicBool::new(false)).unwrap();
        assert_eq!(last, XML.len() as u64);
        w.finish(&tmp, &dest, 1).unwrap();
        let idx = LbIndex::open(&dest).unwrap();
        assert_eq!(idx.lookup("half life 2", "Windows").unwrap(), LbArt { back: Some("bb-back.jpg".into()), spine: None });
        let mario = LbArt { back: None, spine: Some("cc-spine.png".into()) };
        assert_eq!(idx.lookup("super mario world", "Super Nintendo Entertainment System").unwrap(), mario);
        assert_eq!(idx.lookup("super mario bros 4", "Super Nintendo Entertainment System").unwrap(), mario);
        // Plateforme non retenue, nom de fichier dangereux : absents.
        assert_eq!(idx.games_with_art(), 2);
    }

    #[test]
    fn cancel_stops_reading() {
        let dir = tempfile::tempdir().unwrap();
        let w = LbWriter::create(&dir.path().join("i.tmp")).unwrap();
        assert!(index_xml(XML.as_bytes(), 1, &w, &mut |_, _| {}, &AtomicBool::new(true)).is_err());
    }

    /// Mesure sur une vraie archive : `HALCYON_LB_BENCH=<Metadata.zip> cargo test --release
    /// -p halcyon-integrations -- --ignored lb_bench --nocapture`.
    #[test]
    #[ignore]
    fn lb_bench() {
        let Ok(zip) = std::env::var("HALCYON_LB_BENCH") else { return };
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("lb.sqlite");
        let t = std::time::Instant::now();
        build_index(Path::new(&zip), &dest, &mut |_, _| {}, &AtomicBool::new(false), 1).unwrap();
        let idx = LbIndex::open(&dest).unwrap();
        let size = std::fs::metadata(&dest).unwrap().len();
        println!("index : {:?}, {} jeux avec images, {} Kio", t.elapsed(), idx.games_with_art(), size / 1024);
        let t = std::time::Instant::now();
        for i in 0..1000 {
            let _ = idx.lookup(&normalize_title(&format!("Game Title Number {i} & Friends")), "Windows").unwrap();
        }
        println!("1000 recherches : {:?}", t.elapsed());
    }

    #[test]
    fn fields_are_unescaped() {
        assert_eq!(field("<Game><Name> Tom &amp; Jerry </Name></Game>", "Name").as_deref(), Some("Tom & Jerry"));
        assert_eq!(field("<Game></Game>", "Name"), None);
    }
}
