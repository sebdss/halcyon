//! Recherche universelle (Ctrl+K) : jeux, pages, réglages, collections et commandes ; et
//! recherche dans les paramètres.

use crate::controller::Controller;
use crate::{AppUi, CommandUi, HomeUi, LibraryUi, Nav, Page, QuickRow, SettingsUi};
use halcyon_core::util::search_key;
use slint::{ComponentHandle, ModelRc, VecModel};
use std::rc::Rc;

/// Sections de la page Paramètres : (titre, mots-clés). L'ordre est celui de la page.
pub const SETTINGS_SECTIONS: [(&str, &str); 12] = [
    (tr_noop!("Plateformes"), trc_noop!("keywords", "steam epic sources analyser dossier manifestes detection plateformes")),
    (tr_noop!("Dossiers de jeux"), trc_noop!("keywords", "dossiers jeux standalone ajouter parcourir analyse")),
    (
        tr_noop!("Comptes et succès"),
        trc_noop!("keywords", "comptes steam web api cle steamid retroachievements nexus mods steamgriddb visuels jaquettes succes notifications synchroniser surveiller linux wine proton umu windows runner"),
    ),
    (tr_noop!("Comportement"), trc_noop!("keywords", "demarrage analyser reduire pendant jeu big picture metadonnees en ligne visuels telecharger jaquettes")),
    (tr_noop!("Connexion"), trc_noop!("keywords", "hors ligne connexion reseau internet")),
    (tr_noop!("Système"), trc_noop!("keywords", "systeme demarrer session zone notification tray fermer reduire fenetre compagnon")),
    (tr_noop!("Raccourcis clavier"), trc_noop!("keywords", "raccourcis clavier touches hotkey global ctrl")),
    (tr_noop!("Configuration"), trc_noop!("keywords", "exporter restaurer configuration archive sauvegarde backup zip")),
    (tr_noop!("Mises à jour"), trc_noop!("keywords", "mises a jour halcyon version github depot verifier telecharger")),
    (tr_noop!("Stockage et diagnostic"), trc_noop!("keywords", "stockage cache miniatures donnees journaux logs integrite diagnostic verification")),
    (tr_noop!("Son"), trc_noop!("keywords", "son sons audio effets sonores volume silencieux survol musique bruitage")),
    (tr_noop!("Langue"), trc_noop!("keywords", "langue langues traduction language english deutsch espanol italiano portugues francais")),
];

/// Commandes du lancement rapide : (libellé, mots-clés, code).
const COMMANDS: [(&str, &str, &str); 16] = [
    (tr_noop!("Analyser les sources"), trc_noop!("keywords", "scan rescan detecter jeux"), "scan"),
    (tr_noop!("Activer / désactiver le mode Performance"), trc_noop!("keywords", "performance jeu fps"), "perf"),
    (tr_noop!("Mode d'affichage standard"), trc_noop!("keywords", "standard normal interface"), "mode:0"),
    (tr_noop!("Mode minimaliste"), trc_noop!("keywords", "minimal compact simple"), "mode:1"),
    (tr_noop!("Mode cinéma"), trc_noop!("keywords", "cinema plein ecran immersif"), "mode:2"),
    (tr_noop!("Big Picture"), trc_noop!("keywords", "tv salon manette plein ecran"), "bp"),
    (tr_noop!("Mode compact (fenêtre réduite)"), trc_noop!("keywords", "mini petite fenetre"), "mini"),
    (tr_noop!("Centre de commandes"), trc_noop!("keywords", "taches notifications telechargements erreurs"), "center"),
    (tr_noop!("Personnaliser l'accueil"), trc_noop!("keywords", "widgets accueil disposition"), "home-edit"),
    (tr_noop!("Ajouter un jeu"), trc_noop!("keywords", "manuel executable"), "add"),
    (tr_noop!("Gérer les collections"), trc_noop!("keywords", "collections series etagere ranger ordre couleur"), "collections"),
    (tr_noop!("Nouvelle collection…"), trc_noop!("keywords", "creer collection serie cocher"), "new-collection"),
    (tr_noop!("Bibliothèque en grille"), trc_noop!("keywords", "vue grille jaquettes"), "view:0"),
    (tr_noop!("Bibliothèque en liste"), trc_noop!("keywords", "vue liste"), "view:1"),
    (tr_noop!("Bibliothèque en carrousel"), trc_noop!("keywords", "vue carrousel"), "view:2"),
    (tr_noop!("Bibliothèque immersive"), trc_noop!("keywords", "vue immersive"), "view:3"),
];

/// Texte cherché pour une entrée : libellé et mots-clés dans la langue de l'interface, puis
/// en français (les deux restent valables).
pub fn search_text(label: &str, words: &str) -> String {
    format!("{} {} {label} {words}", halcyon_i18n::t(label), halcyon_i18n::tc("keywords", words))
}

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
                words_match(query, &search_text(title, words))
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
            if words_match(q, &search_text(title, words)) {
                rows.push(QuickRow { kind: 3, id: i as i32, title: tr!("Paramètres › {title}", title = halcyon_i18n::t(title)).into(), subtitle: tr!("Réglage").into(), card: Default::default() });
            }
        }
        for (i, c) in self.collections.borrow().iter().enumerate() {
            if words_match(q, &format!("collection {}", c.name)) {
                rows.push(QuickRow { kind: 2, id: i as i32 + 1, title: c.name.as_str().into(), subtitle: trn!(c.count, "Collection · {n} jeu", "Collection · {n} jeux").into(), card: Default::default() });
            }
        }
        for (i, (label, words, _)) in COMMANDS.iter().enumerate() {
            if words_match(q, &search_text(label, words)) {
                rows.push(QuickRow { kind: 4, id: i as i32, title: halcyon_i18n::t(label).into(), subtitle: tr!("Commande").into(), card: Default::default() });
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
                    "collections" => self.open_collections_page(None),
                    "new-collection" => self.open_create_collection(""),
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
