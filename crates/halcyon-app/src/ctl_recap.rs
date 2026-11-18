//! Récap de la semaine, du mois ou de l'année, sa story et leurs exports (1.1).

use crate::controller::{post, Controller};
use crate::ctl_bigpicture::local_offset_secs;
use crate::models::card_for;
use crate::{GameCard, KeyValue, RecapUi, StoryPage, TopGame};
use halcyon_core::recap::{self, Period, Play, Recap, Unlock};
use halcyon_core::{util, GameId};
use halcyon_storage::now_unix;
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

/// Mois en toutes lettres (titres du récap).
fn month_name(m: u32) -> String {
    match m {
        1 => trc!("month", "janvier"),
        2 => trc!("month", "février"),
        3 => trc!("month", "mars"),
        4 => trc!("month", "avril"),
        5 => trc!("month", "mai"),
        6 => trc!("month", "juin"),
        7 => trc!("month", "juillet"),
        8 => trc!("month", "août"),
        9 => trc!("month", "septembre"),
        10 => trc!("month", "octobre"),
        11 => trc!("month", "novembre"),
        _ => trc!("month", "décembre"),
    }
}

fn period_of(i: i32) -> Period {
    Period::ALL.get(i.max(0) as usize).copied().unwrap_or(Period::Week)
}

/// Libellé de la période (« Semaine du 11 nov. 2026 », « Novembre 2026 », « 2026 »).
fn period_label(r: &Recap, tz: i64) -> String {
    let (y, m, _) = util::civil_from_days((r.start + tz).div_euclid(86_400));
    match r.period {
        Period::Week => tr!("Semaine du {date}", date = util::format_date(r.start + tz)),
        Period::Month => halcyon_i18n::upper_first(&trc!("date", "{month} {year}", month = month_name(m), year = y)),
        Period::Year => y.to_string(),
    }
}

/// Comparaison avec la période précédente.
fn compare(r: &Recap) -> String {
    let previous = match r.period {
        Period::Week => tr!("la semaine précédente"),
        Period::Month => tr!("le mois précédent"),
        Period::Year => tr!("l'année précédente"),
    };
    match (r.total_secs, r.previous_secs) {
        (_, 0) => tr!("Rien de suivi sur {previous}.", previous = previous),
        (a, b) if a > b => tr!("{delta} de plus que {previous}.", delta = util::format_playtime(a - b), previous = previous),
        (a, b) if a < b => tr!("{delta} de moins que {previous}.", delta = util::format_playtime(b - a), previous = previous),
        _ => tr!("Autant que {previous}.", previous = previous),
    }
}

/// Rectangle de la story dans la fenêtre (pixels physiques) : même calcul que `StoryOverlay`.
fn card_rect(width: f32, height: f32, scale: f32) -> (u32, u32, u32, u32) {
    let (w, h) = (width / scale, height / scale);
    let card_h = (h - 120.0).min((w - 80.0) * 16.0 / 9.0).max(1.0);
    let card_w = card_h * 9.0 / 16.0;
    let x = (w - card_w) / 2.0;
    let y = (h - card_h) / 2.0 - 24.0;
    ((x * scale).round().max(0.0) as u32, (y * scale).round().max(0.0) as u32, (card_w * scale).round() as u32, (card_h * scale).round() as u32)
}

fn ffmpeg() -> Option<PathBuf> {
    let name = if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" };
    std::env::var_os("PATH").and_then(|p| std::env::split_paths(&p).map(|d| d.join(name)).find(|p| p.is_file()))
}

/// Dossier des exports (Images/Halcyon ou Vidéos/Halcyon).
fn export_dir(video: bool) -> PathBuf {
    let base = if video { dirs::video_dir() } else { dirs::picture_dir() };
    base.or_else(dirs::home_dir).unwrap_or_default().join("Halcyon")
}

impl Controller {
    pub(crate) fn wire_recap_with(self: &Rc<Self>, app: &crate::AppWindow) {
        let ui = app.global::<RecapUi>();
        let weak = Rc::downgrade(self);
        ui.on_changed({
            let weak = weak.clone();
            move || {
                if let Some(c) = weak.upgrade() {
                    c.refresh_recap();
                }
            }
        });
        ui.on_export_image({
            let weak = weak.clone();
            move || {
                if let Some(c) = weak.upgrade() {
                    c.export_story_image();
                }
            }
        });
        ui.on_export_video(move || {
            if let Some(c) = weak.upgrade() {
                c.export_story_video();
            }
        });
    }

