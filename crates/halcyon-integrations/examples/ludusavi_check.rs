//! Vérifie la lecture d'un manifeste Ludusavi réel : `cargo run --release -p halcyon-integrations --example ludusavi_check -- manifest.yaml`
fn main() {
    let path = std::env::args().nth(1).expect("chemin du manifeste");
    let text = std::fs::read_to_string(&path).expect("lecture");
    let t = std::time::Instant::now();
    match halcyon_integrations::ludusavi::Index::from_manifest(&text) {
        Ok(idx) => {
            println!("{} jeux avec sauvegardes, lus en {:?}", idx.len(), t.elapsed());
            use halcyon_core::GameSource;
            use halcyon_integrations::ludusavi::{Places, Query};
            for (name, src, key) in [("Hades", GameSource::Steam, "1145360"), ("Celeste", GameSource::Itch, "x"), ("Elden Ring", GameSource::Steam, "1245620"), ("Stardew Valley", GameSource::Gog, "1453375253"), ("Hollow Knight", GameSource::Steam, "367520")] {
                let env = Places::current(Some("/home/sebi/Steam".into()), Some(22202)).with_proton(key, None);
                let q = Query { name: name.into(), source: Some(src), source_key: key.into(), install_dir: Some(format!("/jeux/{name}").into()) };
                println!("{name} → {:?}", idx.locations(&q, &env));
            }
        }
        Err(e) => println!("refusé : {e}"),
    }
}
