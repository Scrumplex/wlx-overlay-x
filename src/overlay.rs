use std::f32::consts::PI;

use glam::{vec2, vec3, Affine3A, Quat, Vec3, Vec3A, Mat3A};
use log::{debug, info};
use stereokit::{
    sys::color32, Color128, Material, Mesh, RenderLayer, SkDraw, StereoKitMultiThread, Tex,
    TextureFormat, TextureType, Vert, StereoKitDraw, Pose,
};

use crate::{interactions::{InteractionHandler, DummyInteractionHandler}, AppState};

pub const COLOR_WHITE: Color128 = Color128 {
    r: 1.,
    g: 1.,
    b: 1.,
    a: 1.,
};
pub const COLOR_FALLBACK: Color128 = Color128 {
    r: 1.,
    g: 0.,
    b: 1.,
    a: 1.,
};

pub struct OverlayData {
    pub name: String,
    pub width: f32,
    pub size: (i32, i32),
    pub visible: bool,
    pub want_visible: bool,
    pub color: Color128,
    pub transform: Affine3A,
    pub interaction_transform: Affine3A,
    pub renderer: Box<dyn OverlayRenderer>,
    pub interaction: Box<dyn InteractionHandler>,
    pub primary_pointer: Option<usize>,
    pub gfx: Option<OverlayGraphics>,
}

pub struct OverlayGraphics {
    pub tex: Tex,
    pub mesh: Mesh,
    pub mat: Material,
}

pub trait OverlayRenderer {
    fn init(&mut self, sk: &SkDraw);
    fn pause(&mut self);
    fn resume(&mut self);
    fn render(&mut self, sk: &SkDraw, tex: &Tex, app: &mut AppState);
}

impl OverlayData {

    pub fn show(&mut self, sk: &SkDraw, app: &AppState) {
        if self.visible {
            return;
        }

        info!("{}: Show", &self.name);

        self.visible = true;

        if self.gfx.is_none() {
            let tex = sk.tex_gen_color(
                COLOR_FALLBACK,
                self.size.0,
                self.size.1,
                TextureType::IMAGE_NO_MIPS,
                TextureFormat::RGBA32,
            );

            let mesh = sk.mesh_create();

            let scr_w = self.size.0 as f32;
            let scr_h = self.size.1 as f32;

            let half_w: f32;
            let half_h: f32;

            if scr_w >= scr_h {
                half_w = 1.;
                half_h = scr_h / scr_w;
            } else {
                half_w = scr_w / scr_h;
                half_h = 1.;
            }

            self.interaction_transform = Affine3A::from_scale_rotation_translation(
                vec3(0.5 / -half_w, 0.5 / -half_h, 0.),
                Quat::IDENTITY,
                vec3(0.5, 0.5, 0.),
            );

            let norm = vec3(0., 0., -1.);
            let col = color32::new_rgb(255, 255, 255);

            let mut x0 = 0f32;
            let mut x1 = 1f32;
            let mut y0 = 0f32;
            let mut y1 = 1f32;

            if app.session.screen_flip_h {
                x0 = 1.;
                x1 = 0.;
            }
            if app.session.screen_flip_v {
                y0 = 1.;
                y1 = 0.;
            }

            #[rustfmt::skip]
            let verts = vec![
                Vert { pos: vec3(-half_w, -half_h, 0.), uv: vec2(x1, y1), norm, col },
                Vert { pos: vec3(-half_w, half_h, 0.), uv: vec2(x1, y0), norm, col },
                Vert { pos: vec3(half_w, -half_h, 0.), uv: vec2(x0, y1), norm, col },
                Vert { pos: vec3(half_w, half_h, 0.), uv: vec2(x0, y0), norm, col },
            ];

            let inds = vec![0, 3, 2, 3, 0, 1];
            sk.mesh_set_verts(&mesh, &verts, true);
            sk.mesh_set_inds(&mesh, &inds);

            let mat = sk.material_copy(Material::UNLIT);
            sk.material_set_texture(&mat, "diffuse", &tex);

            self.gfx = Some(OverlayGraphics { tex, mat, mesh });

            self.renderer.init(sk);
        }

        debug!(
            "Head at {}, looking {}",
            sk.input_head().position,
            sk.input_head().forward()
        );

        let forward = sk.input_head().position + sk.input_head().forward();
        self.transform.translation = forward.into();

        self.transform = Affine3A::from_rotation_y(PI);
        self.transform.translation = forward.into();

        debug!(
            "Overlay at {}, looking at {}",
            forward,
            self.transform.transform_vector3a(-Vec3A::Z)
        );
    }
    pub fn render(&mut self, sk: &SkDraw, app: &mut AppState) {
        if !self.visible {
            return;
        }

        let m = Affine3A::from_rotation_translation(app.input.hmd.orientation, app.input.hmd.position);
        debug!("{}", m.y_axis);
        debug!("{}", m.transform_vector3a(Vec3A::Y));

        if let Some(gfx) = self.gfx.as_mut() {
            self.renderer.render(sk, &gfx.tex, app);
            sk.mesh_draw(
                &gfx.mesh,
                &gfx.mat,
                self.transform,
                self.color,
                RenderLayer::LAYER0,
            );
        }
        /*
                    let x = pos.x * (self.output.size.0 as f32) - 8.;
                    let y = pos.y * (self.output.size.1 as f32) - 8.;
                    state.gl.draw_color(vec3(1., 0., 0.), x, y, 16., 16.);
        */
    }

