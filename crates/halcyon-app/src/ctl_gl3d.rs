//! Mode console en vraie 3D (1.0.0-alpha.7) : relie le moteur `gl3d` à Slint et aux vues.
//!
//! - Slint appelle notre fonction avant chaque image (`set_rendering_notifier`) dans son
//!   contexte OpenGL ; si la vue courante est en 3D, la scène est construite depuis l'état du
//!   mode console (étagère, musée, Inspecter), dessinée dans une texture, puis montrée par
//!   l'image `ShelfUi.scene`.
//! - Les animations sont calculées ici (`gl3d::anim`) ; tant qu'une valeur bouge, une nouvelle
//!   image est demandée ; au repos, plus rien n'est redessiné.
//! - Faces : jaquette (grande pour le boîtier choisi), tranche et dos réels (LaunchBox) quand ils
//!   existent, sinon composés une fois (`gl3d::raster`), au plus quelques-uns par image.
//! - Repli : rendu logiciel (pas d'OpenGL), OpenGL < 3, mode Performance, option coupée → la
//!   2,5D de la 1.0.0-alpha.7, inchangée.

use crate::controller::Controller;
use crate::ctl_shelf::{band_color, window};
use crate::gl3d::anim::{rate, Smooth};
use crate::gl3d::math::{normalize, project, Mat4};
use crate::gl3d::raster::{self, CaseInfo};
use crate::gl3d::renderer::{Face, Geom, Item, Light, Pixels, Renderer, Scene, TexKey, BACK, BOTTOM, EDGE, FRONT, SPINE, TOP};
use crate::gl3d::{self, D, H, PITCH, ROW_H, SHELF_FRONT, W};
use crate::images::{ImageKey, Kind};
use crate::models::{placeholder_colors, platform_names};
use crate::{AppWindow, MuseumUi, ShelfUi};
use halcyon_core::{Game, GameId};
use slint::ComponentHandle;
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::rc::Rc;
use std::time::Instant;

/// Scène construite : la scène, les boîtiers cliquables, « encore en mouvement ».
type Built = Option<(Scene, Vec<(Mat4, Hit)>, bool)>;

/// Faces générées par image au plus (le reste suit aux images suivantes).
const GEN_PER_FRAME: usize = 6;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hit {
    Shelf(usize),
    Museum(usize, usize),
}

#[derive(Debug)]
pub struct Inspect {
    pub game: GameId,
    pub yaw: Smooth,
    pub pitch: Smooth,
    pub dist: Smooth,
    pub lid: Smooth,
    pub drag: Option<(f32, f32)>,
}

#[derive(Default)]
pub struct View3d {
    /// Rendu impossible (rendu logiciel, OpenGL trop ancien, erreur) : 2,5D jusqu'à la fin.
    pub failed: bool,
    pub ready: bool,
    pub shelf_pos: Smooth,
    pub shelf_index: Option<usize>,
    pub flips: HashMap<GameId, Smooth>,
    pub outs: HashMap<GameId, Smooth>,
    pub cam_x: Smooth,
    pub cam_y: Smooth,
    pub museum_seen: bool,
    pub inspect: Option<Inspect>,
    pub last: Option<Instant>,
    /// Boîtiers à l'écran (pixels physiques) : x0, y0, x1, y1, profondeur, cible.
    pub hits: Vec<(f32, f32, f32, f32, f32, Hit)>,
    pub shown: Option<(NonZeroU32, u32, u32)>,
    /// Textures à oublier (visuel remplacé), traitées à l'image suivante.
    pub forget: Vec<GameId>,
    pub published: [f32; 6],
    /// Scène à redessiner (état changé) ; sinon la texture précédente est réutilisée quand
    /// Slint redessine pour autre chose (halo, horloge) : aucun rendu 3D inutile.
    pub dirty: bool,
    pub animating: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Off,
    Shelf,
    Museum,
    Inspect,
}

/// Parties d'un boîtier (clé de texture = identifiant du jeu × 16 + partie).
const P_FRONT_LARGE: u64 = 1;
const P_FRONT: u64 = 2;
const P_FRONT_GEN: u64 = 3;
const P_SPINE: u64 = 4;
const P_SPINE_GEN: u64 = 5;
const P_BACK: u64 = 6;
const P_BACK_GEN: u64 = 7;

fn key(id: GameId, part: u64) -> TexKey {
    (id as u64) << 4 | part
}

fn pixels(img: &slint::Image) -> Option<Pixels> {
    let b = img.to_rgba8()?;
    Some(Pixels { w: b.width(), h: b.height(), data: b.as_bytes().to_vec() })
}

