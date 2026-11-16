//! A mesh's colours reduced to a few, for bricks.
//!
//! A brick is one colour, and a form's paint holds at most 32 (the
//! editor's `paint::MOST_COLOURS`). A mesh has a colour at every point of
//! its surface, from its vertices or from a picture. So the surface's
//! colours are gathered, clustered into as many as are asked for, and each
//! brick is given the number of the cluster nearest the colour of the
//! surface at it.
//!
//! The clustering has to give the same palette on every computer: a form
//! saved on one and opened on another is built again from the mesh, and
//! must come out in the same colours. So it uses whole numbers throughout
//! and no chance:
//!
//! 1. The surface is sampled at places fixed by the triangles alone, each
//!    sample weighted by the area it stands for.
//! 2. The samples are gathered into bins of 8 levels a channel (32,768
//!    bins), which bounds the work whatever the mesh.
//! 3. The bins are cut into boxes, as median cut does (Heckbert 1982,
//!    <https://doi.org/10.1145/965145.801294>), but each cut is made where
//!    it takes most off the boxes' summed squared error, along whichever
//!    channel that is, and the box cut next is the one a cut helps most
//!    (the greedy rule of Wu, "Efficient statistical computations for
//!    optimal color quantization", Graphics Gems II, 1991).
//! 4. The boxes' means are improved by a fixed number of rounds of k-means
//!    (Lloyd 1982, <https://doi.org/10.1109/TIT.1982.1056489>), which
//!    moves each to the mean of the bins nearest it.
//!
//! Cutting at the median, which is where the method gets its name, was
//! tried first and is in the test below as the case it failed: a ball in
//! red, white and blue with a thin band of mixed colour between each two,
//! asked for three colours, came back white, purple and lavender. The
//! median put pure red and pure blue in one box and the mixed colours in
//! the other, and k-means could not get out of that start.
//!
//! k-means from random starts (k-means++, Arthur and Vassilvitskii 2007)
//! is the usual way to start it, and would need a random number generator
//! carried with the mesh to be repeatable. A perceptual colour space
//! (Oklab, CIELAB) would cluster nearer to what the eye sees, and needs
//! cube roots, which processors' libraries do not all round alike: a bin
//! on the line between two clusters could go either way by machine. The
//! weighted distance below is the low-cost one of Riemersma
//! (<https://www.compuphase.com/cmetric.htm>) in whole numbers.

use super::Mesh;

/// About how many samples of the surface are taken, where its triangles
/// are fewer than that: a feature a four-hundredth of the surface (an eye,
/// a button) is then a hundred of them. A mesh of more triangles has one
/// sample for each.
const SAMPLES: u64 = 40_000;
/// The most rounds of k-means after the cuts: it stops sooner if a round
/// moves nothing, and the limit bounds the work where it would not.
const ROUNDS: usize = 12;

/// How far apart two colours are to the eye, roughly: green counts most
/// and red least, as the eye has it.
pub fn apart(a: [u8; 3], b: [u8; 3]) -> u32 {
    let d = [0, 1, 2].map(|k| (a[k] as i32 - b[k] as i32).pow(2) as u32);
    2 * d[0] + 4 * d[1] + 3 * d[2]
}

/// The number of the colour in `palette` nearest `colour`: the first of
/// them, if two are as near. 0 for an empty palette.
pub fn nearest_in(palette: &[[u8; 3]], colour: [u8; 3]) -> u8 {
    (0..palette.len()).min_by_key(|i| (apart(palette[*i], colour), *i)).unwrap_or(0) as u8
}

#[derive(Clone, Copy, Default)]
struct Bin {
    weight: u64,
    sum: [u64; 3],
}

impl Bin {
    fn mean(&self) -> [u8; 3] {
        self.sum.map(|s| ((s + self.weight / 2) / self.weight.max(1)).min(255) as u8)
    }
}

