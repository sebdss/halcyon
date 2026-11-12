//! Import des ROM depuis les dossiers configurés par plateforme.
//!
//! - jeux multi-disques : un `.m3u` masque les disques qu'il référence, un `.cue`
//!   masque ses pistes `.bin` ;
//! - PS3 : un dossier contenant `PS3_GAME/USRDIR/EBOOT.BIN` est un jeu ;
//! - Wii U : un `.rpx` prend le nom du dossier du jeu (`<jeu>/code/x.rpx`).
//!
//! Aucune ROM ni BIOS n'est fourni ou téléchargé par Halcyon.

use crate::emulators::PlatformDef;
use crate::standalone::clean_title;
use halcyon_core::util::path_key;
use halcyon_core::{DiscoveredGame, GameScanner, GameSource, ScanContext, ScanReport};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

const MAX_DEPTH: usize = 4;
const MAX_ENTRIES: usize = 100_000;

pub struct RomScanner {
    folders: Vec<(PlatformDef, PathBuf)>,
}

impl RomScanner {
    pub fn new(folders: Vec<(PlatformDef, PathBuf)>) -> Self {
        Self { folders }
    }
}

impl GameScanner for RomScanner {
    fn source(&self) -> GameSource {
        GameSource::Emulated
    }

    fn scan(&self, ctx: &ScanContext) -> halcyon_core::Result<ScanReport> {
        let mut report = ScanReport { complete: true, ..Default::default() };
        for (platform, root) in &self.folders {
            if ctx.is_cancelled() {
                report.complete = false;
                break;
            }
            ctx.progress(&format!("ROM {} : {}", platform.short_or_name(), root.display()));
            if !root.is_dir() {
                report.warnings.push(format!("dossier de ROM inaccessible : {}", root.display()));
                continue;
            }
            let mut files = Vec::new();
            let mut seen = 0;
            walk(root, 0, &mut files, &mut seen);
            if platform.folder_game.as_deref() == Some("ps3") {
                report.games.extend(ps3_games(&files, platform));
            }
            report.games.extend(roms_from_files(&files, platform));
        }
        log::info!("ROM : {} jeux trouvés", report.games.len());
        Ok(report)
    }
}

impl PlatformDef {
    pub fn short_or_name(&self) -> &str {
        if self.short.is_empty() { &self.name } else { &self.short }
    }
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>, seen: &mut usize) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        *seen += 1;
        if *seen > MAX_ENTRIES {
            return;
        }
        let p = e.path();
        match e.file_type() {
            Ok(t) if t.is_dir() => {
                if depth < MAX_DEPTH && !e.file_name().to_string_lossy().starts_with('.') {
                    walk(&p, depth + 1, out, seen);
                }
            }
            Ok(_) => out.push(p),
            Err(_) => {}
        }
    }
}

