//! Command-line surface of the input-latency driver: which workload a run
//! measures, within what bounds, and what the harness tells the operator.

use super::fixture::{Base, Content};

pub(super) const USAGE: &str = "measure-input [options]\n  --samples N       independent runs (default 1; max 10)\n  --strokes N       strokes per run (default 1; max 100)\n  --moves N         pointer moves per stroke (default 1; max 1000)\n  --canvas N        canvas width (2 = bounded fixture; default 2; max 4096)\n  --canvas-height N canvas height (default: same as --canvas)\n  --layers N        raster layers in the generated fixture (default 1; max 4)\n  --brush-sizes A,B brush sizes in document px (default 2)\n  --base blank|opaque base layer for a generated canvas (default opaque)\n  --content plain|canonical layer content of a generated canvas\n                  (default plain; canonical = distinct patterned layers)\n  --case stroke|click|hover|slider-adjust|slider-opacity|dialog-drag|pan|transform  measured interaction (default stroke)\n  --mode cold|warm  warm-up phase before the measured samples (default cold)\n  --warmup N        warm-up events (default 3 warm, 0 cold)\n  --fixture-out PATH  copy the staged fixture here so it can be hashed\n  --help            show this help\n\nCanonical fixture: --canvas 4096 --canvas-height 1024 --layers 3 --content canonical --brush-sizes 800,40 --samples 3 --strokes 12 --moves 40";

const MAX_SAMPLES: usize = 10;
const MAX_STROKES: usize = 100;
const MAX_MOVES: usize = 1_000;
const MAX_CANVAS: u32 = 4_096;
const MAX_BRUSH: u32 = 4_096;
const MAX_LAYERS: usize = 4;

/// Which production interaction the run drives. `stroke` is the brush path;
/// `click` is a Move-tool press/release on the page (a selection/metadata
/// change); `hover` is a single pointer move with the Brush cursor armed. The
/// remaining cases are drags: a slider track, a dialog titlebar, a middle
/// pan, and a Move transform.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Case {
    Stroke,
    Click,
    Hover,
    SliderAdjust,
    SliderOpacity,
    DialogDrag,
    Pan,
    Transform,
}

