//! Base LaunchBox (1.0.0-alpha.7) : règles pures pour retrouver le dos et la tranche d'un
//! boîtier. Le téléchargement et l'index sont ailleurs (`halcyon-integrations`,
//! `halcyon-storage`) ; ici : nom normalisé, plateforme, choix de l'image, contrôle des noms
//! de fichiers reçus.

use crate::model::GameSource;

/// Hôte des images de la base (aucun autre n'est contacté pour les dos et les tranches).
pub const IMAGE_HOST: &str = "https://images.launchbox-app.com/";
/// Archive des métadonnées (environ 110 Mo, un fichier XML d'environ 500 Mo).
pub const METADATA_URL: &str = "https://gamesdb.launchbox-app.com/Metadata.zip";

/// Partie de boîtier recherchée.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    Back,
    Spine,
}

impl Part {
    pub fn id(self) -> i64 {
        match self {
            Part::Back => 1,
            Part::Spine => 2,
        }
    }
}

/// Nom de plateforme LaunchBox d'un jeu : « Windows » pour les jeux PC, le système pour les ROM.
pub fn platform_for(source: GameSource, platform: Option<&str>) -> Option<&'static str> {
    if let Some(p) = platform {
        return Some(match p {
            "nes" => "Nintendo Entertainment System",
            "snes" => "Super Nintendo Entertainment System",
            "n64" => "Nintendo 64",
            "gb" => "Nintendo Game Boy",
            "gbc" => "Nintendo Game Boy Color",
            "gba" => "Nintendo Game Boy Advance",
            "nds" => "Nintendo DS",
            "gamecube" => "Nintendo GameCube",
            "wii" => "Nintendo Wii",
            "wiiu" => "Nintendo Wii U",
            "switch" => "Nintendo Switch",
            "megadrive" => "Sega Genesis",
            "mastersystem" => "Sega Master System",
            "gamegear" => "Sega Game Gear",
            "saturn" => "Sega Saturn",
            "dreamcast" => "Sega Dreamcast",
            "pcengine" => "NEC TurboGrafx-16",
            "atari2600" => "Atari 2600",
            "atari7800" => "Atari 7800",
            "lynx" => "Atari Lynx",
            "ps1" => "Sony Playstation",
            "ps2" => "Sony Playstation 2",
            "ps3" => "Sony Playstation 3",
            "psp" => "Sony PSP",
            _ => return None,
        });
    }
    match source {
        GameSource::Minecraft | GameSource::Emulated => None,
        _ => Some("Windows"),
    }
}

/// Plateformes indexées (les autres sont ignorées à la lecture : index plus petit).
pub const PLATFORMS: &[&str] = &[
    "Windows",
    "Nintendo Entertainment System",
    "Super Nintendo Entertainment System",
    "Nintendo 64",
    "Nintendo Game Boy",
    "Nintendo Game Boy Color",
    "Nintendo Game Boy Advance",
    "Nintendo DS",
    "Nintendo GameCube",
    "Nintendo Wii",
    "Nintendo Wii U",
    "Nintendo Switch",
    "Sega Genesis",
    "Sega Master System",
    "Sega Game Gear",
    "Sega Saturn",
    "Sega Dreamcast",
    "NEC TurboGrafx-16",
    "Atari 2600",
    "Atari 7800",
    "Atari Lynx",
    "Sony Playstation",
    "Sony Playstation 2",
    "Sony Playstation 3",
    "Sony PSP",
];

/// Nom normalisé pour comparer des titres : minuscules sans accents, « & » → « and »,
/// ponctuation retirée, articles anglais retirés, chiffres romains II à XX en chiffres
/// (« Black Ops III » = « Black Ops 3 » ; « X » seul reste tel quel : Mega Man X).
pub fn normalize_title(s: &str) -> String {
    let mut flat = String::with_capacity(s.len());
    for c in s.chars() {
        let c = fold_accent(c);
        match c {
            '&' => flat.push_str(" and "),
            '\'' | '’' | '™' | '®' => {}
            c if c.is_alphanumeric() => flat.extend(c.to_lowercase()),
            _ => flat.push(' '),
        }
    }
    let words: Vec<String> = flat
        .split_whitespace()
        .filter(|w| !matches!(*w, "the" | "a" | "an"))
        .map(|w| roman(w).map(|n| n.to_string()).unwrap_or_else(|| w.to_string()))
        .collect();
    words.join(" ")
}

fn fold_accent(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => 'a',
        'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' => 'a',
        'ç' | 'Ç' => 'c',
        'è' | 'é' | 'ê' | 'ë' | 'È' | 'É' | 'Ê' | 'Ë' => 'e',
        'ì' | 'í' | 'î' | 'ï' | 'Ì' | 'Í' | 'Î' | 'Ï' => 'i',
        'ñ' | 'Ñ' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' | 'ø' | 'Ø' => 'o',
        'ù' | 'ú' | 'û' | 'ü' | 'Ù' | 'Ú' | 'Û' | 'Ü' => 'u',
        'ý' | 'ÿ' | 'Ý' => 'y',
        c => c,
    }
}

