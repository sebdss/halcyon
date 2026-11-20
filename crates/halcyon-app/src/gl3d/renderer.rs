//! Rendu OpenGL des boîtiers (1.0.0-alpha.8).
//!
//! Le contexte est celui de Slint (rendu femtovg) : la scène est dessinée dans une texture hors
//! écran (avec anticrénelage MSAA quand le pilote le permet), que Slint affiche ensuite comme une
//! image ordinaire, sous ses propres éléments (plaques, aides). Tout l'état OpenGL touché est
//! sauvegardé puis rendu à Slint tel quel.
//!
//! Géométrie : un pavé unitaire (6 faces, chacune sa texture), une planche, un disque. Lumière :
//! ambiante, une principale, un projecteur facultatif ; plastique brillant (reflet spéculaire et
//! frange de Fresnel) sur les faces avant et arrière. Reflets au sol par le pochoir (stencil) :
//! seulement sur le dessus de la planche.

#![allow(unsafe_code)]

use super::math::{Mat4, Vec3};
use glow::HasContext;
use std::collections::HashMap;
use std::num::NonZeroU32;

/// Faces du pavé, dans l'ordre des indices.
pub const FRONT: usize = 0;
pub const BACK: usize = 1;
pub const SPINE: usize = 2;
pub const EDGE: usize = 3;
pub const TOP: usize = 4;
pub const BOTTOM: usize = 5;

pub type TexKey = u64;

#[derive(Clone, Copy, Debug)]
pub struct Face {
    pub tex: Option<TexKey>,
    pub tint: [f32; 4],
    pub gloss: f32,
}