impl Case {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Stroke => "stroke",
            Self::Click => "click",
            Self::Hover => "hover",
            Self::SliderAdjust => "slider-adjust",
            Self::SliderOpacity => "slider-opacity",
            Self::DialogDrag => "dialog-drag",
            Self::Pan => "pan",
            Self::Transform => "transform",
        }
    }

    pub(super) fn is_drag(self) -> bool {
        matches!(
            self,
            Self::SliderAdjust
                | Self::SliderOpacity
                | Self::DialogDrag
                | Self::Pan
                | Self::Transform
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Config {
    pub(super) samples: usize,
    pub(super) strokes: usize,
    pub(super) moves: usize,
    pub(super) canvas: u32,
    pub(super) canvas_height: u32,
    pub(super) layers: usize,
    pub(super) brush_sizes: Vec<u32>,
    pub(super) base: Base,
    pub(super) content: Content,
    pub(super) case: Case,
    pub(super) mode: String,
    pub(super) warmup: usize,
    pub(super) fixture_out: Option<String>,
}

impl Config {
    pub(super) fn height(&self) -> u32 {
        if self.canvas_height == 0 {
            self.canvas
        } else {
            self.canvas_height
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            samples: 1,
            strokes: 1,
            moves: 1,
            canvas: 2,
            canvas_height: 0,
            layers: 1,
            brush_sizes: vec![2],
            base: Base::Opaque,
            content: Content::Plain,
            case: Case::Stroke,
            mode: String::from("cold"),
            warmup: 0,
            fixture_out: None,
        }
    }
}

/// `Ok(None)` means the operator asked for help rather than a run.
pub(super) fn parse_args(args: &[String]) -> Result<Option<Config>, String> {
    let mut config = Config::default();
    let mut warmup_given = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--help" => return Ok(None),
            "--samples" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--samples requires a value".to_owned())?;
                config.samples = value
                    .parse::<usize>()
                    .map_err(|_| format!("invalid sample count: {value}"))?;
                if !(1..=MAX_SAMPLES).contains(&config.samples) {
                    return Err(format!("--samples must be in 1..={MAX_SAMPLES}"));
                }
            }
            "--strokes" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--strokes requires a value".to_owned())?;
                config.strokes = value
                    .parse::<usize>()
                    .map_err(|_| format!("invalid stroke count: {value}"))?;
                if !(1..=MAX_STROKES).contains(&config.strokes) {
                    return Err(format!("--strokes must be in 1..={MAX_STROKES}"));
                }
            }
            "--moves" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--moves requires a value".to_owned())?;
                config.moves = value
                    .parse::<usize>()
                    .map_err(|_| format!("invalid move count: {value}"))?;
                if !(1..=MAX_MOVES).contains(&config.moves) {
                    return Err(format!("--moves must be in 1..={MAX_MOVES}"));
                }
            }
            "--canvas" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--canvas requires a value".to_owned())?;
                config.canvas = value
                    .parse::<u32>()
                    .map_err(|_| format!("invalid canvas size: {value}"))?;
                if !(2..=MAX_CANVAS).contains(&config.canvas) {
                    return Err(format!("--canvas must be in 2..={MAX_CANVAS}"));
                }
            }
            "--canvas-height" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--canvas-height requires a value".to_owned())?;
                config.canvas_height = value
                    .parse::<u32>()
                    .map_err(|_| format!("invalid canvas height: {value}"))?;
                if !(2..=MAX_CANVAS).contains(&config.canvas_height) {
                    return Err(format!("--canvas-height must be in 2..={MAX_CANVAS}"));
                }
            }
            "--layers" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--layers requires a value".to_owned())?;
                config.layers = value
                    .parse::<usize>()
                    .map_err(|_| format!("invalid layer count: {value}"))?;
                if !(1..=MAX_LAYERS).contains(&config.layers) {
                    return Err(format!("--layers must be in 1..={MAX_LAYERS}"));
                }
            }
            "--brush-sizes" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--brush-sizes requires a value".to_owned())?;
                let mut sizes = Vec::new();
                for part in value.split(',') {
                    let size = part
                        .parse::<u32>()
                        .map_err(|_| format!("invalid brush size: {part}"))?;
                    if !(1..=MAX_BRUSH).contains(&size) {
                        return Err(format!("brush sizes must be in 1..={MAX_BRUSH}"));
                    }
                    sizes.push(size);
                }
                if sizes.is_empty() {
                    return Err("--brush-sizes requires at least one value".to_owned());
                }
                config.brush_sizes = sizes;
            }
            "--base" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--base requires a value".to_owned())?;
                config.base = match value.as_str() {
                    "blank" => Base::Blank,
                    "opaque" => Base::Opaque,
                    other => return Err(format!("unknown base: {other}")),
                };
            }
            "--content" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--content requires a value".to_owned())?;
                config.content = match value.as_str() {
                    "plain" => Content::Plain,
                    "canonical" => Content::Canonical,
                    other => return Err(format!("unknown content: {other}")),
                };
            }
            "--case" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--case requires a value".to_owned())?;
                config.case = match value.as_str() {
                    "stroke" => Case::Stroke,
                    "click" => Case::Click,
                    "hover" => Case::Hover,
                    "slider-adjust" => Case::SliderAdjust,
                    "slider-opacity" => Case::SliderOpacity,
                    "dialog-drag" => Case::DialogDrag,
                    "pan" => Case::Pan,
                    "transform" => Case::Transform,
                    other => return Err(format!("unknown case: {other}")),
                };
            }
            "--mode" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--mode requires a value".to_owned())?;
                if value != "cold" && value != "warm" {
                    return Err(format!("--mode: expected cold or warm, got {value}"));
                }
                config.mode = value.to_string();
            }
            "--warmup" => {
                warmup_given = true;
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--warmup requires a value".to_owned())?;
                config.warmup = value
                    .parse::<usize>()
                    .map_err(|_| format!("invalid warm-up count: {value}"))?;
                if config.warmup > MAX_STROKES {
                    return Err(format!("--warmup must be in 0..={MAX_STROKES}"));
                }
            }
            "--fixture-out" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--fixture-out requires a value".to_owned())?;
                config.fixture_out = Some(value.to_string());
            }
            other => return Err(format!("unknown option: {other}")),
        }
        index += 1;
    }
    if config.mode == "warm" && !warmup_given {
        config.warmup = 3;
    }
    Ok(Some(config))
}

#[cfg(test)]
mod tests {
    use super::super::fixture::{Base, Content};
    use super::{parse_args, Case, Config};