    fn current_recap(&self, period: Period, offset: i64) -> Recap {
        let now = now_unix();
        let tz = local_offset_secs();
        let plays: Vec<Play> =
            self.db().sessions().unwrap_or_default().into_iter().map(|s| Play { game: s.game_id, started_at: s.started_at, secs: s.duration_secs }).collect();
        let (start, end) = recap::bounds(period, now, tz, offset);
        let unlocks: Vec<Unlock> = self
            .db()
            .unlocks_between(start, end)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|a| Some(Unlock { game: a.game_id, name: a.data.name.clone(), at: a.data.unlocked_at?, rarity: a.data.rarity }))
            .collect();
        recap::compute(period, offset, &plays, &unlocks, now, tz)
    }

    pub(crate) fn refresh_recap(&self) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<RecapUi>();
        if ui.get_offset() > 0 {
            ui.set_offset(0);
        }
        let r = self.current_recap(period_of(ui.get_period()), i64::from(ui.get_offset()));
        let tz = local_offset_secs();
        let now = now_unix();
        let lib = self.lib.borrow();
        let name = |id: GameId| lib.get(id).map(|g| g.name.clone()).unwrap_or_else(|| tr!("Jeu supprimé"));
        let card = |id: GameId| lib.get(id).map(|g| card_for(g, &self.images, now));
        ui.set_label(period_label(&r, tz).into());
        ui.set_has_data(r.total_secs > 0);
        ui.set_total(util::format_playtime(r.total_secs).into());
        ui.set_compare(compare(&r).into());
        ui.set_sessions(r.sessions as i32);
        ui.set_days(tr!("{played} / {total}", played = r.days_played, total = r.days_in_period).into());
        ui.set_streak(r.best_streak as i32);
        ui.set_new_games(r.new_games as i32);
        ui.set_unlocks(r.unlocks as i32);
        let max = r.top.first().map(|t| t.1).unwrap_or(1).max(1);
        let top: Vec<TopGame> = r
            .top
            .iter()
            .take(5)
            .filter_map(|(id, secs, n)| {
                Some(TopGame {
                    id: *id as i32,
                    title: name(*id).into(),
                    value: util::format_playtime(*secs).into(),
                    origin: trn!(*n as u64, "{n} session", "{n} sessions").into(),
                    ratio: *secs as f32 / max as f32,
                    card: card(*id)?,
                })
            })
            .collect();
        ui.set_top(ModelRc::new(VecModel::from(top)));
        let mut facts = Vec::new();
        if let (Some(d), Some(p)) = (r.weekday, r.day_part) {
            facts.push(KeyValue { label: tr!("Moment préféré").into(), value: tr!("le {day}, surtout {part}", day = recap::weekday_name(d), part = p.label()).into(), ratio: 0.0 });
        }
        if let Some((g, secs, at)) = r.longest {
            facts.push(KeyValue {
                label: tr!("Plus longue session").into(),
                value: tr!("{time} sur {game}, le {date}", time = util::format_playtime(secs), game = name(g), date = util::format_date(at + tz)).into(),
                ratio: 0.0,
            });
        }
        if let Some(u) = &r.rarest {
            facts.push(KeyValue {
                label: tr!("Succès le plus rare").into(),
                value: tr!("{name} ({game}), {pct} des joueurs", name = u.name, game = name(u.game), pct = format!("{} %", halcyon_i18n::decimal(f64::from(u.rarity.unwrap_or(0.0)), 1))).into(),
                ratio: 0.0,
            });
        }
        ui.set_facts(ModelRc::new(VecModel::from(facts)));
        ui.set_profile_title(r.profile.title().into());
        ui.set_profile_text(r.profile.description().into());
        // Story : couleurs du jeu le plus joué.
        if let Some(c) = r.top.first().and_then(|t| card(t.0)) {
            ui.set_color_a(c.color_a);
            ui.set_color_b(c.color_b);
        }
        drop(lib);
        let pages = self.story_pages(&r, tz);
        if ui.get_story_index() as usize >= pages.len() {
            ui.set_story_index(0);
        }
        ui.set_pages(ModelRc::new(VecModel::from(pages)));
    }

    fn story_pages(&self, r: &Recap, tz: i64) -> Vec<StoryPage> {
        let now = now_unix();
        let lib = self.lib.borrow();
        let name = |id: GameId| lib.get(id).map(|g| g.name.clone()).unwrap_or_else(|| tr!("Jeu supprimé"));
        let card = |id: GameId| lib.get(id).map(|g| card_for(g, &self.images, now));
        let empty_card = GameCard::default();
        let page = |kicker: String, value: String, title: String, text: String, card: Option<GameCard>, lines: Vec<String>| StoryPage {
            kicker: kicker.into(),
            value: value.into(),
            title: title.into(),
            text: text.into(),
            has_card: card.is_some(),
            card: card.unwrap_or_else(|| empty_card.clone()),
            lines: ModelRc::new(VecModel::from(lines.into_iter().map(SharedString::from).collect::<Vec<_>>())),
        };
        let label = period_label(r, tz);
        let mut pages = vec![page(
            match r.period {
                Period::Week => tr!("Votre semaine"),
                Period::Month => tr!("Votre mois"),
                Period::Year => tr!("Votre année"),
            },
            util::format_playtime(r.total_secs),
            tr!("de jeu · {label}", label = label),
            compare(r),
            None,
            Vec::new(),
        )];
        if let Some((g, secs, n)) = r.top.first() {
            pages.push(page(
                tr!("Votre jeu n° 1"),
                util::format_playtime(*secs),
                name(*g),
                trn!(*n as u64, "{n} session", "{n} sessions"),
                card(*g),
                Vec::new(),
            ));
        }
        if r.top.len() > 1 {
            let lines = r.top.iter().take(5).map(|(g, s, _)| format!("{} · {}", name(*g), util::format_playtime(*s))).collect();
            pages.push(page(tr!("Votre top 5"), String::new(), tr!("Les plus joués"), String::new(), None, lines));
        }
        if let (Some(d), Some(p)) = (r.weekday, r.day_part) {
            pages.push(page(
                tr!("Votre rythme"),
                tr!("{n} j", n = r.days_played),
                tr!("de jeu sur {total}", total = trn!(r.days_in_period as u64, "{n} jour", "{n} jours")),
                tr!("Votre jour : le {day}, surtout {part}. Plus longue série : {streak}.", day = recap::weekday_name(d), part = p.label(), streak = trn!(r.best_streak as u64, "{n} jour", "{n} jours")),
                None,
                Vec::new(),
            ));
        }
        if let Some((g, secs, at)) = r.longest {
            pages.push(page(tr!("Plus longue session"), util::format_playtime(secs), name(g), util::format_date(at + tz), card(g), Vec::new()));
        }
        if r.unlocks > 0 {
            let text = r
                .rarest
                .as_ref()
                .map(|u| tr!("Le plus rare : {name} ({game}), {pct} des joueurs.", name = u.name, game = name(u.game), pct = format!("{} %", halcyon_i18n::decimal(f64::from(u.rarity.unwrap_or(0.0)), 1))))
                .unwrap_or_default();
            pages.push(page(tr!("Succès"), r.unlocks.to_string(), trn!(r.unlocks as u64, "succès débloqué", "succès débloqués"), text, None, Vec::new()));
        }
        if r.new_games > 0 {
            pages.push(page(tr!("Découvertes"), r.new_games.to_string(), trn!(r.new_games as u64, "nouveau jeu", "nouveaux jeux"), String::new(), None, Vec::new()));
        }
        pages.push(page(tr!("Votre profil de joueur"), String::new(), r.profile.title(), r.profile.description(), None, Vec::new()));
        let mut summary = vec![
            tr!("Temps de jeu : {v}", v = util::format_playtime(r.total_secs)),
            trn!(r.sessions as u64, "{n} session", "{n} sessions"),
        ];
        if let Some((g, _, _)) = r.top.first() {
            summary.push(tr!("Jeu n° 1 : {v}", v = name(*g)));
        }
        if r.unlocks > 0 {
            summary.push(trn!(r.unlocks as u64, "{n} succès débloqué", "{n} succès débloqués"));
        }
        summary.push(r.profile.title());
        pages.push(page(tr!("En résumé"), String::new(), label, String::new(), None, summary));
        pages
    }

    /// Capture de la page affichée de la story (pixels de la fenêtre, recadrés sur la story).
    fn snapshot_story(&self) -> Result<image::RgbaImage, String> {
        let app = self.app().ok_or_else(|| tr!("fenêtre indisponible"))?;
        let shot = app.window().take_snapshot().map_err(|e| e.to_string())?;
        let (w, h) = (shot.width(), shot.height());
        let img = image::RgbaImage::from_raw(w, h, shot.as_bytes().to_vec()).ok_or_else(|| tr!("capture illisible"))?;
        let (x, y, cw, ch) = card_rect(w as f32, h as f32, app.window().scale_factor());
        if x + cw > w || y + ch > h || cw == 0 || ch == 0 {
            return Err(tr!("fenêtre trop petite pour la capture"));
        }
        Ok(image::imageops::crop_imm(&img, x, y, cw, ch).to_image())
    }

    fn story_file_stem(&self) -> String {
        let Some(app) = self.app() else { return "recap".into() };
        let ui = app.global::<RecapUi>();
        let r = recap::bounds(period_of(ui.get_period()), now_unix(), local_offset_secs(), i64::from(ui.get_offset()));
        let (y, m, d) = util::civil_from_days((r.0 + local_offset_secs()).div_euclid(86_400));
        let kind = ["semaine", "mois", "annee"].get(ui.get_period().max(0) as usize).copied().unwrap_or("semaine");
        format!("halcyon-recap-{kind}-{y:04}-{m:02}-{d:02}")
    }

    pub(crate) fn export_story_image(&self) {
        let Some(app) = self.app() else { return };
        let index = app.global::<RecapUi>().get_story_index() + 1;
        let dir = export_dir(false);
        let path = dir.join(format!("{}-{index}.png", self.story_file_stem()));
        let result = self.snapshot_story().and_then(|img| {
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            img.save(&path).map_err(|e| e.to_string())
        });
        match result {
            Ok(()) => self.toast(tr!("Image enregistrée : {path}", path = path.display()), false),
            Err(e) => self.toast(tr!("Image non enregistrée : {e}", e = e), true),
        }
    }

    /// Vidéo verticale : chaque page est capturée, puis assemblée par ffmpeg (s'il est
    /// installé) en 1080 × 1920 avec un fondu entre les pages. Sans ffmpeg, les pages
    /// restent disponibles en images.
    pub(crate) fn export_story_video(self: &Rc<Self>) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<RecapUi>();
        if ui.get_exporting() {
            return;
        }
        let pages = ui.get_pages().row_count();
        if pages == 0 {
            return;
        }
        let work = std::env::temp_dir().join(format!("halcyon-story-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&work);
        if let Err(e) = std::fs::create_dir_all(&work) {
            return self.toast(tr!("Vidéo impossible : {e}", e = e), true);
        }
        ui.set_exporting(true);
        ui.set_story_index(0);
        self.capture_story_page(0, pages, work);
    }

    fn capture_story_page(self: &Rc<Self>, index: usize, pages: usize, work: PathBuf) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<RecapUi>();
        ui.set_story_index(index as i32);
        ui.set_export_status(tr!("Capture de la page {i} sur {n}…", i = index + 1, n = pages).into());
        let weak = Rc::downgrade(self);
        // Laisse l'animation d'entrée se terminer avant la capture.
        slint::Timer::single_shot(Duration::from_millis(650), move || {
            let Some(c) = weak.upgrade() else { return };
            let saved = c.snapshot_story().and_then(|img| img.save(work.join(format!("page-{index:02}.png"))).map_err(|e| e.to_string()));
            if let Err(e) = saved {
                c.finish_story_video(Err(e), &work);
                return;
            }
            if index + 1 < pages {
                c.capture_story_page(index + 1, pages, work);
            } else {
                c.encode_story_video(pages, work);
            }
        });
    }

    fn encode_story_video(self: &Rc<Self>, pages: usize, work: PathBuf) {
        let Some(app) = self.app() else { return };
        app.global::<RecapUi>().set_export_status(tr!("Assemblage de la vidéo…").into());
        let dir = export_dir(true);
        let out = dir.join(format!("{}.mp4", self.story_file_stem()));
        let images = export_dir(false).join(self.story_file_stem());
        std::thread::spawn(move || {
            let result = match ffmpeg() {
                Some(ff) => std::fs::create_dir_all(&dir).map_err(|e| e.to_string()).and_then(|_| encode(&ff, &work, pages, &out)).map(|_| out.clone()),
                // Sans ffmpeg : les pages sont gardées en images.
                None => copy_pages(&work, pages, &images).and_then(|_| Err(tr!("ffmpeg est introuvable : les {n} pages ont été enregistrées en images dans {dir}", n = pages, dir = images.display()))),
            };
            post(move |c| c.finish_story_video(result, &work));
        });
    }

    fn finish_story_video(&self, result: Result<PathBuf, String>, work: &Path) {
        let _ = std::fs::remove_dir_all(work);
        if let Some(app) = self.app() {
            let ui = app.global::<RecapUi>();
            ui.set_exporting(false);
            ui.set_story_index(0);
        }
        match result {
            Ok(p) => self.toast(tr!("Vidéo enregistrée : {path}", path = p.display()), false),
            Err(e) => self.toast(e, true),
        }
    }

    /// Au démarrage : signale le récap de la semaine écoulée, une seule fois.
    pub(crate) fn announce_recap(&self) {
        let r = self.current_recap(Period::Week, -1);
        if r.total_secs == 0 || self.settings.borrow().recap_announced >= r.start {
            return;
        }
        self.settings.borrow_mut().recap_announced = r.start;
        self.save_settings();
        let msg = tr!("Votre récap de la semaine est prêt : {time} de jeu.", time = util::format_playtime(r.total_secs));
        self.toast(msg.clone(), false);
        if self.active_session.get().is_some() || self.app().is_none_or(|a| !a.window().is_visible()) {
            crate::notify::info("Halcyon", &msg);
        }
    }
}

