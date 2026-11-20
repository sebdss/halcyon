//! Boîtiers en vraie 3D (1.0.0-alpha.8) : étagère, musée et vue « Inspecter » du mode console.
//!
//! Le rendu passe par le contexte OpenGL que Slint utilise déjà (femtovg) et par `glow`, que
//! femtovg embarque : aucune dépendance lourde. La scène est dessinée dans une texture que
//! Slint affiche comme une image ; ses plaques, cartes et aides restent des éléments Slint
//! posés par-dessus. Sans OpenGL 3 (rendu logiciel), en mode Performance ou si l'option est
//! coupée, la version 2,5D de la 1.0.0-alpha.7 reste en place.
//!
//! Ici : dimensions, placement des boîtiers (fonctions pures, testées) ; `renderer` dessine,
//! `raster` compose les faces générées, `anim` anime.

pub mod anim;
pub mod math;
pub mod raster;
pub mod renderer;

use math::{mix, Mat4};
use std::f32::consts::PI;

/// Boîtier : largeur 1 (face 214 × 320 de la 2,5D), hauteur, épaisseur.
pub const W: f32 = 1.0;
pub const H: f32 = 1.495;
pub const D: f32 = 0.187;

/// Planche de l'étagère : bord avant (le dessus est à y = 0).
pub const SHELF_FRONT: f32 = 1.55;
/// Musée : écart vertical entre deux rangées et pas des tranches.
pub const ROW_H: f32 = 1.85;
pub const PITCH: f32 = 0.215;

fn rad(deg: f32) -> f32 {
    deg * PI / 180.0
}

/// Matrice d'un boîtier posé en `(x, y, z)`, tourné de `ry` autour de la verticale.
pub fn case_model(x: f32, y: f32, z: f32, ry: f32) -> Mat4 {
    Mat4::translate(x, y, z).mul(&Mat4::rotate_y(ry)).mul(&Mat4::scale(W, H, D))
}

/// Étagère : place d'un boîtier à `k` crans de la sélection (animé), retourné de `flip`
/// (0..1). Le choisi est au centre, avancé, tourné de 22° (on voit sa tranche) ; les voisins
/// reculent en pile serrée, tournés de 60° vers lui. Renvoie la matrice et l'assombrissement.
pub fn shelf_case(k: f32, flip: f32) -> (Mat4, f32) {
    let ak = k.abs();
    let t = ak.min(1.0);
    let sg = if k < 0.0 { -1.0 } else { 1.0 };
    let x = sg * if ak < 1.0 { ak * 1.22 } else { 1.22 + (ak - 1.0) * 0.39 };
    let z = if ak < 1.0 { mix(0.75, -1.0, t) } else { -1.0 - (ak - 1.0) * 0.03 };
    let side = if k < 0.0 { rad(60.0) } else { rad(-60.0) };
    let ry = mix(rad(22.0), side, t) + PI * flip;
    (case_model(x, 0.0, z, ry), 0.34 * t)
}

/// Caméra de l'étagère pour un écran `aspect` : regarde la pile un peu d'en haut ; le bord
/// avant de la planche tombe à 73 % de la hauteur (là où la 2,5D posait sa planche).
pub fn shelf_camera(aspect: f32) -> (Mat4, [f32; 3]) {
    let eye = [0.0, 1.2, 7.6];
    let view = Mat4::look_at(eye, [0.0, 0.7, 0.0], [0.0, 1.0, 0.0]);
    let proj = Mat4::perspective(rad(31.0), aspect, 2.0, 40.0);
    (with_shift(&proj.mul(&view), [0.0, 0.0, SHELF_FRONT], 0.73), eye)
}

/// Décale verticalement l'image pour que `anchor` tombe à `y_frac` de la hauteur.
fn with_shift(vp: &Mat4, anchor: [f32; 3], y_frac: f32) -> Mat4 {
    let c = vp.transform(anchor);
    let ndc = c[1] / c[3];
    let want = 1.0 - 2.0 * y_frac;
    Mat4::translate(0.0, want - ndc, 0.0).mul(vp)
}

