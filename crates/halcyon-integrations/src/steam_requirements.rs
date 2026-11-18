//! Configuration requise d'un jeu (boutique Steam, sans clé) comparée à ce PC.
//!
//! La fiche publique `appdetails` donne la configuration minimale et recommandée en HTML
//! (`<li><strong>Memory:</strong> 8 GB RAM</li>`). Elle est demandée en anglais : les
//! libellés sont alors stables. Seuls la mémoire, la mémoire vidéo, l'espace disque et le
//! 64 bits se comparent sans ambiguïté ; le processeur et la carte graphique sont affichés
//! côte à côte, sans verdict (comparer deux modèles demanderait une base de performances).

use serde_json::Value;

/// Une configuration (minimale ou recommandée).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Spec {
    pub os: Option<String>,
    pub cpu: Option<String>,
    pub gpu: Option<String>,
    pub directx: Option<String>,
    pub ram_mb: Option<u64>,
    /// Plus petite mémoire vidéo parmi les cartes citées.
    pub vram_mb: Option<u64>,
    pub storage_mb: Option<u64>,
    pub needs_64bit: bool,
}

impl Spec {
    fn is_empty(&self) -> bool {
        self.os.is_none() && self.cpu.is_none() && self.gpu.is_none() && self.ram_mb.is_none() && self.storage_mb.is_none()
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Requirements {
    pub minimum: Spec,
    pub recommended: Option<Spec>,
}

pub fn url(appid: u64) -> String {
    format!("https://store.steampowered.com/api/appdetails?appids={appid}&l=english&filters=basic")
}

/// Configuration de la fiche. Sous Linux, celle de la version Linux si elle est renseignée,
/// sinon celle de Windows (jeu lancé par Proton).
pub fn parse(v: &Value, appid: u64, linux: bool) -> Option<Requirements> {
    let data = v.get(appid.to_string())?.get("data")?;
    let pick = |key: &str| -> Option<Requirements> {
        let r = data.get(key)?;
        let minimum = parse_block(r.get("minimum")?.as_str()?);
        if minimum.is_empty() {
            return None;
        }
        let recommended = r.get("recommended").and_then(Value::as_str).map(parse_block).filter(|s| !s.is_empty());
        Some(Requirements { minimum, recommended })
    };
    if linux {
        if let Some(r) = pick("linux_requirements") {
            return Some(r);
        }
    }
    pick("pc_requirements")
}

/// Une liste `<li><strong>Libellé:</strong> valeur</li>`.
pub fn parse_block(html: &str) -> Spec {
    let mut spec = Spec::default();
    for item in html.split("<li>").skip(1) {
        let item = item.split("</li>").next().unwrap_or(item);
        let text = clean(item);
        if text.to_ascii_lowercase().contains("64-bit") && !item.contains("<strong>") {
            spec.needs_64bit = true;
            continue;
        }
        let Some((label, value)) = text.split_once(':') else { continue };
        let label = label.trim().trim_end_matches('*').trim().to_ascii_lowercase();
        let value = value.trim().to_string();
        if value.is_empty() {
            continue;
        }
        match label.as_str() {
            "os" | "operating system" | "os version" => {
                if value.to_ascii_lowercase().contains("64-bit") || value.contains("64 bit") {
                    spec.needs_64bit = true;
                }
                spec.os = Some(value);
            }
            "processor" | "cpu" => spec.cpu = Some(value),
            "memory" | "ram" | "system memory" => spec.ram_mb = size_mb(&value).first().copied(),
            "graphics" | "video card" | "video" | "graphics card" | "gpu" => {
                spec.vram_mb = size_mb(&value).into_iter().filter(|&m| m >= 256).min();
                spec.gpu = Some(value);
            }
            "directx" => spec.directx = Some(value),
            "storage" | "hard drive" | "hard disk space" | "disk space" | "hard disk" | "hdd" => spec.storage_mb = size_mb(&value).first().copied(),
            _ => {}
        }
    }
    spec
}

/// Texte d'un élément : balises retirées, entités courantes décodées, espaces réduits.
fn clean(html: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    let out = out.replace("&nbsp;", " ").replace('\u{a0}', " ").replace("&amp;", "&").replace("&quot;", "\"").replace("&#39;", "'");
    let out = out.replace(" :", ":");
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Tailles citées dans un texte, en Mio (« 8 GB », « 1.5GB », « 512 MB », « 1 TB »).
pub fn size_mb(text: &str) -> Vec<u64> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_ascii_digit() || (i > 0 && (chars[i - 1].is_ascii_alphanumeric() || chars[i - 1] == '.')) {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && (chars[i].is_ascii_digit() || ((chars[i] == '.' || chars[i] == ',') && i + 1 < chars.len() && chars[i + 1].is_ascii_digit())) {
            i += 1;
        }
        let number: String = chars[start..i].iter().collect::<String>().replace(',', ".");
        let mut j = i;
        while j < chars.len() && chars[j] == ' ' {
            j += 1;
        }
        let word: String = chars[j..].iter().take_while(|c| c.is_ascii_alphabetic()).collect::<String>().to_ascii_uppercase();
        let factor = match word.as_str() {
            "TB" | "TIB" => 1024.0 * 1024.0,
            "GB" | "GIB" => 1024.0,
            "MB" | "MIB" => 1.0,
            _ => 0.0,
        };
        if factor > 0.0 {
            if let Ok(n) = number.parse::<f64>() {
                out.push((n * factor).round() as u64);
            }
        }
        i = j.max(i);
    }
    out
}

/// Ce PC, tel que mesuré.
#[derive(Clone, Debug, Default)]
pub struct Machine {
    pub cpu: String,
    pub gpus: Vec<(String, Option<u64>)>,
    pub ram_mb: u64,
    pub is_64bit: bool,
}

impl Machine {
    pub fn current() -> Self {
        let mut m = crate::sysmon::Monitor::new();
        let snap = m.sample(false, false);
        Self {
            cpu: snap.cpu_name,
            gpus: snap.gpus.into_iter().map(|g| (g.name, g.vram_total_mb)).collect(),
            ram_mb: snap.ram_total_mb,
            is_64bit: cfg!(target_pointer_width = "64"),
        }
    }

    pub fn vram_mb(&self) -> Option<u64> {
        self.gpus.iter().filter_map(|g| g.1).max()
    }
}

/// Résultat d'une comparaison.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// Non comparable (texte seul, ou valeur inconnue).
    Unknown = 0,
    Below = 1,
    Minimum = 2,
    Recommended = 3,
}

/// Situe une valeur mesurée. Une marge de 10 % absorbe la mémoire réservée par le système
/// (16 Go installés : 15,5 Gio vus) et les arrondis des fiches.
pub fn level(have: Option<u64>, min: Option<u64>, rec: Option<u64>) -> Level {
    let (Some(have), Some(min)) = (have, min) else { return Level::Unknown };
    let ok = |need: u64| have as f64 >= need as f64 * 0.9;
    match rec {
        Some(r) if ok(r) => Level::Recommended,
        _ if ok(min) => {
            if rec.is_none() {
                Level::Recommended
            } else {
                Level::Minimum
            }
        }
        _ => Level::Below,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: &str = "<strong>Minimum:</strong><br><ul class=\"bb_ul\"><li>Requires a 64-bit processor and operating system<br></li><li><strong>OS:</strong> Windows 10<br></li><li><strong>Processor:</strong> INTEL CORE I5-8400 or AMD RYZEN 3 3300X<br></li><li><strong>Memory:</strong> 12&nbsp;GB RAM<br></li><li><strong>Graphics:</strong> NVIDIA GEFORCE GTX 1060 3 GB or AMD RADEON RX 580 4 GB<br></li><li><strong>DirectX:</strong> Version 12<br></li><li><strong>Storage:</strong> 60 GB available space<br></li><li><strong>Additional Notes:</strong> </li></ul>";

    #[test]
    fn parses_store_block() {
        let s = parse_block(MIN);
        assert!(s.needs_64bit);
        assert_eq!(s.os.as_deref(), Some("Windows 10"));
        assert_eq!(s.cpu.as_deref(), Some("INTEL CORE I5-8400 or AMD RYZEN 3 3300X"));
        assert_eq!(s.ram_mb, Some(12 * 1024));
        assert_eq!(s.vram_mb, Some(3 * 1024));
        assert_eq!(s.storage_mb, Some(60 * 1024));
        assert_eq!(s.directx.as_deref(), Some("Version 12"));
    }

    #[test]
    fn sizes() {
        assert_eq!(size_mb("2048 MB RAM"), vec![2048]);
        assert_eq!(size_mb("1.5GB"), vec![1536]);
        assert_eq!(size_mb("1,5 GB"), vec![1536]);
        assert_eq!(size_mb("GTX 1060 6GB / RX 480"), vec![6144]);
        assert_eq!(size_mb("DirectX 11 compatible, Shader Model 5"), Vec::<u64>::new());
        assert_eq!(size_mb("1 TB SSD"), vec![1024 * 1024]);
        assert_eq!(size_mb("RTX3060 12 GB"), vec![12 * 1024]);
    }

    #[test]
    fn picks_linux_when_present() {
        let v = serde_json::json!({"10": {"success": true, "data": {
            "pc_requirements": {"minimum": MIN},
            "linux_requirements": {"minimum": "<ul class=\"bb_ul\"></ul>"}
        }}});
        assert_eq!(parse(&v, 10, true).unwrap().minimum.ram_mb, Some(12 * 1024));
        let v = serde_json::json!({"10": {"success": true, "data": {"pc_requirements": []}}});
        assert!(parse(&v, 10, false).is_none());
    }

    #[test]
    fn levels_with_margin() {
        assert_eq!(level(Some(15_800), Some(8192), Some(16_384)), Level::Recommended);
        assert_eq!(level(Some(12_000), Some(8192), Some(16_384)), Level::Minimum);
        assert_eq!(level(Some(4096), Some(8192), Some(16_384)), Level::Below);
        assert_eq!(level(None, Some(8192), None), Level::Unknown);
        assert_eq!(level(Some(8192), Some(8192), None), Level::Recommended);
    }
}
