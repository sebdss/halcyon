//! Canaux, plateformes, états et choix de la version à proposer.

use crate::version::Version;
use serde::{Deserialize, Serialize};

/// Canal de publication.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    /// Versions finales uniquement.
    Stable,
    /// Prépublications et versions finales plus récentes.
    Beta,
}

impl Channel {
    pub fn parse(s: &str) -> Option<Channel> {
        match s.trim().to_ascii_lowercase().as_str() {
            "stable" => Some(Channel::Stable),
            "beta" => Some(Channel::Beta),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Channel::Stable => "stable",
            Channel::Beta => "beta",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Channel::Stable => "Stable",
            Channel::Beta => "Beta",
        }
    }

    /// Canal proposé par défaut à une installation qui n'en a jamais choisi : Beta pour une
    /// prépublication (sinon elle ne recevrait jamais la suivante), Stable sinon.
    pub fn default_for(current: &Version) -> Channel {
        if current.is_prerelease() { Channel::Beta } else { Channel::Stable }
    }

    /// Une version publiée est-elle proposable sur ce canal ? `flagged_prerelease` : la
    /// publication GitHub est marquée « pre-release ».
    pub fn accepts(self, version: &Version, flagged_prerelease: bool) -> bool {
        match self {
            Channel::Stable => !version.is_prerelease() && !flagged_prerelease,
            Channel::Beta => true,
        }
    }

    /// Canal auquel appartient une version publiée.
    pub fn of(version: &Version, flagged_prerelease: bool) -> Channel {
        if version.is_prerelease() || flagged_prerelease { Channel::Beta } else { Channel::Stable }
    }
}

/// Plateformes prises en charge par HUS.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Platform {
    #[serde(rename = "windows-x86_64")]
    WindowsX64,
    #[serde(rename = "linux-x86_64")]
    LinuxX64,
}

/// Format de paquet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PackageFormat {
    /// Archive ZIP installée par l'updater (Windows).
    Zip,
    /// AppImage autonome (Linux).
    Appimage,
}

impl Platform {
    /// Plateforme de ce binaire, si HUS la prend en charge.
    pub fn current() -> Option<Platform> {
        match (std::env::consts::OS, std::env::consts::ARCH) {
            ("windows", "x86_64") => Some(Platform::WindowsX64),
            ("linux", "x86_64") => Some(Platform::LinuxX64),
            _ => None,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Platform::WindowsX64 => "windows-x86_64",
            Platform::LinuxX64 => "linux-x86_64",
        }
    }

    pub fn format(self) -> PackageFormat {
        match self {
            Platform::WindowsX64 => PackageFormat::Zip,
            Platform::LinuxX64 => PackageFormat::Appimage,
        }
    }

    /// Nom officiel du paquet : `Halcyon-{VERSION}-Windows-x64.zip`,
    /// `Halcyon-{VERSION}-x86_64.AppImage`.
    pub fn package_name(self, version: &Version) -> String {
        match self {
            Platform::WindowsX64 => format!("Halcyon-{version}-Windows-x64.zip"),
            Platform::LinuxX64 => format!("Halcyon-{version}-x86_64.AppImage"),
        }
    }
}

/// États du système de mise à jour (affichés tels quels et journalisés).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum State {
    Idle,
    Checking,
    UpToDate,
    Available,
    Downloading,
    Verifying,
    /// Paquet vérifié, prêt à être installé (maintenant ou au prochain démarrage).
    Staging,
    Installing,
    /// Nouvelle version en place, en attente de la preuve qu'elle démarre.
    HealthCheck,
    Completed,
    Failed,
    /// Reprise d'une installation interrompue.
    Recovery,
    RolledBack,
}

impl State {
    pub fn code(self) -> i32 {
        self as i32
    }

    pub fn label(self) -> &'static str {
        match self {
            State::Idle => "En attente",
            State::Checking => "Recherche de mises à jour…",
            State::UpToDate => "À jour",
            State::Available => "Mise à jour disponible",
            State::Downloading => "Téléchargement…",
            State::Verifying => "Vérification…",
            State::Staging => "Prête à être installée",
            State::Installing => "Installation…",
            State::HealthCheck => "Contrôle du démarrage…",
            State::Completed => "Mise à jour installée",
            State::Failed => "Échec",
            State::Recovery => "Récupération…",
            State::RolledBack => "Version précédente restaurée",
        }
    }
}