fn canvas_pixels(c: raster::Canvas) -> Pixels {
    Pixels { w: c.w, h: c.h, data: c.px }
}

fn rgb(c: slint::Color) -> [f32; 3] {
    [f32::from(c.red()) / 255., f32::from(c.green()) / 255., f32::from(c.blue()) / 255.]
}

impl Controller {
    /// Branche le rendu 3D sur la fenêtre (une fois, au démarrage).
    pub(crate) fn gl3d_install(self: &Rc<Self>, app: &AppWindow) {
        let weak = Rc::downgrade(self);
        let mut renderer: Option<Renderer> = None;
        let mut broken = false;
        let r = app.window().set_rendering_notifier(move |state, api| {
            let Some(c) = weak.upgrade() else { return };
            match state {
                slint::RenderingState::BeforeRendering => {
                    if broken || !c.gl3d_wanted() {
                        return;
                    }
                    if renderer.is_none() {
                        let made = match api {
                            slint::GraphicsAPI::NativeOpenGL { get_proc_address } => Renderer::new(get_proc_address),
                            _ => Err("pas d'OpenGL".into()),
                        };
                        match made {
                            Ok(r) => {
                                log::info!("3D : {}", r.info);
                                renderer = Some(r);
                            }
                            Err(e) => {
                                log::warn!("3D indisponible, étagère en 2,5D : {e}");
                                broken = true;
                                c.gl3d_unavailable();
                                return;
                            }
                        }
                    }
                    if let Some(r) = renderer.as_mut() {
                        if let Err(e) = c.gl3d_frame(r) {
                            log::warn!("3D : {e} ; retour à la 2,5D");
                            broken = true;
                            r.teardown();
                            renderer = None;
                            c.gl3d_unavailable();
                        }
                    }
                }
                slint::RenderingState::RenderingTeardown => {
                    if let Some(mut r) = renderer.take() {
                        r.teardown();
                    }
                    c.view3d.borrow_mut().shown = None;
                }
                _ => {}
            }
        });
        if r.is_err() {
            // Rendu logiciel : pas de contexte OpenGL.
            self.view3d.borrow_mut().failed = true;
        }
    }

    fn gl3d_unavailable(&self) {
        self.view3d.borrow_mut().failed = true;
        if let Some(app) = self.app() {
            app.global::<ShelfUi>().set_gl3d(false);
            app.global::<ShelfUi>().set_gl3d_ok(false);
        }
    }

    /// La 3D est-elle possible ici (option, effets, OpenGL) ?
    pub(crate) fn gl3d_allowed(&self) -> bool {
        !self.view3d.borrow().failed
            && self.settings.borrow().console_3d
            && self.app().is_some_and(|a| a.global::<crate::Theme>().get_effects())
    }

    fn gl3d_mode(&self) -> Mode {
        if !self.gl3d_allowed() {
            return Mode::Off;
        }
        let bp = self.bp.borrow();
        if !bp.active {
            return Mode::Off;
        }
        if self.view3d.borrow().inspect.is_some() {
            return Mode::Inspect;
        }
        if bp.section != 0 || bp.sheet.is_some() {
            return Mode::Off;
        }
        match self.settings.borrow().console_view {
            1 => Mode::Shelf,
            2 => Mode::Museum,
            _ => Mode::Off,
        }
    }

    fn gl3d_wanted(&self) -> bool {
        self.gl3d_mode() != Mode::Off
    }

    /// Demande une image (l'état du mode console a changé).
    pub(crate) fn gl3d_redraw(&self) {
        let Some(app) = self.app() else { return };
        // 3D coupée (option, mode Performance) : retour immédiat à la 2,5D.
        if !self.gl3d_allowed() {
            let ui = app.global::<ShelfUi>();
            if ui.get_gl3d() {
                ui.set_gl3d(false);
                self.view3d.borrow_mut().shown = None;
            }
            if self.view3d.borrow().inspect.is_some() {
                self.view3d.borrow_mut().inspect = None;
                ui.set_inspect(false);
            }
            return;
        }
        if self.gl3d_wanted() {
            self.view3d.borrow_mut().dirty = true;
            app.window().request_redraw();
        }
    }

    /// Oublie les textures d'un jeu (visuel remplacé).
    pub(crate) fn gl3d_forget(&self, id: GameId) {
        {
            let mut v = self.view3d.borrow_mut();
            v.forget.push(id);
            v.dirty = true;
        }
        self.gl3d_redraw();
    }

