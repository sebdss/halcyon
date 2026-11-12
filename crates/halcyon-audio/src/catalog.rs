//! Catalogue des sons de Halcyon : chaque effet a une fonction, une catégorie de volume,
//! une priorité et un délai minimal entre deux lectures.
//!
//! Les fichiers sont produits par `tools/sound/make_sounds.py` (synthèse originale) et
//! intégrés au binaire : aucun fichier à installer, aucun chargement depuis le disque.

/// Catégorie de volume (réglable séparément).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    /// Sélection, sections, panneaux, menus, retour.
    Navigation,
    /// Validation, favoris, collections, fiche d'un jeu, réglages.
    Interaction,
    /// Réussite, avertissement, erreur, téléchargements, installations, mises à jour.
    Notification,
    /// Lancement d'un jeu, grandes réussites.
    Important,
}

impl Category {
    pub const ALL: [Category; 4] = [Self::Navigation, Self::Interaction, Self::Notification, Self::Important];

    pub fn index(self) -> usize {
        match self {
            Self::Navigation => 0,
            Self::Interaction => 1,
            Self::Notification => 2,
            Self::Important => 3,
        }
    }

    /// Voix simultanées au plus dans la catégorie (évite les empilements).
    pub fn max_voices(self) -> usize {
        match self {
            Self::Navigation => 2,
            Self::Interaction => 2,
            Self::Notification => 2,
            Self::Important => 1,
        }
    }
}

macro_rules! cues {
    ($( $variant:ident => $id:literal, $file:literal, $cat:ident, $prio:literal, $cooldown:literal, $doc:literal; )*) => {
        /// Effet sonore. Identifiant textuel : `Cue::id()` / `Cue::from_id()`.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum Cue {
            $( #[doc = $doc] $variant, )*
        }

        impl Cue {
            pub const ALL: &'static [Cue] = &[$(Cue::$variant),*];

            pub fn id(self) -> &'static str {
                match self { $(Cue::$variant => $id,)* }
            }

            pub fn from_id(id: &str) -> Option<Cue> {
                match id { $($id => Some(Cue::$variant),)* _ => None }
            }

            pub fn category(self) -> Category {
                match self { $(Cue::$variant => Category::$cat,)* }
            }

            /// Priorité (0 à 9) : en cas de saturation, la voix la moins prioritaire s'arrête.
            pub fn priority(self) -> u8 {
                match self { $(Cue::$variant => $prio,)* }
            }

            /// Délai minimal entre deux lectures du même son (ms).
            pub fn cooldown_ms(self) -> u64 {
                match self { $(Cue::$variant => $cooldown,)* }
            }

            /// Fonction du son, pour la documentation et les réglages.
            pub fn description(self) -> &'static str {
                match self { $(Cue::$variant => $doc,)* }
            }

            pub(crate) fn wav(self) -> &'static [u8] {
                match self { $(Cue::$variant => include_bytes!(concat!("../sounds/", $file, ".wav")),)* }
            }
        }
    };
}

cues! {
    Select => "ui.select", "ui/select", Navigation, 1, 45, "Sélection d'un élément";
    Hover => "ui.hover", "ui/hover", Navigation, 0, 70, "Survol d'un élément (désactivable)";
    Section => "ui.section", "ui/section", Navigation, 2, 120, "Changement de section";
    PanelOpen => "ui.panel-open", "ui/panel-open", Navigation, 3, 150, "Ouverture d'un panneau";
    PanelClose => "ui.panel-close", "ui/panel-close", Navigation, 3, 150, "Fermeture d'un panneau";
    MenuOpen => "ui.menu-open", "ui/menu-open", Navigation, 3, 120, "Ouverture d'un menu";
    MenuClose => "ui.menu-close", "ui/menu-close", Navigation, 2, 120, "Fermeture d'un menu";
    Back => "ui.back", "ui/back", Navigation, 2, 120, "Retour en arrière";
    Confirm => "ui.confirm", "ui/confirm", Interaction, 5, 150, "Validation d'une action";
    GameSelect => "game.select", "game/select", Navigation, 1, 45, "Sélection d'un jeu";
    GameOpen => "game.open", "game/open", Interaction, 4, 250, "Ouverture de la fiche d'un jeu";
    Favorite => "game.favorite", "game/favorite", Interaction, 5, 250, "Ajout aux favoris";
    Collection => "game.collection", "game/collection", Interaction, 5, 250, "Ajout ou création d'une collection";
    Launch => "game.launch", "game/launch", Important, 9, 1500, "Début du lancement d'un jeu";
    Launched => "game.launched", "game/launched", Interaction, 7, 1500, "Jeu démarré";
    Success => "notify.success", "notify/success", Notification, 6, 400, "Action réussie";
    Warning => "notify.warning", "notify/warning", Notification, 7, 400, "Avertissement";
    Error => "notify.error", "notify/error", Notification, 8, 400, "Erreur";
    Download => "notify.download", "notify/download", Notification, 6, 800, "Téléchargement terminé";
    Install => "notify.install", "notify/install", Notification, 6, 800, "Installation terminée";
    Update => "notify.update", "notify/update", Notification, 6, 800, "Mise à jour terminée";
    Major => "notify.major", "notify/major", Important, 8, 1500, "Opération importante terminée";
    CommandCenter => "ui.command-center", "ui/command-center", Interaction, 4, 200, "Ouverture du centre de commandes";
    Mode => "ui.mode", "ui/mode", Interaction, 5, 300, "Changement de mode";
    ToggleOn => "ui.toggle-on", "ui/toggle-on", Interaction, 3, 80, "Activation d'une fonction";
    ToggleOff => "ui.toggle-off", "ui/toggle-off", Interaction, 3, 80, "Désactivation d'une fonction";
    Window => "ui.window", "ui/window", Interaction, 4, 250, "Ouverture d'une fenêtre importante";
    Setting => "ui.setting", "ui/setting", Interaction, 2, 80, "Confirmation d'un réglage";
}

impl Cue {
    /// Sons de survol : jouables seulement si l'option est active.
    pub fn is_hover(self) -> bool {
        self == Cue::Hover
    }

    /// Un nouveau déclenchement redémarre le son au lieu de l'empiler.
    pub fn restarts(self) -> bool {
        self.category() == Category::Navigation
    }

    /// Son représentatif d'une catégorie (prévisualisation dans les réglages).
    pub fn sample_for(cat: Category) -> Cue {
        match cat {
            Category::Navigation => Cue::Section,
            Category::Interaction => Cue::Confirm,
            Category::Notification => Cue::Success,
            Category::Important => Cue::Launch,
        }
    }
}
