//! Faces générées des boîtiers 3D (quand aucun scan n'existe) : jaquette de repli, tranche et
//! dos, dessinés sur le processeur dans de petites images RGBA, une fois par jeu, avec les
//! polices de Halcyon (Archivo). `ab_glyph` ne fait que la rastérisation des lettres.

use ab_glyph::{Font, FontRef, PxScale, ScaleFont};

static DISPLAY: &[u8] = include_bytes!("../../ui/fonts/ArchivoExpanded-Bold.ttf");
static BODY: &[u8] = include_bytes!("../../ui/fonts/Archivo-Medium.ttf");

pub type Rgba = [u8; 4];

/// Image RGBA (non prémultipliée), rangées de haut en bas.
pub struct Canvas {
    pub w: u32,
    pub h: u32,
    pub px: Vec<u8>,
}

impl Canvas {
    pub fn new(w: u32, h: u32, fill: Rgba) -> Self {
        let mut px = Vec::with_capacity((w * h * 4) as usize);
        for _ in 0..w * h {
            px.extend_from_slice(&fill);
        }
        Self { w, h, px }
    }

    fn blend(&mut self, x: i32, y: i32, c: Rgba, cov: f32) {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 {
            return;
        }
        let a = (cov.clamp(0., 1.) * f32::from(c[3]) / 255.).clamp(0., 1.);
        let i = ((y as u32 * self.w + x as u32) * 4) as usize;
        for (k, ch) in c.iter().enumerate().take(3) {
            self.px[i + k] = (f32::from(self.px[i + k]) * (1. - a) + f32::from(*ch) * a) as u8;
        }
        self.px[i + 3] = self.px[i + 3].max((a * 255.) as u8);
    }

    pub fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: Rgba) {
        for yy in y.max(0)..(y + h).min(self.h as i32) {
            for xx in x.max(0)..(x + w).min(self.w as i32) {
                self.blend(xx, yy, c, 1.0);
            }
        }
    }

    /// Dégradé vertical de `a` (haut) à `b` (bas).
    pub fn vgradient(&mut self, a: Rgba, b: Rgba) {
        for y in 0..self.h {
            let t = y as f32 / self.h.max(1) as f32;
            let c = [0, 1, 2, 3].map(|k| (f32::from(a[k]) * (1. - t) + f32::from(b[k]) * t) as u8);
            for x in 0..self.w {
                let i = ((y * self.w + x) * 4) as usize;
                self.px[i..i + 4].copy_from_slice(&c);
            }
        }
    }

    /// Copie d'une autre image, étirée pour remplir `(x, y, w, h)` (plus proche voisin).
    pub fn blit_cover(&mut self, src: &Canvas, x: i32, y: i32, w: i32, h: i32, alpha: f32) {
        if src.w == 0 || src.h == 0 || w <= 0 || h <= 0 {
            return;
        }
        // Recadrage « cover » : garde le rapport de l'image source.
        let (sw, sh) = (src.w as f32, src.h as f32);
        let k = (w as f32 / sw).max(h as f32 / sh);
        let (ox, oy) = ((sw * k - w as f32) / 2., (sh * k - h as f32) / 2.);
        for yy in 0..h {
            for xx in 0..w {
                let sx = (((xx as f32 + ox) / k) as u32).min(src.w - 1);
                let sy = (((yy as f32 + oy) / k) as u32).min(src.h - 1);
                let i = ((sy * src.w + sx) * 4) as usize;
                let c = [src.px[i], src.px[i + 1], src.px[i + 2], 255];
                self.blend(x + xx, y + yy, c, alpha * f32::from(src.px[i + 3]) / 255.);
            }
        }
    }

    /// Tourne d'un quart de tour vers la gauche (le texte se lit de bas en haut).
    pub fn rotate_left(&self) -> Canvas {
        let mut out = Canvas::new(self.h, self.w, [0, 0, 0, 0]);
        for y in 0..self.h {
            for x in 0..self.w {
                let s = ((y * self.w + x) * 4) as usize;
                let (nx, ny) = (y, self.w - 1 - x);
                let d = ((ny * out.w + nx) * 4) as usize;
                out.px[d..d + 4].copy_from_slice(&self.px[s..s + 4]);
            }
        }
        out
    }

    /// Superpose `src` (même taille ou plus petit) en `(x, y)`.
    pub fn over(&mut self, src: &Canvas, x: i32, y: i32) {
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let i = ((yy as u32 * src.w + xx as u32) * 4) as usize;
                let a = f32::from(src.px[i + 3]) / 255.;
                if a > 0. {
                    self.blend(x + xx, y + yy, [src.px[i], src.px[i + 1], src.px[i + 2], 255], a);
                }
            }
        }
    }
}

fn font(display: bool) -> FontRef<'static> {
    FontRef::try_from_slice(if display { DISPLAY } else { BODY }).expect("police intégrée")
}