    /// Une image : construit la scène de la vue courante, la dessine, publie la texture et les
    /// positions des éléments Slint posés dessus.
    fn gl3d_frame(&self, r: &mut Renderer) -> Result<(), String> {
        let Some(app) = self.app() else { return Ok(()) };
        let mode = self.gl3d_mode();
        let size = app.window().size();
        let scale = app.window().scale_factor();
        let (w, h) = (size.width.max(1), size.height.max(1));
        let motion = app.global::<crate::Theme>().get_motion();
        {
            // Rien n'a changé : l'image précédente suffit.
            let v = self.view3d.borrow();
            let same_size = v.shown.is_some_and(|(_, sw, sh)| sw == w && sh == h);
            if same_size && !v.dirty && !v.animating && v.forget.is_empty() {
                return Ok(());
            }
        }
        let dt = {
            let mut v = self.view3d.borrow_mut();
            let now = Instant::now();
            let dt = v.last.map(|l| now.duration_since(l).as_secs_f32()).unwrap_or(1.0 / 60.0);
            v.last = Some(now);
            for id in std::mem::take(&mut v.forget) {
                for p in [P_FRONT_LARGE, P_FRONT, P_FRONT_GEN, P_SPINE, P_SPINE_GEN, P_BACK, P_BACK_GEN] {
                    r.forget(key(id, p));
                }
            }
            dt.min(0.1)
        };
        let rt = rate(motion);
        let mut budget = GEN_PER_FRAME;
        let aspect = w as f32 / h as f32;
        let built = match mode {
            Mode::Off => return Ok(()),
            Mode::Shelf => self.gl3d_shelf(r, dt, rt, aspect, &mut budget),
            Mode::Museum => self.gl3d_museum(r, dt, rt, aspect, &mut budget),
            Mode::Inspect => self.gl3d_inspect(r, dt, rt, aspect, &mut budget),
        };
        let Some((scene, hits, moving)) = built else { return Ok(()) };
        let tex = r.render(&scene, w, h)?;
        // Zones cliquables et positions des plaques / cartes (pixels logiques pour Slint).
        let (wf, hf) = (w as f32, h as f32);
        let mut rects = Vec::new();
        for (m, hit) in hits {
            if let Some((x0, y0, x1, y1, z)) = gl3d::screen_rect(&scene.view_proj, &m, wf, hf) {
                rects.push((x0, y0, x1, y1, z, hit));
            }
        }
        let px = |p: [f32; 3]| project(&scene.view_proj, p, wf, hf).map(|(x, y, _)| (x / scale, y / scale)).unwrap_or((0., 0.));
        let mut published = [0f32; 6];
        match mode {
            Mode::Shelf => published[0] = px([0., 0., SHELF_FRONT]).1,
            Mode::Museum => {
                let (_, y0) = px([0., 0., 0.5]);
                let (_, y1) = px([0., -ROW_H, 0.5]);
                published[1] = y0;
                published[2] = y1 - y0;
                let (cx, cy) = self.museum_card_anchor().map(px).unwrap_or((0., 0.));
                published[3] = cx;
                published[4] = cy;
            }
            _ => {}
        }
        let (changed, show) = {
            let mut v = self.view3d.borrow_mut();
            v.hits = rects;
            let show = v.shown != Some((tex, w, h));
            v.shown = Some((tex, w, h));
            let changed = published.iter().zip(v.published.iter()).any(|(a, b)| (a - b).abs() > 0.25);
            v.published = published;
            v.ready = true;
            v.dirty = false;
            v.animating = moving || budget == 0;
            (changed, show)
        };
        let ui = app.global::<ShelfUi>();
        if show {
            #[allow(unsafe_code)]
            let img = unsafe { slint::BorrowedOpenGLTextureBuilder::new_gl_2d_rgba_texture(tex, (w, h).into()) }
                .origin(slint::BorrowedOpenGLTextureOrigin::BottomLeft)
                .build();
            ui.set_scene(img);
        }
        if !ui.get_gl3d() {
            ui.set_gl3d(true);
            ui.set_gl3d_ok(true);
        }
        if changed {
            match mode {
                Mode::Shelf => ui.set_plank_y(published[0]),
                Mode::Museum => {
                    let m = app.global::<MuseumUi>();
                    m.set_row_top(published[1]);
                    m.set_row_step(published[2]);
                    m.set_card_x(published[3]);
                    m.set_card_y(published[4]);
                }
                _ => {}
            }
        }
        if moving || budget == 0 {
            app.window().request_redraw();
        }
        Ok(())
    }

