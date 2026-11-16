// The bricks' surface: lit plastic with a dark line along every face edge.
// Appended to three-d's lighting, tone-mapping and colour-mapping sources
// (see material.rs), which supply calculate_lighting, tone_mapping and
// color_mapping.

uniform vec3 cameraPosition;
uniform float lineShade;
uniform float lineWidth;
uniform float roughness;

in vec3 pos;
// the brick's colour: one per brick, already linear
in vec4 col;
// x is 0 on the rim of the face and 1 at its middle (see mesh.rs)
in vec2 uvs;

layout (location = 0) out vec4 outColor;

void main()
{
    // The face's own normal, from how the position changes across the pixel.
    // Every face is flat, so this is exact, and unlike a normal stored on the
    // vertices it stays right when a brick is stretched unevenly.
    vec3 normal = normalize(cross(dFdx(pos), dFdy(pos)));
    if (dot(normal, cameraPosition - pos) < 0.0) normal = -normal;

    vec3 lit = calculate_lighting(cameraPosition, col.rgb, pos, normal, 0.0, roughness, 1.0);
    // a little darker underneath, as under an open sky
    lit *= mix(0.72, 1.0, normal.z * 0.5 + 0.5);

    // a line where the pixel is within lineWidth pixels of the face's rim
    float width = fwidth(uvs.x);
    float face = smoothstep(width * (lineWidth - 0.5), width * (lineWidth + 0.5), uvs.x);
    lit *= mix(lineShade, 1.0, face);

    // (how solid: 1 but for water and ice, which are drawn blended over what is behind them)
    outColor = vec4(color_mapping(tone_mapping(lit)), col.a);
}