/// Musée : place d'un boîtier de la rangée `row`, position `i`. `out` (0..1) : sorti de la
/// rangée et tourné de face ; `shift` : écart pris par un voisin sorti à sa gauche.
pub fn museum_case(row: usize, i: usize, out: f32, shift: f32, flip: f32) -> Mat4 {
    let y = -(row as f32) * ROW_H;
    let x = i as f32 * PITCH + D / 2.0 + shift + out * (W / 2.0 - D / 2.0 + 0.05);
    let ry = mix(rad(90.0), 0.0, out) + PI * flip;
    case_model(x, y, out * 0.55, ry)
}

/// Écart laissé par un boîtier sorti pour ses voisins de droite.
pub fn museum_gap(out: f32) -> f32 {
    out * (W - D + 0.15)
}

/// Abscisse de la caméra du musée : le boîtier choisi vers 45 % de l'écran, le début de la
/// rangée jamais sous les plaques.
pub fn museum_cam_x(focus: usize) -> f32 {
    (focus as f32 * PITCH + 0.6).max(3.4)
}

/// Rangée au centre de l'écran : la rangée choisie reste en deuxième position si possible.
pub fn museum_center_row(row: usize, rows: usize) -> f32 {
    let v = (row as f32 - 1.0).min(rows as f32 - 3.0).max(0.0);
    v + 1.0
}

pub fn museum_camera(aspect: f32, cam_x: f32, cam_y: f32) -> (Mat4, [f32; 3]) {
    let eye = [cam_x, cam_y + 0.75, 10.5];
    let view = Mat4::look_at(eye, [cam_x, cam_y + 0.35, 0.0], [0.0, 1.0, 0.0]);
    let proj = Mat4::perspective(rad(40.0), aspect, 3.0, 60.0);
    (proj.mul(&view), eye)
}

/// Inspecter : boîtier centré, tourné de `yaw` / `pitch`.
pub fn inspect_base(yaw: f32, pitch: f32) -> Mat4 {
    Mat4::rotate_x(pitch).mul(&Mat4::rotate_y(yaw)).mul(&Mat4::translate(0.0, -H / 2.0, 0.0))
}

/// Couvercle (face avant, 2 centièmes d'épaisseur) ouvert de `lid` (0..1) autour de sa charnière, côté tranche.
pub fn inspect_lid(base: &Mat4, lid: f32) -> Mat4 {
    base.mul(&Mat4::translate(-W / 2.0, 0.0, D / 2.0))
        .mul(&Mat4::rotate_y(-lid * rad(115.0)))
        .mul(&Mat4::translate(W / 2.0, 0.0, 0.01))
        .mul(&Mat4::scale(W, H, 0.02))
}

/// Disque dans le boîtier ouvert.
pub fn inspect_disc(base: &Mat4) -> Mat4 {
    base.mul(&Mat4::translate(0.0, H / 2.0, D / 2.0 + 0.01)).mul(&Mat4::scale(0.42, 0.42, 1.0))
}

