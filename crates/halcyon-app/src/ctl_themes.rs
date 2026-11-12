//! Personnalisation : thèmes (intégrés, importés, fournis par une extension),
//! couleur d'accent, densité, animations, fond d'écran, barre latérale et accueil.
//!
//! Un thème est un simple fichier JSON de couleurs : il ne contient aucun code.

use crate::controller::Controller;
use crate::{AppUi, NavVisibility, Theme, ThemeRow, ThemeUi, ToggleRow};
use halcyon_storage::settings::Appearance;
use serde::{Deserialize, Serialize};
use slint::{Color, ComponentHandle, ModelRc, SharedString, VecModel};
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// Couleurs d'un thème, au format « #rrggbb » ou « #rrggbbaa ». Une couleur absente
/// reprend celle du thème Halcyon.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Palette {
    pub bg: String,
    pub sidebar: String,
    pub surface: String,
    pub surface_hover: String,
    pub surface_active: String,
    pub border: String,
    pub border_strong: String,
    pub text: String,
    pub text_muted: String,
    pub text_dim: String,
    pub accent: String,
    pub on_accent: String,
    pub warm: String,
    pub danger: String,
    pub success: String,
    pub bp_bg: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeDef {
    pub id: String,
    pub name: String,
    pub author: String,
    pub colors: Palette,
    /// Provenance affichée (« importé », nom de l'extension) ; non sérialisée.
    #[serde(skip)]
    pub source: String,
}

#[allow(clippy::too_many_arguments)]
fn palette(bg: &str, sidebar: &str, surface: &str, hover: &str, active: &str, accent: &str, on_accent: &str, bp: &str) -> Palette {
    Palette {
        bg: bg.into(),
        sidebar: sidebar.into(),
        surface: surface.into(),
        surface_hover: hover.into(),
        surface_active: active.into(),
        accent: accent.into(),
        on_accent: on_accent.into(),
        bp_bg: bp.into(),
        ..Default::default()
    }
}

pub fn builtin_themes() -> Vec<ThemeDef> {
    let t = |id: &str, name: &str, colors: Palette| ThemeDef { id: id.into(), name: name.into(), author: "Halcyon".into(), colors, source: String::new() };
    vec![
        t("halcyon", "Halcyon", Palette::default()),
        t("minuit", "Minuit", palette("#0b0e1a", "#0e1220", "#151a2c", "#1c2238", "#252c46", "#6c8cff", "#0a1033", "#060812")),
        t("braise", "Braise", Palette { warm: "#ffc46b".into(), ..palette("#110c0a", "#150f0c", "#1d1512", "#261c18", "#30231e", "#ff7a45", "#2a0f03", "#0a0706") }),
        t("foret", "Forêt", palette("#0a100c", "#0c140f", "#121c16", "#18251d", "#1f2f25", "#5fd48a", "#04210f", "#060a07")),
        t("graphite", "Graphite", palette("#101010", "#141414", "#1b1b1b", "#232323", "#2c2c2c", "#d8dce3", "#111316", "#080808")),
    ]
}

/// Couleurs d'accent proposées (nom, valeur).
pub const ACCENTS: [(&str, &str); 8] = [
    ("Turquoise", "#2fd4c5"),
    ("Bleu", "#5b8cff"),
    ("Violet", "#a77bff"),
    ("Rose", "#ff6fae"),
    ("Orange", "#ff8a4c"),
    ("Jaune", "#f2c94c"),
    ("Vert", "#5ad48a"),
    ("Rouge", "#ff5d5d"),
];

pub fn parse_hex(s: &str) -> Option<Color> {
    let h = s.trim().strip_prefix('#')?;
    if !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
    match h.len() {
        6 => Some(Color::from_rgb_u8(byte(0)?, byte(2)?, byte(4)?)),
        8 => Some(Color::from_argb_u8(byte(6)?, byte(0)?, byte(2)?, byte(4)?)),
        _ => None,
    }
}