/// Une publication candidate (déjà filtrée : ni brouillon, ni version illisible).
pub trait Candidate {
    fn version(&self) -> &Version;
    fn flagged_prerelease(&self) -> bool;
}

/// Publications proposables sur `channel`, plus récentes que `current`, de la plus récente à
/// la plus ancienne. Jamais de retour vers une version plus ancienne : passer de Beta à
/// Stable ne fait pas redescendre une 1.1.0-beta.2 vers la 1.0.0, il faut attendre une
/// version stable plus récente.
pub fn candidates<'a, C: Candidate>(releases: &'a [C], channel: Channel, current: &Version) -> Vec<&'a C> {
    let mut list: Vec<&C> = releases.iter().filter(|r| channel.accepts(r.version(), r.flagged_prerelease()) && r.version() > current).collect();
    list.sort_by(|a, b| b.version().cmp(a.version()));
    list.dedup_by(|a, b| a.version() == b.version());
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    struct R(Version, bool);
    impl Candidate for R {
        fn version(&self) -> &Version {
            &self.0
        }
        fn flagged_prerelease(&self) -> bool {
            self.1
        }
    }
    fn r(s: &str) -> R {
        let v: Version = s.parse().unwrap();
        let pre = v.is_prerelease();
        R(v, pre)
    }
    fn first(list: &[R], ch: Channel, cur: &str) -> Option<String> {
        candidates(list, ch, &cur.parse().unwrap()).first().map(|c| c.0.to_string())
    }

    #[test]
    fn channels_select_expected_versions() {
        let list = vec![r("0.5.0"), r("0.5.1"), r("1.0.0"), r("1.1.0-beta.1"), r("1.1.0-beta.2")];
        // Nouvelle version détectée / aucune mise à jour.
        assert_eq!(first(&list, Channel::Stable, "0.5.1").as_deref(), Some("1.0.0"));
        assert_eq!(first(&list, Channel::Stable, "1.0.0"), None, "jamais de Beta sur Stable");
        // Stable vers Beta : la dernière prépublication devient proposable.
        assert_eq!(first(&list, Channel::Beta, "1.0.0").as_deref(), Some("1.1.0-beta.2"));
        // Beta vers Stable : pas de retour en arrière vers la 1.0.0.
        assert_eq!(first(&list, Channel::Stable, "1.1.0-beta.2"), None);
        // … et la version stable plus récente sera proposée dès sa sortie.
        let mut later = list;
        later.push(r("1.1.0"));
        assert_eq!(first(&later, Channel::Stable, "1.1.0-beta.2").as_deref(), Some("1.1.0"));
        // Beta reçoit aussi les versions stables plus récentes.
        assert_eq!(first(&later, Channel::Beta, "1.1.0-beta.2").as_deref(), Some("1.1.0"));
        // Une version marquée « pre-release » sur GitHub n'est jamais Stable, même sans suffixe.
        let flagged = vec![R("2.0.0".parse().unwrap(), true)];
        assert_eq!(first(&flagged, Channel::Stable, "1.0.0"), None);
        assert_eq!(first(&flagged, Channel::Beta, "1.0.0").as_deref(), Some("2.0.0"));
    }

    #[test]
    fn package_names_and_defaults() {
        let v: Version = "1.1.0-beta.1".parse().unwrap();
        assert_eq!(Platform::WindowsX64.package_name(&v), "Halcyon-1.1.0-beta.1-Windows-x64.zip");
        assert_eq!(Platform::LinuxX64.package_name(&v), "Halcyon-1.1.0-beta.1-x86_64.AppImage");
        assert_eq!(Channel::default_for(&v), Channel::Beta);
        assert_eq!(Channel::default_for(&"1.0.0".parse().unwrap()), Channel::Stable);
        assert_eq!(Channel::parse(" BETA "), Some(Channel::Beta));
        assert_eq!(Channel::parse("nightly"), None);
    }
}
