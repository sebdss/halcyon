//! Archives ZIP (sauvegardes de jeux, export de configuration) et empreintes SHA-256.
//!
//! L'extraction refuse tout chemin sortant du dossier cible (« zip slip »).

use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Component, Path, PathBuf};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

/// Empreinte SHA-256 (hexadécimale) d'un fichier.
pub fn sha256_file(path: &Path) -> io::Result<String> {
    let mut f = BufReader::new(File::open(path)?);
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex(&h.finalize()))
}

pub fn sha256_bytes(data: &[u8]) -> String {
    hex(&Sha256::digest(data))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Écrit une archive : `entries` = (nom dans l'archive, fichier sur disque).
pub fn write_zip(dest: &Path, entries: &[(String, PathBuf)], extra: &[(String, Vec<u8>)]) -> io::Result<()> {
    let tmp = dest.with_extension("zip.part");
    {
        let file = BufWriter::new(File::create(&tmp)?);
        let mut zip = ZipWriter::new(file);
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated).large_file(true);
        for (name, data) in extra {
            zip.start_file(name.as_str(), opts).map_err(io::Error::other)?;
            zip.write_all(data)?;
        }
        for (name, path) in entries {
            zip.start_file(name.as_str(), opts).map_err(io::Error::other)?;
            let mut f = File::open(path)?;
            io::copy(&mut f, &mut zip)?;
        }
        zip.finish().map_err(io::Error::other)?.flush()?;
    }
    std::fs::rename(&tmp, dest)
}

/// Lit un fichier de l'archive en mémoire (petits fichiers : manifestes).
pub fn read_entry(archive: &Path, name: &str) -> io::Result<Vec<u8>> {
    let mut zip = ZipArchive::new(BufReader::new(File::open(archive)?)).map_err(io::Error::other)?;
    let mut f = zip.by_name(name).map_err(|e| io::Error::new(io::ErrorKind::NotFound, e))?;
    let mut out = Vec::new();
    f.read_to_end(&mut out)?;
    Ok(out)
}

/// Noms des entrées de l'archive.
pub fn list_entries(archive: &Path) -> io::Result<Vec<String>> {
    let zip = ZipArchive::new(BufReader::new(File::open(archive)?)).map_err(io::Error::other)?;
    Ok(zip.file_names().filter_map(|n| n.ok()).map(|n| n.into_owned()).collect())
}

/// Chemin relatif sûr (pas de `..`, pas de racine).
pub fn safe_relative(name: &str) -> Option<PathBuf> {
    let p = Path::new(name);
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::Normal(s) => out.push(s),
            Component::CurDir => {}
            _ => return None,
        }
    }
    (!out.as_os_str().is_empty()).then_some(out)
}

/// Extrait les entrées dont le nom commence par `prefix` dans `dest` (préfixe retiré).
/// Renvoie le nombre de fichiers écrits.
pub fn extract_prefix(archive: &Path, prefix: &str, dest: &Path) -> io::Result<usize> {
    let mut zip = ZipArchive::new(BufReader::new(File::open(archive)?)).map_err(io::Error::other)?;
    let mut n = 0;
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).map_err(io::Error::other)?;
        let name = f.name().map_err(io::Error::other)?.replace('\\', "/");
        let Some(rest) = name.strip_prefix(prefix) else { continue };
        if f.is_dir() || rest.is_empty() {
            continue;
        }
        let rel = safe_relative(rest).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, tr!("chemin refusé : {name}", name = name)))?;
        let out = dest.join(rel);
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut w = BufWriter::new(File::create(&out)?);
        io::copy(&mut f, &mut w)?;
        w.flush()?;
        n += 1;
    }
    Ok(n)
}

/// Fichiers d'un dossier (récursif, liens symboliques ignorés), chemins relatifs triés.
pub fn walk_files(root: &Path, limit: usize) -> io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for e in std::fs::read_dir(&dir)? {
            let e = e?;
            let ft = e.file_type()?;
            if ft.is_symlink() {
                continue;
            }
            let p = e.path();
            if ft.is_dir() {
                stack.push(p);
            } else if ft.is_file() {
                if let Ok(rel) = p.strip_prefix(root) {
                    out.push(rel.to_path_buf());
                    if out.len() > limit {
                        return Err(io::Error::other(trn!(limit, "plus de {n} fichier dans {root}", "plus de {n} fichiers dans {root}", root = root.display())));
                    }
                }
            }
        }
    }
    out.sort();
    Ok(out)
}

/// Copie récursive d'un dossier.
pub fn copy_dir(src: &Path, dest: &Path) -> io::Result<u64> {
    let mut total = 0;
    for rel in walk_files(src, 200_000)? {
        let to = dest.join(&rel);
        if let Some(p) = to.parent() {
            std::fs::create_dir_all(p)?;
        }
        total += std::fs::copy(src.join(&rel), &to)?;
    }
    Ok(total)
}

/// Taille totale d'un dossier.
pub fn dir_size(root: &Path) -> u64 {
    walk_files(root, 1_000_000)
        .map(|files| files.iter().filter_map(|f| root.join(f).metadata().ok()).map(|m| m.len()).sum())
        .unwrap_or(0)
}

/// Chemin relatif → nom d'entrée d'archive (séparateurs `/`).
pub fn entry_name(prefix: &str, rel: &Path) -> String {
    let parts: Vec<String> = rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
    format!("{prefix}{}", parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zip_round_trip_and_zip_slip() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        std::fs::create_dir_all(src.join("a/b")).unwrap();
        std::fs::write(src.join("a/b/save.dat"), b"progression").unwrap();
        std::fs::write(src.join("top.ini"), b"[x]").unwrap();
        let files = walk_files(&src, 100).unwrap();
        assert_eq!(files.len(), 2);
        let entries: Vec<(String, PathBuf)> = files.iter().map(|f| (entry_name("0/", f), src.join(f))).collect();
        let zip = tmp.path().join("t.zip");
        write_zip(&zip, &entries, &[("meta.json".into(), b"{}".to_vec())]).unwrap();
        assert_eq!(read_entry(&zip, "meta.json").unwrap(), b"{}");
        let out = tmp.path().join("out");
        assert_eq!(extract_prefix(&zip, "0/", &out).unwrap(), 2);
        assert_eq!(std::fs::read(out.join("a/b/save.dat")).unwrap(), b"progression");
        assert_eq!(sha256_file(&out.join("top.ini")).unwrap(), sha256_bytes(b"[x]"));
        assert!(safe_relative("../evil").is_none());
        assert!(safe_relative("/etc/passwd").is_none());
        assert_eq!(safe_relative("a/./b"), Some(PathBuf::from("a/b")));
    }
}