/// Texte lisible sur l'accent : sombre teinté pour les couleurs claires, blanc sinon.
fn on_accent_for(c: Color) -> Color {
    let lum = 0.2126 * f32::from(c.red()) + 0.7152 * f32::from(c.green()) + 0.0722 * f32::from(c.blue());
    if lum > 110.0 {
        let k = |v: u8| (f32::from(v) * 0.12) as u8;
        Color::from_rgb_u8(k(c.red()), k(c.green()), k(c.blue()))
    } else {
        Color::from_rgb_u8(255, 255, 255)
    }
}

/// Lit un fichier de thème ; refuse les identifiants des thèmes intégrés.
pub fn read_theme_file(path: &Path) -> Result<ThemeDef, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("lecture impossible : {e}"))?;
    let value: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("JSON invalide : {e}"))?;
    theme_from_value(&value)
}

pub fn theme_from_value(value: &serde_json::Value) -> Result<ThemeDef, String> {
    let t: ThemeDef = serde_json::from_value(value.clone()).map_err(|e| format!("thème invalide : {e}"))?;
    let id_ok = !t.id.is_empty() && t.id.len() <= 64 && t.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !id_ok {
        return Err("identifiant de thème invalide (lettres, chiffres, - et _)".into());
    }
    if t.name.trim().is_empty() {
        return Err("nom de thème manquant".into());
    }
    if builtin_themes().iter().any(|b| b.id == t.id) {
        return Err(format!("« {} » est un thème intégré : choisissez un autre identifiant", t.id));
    }
    let c = &t.colors;
    for (name, v) in [
        ("bg", &c.bg), ("sidebar", &c.sidebar), ("surface", &c.surface), ("surface_hover", &c.surface_hover),
        ("surface_active", &c.surface_active), ("border", &c.border), ("border_strong", &c.border_strong),
        ("text", &c.text), ("text_muted", &c.text_muted), ("text_dim", &c.text_dim), ("accent", &c.accent),
        ("on_accent", &c.on_accent), ("warm", &c.warm), ("danger", &c.danger), ("success", &c.success), ("bp_bg", &c.bp_bg),
    ] {
        if !v.is_empty() && parse_hex(v).is_none() {
            return Err(format!("couleur « {name} » invalide : {v}"));
        }
    }
    Ok(t)
}

fn hex(c: Color) -> String {
    if c.alpha() == 255 {
        format!("#{:02x}{:02x}{:02x}", c.red(), c.green(), c.blue())
    } else {
        format!("#{:02x}{:02x}{:02x}{:02x}", c.red(), c.green(), c.blue(), c.alpha())
    }
}

/// Entrées masquables de la barre latérale.
pub const NAV_ITEMS: [(&str, &str); 11] = [
    ("assistant", "Que jouer ?"),
    ("big-picture", "Big Picture"),
    ("achievements", "Succès"),
    ("stats", "Statistiques"),
    ("journal", "Journal"),
    ("updates", "Mises à jour"),
    ("saves", "Sauvegardes"),
    ("emulators", "Émulateurs"),
    ("controllers", "Manettes"),
    ("remote", "Jeu à distance"),
    ("plugins", "Extensions"),
];

impl Controller {
    pub(crate) fn themes_dir(&self) -> PathBuf {
        self.paths.data_dir.join("themes")
    }

    /// Thèmes intégrés, puis importés (dossier `themes`), puis fournis par les extensions actives.
    pub(crate) fn all_themes(&self) -> Vec<ThemeDef> {
        let mut out = builtin_themes();
        if let Ok(entries) = std::fs::read_dir(self.themes_dir()) {
            let mut files: Vec<PathBuf> =
                entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
            files.sort();
            for f in files {
                match read_theme_file(&f) {
                    Ok(mut t) if !out.iter().any(|o| o.id == t.id) => {
                        t.source = "importé".into();
                        out.push(t);
                    }
                    Ok(_) => {}
                    Err(e) => log::warn!("thème {} ignoré : {e}", f.display()),
                }
            }
        }
        for (plugin, value) in self.plugin_themes() {
            match theme_from_value(&value) {
                Ok(mut t) if !out.iter().any(|o| o.id == t.id) => {
                    t.source = plugin;
                    out.push(t);
                }
                Ok(_) => {}
                Err(e) => log::warn!("thème de l'extension {plugin} ignoré : {e}"),
            }
        }
        out
    }