    /// Ce qu'il faut pour dessiner les faces générées d'un jeu.
    fn case_info(&self, g: &Game) -> CaseInfo {
        let (a, b) = placeholder_colors(&g.name);
        let mark = match &g.platform {
            Some(p) => platform_names(p).0,
            None => g.source.label().to_string(),
        };
        let mut lines = Vec::new();
        if !g.genres.is_empty() {
            lines.push(g.genres.join(" · "));
        }
        let credit: Vec<String> = [g.developer.clone(), g.release_date.clone()].into_iter().flatten().filter(|s| !s.is_empty()).collect();
        if !credit.is_empty() {
            lines.push(credit.join(" · "));
        }
        let pt = g.effective_playtime();
        if pt > 0 {
            lines.push(tr!("Temps de jeu : {t}", t = halcyon_core::util::format_playtime(pt)));
        }
        CaseInfo {
            title: g.name.clone(),
            initials: halcyon_core::util::initials(&g.name),
            mark,
            color_a: raster::rgba(a),
            color_b: raster::rgba(b),
            band: raster::rgba(band_color(g)),
            description: g.description.clone().filter(|d| !d.trim().is_empty()).unwrap_or_else(|| tr!("Aucun résumé pour ce jeu.")),
            lines,
        }
    }

    /// Faces d'un boîtier : jaquette (réelle, sinon générée), tranche et dos (réels, sinon
    /// composés), chants et dessus en plastique.
    fn case_faces(&self, r: &mut Renderer, g: &Game, large: bool, need_back: bool, budget: &mut usize) -> [Face; 6] {
        let (ca, cb) = placeholder_colors(&g.name);
        let base = rgb(cb);
        let dark = [base[0] * 0.35 + 0.04, base[1] * 0.35 + 0.04, base[2] * 0.35 + 0.05];
        let mut info: Option<CaseInfo> = None;
        let mut gen = |r: &mut Renderer, k: TexKey, budget: &mut usize, f: &dyn Fn(&CaseInfo) -> raster::Canvas| -> bool {
            if r.has_texture(k) {
                return r.texture(k, || None);
            }
            if *budget == 0 {
                return false;
            }
            *budget -= 1;
            let i = info.get_or_insert_with(|| self.case_info(g)).clone();
            r.texture(k, || Some(canvas_pixels(f(&i))))
        };
        let large_ok = large
            && r.texture(key(g.id, P_FRONT_LARGE), || {
                self.images.get(ImageKey { game: g.id, kind: Kind::CoverLarge }, g.artwork.cover.as_deref()).and_then(|i| pixels(&i))
            });
        let front = if large_ok {
            Face::tex(key(g.id, P_FRONT_LARGE), 1.0)
        } else if r.texture(key(g.id, P_FRONT), || self.images.get(ImageKey { game: g.id, kind: Kind::Cover }, g.artwork.cover.as_deref()).and_then(|i| pixels(&i))) {
            Face::tex(key(g.id, P_FRONT), 1.0)
        } else if gen(r, key(g.id, P_FRONT_GEN), budget, &raster::cover) {
            Face::tex(key(g.id, P_FRONT_GEN), 1.0)
        } else {
            Face::plain(rgb(ca), 1.0)
        };
        let spine = if r.texture(key(g.id, P_SPINE), || self.case_spine(g).and_then(|i| pixels(&i))) {
            Face::tex(key(g.id, P_SPINE), 0.6)
        } else if gen(r, key(g.id, P_SPINE_GEN), budget, &raster::spine) {
            Face::tex(key(g.id, P_SPINE_GEN), 0.6)
        } else {
            Face::plain(rgb(band_color(g)), 0.6)
        };
        let back = if !need_back {
            Face::plain(dark, 0.8)
        } else if r.texture(key(g.id, P_BACK), || self.case_back(g).and_then(|i| pixels(&i))) {
            Face::tex(key(g.id, P_BACK), 1.0)
        } else {
            let picture = self
                .images
                .get(ImageKey { game: g.id, kind: Kind::Cover }, g.artwork.cover.as_deref())
                .and_then(|i| i.to_rgba8())
                .map(|b| raster::Canvas { w: b.width(), h: b.height(), px: b.as_bytes().to_vec() });
            let make = move |i: &CaseInfo| raster::back(i, picture.as_ref());
            if gen(r, key(g.id, P_BACK_GEN), budget, &make) {
                Face::tex(key(g.id, P_BACK_GEN), 1.0)
            } else {
                Face::plain(dark, 0.8)
            }
        };
        let mut faces = [Face::plain(dark, 0.7); 6];
        faces[FRONT] = front;
        faces[BACK] = back;
        faces[SPINE] = spine;
        faces[EDGE] = Face::plain([dark[0] * 1.3, dark[1] * 1.3, dark[2] * 1.3], 0.5);
        faces[TOP] = Face::plain(dark, 0.6);
        faces[BOTTOM] = Face::plain(dark, 0.2);
        faces
    }

