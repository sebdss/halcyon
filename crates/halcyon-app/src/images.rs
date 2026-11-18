//! Chargement des visuels, conçu pour les grandes bibliothèques :
//!
//! - décodage dans des threads dédiés (jamais sur le thread de l'interface) ;
//! - pile LIFO : les images demandées en dernier (celles que l'on regarde) passent d'abord ;
//! - redimensionnement à la taille d'affichage, puis miniature écrite sur disque :
//!   au lancement suivant, on décode une petite image au lieu de l'original ;
//! - cache mémoire LRU borné en octets.

use halcyon_core::util::stable_hash;
use halcyon_core::GameId;
use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView, ImageReader};
use slint::{Image, Rgba8Pixel, SharedPixelBuffer};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};

/// Budget mémoire des images décodées (RGBA).
const MEMORY_BUDGET: usize = 32 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Cover,
    Banner,
    Logo,
    /// Icône de succès (carrée).
    Icon,
    /// Capture d'écran (vignette 16:9).
    Shot,
    /// Ambiance : jaquette réduite et floutée, étirée en fond (mode immersif).
    Ambient,
    /// Capture en grand, pour la bannière animée de la fiche (16:9).
    Slide,
}

impl Kind {
    /// Taille cible du décodage (largeur, hauteur).
    fn target(self) -> (u32, u32) {
        match self {
            // Jaquette de ~170 px logiques, nette jusqu'à ~140 % d'échelle d'affichage.
            Kind::Cover => (240, 360),
            Kind::Banner => (1600, 0),
            Kind::Logo => (640, 260),
            Kind::Icon => (96, 96),
            Kind::Shot => (480, 270),
            Kind::Ambient => (48, 72),
            Kind::Slide => (1280, 720),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImageKey {
    pub game: GameId,
    pub kind: Kind,
}

impl ImageKey {
    /// Clé d'une image distante (icône, capture), dérivée de son URL.
    pub fn remote(url: &str, kind: Kind) -> Self {
        Self { game: ((stable_hash(url) >> 3) as i64) | (1 << 60), kind }
    }
}

struct Request {
    key: ImageKey,
    path: PathBuf,
}

struct Queue {
    jobs: Mutex<Vec<Request>>,
    cv: Condvar,
}

pub type LoadResult = Result<SharedPixelBuffer<Rgba8Pixel>, String>;

struct Entry {
    image: Image,
    bytes: usize,
    last_used: u64,
}

#[derive(Default)]
struct Inner {
    map: HashMap<ImageKey, Entry>,
    pending: HashSet<ImageKey>,
    failed: HashSet<ImageKey>,
    bytes: usize,
    tick: u64,
}

pub struct ImageCache {
    inner: RefCell<Inner>,
    queue: Arc<Queue>,
}

impl ImageCache {
    /// Démarre `workers` threads de décodage. `deliver` est appelé depuis ces threads
    /// avec le résultat ; il doit le transmettre au thread de l'interface.
    pub fn new(
        thumbs_dir: PathBuf,
        workers: usize,
        deliver: impl Fn(ImageKey, LoadResult) + Send + Sync + 'static,
    ) -> Self {
        let queue = Arc::new(Queue { jobs: Mutex::new(Vec::new()), cv: Condvar::new() });
        let deliver = Arc::new(deliver);
        for i in 0..workers.max(1) {
            let queue = queue.clone();
            let deliver = deliver.clone();
            let thumbs = thumbs_dir.clone();
            std::thread::Builder::new()
                .name(format!("halcyon-images-{i}"))
                .spawn(move || loop {
                    let req = {
                        let mut jobs = queue.jobs.lock().unwrap_or_else(|e| e.into_inner());
                        while jobs.is_empty() {
                            jobs = queue.cv.wait(jobs).unwrap_or_else(|e| e.into_inner());
                        }
                        jobs.pop().expect("file non vide")
                    };
                    let result = load(&req.path, req.key.kind, &thumbs);
                    if let Err(e) = &result {
                        log::debug!("visuel {:?} illisible ({}) : {e}", req.key, req.path.display());
                    }
                    deliver(req.key, result);
                })
                .expect("création d'un thread de décodage");
        }
        Self { inner: RefCell::new(Inner::default()), queue }
    }

    /// Image en cache, ou `None` (le chargement est alors lancé si un fichier est connu).
    pub fn get(&self, key: ImageKey, path: Option<&Path>) -> Option<Image> {
        let mut inner = self.inner.borrow_mut();
        inner.tick += 1;
        let tick = inner.tick;
        if let Some(e) = inner.map.get_mut(&key) {
            e.last_used = tick;
            return Some(e.image.clone());
        }
        let path = path?;
        if inner.pending.contains(&key) || inner.failed.contains(&key) {
            return None;
        }
        inner.pending.insert(key);
        drop(inner);
        let mut jobs = self.queue.jobs.lock().unwrap_or_else(|e| e.into_inner());
        jobs.push(Request { key, path: path.to_path_buf() });
        self.queue.cv.notify_one();
        None
    }

    /// Reçoit un résultat (thread de l'interface). Renvoie `true` si une image a été ajoutée.
    pub fn complete(&self, key: ImageKey, result: LoadResult) -> bool {
        let mut inner = self.inner.borrow_mut();
        if !inner.pending.remove(&key) {
            // Invalidée entre-temps : résultat périmé.
            return false;
        }
        match result {
            Ok(buffer) => {
                let bytes = (buffer.width() * buffer.height() * 4) as usize;
                inner.tick += 1;
                let tick = inner.tick;
                let image = Image::from_rgba8(buffer);
                inner.bytes += bytes;
                inner.map.insert(key, Entry { image, bytes, last_used: tick });
                evict(&mut inner);
                true
            }
            Err(_) => {
                inner.failed.insert(key);
                false
            }
        }
    }

    /// Oublie les visuels d'un jeu (fichier remplacé, téléchargé…).
    pub fn invalidate(&self, game: GameId) {
        let mut inner = self.inner.borrow_mut();
        for kind in [Kind::Cover, Kind::Banner, Kind::Logo, Kind::Icon, Kind::Shot, Kind::Slide] {
            let key = ImageKey { game, kind };
            if let Some(e) = inner.map.remove(&key) {
                inner.bytes -= e.bytes;
            }
            inner.pending.remove(&key);
            inner.failed.remove(&key);
        }
    }

    pub fn memory_usage(&self) -> usize {
        self.inner.borrow().bytes
    }
}

fn evict(inner: &mut Inner) {
    while inner.bytes > MEMORY_BUDGET && inner.map.len() > 1 {
        let Some((&oldest, _)) = inner.map.iter().min_by_key(|(_, e)| e.last_used) else { break };
        if let Some(e) = inner.map.remove(&oldest) {
            inner.bytes -= e.bytes;
        }
    }
}

/// Charge un visuel à la taille d'affichage, en passant par le cache de miniatures.
pub fn load(path: &Path, kind: Kind, thumbs_dir: &Path) -> LoadResult {
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    let mtime = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs());
    let (tw, th) = kind.target();
    let ext = if kind == Kind::Logo { "png" } else { "jpg" };
    let id = stable_hash(&format!("{}|{:?}|{}|{tw}x{th}", path.display(), mtime, meta.len()));
    let thumb = thumbs_dir.join(format!("{id:016x}.{ext}"));

    if let Ok(img) = decode(&thumb) {
        return Ok(to_buffer(img));
    }

    let img = decode(path)?;
    let (w, h) = img.dimensions();
    let resized = match kind {
        Kind::Cover | Kind::Icon | Kind::Shot | Kind::Slide => img.resize_to_fill(tw, th, FilterType::Triangle),
        Kind::Ambient => img.resize_to_fill(tw, th, FilterType::Triangle).blur(2.5),
        Kind::Banner if w > tw => img.resize(tw, (h as u64 * tw as u64 / w.max(1) as u64) as u32, FilterType::Triangle),
        Kind::Logo if w > tw || h > th => img.resize(tw, th, FilterType::Triangle),
        _ => img,
    };
    let saved = if kind == Kind::Logo {
        resized.save_with_format(&thumb, image::ImageFormat::Png)
    } else {
        DynamicImage::ImageRgb8(resized.to_rgb8()).save_with_format(&thumb, image::ImageFormat::Jpeg)
    };
    if let Err(e) = saved {
        log::debug!("miniature non écrite ({}) : {e}", thumb.display());
    }
    Ok(to_buffer(resized))
}

fn decode(path: &Path) -> Result<DynamicImage, String> {
    ImageReader::open(path)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?
        .decode()
        .map_err(|e| e.to_string())
}

fn to_buffer(img: DynamicImage) -> SharedPixelBuffer<Rgba8Pixel> {
    let rgba = img.into_rgba8();
    SharedPixelBuffer::clone_from_slice(rgba.as_raw(), rgba.width(), rgba.height())
}

/// Taille totale d'un dossier (miniatures), en octets.
pub fn dir_size(dir: &Path) -> u64 {
    std::fs::read_dir(dir)
        .map(|entries| entries.flatten().filter_map(|e| e.metadata().ok()).filter(|m| m.is_file()).map(|m| m.len()).sum())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn covers_are_cropped_to_target_and_thumbnailed() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("wide.png");
        image::RgbImage::from_pixel(1200, 400, image::Rgb([200, 30, 30])).save(&src).unwrap();
        let thumbs = tmp.path().join("thumbs");
        std::fs::create_dir_all(&thumbs).unwrap();

        let buf = load(&src, Kind::Cover, &thumbs).unwrap();
        assert_eq!((buf.width(), buf.height()), (240, 360));
        assert_eq!(std::fs::read_dir(&thumbs).unwrap().count(), 1, "miniature écrite");

        // Deuxième chargement : depuis la miniature, même résultat.
        let again = load(&src, Kind::Cover, &thumbs).unwrap();
        assert_eq!((again.width(), again.height()), (240, 360));

        let banner = load(&src, Kind::Banner, &thumbs).unwrap();
        assert_eq!(banner.width(), 1200, "pas d'agrandissement");
        assert!(load(&tmp.path().join("absent.jpg"), Kind::Cover, &thumbs).is_err());
    }
}
