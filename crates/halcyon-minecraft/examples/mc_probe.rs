//! Essais réels du moteur Minecraft (réseau, disque, Java), sans interface.
//! `cargo run -p halcyon-minecraft --example mc_probe -- <racine> <commande> [arguments]`
//!
//! Commandes : `latest`, `install <version>`, `verify <version>`, `java`, `runtime <composant>`,
//! `loaders <loader> <mc>`, `loader <loader> <mc> <version>`, `check <version>` (prépare la
//! ligne de commande sans lancer le jeu), `search <texte> <mc> <loader>`,
//! `mod <profil> <projet>`, `pack <projet modrinth>`.
use halcyon_minecraft::http::Http;
use halcyon_minecraft::install::{self, Event, Layout};
use halcyon_minecraft::launch::{self, Identity, Options};
use halcyon_minecraft::{java, loaders, modpack, mods, profiles, Loader};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let layout = Layout::new(PathBuf::from(&a[0]));
    let http = Http::new();
    let cancel = AtomicBool::new(false);
    let last = Mutex::new(std::time::Instant::now());
    let sink = |e: Event| match e {
        Event::Progress { bytes, total_bytes, files, total_files } => {
            let mut l = last.lock().unwrap();
            if l.elapsed().as_millis() > 1500 || files == total_files {
                *l = std::time::Instant::now();
                println!("  {files}/{total_files} fichiers, {} / {}", mods::human(bytes), mods::human(total_bytes));
            }
        }
        Event::Step(s) => println!("> {s}"),
        Event::Log(l) => println!("  | {l}"),
    };
    let t = std::time::Instant::now();
    let arg = |i: usize| a.get(i).cloned().unwrap_or_default();
    let java_for = |id: &str| -> PathBuf {
        let v = install::resolve_local(&layout, id).expect("version installée");
        let comp = v.java_component();
        java::mojang_runtime(&layout, &comp).unwrap_or_else(|| java::install_runtime(&http, &layout, &comp, &cancel, &sink).expect("runtime"))
    };
    match a[1].as_str() {
        "latest" => {
            let m = install::manifest(&http, &layout, true).unwrap();
            println!("release {} · snapshot {} · {} versions", m.latest.release, m.latest.snapshot, m.versions.len());
        }
        "install" => {
            let m = install::manifest(&http, &layout, true).unwrap();
            let v = install::install(&http, &layout, &arg(2), Some(&m), false, &cancel, &sink).unwrap();
            println!("OK {} (Java {} / {})", v.id, v.java_major(), v.java_component());
        }
        "verify" => println!("{:?}", install::verify(&layout, &arg(2)).unwrap()),
        "java" => {
            for j in java::detect(&layout) {
                println!("{} {} ({}) {}", j.major, j.version, j.source, j.path.display());
            }
        }
        "runtime" => println!("{}", java::install_runtime(&http, &layout, &arg(2), &cancel, &sink).unwrap().display()),
        "loaders" => {
            let l = Loader::from_id(&arg(2)).unwrap();
            let v = loaders::versions(&http, l, &arg(3)).unwrap();
            println!("{} versions ; {:?}", v.len(), v.iter().take(5).collect::<Vec<_>>());
        }
        "loader" => {
            let l = Loader::from_id(&arg(2)).unwrap();
            let mc = arg(3);
            let m = install::manifest(&http, &layout, true).unwrap();
            install::install(&http, &layout, &mc, Some(&m), false, &cancel, &sink).unwrap();
            let j = java_for(&mc);
            let id = loaders::install(&http, &layout, l, &mc, &arg(4), Some(&j), &cancel, &sink).unwrap();
            let v = install::install(&http, &layout, &id, Some(&m), false, &cancel, &sink).unwrap();
            println!("OK {} → {} (main {})", id, v.id, v.main_class);
        }
        "check" => {
            let id = arg(2);
            let v = install::resolve_local(&layout, &id).unwrap();
            let natives = install::extract_natives(&layout, &v).unwrap();
            let j = java_for(&id);
            let game = layout.profiles().join("probe");
            let cmd = launch::command(&layout, &v, &game, &j, &natives, &Identity { name: "Probe".into(), uuid: "0".repeat(32), access_token: "x".into(), xuid: String::new() }, &Options::default());
            let env = halcyon_minecraft::meta::Env::current();
            let missing: Vec<_> = launch::classpath(&layout, &v, &env).into_iter().filter(|p| !p.is_file()).collect();
            let n = std::fs::read_dir(&natives).map(|r| r.count()).unwrap_or(0);
            println!("classpath manquant : {missing:?}\nnatives : {n} fichier(s)\nJava : {}", j.display());
            println!("{}", cmd.redacted().chars().take(600).collect::<String>());
            // Le jeu n'est pas lancé : il faut un compte Microsoft valide.
            let out = std::process::Command::new(j.with_file_name(if cfg!(windows) { "java.exe" } else { "java" }))
                .args(["-cp", &launch::classpath(&layout, &v, &env).iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(launch::classpath_separator()), "-XshowSettings:properties", "-version"])
                .output()
                .unwrap();
            println!("java -cp … -version : {}", out.status);
        }
        "search" => {
            let c = mods::Client { http: &http, curseforge_key: "" };
            let (p, total) = c.search(mods::Source::Modrinth, mods::Kind::Mod, &arg(2), Some(&arg(3)), Loader::from_id(&arg(4)), 0).unwrap();
            println!("{total} résultats : {:?}", p.iter().take(5).map(|p| (&p.title, &p.id)).collect::<Vec<_>>());
        }
        "mod" => {
            let mut p = profiles::load(&layout, &arg(2)).expect("profil");
            let c = mods::Client { http: &http, curseforge_key: "" };
            let v = c.best(mods::Source::Modrinth, &arg(3), &p.mc, p.loader).unwrap().expect("version compatible");
            let added = c.install_mod(&layout, &mut p, v, &cancel, &sink).unwrap();
            println!("{:?}", added.iter().map(|m| (&m.name, &m.file, m.dependency)).collect::<Vec<_>>());
            println!("mises à jour : {:?}", mods::modrinth_updates(&http, &layout, &p).unwrap().len());
        }
        "profile" => {
            let l = Loader::from_id(&arg(3)).unwrap();
            let p = profiles::for_version(&layout, &arg(2), l, &arg(4), &arg(5));
            profiles::save(&layout, &p).unwrap();
            println!("{}", p.id);
        }
        "pack" => {
            let c = mods::Client { http: &http, curseforge_key: "" };
            let vs = c.versions(mods::Source::Modrinth, &arg(2), None, None).unwrap();
            let v = vs.first().expect("version");
            let f = layout.cache().join("packs").join(&v.file.filename);
            http.download(v.file.url.as_deref().unwrap(), &f, v.file.sha1.as_deref(), &cancel, &mut |_| {}).unwrap();
            let r = profiles::ModpackRef { source: "modrinth".into(), project_id: arg(2), version_id: v.id.clone(), name: String::new(), version: String::new(), files: vec![] };
            let p = modpack::install_mrpack(&http, &layout, &f, r, None, &cancel, &sink).unwrap();
            println!("OK {} : Minecraft {} {} {} · {} mods", p.id, p.mc, p.loader.name(), p.loader_version, p.mods.len());
        }
        other => println!("commande inconnue {other}"),
    }
    println!("({:.1} s)", t.elapsed().as_secs_f32());
}