    pub fn on_size(&mut self, delta: f32) {
        let t = self.transform.matrix3.mul_scalar(1. - delta.powi(3) * 2.);
        let len_sq = t.x_axis.length_squared();
        if len_sq > 0.12 && len_sq < 100. {
            self.transform.matrix3 = t;
        }
    }

    pub fn on_move(&mut self, pos: Vec3, hmd: &Pose) {
        if (hmd.position - pos).length_squared() > 0.2 {
            self.transform.translation = pos.into();
            self.realign(hmd);
        }
    }

    pub fn on_drop(&mut self) {
        // TODO save position
    }

    pub fn on_curve(&mut self) {

    }

    pub fn realign(&mut self, hmd_pose: &Pose) {
        let hmd = Affine3A::from_rotation_translation(hmd_pose.orientation, hmd_pose.position);
        let to_hmd = hmd.translation - self.transform.translation;
        let up_dir: Vec3A;

        if hmd.x_axis.dot(Vec3A::Y).abs() > 0.2 {
            // Snap upright
            up_dir = hmd.y_axis;
        } else {
            let dot = to_hmd.normalize().dot(hmd.z_axis);
            let z_dist = to_hmd.length();
            let y_dist = (self.transform.translation.y - hmd.translation.y).abs();
            let x_angle = (y_dist / z_dist).asin();

            if dot < -f32::EPSILON { // facing down
                let up_point = hmd.translation + z_dist / x_angle.cos() * Vec3A::Y;
                up_dir = (up_point - self.transform.translation).normalize();
            } else if dot > f32::EPSILON { // facing up
                let dn_point = hmd.translation + z_dist / x_angle.cos() * Vec3A::NEG_Y;
                up_dir = (self.transform.translation - dn_point).normalize();
            } else { // perfectly upright
                up_dir = Vec3A::Y;
            }
        }

        let scale = self.transform.x_axis.length();

        let col_z = (self.transform.translation - hmd.translation).normalize();
        let col_y = up_dir;
        let col_x = col_y.cross(col_z);
        let col_y = col_z.cross(col_x).normalize();
        let col_x = col_x.normalize();

        self.transform.matrix3 = Mat3A::from_cols(col_x, col_y, col_z).mul_scalar(scale);
    }
}

// --- Dummy impls below ---

impl Default for OverlayData {
    fn default() -> OverlayData {
        OverlayData {
            name: String::new(),
            width: 1.,
            size: (0, 0),
            visible: false,
            want_visible: false,
            color: COLOR_WHITE,
            transform: Affine3A::IDENTITY,
            interaction_transform: Affine3A::IDENTITY,
            gfx: None,
            renderer: Box::new(FallbackRenderer),
            interaction: Box::new(DummyInteractionHandler),
            primary_pointer: None,
        }
    }
}

pub struct FallbackRenderer;

impl OverlayRenderer for FallbackRenderer {
    fn init(&mut self, _sk: &SkDraw) {}
    fn pause(&mut self) {}
    fn resume(&mut self) {}
    fn render(&mut self, sk: &SkDraw, tex: &Tex, app: &mut AppState) {
        app.gl.begin_sk(sk, tex);
        app.gl.draw_color(
            vec3(1., 0., 1.),
            0.,
            0.,
            sk.tex_get_width(tex) as _,
            sk.tex_get_height(tex) as _,
        );
        app.gl.end();
    }
}

