//! Charge utile de l'installateur : une archive zip ajoutée à la fin de l'exécutable,
//! suivie d'un pied de 24 octets (`HALCYPAY`, position et taille de l'archive).
//!
//! L'archive contient `manifest.json` (version, plateforme, liste des fichiers avec taille et
//! empreinte SHA-256) puis les fichiers sous `files/`. Le même exécutable sert à fabriquer
//! un installateur (`--pack`), à installer, à réparer et à désinstaller.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

const MAGIC: &[u8; 8] = b"HALCYPAY";
const FOOTER: u64 = 24;

/// Fichier livré.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FileEntry {
    /// Chemin relatif au dossier d'installation, séparé par `/`.
    pub path: String,
    pub size: u64,
    pub sha256: String,
    #[serde(default)]
    pub executable: bool,
}

/// Description d'une version.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub product: String,
    pub version: String,
    /// `linux-x86_64` ou `windows-x86_64`.
    pub platform: String,
    /// Exécutable principal (relatif).
    pub main: String,
    /// Version minimale de la glibc (Linux).
    #[serde(default)]
    pub min_glibc: Option<String>,
    pub files: Vec<FileEntry>,
}

impl Manifest {
    pub fn total_size(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

pub fn hex(d: &[u8]) -> String {
    d.iter().map(|b| format!("{b:02x}")).collect()
}

/// Empreinte d'un fichier lu par blocs.
pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut f = File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex(&h.finalize()))
}

/// Chemin relatif sûr (pas de `..`, pas d'absolu) : protège contre une archive piégée.
pub fn safe_relative(path: &str) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for part in path.split('/') {
        if part.is_empty() || part == "." || part == ".." || part.contains('\\') || part.contains(':') {
            return None;
        }
        out.push(part);
    }
    (!out.as_os_str().is_empty()).then_some(out)
}

/// Archive ouverte depuis l'exécutable (ou un fichier seul, pour les tests).
pub struct Payload {
    file: PathBuf,
    offset: u64,
    len: u64,
    pub manifest: Manifest,
}

/// Lecture bornée d'une partie d'un fichier (l'archive à l'intérieur de l'exécutable).
struct Slice {
    f: File,
    start: u64,
    len: u64,
    pos: u64,
}

impl Read for Slice {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let left = self.len.saturating_sub(self.pos);
        if left == 0 {
            return Ok(0);
        }
        let n = buf.len().min(left as usize);
        self.f.seek(SeekFrom::Start(self.start + self.pos))?;
        let r = self.f.read(&mut buf[..n])?;
        self.pos += r as u64;
        Ok(r)
    }
}

impl Seek for Slice {
    fn seek(&mut self, to: SeekFrom) -> std::io::Result<u64> {
        let new = match to {
            SeekFrom::Start(p) => p as i64,
            SeekFrom::End(d) => self.len as i64 + d,
            SeekFrom::Current(d) => self.pos as i64 + d,
        };
        if new < 0 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "position négative"));
        }
        self.pos = new as u64;
        Ok(self.pos)
    }
}

impl Payload {
    /// Charge utile ajoutée à la fin de `exe` ; `Ok(None)` si l'exécutable n'en contient pas
    /// (programme de désinstallation copié dans le dossier d'installation).
    pub fn from_exe(exe: &Path) -> Result<Option<Payload>, String> {
        let mut f = File::open(exe).map_err(|e| e.to_string())?;
        let size = f.metadata().map_err(|e| e.to_string())?.len();
        if size < FOOTER {
            return Ok(None);
        }
        // Exécutable Windows signé : la signature suit la charge utile.
        let mut head = vec![0u8; size.min(4096) as usize];
        f.read_exact(&mut head).map_err(|e| e.to_string())?;
        let end = match pe_cert_table(&head, size as usize) {
            Some((off, len, _)) if (off + len) as u64 == size => off as u64,
            _ => size,
        };
        // La signature est alignée sur 8 octets : jusqu'à 7 octets de remplissage.
        let tail_len = end.min(FOOTER + 8);
        f.seek(SeekFrom::Start(end - tail_len)).map_err(|e| e.to_string())?;
        let mut tail = vec![0u8; tail_len as usize];
        f.read_exact(&mut tail).map_err(|e| e.to_string())?;
        let Some(footer_at) = find_footer(&tail) else { return Ok(None) };
        let foot = &tail[footer_at..footer_at + FOOTER as usize];
        let footer_pos = end - tail_len + footer_at as u64;
        let offset = u64::from_le_bytes(foot[8..16].try_into().unwrap_or_default());
        let len = u64::from_le_bytes(foot[16..24].try_into().unwrap_or_default());
        if offset.checked_add(len).is_none_or(|e| e != footer_pos) {
            return Err("charge utile incohérente : fichier d'installation endommagé".into());
        }
        Self::open(exe, offset, len).map(Some)
    }