fn copy_pages(work: &Path, pages: usize, dest: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    for i in 0..pages {
        std::fs::copy(work.join(format!("page-{i:02}.png")), dest.join(format!("page-{:02}.png", i + 1))).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Assemble les pages (3 s chacune, fondu de 0,5 s) en vidéo H.264 1080 × 1920.
fn encode(ffmpeg: &Path, work: &Path, pages: usize, out: &Path) -> Result<(), String> {
    const SHOW: f32 = 3.0;
    const FADE: f32 = 0.5;
    let mut cmd = std::process::Command::new(ffmpeg);
    cmd.arg("-y").arg("-loglevel").arg("error");
    for i in 0..pages {
        cmd.args(["-loop", "1", "-t", &format!("{SHOW}"), "-i"]).arg(work.join(format!("page-{i:02}.png")));
    }
    let mut filter = String::new();
    for i in 0..pages {
        filter.push_str(&format!(
            "[{i}:v]scale=1080:1920:force_original_aspect_ratio=decrease,pad=1080:1920:(ow-iw)/2:(oh-ih)/2:color=0x0b0d12,setsar=1,fps=30,format=yuv420p[p{i}];"
        ));
    }
    let mut last = "p0".to_string();
    for i in 1..pages {
        let offset = i as f32 * (SHOW - FADE);
        filter.push_str(&format!("[{last}][p{i}]xfade=transition=fade:duration={FADE}:offset={offset}[x{i}];"));
        last = format!("x{i}");
    }
    let filter = filter.trim_end_matches(';').to_string();
    cmd.args(["-filter_complex", &filter, "-map", &format!("[{last}]"), "-c:v", "libx264", "-preset", "veryfast", "-crf", "20", "-pix_fmt", "yuv420p", "-movflags", "+faststart"]).arg(out);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let output = cmd.output().map_err(|e| tr!("ffmpeg : {e}", e = e))?;
    if output.status.success() && out.is_file() {
        Ok(())
    } else {
        Err(tr!("ffmpeg a échoué : {e}", e = String::from_utf8_lossy(&output.stderr).lines().last().unwrap_or_default()))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn card_geometry_matches_overlay() {
        // Fenêtre 1440 × 900 (logique), facteur 2.
        let (x, y, w, h) = super::card_rect(2880.0, 1800.0, 2.0);
        assert_eq!((w, h), (878, 1560));
        assert_eq!((x, y), (1001, 72));
    }
}
