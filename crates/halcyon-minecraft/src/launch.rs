//! Ligne de commande de Minecraft : classpath, natives, arguments du fichier de version
//! (nouveau et ancien format), mémoire, résolution, compte. Le jeton d'accès n'apparaît
//! jamais dans les journaux (`LaunchCommand::redacted`).

use crate::install::{client_jar_id, Layout};
use crate::meta::{allowed, Arg, Env, Version};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Identité transmise au jeu.
#[derive(Clone, Debug, Default)]
pub struct Identity {
    pub name: String,
    /// UUID sans tirets.
    pub uuid: String,
    pub access_token: String,
    pub xuid: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    pub min_ram_mb: u32,
    pub max_ram_mb: u32,
    pub jvm_args: String,
    pub width: u32,
    pub height: u32,
    pub fullscreen: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self { min_ram_mb: 1024, max_ram_mb: 4096, jvm_args: String::new(), width: 0, height: 0, fullscreen: false }
    }
}

#[derive(Clone, Debug)]
pub struct LaunchCommand {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    secret: String,
}

impl LaunchCommand {
    /// Ligne de commande affichable (jeton masqué).
    pub fn redacted(&self) -> String {
        let mut s = format!("\"{}\"", self.program.display());
        for a in &self.args {
            let a = if !self.secret.is_empty() { a.replace(&self.secret, "***") } else { a.clone() };
            if a.contains(' ') {
                s.push_str(&format!(" \"{a}\""));
            } else {
                s.push(' ');
                s.push_str(&a);
            }
        }
        s
    }
}

pub fn classpath_separator() -> &'static str {
    if cfg!(windows) {
        ";"
    } else {
        ":"
    }
}

/// Découpe une chaîne d'arguments (guillemets doubles pris en charge).
pub fn split_args(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut any = false;
    for c in s.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                any = true;
            }
            c if c.is_whitespace() && !quoted => {
                if any || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                    any = false;
                }
            }
            c => cur.push(c),
        }
    }
    if any || !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn render(t: &str, vars: &HashMap<&str, String>) -> String {
    let mut out = t.to_string();
    // Remplacement de `${nom}` ; une variable inconnue est laissée telle quelle.
    let mut i = 0;
    while let Some(start) = out[i..].find("${").map(|p| p + i) {
        let Some(end) = out[start..].find('}').map(|p| p + start) else { break };
        let key = &out[start + 2..end];
        match vars.get(key) {
            Some(v) => {
                let v = v.clone();
                out.replace_range(start..=end, &v);
                i = start + v.len();
            }
            None => i = end + 1,
        }
    }
    out
}

fn expand(args: &[Arg], env: &Env, vars: &HashMap<&str, String>) -> Vec<String> {
    let mut out = Vec::new();
    for a in args {
        match a {
            Arg::Plain(s) => out.push(render(s, vars)),
            Arg::Ruled(rules, values) => {
                if allowed(rules, env) {
                    out.extend(values.iter().map(|v| render(v, vars)));
                }
            }
        }
    }
    out
}

/// Classpath : bibliothèques (sans les natives à l'ancienne), puis le client.
pub fn classpath(layout: &Layout, v: &Version, env: &Env) -> Vec<PathBuf> {
    let mut cp: Vec<PathBuf> = Vec::new();
    for f in v.library_files(env).into_iter().filter(|f| f.classpath) {
        let p = layout.libraries().join(&f.path);
        if !cp.contains(&p) {
            cp.push(p);
        }
    }
    let jar = layout.version_jar(&client_jar_id(v));
    if !cp.contains(&jar) {
        cp.push(jar);
    }
    cp
}

