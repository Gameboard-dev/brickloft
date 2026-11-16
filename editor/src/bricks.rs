//! The bricks of the form being edited, as the graphics card is given
//! them: for each brick its corner, its size and its colour, and nothing
//! worked out for it here.
//!
//! The view drew every brick through three-d's `InstancedMesh`, one mesh
//! for each brick shape, and the other forms' bricks are drawn so still.
//! That mesh is made for instances placed any way at all: each has a whole
//! matrix, and when the instances are set it copies them, turns each
//! instance's box by its matrix to find the box round them all, splits
//! every matrix into three attributes and converts every colour with a
//! power function. Measured on a lawn of 28,000 bricks that was 11 to 17
//! ms each time the lawn changed, which was every touch of the brush; and
//! each mesh is drawn as an object of its own, which sets every light's
//! uniforms again, 45 microseconds a mesh a frame.
//!
//! A brick is a box on the lattice: a corner, a size and a colour, ten
//! numbers, which the vertex shader (shaders/brick.vert) makes a place of
//! with one multiply and one add. So this is one object for all the
//! shapes: the lights are set once, and each shape is drawn with one call
//! for each tile of ground the form is divided into (see `tiles`), usually
//! one. Measured on the same lawn: 0.5 ms to give the card every brick
//! again, and the drawing of a frame down from 1.3 ms of the processor's
//! time to 0.3 (see docs/performance.md).
//!
//! Only the form being edited, since its grid's turn is one matrix for the
//! whole set. The other forms may each stand on a grid at an angle of its
//! own, which is a matrix an instance, and they are given to the card
//! only when one of them changes.

use crate::tiles::{Drawn, Key};
use std::collections::BTreeMap;
use three_d::*;

/// The vertex shader, in its own file so that it is edited, highlighted
/// and checked as GLSL (as the fragment shader is: see `material`).
const VERTEX: &str = include_str!("shaders/brick.vert");

/// One brick shape: its triangles, and the bricks of that shape in each tile.
struct Shape {
    positions: VertexBuffer<Vec3>,
    rim: VertexBuffer<f32>,
    tiles: BTreeMap<Key, Batch>,
}

/// The bricks of one shape in one tile, as the card has them.
struct Batch {
    at: InstanceBuffer<Vec3>,
    size: InstanceBuffer<Vec3>,
    colour: InstanceBuffer<Vec4>,
    /// the box round them, before the set is moved to where the form stands
    low: Vec3,
    high: Vec3,
}

pub struct Bricks {
    context: Context,
    shapes: BTreeMap<u8, Shape>,
    transformation: Mat4,
    /// what each of the 256 values of a colour's red, green or blue is as light, which is what the shader works in
    linear: [f32; 256],
}

impl Bricks {
    pub fn new(context: &Context) -> Bricks {
        // (three-d's own conversion, so a brick is the colour here that it is in the other forms' set)
        let mut linear = [0.0; 256];
        for (value, light) in linear.iter_mut().enumerate() {
            *light = Srgba::new_opaque(value as u8, 0, 0).to_linear_srgb().x;
        }
        Bricks { context: context.clone(), shapes: BTreeMap::new(), transformation: Mat4::identity(), linear }
    }

    /// Where the form stands, and how its grid is turned: every brick is moved by this.
    pub fn set_transformation(&mut self, transformation: Mat4) {
        self.transformation = transformation;
    }

