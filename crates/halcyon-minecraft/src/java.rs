//! Java : détection des installations du système, et runtimes officiels de Mojang (ceux du
//! launcher officiel), téléchargés à la demande selon la version de Minecraft.

use crate::http::{Error, Http, Result};
use crate::install::{fetch_all, Event, Job, Layout, Sink};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

pub const RUNTIMES: &str = "https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json";

#[derive(Clone, Debug, PartialEq)]
pub struct JavaInstall {
    /// Exécutable (`javaw.exe` sous Windows, sans console ; `java` sous Linux).
    pub path: PathBuf,
    pub version: String,
    pub major: u32,
    /// `Mojang`, `JAVA_HOME`, `PATH`, `système`.
    pub source: String,
}

/// « 1.8.0_382 » → 8 ; « 17.0.8 » → 17 ; « 21 » → 21.
pub fn major_of(version: &str) -> Option<u32> {
    let mut it = version.split(['.', '_', '-', '+']);
    let first: u32 = it.next()?.parse().ok()?;
    if first == 1 {
        it.next()?.parse().ok()
    } else {
        Some(first)
    }
}

/// Version lue dans la sortie de `java -version` (`openjdk version "17.0.8" 2023-07-18`).
pub fn parse_version_output(out: &str) -> Option<String> {
    let line = out.lines().find(|l| l.contains("version \""))?;
    let start = line.find('"')? + 1;
    let end = line[start..].find('"')? + start;
    Some(line[start..end].to_string())
}

fn exe_names() -> &'static [&'static str] {
    if cfg!(windows) {
        &["javaw.exe", "java.exe"]
    } else {
        &["java"]
    }
}

/// Version d'un exécutable Java (lancé avec `-version`, sans fenêtre).
pub fn probe(exe: &Path) -> Option<(String, u32)> {
    // `javaw` n'écrit rien : on interroge `java` du même dossier.
    let cli = if exe.file_name().is_some_and(|n| n.eq_ignore_ascii_case("javaw.exe")) { exe.with_file_name("java.exe") } else { exe.to_path_buf() };
    let mut cmd = std::process::Command::new(&cli);
    cmd.arg("-version").stdin(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let out = cmd.output().ok()?;
    let text = format!("{}{}", String::from_utf8_lossy(&out.stderr), String::from_utf8_lossy(&out.stdout));
    let v = parse_version_output(&text)?;
    let m = major_of(&v)?;
    Some((v, m))
}

fn exe_in(home: &Path) -> Option<PathBuf> {
    exe_names().iter().map(|n| home.join("bin").join(n)).find(|p| p.is_file())
}

/// Installations de Java trouvées sur la machine (dédoublonnées), runtimes Mojang compris.
pub fn detect(layout: &Layout) -> Vec<JavaInstall> {
    let mut candidates: Vec<(PathBuf, String)> = Vec::new();
    // Runtimes officiels déjà téléchargés par Halcyon.
    if let Ok(rd) = std::fs::read_dir(layout.runtime()) {
        for e in rd.flatten() {
            if let Some(exe) = exe_in(&e.path()) {
                candidates.push((exe, "Mojang".into()));
            }
        }
    }
    if let Some(home) = std::env::var_os("JAVA_HOME") {
        if let Some(exe) = exe_in(Path::new(&home)) {
            candidates.push((exe, "JAVA_HOME".into()));
        }
    }
    if let Some(path) = std::env::var_os("PATH") {
        for d in std::env::split_paths(&path) {
            for n in exe_names() {
                let p = d.join(n);
                if p.is_file() {
                    candidates.push((p, "PATH".into()));
                    break;
                }
            }
        }
    }
    let mut roots: Vec<PathBuf> = Vec::new();
    if cfg!(windows) {
        for var in ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"] {
            if let Some(pf) = std::env::var_os(var) {
                for vendor in ["Java", "Eclipse Adoptium", "Eclipse Foundation", "Microsoft", "Zulu", "BellSoft", "Amazon Corretto", "Semeru"] {
                    roots.push(PathBuf::from(&pf).join(vendor));
                }
            }
        }
        // Runtimes du launcher officiel (lecture seule, jamais modifiés).
        if let Some(local) = dirs::data_local_dir() {
            roots.push(local.join("Packages").join("Microsoft.4297127D64EC6_8wekyb3d8bbwe").join("LocalCache").join("Local").join("runtime"));
        }
    } else {
        roots.push(PathBuf::from("/usr/lib/jvm"));
        roots.push(PathBuf::from("/usr/lib64/jvm"));
        roots.push(PathBuf::from("/opt"));
        if let Some(h) = dirs::home_dir() {
            roots.push(h.join(".sdkman/candidates/java"));
            roots.push(h.join(".jdks"));
        }
    }
    for r in roots {
        let Ok(rd) = std::fs::read_dir(&r) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if let Some(exe) = exe_in(&p) {
                candidates.push((exe, "système".into()));
            } else if let Ok(rd2) = std::fs::read_dir(&p) {
                // runtime/<composant>/<plateforme>/<composant>/bin (launcher officiel)
                for e2 in rd2.flatten() {
                    if let Some(exe) = exe_in(&e2.path()).or_else(|| exe_in(&e2.path().join(e.file_name()))) {
                        candidates.push((exe, "système".into()));
                    }
                }
            }
        }
    }
    let mut out: Vec<JavaInstall> = Vec::new();
    for (exe, source) in candidates {
        let canon = exe.canonicalize().unwrap_or(exe.clone());
        if out.iter().any(|j| j.path.canonicalize().unwrap_or(j.path.clone()) == canon) {
            continue;
        }
        if let Some((version, major)) = probe(&exe) {
            out.push(JavaInstall { path: exe, version, major, source });
        }
    }
    out.sort_by(|a, b| b.major.cmp(&a.major).then(a.source.cmp(&b.source)));
    out
}

