//! Source « standalone » : jeux installés hors plateforme, trouvés en parcourant
//! des dossiers choisis par l'utilisateur (ex. `D:\Jeux`).
//!
//! Hypothèse : chaque sous-dossier direct d'un dossier surveillé est un jeu.
//! Pour chacun, on cherche le meilleur exécutable (jusqu'à 3 niveaux) en
//! écartant installeurs, désinstalleurs, redistribuables et outils de rapport
//! de plantage. L'utilisateur peut corriger l'exécutable retenu depuis la fiche.
//!
//! Sous Linux, sont aussi reconnus les exécutables natifs (ELF, `.sh`, `.x86_64`,
//! AppImage) — préférés aux `.exe`, qui restent lançables via Wine/Proton.

use halcyon_core::util::{path_key, search_key};
use halcyon_core::{DiscoveredGame, GameScanner, GameSource, ScanContext, ScanReport};
use std::path::{Path, PathBuf};

const MAX_DEPTH: usize = 3;
/// Limite de fichiers examinés par jeu (protège contre les arborescences énormes).
const MAX_ENTRIES_PER_GAME: usize = 4_000;

/// Dossiers jamais explorés (contenu tiers, jamais le jeu lui-même).
const SKIP_DIRS: &[&str] = &[
    "_commonredist", "commonredist", "redist", "redists", "redistributables", "directx", "dotnet", "vcredist",
    "support", "__installer", "installer", "installers", "prerequisites", "prereqs", "easyanticheat", "battleye",
    "crashreportclient", "engine", "mono", "monobleedingedge", "__macosx", "_redist", "dxsetup", "physx",
];

/// Fragments de noms d'exécutables qui ne lancent pas le jeu.
const BAD_EXE_PARTS: &[&str] = &[
    "unins", "uninst", "setup", "install", "redist", "vc_redist", "vcredist", "dxsetup", "dxwebsetup", "crash",
    "report", "unitycrashhandler", "updater", "update", "patcher", "helper", "dotnet", "python", "7z", "config",
    "settings", "touchup", "cleanup", "easyanticheat", "battleye", "beservice", "eac", "dedicated", "server",
    "editor", "sdk", "benchmark", "register", "activation", "cefprocess", "webhelper", "notification", "overlay",
];

pub struct StandaloneScanner {
    roots: Vec<PathBuf>,
}

impl StandaloneScanner {
    pub fn new(roots: Vec<PathBuf>) -> Self {
        Self { roots }
    }
}

impl GameScanner for StandaloneScanner {
    fn source(&self) -> GameSource {
        GameSource::Standalone
    }

    fn scan(&self, ctx: &ScanContext) -> halcyon_core::Result<ScanReport> {
        // Un dossier débranché rend ses jeux indisponibles : `complete = true` les
        // marque désinstallés, et ils reviennent automatiquement au prochain scan.
        let mut report = ScanReport { complete: true, ..Default::default() };
        for root in &self.roots {
            if ctx.is_cancelled() {
                report.complete = false;
                break;
            }
            ctx.progress(&format!("Dossiers : {}", root.display()));
            let entries = match std::fs::read_dir(root) {
                Ok(e) => e,
                Err(e) => {
                    report.warnings.push(format!("dossier inaccessible {} : {e}", root.display()));
                    continue;
                }
            };
            let mut dirs: Vec<PathBuf> = Vec::new();
            let mut root_has_exe = false;
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    if !is_skipped_dir(&p) {
                        dirs.push(p);
                    }
                } else if is_candidate(&p) {
                    root_has_exe = true;
                }
            }
            // Si l'utilisateur a pointé directement le dossier d'un jeu.
            if root_has_exe && dirs.len() <= 3 {
                if let Some(g) = detect_game(root) {
                    report.games.push(g);
                    continue;
                }
            }
            dirs.sort();
            for dir in dirs {
                if let Some(g) = detect_game(&dir) {
                    report.games.push(g);
                }
            }
        }
        log::info!("Dossiers : {} jeux trouvés", report.games.len());
        Ok(report)
    }
}

fn is_exe(p: &Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe"))
}

/// Exécutable natif Linux : script, AppImage, binaire Unity/Godot, ou fichier ELF exécutable.
pub fn is_native_linux(p: &Path) -> bool {
    if !cfg!(unix) {
        return false;
    }
    let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "sh" | "x86_64" | "x86" | "appimage" => true,
        "" => is_executable_elf(p),
        _ => false,
    }
}