fn roman(w: &str) -> Option<u32> {
    Some(match w {
        "ii" => 2,
        "iii" => 3,
        "iv" => 4,
        "v" => 5,
        "vi" => 6,
        "vii" => 7,
        "viii" => 8,
        "ix" => 9,
        "xi" => 11,
        "xii" => 12,
        "xiii" => 13,
        "xiv" => 14,
        "xv" => 15,
        "xvi" => 16,
        "xvii" => 17,
        "xviii" => 18,
        "xix" => 19,
        "xx" => 20,
        _ => return None,
    })
}

/// Partie et rang d'une image de la base (rang plus petit = meilleur), ou `None` si l'image ne
/// sert pas. Les scans réels passent avant les reconstructions et les créations de fans ; parmi
/// eux, les éditions anglaises sans région, puis Amérique du Nord, Europe, monde, le reste.
pub fn image_rank(kind: &str, region: &str) -> Option<(Part, u32)> {
    let (part, base) = match kind {
        "Box - Back" => (Part::Back, 0),
        "Box - Back - Reconstructed" => (Part::Back, 10),
        "Fanart - Box - Back" => (Part::Back, 20),
        "Box - Spine" => (Part::Spine, 0),
        _ => return None,
    };
    let r = match region.trim() {
        "" => 0,
        "United States" | "North America" => 1,
        "United Kingdom" | "Australia" => 2,
        "Europe" => 3,
        "World" => 4,
        _ => 6,
    };
    Some((part, base + r))
}

/// Nom de fichier d'image tel que la base le donne (« 3c1c….jpg », parfois dans un
/// sous-dossier) : lettres, chiffres, `-`, `_`, `.`, au plus un `/`, extension d'image, jamais
/// `..`. Tout autre nom est ignoré : rien d'autre ne peut être demandé au serveur ni écrit.
pub fn valid_image_file(name: &str) -> bool {
    if name.is_empty() || name.len() > 160 || name.contains("..") || name.starts_with('/') || name.matches('/').count() > 1 {
        return false;
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/')) {
        return false;
    }
    let lower = name.to_ascii_lowercase();
    [".png", ".jpg", ".jpeg"].iter().any(|e| lower.ends_with(e))
}

/// L'octet de tête d'une image reçue est-il bien du PNG ou du JPEG ?
pub fn looks_like_image(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0x89, b'P', b'N', b'G']) || bytes.starts_with(&[0xFF, 0xD8, 0xFF])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_compare_loosely() {
        assert_eq!(normalize_title("The Legend of Zelda: Ocarina of Time"), "legend of zelda ocarina of time");
        assert_eq!(normalize_title("Call of Duty®: Black Ops III"), "call of duty black ops 3");
        assert_eq!(normalize_title("Call of Duty: Black Ops 3"), "call of duty black ops 3");
        assert_eq!(normalize_title("Mega Man X"), "mega man x");
        assert_eq!(normalize_title("Pokémon Rouge & Bleu"), "pokemon rouge and bleu");
        assert_eq!(normalize_title("Assassin's Creed™ II"), "assassins creed 2");
        assert_eq!(normalize_title("  "), "");
    }

    #[test]
    fn platforms_map_sources_and_systems() {
        assert_eq!(platform_for(GameSource::Steam, None), Some("Windows"));
        assert_eq!(platform_for(GameSource::Emulated, Some("snes")), Some("Super Nintendo Entertainment System"));
        assert_eq!(platform_for(GameSource::Emulated, Some("inconnue")), None);
        assert_eq!(platform_for(GameSource::Minecraft, None), None);
        for p in ["nes", "gamecube", "ps2", "psp", "megadrive"] {
            assert!(PLATFORMS.contains(&platform_for(GameSource::Emulated, Some(p)).unwrap()));
        }
    }

    #[test]
    fn real_scans_and_english_regions_first() {
        let us = image_rank("Box - Back", "North America").unwrap();
        let none = image_rank("Box - Back", "").unwrap();
        let jp = image_rank("Box - Back", "Japan").unwrap();
        let recon = image_rank("Box - Back - Reconstructed", "").unwrap();
        assert_eq!(us.0, Part::Back);
        assert!(none.1 < us.1 && us.1 < jp.1 && jp.1 < recon.1);
        assert_eq!(image_rank("Box - Spine", "Europe").unwrap().0, Part::Spine);
        assert!(image_rank("Box - Front", "").is_none());
        assert!(image_rank("Clear Logo", "").is_none());
    }

    #[test]
    fn image_names_are_checked() {
        assert!(valid_image_file("3c1c2f4e-1b2a-4c5d-9e8f-0a1b2c3d4e5f.jpg"));
        assert!(!valid_image_file("Box - Back/abc.png"));
        assert!(valid_image_file("sub/abc-1.PNG"));
        for bad in ["", "../etc/passwd.png", "/abs.png", "a/b/c.png", "x.exe", "a b.jpg", "x.jpg?y=1", "a\\b.jpg"] {
            assert!(!valid_image_file(bad), "{bad}");
        }
        assert!(looks_like_image(&[0x89, b'P', b'N', b'G', 13, 10]));
        assert!(looks_like_image(&[0xFF, 0xD8, 0xFF, 0xE0]));
        assert!(!looks_like_image(b"<html>"));
    }
}