    #[test]
    fn parser_bounds_workload() {
        assert_eq!(parse_args(&[]), Ok(Some(Config::default())));
        assert!(parse_args(&["--samples".into(), "2".into()]).is_ok());
        assert!(parse_args(&["--samples".into(), "0".into()]).is_err());
        assert!(parse_args(&["--samples".into(), "11".into()]).is_err());
        assert!(parse_args(&["--strokes".into(), "0".into()]).is_err());
        assert!(parse_args(&["--moves".into(), "1001".into()]).is_err());
        assert!(parse_args(&["--canvas".into(), "1".into()]).is_err());
        assert!(parse_args(&["--canvas".into(), "4097".into()]).is_err());
        assert!(parse_args(&["--canvas-height".into(), "1".into()]).is_err());
        assert!(parse_args(&["--canvas-height".into(), "4097".into()]).is_err());
        assert!(parse_args(&["--layers".into(), "0".into()]).is_err());
        assert!(parse_args(&["--layers".into(), "5".into()]).is_err());
        assert!(parse_args(&["--brush-sizes".into(), "0".into()]).is_err());
        assert!(parse_args(&["--base".into(), "both".into()]).is_err());
        assert!(parse_args(&["--content".into(), "checkerboard".into()]).is_err());
        assert!(parse_args(&["--case".into(), "wheel".into()]).is_err());
        assert!(parse_args(&["--mode".into(), "tepid".into()]).is_err());
        assert!(parse_args(&["--warmup".into(), "101".into()]).is_err());
        assert!(parse_args(&["--fixture-out".into()]).is_err());
        assert_eq!(parse_args(&["--help".into()]), Ok(None));
    }

    #[test]
    fn parser_defaults_keep_a_cold_plain_stroke_run() {
        let config = Config::default();
        assert_eq!(config.case, Case::Stroke);
        assert_eq!(config.mode, "cold");
        assert_eq!(config.warmup, 0);
        assert_eq!(config.content, Content::Plain);
    }

    #[test]
    fn parser_selects_the_drag_cases() {
        for (value, case) in [
            ("slider-adjust", Case::SliderAdjust),
            ("slider-opacity", Case::SliderOpacity),
            ("dialog-drag", Case::DialogDrag),
            ("pan", Case::Pan),
            ("transform", Case::Transform),
        ] {
            let config = parse_args(&["--case".into(), value.into()])
                .expect("parses")
                .expect("not help");
            assert_eq!(config.case, case);
            assert!(case.is_drag());
        }
    }

    #[test]
    fn parser_warms_up_only_when_the_run_is_warm() {
        assert_eq!(
            parse_args(&["--mode".into(), "warm".into()])
                .expect("parses")
                .expect("not help")
                .warmup,
            3
        );
        let explicit: Vec<String> = ["--mode", "warm", "--warmup", "0"]
            .iter()
            .map(|value| (*value).to_owned())
            .collect();
        assert_eq!(
            parse_args(&explicit)
                .expect("parses")
                .expect("not help")
                .warmup,
            0
        );
    }

    #[test]
    fn parser_selects_the_measured_case() {
        for (value, expected) in [
            ("stroke", Case::Stroke),
            ("click", Case::Click),
            ("hover", Case::Hover),
        ] {
            let config = parse_args(&["--case".into(), value.into()])
                .expect("case parses")
                .expect("not help");
            assert_eq!(config.case, expected);
        }
    }

    #[test]
    fn parser_accepts_the_canonical_fixture_workload() {
        let args: Vec<String> = [
            "--samples",
            "3",
            "--strokes",
            "12",
            "--moves",
            "40",
            "--canvas",
            "4096",
            "--canvas-height",
            "1024",
            "--layers",
            "3",
            "--content",
            "canonical",
            "--brush-sizes",
            "800,40",
            "--base",
            "blank",
        ]
        .iter()
        .map(|value| (*value).to_owned())
        .collect();
        let config = parse_args(&args)
            .expect("canonical arguments parse")
            .expect("canonical arguments are not help");
        assert_eq!(config.samples, 3);
        assert_eq!(config.strokes, 12);
        assert_eq!(config.moves, 40);
        assert_eq!(config.canvas, 4096);
        assert_eq!(config.height(), 1024);
        assert_eq!(config.layers, 3);
        assert_eq!(config.content, Content::Canonical);
        assert_eq!(config.brush_sizes, vec![800, 40]);
        assert_eq!(config.base, Base::Blank);
    }

    #[test]
    fn canvas_height_defaults_to_width() {
        let args: Vec<String> = ["--canvas", "4096"]
            .iter()
            .map(|value| (*value).to_owned())
            .collect();
        let config = parse_args(&args).expect("width parses").expect("not help");
        assert_eq!(config.canvas, 4096);
        assert_eq!(config.height(), 4096);
        assert_eq!(config.layers, 1);
    }
}