    /// Archive zip seule (tests, outils).
    pub fn from_zip(path: &Path) -> Result<Payload, String> {
        let len = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
        Self::open(path, 0, len)
    }

    fn open(file: &Path, offset: u64, len: u64) -> Result<Payload, String> {
        let mut p = Payload { file: file.to_path_buf(), offset, len, manifest: empty_manifest() };
        let mut zip = p.archive()?;
        let mut m = zip.by_name("manifest.json").map_err(|_| "manifeste absent de la charge utile".to_string())?;
        let mut text = String::new();
        m.read_to_string(&mut text).map_err(|e| e.to_string())?;
        drop(m);
        p.manifest = serde_json::from_str(&text).map_err(|e| format!("manifeste illisible : {e}"))?;
        for f in &p.manifest.files {
            if safe_relative(&f.path).is_none() {
                return Err(format!("chemin refusé dans la charge utile : {}", f.path));
            }
        }
        Ok(p)
    }

    fn archive(&self) -> Result<zip::ZipArchive<Slice>, String> {
        let f = File::open(&self.file).map_err(|e| e.to_string())?;
        zip::ZipArchive::new(Slice { f, start: self.offset, len: self.len, pos: 0 }).map_err(|e| format!("archive illisible : {e}"))
    }

    /// Extrait un fichier vers `dest` en vérifiant son empreinte ; `progress(octets)` est
    /// appelé au fil de l'écriture réelle.
    pub fn extract(&self, entry: &FileEntry, dest: &Path, progress: &mut dyn FnMut(u64)) -> Result<(), String> {
        let mut zip = self.archive()?;
        let mut src = zip.by_name(&format!("files/{}", entry.path)).map_err(|_| format!("{} absent de la charge utile", entry.path))?;
        if let Some(dir) = dest.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{} : {e}", dir.display()))?;
        }
        let mut out = File::create(dest).map_err(|e| format!("{} : {e}", dest.display()))?;
        let mut h = Sha256::new();
        let mut buf = vec![0u8; 1 << 16];
        loop {
            let n = src.read(&mut buf).map_err(|e| format!("{} : {e}", entry.path))?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n]).map_err(|e| format!("{} : {e}", dest.display()))?;
            h.update(&buf[..n]);
            progress(n as u64);
        }
        out.sync_all().map_err(|e| format!("{} : {e}", dest.display()))?;
        if hex(&h.finalize()) != entry.sha256 {
            return Err(format!("{} : empreinte incorrecte (charge utile endommagée)", entry.path));
        }
        #[cfg(unix)]
        if entry.executable {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(dest, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

fn empty_manifest() -> Manifest {
    Manifest { product: String::new(), version: String::new(), platform: String::new(), main: String::new(), min_glibc: None, files: vec![] }
}

/// Construit la charge utile à partir d'un dossier, et l'ajoute à `stub` (l'installateur
/// sans charge utile) pour produire `out`.
pub fn pack(stub: &Path, dir: &Path, mut manifest: Manifest, out: &Path) -> Result<Manifest, String> {
    use zip::write::SimpleFileOptions;
    let mut files = Vec::new();
    collect(dir, dir, &mut files)?;
    files.sort();
    manifest.files.clear();
    let zip_path = out.with_extension("payload.tmp");
    {
        let f = File::create(&zip_path).map_err(|e| e.to_string())?;
        let mut zip = zip::ZipWriter::new(f);
        let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated).compression_level(Some(9));
        for rel in &files {
            let full = dir.join(rel);
            let bytes = std::fs::read(&full).map_err(|e| format!("{} : {e}", full.display()))?;
            #[cfg(unix)]
            let executable = {
                use std::os::unix::fs::PermissionsExt;
                std::fs::metadata(&full).map(|m| m.permissions().mode() & 0o111 != 0).unwrap_or(false)
            };
            #[cfg(not(unix))]
            let executable = rel.ends_with(".exe");
            manifest.files.push(FileEntry { path: rel.clone(), size: bytes.len() as u64, sha256: sha256_hex(&bytes), executable });
            zip.start_file(format!("files/{rel}"), opts).map_err(|e| e.to_string())?;
            zip.write_all(&bytes).map_err(|e| e.to_string())?;
        }
        if !manifest.files.iter().any(|f| f.path == manifest.main) {
            return Err(format!("exécutable principal {} absent du dossier", manifest.main));
        }
        zip.start_file("manifest.json", opts).map_err(|e| e.to_string())?;
        zip.write_all(serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?.as_bytes()).map_err(|e| e.to_string())?;
        zip.finish().map_err(|e| e.to_string())?;
    }
    let stub_bytes = stub_bytes(&std::fs::read(stub).map_err(|e| e.to_string())?);
    let stub_len = stub_bytes.len();
    let zip_bytes = std::fs::read(&zip_path).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&zip_path);
    let mut o = File::create(out).map_err(|e| e.to_string())?;
    o.write_all(&stub_bytes[..stub_len]).map_err(|e| e.to_string())?;
    o.write_all(&zip_bytes).map_err(|e| e.to_string())?;
    o.write_all(MAGIC).map_err(|e| e.to_string())?;
    o.write_all(&(stub_len as u64).to_le_bytes()).map_err(|e| e.to_string())?;
    o.write_all(&(zip_bytes.len() as u64).to_le_bytes()).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(out, std::fs::Permissions::from_mode(0o755));
    }
    Ok(manifest)
}