    /// Applique l'apparence enregistrée au thème Slint.
    pub(crate) fn apply_appearance(&self) {
        let Some(app) = self.app() else { return };
        let a = self.settings.borrow().appearance.clone();
        let themes = self.all_themes();
        let def = themes.iter().find(|t| t.id == a.theme).cloned().unwrap_or_else(|| themes[0].clone());
        let base = Palette {
            bg: "#0a0c10".into(),
            sidebar: "#0d1015".into(),
            surface: "#13171f".into(),
            surface_hover: "#1a1f29".into(),
            surface_active: "#222835".into(),
            border: "#ffffff12".into(),
            border_strong: "#ffffff22".into(),
            text: "#eceff4".into(),
            text_muted: "#9aa3b3".into(),
            text_dim: "#5f6777".into(),
            accent: "#2fd4c5".into(),
            on_accent: "#03201d".into(),
            warm: "#ff9f5a".into(),
            danger: "#ff6b6b".into(),
            success: "#5ad48a".into(),
            bp_bg: "#05070a".into(),
        };
        let c = &def.colors;
        let pick = |v: &str, fallback: &str| parse_hex(v).or_else(|| parse_hex(fallback)).unwrap_or_default();
        let theme = app.global::<Theme>();
        theme.set_bg(pick(&c.bg, &base.bg));
        theme.set_sidebar(pick(&c.sidebar, &base.sidebar));
        theme.set_surface(pick(&c.surface, &base.surface));
        theme.set_surface_hover(pick(&c.surface_hover, &base.surface_hover));
        theme.set_surface_active(pick(&c.surface_active, &base.surface_active));
        theme.set_border(pick(&c.border, &base.border));
        theme.set_border_strong(pick(&c.border_strong, &base.border_strong));
        theme.set_text(pick(&c.text, &base.text));
        theme.set_text_muted(pick(&c.text_muted, &base.text_muted));
        theme.set_text_dim(pick(&c.text_dim, &base.text_dim));
        theme.set_warm(pick(&c.warm, &base.warm));
        theme.set_danger(pick(&c.danger, &base.danger));
        theme.set_success(pick(&c.success, &base.success));
        theme.set_bp_bg(pick(&c.bp_bg, &base.bp_bg));
        match parse_hex(&a.accent) {
            Some(accent) => {
                theme.set_accent(accent);
                theme.set_on_accent(on_accent_for(accent));
            }
            None => {
                theme.set_accent(pick(&c.accent, &base.accent));
                theme.set_on_accent(if c.accent.is_empty() { pick(&base.on_accent, "") } else { pick(&c.on_accent, &hex(on_accent_for(pick(&c.accent, &base.accent)))) });
            }
        }
        theme.set_density(i32::from(a.density.min(1)));
        theme.set_animated_background(a.animated_background);
        theme.set_background_opacity(a.background_opacity.clamp(0.02, 0.6));
        let image = a.background.as_ref().filter(|p| p.is_file()).and_then(|p| match slint::Image::load_from_path(p) {
            Ok(img) => Some(img),
            Err(_) => {
                log::warn!("fond d'écran illisible : {}", p.display());
                None
            }
        });
        theme.set_has_background(image.is_some());
        if let Some(img) = image {
            theme.set_background_image(img);
        }

        let hidden = &a.hidden_nav;
        let show = |k: &str| !hidden.iter().any(|h| h == k);
        app.global::<AppUi>().set_nav(NavVisibility {
            assistant: show("assistant"),
            big_picture: show("big-picture"),
            achievements: show("achievements"),
            stats: show("stats"),
            journal: show("journal"),
            updates: show("updates"),
            saves: show("saves"),
            emulators: show("emulators"),
            controllers: show("controllers"),
            remote: show("remote"),
            plugins: show("plugins"),
        });
        self.refresh_appearance_ui_with(&themes);
        // Mouvement, effets, taille du texte, modes : appliqués en dernier (le mode
        // Performance l'emporte sur les réglages du thème).
        self.apply_ui_modes();
    }

    pub(crate) fn set_motion(&self, m: i32) {
        self.update_appearance(|a| {
            a.motion = m.clamp(0, 2) as u8;
            a.animations = a.motion < 2;
        });
    }