/// Clé de plateforme des runtimes Mojang.
pub fn platform_key() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86") => "windows-x86",
        ("windows", "aarch64") => "windows-arm64",
        ("windows", _) => "windows-x64",
        ("macos", "aarch64") => "mac-os-arm64",
        ("macos", _) => "mac-os",
        ("linux", "x86") => "linux-i386",
        _ => "linux",
    }
}

/// Exécutable du runtime Mojang `component` s'il est déjà installé.
pub fn mojang_runtime(layout: &Layout, component: &str) -> Option<PathBuf> {
    exe_in(&layout.runtime().join(component))
}

/// Télécharge le runtime officiel `component` (`java-runtime-delta`, `jre-legacy`…).
pub fn install_runtime(http: &Http, layout: &Layout, component: &str, cancel: &AtomicBool, sink: Sink) -> Result<PathBuf> {
    if component.is_empty() || component.contains(['/', '\\', '.']) {
        return Err(Error::Invalid(format!("composant Java refusé : {component}")));
    }
    sink(Event::Step(format!("Java ({component})")));
    let all = http.get_json(RUNTIMES, &[])?;
    let entry = all
        .get(platform_key())
        .and_then(|p| p.get(component))
        .and_then(|c| c.as_array())
        .and_then(|a| a.first())
        .ok_or_else(|| Error::Invalid(format!("Mojang ne publie pas « {component} » pour {}", platform_key())))?;
    let url = entry.pointer("/manifest/url").and_then(|u| u.as_str()).ok_or_else(|| Error::Invalid("manifeste Java incomplet".into()))?;
    let manifest = http.get_json(url, &[])?;
    let files = manifest.get("files").and_then(|f| f.as_object()).ok_or_else(|| Error::Invalid("manifeste Java vide".into()))?;
    let dir = layout.runtime().join(component);
    let mut jobs = Vec::new();
    let mut executables = Vec::new();
    let mut links = Vec::new();
    for (rel, f) in files {
        if rel.split(['/', '\\']).any(|c| c == "..") || Path::new(rel).is_absolute() {
            return Err(Error::Invalid(format!("chemin refusé dans le runtime Java : {rel}")));
        }
        match f.get("type").and_then(|t| t.as_str()) {
            Some("directory") => {
                std::fs::create_dir_all(dir.join(rel)).map_err(|e| Error::Disk(e.to_string()))?;
            }
            Some("file") => {
                let raw = f.pointer("/downloads/raw").ok_or_else(|| Error::Invalid(format!("{rel} sans téléchargement")))?;
                jobs.push(Job {
                    url: raw.get("url").and_then(|u| u.as_str()).unwrap_or_default().to_string(),
                    dest: dir.join(rel),
                    sha1: raw.get("sha1").and_then(|s| s.as_str()).map(str::to_string),
                    size: raw.get("size").and_then(|s| s.as_u64()),
                });
                if f.get("executable").and_then(|e| e.as_bool()).unwrap_or(false) {
                    executables.push(dir.join(rel));
                }
            }
            Some("link") => {
                if let Some(t) = f.get("target").and_then(|t| t.as_str()) {
                    links.push((dir.join(rel), t.to_string()));
                }
            }
            _ => {}
        }
    }
    fetch_all(http, jobs, false, cancel, sink)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for p in &executables {
            let _ = std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755));
        }
        for (link, target) in &links {
            let _ = std::fs::remove_file(link);
            let _ = std::os::unix::fs::symlink(target, link);
        }
    }
    let _ = (&executables, &links);
    exe_in(&dir).ok_or_else(|| Error::Invalid(format!("runtime {component} installé sans exécutable java")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_are_parsed() {
        assert_eq!(major_of("1.8.0_382"), Some(8));
        assert_eq!(major_of("17.0.8"), Some(17));
        assert_eq!(major_of("21"), Some(21));
        assert_eq!(major_of("21-ea"), Some(21));
        assert_eq!(major_of("x"), None);
        let out = "openjdk version \"17.0.8\" 2023-07-18\nOpenJDK Runtime Environment Temurin-17.0.8+7 (build 17.0.8+7)\n";
        assert_eq!(parse_version_output(out).as_deref(), Some("17.0.8"));
        assert_eq!(parse_version_output("java version \"1.8.0_51\"").as_deref(), Some("1.8.0_51"));
        assert!(parse_version_output("rien").is_none());
    }

    #[test]
    fn rejects_bad_components() {
        let t = tempfile::tempdir().unwrap();
        let r = install_runtime(&Http::new(), &Layout::new(t.path()), "../x", &AtomicBool::new(false), &|_| {});
        assert!(matches!(r, Err(Error::Invalid(_))));
    }
}
