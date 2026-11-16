//! The bricks' material: lit, shadowed plastic with a dark line along every
//! edge of every face.
//!
//! The lines are drawn in the fragment shader, not as line geometry. Each
//! vertex carries how far it is from the rim of its face (see
//! `mesh::shape_mesh`); the shader darkens a pixel where that is within a
//! line's width of zero, measured in screen pixels with `fwidth`. A brick's
//! colour comes with it as a per-brick attribute. So a line is the same width near and far, a build of
//! any size is still one draw call per shape, and there is no second copy of
//! the geometry to keep in step. Real line primitives would be one pixel wide
//! at best in WebGL and would need a draw call or an instance per edge.
//! https://developer.mozilla.org/en-US/docs/Web/API/WebGLRenderingContext/lineWidth

use three_d::*;

pub struct BrickMaterial {
    /// how dark an edge line is: the lit colour is multiplied by this
    pub line_shade: f32,
    /// line width in pixels
    pub line_width: f32,
    pub roughness: f32,
    /// Drawn see-through, each brick as clear as the fourth part of its
    /// colour says: the water of a pond, and its ice. Such bricks are
    /// drawn after the solid ones, tested against their depth but leaving
    /// none of their own, so that what lies under water shows through it
    /// and what stands in front still hides it. Writing their depth would
    /// hide the bed behind the first face drawn.
    /// https://www.khronos.org/opengl/wiki/Transparency_Sorting
    pub clear: bool,
}

impl Default for BrickMaterial {
    fn default() -> Self {
        BrickMaterial { line_shade: 0.28, line_width: 1.3, roughness: 0.7, clear: false }
    }
}

/// The shader itself, in its own file so that it is edited, highlighted and
/// checked as GLSL. It is compiled into the program, not read at run time,
/// so there is still nothing to ship beside the executable.
const FRAGMENT: &str = include_str!("shaders/brick.frag");

impl Material for BrickMaterial {
    fn id(&self) -> EffectMaterialId {
        // ids up to 0x4FFF are left for materials defined outside three-d
        EffectMaterialId(0x0A5C)
    }

    fn fragment_shader_source(&self, lights: &[&dyn Light]) -> String {
        let mut source = lights_shader_source(lights);
        source.push_str(ToneMapping::fragment_shader_source());
        source.push_str(ColorMapping::fragment_shader_source());
        source.push_str(FRAGMENT);
        source
    }

    fn use_uniforms(&self, program: &Program, viewer: &dyn Viewer, lights: &[&dyn Light]) {
        viewer.tone_mapping().use_uniforms(program);
        viewer.color_mapping().use_uniforms(program);
        program.use_uniform_if_required("cameraPosition", viewer.position());
        for (i, light) in lights.iter().enumerate() {
            light.use_uniforms(program, i as u32);
        }
        program.use_uniform("lineShade", self.line_shade);
        program.use_uniform("lineWidth", self.line_width);
        program.use_uniform_if_required("roughness", self.roughness);
    }

    fn render_states(&self) -> RenderStates {
        if self.clear {
            // (the picture's own fourth part is left as it was: on a web page a canvas less than solid would show the page through it)
            RenderStates { write_mask: WriteMask::COLOR, blend: Blend::TRANSPARENCY, ..Default::default() }
        } else {
            RenderStates::default()
        }
    }

    fn material_type(&self) -> MaterialType {
        if self.clear { MaterialType::Transparent } else { MaterialType::Opaque }
    }
}
