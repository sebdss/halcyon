//! Petite algèbre 4 × 4 (colonnes d'abord, comme OpenGL) : juste ce qu'il faut pour placer des
//! boîtiers et une caméra. Pas de dépendance.

pub type Vec3 = [f32; 3];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat4(pub [f32; 16]);

impl Mat4 {
    pub const IDENTITY: Mat4 = Mat4([1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.]);

    fn at(&self, row: usize, col: usize) -> f32 {
        self.0[col * 4 + row]
    }

    pub fn mul(&self, o: &Mat4) -> Mat4 {
        let mut r = [0f32; 16];
        for c in 0..4 {
            for row in 0..4 {
                r[c * 4 + row] = (0..4).map(|k| self.at(row, k) * o.at(k, c)).sum();
            }
        }
        Mat4(r)
    }

    pub fn translate(x: f32, y: f32, z: f32) -> Mat4 {
        let mut m = Self::IDENTITY;
        m.0[12] = x;
        m.0[13] = y;
        m.0[14] = z;
        m
    }

    pub fn scale(x: f32, y: f32, z: f32) -> Mat4 {
        let mut m = Self::IDENTITY;
        m.0[0] = x;
        m.0[5] = y;
        m.0[10] = z;
        m
    }

    /// Rotation autour de l'axe vertical (radians ; positif : la face avant tourne vers la droite).
    pub fn rotate_y(a: f32) -> Mat4 {
        let (s, c) = a.sin_cos();
        Mat4([c, 0., -s, 0., 0., 1., 0., 0., s, 0., c, 0., 0., 0., 0., 1.])
    }

    pub fn rotate_x(a: f32) -> Mat4 {
        let (s, c) = a.sin_cos();
        Mat4([1., 0., 0., 0., 0., c, s, 0., 0., -s, c, 0., 0., 0., 0., 1.])
    }

    /// Perspective (champ vertical `fovy` en radians), profondeur OpenGL [-1, 1].
    pub fn perspective(fovy: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
        let f = 1.0 / (fovy / 2.0).tan();
        let nf = 1.0 / (near - far);
        Mat4([f / aspect, 0., 0., 0., 0., f, 0., 0., 0., 0., (far + near) * nf, -1., 0., 0., 2. * far * near * nf, 0.])
    }

    pub fn look_at(eye: Vec3, target: Vec3, up: Vec3) -> Mat4 {
        let f = normalize(sub(target, eye));
        let s = normalize(cross(f, up));
        let u = cross(s, f);
        Mat4([s[0], u[0], -f[0], 0., s[1], u[1], -f[1], 0., s[2], u[2], -f[2], 0., -dot(s, eye), -dot(u, eye), dot(f, eye), 1.])
    }

    pub fn transform(&self, p: Vec3) -> [f32; 4] {
        let mut r = [0f32; 4];
        for (row, out) in r.iter_mut().enumerate() {
            *out = self.at(row, 0) * p[0] + self.at(row, 1) * p[1] + self.at(row, 2) * p[2] + self.at(row, 3);
        }
        r
    }
}

pub fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub fn dot(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

pub fn normalize(a: Vec3) -> Vec3 {
    let l = dot(a, a).sqrt().max(1e-9);
    [a[0] / l, a[1] / l, a[2] / l]
}

pub fn mix(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Projection d'un point du monde vers l'écran (pixels, origine en haut à gauche), avec sa
/// profondeur ; `None` derrière la caméra.
pub fn project(view_proj: &Mat4, p: Vec3, w: f32, h: f32) -> Option<(f32, f32, f32)> {
    let c = view_proj.transform(p);
    if c[3] <= 1e-5 {
        return None;
    }
    let (x, y, z) = (c[0] / c[3], c[1] / c[3], c[2] / c[3]);
    Some(((x * 0.5 + 0.5) * w, (1.0 - (y * 0.5 + 0.5)) * h, z))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn rotations_and_products() {
        let r = Mat4::rotate_y(std::f32::consts::FRAC_PI_2);
        // La face avant (+z) tournée d'un quart de tour vers la droite regarde +x.
        let p = r.transform([0., 0., 1.]);
        assert!(close(p[0], 1.) && close(p[2], 0.));
        let t = Mat4::translate(1., 2., 3.).mul(&Mat4::scale(2., 2., 2.));
        let q = t.transform([1., 1., 1.]);
        assert!(close(q[0], 3.) && close(q[1], 4.) && close(q[2], 5.));
        assert_eq!(Mat4::IDENTITY.mul(&r), r);
    }

    #[test]
    fn camera_projects_target_to_centre() {
        let view = Mat4::look_at([0., 1., 7.], [0., 1., 0.], [0., 1., 0.]);
        let proj = Mat4::perspective(0.6, 16. / 9., 0.1, 50.);
        let vp = proj.mul(&view);
        let (x, y, _) = project(&vp, [0., 1., 0.], 1920., 1080.).unwrap();
        assert!(close(x, 960.) && close(y, 540.));
        // Plus haut dans le monde : plus haut à l'écran.
        let (_, y2, _) = project(&vp, [0., 2., 0.], 1920., 1080.).unwrap();
        assert!(y2 < y);
        // Derrière la caméra : rien.
        assert!(project(&vp, [0., 1., 9.], 1920., 1080.).is_none());
    }
}