    pub(crate) fn set_text_scale(&self, v: f32) {
        self.update_appearance(|a| a.text_scale = v.clamp(0.9, 1.3));
    }

    pub(crate) fn set_performance_auto(&self, on: bool) {
        self.update_appearance(|a| a.performance_auto = on);
    }

    pub(crate) fn refresh_appearance_ui(&self) {
        let themes = self.all_themes();
        self.refresh_appearance_ui_with(&themes);
    }

    fn refresh_appearance_ui_with(&self, themes: &[ThemeDef]) {
        let Some(app) = self.app() else { return };
        let a = self.settings.borrow().appearance.clone();
        let ui = app.global::<ThemeUi>();
        let base_accent = parse_hex("#2fd4c5").unwrap_or_default();
        let rows: Vec<ThemeRow> = themes
            .iter()
            .map(|t| ThemeRow {
                id: t.id.clone().into(),
                name: t.name.clone().into(),
                bg: parse_hex(&t.colors.bg).unwrap_or_else(|| parse_hex("#0a0c10").unwrap_or_default()),
                surface: parse_hex(&t.colors.surface).unwrap_or_else(|| parse_hex("#13171f").unwrap_or_default()),
                accent: parse_hex(&t.colors.accent).unwrap_or(base_accent),
                source: t.source.clone().into(),
            })
            .collect();
        ui.set_selected(themes.iter().position(|t| t.id == a.theme).unwrap_or(0) as i32);
        ui.set_themes(ModelRc::new(VecModel::from(rows)));
        ui.set_accents(ModelRc::new(VecModel::from(ACCENTS.iter().filter_map(|(_, h)| parse_hex(h)).collect::<Vec<_>>())));
        ui.set_accent_hexes(ModelRc::new(VecModel::from(ACCENTS.iter().map(|(_, h)| SharedString::from(*h)).collect::<Vec<_>>())));
        ui.set_accent(a.accent.clone().into());
        ui.set_density(i32::from(a.density.min(1)));
        ui.set_animations(a.animations);
        ui.set_motion(i32::from(a.motion()));
        ui.set_text_scale(a.text_scale);
        ui.set_performance_auto(a.performance_auto);
        ui.set_background(a.background.as_ref().map(|p| p.display().to_string()).unwrap_or_default().into());
        ui.set_animated_background(a.animated_background);
        ui.set_background_opacity(a.background_opacity);
        let nav: Vec<ToggleRow> = NAV_ITEMS
            .iter()
            .map(|(k, l)| ToggleRow { key: (*k).into(), label: (*l).into(), visible: !a.hidden_nav.iter().any(|h| h == k) })
            .collect();
        ui.set_nav_items(ModelRc::new(VecModel::from(nav)));
        let sections: Vec<ToggleRow> = a
            .widgets()
            .iter()
            .map(|w| ToggleRow {
                key: w.kind.to_string().into(),
                label: crate::ctl_home::WIDGET_TITLES.get(usize::from(w.kind)).copied().unwrap_or("?").into(),
                visible: w.enabled,
            })
            .collect();
        ui.set_sections(ModelRc::new(VecModel::from(sections)));
    }

    fn update_appearance(&self, f: impl FnOnce(&mut Appearance)) {
        f(&mut self.settings.borrow_mut().appearance);
        self.save_settings();
        self.apply_appearance();
    }

    fn theme_status(&self, msg: impl Into<SharedString>) {
        if let Some(app) = self.app() {
            app.global::<ThemeUi>().set_status(msg.into());
        }
    }

    pub(crate) fn select_theme(&self, index: usize) {
        let themes = self.all_themes();
        let Some(t) = themes.get(index) else { return };
        let id = t.id.clone();
        // Un thème choisi impose son propre accent : l'accent personnalisé est oublié.
        self.update_appearance(|a| {
            a.theme = id;
            a.accent.clear();
        });
    }

    pub(crate) fn set_accent(&self, value: &str) {
        let value = value.trim();
        if !value.is_empty() && parse_hex(value).is_none_or(|c| c.alpha() != 255) {
            self.theme_status(format!("« {value} » n'est pas une couleur #RRGGBB"));
            return;
        }
        let v = value.to_lowercase();
        self.theme_status("");
        self.update_appearance(|a| a.accent = v);
    }