pub fn inspect_camera(aspect: f32, dist: f32) -> (Mat4, [f32; 3]) {
    let eye = [0.0, 0.0, dist];
    let view = Mat4::look_at(eye, [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
    (Mat4::perspective(rad(32.0), aspect, 0.6, 20.0).mul(&view), eye)
}

/// Rectangle à l'écran (pixels) des huit coins d'un pavé unitaire transformé par `model`.
pub fn screen_rect(vp: &Mat4, model: &Mat4, w: f32, h: f32) -> Option<(f32, f32, f32, f32, f32)> {
    let mvp = vp.mul(model);
    let mut r = (f32::MAX, f32::MAX, f32::MIN, f32::MIN, f32::MAX);
    for &x in &[-0.5, 0.5] {
        for &y in &[0.0, 1.0] {
            for &z in &[-0.5, 0.5] {
                let (sx, sy, sz) = math::project(&mvp, [x, y, z], w, h)?;
                r = (r.0.min(sx), r.1.min(sy), r.2.max(sx), r.3.max(sy), r.4.min(sz));
            }
        }
    }
    Some(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shelf_selection_is_centred_and_forward() {
        let (m, dim) = shelf_case(0.0, 0.0);
        let p = m.transform([0.0, 0.0, 0.0]);
        assert_eq!(dim, 0.0);
        assert!(p[0].abs() < 1e-5 && p[2] > 0.7);
        // Voisins : à gauche et à droite, en retrait, voilés ; pile régulière.
        let (l, dl) = shelf_case(-1.0, 0.0);
        let (r, _) = shelf_case(2.0, 0.0);
        assert!(l.transform([0.; 3])[0] < -1.0 && l.transform([0.; 3])[2] < 0.0 && dl > 0.3);
        let step = shelf_case(3.0, 0.0).0.transform([0.; 3])[0] - r.transform([0.; 3])[0];
        assert!((step - 0.39).abs() < 1e-4);
        // À gauche, la tranche (face -x) regarde la caméra (+z).
        let n = l.transform([-1.0, 0.0, 0.0]);
        let o = l.transform([0.0, 0.0, 0.0]);
        assert!(n[2] - o[2] > 0.0);
    }

    #[test]
    fn shelf_camera_puts_plank_edge_low() {
        let (vp, _) = shelf_camera(16.0 / 9.0);
        let (_, y, _) = math::project(&vp, [0.0, 0.0, SHELF_FRONT], 1920.0, 1080.0).unwrap();
        assert!((y - 0.73 * 1080.0).abs() < 1.0);
        // Le boîtier choisi est entièrement à l'écran, au milieu.
        let (m, _) = shelf_case(0.0, 0.0);
        let (x0, y0, x1, y1, _) = screen_rect(&vp, &m, 1920.0, 1080.0).unwrap();
        assert!(x0 > 700.0 && x1 < 1220.0 && y0 > 200.0 && y1 < 900.0);
    }

    #[test]
    fn museum_cases_stand_spine_out_then_face() {
        let m = museum_case(0, 3, 0.0, 0.0, 0.0);
        // Tranche (face -x) vers la caméra.
        let n = m.transform([-1.0, 0.5, 0.0]);
        let o = m.transform([0.0, 0.5, 0.0]);
        assert!(n[2] > o[2]);
        let f = museum_case(0, 3, 1.0, 0.0, 0.0);
        let n = f.transform([0.0, 0.5, 1.0]);
        let o = f.transform([0.0, 0.5, 0.0]);
        assert!(n[2] > o[2] && f.transform([0.; 3])[2] > 0.5);
        assert!(museum_gap(1.0) > W - D);
        assert_eq!(museum_cam_x(0), 3.4);
        assert_eq!(museum_center_row(0, 10), 1.0);
        assert_eq!(museum_center_row(9, 10), 8.0);
        assert_eq!(museum_center_row(0, 1), 1.0);
    }

    #[test]
    fn lid_swings_out_from_the_spine() {
        let base = inspect_base(0.0, 0.0);
        let closed = inspect_lid(&base, 0.0);
        let open = inspect_lid(&base, 1.0);
        // Bord libre du couvercle : devant quand il est ouvert.
        assert!(open.transform([0.5, 0.5, 0.0])[2] > closed.transform([0.5, 0.5, 0.0])[2] + 0.5);
        // La charnière ne bouge pas.
        let a = closed.transform([-0.5, 0.5, 0.0]);
        let b = open.transform([-0.5, 0.5, 0.0]);
        assert!((a[0] - b[0]).abs() < 0.02 && (a[2] - b[2]).abs() < 0.02);
    }
}
