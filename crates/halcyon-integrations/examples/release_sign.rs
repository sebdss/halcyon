//! Signature des versions de Halcyon (outil de l'auteur, utilisé par la CI).
//!
//! ```text
//! cargo run -p halcyon-integrations --example release_sign -- keygen <fichier-clé-privée>
//! cargo run -p halcyon-integrations --example release_sign -- sign <fichier-clé-privée> SHA256SUMS
//! cargo run -p halcyon-integrations --example release_sign -- verify <clé-publique-base64> SHA256SUMS
//! cargo run -p halcyon-integrations --example release_sign -- sign-plugin <fichier-clé-privée> <dossier-extension>
//! ```
//!
//! `keygen` écrit la clé privée (base64, droits 0600) et affiche la clé publique à placer
//! dans la variable de dépôt `HALCYON_UPDATE_PUBKEY`. La clé privée va dans le secret de CI
//! `HALCYON_SIGNING_KEY` et ne doit jamais être commitée. `sign` écrit `SHA256SUMS.sig`.
//! La clé privée peut aussi être passée par la variable d'environnement
//! `HALCYON_SIGNING_KEY` (le chemin vaut alors `-`).

use base64::Engine;
use std::process::ExitCode;

fn main() -> ExitCode {
    let b64 = base64::engine::general_purpose::STANDARD;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let read_secret = |path: &str| -> Result<ed25519_compact::SecretKey, String> {
        let text = if path == "-" { std::env::var("HALCYON_SIGNING_KEY").map_err(|_| "HALCYON_SIGNING_KEY absente".to_string())? } else { std::fs::read_to_string(path).map_err(|e| e.to_string())? };
        let bytes = b64.decode(text.trim()).map_err(|_| "clé privée illisible".to_string())?;
        ed25519_compact::SecretKey::from_slice(&bytes).map_err(|_| "clé privée invalide".to_string())
    };
    let result: Result<(), String> = match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["keygen", out] => {
            let kp = ed25519_compact::KeyPair::generate();
            let mut opts = std::fs::OpenOptions::new();
            opts.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                opts.mode(0o600);
            }
            use std::io::Write;
            opts.open(out).and_then(|mut f| f.write_all(b64.encode(kp.sk.as_ref()).as_bytes())).map_err(|e| format!("{out} : {e}")).map(|_| {
                println!("Clé publique (HALCYON_UPDATE_PUBKEY) :\n{}", b64.encode(kp.pk.as_ref()));
                eprintln!("Clé privée écrite dans {out} : à placer dans le secret HALCYON_SIGNING_KEY, puis à conserver hors ligne.");
            })
        }
        ["sign", key, sums] => read_secret(key).and_then(|sk| {
            let msg = std::fs::read(sums).map_err(|e| format!("{sums} : {e}"))?;
            let sig = sk.sign(&msg, None);
            sk.public_key().verify(&msg, &sig).map_err(|_| "auto-vérification échouée".to_string())?;
            std::fs::write(format!("{sums}.sig"), format!("{}\n", b64.encode(sig.as_ref()))).map_err(|e| e.to_string())?;
            println!("{sums}.sig écrit");
            Ok(())
        }),
        ["sign-plugin", key, dir] => read_secret(key).and_then(|sk| {
            let pk = halcyon_integrations::plugin_trust::sign(std::path::Path::new(dir), sk.as_ref())?;
            println!("{dir}/plugin.sig écrit (clé {})", halcyon_integrations::plugin_trust::fingerprint(&pk));
            Ok(())
        }),
        ["verify", pk, sums] => {
            let msg = std::fs::read(sums).map_err(|e| format!("{sums} : {e}"));
            let sig = std::fs::read_to_string(format!("{sums}.sig")).map_err(|e| format!("{sums}.sig : {e}"));
            msg.and_then(|m| sig.and_then(|s| halcyon_integrations::update::verify_signature(pk, &m, &s))).map(|_| println!("signature valide"))
        }
        _ => Err("usage : release_sign keygen <clé> | sign <clé|-> <SHA256SUMS> | verify <clé-publique> <SHA256SUMS> | sign-plugin <clé|-> <dossier>".into()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("erreur : {e}");
            ExitCode::FAILURE
        }
    }
}