/// Largeur d'un texte en pixels.
pub fn measure(text: &str, px: f32, display: bool) -> f32 {
    let f = font(display);
    let s = f.as_scaled(PxScale::from(px));
    let mut w = 0.;
    let mut prev = None;
    for c in text.chars() {
        let id = s.glyph_id(c);
        if let Some(p) = prev {
            w += s.kern(p, id);
        }
        w += s.h_advance(id);
        prev = Some(id);
    }
    w
}

/// Taille, couleur et police d'un texte.
#[derive(Clone, Copy)]
pub struct Pen {
    pub px: f32,
    pub color: Rgba,
    pub display: bool,
}

/// Écrit une ligne ; `y` : ligne de base. Le texte trop long est coupé par « … ».
pub fn text(c: &mut Canvas, s: &str, x: f32, y: f32, pen: Pen, max_w: f32) {
    let Pen { px, color, display } = pen;
    let s = fit(s, px, display, max_w);
    let f = font(display);
    let sf = f.as_scaled(PxScale::from(px));
    let mut pen = x;
    let mut prev = None;
    for ch in s.chars() {
        let id = sf.glyph_id(ch);
        if let Some(p) = prev {
            pen += sf.kern(p, id);
        }
        let g = id.with_scale_and_position(PxScale::from(px), ab_glyph::point(pen, y));
        if let Some(o) = f.outline_glyph(g) {
            let b = o.px_bounds();
            o.draw(|gx, gy, cov| c.blend(b.min.x as i32 + gx as i32, b.min.y as i32 + gy as i32, color, cov));
        }
        pen += sf.h_advance(id);
        prev = Some(id);
    }
}

fn fit(s: &str, px: f32, display: bool, max_w: f32) -> String {
    if measure(s, px, display) <= max_w {
        return s.to_string();
    }
    let mut out: String = s.to_string();
    while !out.is_empty() && measure(&format!("{out}…"), px, display) > max_w {
        out.pop();
    }
    format!("{}…", out.trim_end())
}