    fn gl3d_shelf(&self, r: &mut Renderer, dt: f32, rt: f32, aspect: f32, budget: &mut usize) -> Built {
        let (games, focus, index, flipped, color) = {
            let bp = self.bp.borrow();
            let st = &bp.shelf;
            let s = st.shelves.get(st.shelf)?;
            (s.games.clone(), st.pos.get(st.shelf).copied().unwrap_or(0), st.shelf, st.flipped, s.color)
        };
        let mut moving = false;
        let pos = {
            let mut v = self.view3d.borrow_mut();
            if v.shelf_index != Some(index) {
                v.shelf_index = Some(index);
                v.shelf_pos.snap(focus as f32);
                v.flips.clear();
            } else {
                v.shelf_pos.set(focus as f32);
            }
            moving |= v.shelf_pos.step(dt, rt);
            v.shelf_pos.value
        };
        let lib = self.lib.borrow();
        let mut items = Vec::new();
        let mut hits = Vec::new();
        for i in window(focus, games.len(), crate::ctl_shelf::SLOTS) {
            let Some(g) = games.get(i).and_then(|id| lib.get(*id)) else { continue };
            let k = i as f32 - pos;
            if k.abs() > 10.5 {
                continue;
            }
            let selected = i == focus;
            let flip = {
                let mut v = self.view3d.borrow_mut();
                let f = v.flips.entry(g.id).or_insert_with(|| Smooth::new(0.0));
                f.set(if selected && flipped { 1.0 } else { 0.0 });
                moving |= f.step(dt, rt * 0.6);
                f.value
            };
            let (model, dim) = gl3d::shelf_case(k, flip);
            let faces = self.case_faces(r, g, selected, flip > 0.01, budget);
            let ak = k.abs();
            items.push(Item {
                geom: Geom::Box,
                model,
                faces,
                dim,
                alpha: if ak > 9.0 { (10.0 - ak).max(0.0) } else { 1.0 },
                outline: if selected { 1.0 - ak.min(1.0) } else { 0.0 },
                outline_col: [1.0, 1.0, 1.0],
                reflect: true,
            });
            hits.push((model, Hit::Shelf(i)));
        }
        drop(lib);
        self.view3d.borrow_mut().flips.retain(|_, f| f.value > 0.001 || f.target > 0.0);
        let (view_proj, eye) = gl3d::shelf_camera(aspect);
        let c = rgb(color);
        let scene = Scene {
            view_proj,
            eye,
            light: Light {
                ambient: [0.5, 0.5, 0.53],
                key_dir: normalize([0.35, -0.6, -0.75]),
                key_col: [0.62, 0.6, 0.58],
                spot: Some(([0.0, 2.6, 3.2], [c[0] * 0.5 + 0.2, c[1] * 0.5 + 0.2, c[2] * 0.5 + 0.2])),
            },
            planks: vec![(Mat4::translate(0.0, -0.12, 0.05).mul(&Mat4::scale(18.0, 0.12, 3.0)), [0.075, 0.08, 0.095])],
            floor_y: 0.0,
            items,
        };
        Some((scene, hits, moving))
    }