    pub(crate) fn set_density(&self, d: i32) {
        self.update_appearance(|a| a.density = d.clamp(0, 1) as u8);
    }

    pub(crate) fn set_animations(&self, on: bool) {
        self.update_appearance(|a| a.animations = on);
    }

    pub(crate) fn set_animated_background(&self, on: bool) {
        self.update_appearance(|a| a.animated_background = on);
    }

    pub(crate) fn set_background_opacity(&self, v: f32) {
        self.update_appearance(|a| a.background_opacity = v.clamp(0.02, 0.6));
    }

    /// Copie l'image dans les données de Halcyon (incluse dans les exports de configuration).
    pub(crate) fn set_background(&self, path: &str) {
        let path = path.trim();
        if path.is_empty() {
            self.update_appearance(|a| a.background = None);
            self.theme_status("Fond retiré");
            return;
        }
        let src = Path::new(path);
        if let Err(e) = slint::Image::load_from_path(src) {
            self.theme_status(format!("Image illisible : {e:?}"));
            return;
        }
        let dir = self.themes_dir().join("backgrounds");
        let ext = src.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_else(|| "png".into());
        let dest = dir.join(format!("fond-{}.{ext}", crate::ctl_backup::stamp(halcyon_storage::now_unix())));
        let result = std::fs::create_dir_all(&dir).and_then(|_| std::fs::copy(src, &dest));
        match result {
            Ok(_) => {
                // Les anciens fonds copiés ne servent plus.
                let old = self.settings.borrow().appearance.background.clone();
                self.update_appearance(|a| a.background = Some(dest.clone()));
                if let Some(old) = old.filter(|o| o.starts_with(&dir) && *o != dest) {
                    let _ = std::fs::remove_file(old);
                }
                self.theme_status("Fond appliqué (copie conservée dans les données de Halcyon)");
            }
            Err(e) => self.theme_status(format!("Copie de l'image impossible : {e}")),
        }
    }

    pub(crate) fn toggle_nav(&self, key: &str) {
        let key = key.to_string();
        self.update_appearance(|a| {
            if let Some(i) = a.hidden_nav.iter().position(|h| *h == key) {
                a.hidden_nav.remove(i);
            } else {
                a.hidden_nav.push(key);
            }
        });
    }

    /// Liste des widgets de la page Apparence : mêmes opérations que l'édition de l'accueil.
    pub(crate) fn toggle_section(&self, index: usize) {
        let w = self.settings.borrow().appearance.widgets().get(index).copied();
        if let Some(w) = w {
            self.widget_toggle(w.kind, !w.enabled);
        }
    }

    pub(crate) fn move_section(&self, index: usize, delta: i32) {
        {
            let mut s = self.settings.borrow_mut();
            let mut w = s.appearance.widgets();
            let j = index as i64 + i64::from(delta);
            if index < w.len() && j >= 0 && (j as usize) < w.len() {
                w.swap(index, j as usize);
            }
            s.appearance.widgets = w;
        }
        self.save_settings();
        self.refresh_home();
        self.refresh_appearance_ui();
    }

    pub(crate) fn import_theme(&self, path: &str) {
        let src = Path::new(path.trim());
        match read_theme_file(src) {
            Ok(t) => {
                let dir = self.themes_dir();
                let dest = dir.join(format!("{}.json", t.id));
                let existed = dest.exists();
                let result = std::fs::create_dir_all(&dir).and_then(|_| {
                    let json = serde_json::to_string_pretty(&t).unwrap_or_default();
                    // Écriture atomique : un thème remplacé n'est jamais laissé à moitié écrit.
                    let tmp = dest.with_extension("json.tmp");
                    std::fs::write(&tmp, json)?;
                    std::fs::rename(&tmp, &dest)
                });
                match result {
                    Ok(()) => {
                        let id = t.id.clone();
                        self.update_appearance(|a| {
                            a.theme = id;
                            a.accent.clear();
                        });
                        self.theme_status(format!("Thème « {} » {}", t.name, if existed { "mis à jour" } else { "importé" }));
                    }
                    Err(e) => self.theme_status(format!("Import impossible : {e}")),
                }
            }
            Err(e) => self.theme_status(format!("Thème refusé : {e}")),
        }
    }