/// Découpe en lignes de largeur `max_w` (au plus `max_lines`, la dernière coupée par « … »).
pub fn wrap(s: &str, px: f32, display: bool, max_w: f32, max_lines: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    for word in s.split_whitespace() {
        let cand = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };
        if measure(&cand, px, display) <= max_w || cur.is_empty() {
            cur = cand;
        } else {
            lines.push(std::mem::take(&mut cur));
            cur = word.to_string();
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    if lines.len() > max_lines {
        lines.truncate(max_lines);
        if let Some(l) = lines.last_mut() {
            *l = fit(&format!("{l} …"), px, display, max_w);
        }
    }
    lines
}

pub fn rgba(c: slint::Color) -> Rgba {
    [c.red(), c.green(), c.blue(), 255]
}

pub fn mix(a: Rgba, b: Rgba, t: f32) -> Rgba {
    [0, 1, 2, 3].map(|k| (f32::from(a[k]) * (1. - t) + f32::from(b[k]) * t) as u8)
}

/// Ce qu'il faut savoir d'un jeu pour dessiner ses faces générées.
#[derive(Clone, Debug, Default)]
pub struct CaseInfo {
    pub title: String,
    pub initials: String,
    pub mark: String,
    pub color_a: Rgba,
    pub color_b: Rgba,
    pub band: Rgba,
    pub description: String,
    pub lines: Vec<String>,
}

/// Jaquette de repli (jeu sans visuel) : dégradé, initiales, titre.
pub fn cover(info: &CaseInfo) -> Canvas {
    let (w, h) = (240, 360);
    let mut c = Canvas::new(w, h, [0, 0, 0, 255]);
    c.vgradient(mix(info.color_a, [16, 18, 21, 255], 0.35), mix(info.color_a, [16, 18, 21, 255], 0.7));
    c.rect(0, 0, w as i32, 9, info.color_b);
    text(&mut c, &info.initials, 18., 110., Pen { px: 82., color: [255, 255, 255, 60], display: true }, w as f32 - 30.);
    let lines = wrap(&info.title.to_uppercase(), 26., true, w as f32 - 36., 3);
    let base = h as f32 - 24. - (lines.len().saturating_sub(1)) as f32 * 30.;
    for (i, l) in lines.iter().enumerate() {
        text(&mut c, l, 18., base + i as f32 * 30., Pen { px: 26., color: [255, 255, 255, 240], display: true }, w as f32 - 36.);
    }
    c
}

/// Tranche : couleur du jeu, bande de la source en haut, titre de bas en haut.
pub fn spine(info: &CaseInfo) -> Canvas {
    let (w, h) = (64u32, 512u32);
    let mut c = Canvas::new(w, h, mix(info.color_b, info.color_a, 0.55));
    c.rect(0, 0, w as i32, 72, info.band);
    // Le texte est écrit à plat (h × w) puis tourné.
    let mut mark = Canvas::new(72, w, [0, 0, 0, 0]);
    let mw = measure(&info.mark, 17., false).min(66.);
    text(&mut mark, &info.mark, (72. - mw) / 2., 39., Pen { px: 17., color: [255, 255, 255, 255], display: false }, 66.);
    c.over(&mark.rotate_left(), 0, 0);
    let len = h - 90;
    let mut title = Canvas::new(len, w, [0, 0, 0, 0]);
    let tw = measure(&info.title, 26., true).min(len as f32 - 16.);
    text(&mut title, &info.title, (len as f32 - tw) / 2., 41., Pen { px: 26., color: [255, 255, 255, 235], display: true }, len as f32 - 16.);
    c.over(&title.rotate_left(), 0, 80);
    // Ombre légère sur les bords (arêtes du boîtier).
    c.rect(0, 0, 3, h as i32, [0, 0, 0, 60]);
    c.rect(w as i32 - 3, 0, 3, h as i32, [0, 0, 0, 60]);
    c
}

/// Dos composé : image du jeu en tête, titre, résumé, fiche, bandeau de la source.
pub fn back(info: &CaseInfo, picture: Option<&Canvas>) -> Canvas {
    let (w, h) = (320u32, 480u32);
    let bg = mix(info.color_b, [13, 15, 18, 255], 0.75);
    let mut c = Canvas::new(w, h, bg);
    if let Some(p) = picture {
        c.blit_cover(p, 0, 0, w as i32, 170, 1.0);
        // Fondu de l'image dans le fond.
        for y in 110..170 {
            let t = (y - 110) as f32 / 60.;
            c.rect(0, y, w as i32, 1, [bg[0], bg[1], bg[2], (t * 255.) as u8]);
        }
    }
    let pad = 18.;
    let mut y = 196.;
    for l in wrap(&info.title, 20., true, w as f32 - 2. * pad, 2) {
        text(&mut c, &l, pad, y, Pen { px: 20., color: [255, 255, 255, 255], display: true }, w as f32 - 2. * pad);
        y += 24.;
    }
    y += 6.;
    for l in wrap(&info.description, 13., false, w as f32 - 2. * pad, 9) {
        text(&mut c, &l, pad, y, Pen { px: 13., color: [255, 255, 255, 205], display: false }, w as f32 - 2. * pad);
        y += 17.;
    }
    let mut yb = h as f32 - 44. - info.lines.len() as f32 * 16.;
    c.rect(pad as i32, yb as i32 - 14, (w as f32 - 2. * pad) as i32, 1, [255, 255, 255, 50]);
    for l in &info.lines {
        text(&mut c, l, pad, yb, Pen { px: 12., color: [255, 255, 255, 180], display: false }, w as f32 - 2. * pad);
        yb += 16.;
    }
    c.rect(0, h as i32 - 36, w as i32, 36, info.band);
    let mw = measure(&info.mark, 15., true).min(w as f32 - 20.);
    text(&mut c, &info.mark, (w as f32 - mw) / 2., h as f32 - 12., Pen { px: 15., color: [255, 255, 255, 255], display: true }, w as f32 - 20.);
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info() -> CaseInfo {
        CaseInfo {
            title: "Une aventure au titre vraiment très long".into(),
            initials: "UA".into(),
            mark: "Steam".into(),
            color_a: [80, 40, 120, 255],
            color_b: [20, 90, 140, 255],
            band: [42, 71, 94, 255],
            description: "Un résumé de plusieurs phrases. ".repeat(20),
            lines: vec!["Action · Aventure".into(), "Temps de jeu : 3 h".into()],
        }
    }

    #[test]
    fn generated_faces_have_expected_sizes_and_ink() {
        let i = info();
        let s = spine(&i);
        assert_eq!((s.w, s.h, s.px.len()), (64, 512, 64 * 512 * 4));
        let b = back(&i, Some(&cover(&i)));
        assert_eq!((b.w, b.h), (320, 480));
        // Du texte blanc a bien été posé sur le fond sombre du dos.
        assert!(b.px.chunks(4).any(|p| p[0] > 200 && p[1] > 200 && p[2] > 200));
        let c = cover(&i);
        assert_eq!((c.w, c.h), (240, 360));
    }

    #[test]
    fn wrapping_and_elision() {
        let lines = wrap("un deux trois quatre cinq six sept huit neuf dix", 20., false, 80., 3);
        assert_eq!(lines.len(), 3);
        assert!(lines[2].ends_with('…'));
        assert!(measure(&fit("abcdefghijklmnopqrstuvwxyz", 20., false, 60.), 20., false) <= 60.);
        let r = Canvas::new(3, 2, [1, 2, 3, 4]).rotate_left();
        assert_eq!((r.w, r.h), (2, 3));
    }
}
