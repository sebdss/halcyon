//! Outil de publication HUS (CI et auteur). Ne contient aucune clé.
//!
//! ```text
//! hus-release zip <dossier> <sortie.zip>
//! hus-release manifest --version <V> --notes <fichier> [--min-updater <V> (défaut : runner::PROTOCOL_MIN)] [--security-floor <V>] --out <manifeste> <paquets…>
//! hus-release sign <manifeste>                       # clé privée : variable HALCYON_SIGNING_KEY
//! hus-release verify <dossier> [--pubkey <base64>]   # manifeste, signature, paquets du dossier
//! hus-release verify-remote --repo <propriétaire/nom> --tag <vX.Y.Z> [--pubkey <base64>]
//! ```
//!
//! `verify` et `verify-remote` utilisent les clés intégrées (`keys/release.pub`,
//! `HALCYON_UPDATE_PUBKEY`) si `--pubkey` n'est pas donné : exactement ce que vérifiera Halcyon.

use base64::Engine;
use halcyon_update::core::{Channel, Platform};
use halcyon_update::download::{self, Retry};
use halcyon_update::manifest::{Manifest, Package, MANIFEST_NAME, SCHEMA, SIGNATURE_NAME};
use halcyon_update::network::{self, Net, Query};
use halcyon_update::security::{self, PublicKey};
use halcyon_update::Version;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn value(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

fn keys(args: &[String]) -> Result<Vec<PublicKey>, String> {
    let k = match value(args, "--pubkey") {
        Some(b64) => security::parse_keys(&b64),
        None => security::trusted_keys(),
    };
    if k.is_empty() {
        return Err("aucune clé publique : passez --pubkey ou renseignez keys/release.pub".into());
    }
    Ok(k)
}

/// Archive ZIP reproductible : fichiers triés, date fixe, exécutables en 0755.
fn make_zip(dir: &Path, out: &Path) -> Result<(), String> {
    fn walk(root: &Path, d: &Path, acc: &mut Vec<PathBuf>) -> std::io::Result<()> {
        for e in std::fs::read_dir(d)? {
            let p = e?.path();
            if p.is_dir() {
                walk(root, &p, acc)?;
            } else {
                acc.push(p.strip_prefix(root).unwrap().to_path_buf());
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    walk(dir, dir, &mut files).map_err(|e| e.to_string())?;
    files.sort();
    let f = std::fs::File::create(out).map_err(|e| format!("{} : {e}", out.display()))?;
    let mut w = zip::ZipWriter::new(f);
    let date = zip::DateTime::from_date_and_time(2026, 1, 1, 0, 0, 0).map_err(|e| e.to_string())?;
    for rel in files {
        let name = rel.to_string_lossy().replace('\\', "/");
        let exec = name.ends_with(".exe");
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated).last_modified_time(date).unix_permissions(if exec { 0o755 } else { 0o644 });
        w.start_file(name.as_str(), opts).map_err(|e| e.to_string())?;
        w.write_all(&std::fs::read(dir.join(&rel)).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    }
    w.finish().map_err(|e| e.to_string())?;
    security::check_zip(out).map(|_| ())
}

fn platform_of(name: &str, v: &Version) -> Option<Platform> {
    [Platform::WindowsX64, Platform::LinuxX64].into_iter().find(|p| p.package_name(v) == name)
}

fn make_manifest(args: &[String]) -> Result<(), String> {
    let version: Version = value(args, "--version").ok_or("--version manquant")?.parse()?;
    let notes = std::fs::read_to_string(value(args, "--notes").ok_or("--notes manquant")?).map_err(|e| format!("notes : {e}"))?;
    let out = PathBuf::from(value(args, "--out").ok_or("--out manquant")?);
    let min_updater = value(args, "--min-updater").unwrap_or_else(|| halcyon_update::runner::PROTOCOL_MIN.to_string());
    let security_floor = value(args, "--security-floor");
    let flags = ["--version", "--notes", "--out", "--min-updater", "--security-floor"];
    let mut packages = Vec::new();
    let mut skip = false;
    for a in &args[1..] {
        if skip {
            skip = false;
            continue;
        }
        if flags.contains(&a.as_str()) {
            skip = true;
            continue;
        }
        let path = PathBuf::from(a);
        let name = path.file_name().and_then(|n| n.to_str()).ok_or("nom de paquet invalide")?.to_string();
        let platform = platform_of(&name, &version).ok_or_else(|| format!("{name} : nom de paquet non officiel"))?;
        security::check_package(&path, platform.format())?;
        let size = std::fs::metadata(&path).map_err(|e| e.to_string())?.len();
        packages.push(Package { platform, format: platform.format(), name, size, sha256: security::sha256_file(&path).map_err(|e| e.to_string())? });
    }
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let (y, m, d) = civil(now.div_euclid(86_400));
    let s = now.rem_euclid(86_400);
    let m = Manifest {
        schema: SCHEMA,
        product: "Halcyon".into(),
        channel: Channel::of(&version, false),
        version: version.to_string(),
        published: format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", s / 3600, (s % 3600) / 60, s % 60),
        min_updater,
        security_floor,
        packages,
        notes: notes.trim().to_string(),
    };
    let bytes = serde_json::to_vec_pretty(&m).map_err(|e| e.to_string())?;
    // Le manifeste produit doit passer les contrôles du client.
    Manifest::parse(&bytes)?.check(Some(&version), Some(version.is_prerelease()))?;
    std::fs::write(&out, &bytes).map_err(|e| e.to_string())?;
    println!("{} écrit : {} paquet(s), canal {}", out.display(), m.packages.len(), m.channel.as_str());
    Ok(())
}

fn civil(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

fn sign(path: &str) -> Result<(), String> {
    let b64 = base64::engine::general_purpose::STANDARD;
    let secret = std::env::var("HALCYON_SIGNING_KEY").map_err(|_| "HALCYON_SIGNING_KEY absente".to_string())?;
    let sk = ed25519_compact::SecretKey::from_slice(&b64.decode(secret.trim()).map_err(|_| "clé privée illisible")?).map_err(|_| "clé privée invalide")?;
    let msg = std::fs::read(path).map_err(|e| format!("{path} : {e}"))?;
    let sig = b64.encode(sk.sign(&msg, None).as_ref());
    security::verify_signature(&[sk.public_key()], &msg, &sig)?;
    std::fs::write(format!("{path}.sig"), format!("{sig}\n")).map_err(|e| e.to_string())?;
    println!("{path}.sig écrit (clé {})", security::fingerprint(&sk.public_key()));
    Ok(())
}

fn verify_dir(dir: &Path, keys: &[PublicKey]) -> Result<(), String> {
    let bytes = std::fs::read(dir.join(MANIFEST_NAME)).map_err(|e| format!("{MANIFEST_NAME} : {e}"))?;
    let sig = std::fs::read_to_string(dir.join(SIGNATURE_NAME)).map_err(|e| format!("{SIGNATURE_NAME} : {e}"))?;
    security::verify_signature(keys, &bytes, &sig)?;
    let c = Manifest::parse(&bytes)?.check(None, None)?;
    for p in &c.manifest.packages {
        let path = dir.join(&p.name);
        let len = std::fs::metadata(&path).map_err(|e| format!("{} : {e}", p.name))?.len();
        if len != p.size || security::sha256_file(&path).map_err(|e| e.to_string())? != p.sha256 {
            return Err(format!("{} : taille ou empreinte différente du manifeste", p.name));
        }
        security::check_package(&path, p.format)?;
        println!("{} : signature, SHA-256 et structure ({:?}) vérifiées", p.name, p.format);
    }
    println!("{} {} : manifeste valide", c.manifest.product, c.version);
    Ok(())
}

fn verify_remote(args: &[String]) -> Result<(), String> {
    let repo = value(args, "--repo").ok_or("--repo manquant")?;
    let tag = value(args, "--tag").ok_or("--tag manquant")?;
    let version: Version = tag.parse()?;
    let keys = keys(args)?;
    let tmp = std::env::temp_dir().join(format!("hus-verify-{}", std::process::id()));
    let net = Net::new();
    let releases = network::fetch_releases(&net, &repo, &tmp.join("releases.json"))?;
    let release = releases.iter().find(|r| r.version == version).ok_or_else(|| format!("publication {tag} introuvable sur {repo}"))?;
    for platform in [Platform::WindowsX64, Platform::LinuxX64] {
        let older: Version = "0.0.0".parse().unwrap();
        let q = Query { repo: &repo, channel: Channel::Beta, current: &older, platform, keys: &keys, floor: None, cache: tmp.join("releases.json") };
        let found = network::inspect(&net, release, &q)?.ok_or("manifeste ou signature absent de la publication")?;
        let never = std::sync::atomic::AtomicBool::new(false);
        let file = download::download(&net, &found.package_url, &found.package, &tmp, &never, Retry::default(), &mut |_| {}).map_err(|e| e.to_string())?;
        security::check_package(&file, found.package.format)?;
        println!("{} publié : signature, SHA-256 et structure vérifiées", found.package.name);
    }
    let _ = std::fs::remove_dir_all(&tmp);
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let r = match args.first().map(String::as_str) {
        Some("zip") if args.len() == 3 => make_zip(Path::new(&args[1]), Path::new(&args[2])),
        Some("manifest") => make_manifest(&args),
        Some("sign") if args.len() == 2 => sign(&args[1]),
        Some("verify") if args.len() >= 2 => keys(&args).and_then(|k| verify_dir(Path::new(&args[1]), &k)),
        Some("verify-remote") => verify_remote(&args),
        _ => Err("usage : voir l'en-tête de hus-release.rs (zip, manifest, sign, verify, verify-remote)".into()),
    };
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("hus-release : {e}");
            ExitCode::FAILURE
        }
    }
}