    fn gl3d_museum(&self, r: &mut Renderer, dt: f32, rt: f32, aspect: f32, budget: &mut usize) -> Built {
        let (rows, row, pos, flipped, fresh_after) = {
            let bp = self.bp.borrow();
            let st = &bp.museum;
            if st.rows.is_empty() {
                return None;
            }
            (st.rows.clone(), st.row, st.pos.clone(), st.flipped, st.fresh_after)
        };
        let lighting = self.settings.borrow().museum_lighting.min(2);
        let mut moving = false;
        let (cam_x, cam_y) = {
            let mut v = self.view3d.borrow_mut();
            let tx = gl3d::museum_cam_x(pos.get(row).copied().unwrap_or(0));
            let ty = -gl3d::museum_center_row(row, rows.len()) * ROW_H;
            if !v.museum_seen {
                v.museum_seen = true;
                v.cam_x.snap(tx);
                v.cam_y.snap(ty);
                v.outs.clear();
            }
            v.cam_x.set(tx);
            v.cam_y.set(ty);
            moving |= v.cam_x.step(dt, rt);
            moving |= v.cam_y.step(dt, rt);
            (v.cam_x.value, v.cam_y.value)
        };
        let lib = self.lib.borrow();
        let mut items = Vec::new();
        let mut hits = Vec::new();
        let mut planks = Vec::new();
        let lo = row.saturating_sub(1).min(rows.len().saturating_sub(crate::ctl_museum::ROWS));
        let hi = (lo + crate::ctl_museum::ROWS).min(rows.len());
        let half_w = 10.5 * (20f32.to_radians()).tan() * aspect + 1.5;
        for (ri, rdef) in rows.iter().enumerate().take(hi).skip(lo) {
            let y = -(ri as f32) * ROW_H;
            let len = rdef.games.len();
            let focus = pos.get(ri).copied().unwrap_or(0);
            let active = ri == row;
            planks.push((
                Mat4::translate((len as f32 * PITCH) / 2.0 + 0.2, y - 0.1, 0.05).mul(&Mat4::scale(len as f32 * PITCH + 1.6, 0.1, 1.3)),
                if lighting == 1 { [0.42, 0.44, 0.48] } else { [0.2, 0.17, 0.14] },
            ));
            // Boîtiers à l'écran seulement (fenêtre autour de la caméra).
            let first = (((cam_x - half_w) / PITCH).floor().max(0.0)) as usize;
            let last = ((((cam_x + half_w) / PITCH).ceil().max(0.0)) as usize + 1).min(len);
            let mut shift = 0.0;
            for i in first..last {
                let Some(g) = rdef.games.get(i).and_then(|id| lib.get(*id)) else { continue };
                let is_focus = active && i == focus;
                let (out, flip) = {
                    let mut v = self.view3d.borrow_mut();
                    let o = v.outs.entry(g.id).or_insert_with(|| Smooth::new(0.0));
                    o.set(if is_focus { 1.0 } else { 0.0 });
                    moving |= o.step(dt, rt);
                    let out = o.value;
                    let f = v.flips.entry(g.id).or_insert_with(|| Smooth::new(0.0));
                    f.set(if is_focus && flipped { 1.0 } else { 0.0 });
                    moving |= f.step(dt, rt * 0.6);
                    (out, f.value)
                };
                let model = gl3d::museum_case(ri, i, out, shift, flip);
                shift += gl3d::museum_gap(out);
                let faces = self.case_faces(r, g, false, flip > 0.01, budget);
                let fresh = g.added_at > fresh_after;
                items.push(Item {
                    geom: Geom::Box,
                    model,
                    faces,
                    dim: if active { 0.0 } else { 0.28 },
                    alpha: 1.0,
                    outline: if is_focus { out } else if fresh { 0.8 } else { 0.0 },
                    outline_col: if is_focus { [1.0, 1.0, 1.0] } else { [0.95, 0.55, 0.2] },
                    reflect: false,
                });
                hits.push((model, Hit::Museum(ri, i)));
            }
        }
        drop(lib);
        {
            let mut v = self.view3d.borrow_mut();
            v.outs.retain(|_, o| o.value > 0.001 || o.target > 0.0);
            v.flips.retain(|_, f| f.value > 0.001 || f.target > 0.0);
        }
        let (view_proj, eye) = gl3d::museum_camera(aspect, cam_x, cam_y);
        let focus_x = pos.get(row).copied().unwrap_or(0) as f32 * PITCH + W / 2.0;
        let spot_at = [focus_x, -(row as f32) * ROW_H + 2.4, 2.6];
        let row_col = rgb(rows[row].color);
        let light = match lighting {
            // Galerie : pièce sombre, projecteur chaud sur le boîtier choisi.
            0 => Light { ambient: [0.3, 0.27, 0.24], key_dir: normalize([0.2, -0.7, -0.7]), key_col: [0.35, 0.32, 0.28], spot: Some((spot_at, [1.2, 1.0, 0.75])) },
            // Jour : lumière égale.
            1 => Light { ambient: [0.62, 0.62, 0.64], key_dir: normalize([0.3, -0.6, -0.75]), key_col: [0.5, 0.5, 0.5], spot: None },
            // Néon : pièce noire, projecteur de la couleur de la rangée.
            _ => Light {
                ambient: [0.16, 0.17, 0.22],
                key_dir: normalize([0.0, -0.6, -0.8]),
                key_col: [0.22, 0.22, 0.28],
                spot: Some((spot_at, [row_col[0] * 1.6 + 0.2, row_col[1] * 1.6 + 0.2, row_col[2] * 1.6 + 0.2])),
            },
        };
        Some((Scene { view_proj, eye, light, planks, floor_y: -100.0, items }, hits, moving))
    }

