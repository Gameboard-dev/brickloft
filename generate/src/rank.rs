//! Which of several pictures is most likely to make a good mesh.
//!
//! About half of the pictures Stable Diffusion makes for a prompt are no use
//! to TripoSR: the object runs off the frame, or there are two of it, or it
//! stands in a scene, or it is seen from overhead. Each of those shows in
//! what the cut-out found (`cutout::Report`), which costs a few
//! milliseconds a picture and needs no model. So every candidate is cut out,
//! judged here on those numbers, and the candidates are returned best first
//! with the reasons, for the editor to show in that order and a person to
//! overrule.
//!
//! A model that scores pictures against the prompt (CLIP's image half) was
//! the other way. It would say whether the picture is of a barrel, which
//! this cannot; it would not say whether the barrel is whole and alone on a
//! plain ground, which is what decides the mesh; and it is several hundred
//! megabytes more to fetch, under another licence to be read first.

use crate::cutout::Report;
use serde_json::{json, Value};

/// The object touches this share of the frame or more: it is cut by the frame.
const ON_FRAME: f32 = 0.004;
/// The object is this share of the picture at least, and at most.
const COVERAGE: std::ops::RangeInclusive<f32> = 0.05..=0.75;
/// Less than this share of the frame is plain ground: the picture is a scene.
const PLAIN_FRAME: f32 = 0.6;
/// More than this share of the object is the ground's colour: the cut is doubtful.
const GROUND_LIKE: f32 = 0.35;
/// The lower outline's rise (see `cutout::rise`) below which the view is level, and above which it is from high up.
const RISE_LEVEL: f32 = 0.3;
const RISE_HIGH: f32 = 0.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Level,
    Raised,
    High,
}

