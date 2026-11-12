//! Images distantes secondaires (icônes de succès, captures d'écran) :
//! téléchargées à la demande dans le cache, puis servies hors ligne.

use halcyon_core::util::stable_hash;
use halcyon_integrations::artwork::Downloader;
use std::cell::RefCell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Sender};

pub struct RemoteImages {
    dir: PathBuf,
    pending: RefCell<HashSet<String>>,
    tx: Sender<String>,
}

impl RemoteImages {
    /// `done(url, succès)` est appelé depuis le thread de téléchargement.
    pub fn new(dir: PathBuf, done: impl Fn(String, bool) + Send + 'static) -> Self {
        let _ = std::fs::create_dir_all(&dir);
        let (tx, rx) = channel::<String>();
        let target = dir.clone();
        std::thread::Builder::new()
            .name("halcyon-remote".into())
            .spawn(move || {
                let dl = Downloader::new();
                for url in rx {
                    let ok = match dl.fetch_to(&url, &path_in(&target, &url)) {
                        Ok(()) => true,
                        Err(e) => {
                            log::debug!("image distante indisponible {url} : {e}");
                            false
                        }
                    };
                    done(url, ok);
                }
            })
            .expect("thread de téléchargement");
        Self { dir, pending: RefCell::new(HashSet::new()), tx }
    }

    pub fn path_for(&self, url: &str) -> PathBuf {
        path_in(&self.dir, url)
    }

    /// Chemin local si l'image est déjà en cache ; sinon lance le téléchargement.
    pub fn ensure(&self, url: &str, online: bool) -> Option<PathBuf> {
        let p = self.path_for(url);
        if p.is_file() {
            return Some(p);
        }
        if online && self.pending.borrow_mut().insert(url.to_string()) {
            let _ = self.tx.send(url.to_string());
        }
        None
    }

    pub fn finished(&self, url: &str) {
        self.pending.borrow_mut().remove(url);
    }
}

fn path_in(dir: &Path, url: &str) -> PathBuf {
    dir.join(format!("{:016x}.img", stable_hash(url)))
}
