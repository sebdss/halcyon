//! Fichiers d'intégration au bureau Linux (spécification freedesktop), partagés par
//! l'application et l'installateur : même contenu, quel que soit celui qui les écrit.

/// Échappe une valeur pour la clé `Exec` d'un fichier `.desktop`.
pub fn desktop_quote(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        if matches!(c, '"' | '`' | '$' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

/// Nom de fichier sûr (caractères interdits sous Windows retirés).
pub fn safe_file_name(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || c.is_control() { ' ' } else { c })
        .collect();
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    let s = s.trim_matches('.').to_string();
    if s.is_empty() { "Jeu".into() } else { s.chars().take(80).collect() }
}

/// Entrée de démarrage automatique (`~/.config/autostart/halcyon.desktop`).
pub fn autostart_entry(exe: &str) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName=Halcyon\nComment=Launcher de jeux\nExec={} --minimized\nIcon=halcyon\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
        desktop_quote(exe)
    )
}

/// Entrée de menu principale de Halcyon.
pub fn app_entry(exe: &str, icon: &str) -> String {
    let q = desktop_quote(exe);
    format!(
        "[Desktop Entry]\nType=Application\nName=Halcyon\nGenericName=Launcher de jeux\n\
         Comment=Tous vos jeux Steam, Epic, standalone et émulés au même endroit\nExec={q} %f\nIcon={icon}\n\
         Terminal=false\nCategories=Game;\nKeywords=jeux;games;steam;epic;emulateur;launcher;\n\
         MimeType=application/x-halcyon-theme;application/x-halcyon-plugin;\nStartupWMClass=halcyon\n\
         Actions=quick;bigpicture;\n\n[Desktop Action quick]\nName=Lancement rapide\nExec={q} --quick\n\n\
         [Desktop Action bigpicture]\nName=Big Picture\nExec={q} --big-picture\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting_and_names() {
        assert_eq!(desktop_quote("/opt/My \"App\"/$x"), "\"/opt/My \\\"App\\\"/\\$x\"");
        assert_eq!(safe_file_name("Half-Life: Alyx?"), "Half-Life Alyx");
        assert_eq!(safe_file_name("..."), "Jeu");
    }

    #[test]
    fn entries_point_to_the_executable() {
        let e = app_entry("/home/a b/.local/opt/halcyon/halcyon", "halcyon");
        assert!(e.contains("Exec=\"/home/a b/.local/opt/halcyon/halcyon\" %f"));
        assert!(e.contains("Exec=\"/home/a b/.local/opt/halcyon/halcyon\" --quick"));
        assert!(autostart_entry("/x/halcyon").contains("Exec=\"/x/halcyon\" --minimized"));
    }
}