fn ext_of(p: &Path) -> String {
    p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

/// Fichiers référencés par une playlist `.m3u` ou un `.cue`, à ne pas lister séparément.
fn referenced_files(files: &[PathBuf]) -> HashSet<String> {
    let mut hidden = HashSet::new();
    for f in files {
        let ext = ext_of(f);
        if ext != "m3u" && ext != "cue" {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(f) else { continue };
        let dir = f.parent().unwrap_or(Path::new(""));
        for line in text.lines() {
            let line = line.trim();
            let target = if ext == "m3u" {
                (!line.is_empty() && !line.starts_with('#')).then_some(line)
            } else {
                line.strip_prefix("FILE ").and_then(|rest| {
                    let rest = rest.trim();
                    if let Some(q) = rest.strip_prefix('"') {
                        q.split('"').next()
                    } else {
                        rest.split_whitespace().next()
                    }
                })
            };
            if let Some(t) = target {
                hidden.insert(path_key(&dir.join(t)));
            }
        }
    }
    hidden
}

fn roms_from_files(files: &[PathBuf], platform: &PlatformDef) -> Vec<DiscoveredGame> {
    let exts: HashSet<String> = platform.extensions.iter().map(|e| e.to_lowercase()).collect();
    let hidden = referenced_files(files);
    let mut out = Vec::new();
    for f in files {
        let ext = ext_of(f);
        if !exts.contains(&ext) || hidden.contains(&path_key(f)) {
            continue;
        }
        // Les dossiers PS3 sont traités à part ; on ignore les binaires internes.
        if platform.folder_game.as_deref() == Some("ps3") && ext != "iso" {
            continue;
        }
        let name_source = if ext == "rpx" {
            // <jeu>/code/x.rpx → « jeu »
            f.parent().and_then(|p| p.parent()).and_then(|p| p.file_name())
        } else {
            f.file_stem()
        };
        let name = clean_title(&name_source.map(|s| s.to_string_lossy().into_owned()).unwrap_or_default());
        let mut g = DiscoveredGame::new(path_key(f), name);
        g.executable = Some(f.clone());
        g.install_dir = f.parent().map(Path::to_path_buf);
        g.platform = Some(platform.id.clone());
        g.size_bytes = std::fs::metadata(f).ok().map(|m| m.len());
        out.push(g);
    }
    out
}

fn ps3_games(files: &[PathBuf], platform: &PlatformDef) -> Vec<DiscoveredGame> {
    files
        .iter()
        .filter(|f| {
            f.file_name().is_some_and(|n| n.eq_ignore_ascii_case("EBOOT.BIN"))
                && f.parent().and_then(|p| p.file_name()).is_some_and(|n| n.eq_ignore_ascii_case("USRDIR"))
                && f.parent().and_then(|p| p.parent()).and_then(|p| p.file_name()).is_some_and(|n| n.eq_ignore_ascii_case("PS3_GAME"))
        })
        .filter_map(|eboot| {
            let game_dir = eboot.parent()?.parent()?.parent()?;
            let mut g = DiscoveredGame::new(path_key(eboot), clean_title(&game_dir.file_name()?.to_string_lossy()));
            g.executable = Some(eboot.clone());
            g.install_dir = Some(game_dir.to_path_buf());
            g.platform = Some(platform.id.clone());
            Some(g)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emulators::Registry;
    use std::fs;

    fn touch(p: &Path, content: &str) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, content).unwrap();
    }

    #[test]
    fn imports_roms_with_multidisc_and_folder_games() {
        let tmp = tempfile::tempdir().unwrap();
        let reg = Registry::builtin();
        let snes = tmp.path().join("snes");
        touch(&snes.join("Super Metroid (Japan, USA) (En,Ja).sfc"), "x");
        touch(&snes.join("notes.txt"), "x");
        let ps1 = tmp.path().join("ps1");
        touch(&ps1.join("Big Game/Big Game (Disc 1).cue"), "FILE \"Big Game (Disc 1).bin\" BINARY\n");
        touch(&ps1.join("Big Game/Big Game (Disc 1).bin"), "x");
        touch(&ps1.join("Big Game/Big Game (Disc 2).cue"), "FILE \"Big Game (Disc 2).bin\" BINARY\n");
        touch(&ps1.join("Big Game/Big Game.m3u"), "Big Game (Disc 1).cue\nBig Game (Disc 2).cue\n");
        touch(&ps1.join("Solo (USA).cue"), "FILE \"Solo (USA).bin\" BINARY");
        let ps3 = tmp.path().join("ps3");
        touch(&ps3.join("Demon Quest [BLUS00001]/PS3_GAME/USRDIR/EBOOT.BIN"), "x");
        touch(&ps3.join("Demon Quest [BLUS00001]/PS3_GAME/PARAM.SFO"), "x");

        let scanner = RomScanner::new(vec![
            (reg.platform("snes").unwrap().clone(), snes),
            (reg.platform("ps1").unwrap().clone(), ps1),
            (reg.platform("ps3").unwrap().clone(), ps3),
            (reg.platform("gba").unwrap().clone(), tmp.path().join("absent")),
        ]);
        let report = scanner.scan(&ScanContext::silent()).unwrap();
        let mut names: Vec<(String, String)> =
            report.games.iter().map(|g| (g.platform.clone().unwrap(), g.name.clone())).collect();
        names.sort();
        assert_eq!(
            names,
            [
                ("ps1".into(), "Big Game".into()),
                ("ps1".into(), "Solo".into()),
                ("ps3".into(), "Demon Quest".into()),
                ("snes".into(), "Super Metroid".into())
            ]
        );
        assert_eq!(report.warnings.len(), 1);
        let big = report.games.iter().find(|g| g.name == "Big Game").unwrap();
        assert!(big.executable.as_ref().unwrap().ends_with("Big Game.m3u"));
    }
}