impl View {
    pub fn name(self) -> &'static str {
        match self {
            View::Level => "level",
            View::Raised => "raised",
            View::High => "high",
        }
    }

    pub fn of(rise: f32) -> View {
        if rise < RISE_LEVEL {
            View::Level
        } else if rise < RISE_HIGH {
            View::Raised
        } else {
            View::High
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Judged {
    /// From 0 to 1: higher is likelier to make a good mesh.
    pub score: f32,
    /// Nothing was found that rules the picture out.
    pub usable: bool,
    /// What is wrong with it, in words for a person. Empty if nothing is.
    pub problems: Vec<&'static str>,
    pub report: Option<Report>,
}

/// Judges a picture by what the cut-out found in it, or by its finding nothing.
pub fn judge(report: Option<&Report>) -> Judged {
    let Some(report) = report else {
        return Judged { score: 0.0, usable: false, problems: vec!["no object was found on a plain ground"], report: None };
    };
    let (mut score, mut usable, mut problems) = (1.0f32, true, Vec::new());
    if report.on_frame >= ON_FRAME {
        // the more of the frame it touches, the more of it is missing
        score -= 0.35 + 0.35 * (report.on_frame / 0.15).min(1.0);
        usable = false;
        problems.push("the object is cut off by the edge of the picture");
    }
    if report.pieces > 1 {
        score -= 0.25;
        usable = false;
        problems.push("there is more than one object");
    }
    if report.plain_frame < PLAIN_FRAME {
        score -= 0.3 * (1.0 - report.plain_frame / PLAIN_FRAME);
        usable &= report.plain_frame >= PLAIN_FRAME / 2.0;
        problems.push("the ground is not plain");
    }
    if report.coverage < *COVERAGE.start() {
        score -= 0.3;
        usable = false;
        problems.push("the object is very small in the picture");
    } else if report.coverage > *COVERAGE.end() {
        score -= 0.3;
        usable = false;
        problems.push("the object fills nearly the whole picture");
    }
    if report.ground_like > GROUND_LIKE {
        // the more of it, the less the cut-out can be trusted
        score -= 0.15 + 0.25 * ((report.ground_like - GROUND_LIKE) / (1.0 - GROUND_LIKE));
        problems.push("the object and the ground are alike in colour, so some ground may be left around it");
    }
    match View::of(report.rise) {
        View::Level => {}
        View::Raised => score -= 0.08,
        View::High => {
            // still usable: the mesh comes out tipped, and `level` stands most of those up again
            score -= 0.2;
            problems.push("it looks taken from high above, so the mesh may come out tilted");
        }
    }
    Judged { score: score.clamp(0.0, 1.0), usable, problems, report: Some(report.clone()) }
}

fn rounded(value: f32) -> f64 {
    (value as f64 * 1000.0).round() / 1000.0
}

/// What the result says of a cut-out.
pub fn report_json(report: &Report) -> Value {
    json!({
        "coverage": rounded(report.coverage),
        "on_frame": rounded(report.on_frame),
        "pieces": report.pieces,
        "plain_frame": rounded(report.plain_frame),
        "shadow_removed": report.shadow_removed,
        "holes": report.holes,
        "ground_like": rounded(report.ground_like),
        "rise": rounded(report.rise),
        "view": View::of(report.rise).name(),
    })
}

/// The candidates best first, each with its rank and what was found. Two
/// with the same score stay in the order they were made, so the answer is
/// the same every time.
pub fn ranked(mut candidates: Vec<(Judged, Value)>) -> Vec<Value> {
    candidates.sort_by(|a, b| b.0.score.total_cmp(&a.0.score));
    candidates.into_iter().enumerate().map(|(rank, (judged, mut fields))| {
        if let Value::Object(fields) = &mut fields {
            fields.insert("rank".into(), json!(rank + 1));
            fields.insert("score".into(), json!(rounded(judged.score)));
            fields.insert("usable".into(), json!(judged.usable));
            fields.insert("problems".into(), json!(judged.problems));
            fields.insert("cutout".into(), judged.report.as_ref().map_or(Value::Null, report_json));
        }
        fields
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cutout::{self, tests::{boxed, studio}};
    use image::Rgba;

    fn good() -> Report {
        Report { coverage: 0.3, on_frame: 0.0, pieces: 1, plain_frame: 1.0, shadow_removed: false, holes: 0, ground_like: 0.05, rise: 0.2, bounds: [0, 0, 10, 10] }
    }

    #[test]
    fn a_whole_object_alone_seen_level_is_best() {
        let best = judge(Some(&good()));
        assert!(best.usable && best.problems.is_empty());
        assert_eq!(best.score, 1.0);
        let worse = [
            Report { on_frame: 0.05, ..good() },
            Report { pieces: 2, ..good() },
            Report { plain_frame: 0.2, ..good() },
            Report { coverage: 0.01, ..good() },
            Report { coverage: 0.9, ..good() },
            Report { ground_like: 0.7, ..good() },
            Report { rise: 0.7, ..good() },
        ];
        for report in &worse {
            let judged = judge(Some(report));
            assert!(judged.score < best.score, "{report:?}");
            assert_eq!(judged.problems.len(), 1, "{report:?}");
        }
        // cut off is worse than seen from above, and nothing found is worst
        assert!(judge(Some(&worse[0])).score < judge(Some(&worse[6])).score);
        assert!(!judge(Some(&worse[0])).usable && judge(Some(&worse[6])).usable);
        let none = judge(None);
        assert_eq!((none.score, none.usable), (0.0, false));
    }

    #[test]
    fn candidates_come_back_best_first() {
        // three made pictures: a box cut by the frame, a whole box, and nothing but ground
        let mut cut_off = studio(256, 256);
        for y in 80..256 {
            for x in 0..110 {
                cut_off.put_pixel(x, y, Rgba([170, 50, 40, 255]));
            }
        }
        let pictures = [("cut off", cut_off), ("whole", boxed()), ("empty", studio(256, 256))];
        let candidates = pictures.iter().map(|(name, picture)| (judge(cutout::object_in(picture).map(|(_, r)| r).as_ref()), json!({ "name": name }))).collect();
        let ranked = ranked(candidates);
        let order: Vec<&str> = ranked.iter().map(|c| c["name"].as_str().unwrap()).collect();
        assert_eq!(order, ["whole", "cut off", "empty"]);
        assert_eq!(ranked[0]["rank"], 1);
        assert_eq!(ranked[0]["usable"], true);
        assert_eq!(ranked[0]["cutout"]["pieces"], 1);
        assert_eq!(ranked[1]["problems"][0], "the object is cut off by the edge of the picture");
        assert_eq!(ranked[2]["cutout"], Value::Null);
        assert!(ranked[0]["score"].as_f64() > ranked[1]["score"].as_f64() && ranked[1]["score"].as_f64() > ranked[2]["score"].as_f64());
    }

    #[test]
    fn equal_scores_keep_their_order() {
        let ranked = ranked((0..4).map(|i| (judge(Some(&good())), json!({ "seed": i }))).collect());
        assert_eq!(ranked.iter().map(|c| c["seed"].as_u64().unwrap()).collect::<Vec<_>>(), [0, 1, 2, 3]);
    }
}
