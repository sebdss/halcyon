//! Recherche universelle (Ctrl+K) : jeux, pages, réglages, collections et commandes ; et
//! recherche dans les paramètres.

use crate::controller::Controller;
use crate::{AppUi, CommandUi, HomeUi, LibraryUi, Nav, Page, QuickRow, SettingsUi};
use halcyon_core::util::search_key;
use slint::{ComponentHandle, ModelRc, VecModel};
use std::rc::Rc;

/// Sections de la page Paramètres : (titre, mots-clés). L'ordre est celui de la page.
pub const SETTINGS_SECTIONS: [(&str, &str); 11] = [
    ("Plateformes", "steam epic sources analyser dossier manifestes detection plateformes"),
    ("Dossiers de jeux", "dossiers jeux standalone ajouter parcourir analyse"),
    (
        "Comptes et succès",
        "comptes steam web api cle steamid retroachievements nexus mods steamgriddb visuels jaquettes succes notifications synchroniser surveiller linux wine proton umu windows runner",
    ),
    ("Comportement", "demarrage analyser reduire pendant jeu big picture metadonnees en ligne visuels telecharger jaquettes"),
    ("Connexion", "hors ligne connexion reseau internet"),
    ("Système", "systeme demarrer session zone notification tray fermer reduire fenetre compagnon"),
    ("Raccourcis clavier", "raccourcis clavier touches hotkey global ctrl"),
    ("Configuration", "exporter restaurer configuration archive sauvegarde backup zip"),
    ("Mises à jour", "mises a jour halcyon version github depot verifier telecharger"),
    ("Stockage et diagnostic", "stockage cache miniatures donnees journaux logs integrite diagnostic verification"),
    ("Son", "son sons audio effets sonores volume silencieux survol musique bruitage"),
];

/// Commandes du lancement rapide : (libellé, mots-clés, code).
const COMMANDS: [(&str, &str, &str); 14] = [
    ("Analyser les sources", "scan rescan detecter jeux", "scan"),
    ("Activer / désactiver le mode Performance", "performance jeu fps", "perf"),
    ("Mode d'affichage standard", "standard normal interface", "mode:0"),
    ("Mode minimaliste", "minimal compact simple", "mode:1"),
    ("Mode cinéma", "cinema plein ecran immersif", "mode:2"),
    ("Big Picture", "tv salon manette plein ecran", "bp"),
    ("Mode compact (fenêtre réduite)", "mini petite fenetre", "mini"),
    ("Centre de commandes", "taches notifications telechargements erreurs", "center"),
    ("Personnaliser l'accueil", "widgets accueil disposition", "home-edit"),
    ("Ajouter un jeu", "manuel executable", "add"),
    ("Bibliothèque en grille", "vue grille jaquettes", "view:0"),
    ("Bibliothèque en liste", "vue liste", "view:1"),
    ("Bibliothèque en carrousel", "vue carrousel", "view:2"),
    ("Bibliothèque immersive", "vue immersive", "view:3"),
];

/// Les mots de la requête sont-ils tous présents (sans accents ni casse) ?
pub fn words_match(query: &str, haystack: &str) -> bool {
    let key = search_key(query);
    let hay = search_key(haystack);
    let mut any = false;
    for w in key.split(' ').filter(|w| !w.is_empty()) {
        any = true;
        if !hay.contains(w) {
            return false;
        }
    }
    any
}

/// Sections de paramètres affichées pour une recherche et un filtre de section.
pub fn settings_shown(query: &str, section: i32) -> Vec<bool> {
    SETTINGS_SECTIONS
        .iter()
        .enumerate()
        .map(|(i, (title, words))| {
            if query.trim().is_empty() {
                section < 0 || section == i as i32
            } else {
                words_match(query, &format!("{title} {words}"))
            }
        })
        .collect()
}