/// Construit la commande. `natives` : dossier où les natives ont été extraites.
pub fn command(layout: &Layout, v: &Version, game_dir: &Path, java: &Path, natives: &Path, who: &Identity, opts: &Options) -> LaunchCommand {
    let mut env = Env::current();
    let custom_res = opts.width > 0 && opts.height > 0;
    if custom_res {
        env.features.push("has_custom_resolution");
    }
    let sep = classpath_separator();
    let cp = classpath(layout, v, &env).iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(sep);
    let assets_root = layout.assets();
    let index_id = v.asset_index.as_ref().map(|a| a.id.clone()).or_else(|| v.assets.clone()).unwrap_or_else(|| "legacy".into());
    let virtual_dir = assets_root.join("virtual").join(&index_id);
    let game_assets = if virtual_dir.is_dir() { virtual_dir } else { assets_root.clone() };
    let mut vars: HashMap<&str, String> = HashMap::new();
    vars.insert("auth_player_name", who.name.clone());
    vars.insert("version_name", v.id.clone());
    vars.insert("game_directory", game_dir.display().to_string());
    vars.insert("assets_root", assets_root.display().to_string());
    vars.insert("game_assets", game_assets.display().to_string());
    vars.insert("assets_index_name", index_id);
    vars.insert("auth_uuid", who.uuid.clone());
    vars.insert("auth_access_token", who.access_token.clone());
    vars.insert("auth_session", format!("token:{}:{}", who.access_token, who.uuid));
    vars.insert("auth_xuid", who.xuid.clone());
    vars.insert("clientid", String::new());
    vars.insert("user_type", "msa".into());
    vars.insert("user_properties", "{}".into());
    vars.insert("version_type", v.kind.clone());
    vars.insert("resolution_width", opts.width.to_string());
    vars.insert("resolution_height", opts.height.to_string());
    vars.insert("natives_directory", natives.display().to_string());
    vars.insert("launcher_name", "Halcyon".into());
    vars.insert("launcher_version", env!("CARGO_PKG_VERSION").into());
    vars.insert("classpath", cp);
    vars.insert("classpath_separator", sep.into());
    vars.insert("library_directory", layout.libraries().display().to_string());

    let mut args: Vec<String> = Vec::new();
    let min = opts.min_ram_mb.min(opts.max_ram_mb).max(256);
    args.push(format!("-Xms{min}M"));
    args.push(format!("-Xmx{}M", opts.max_ram_mb.max(512)));
    if v.jvm_args.is_empty() {
        // Ancien format : arguments implicites du launcher officiel.
        args.push(format!("-Djava.library.path={}", natives.display()));
        args.push("-cp".into());
        args.push(vars["classpath"].clone());
    } else {
        args.extend(expand(&v.jvm_args, &env, &vars));
    }
    if let Some(l) = &v.logging {
        let cfg = layout.assets().join("log_configs").join(&l.file.id);
        if cfg.is_file() {
            args.push(l.argument.replace("${path}", &cfg.display().to_string()));
        }
    }
    args.extend(split_args(&opts.jvm_args));
    args.push(v.main_class.clone());
    match &v.legacy_args {
        Some(s) if v.game_args.is_empty() => {
            args.extend(s.split_whitespace().map(|a| render(a, &vars)));
            if custom_res {
                args.extend(["--width".into(), opts.width.to_string(), "--height".into(), opts.height.to_string()]);
            }
        }
        _ => args.extend(expand(&v.game_args, &env, &vars)),
    }
    if opts.fullscreen && !args.iter().any(|a| a == "--fullscreen") {
        args.push("--fullscreen".into());
    }
    LaunchCommand { program: java.to_path_buf(), args, cwd: game_dir.to_path_buf(), secret: who.access_token.clone() }
}

/// Démarre le jeu ; sorties standard et d'erreur écrites dans `log`.
pub fn spawn(cmd: &LaunchCommand, log: &Path) -> std::io::Result<std::process::Child> {
    std::fs::create_dir_all(&cmd.cwd)?;
    if let Some(d) = log.parent() {
        std::fs::create_dir_all(d)?;
    }
    let out = std::fs::File::create(log)?;
    let err = out.try_clone()?;
    let mut c = std::process::Command::new(&cmd.program);
    c.args(&cmd.args).current_dir(&cmd.cwd).stdin(std::process::Stdio::null()).stdout(out).stderr(err);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    c.spawn()
}

