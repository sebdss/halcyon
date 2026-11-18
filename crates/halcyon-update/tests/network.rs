//! Réseau et téléchargement contre un serveur HTTP local : détection, canaux, ETag,
//! manifeste falsifié, paquet modifié, coupure et reprise (HTTP Range), annulation.
#![cfg(feature = "fault-injection")]

use halcyon_update::core::{Channel, Platform};
use halcyon_update::download::{download, DownloadError, Retry};
use halcyon_update::network::{self, Net, Outcome, Query};
use halcyon_update::testkit::{self, Route, Server};
use halcyon_update::Version;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

const REPO: &str = "sebdss/halcyon";
const P: Platform = Platform::LinuxX64;

fn v(s: &str) -> Version {
    s.parse().unwrap()
}

struct Fixture {
    server: Server,
    kp: ed25519_compact::KeyPair,
    dir: tempfile::TempDir,
}

fn fixture(versions: &[&str]) -> Fixture {
    let server = Server::start();
    let kp = testkit::test_key(7);
    let list: Vec<_> = versions
        .iter()
        .map(|ver| {
            let payload = format!("paquet {ver}").repeat(2000).into_bytes();
            testkit::publish(&server, &kp, &testkit::manifest_for(ver, P, &payload), &payload)
        })
        .collect();
    testkit::set_feed(&server, REPO, &list, "\"etag-1\"");
    Fixture { server, kp, dir: tempfile::tempdir().unwrap() }
}

fn query<'a>(f: &Fixture, channel: Channel, current: &'a Version, keys: &'a [halcyon_update::security::PublicKey]) -> Query<'a> {
    Query { repo: REPO, channel, current, platform: P, keys, floor: None, cache: f.dir.path().join("releases.json") }
}

fn available(o: Outcome) -> Option<String> {
    match o {
        Outcome::Available(f) => Some(f.checked.version.to_string()),
        Outcome::UpToDate { .. } => None,
    }
}

#[test]
fn detects_update_and_respects_channels() {
    let f = fixture(&["1.0.0", "1.1.0-beta.1"]);
    let net = Net::for_local_tests(&f.server.base);
    let keys = [testkit::public(&f.kp)];
    // Nouvelle version détectée.
    assert_eq!(available(network::check(&net, &query(&f, Channel::Stable, &v("0.5.1"), &keys)).unwrap()).as_deref(), Some("1.0.0"));
    // Absence de mise à jour.
    assert_eq!(available(network::check(&net, &query(&f, Channel::Stable, &v("1.0.0"), &keys)).unwrap()), None);
    // Passage Stable → Beta.
    assert_eq!(available(network::check(&net, &query(&f, Channel::Beta, &v("1.0.0"), &keys)).unwrap()).as_deref(), Some("1.1.0-beta.1"));
    // Passage Beta → Stable : aucune descente, note explicative.
    match network::check(&net, &query(&f, Channel::Stable, &v("1.1.0-beta.1"), &keys)).unwrap() {
        Outcome::UpToDate { note } => assert!(note.unwrap().contains("prochaine version stable")),
        other => panic!("{other:?}"),
    }
}

#[test]
fn etag_makes_requests_conditional() {
    let f = fixture(&["1.0.0"]);
    let net = Net::for_local_tests(&f.server.base);
    let keys = [testkit::public(&f.kp)];
    let cur = v("0.5.1");
    let q = query(&f, Channel::Stable, &cur, &keys);
    assert!(available(network::check(&net, &q).unwrap()).is_some());
    assert!(available(network::check(&net, &q).unwrap()).is_some(), "réponse 304 : liste reprise du cache");
    let reqs = f.server.requests("/repos/sebdss/halcyon/releases?per_page=30");
    assert_eq!(reqs.len(), 2);
    assert!(!reqs[0].headers.contains_key("if-none-match"));
    assert_eq!(reqs[1].headers.get("if-none-match").map(String::as_str), Some("\"etag-1\""));
}

#[test]
fn api_errors_are_explained() {
    let f = fixture(&[]);
    let net = Net::for_local_tests(&f.server.base);
    let keys = [testkit::public(&f.kp)];
    let path = "/repos/sebdss/halcyon/releases?per_page=30";
    f.server.set(path, Route { status: 403, headers: vec![("x-ratelimit-remaining".into(), "0".into())], ..Default::default() });
    assert!(network::check(&net, &query(&f, Channel::Stable, &v("1.0.0"), &keys)).unwrap_err().contains("limite"));
    f.server.set(path, Route { status: 503, ..Default::default() });
    assert!(network::check(&net, &query(&f, Channel::Stable, &v("1.0.0"), &keys)).unwrap_err().contains("indisponible"));
    f.server.set(path, Route::ok("pas du json"));
    assert!(network::check(&net, &query(&f, Channel::Stable, &v("1.0.0"), &keys)).is_err());
    // Serveur injoignable.
    let dead = Net::for_local_tests("http://127.0.0.1:9");
    assert!(network::check(&dead, &query(&f, Channel::Stable, &v("1.0.0"), &keys)).is_err());
    // HTTPS obligatoire hors tests.
    assert!(Net::new().fetch_small("http://example.com/x", 10).unwrap_err().contains("HTTPS"));
}