#[cfg(unix)]
fn is_executable_elf(p: &Path) -> bool {
    use std::io::Read;
    use std::os::unix::fs::PermissionsExt;
    let Ok(meta) = std::fs::metadata(p) else { return false };
    if meta.permissions().mode() & 0o111 == 0 || meta.len() < 64 {
        return false;
    }
    let mut magic = [0u8; 4];
    std::fs::File::open(p).and_then(|mut f| f.read_exact(&mut magic)).is_ok() && &magic == b"\x7fELF"
}

#[cfg(not(unix))]
fn is_executable_elf(_: &Path) -> bool {
    false
}

fn is_candidate(p: &Path) -> bool {
    is_exe(p) || is_native_linux(p)
}

fn is_skipped_dir(p: &Path) -> bool {
    let name = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    name.starts_with('.') || name.starts_with('$') || SKIP_DIRS.contains(&name.as_str())
}

/// Analyse un dossier de jeu ; `None` s'il ne contient aucun exécutable plausible.
pub fn detect_game(dir: &Path) -> Option<DiscoveredGame> {
    let folder_name = dir.file_name()?.to_string_lossy().into_owned();
    let mut candidates = Vec::new();
    collect_exes(dir, 0, &mut candidates, &mut 0);
    let best = candidates
        .into_iter()
        .map(|(path, depth, size)| (score_exe(&path, depth, size, &folder_name), path))
        .filter(|(score, _)| *score > -50)
        .max_by_key(|(score, _)| *score)?
        .1;

    let mut g = DiscoveredGame::new(path_key(&best), clean_title(&folder_name));
    g.working_dir = best.parent().map(Path::to_path_buf);
    g.executable = Some(best);
    g.install_dir = Some(dir.to_path_buf());
    Some(g)
}

fn collect_exes(dir: &Path, depth: usize, out: &mut Vec<(PathBuf, usize, u64)>, seen: &mut usize) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        *seen += 1;
        if *seen > MAX_ENTRIES_PER_GAME {
            return;
        }
        let p = e.path();
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_dir() {
            if depth < MAX_DEPTH && !is_skipped_dir(&p) {
                collect_exes(&p, depth + 1, out, seen);
            }
        } else if is_candidate(&p) {
            let size = e.metadata().map(|m| m.len()).unwrap_or(0);
            out.push((p, depth, size));
        }
    }
}

/// Score heuristique d'un exécutable : proximité avec le nom du dossier,
/// profondeur, taille, et pénalité forte pour les utilitaires connus.
fn score_exe(path: &Path, depth: usize, size: u64, folder: &str) -> i64 {
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
    let mut score: i64 = 0;
    if BAD_EXE_PARTS.iter().any(|b| stem.contains(b)) {
        score -= 100;
    }
    let s = compact(&stem);
    let f = compact(folder);
    if !s.is_empty() && s == f {
        score += 60;
    } else if !s.is_empty() && (f.contains(&s) || s.contains(&f)) {
        score += 35;
    } else {
        let folder_words = search_key(folder);
        let shared = search_key(&stem).split(' ').filter(|w| w.len() > 2 && folder_words.contains(*w)).count();
        score += 10 * shared as i64;
    }
    // Exécutables Unreal : préférer le binaire « Shipping » au lanceur s'il est seul.
    if stem.ends_with("-win64-shipping") {
        score += 5;
    }
    let parent = path.parent().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    if matches!(parent.as_str(), "bin64" | "x64" | "win64" | "binaries") {
        score += 5;
    }
    // Sous Linux, une version native est préférée à la version Windows.
    if is_native_linux(path) {
        score += 25;
        if stem.ends_with(".sh") || path.extension().is_some_and(|e| e == "sh") {
            score -= 5;
        }
    }
    score -= 12 * depth as i64;
    // Bonus logarithmique de taille : un jeu pèse plus lourd qu'un utilitaire.
    score += (64 - size.max(1).leading_zeros() as i64).min(30);
    score
}

fn compact(s: &str) -> String {
    search_key(s).replace(' ', "")
}