impl Face {
    pub fn plain(rgb: [f32; 3], gloss: f32) -> Self {
        Self { tex: None, tint: [rgb[0], rgb[1], rgb[2], 1.0], gloss }
    }
    pub fn tex(key: TexKey, gloss: f32) -> Self {
        Self { tex: Some(key), tint: [1.0; 4], gloss }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Geom {
    Box,
    Disc,
}

#[derive(Clone, Debug)]
pub struct Item {
    pub geom: Geom,
    pub model: Mat4,
    pub faces: [Face; 6],
    /// Assombrissement (voisins) 0..1.
    pub dim: f32,
    pub alpha: f32,
    /// Liseré (boîtier choisi : blanc ; jeu nouveau au musée : couleur chaude), 0..1.
    pub outline: f32,
    pub outline_col: [f32; 3],
    /// Reflet sur le dessus de la planche.
    pub reflect: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Light {
    pub ambient: [f32; 3],
    pub key_dir: Vec3,
    pub key_col: [f32; 3],
    pub spot: Option<(Vec3, [f32; 3])>,
}

#[derive(Clone, Debug)]
pub struct Scene {
    pub view_proj: Mat4,
    pub eye: Vec3,
    pub light: Light,
    /// Planches : dessus (avec reflets éventuels), puis le reste du pavé.
    pub planks: Vec<(Mat4, [f32; 3])>,
    /// Hauteur du sol (dessus des planches qui reflètent).
    pub floor_y: f32,
    pub items: Vec<Item>,
}

/// Image fournie au cache de textures (RGBA non prémultiplié).
pub struct Pixels {
    pub w: u32,
    pub h: u32,
    pub data: Vec<u8>,
}

struct Tex {
    tex: glow::Texture,
    used: u64,
    bytes: usize,
}

struct Target {
    w: i32,
    h: i32,
    color: glow::Texture,
    resolve: glow::Framebuffer,
    /// Rendu multi-échantillonné (MSAA), résolu dans `color` ; sinon rendu direct.
    ms: Option<(glow::Framebuffer, glow::Renderbuffer, glow::Renderbuffer)>,
    depth: Option<glow::Renderbuffer>,
}

struct Uniforms {
    vp: Option<glow::UniformLocation>,
    model: Option<glow::UniformLocation>,
    tex: Option<glow::UniformLocation>,
    tint: Option<glow::UniformLocation>,
    gloss: Option<glow::UniformLocation>,
    dim: Option<glow::UniformLocation>,
    alpha: Option<glow::UniformLocation>,
    eye: Option<glow::UniformLocation>,
    amb: Option<glow::UniformLocation>,
    key_dir: Option<glow::UniformLocation>,
    key_col: Option<glow::UniformLocation>,
    spot_pos: Option<glow::UniformLocation>,
    spot_col: Option<glow::UniformLocation>,
    spot_on: Option<glow::UniformLocation>,
    mode: Option<glow::UniformLocation>,
    floor: Option<glow::UniformLocation>,
}

pub struct Renderer {
    gl: glow::Context,
    prog: glow::Program,
    u: Uniforms,
    box_vao: glow::VertexArray,
    box_buf: (glow::Buffer, glow::Buffer),
    disc_vao: glow::VertexArray,
    disc_buf: (glow::Buffer, glow::Buffer),
    disc_count: i32,
    white: glow::Texture,
    target: Option<Target>,
    textures: HashMap<TexKey, Tex>,
    frame: u64,
    samples: i32,
    pub info: String,
}

const VS: &str = r"
uniform mat4 u_vp;
uniform mat4 u_model;
in vec3 a_pos;
in vec3 a_nrm;
in vec2 a_uv;
out vec3 v_wpos;
out vec3 v_nrm;
out vec2 v_uv;
void main() {
    vec4 w = u_model * vec4(a_pos, 1.0);
    v_wpos = w.xyz;
    v_nrm = mat3(u_model) * a_nrm;
    v_uv = a_uv;
    gl_Position = u_vp * w;
}
";

const FS: &str = r"
uniform sampler2D u_tex;
uniform vec4 u_tint;
uniform float u_gloss;
uniform float u_dim;
uniform float u_alpha;
uniform vec3 u_eye;
uniform vec3 u_amb;
uniform vec3 u_key_dir;
uniform vec3 u_key_col;
uniform vec3 u_spot_pos;
uniform vec3 u_spot_col;
uniform float u_spot_on;
uniform int u_mode;
uniform float u_floor;
in vec3 v_wpos;
in vec3 v_nrm;
in vec2 v_uv;
out vec4 frag;
void main() {
    vec4 t = texture(u_tex, v_uv) * u_tint;
    // Les faces sont opaques, même si l'image a des bords transparents.
    t.a = u_tint.a;
    if (u_mode == 3) {
        frag = vec4(u_tint.rgb * u_tint.a * u_alpha, u_tint.a * u_alpha);
        return;
    }
    if (u_mode == 1) {
        vec2 c = v_uv * 2.0 - 1.0;
        float r = length(c);
        if (r > 1.0 || r < 0.15) discard;
        // Bord et moyeu argentés, arc-en-ciel discret du disque.
        float rim = smoothstep(0.93, 0.97, r) + (1.0 - smoothstep(0.28, 0.31, r));
        // Étiquette imprimée sur fond argenté, plus claire que la jaquette.
        t.rgb = mix(vec3(0.8, 0.82, 0.86), t.rgb * 1.25 + 0.06, 0.8);
        t.rgb = mix(t.rgb, vec3(0.86, 0.88, 0.92), clamp(rim, 0.0, 1.0));
    }
    vec3 n = normalize(v_nrm);
    vec3 v = normalize(u_eye - v_wpos);
    vec3 l = normalize(-u_key_dir);
    float diff = max(dot(n, l), 0.0);
    vec3 h = normalize(l + v);
    float spec = pow(max(dot(n, h), 0.0), 60.0) * u_gloss * 0.55;
    float fres = pow(1.0 - max(dot(n, v), 0.0), 3.0) * 0.22 * u_gloss;
    vec3 sp = vec3(0.0);
    if (u_spot_on > 0.5) {
        vec3 d = u_spot_pos - v_wpos;
        float dist = length(d);
        vec3 sl = d / dist;
        float att = 1.0 / (1.0 + 0.12 * dist * dist);
        sp = u_spot_col * max(dot(n, sl), 0.0) * att;
        spec += pow(max(dot(n, normalize(sl + v)), 0.0), 60.0) * u_gloss * att * 0.6;
    }
    vec3 col = t.rgb * (u_amb + u_key_col * diff + sp) + vec3(spec + fres);
    col *= (1.0 - u_dim);
    float a = t.a * u_alpha;
    if (u_mode == 2) {
        // Reflet : s'efface avec la distance sous le sol.
        float fade = clamp(1.0 - (u_floor - v_wpos.y) / 0.75, 0.0, 1.0);
        a *= fade * fade;
    }
    frag = vec4(col * a, a);
}
";

/// Pavé unitaire x ∈ [-½, ½], y ∈ [0, 1], z ∈ [-½, ½] : position, normale, coordonnées de
/// texture ; 4 sommets par face, faces dans l'ordre `FRONT`, `BACK`, `SPINE`, `EDGE`, `TOP`,
/// `BOTTOM`, sens trigonométrique vu de l'extérieur.
pub fn box_mesh() -> (Vec<f32>, Vec<u16>) {
    // Chaque face : quatre coins (bas-gauche, bas-droite, haut-droite, haut-gauche) vus de
    // l'extérieur, et sa normale.
    let faces: [([[f32; 3]; 4], [f32; 3]); 6] = [
        ([[-0.5, 0., 0.5], [0.5, 0., 0.5], [0.5, 1., 0.5], [-0.5, 1., 0.5]], [0., 0., 1.]),
        ([[0.5, 0., -0.5], [-0.5, 0., -0.5], [-0.5, 1., -0.5], [0.5, 1., -0.5]], [0., 0., -1.]),
        ([[-0.5, 0., -0.5], [-0.5, 0., 0.5], [-0.5, 1., 0.5], [-0.5, 1., -0.5]], [-1., 0., 0.]),
        ([[0.5, 0., 0.5], [0.5, 0., -0.5], [0.5, 1., -0.5], [0.5, 1., 0.5]], [1., 0., 0.]),
        ([[-0.5, 1., 0.5], [0.5, 1., 0.5], [0.5, 1., -0.5], [-0.5, 1., -0.5]], [0., 1., 0.]),
        ([[-0.5, 0., -0.5], [0.5, 0., -0.5], [0.5, 0., 0.5], [-0.5, 0., 0.5]], [0., -1., 0.]),
    ];
    let uv = [[0., 1.], [1., 1.], [1., 0.], [0., 0.]];
    let mut v = Vec::new();
    let mut idx = Vec::new();
    for (fi, (corners, n)) in faces.iter().enumerate() {
        for (c, t) in corners.iter().zip(uv) {
            v.extend_from_slice(c);
            v.extend_from_slice(n);
            v.extend_from_slice(&t);
        }
        let b = (fi * 4) as u16;
        idx.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
    }
    (v, idx)
}

/// Disque de rayon 1 dans le plan XY, face vers +z.
pub fn disc_mesh(segments: u16) -> (Vec<f32>, Vec<u16>) {
    let mut v = vec![0., 0., 0., 0., 0., 1., 0.5, 0.5];
    for i in 0..=segments {
        let a = f32::from(i) / f32::from(segments) * std::f32::consts::TAU;
        let (s, c) = a.sin_cos();
        v.extend_from_slice(&[c, s, 0., 0., 0., 1., c * 0.5 + 0.5, 0.5 - s * 0.5]);
    }
    let mut idx = Vec::new();
    for i in 1..=segments {
        idx.extend_from_slice(&[0, i, i + 1]);
    }
    (v, idx)
}

fn bytes(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_ne_bytes()).collect()
}

fn bytes16(v: &[u16]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_ne_bytes()).collect()
}

/// État OpenGL sauvegardé avant de dessiner, rendu à Slint ensuite.
struct Saved {
    draw_fb: Option<glow::Framebuffer>,
    read_fb: Option<glow::Framebuffer>,
    viewport: [i32; 4],
    program: Option<glow::Program>,
    vao: Option<glow::VertexArray>,
    array_buf: Option<glow::Buffer>,
    active_tex: i32,
    tex0: Option<glow::Texture>,
    rb: Option<glow::Renderbuffer>,
    blend: bool,
    depth: bool,
    cull: bool,
    scissor: bool,
    stencil: bool,
    blend_src_rgb: i32,
    blend_dst_rgb: i32,
    blend_src_a: i32,
    blend_dst_a: i32,
    color_mask: [bool; 4],
    depth_mask: bool,
    unpack_align: i32,
}

impl Renderer {
    /// Crée le moteur dans le contexte courant de Slint. `None` (et un message) si OpenGL ne
    /// convient pas : l'étagère reste alors en 2,5D.
    pub fn new(get_proc: &dyn Fn(&std::ffi::CStr) -> *const std::ffi::c_void) -> Result<Self, String> {
        let gl = unsafe { glow::Context::from_loader_function_cstr(|s| get_proc(s)) };
        let version = unsafe { gl.get_parameter_string(glow::VERSION) };
        let es = version.contains("OpenGL ES");
        let major = version
            .split(|c: char| !c.is_ascii_digit() && c != '.')
            .find(|s| s.contains('.'))
            .and_then(|s| s.split('.').next()?.parse::<u32>().ok())
            .unwrap_or(0);
        if major < 3 {
            return Err(format!("OpenGL {version} : version 3 nécessaire"));
        }
        let header = if es { "#version 300 es\nprecision mediump float;\n" } else { "#version 330 core\n" };
        unsafe {
            let prog = gl.create_program()?;
            for (kind, src) in [(glow::VERTEX_SHADER, VS), (glow::FRAGMENT_SHADER, FS)] {
                let sh = gl.create_shader(kind)?;
                gl.shader_source(sh, &format!("{header}{src}"));
                gl.compile_shader(sh);
                if !gl.get_shader_compile_status(sh) {
                    return Err(gl.get_shader_info_log(sh));
                }
                gl.attach_shader(prog, sh);
            }
            gl.bind_attrib_location(prog, 0, "a_pos");
            gl.bind_attrib_location(prog, 1, "a_nrm");
            gl.bind_attrib_location(prog, 2, "a_uv");
            gl.link_program(prog);
            if !gl.get_program_link_status(prog) {
                return Err(gl.get_program_info_log(prog));
            }
            let loc = |n: &str| gl.get_uniform_location(prog, n);
            let u = Uniforms {
                vp: loc("u_vp"),
                model: loc("u_model"),
                tex: loc("u_tex"),
                tint: loc("u_tint"),
                gloss: loc("u_gloss"),
                dim: loc("u_dim"),
                alpha: loc("u_alpha"),
                eye: loc("u_eye"),
                amb: loc("u_amb"),
                key_dir: loc("u_key_dir"),
                key_col: loc("u_key_col"),
                spot_pos: loc("u_spot_pos"),
                spot_col: loc("u_spot_col"),
                spot_on: loc("u_spot_on"),
                mode: loc("u_mode"),
                floor: loc("u_floor"),
            };
            let saved = Self::save(&gl);
            let mk = |v: &[f32], i: &[u16]| -> Result<(glow::VertexArray, glow::Buffer, glow::Buffer), String> {
                let vao = gl.create_vertex_array()?;
                gl.bind_vertex_array(Some(vao));
                let vb = gl.create_buffer()?;
                gl.bind_buffer(glow::ARRAY_BUFFER, Some(vb));
                gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, &bytes(v), glow::STATIC_DRAW);
                let ib = gl.create_buffer()?;
                gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(ib));
                gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, &bytes16(i), glow::STATIC_DRAW);
                let stride = 8 * 4;
                gl.enable_vertex_attrib_array(0);
                gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, stride, 0);
                gl.enable_vertex_attrib_array(1);
                gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, stride, 12);
                gl.enable_vertex_attrib_array(2);
                gl.vertex_attrib_pointer_f32(2, 2, glow::FLOAT, false, stride, 24);
                gl.bind_vertex_array(None);
                Ok((vao, vb, ib))
            };
            let (bv, bi) = box_mesh();
            let (box_vao, b1, b2) = mk(&bv, &bi)?;
            let (dv, di) = disc_mesh(64);
            let (disc_vao, d1, d2) = mk(&dv, &di)?;
            let white = gl.create_texture()?;
            gl.bind_texture(glow::TEXTURE_2D, Some(white));
            gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA as i32, 1, 1, 0, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(Some(&[255, 255, 255, 255])));
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
            let samples = gl.get_parameter_i32(glow::MAX_SAMPLES).clamp(0, 4);
            Self::restore(&gl, &saved);
            Ok(Self {
                gl,
                prog,
                u,
                box_vao,
                box_buf: (b1, b2),
                disc_vao,
                disc_buf: (d1, d2),
                disc_count: di.len() as i32,
                white,
                target: None,
                textures: HashMap::new(),
                frame: 0,
                samples,
                info: format!("{version}, MSAA {samples}×"),
            })
        }
    }

    fn save(gl: &glow::Context) -> Saved {
        unsafe {
            let mut viewport = [0; 4];
            gl.get_parameter_i32_slice(glow::VIEWPORT, &mut viewport);
            let mut cm = [true; 4];
            let v: [bool; 4] = gl.get_parameter_bool_array(glow::COLOR_WRITEMASK);
            cm.copy_from_slice(&v);
            let active_tex = gl.get_parameter_i32(glow::ACTIVE_TEXTURE);
            gl.active_texture(glow::TEXTURE0);
            Saved {
                draw_fb: gl.get_parameter_framebuffer(glow::DRAW_FRAMEBUFFER_BINDING),
                read_fb: gl.get_parameter_framebuffer(glow::READ_FRAMEBUFFER_BINDING),
                viewport,
                program: gl.get_parameter_program(glow::CURRENT_PROGRAM),
                vao: gl.get_parameter_vertex_array(glow::VERTEX_ARRAY_BINDING),
                array_buf: gl.get_parameter_buffer(glow::ARRAY_BUFFER_BINDING),
                active_tex,
                tex0: gl.get_parameter_texture(glow::TEXTURE_BINDING_2D),
                rb: gl.get_parameter_renderbuffer(glow::RENDERBUFFER_BINDING),
                blend: gl.is_enabled(glow::BLEND),
                depth: gl.is_enabled(glow::DEPTH_TEST),
                cull: gl.is_enabled(glow::CULL_FACE),
                scissor: gl.is_enabled(glow::SCISSOR_TEST),
                stencil: gl.is_enabled(glow::STENCIL_TEST),
                blend_src_rgb: gl.get_parameter_i32(glow::BLEND_SRC_RGB),
                blend_dst_rgb: gl.get_parameter_i32(glow::BLEND_DST_RGB),
                blend_src_a: gl.get_parameter_i32(glow::BLEND_SRC_ALPHA),
                blend_dst_a: gl.get_parameter_i32(glow::BLEND_DST_ALPHA),
                color_mask: cm,
                depth_mask: gl.get_parameter_bool(glow::DEPTH_WRITEMASK),
                unpack_align: gl.get_parameter_i32(glow::UNPACK_ALIGNMENT),
            }
        }
    }

    fn restore(gl: &glow::Context, s: &Saved) {
        unsafe {
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, s.draw_fb);
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, s.read_fb);
            gl.viewport(s.viewport[0], s.viewport[1], s.viewport[2], s.viewport[3]);
            gl.use_program(s.program);
            gl.bind_vertex_array(s.vao);
            gl.bind_buffer(glow::ARRAY_BUFFER, s.array_buf);
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, s.tex0);
            gl.active_texture(s.active_tex as u32);
            gl.bind_renderbuffer(glow::RENDERBUFFER, s.rb);
            let set = |cap: u32, on: bool| if on { gl.enable(cap) } else { gl.disable(cap) };
            set(glow::BLEND, s.blend);
            set(glow::DEPTH_TEST, s.depth);
            set(glow::CULL_FACE, s.cull);
            set(glow::SCISSOR_TEST, s.scissor);
            set(glow::STENCIL_TEST, s.stencil);
            gl.blend_func_separate(s.blend_src_rgb as u32, s.blend_dst_rgb as u32, s.blend_src_a as u32, s.blend_dst_a as u32);
            gl.color_mask(s.color_mask[0], s.color_mask[1], s.color_mask[2], s.color_mask[3]);
            gl.depth_mask(s.depth_mask);
            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, s.unpack_align);
        }
    }

    /// Texture d'une clé ; `make` n'est appelé que si elle n'existe pas encore (et peut
    /// renvoyer `None` : image pas encore prête).
    pub fn texture(&mut self, key: TexKey, make: impl FnOnce() -> Option<Pixels>) -> bool {
        if let Some(t) = self.textures.get_mut(&key) {
            t.used = self.frame;
            return true;
        }
        let Some(p) = make() else { return false };
        if p.w == 0 || p.h == 0 || p.data.len() < (p.w * p.h * 4) as usize {
            return false;
        }
        unsafe {
            let gl = &self.gl;
            let saved_tex = gl.get_parameter_texture(glow::TEXTURE_BINDING_2D);
            let Ok(tex) = gl.create_texture() else { return false };
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
            gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA as i32, p.w as i32, p.h as i32, 0, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(Some(&p.data)));
            gl.generate_mipmap(glow::TEXTURE_2D);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR_MIPMAP_LINEAR as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
            gl.bind_texture(glow::TEXTURE_2D, saved_tex);
            self.textures.insert(key, Tex { tex, used: self.frame, bytes: p.data.len() * 4 / 3 });
        }
        true
    }

    pub fn has_texture(&self, key: TexKey) -> bool {
        self.textures.contains_key(&key)
    }

    /// Oublie une texture (visuel remplacé).
    pub fn forget(&mut self, key: TexKey) {
        if let Some(t) = self.textures.remove(&key) {
            unsafe { self.gl.delete_texture(t.tex) };
        }
    }

    /// Mémoire graphique des textures (octets, mipmaps compris).
    pub fn texture_bytes(&self) -> usize {
        self.textures.values().map(|t| t.bytes).sum()
    }

    /// Libère les textures les moins récemment utilisées au-delà de `budget` octets.
    fn evict(&mut self, budget: usize) {
        let mut total = self.texture_bytes();
        if total <= budget {
            return;
        }
        let mut by_age: Vec<(u64, TexKey)> = self.textures.iter().filter(|(_, t)| t.used < self.frame).map(|(k, t)| (t.used, *k)).collect();
        by_age.sort_unstable();
        for (_, k) in by_age {
            if total <= budget {
                break;
            }
            if let Some(t) = self.textures.remove(&k) {
                total -= t.bytes;
                unsafe { self.gl.delete_texture(t.tex) };
            }
        }
    }

    fn ensure_target(&mut self, w: i32, h: i32) -> Result<(), String> {
        if self.target.as_ref().is_some_and(|t| t.w == w && t.h == h) {
            return Ok(());
        }
        self.drop_target();
        unsafe {
            let gl = &self.gl;
            let color = gl.create_texture()?;
            gl.bind_texture(glow::TEXTURE_2D, Some(color));
            gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, w, h, 0, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(None));
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
            let resolve = gl.create_framebuffer()?;
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(resolve));
            gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(color), 0);
            let mut ms = None;
            let mut depth = None;
            if self.samples >= 2 {
                let fb = gl.create_framebuffer()?;
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fb));
                let c = gl.create_renderbuffer()?;
                gl.bind_renderbuffer(glow::RENDERBUFFER, Some(c));
                gl.renderbuffer_storage_multisample(glow::RENDERBUFFER, self.samples, glow::RGBA8, w, h);
                gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::RENDERBUFFER, Some(c));
                let d = gl.create_renderbuffer()?;
                gl.bind_renderbuffer(glow::RENDERBUFFER, Some(d));
                gl.renderbuffer_storage_multisample(glow::RENDERBUFFER, self.samples, glow::DEPTH24_STENCIL8, w, h);
                gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::DEPTH_STENCIL_ATTACHMENT, glow::RENDERBUFFER, Some(d));
                if gl.check_framebuffer_status(glow::FRAMEBUFFER) == glow::FRAMEBUFFER_COMPLETE {
                    ms = Some((fb, c, d));
                } else {
                    gl.delete_framebuffer(fb);
                    gl.delete_renderbuffer(c);
                    gl.delete_renderbuffer(d);
                    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(resolve));
                }
            }
            if ms.is_none() {
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(resolve));
                let d = gl.create_renderbuffer()?;
                gl.bind_renderbuffer(glow::RENDERBUFFER, Some(d));
                gl.renderbuffer_storage(glow::RENDERBUFFER, glow::DEPTH24_STENCIL8, w, h);
                gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::DEPTH_STENCIL_ATTACHMENT, glow::RENDERBUFFER, Some(d));
                depth = Some(d);
            }
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(resolve));
            if gl.check_framebuffer_status(glow::FRAMEBUFFER) != glow::FRAMEBUFFER_COMPLETE {
                return Err("tampon d'image incomplet".into());
            }
            self.target = Some(Target { w, h, color, resolve, ms, depth });
        }
        Ok(())
    }

    fn drop_target(&mut self) {
        if let Some(t) = self.target.take() {
            unsafe {
                let gl = &self.gl;
                gl.delete_texture(t.color);
                gl.delete_framebuffer(t.resolve);
                if let Some((fb, c, d)) = t.ms {
                    gl.delete_framebuffer(fb);
                    gl.delete_renderbuffer(c);
                    gl.delete_renderbuffer(d);
                }
                if let Some(d) = t.depth {
                    gl.delete_renderbuffer(d);
                }
            }
        }
    }

    /// Dessine la scène dans la texture hors écran ; renvoie son identifiant OpenGL.
    pub fn render(&mut self, scene: &Scene, w: u32, h: u32) -> Result<NonZeroU32, String> {
        self.frame += 1;
        let saved = Self::save(&self.gl);
        let r = self.render_inner(scene, w.max(1) as i32, h.max(1) as i32);
        Self::restore(&self.gl, &saved);
        // Budget de textures : environ 160 Mo de mémoire graphique.
        self.evict(160 << 20);
        r
    }

    fn render_inner(&mut self, scene: &Scene, w: i32, h: i32) -> Result<NonZeroU32, String> {
        self.ensure_target(w, h)?;
        let t = self.target.as_ref().ok_or("pas de cible")?;
        let gl = &self.gl;
        let u = &self.u;
        unsafe {
            let draw_fb = t.ms.as_ref().map(|m| m.0).unwrap_or(t.resolve);
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(draw_fb));
            gl.viewport(0, 0, w, h);
            gl.disable(glow::SCISSOR_TEST);
            gl.color_mask(true, true, true, true);
            gl.depth_mask(true);
            gl.clear_color(0., 0., 0., 0.);
            gl.clear_depth_f32(1.0);
            gl.clear_stencil(0);
            gl.stencil_mask(0xff);
            gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT | glow::STENCIL_BUFFER_BIT);
            gl.use_program(Some(self.prog));
            gl.uniform_matrix_4_f32_slice(u.vp.as_ref(), false, &scene.view_proj.0);
            gl.uniform_3_f32(u.eye.as_ref(), scene.eye[0], scene.eye[1], scene.eye[2]);
            let li = &scene.light;
            gl.uniform_3_f32(u.amb.as_ref(), li.ambient[0], li.ambient[1], li.ambient[2]);
            gl.uniform_3_f32(u.key_dir.as_ref(), li.key_dir[0], li.key_dir[1], li.key_dir[2]);
            gl.uniform_3_f32(u.key_col.as_ref(), li.key_col[0], li.key_col[1], li.key_col[2]);
            match li.spot {
                Some((p, c)) => {
                    gl.uniform_3_f32(u.spot_pos.as_ref(), p[0], p[1], p[2]);
                    gl.uniform_3_f32(u.spot_col.as_ref(), c[0], c[1], c[2]);
                    gl.uniform_1_f32(u.spot_on.as_ref(), 1.0);
                }
                None => gl.uniform_1_f32(u.spot_on.as_ref(), 0.0),
            }
            gl.uniform_1_f32(u.floor.as_ref(), scene.floor_y);
            gl.uniform_1_i32(u.tex.as_ref(), 0);
            gl.active_texture(glow::TEXTURE0);
            gl.enable(glow::BLEND);
            gl.blend_func_separate(glow::ONE, glow::ONE_MINUS_SRC_ALPHA, glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
            gl.disable(glow::CULL_FACE);
            gl.enable(glow::DEPTH_TEST);
            gl.depth_func(glow::LEQUAL);

            // 1. Dessus des planches : marque le pochoir (là où les reflets ont le droit d'être).
            gl.enable(glow::STENCIL_TEST);
            gl.stencil_func(glow::ALWAYS, 1, 0xff);
            gl.stencil_op(glow::KEEP, glow::KEEP, glow::REPLACE);
            for (m, c) in &scene.planks {
                self.draw_box_faces(m, &[Face::plain(*c, 0.2); 6], 0.0, 1.0, 0, &[TOP]);
            }
            // 2. Reflets : boîtiers retournés sous le sol, seulement sur le dessus des planches,
            //    sans écrire la profondeur (ils sont « dans » le vernis).
            gl.stencil_func(glow::EQUAL, 1, 0xff);
            gl.stencil_op(glow::KEEP, glow::KEEP, glow::KEEP);
            gl.depth_mask(false);
            gl.disable(glow::DEPTH_TEST);
            let mirror = Mat4::translate(0., scene.floor_y, 0.).mul(&Mat4::scale(1., -1., 1.)).mul(&Mat4::translate(0., -scene.floor_y, 0.));
            let mut refl: Vec<&Item> = scene.items.iter().filter(|i| i.reflect).collect();
            // Du plus loin au plus proche.
            refl.sort_by(|a, b| a.model.0[14].partial_cmp(&b.model.0[14]).unwrap_or(std::cmp::Ordering::Equal));
            for it in refl {
                let m = mirror.mul(&it.model);
                self.draw_box_faces(&m, &it.faces, it.dim, 0.32 * it.alpha, 2, &[FRONT, SPINE, EDGE, BACK]);
            }
            gl.disable(glow::STENCIL_TEST);
            gl.enable(glow::DEPTH_TEST);
            gl.depth_mask(true);
            // 3. Le reste des planches, puis les boîtiers.
            for (m, c) in &scene.planks {
                self.draw_box_faces(m, &[Face::plain(*c, 0.15); 6], 0.0, 1.0, 0, &[FRONT, BACK, SPINE, EDGE, BOTTOM]);
            }
            for it in &scene.items {
                match it.geom {
                    Geom::Box => {
                        if it.outline > 0.01 {
                            // Liseré : le pavé un peu plus grand, faces arrière seulement, en blanc.
                            let s = Mat4::translate(0., 0.5, 0.).mul(&Mat4::scale(1.035, 1.025, 1.12)).mul(&Mat4::translate(0., -0.5, 0.));
                            let m = it.model.mul(&s);
                            gl.enable(glow::CULL_FACE);
                            gl.cull_face(glow::FRONT);
                            let c = it.outline_col;
                            self.draw_box_faces(&m, &[Face { tex: None, tint: [c[0], c[1], c[2], 0.85 * it.outline], gloss: 0. }; 6], 0.0, 1.0, 3, &[0, 1, 2, 3, 4, 5]);
                            gl.disable(glow::CULL_FACE);
                        }
                        self.draw_box_faces(&it.model, &it.faces, it.dim, it.alpha, 0, &[0, 1, 2, 3, 4, 5]);
                    }
                    Geom::Disc => self.draw_disc(&it.model, &it.faces[FRONT], it.alpha),
                }
            }
            // Résolution de l'anticrénelage dans la texture affichée par Slint.
            if let Some((fb, _, _)) = t.ms {
                gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(fb));
                gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(t.resolve));
                gl.blit_framebuffer(0, 0, w, h, 0, 0, w, h, glow::COLOR_BUFFER_BIT, glow::NEAREST);
            }
            gl.flush();
            Ok(t.color.0)
        }
    }

    fn bind_face(&self, f: &Face) {
        let tex = f.tex.and_then(|k| self.textures.get(&k)).map(|t| t.tex).unwrap_or(self.white);
        unsafe {
            self.gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            self.gl.uniform_4_f32(self.u.tint.as_ref(), f.tint[0], f.tint[1], f.tint[2], f.tint[3]);
            self.gl.uniform_1_f32(self.u.gloss.as_ref(), f.gloss);
        }
    }

    fn draw_box_faces(&self, model: &Mat4, faces: &[Face; 6], dim: f32, alpha: f32, mode: i32, which: &[usize]) {
        let gl = &self.gl;
        unsafe {
            gl.bind_vertex_array(Some(self.box_vao));
            gl.uniform_matrix_4_f32_slice(self.u.model.as_ref(), false, &model.0);
            gl.uniform_1_f32(self.u.dim.as_ref(), dim);
            gl.uniform_1_f32(self.u.alpha.as_ref(), alpha);
            gl.uniform_1_i32(self.u.mode.as_ref(), mode);
            for &fi in which {
                self.bind_face(&faces[fi]);
                gl.draw_elements(glow::TRIANGLES, 6, glow::UNSIGNED_SHORT, (fi * 6 * 2) as i32);
            }
        }
    }

    fn draw_disc(&self, model: &Mat4, face: &Face, alpha: f32) {
        let gl = &self.gl;
        unsafe {
            gl.bind_vertex_array(Some(self.disc_vao));
            gl.uniform_matrix_4_f32_slice(self.u.model.as_ref(), false, &model.0);
            gl.uniform_1_f32(self.u.dim.as_ref(), 0.0);
            gl.uniform_1_f32(self.u.alpha.as_ref(), alpha);
            gl.uniform_1_i32(self.u.mode.as_ref(), 1);
            self.bind_face(face);
            gl.draw_elements(glow::TRIANGLES, self.disc_count, glow::UNSIGNED_SHORT, 0);
        }
    }

    /// Libère tout (fermeture de la fenêtre ou du contexte).
    pub fn teardown(&mut self) {
        self.drop_target();
        let keys: Vec<TexKey> = self.textures.keys().copied().collect();
        for k in keys {
            self.forget(k);
        }
        unsafe {
            let gl = &self.gl;
            gl.delete_texture(self.white);
            gl.delete_vertex_array(self.box_vao);
            gl.delete_vertex_array(self.disc_vao);
            for b in [self.box_buf.0, self.box_buf.1, self.disc_buf.0, self.disc_buf.1] {
                gl.delete_buffer(b);
            }
            gl.delete_program(self.prog);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meshes_are_well_formed() {
        let (v, i) = box_mesh();
        assert_eq!(v.len(), 24 * 8);
        assert_eq!(i.len(), 36);
        assert!(i.iter().all(|&k| (k as usize) < 24));
        // Chaque face est orientée vers l'extérieur (sens trigonométrique vu de sa normale).
        for f in 0..6 {
            let p = |k: usize| [v[(f * 4 + k) * 8], v[(f * 4 + k) * 8 + 1], v[(f * 4 + k) * 8 + 2]];
            let n = [v[f * 4 * 8 + 3], v[f * 4 * 8 + 4], v[f * 4 * 8 + 5]];
            let c = super::super::math::cross(super::super::math::sub(p(1), p(0)), super::super::math::sub(p(2), p(0)));
            assert!(super::super::math::dot(c, n) > 0.0, "face {f}");
        }
        let (dv, di) = disc_mesh(16);
        assert_eq!(dv.len(), (16 + 2) * 8);
        assert_eq!(di.len(), 16 * 3);
    }
}
