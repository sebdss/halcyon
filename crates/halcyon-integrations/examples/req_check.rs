//! Lit des fiches `appdetails` enregistrées et affiche la configuration reconnue.
fn main() {
    for path in std::env::args().skip(1) {
        let text = std::fs::read_to_string(&path).unwrap();
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        let appid: u64 = v.as_object().and_then(|o| o.keys().next()).and_then(|k| k.parse().ok()).unwrap();
        println!("{appid}: {:#?}", halcyon_integrations::steam_requirements::parse(&v, appid, false));
    }
}