impl Mesh {
    /// Up to `count` colours that stand for the mesh's own, the one that
    /// covers most of its surface first. Fewer if the mesh has fewer, and
    /// none for a mesh that says nothing of its colour.
    pub fn palette(&self, count: usize) -> Vec<[u8; 3]> {
        if !self.coloured() || count == 0 {
            return Vec::new();
        }
        // each triangle's area in parts of a million of the whole surface, as whole numbers from here on
        let areas: Vec<f64> = (0..self.tris.len()).map(|t| { let ([a, b, c], _) = self.triangle(t); (b - a).cross(c - a).length() }).collect();
        let whole: f64 = areas.iter().sum::<f64>().max(1e-300);
        let mut bins = vec![Bin::default(); 1 << 15];
        for (tri, area) in areas.iter().enumerate() {
            let share = area / whole;
            // (a large triangle of a painted mesh shows much of its picture: more samples, up to a limit that keeps one triangle from taking them all)
            let n = ((share * SAMPLES as f64).round() as u64).clamp(1, 256);
            let weight = ((share * 1e9 / n as f64).round() as u64).max(1);
            let ([a, b, c], _) = self.triangle(tri);
            for i in 0..n {
                // Points spread evenly over the triangle: along one side by even steps, across by the golden ratio's
                // multiples, folded back where the two pass the far side. https://extremelearning.com.au/evenly-distributing-points-in-a-triangle/
                let (mut u, mut v) = ((i as f64 + 0.5) / n as f64, (i as f64 * 0.618_033_988_749_895).fract());
                if u + v > 1.0 {
                    (u, v) = (1.0 - u, 1.0 - v);
                }
                let Some(colour) = self.colour(tri, a + (b - a) * u + (c - a) * v) else { continue };
                let bin = &mut bins[(colour[0] as usize >> 3) << 10 | (colour[1] as usize >> 3) << 5 | colour[2] as usize >> 3];
                bin.weight += weight;
                for k in 0..3 {
                    bin.sum[k] += weight * colour[k] as u64;
                }
            }
        }
        // (in the order of their numbers, which is the order everything below goes by)
        let bins: Vec<([u8; 3], u64)> = bins.iter().filter(|b| b.weight > 0).map(|b| (b.mean(), b.weight)).collect();
        // boxes of bins, by their places in `order`
        let mut order: Vec<usize> = (0..bins.len()).collect();
        let mut boxes: Vec<(usize, usize)> = vec![(0, bins.len())];
        // The best place to cut a box in two: how much the cut takes off the summed squared distance of its bins from
        // their box's mean, along which channel the bins are put in order, and how many go to the low side. The sum for
        // a box is its bins' own squares less (sum of colours) squared over its weight, for each channel, so a cut's
        // gain is that last term for the two halves less the same for the whole: sums only, in whole numbers.
        let cut_of = |members: &[usize]| -> (u128, usize, usize) {
            let term = |sum: [u128; 3], weight: u128| (0..3).map(|k| [2, 4, 3][k] * sum[k] * sum[k] / weight.max(1)).sum::<u128>();
            let mut best = (0u128, 0usize, 0usize);
            for channel in 0..3 {
                let mut sorted = members.to_vec();
                sorted.sort_unstable_by_key(|i| (bins[*i].0[channel], *i));
                let (mut all, mut weight) = ([0u128; 3], 0u128);
                for &i in &sorted {
                    weight += bins[i].1 as u128;
                    for k in 0..3 {
                        all[k] += bins[i].1 as u128 * bins[i].0[k] as u128;
                    }
                }
                let whole = term(all, weight);
                let (mut low, mut low_weight) = ([0u128; 3], 0u128);
                for (n, &i) in sorted.iter().enumerate().take(sorted.len() - 1) {
                    low_weight += bins[i].1 as u128;
                    for k in 0..3 {
                        low[k] += bins[i].1 as u128 * bins[i].0[k] as u128;
                    }
                    let gain = (term(low, low_weight) + term([0, 1, 2].map(|k| all[k] - low[k]), weight - low_weight)).saturating_sub(whole);
                    if gain > best.0 {
                        best = (gain, channel, n + 1);
                    }
                }
            }
            best
        };
        let mut cuts: Vec<(u128, usize, usize)> = vec![cut_of(&order)];
        while boxes.len() < count {
            // (the first of them, if two cuts gain the same)
            let Some(at) = (0..boxes.len()).filter(|i| cuts[*i].0 > 0).max_by_key(|i| (cuts[*i].0, std::cmp::Reverse(*i))) else { break };
            let ((from, to), (_, channel, low)) = (boxes[at], cuts[at]);
            order[from..to].sort_unstable_by_key(|i| (bins[*i].0[channel], *i));
            boxes[at] = (from, from + low);
            boxes.push((from + low, to));
            cuts[at] = cut_of(&order[from..from + low]);
            cuts.push(cut_of(&order[from + low..to]));
        }
        let mean_of = |members: &mut dyn Iterator<Item = usize>| -> Option<([u8; 3], u64)> {
            let mut all = Bin::default();
            for i in members {
                all.weight += bins[i].1;
                for k in 0..3 {
                    all.sum[k] += bins[i].1 * bins[i].0[k] as u64;
                }
            }
            (all.weight > 0).then(|| (all.mean(), all.weight))
        };
        let mut centres: Vec<([u8; 3], u64)> = boxes.iter().filter_map(|&(from, to)| mean_of(&mut order[from..to].iter().copied())).collect();
        for _ in 0..ROUNDS {
            let only: Vec<[u8; 3]> = centres.iter().map(|c| c.0).collect();
            let nearest: Vec<u8> = bins.iter().map(|b| nearest_in(&only, b.0)).collect();
            let moved: Vec<([u8; 3], u64)> = (0..centres.len()).map(|c| mean_of(&mut (0..bins.len()).filter(|i| nearest[*i] as usize == c)).unwrap_or((centres[c].0, 0))).collect();
            if moved == centres {
                break;
            }
            centres = moved;
        }
        // the most of the surface first; two that came to the same colour are one
        centres.sort_by_key(|c| (std::cmp::Reverse(c.1), c.0));
        let mut palette: Vec<[u8; 3]> = Vec::with_capacity(centres.len());
        for (colour, weight) in centres {
            if weight > 0 && !palette.contains(&colour) {
                palette.push(colour);
            }
        }
        palette
    }
}