/// Nettoie un nom de dossier : « The_Witcher.3-v1.32 [GOG] » → « The Witcher 3 ».
pub fn clean_title(folder: &str) -> String {
    let mut stripped = String::new();
    let mut depth = 0;
    for c in folder.chars() {
        match c {
            '[' | '(' => depth += 1,
            ']' | ')' => depth = (depth - 1).max(0),
            _ if depth > 0 => {}
            _ => stripped.push(c),
        }
    }
    let is_version = |t: &str| {
        let lt = t.to_lowercase();
        let body = lt.strip_prefix('v').unwrap_or(&lt);
        let numeric = !body.is_empty() && body.chars().all(|c| c.is_ascii_digit() || c == '.');
        numeric && (lt.starts_with('v') || body.contains('.'))
    };
    let words: Vec<String> = stripped
        .split(|c: char| c.is_whitespace() || c == '_' || c == '-')
        .filter(|t| !t.is_empty() && !is_version(t))
        .filter(|t| !matches!(t.to_lowercase().as_str(), "gog" | "repack" | "portable"))
        .map(|t| t.replace('.', " "))
        .collect();
    let out = words.join(" ").split_whitespace().collect::<Vec<_>>().join(" ");
    if out.is_empty() { folder.to_string() } else { out }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn touch(p: &Path, size: usize) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, vec![0u8; size]).unwrap();
    }

    #[test]
    fn picks_the_game_executable() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("Jeux");
        touch(&root.join("Hollow Knight/hollow_knight.exe"), 600_000);
        touch(&root.join("Hollow Knight/UnityCrashHandler64.exe"), 1_500_000);
        touch(&root.join("Hollow Knight/unins000.exe"), 3_000_000);
        touch(&root.join("Hollow Knight/_CommonRedist/vcredist_x64.exe"), 9_000_000);
        touch(&root.join("Stray/Stray.exe"), 300_000);
        touch(&root.join("Stray/Hk_project/Binaries/Win64/Stray-Win64-Shipping.exe"), 90_000_000);
        touch(&root.join("Empty/readme.txt"), 10);

        let report = StandaloneScanner::new(vec![root.clone(), tmp.path().join("absent")])
            .scan(&ScanContext::silent())
            .unwrap();
        assert_eq!(report.games.len(), 2);
        assert_eq!(report.warnings.len(), 1);

        let hk = report.games.iter().find(|g| g.name == "Hollow Knight").unwrap();
        assert!(hk.executable.as_ref().unwrap().ends_with("hollow_knight.exe"));
        assert_eq!(hk.working_dir.as_ref().unwrap(), &root.join("Hollow Knight"));

        let stray = report.games.iter().find(|g| g.name == "Stray").unwrap();
        assert!(stray.executable.as_ref().unwrap().ends_with("Stray.exe"), "le lanceur racine prime");
    }

    #[cfg(unix)]
    #[test]
    fn prefers_native_linux_builds() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let game = tmp.path().join("Jeux/Celeste");
        touch(&game.join("Celeste.exe"), 2_000_000);
        let elf = game.join("Celeste");
        let mut bytes = b"\x7fELF".to_vec();
        bytes.extend(vec![0u8; 500_000]);
        fs::write(&elf, bytes).unwrap();
        fs::set_permissions(&elf, fs::Permissions::from_mode(0o755)).unwrap();
        touch(&game.join("data.bin"), 10);
        let g = detect_game(&game).unwrap();
        assert_eq!(g.executable.unwrap(), elf);
        assert!(!is_native_linux(&game.join("data.bin")));
    }

    #[test]
    fn root_pointing_to_a_single_game() {
        let tmp = tempfile::tempdir().unwrap();
        let game = tmp.path().join("Celeste");
        touch(&game.join("Celeste.exe"), 1000);
        touch(&game.join("Content/data.bin"), 10);
        let report = StandaloneScanner::new(vec![game]).scan(&ScanContext::silent()).unwrap();
        assert_eq!(report.games.len(), 1);
        assert_eq!(report.games[0].name, "Celeste");
    }

    #[test]
    fn cleans_folder_titles() {
        assert_eq!(clean_title("The_Witcher.3-v1.32 [GOG]"), "The Witcher 3");
        assert_eq!(clean_title("Hades (2020)"), "Hades");
        assert_eq!(clean_title("Battlefield 2042"), "Battlefield 2042");
        assert_eq!(clean_title("Game-1.0.3"), "Game");
        assert_eq!(clean_title("Halo 3"), "Halo 3");
        assert_eq!(clean_title("[x]"), "[x]");
    }
}
