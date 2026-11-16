// A brick of the form being edited: one corner of the unit shape, put
// where the brick is and stretched to its size (see bricks.rs).
//
// A brick is a box on the lattice, so where it is and how large are six
// numbers, given once a brick, and the corner's place is one multiply and
// one add. three-d's own instanced mesh takes a whole matrix for each
// instance (twelve numbers in three attributes) and multiplies by it, which
// is what a turned or sheared instance needs and a brick does not.
//
// The names of what goes out (pos, uvs, col, instance_id) are the ones
// three-d's fragment shaders read: the shadow pass reads pos, and a click
// on the view reads pos and instance_id (its mesh.vert writes the same).

uniform mat4 viewProjection;
// where the form stands, and how its grid is turned
uniform mat4 modelMatrix;

// a corner of the shape, each coordinate from 0 to 1 across its box
in vec3 position;
// how near the rim of its face the corner is: 0 on it, 1 at the middle
in float rim;
// the brick: its lowest corner, its size, and its colour (linear)
in vec3 brick_at;
in vec3 brick_size;
in vec4 brick_colour;

out vec3 pos;
out vec2 uvs;
out vec4 col;
flat out int instance_id;

void main()
{
    vec4 world = modelMatrix * vec4(brick_at + position * brick_size, 1.0);
    gl_Position = viewProjection * world;
    pos = world.xyz / world.w;
    uvs = vec2(rim, 0.0);
    col = brick_colour;
    instance_id = gl_InstanceID;
}