    /// Exporte le thème actuel (avec l'accent choisi) dans `themes/exports`.
    pub(crate) fn export_theme(&self) {
        let Some(app) = self.app() else { return };
        let theme = app.global::<Theme>();
        let a = self.settings.borrow().appearance.clone();
        let current = self.all_themes().into_iter().find(|t| t.id == a.theme);
        let name = current.as_ref().map(|t| t.name.clone()).unwrap_or_else(|| "Halcyon".into());
        let stamp = crate::ctl_backup::stamp(halcyon_storage::now_unix());
        let def = ThemeDef {
            id: format!("perso-{}", stamp.to_lowercase()),
            name: format!("{name} (personnalisé)"),
            author: String::new(),
            source: String::new(),
            colors: Palette {
                bg: hex(theme.get_bg()),
                sidebar: hex(theme.get_sidebar()),
                surface: hex(theme.get_surface()),
                surface_hover: hex(theme.get_surface_hover()),
                surface_active: hex(theme.get_surface_active()),
                border: hex(theme.get_border()),
                border_strong: hex(theme.get_border_strong()),
                text: hex(theme.get_text()),
                text_muted: hex(theme.get_text_muted()),
                text_dim: hex(theme.get_text_dim()),
                accent: hex(theme.get_accent()),
                on_accent: hex(theme.get_on_accent()),
                warm: hex(theme.get_warm()),
                danger: hex(theme.get_danger()),
                success: hex(theme.get_success()),
                bp_bg: hex(theme.get_bp_bg()),
            },
        };
        let dir = self.themes_dir().join("exports");
        let dest = dir.join(format!("theme-{stamp}.json"));
        let result = std::fs::create_dir_all(&dir)
            .and_then(|_| std::fs::write(&dest, serde_json::to_string_pretty(&def).unwrap_or_default()));
        match result {
            Ok(()) => self.theme_status(format!("Thème exporté : {}", dest.display())),
            Err(e) => self.theme_status(format!("Export impossible : {e}")),
        }
    }

    pub(crate) fn reset_appearance(self: &Rc<Self>) {
        self.confirm(
            "Rétablir l'apparence d'origine ?",
            "Thème, accent, densité, animations, fond, barre latérale et accueil reprennent leurs valeurs par défaut. Les thèmes importés restent disponibles.",
            "Rétablir",
            |c| {
                c.update_appearance(|a| *a = Appearance::default());
                c.theme_status("Apparence d'origine rétablie");
                c.refresh_home();
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_colors() {
        assert_eq!(parse_hex("#2fd4c5"), Some(Color::from_rgb_u8(0x2f, 0xd4, 0xc5)));
        assert_eq!(parse_hex("#ffffff12").map(|c| c.alpha()), Some(0x12));
        assert!(parse_hex("2fd4c5").is_none());
        assert!(parse_hex("#zzzzzz").is_none());
        assert_eq!(hex(Color::from_rgb_u8(1, 2, 3)), "#010203");
    }

    #[test]
    fn theme_validation() {
        let ok = serde_json::json!({"id": "nuit-polaire", "name": "Nuit polaire", "colors": {"bg": "#000000", "accent": "#88c0d0"}});
        assert!(theme_from_value(&ok).is_ok());
        let builtin = serde_json::json!({"id": "halcyon", "name": "X"});
        assert!(theme_from_value(&builtin).is_err());
        let bad_color = serde_json::json!({"id": "x", "name": "X", "colors": {"bg": "rouge"}});
        assert!(theme_from_value(&bad_color).is_err());
        let bad_id = serde_json::json!({"id": "../x", "name": "X"});
        assert!(theme_from_value(&bad_id).is_err());
    }

    #[test]
    fn on_accent_contrast() {
        assert_eq!(on_accent_for(Color::from_rgb_u8(20, 20, 60)), Color::from_rgb_u8(255, 255, 255));
        assert!(on_accent_for(Color::from_rgb_u8(0xf2, 0xc9, 0x4c)).red() < 40);
    }
}