#[test]
fn forged_manifest_is_rejected() {
    let f = fixture(&["1.0.0"]);
    let net = Net::for_local_tests(&f.server.base);
    // Signé par une autre clé que celle intégrée.
    let other = [testkit::public(&testkit::test_key(9))];
    assert!(network::check(&net, &query(&f, Channel::Stable, &v("0.5.1"), &other)).unwrap_err().contains("signature incorrecte"));
    // Manifeste modifié après signature (empreinte du paquet remplacée).
    let keys = [testkit::public(&f.kp)];
    let path = "/dl/1.0.0/halcyon-manifest.json";
    let mut m: serde_json::Value = serde_json::from_slice(&net.fetch_small(&f.server.url(path), 1 << 20).unwrap()).unwrap();
    m["packages"][0]["sha256"] = serde_json::json!("0".repeat(64));
    f.server.set(path, Route::ok(serde_json::to_vec_pretty(&m).unwrap()));
    assert!(network::check(&net, &query(&f, Channel::Stable, &v("0.5.1"), &keys)).unwrap_err().contains("signature incorrecte"));
    // Aucune clé intégrée : refus.
    assert!(network::check(&net, &query(&f, Channel::Stable, &v("0.5.1"), &[])).is_err());
}

fn found(f: &Fixture, current: &str) -> Box<network::Found> {
    let net = Net::for_local_tests(&f.server.base);
    let keys = [testkit::public(&f.kp)];
    let cur = v(current);
    match network::check(&net, &query(f, Channel::Beta, &cur, &keys)).unwrap() {
        Outcome::Available(x) => x,
        o => panic!("{o:?}"),
    }
}

const FAST: Retry = Retry { attempts: 3, delay: Duration::from_millis(20) };

#[test]
fn modified_package_is_rejected_and_deleted() {
    let f = fixture(&["1.0.0"]);
    let x = found(&f, "0.5.1");
    let net = Net::for_local_tests(&f.server.base);
    let path = format!("/dl/1.0.0/{}", x.package.name);
    let mut tampered = "paquet 1.0.0".repeat(2000).into_bytes();
    tampered[100] ^= 0xff;
    f.server.set(&path, Route::ok(tampered));
    let out = f.dir.path().join("dl");
    let err = download(&net, &x.package_url, &x.package, &out, &AtomicBool::new(false), FAST, &mut |_| {}).unwrap_err();
    assert!(matches!(&err, DownloadError::Failed(e) if e.contains("SHA-256")), "{err}");
    assert!(std::fs::read_dir(&out).unwrap().next().is_none(), "fichier altéré supprimé");
}

#[test]
fn interrupted_download_resumes_with_range() {
    let f = fixture(&["1.0.0"]);
    let x = found(&f, "0.5.1");
    let net = Net::for_local_tests(&f.server.base);
    let path = format!("/dl/1.0.0/{}", x.package.name);
    let body = "paquet 1.0.0".repeat(2000).into_bytes();
    f.server.set(&path, Route { cut_after: Some(5000), cuts: 1, ..Route::ok(body.clone()) });
    let out = f.dir.path().join("dl");
    let mut events = Vec::new();
    let file = download(&net, &x.package_url, &x.package, &out, &AtomicBool::new(false), FAST, &mut |p| events.push(p)).unwrap();
    assert_eq!(std::fs::read(file).unwrap(), body);
    let reqs = f.server.requests(&path);
    assert_eq!(reqs.len(), 2, "un essai coupé, une reprise");
    assert_eq!(reqs[1].headers.get("range").map(String::as_str), Some("bytes=5000-"));
    assert!(events.iter().any(|p| p.resumed));
    assert!(events.windows(2).all(|w| w[0].done <= w[1].done || w[1].resumed), "progression monotone");
    assert_eq!(events.last().unwrap().done, body.len() as u64);
}

#[test]
fn resume_after_process_restart_and_server_without_range() {
    let f = fixture(&["1.0.0"]);
    let x = found(&f, "0.5.1");
    let net = Net::for_local_tests(&f.server.base);
    let path = format!("/dl/1.0.0/{}", x.package.name);
    let body = "paquet 1.0.0".repeat(2000).into_bytes();
    let out = f.dir.path().join("dl");
    // Premier lancement : toutes les tentatives sont coupées (réseau perdu).
    f.server.set(&path, Route { cut_after: Some(3000), cuts: 10, ..Route::ok(body.clone()) });
    let err = download(&net, &x.package_url, &x.package, &out, &AtomicBool::new(false), Retry { attempts: 1, delay: Duration::ZERO }, &mut |_| {}).unwrap_err();
    assert!(matches!(err, DownloadError::Failed(_)));
    assert!(out.join(format!("{}.part", x.package.name)).is_file(), "fichier partiel conservé");
    // Relance (autre processus) : la reprise part de l'octet 3000.
    f.server.set(&path, Route::ok(body.clone()));
    download(&net, &x.package_url, &x.package, &out, &AtomicBool::new(false), FAST, &mut |_| {}).unwrap();
    assert_eq!(f.server.requests(&path).last().unwrap().headers.get("range").map(String::as_str), Some("bytes=3000-"));
    // Serveur sans Range : téléchargement complet, toujours vérifié.
    std::fs::remove_dir_all(&out).unwrap();
    f.server.set(&path, Route { range: false, cut_after: Some(4000), cuts: 1, ..Route::ok(body.clone()) });
    let file = download(&net, &x.package_url, &x.package, &out, &AtomicBool::new(false), FAST, &mut |_| {}).unwrap();
    assert_eq!(std::fs::read(file).unwrap(), body);
}

#[test]
fn cancellation_stops_and_keeps_partial() {
    let f = fixture(&["1.0.0"]);
    let x = found(&f, "0.5.1");
    let net = Net::for_local_tests(&f.server.base);
    let cancel = AtomicBool::new(true);
    let out = f.dir.path().join("dl");
    assert_eq!(download(&net, &x.package_url, &x.package, &out, &cancel, FAST, &mut |_| {}).unwrap_err(), DownloadError::Cancelled);
    assert!(!out.join(&x.package.name).exists(), "aucun fichier définitif après annulation");
}