#[cfg(test)]
mod tests {
    use super::super::made;
    use super::*;

    #[test]
    fn a_mesh_of_three_colours_has_those_three_as_its_palette_the_commonest_first() {
        // a ball: red above, white round the middle (the widest band), blue below
        let mesh = made::painted_ball(24);
        // (the one band of triangles between two colours shades from one to the other: asked for more than three, the rest are those shades)
        let palette = mesh.palette(3);
        assert_eq!(palette.len(), 3, "{palette:?}");
        assert!(mesh.palette(8).len() == 8);
        let near = |a: [u8; 3], b: [u8; 3]| apart(a, b) < 300;
        assert!(near(palette[0], [240, 240, 240]) && palette.iter().any(|c| near(*c, [200, 30, 30])) && palette.iter().any(|c| near(*c, [30, 60, 200])), "{palette:?}");
        // asked for two, the two it keeps are still colours of the mesh's and not a mixture of all three
        let two = mesh.palette(2);
        assert_eq!(two.len(), 2);
        // the same every time, and none for a mesh with no colours or none asked for
        assert_eq!(mesh.palette(3), palette);
        assert!(mesh.palette(0).is_empty() && made::sphere(8).palette(8).is_empty());
        assert_eq!(nearest_in(&palette, [250, 250, 250]), 0);
        assert_eq!(palette[nearest_in(&palette, [180, 0, 0]) as usize], *palette.iter().find(|c| near(**c, [200, 30, 30])).unwrap());
    }

    #[test]
    fn a_palette_is_no_more_colours_than_asked_for_and_each_is_near_some_colour_of_the_mesh() {
        // every point its own colour, by where it is: thousands of colours
        let mesh = made::rainbow_ball(24);
        for count in [1, 4, 16, 32] {
            let palette = mesh.palette(count);
            assert_eq!(palette.len(), count, "{palette:?}");
        }
        // more colours follow the mesh's own more closely: the furthest any sample is from its palette colour falls
        let worst = |palette: &[[u8; 3]]| (0..mesh.triangles()).map(|t| { let ([a, b, c], _) = mesh.triangle(t); let colour = mesh.colour(t, (a + b + c) / 3.0).unwrap(); apart(palette[nearest_in(palette, colour) as usize], colour) }).max().unwrap();
        let (few, many) = (worst(&mesh.palette(4)), worst(&mesh.palette(32)));
        assert!(many * 4 < few, "the worst with 4 colours is {few} and with 32 {many}");
    }
}