    /// Point sous le boîtier choisi du musée (pour la carte Slint).
    fn museum_card_anchor(&self) -> Option<[f32; 3]> {
        let bp = self.bp.borrow();
        let st = &bp.museum;
        let f = *st.pos.get(st.row)?;
        let x = f as f32 * PITCH + D / 2.0 + (W / 2.0 - D / 2.0 + 0.05);
        Some([x, -(st.row as f32) * ROW_H - 0.1, 0.55 + 0.5])
    }

    fn gl3d_inspect(&self, r: &mut Renderer, dt: f32, rt: f32, aspect: f32, budget: &mut usize) -> Built {
        let (game, yaw, pitch, dist, lid, moving) = {
            let mut v = self.view3d.borrow_mut();
            let ins = v.inspect.as_mut()?;
            let mut m = ins.yaw.step(dt, rt);
            m |= ins.pitch.step(dt, rt);
            m |= ins.dist.step(dt, rt);
            m |= ins.lid.step(dt, rt * 0.7);
            (ins.game, ins.yaw.value, ins.pitch.value, ins.dist.value, ins.lid.value, m)
        };
        let lib = self.lib.borrow();
        let g = lib.get(game)?.clone();
        drop(lib);
        let mut faces = self.case_faces(r, &g, true, true, budget);
        let cover = faces[FRONT];
        let base = gl3d::inspect_base(yaw, pitch);
        let mut items = Vec::new();
        if lid > 0.02 {
            // Ouvert : l'intérieur du boîtier, le couvercle, le disque.
            faces[FRONT] = Face::plain([0.09, 0.09, 0.1], 0.3);
            let mut lid_faces = [Face::plain([0.42, 0.43, 0.46], 0.5); 6];
            lid_faces[FRONT] = cover;
            items.push(Item { geom: Geom::Box, model: gl3d::inspect_lid(&base, lid), faces: lid_faces, dim: 0.0, alpha: 1.0, outline: 0.0, outline_col: [1.; 3], reflect: false });
            let mut disc = [cover; 6];
            disc[FRONT].gloss = 1.0;
            items.push(Item { geom: Geom::Disc, model: gl3d::inspect_disc(&base), faces: disc, dim: 0.0, alpha: lid.min(1.0), outline: 0.0, outline_col: [1.; 3], reflect: false });
        }
        items.insert(0, Item { geom: Geom::Box, model: base.mul(&Mat4::scale(W, H, D)), faces, dim: 0.0, alpha: 1.0, outline: 0.0, outline_col: [1.; 3], reflect: false });
        let (view_proj, eye) = gl3d::inspect_camera(aspect, dist);
        let scene = Scene {
            view_proj,
            eye,
            light: Light { ambient: [0.45, 0.45, 0.48], key_dir: normalize([0.45, -0.5, -0.75]), key_col: [0.7, 0.68, 0.64], spot: Some(([-2.0, 1.5, 3.0], [0.35, 0.38, 0.45])) },
            planks: Vec::new(),
            floor_y: -100.0,
            items,
        };
        Some((scene, Vec::new(), moving))
    }

    /// Clic ou survol sur la scène 3D (coordonnées logiques).
    pub(crate) fn gl3d_pointer(self: &Rc<Self>, kind: &str, x: f32, y: f32) -> bool {
        let scale = self.app().map(|a| a.window().scale_factor()).unwrap_or(1.0);
        let (px, py) = (x * scale, y * scale);
        let hit = {
            let v = self.view3d.borrow();
            v.hits.iter().filter(|h| px >= h.0 && px <= h.2 && py >= h.1 && py <= h.3).min_by(|a, b| a.4.partial_cmp(&b.4).unwrap_or(std::cmp::Ordering::Equal)).map(|h| h.5)
        };
        let Some(hit) = hit else { return false };
        match hit {
            Hit::Shelf(i) => {
                let action = match kind {
                    "move" => format!("shelf-hover:{i}"),
                    "click" => format!("shelf-click:{i}"),
                    _ => format!("shelf-play:{i}"),
                };
                self.shelf_input(&action)
            }
            Hit::Museum(r, i) => {
                if kind == "move" {
                    return false;
                }
                let action = if kind == "click" { format!("museum-click:{r}:{i}") } else { format!("museum-play:{r}:{i}") };
                self.museum_input(&action)
            }
        }
    }