    /// Give the card the bricks of the tile `key` as they now are, in
    /// place of what it had for that tile; none takes the tile away.
    pub fn set_tile(&mut self, key: Key, bricks: &[Drawn]) {
        let mut sorted: BTreeMap<u8, (Vec<Vec3>, Vec<Vec3>, Vec<Vec4>)> = BTreeMap::new();
        for (b, [r, g, blue]) in bricks {
            let (at, size, colour) = sorted.entry(b.mask).or_default();
            at.push(vec3(b.min[0] as f32, b.min[1] as f32, b.min[2] as f32));
            size.push(vec3(b.ext[0] as f32, b.ext[1] as f32, b.ext[2] as f32));
            colour.push(vec4(self.linear[*r as usize], self.linear[*g as usize], self.linear[*blue as usize], 1.0));
        }
        // (a shape the tile had bricks of and has none of now)
        for (_, shape) in self.shapes.iter_mut().filter(|(mask, _)| !sorted.contains_key(mask)) {
            shape.tiles.remove(&key);
        }
        for (mask, (at, size, colour)) in sorted {
            let context = &self.context;
            let shape = self.shapes.entry(mask).or_insert_with(|| {
                let mesh = crate::mesh::shape_mesh(mask);
                let positions: Vec<Vec3> = mesh.positions.iter().map(|p| vec3(p[0], p[1], p[2])).collect();
                Shape { positions: VertexBuffer::new_with_data(context, &positions), rim: VertexBuffer::new_with_data(context, &mesh.rim), tiles: BTreeMap::new() }
            });
            let low = at.iter().fold(vec3(f32::MAX, f32::MAX, f32::MAX), |low, p| vec3(low.x.min(p.x), low.y.min(p.y), low.z.min(p.z)));
            let high = at.iter().zip(&size).fold(vec3(f32::MIN, f32::MIN, f32::MIN), |high, (p, s)| vec3(high.x.max(p.x + s.x), high.y.max(p.y + s.y), high.z.max(p.z + s.z)));
            match shape.tiles.get_mut(&key) {
                // (a buffer the card has already is filled again, which makes none)
                Some(batch) => {
                    batch.at.fill(&at);
                    batch.size.fill(&size);
                    batch.colour.fill(&colour);
                    (batch.low, batch.high) = (low, high);
                }
                None => {
                    shape.tiles.insert(key, Batch { at: InstanceBuffer::new_with_data(context, &at), size: InstanceBuffer::new_with_data(context, &size), colour: InstanceBuffer::new_with_data(context, &colour), low, high });
                }
            }
        }
        self.shapes.retain(|_, shape| !shape.tiles.is_empty());
    }

    /// How many bricks the card has.
    pub fn count(&self) -> u32 {
        self.shapes.values().flat_map(|shape| shape.tiles.values()).map(|batch| batch.at.instance_count()).sum()
    }

    /// How many calls it takes to draw them: one for each shape in each tile.
    pub fn calls(&self) -> usize {
        self.shapes.values().map(|shape| shape.tiles.len()).sum()
    }
}

impl<'a> IntoIterator for &'a Bricks {
    type Item = &'a dyn Geometry;
    type IntoIter = std::iter::Once<&'a dyn Geometry>;

    fn into_iter(self) -> Self::IntoIter {
        std::iter::once(self)
    }
}

impl Geometry for Bricks {
    fn draw(&self, viewer: &dyn Viewer, program: &Program, render_states: RenderStates) {
        program.use_uniform("viewProjection", viewer.projection() * viewer.view());
        program.use_uniform("modelMatrix", self.transformation);
        for shape in self.shapes.values() {
            for batch in shape.tiles.values() {
                // (The shape's own attributes again for each tile, and not once for the shape: a draw call ends by
                // switching every attribute off, so the second tile of a shape was drawn from no corners at all.)
                program.use_vertex_attribute("position", &shape.positions);
                // (a pass that draws no lines, the shadows' or a click's, has no use for the rim or the colour, and its program has neither)
                if program.requires_attribute("rim") {
                    program.use_vertex_attribute("rim", &shape.rim);
                }
                program.use_instance_attribute("brick_at", &batch.at);
                program.use_instance_attribute("brick_size", &batch.size);
                if program.requires_attribute("brick_colour") {
                    program.use_instance_attribute("brick_colour", &batch.colour);
                }
                program.draw_arrays_instanced(render_states, viewer.viewport(), shape.positions.vertex_count(), batch.at.instance_count());
            }
        }
    }

    fn vertex_shader_source(&self) -> String {
        VERTEX.to_string()
    }

    fn id(&self) -> GeometryId {
        // (ids under 0x8000 are left for geometries defined outside three-d)
        GeometryId(0x0B5C)
    }

    fn aabb(&self) -> AxisAlignedBoundingBox {
        let mut all = AxisAlignedBoundingBox::EMPTY;
        for batch in self.shapes.values().flat_map(|shape| shape.tiles.values()) {
            all.expand(&[batch.low, batch.high]);
        }
        all.transformed(self.transformation)
    }

    fn render_with_material(&self, material: &dyn Material, viewer: &dyn Viewer, lights: &[&dyn Light]) {
        if let Err(e) = render_with_material(&self.context, viewer, self, material, lights) {
            panic!("{}", e.to_string());
        }
    }

    fn render_with_effect(&self, material: &dyn Effect, viewer: &dyn Viewer, lights: &[&dyn Light], color_texture: Option<ColorTexture>, depth_texture: Option<DepthTexture>) {
        if let Err(e) = render_with_effect(&self.context, viewer, self, material, lights, color_texture, depth_texture) {
            panic!("{}", e.to_string());
        }
    }
}