/// Pied de la charge utile dans les derniers octets (`tail`), remplissage éventuel compris.
fn find_footer(tail: &[u8]) -> Option<usize> {
    let f = FOOTER as usize;
    (0..=tail.len().checked_sub(f)?).rev().find(|&i| &tail[i..i + 8] == MAGIC && tail[i + f..].iter().all(|&b| b == 0))
}

/// Table des certificats (signature Authenticode) d'un exécutable PE : position, taille, et
/// position de son entrée dans le répertoire de données. `head` : début du fichier.
pub fn pe_cert_table(head: &[u8], file_len: usize) -> Option<(usize, usize, usize)> {
    let u32_at = |i: usize| head.get(i..i + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize);
    if head.get(0..2)? != b"MZ" {
        return None;
    }
    let pe = u32_at(0x3c)?;
    if head.get(pe..pe + 4)? != b"PE\0\0" {
        return None;
    }
    let opt = pe + 24;
    let (count_at, dirs) = match head.get(opt..opt + 2)? {
        [0x0b, 0x01] => (opt + 92, opt + 96),
        [0x0b, 0x02] => (opt + 108, opt + 112),
        _ => return None,
    };
    if u32_at(count_at)? < 5 {
        return None;
    }
    let entry = dirs + 4 * 8;
    let (off, len) = (u32_at(entry)?, u32_at(entry + 4)?);
    (off > 0 && len > 0 && off.checked_add(len)? <= file_len).then_some((off, len, entry))
}

/// Longueur de l'exécutable sans sa charge utile ni sa signature éventuelles.
pub fn strip_len(bytes: &[u8]) -> usize {
    let n = bytes.len();
    let end = match pe_cert_table(bytes, n) {
        Some((off, len, _)) if off + len == n => off,
        _ => n,
    };
    let tail_len = end.min(FOOTER as usize + 8);
    if let Some(i) = find_footer(&bytes[end - tail_len..end]) {
        let at = end - tail_len + i;
        let offset = u64::from_le_bytes(bytes[at + 8..at + 16].try_into().unwrap_or_default()) as usize;
        if offset <= at {
            return offset;
        }
    }
    end
}

/// Programme seul (sans charge utile ni signature) : la copie conservée pour la maintenance,
/// et la base d'un nouvel installateur. Une signature retirée est aussi retirée de l'en-tête,
/// sinon Windows considérerait l'exécutable comme endommagé.
pub fn stub_bytes(bytes: &[u8]) -> Vec<u8> {
    let n = strip_len(bytes);
    let mut out = bytes[..n].to_vec();
    if let Some((off, _, entry)) = pe_cert_table(bytes, bytes.len()) {
        if off >= n && entry + 8 <= out.len() {
            out[entry..entry + 8].fill(0);
        }
    }
    out
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), String> {
    for e in std::fs::read_dir(dir).map_err(|e| format!("{} : {e}", dir.display()))? {
        let e = e.map_err(|e| e.to_string())?;
        let p = e.path();
        if p.is_dir() {
            collect(root, &p, out)?;
        } else {
            let rel = p.strip_prefix(root).map_err(|e| e.to_string())?;
            out.push(rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect::<Vec<_>>().join("/"));
        }
    }
    Ok(())
}
