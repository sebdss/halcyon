//! Journalisation dans un fichier quotidien (`halcyon-AAAA-MM-JJ.log`), 7 jours conservés.
//! Les paniques sont journalisées avec leur emplacement avant l'arrêt.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const RETENTION_DAYS: i64 = 7;

struct FileLogger {
    file: Mutex<Option<File>>,
    level: log::LevelFilter,
    echo_stderr: bool,
}

impl log::Log for FileLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        // Les bibliothèques tierces ne remontent que leurs avertissements.
        let ours = metadata.target().starts_with("halcyon");
        metadata.level() <= if ours { self.level } else { log::LevelFilter::Warn }
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = format!("{} {:<5} [{}] {}\n", timestamp(), record.level(), record.target(), record.args());
        if self.echo_stderr {
            eprint!("{line}");
        }
        if let Ok(mut guard) = self.file.lock() {
            if let Some(f) = guard.as_mut() {
                let _ = f.write_all(line.as_bytes());
            }
        }
    }

    fn flush(&self) {
        if let Ok(mut guard) = self.file.lock() {
            if let Some(f) = guard.as_mut() {
                let _ = f.flush();
            }
        }
    }
}

fn timestamp() -> String {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let secs = now.as_secs() as i64;
    let (y, m, d) = halcyon_core::util::civil_from_days(secs.div_euclid(86_400));
    let s = secs.rem_euclid(86_400);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}.{:03}Z", s / 3600, s / 60 % 60, s % 60, now.subsec_millis())
}

fn log_file_name(day: i64) -> String {
    let (y, m, d) = halcyon_core::util::civil_from_days(day);
    format!("halcyon-{y:04}-{m:02}-{d:02}.log")
}

/// Initialise le journal. Renvoie le chemin du fichier du jour.
pub fn init(logs_dir: &Path, level: log::LevelFilter) -> Option<PathBuf> {
    let today = crate::now_unix().div_euclid(86_400);
    cleanup(logs_dir, today);
    let path = logs_dir.join(log_file_name(today));
    let file = OpenOptions::new().create(true).append(true).open(&path).ok();
    let logger = FileLogger { file: Mutex::new(file), level, echo_stderr: cfg!(debug_assertions) };
    if log::set_boxed_logger(Box::new(logger)).is_ok() {
        log::set_max_level(log::LevelFilter::Trace);
    }

    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current().name().unwrap_or("?").to_string();
        log::error!(target: "halcyon::panic", "panique dans le thread « {thread} » : {info}");
        log::logger().flush();
        default_hook(info);
    }));
    Some(path)
}

/// Supprime les journaux plus anciens que la durée de rétention.
fn cleanup(logs_dir: &Path, today: i64) {
    let keep: Vec<String> = (0..RETENTION_DAYS).map(|i| log_file_name(today - i)).collect();
    let Ok(entries) = std::fs::read_dir(logs_dir) else { return };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with("halcyon-") && name.ends_with(".log") && !keep.contains(&name) {
            let _ = std::fs::remove_file(e.path());
        }
    }
}