/// Dernières lignes du journal (erreur affichée quand le jeu s'arrête aussitôt).
pub fn log_tail(log: &Path, lines: usize) -> String {
    let text = std::fs::read(log).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
    let all: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    all[all.len().saturating_sub(lines)..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta::Version;
    use serde_json::json;

    fn who() -> Identity {
        Identity { name: "Sebi".into(), uuid: "0123456789abcdef0123456789abcdef".into(), access_token: "SECRET-TOKEN".into(), xuid: "42".into() }
    }

    #[test]
    fn modern_command_line() {
        let v = Version::parse(&json!({
            "id": "1.20.1", "mainClass": "net.minecraft.client.main.Main", "type": "release",
            "assetIndex": {"id": "5", "url": "https://x"},
            "arguments": {
                "game": ["--username", "${auth_player_name}", "--accessToken", "${auth_access_token}", "--assetIndex", "${assets_index_name}",
                         {"rules": [{"action":"allow","features":{"has_custom_resolution":true}}], "value": ["--width", "${resolution_width}", "--height", "${resolution_height}"]},
                         {"rules": [{"action":"allow","features":{"is_demo_user":true}}], "value": "--demo"}],
                "jvm": [{"rules": [{"action":"allow","os":{"name":"osx"}}], "value": ["-XstartOnFirstThread"]},
                        "-Djava.library.path=${natives_directory}", "-cp", "${classpath}"]
            },
            "libraries": [{"name":"a:b:1","downloads":{"artifact":{"path":"a/b/1/b-1.jar","url":"https://x/b.jar"}}}]
        }))
        .unwrap();
        let l = Layout::new("/mc");
        let opts = Options { width: 1280, height: 720, max_ram_mb: 6144, min_ram_mb: 1024, jvm_args: "-XX:+UseG1GC \"-Dx=a b\"".into(), fullscreen: false };
        let c = command(&l, &v, Path::new("/mc/profiles/p"), Path::new("/java"), Path::new("/mc/natives"), &who(), &opts);
        let a = &c.args;
        assert_eq!(&a[..2], &["-Xms1024M".to_string(), "-Xmx6144M".to_string()]);
        assert!(!a.contains(&"-XstartOnFirstThread".to_string()));
        assert!(a.contains(&"-Djava.library.path=/mc/natives".to_string()));
        let cp = &a[a.iter().position(|x| x == "-cp").unwrap() + 1];
        assert!(cp.contains("b-1.jar") && cp.ends_with(&format!("1.20.1{}1.20.1.jar", std::path::MAIN_SEPARATOR)), "{cp}");
        assert!(a.contains(&"-Dx=a b".to_string()));
        let main = a.iter().position(|x| x == "net.minecraft.client.main.Main").unwrap();
        assert_eq!(&a[main + 1..main + 3], &["--username".to_string(), "Sebi".to_string()]);
        assert!(a.contains(&"--width".to_string()) && a.contains(&"1280".to_string()));
        assert!(!a.contains(&"--demo".to_string()));
        assert!(!c.redacted().contains("SECRET-TOKEN") && c.redacted().contains("***"));
    }

    #[test]
    fn legacy_command_line() {
        let v = Version::parse(&json!({"id":"1.8.9","mainClass":"net.minecraft.client.main.Main",
            "minecraftArguments": "--username ${auth_player_name} --session ${auth_session} --userProperties ${user_properties} --gameDir ${game_directory}",
            "libraries": []})).unwrap();
        let c = command(&Layout::new("/mc"), &v, Path::new("/g"), Path::new("/java"), Path::new("/n"), &who(), &Options { fullscreen: true, ..Default::default() });
        assert!(c.args.contains(&"-Djava.library.path=/n".to_string()));
        assert!(c.args.contains(&"token:SECRET-TOKEN:0123456789abcdef0123456789abcdef".to_string()));
        assert!(c.args.contains(&"{}".to_string()));
        assert_eq!(c.args.last().map(String::as_str), Some("--fullscreen"));
    }

    #[test]
    fn splitting_and_rendering() {
        assert_eq!(split_args("-a  \"b c\" d"), vec!["-a", "b c", "d"]);
        assert_eq!(split_args(""), Vec::<String>::new());
        let mut vars = HashMap::new();
        vars.insert("x", "1".to_string());
        assert_eq!(render("${x}-${inconnu}-${x}", &vars), "1-${inconnu}-1");
    }
}