    /// Ouvre « Inspecter » sur le boîtier choisi (étagère, musée).
    pub(crate) fn inspect_open(&self, id: GameId) {
        if !self.gl3d_allowed() {
            return;
        }
        self.view3d.borrow_mut().inspect = Some(Inspect {
            game: id,
            yaw: { let mut s = Smooth::new(0.35); s.set(0.0); s },
            pitch: Smooth::new(0.0),
            dist: { let mut s = Smooth::new(5.2); s.set(3.9); s },
            lid: Smooth::new(0.0),
            drag: None,
        });
        self.inspect_publish();
        self.sound(halcyon_audio::Cue::PanelOpen);
        self.gl3d_redraw();
    }

    pub(crate) fn inspect_is_open(&self) -> bool {
        self.view3d.borrow().inspect.is_some()
    }

    fn inspect_publish(&self) {
        let Some(app) = self.app() else { return };
        let v = self.view3d.borrow();
        let ui = app.global::<ShelfUi>();
        ui.set_inspect(v.inspect.is_some());
        ui.set_inspect_open_lid(v.inspect.as_ref().is_some_and(|i| i.lid.target > 0.5));
    }

    /// Commandes d'Inspecter : flèches (tourner), A (ouvrir / fermer), Y (retourner),
    /// LB / RB (zoom), B (fermer), souris (glisser pour tourner, molette pour zoomer).
    pub(crate) fn inspect_input(&self, action: &str) {
        let step = 35f32.to_radians();
        let mut close = false;
        let mut fav = None;
        {
            let mut v = self.view3d.borrow_mut();
            let Some(ins) = v.inspect.as_mut() else { return };
            match action {
                "left" => ins.yaw.set(ins.yaw.target - step),
                "right" => ins.yaw.set(ins.yaw.target + step),
                "up" => ins.pitch.set((ins.pitch.target - 0.4).max(-1.1)),
                "down" => ins.pitch.set((ins.pitch.target + 0.4).min(1.1)),
                "accept" => {
                    // Ouvert : vu de trois quarts côté ouverture (on voit le disque) ; fermé : de face.
                    let open = ins.lid.target < 0.5;
                    ins.lid.set(if open { 1.0 } else { 0.0 });
                    ins.yaw.set(if open { -0.5 } else { 0.0 });
                    ins.pitch.set(if open { 0.12 } else { 0.0 });
                    ins.dist.set(if open { 4.8 } else { 3.9 });
                }
                "flip" => ins.yaw.set(ins.yaw.target + std::f32::consts::PI),
                "next" => ins.dist.set((ins.dist.target - 0.6).max(1.8)),
                "prev" => ins.dist.set((ins.dist.target + 0.6).min(6.0)),
                "fav" => fav = Some(ins.game),
                "back" | "inspect" | "exit" => close = true,
                a => {
                    let parse = |p: &str| {
                        let rest = a.strip_prefix(p)?;
                        let (x, y) = rest.split_once(':')?;
                        Some((x.parse::<f32>().ok()?, y.parse::<f32>().ok()?))
                    };
                    if let Some(p) = parse("inspect-press:") {
                        ins.drag = Some(p);
                    } else if let Some((x, y)) = parse("inspect-drag:") {
                        if let Some((px, py)) = ins.drag {
                            let ny = ins.yaw.target + (x - px) * 0.01;
                            let np = (ins.pitch.target + (y - py) * 0.01).clamp(-1.1, 1.1);
                            ins.yaw.snap(ny);
                            ins.pitch.snap(np);
                        }
                        ins.drag = Some((x, y));
                    } else if a == "inspect-release" {
                        ins.drag = None;
                    } else if let Some(d) = a.strip_prefix("inspect-zoom:").and_then(|d| d.parse::<f32>().ok()) {
                        ins.dist.set((ins.dist.target - d * 0.4).clamp(1.8, 6.0));
                    }
                }
            }
            if close {
                v.inspect = None;
            }
        }
        if let Some(id) = fav {
            self.toggle_favorite(id);
            self.rebuild_bp_rows();
            self.refresh_bp();
        }
        if close {
            self.sound(halcyon_audio::Cue::PanelClose);
        }
        self.inspect_publish();
        self.gl3d_redraw();
    }
}