impl Controller {
    pub(crate) fn settings_search(&self, q: &str) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<SettingsUi>();
        let shown = settings_shown(q, ui.get_section());
        ui.set_match_count(shown.iter().filter(|v| **v).count() as i32);
        ui.set_shown(ModelRc::new(VecModel::from(shown)));
    }

    /// Lignes supplémentaires du lancement rapide (réglages, collections, commandes).
    pub(crate) fn palette_rows(&self, q: &str) -> Vec<QuickRow> {
        let mut rows = Vec::new();
        if q.trim().is_empty() {
            return rows;
        }
        for (i, (title, words)) in SETTINGS_SECTIONS.iter().enumerate() {
            if words_match(q, &format!("{title} {words}")) {
                rows.push(QuickRow { kind: 3, id: i as i32, title: format!("Paramètres › {title}").into(), subtitle: "Réglage".into(), card: Default::default() });
            }
        }
        for (i, c) in self.collections.borrow().iter().enumerate() {
            if words_match(q, &format!("collection {}", c.name)) {
                rows.push(QuickRow { kind: 2, id: i as i32 + 1, title: c.name.as_str().into(), subtitle: format!("Collection · {} jeux", c.count).into(), card: Default::default() });
            }
        }
        for (i, (label, words, _)) in COMMANDS.iter().enumerate() {
            if words_match(q, &format!("{label} {words}")) {
                rows.push(QuickRow { kind: 4, id: i as i32, title: (*label).into(), subtitle: "Commande".into(), card: Default::default() });
            }
        }
        rows
    }

    /// Exécute une ligne de type réglage (3), collection (2) ou commande (4).
    pub(crate) fn palette_activate(self: &Rc<Self>, kind: i32, id: i32) {
        let Some(app) = self.app() else { return };
        match kind {
            3 => {
                let s = app.global::<SettingsUi>();
                s.set_search("".into());
                s.set_section(id);
                self.settings_search("");
                app.global::<Nav>().invoke_go(Page::Settings);
            }
            2 => {
                let l = app.global::<LibraryUi>();
                l.set_collection(id);
                app.global::<Nav>().invoke_go(Page::Library);
                l.invoke_query_changed();
            }
            4 => {
                let Some((_, _, code)) = COMMANDS.get(id.max(0) as usize) else { return };
                match *code {
                    "scan" => self.start_scan(),
                    "perf" => {
                        let on = !self.performance_active();
                        self.set_performance(on);
                    }
                    "bp" => self.enter_big_picture(),
                    "mini" => self.show_mini(),
                    "center" => {
                        app.global::<CommandUi>().set_open(true);
                        self.on_command_center_opened();
                    }
                    "home-edit" => {
                        app.global::<Nav>().invoke_go(Page::Home);
                        app.global::<HomeUi>().set_editing(true);
                    }
                    "add" => app.global::<AppUi>().set_add_dialog_open(true),
                    c if c.starts_with("mode:") => self.set_ui_mode(c[5..].parse().unwrap_or(0)),
                    c if c.starts_with("view:") => {
                        app.global::<LibraryUi>().set_view_mode(c[5..].parse().unwrap_or(0));
                        app.global::<Nav>().invoke_go(Page::Library);
                        self.on_view_changed();
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_search_matches_keywords_without_accents() {
        let shown = settings_shown("retroachievements", -1);
        assert!(shown[2]);
        assert_eq!(shown.iter().filter(|v| **v).count(), 1);
        assert!(settings_shown("Hors LIGNE", -1)[4]);
        assert!(settings_shown("succès", -1)[2]);
        assert!(settings_shown("", -1).iter().all(|v| *v));
        let only = settings_shown("", 6);
        assert!(only[6] && only.iter().filter(|v| **v).count() == 1);
        assert!(settings_shown("zzz", -1).iter().all(|v| !*v));
    }

    #[test]
    fn words_must_all_match() {
        assert!(words_match("mode perf", "Activer le mode Performance"));
        assert!(!words_match("mode xyz", "Activer le mode Performance"));
        assert!(!words_match("   ", "x"));
    }
}
