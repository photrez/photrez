use super::{Config, PATCH_COMMIT_MOVES, USAGE};
use photrez::display::canvas_bridge::{MAX_CANVAS_DIM, TILE_SIZE};

/// Raster layers a fixture document may carry; the canonical fixture has three.
const MAX_LAYERS: usize = 4;

pub(super) fn parse_args() -> Result<Config, String> {
    parse_args_from(std::env::args().skip(1))
}

fn parse_args_from(arguments: impl IntoIterator<Item = String>) -> Result<Config, String> {
    let mut size = 4096usize;
    let mut width: Option<usize> = None;
    let mut height: Option<usize> = None;
    let mut layers: Option<usize> = None;
    let mut content: Option<String> = None;
    let mut strokes = 12usize;
    let mut moves = 40usize;
    let mut brush_px = 32.0f32;
    let mut warmup: Option<usize> = None;
    let mut mode = String::from("warm");
    let mut red = String::new();
    let mut demo = false;
    let mut control = false;
    let mut tile_size = TILE_SIZE;
    let mut commit_moves = PATCH_COMMIT_MOVES;
    let mut patch_union = false;
    let mut progressive = false;
    let mut experiment_flag = false;
    let mut presentmon_csv = None;
    let mut presentmon_skip_ms = 0.0f64;
    let mut fixture_png = None;
    let mut out_path = None;

    let mut args = arguments.into_iter();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--size" => size = parse_usize(&mut args, "--size")?,
            "--width" => width = Some(parse_usize(&mut args, "--width")?),
            "--height" => height = Some(parse_usize(&mut args, "--height")?),
            "--layers" => layers = Some(parse_usize(&mut args, "--layers")?),
            "--content" => content = Some(next_value(&mut args, "--content")?),
            "--fixture-png" => fixture_png = Some(next_value(&mut args, "--fixture-png")?),
            "--strokes" => strokes = parse_usize(&mut args, "--strokes")?,
            "--moves" => moves = parse_usize(&mut args, "--moves")?,
            "--brush" => {
                let raw = next_value(&mut args, "--brush")?;
                brush_px = raw
                    .parse::<f32>()
                    .map_err(|_| format!("--brush: not a number: {raw}"))?;
            }
            "--warmup" => warmup = Some(parse_usize(&mut args, "--warmup")?),
            "--tile-size" => {
                experiment_flag = true;
                let value = parse_usize(&mut args, "--tile-size")?;
                tile_size = u32::try_from(value)
                    .map_err(|_| format!("--tile-size does not fit a u32: {value}"))?;
            }
            "--commit-moves" => {
                experiment_flag = true;
                commit_moves = parse_usize(&mut args, "--commit-moves")?;
            }
            "--patch-union" => {
                experiment_flag = true;
                patch_union = true;
            }
            "--progressive" => {
                experiment_flag = true;
                progressive = true;
            }
            "--mode" => {
                let value = next_value(&mut args, "--mode")?;
                if value != "cold" && value != "warm" {
                    return Err(format!("--mode: expected cold or warm, got {value}"));
                }
                mode = value;
            }
            "--red" => red = next_value(&mut args, "--red")?,
            "--presentmon-csv" => presentmon_csv = Some(next_value(&mut args, "--presentmon-csv")?),
            "--presentmon-skip-ms" => {
                let raw = next_value(&mut args, "--presentmon-skip-ms")?;
                presentmon_skip_ms = raw
                    .parse::<f64>()
                    .map_err(|_| format!("--presentmon-skip-ms: not a number: {raw}"))?;
            }
            "--out" => out_path = Some(next_value(&mut args, "--out")?),
            "--demo" => demo = true,
            "--control" => control = true,
            "--help" | "-h" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }

    if !(1..=16384).contains(&size) {
        return Err(format!("--size must be 1..=16384, got {size}"));
    }
    let width = width.unwrap_or(size);
    let height = height.unwrap_or(size);
    if !(1..=16384).contains(&width) {
        return Err(format!("--width must be 1..=16384, got {width}"));
    }
    if !(1..=16384).contains(&height) {
        return Err(format!("--height must be 1..=16384, got {height}"));
    }
    let layers = layers.unwrap_or(1);
    if !(1..=MAX_LAYERS).contains(&layers) {
        return Err(format!("--layers must be 1..={MAX_LAYERS}, got {layers}"));
    }
    let content = content.unwrap_or(if layers > 1 {
        String::from("canonical")
    } else {
        String::from("plain")
    });
    if content != "plain" && content != "canonical" {
        return Err(format!(
            "--content: expected plain or canonical, got {content}"
        ));
    }
    if tile_size == 0 || tile_size > MAX_CANVAS_DIM {
        return Err(format!(
            "--tile-size must be 1..={MAX_CANVAS_DIM}, got {tile_size}"
        ));
    }
    if commit_moves == 0 {
        return Err("--commit-moves must be at least 1".to_string());
    }
    let strokes = strokes.max(1);
    let moves = moves.max(1);
    let total = strokes
        .checked_mul(moves)
        .ok_or_else(|| "--strokes * --moves overflows usize".to_string())?;
    if total > 1_000_000 {
        return Err(format!(
            "--strokes * --moves must be <= 1,000,000, got {total}"
        ));
    }
    if !brush_px.is_finite() || !(1.0..=256.0).contains(&brush_px) {
        return Err(format!("--brush must be 1..=256, got {brush_px}"));
    }
    if !red.is_empty() && red != "upload" && red != "dirty" {
        return Err(format!(
            "--red: unknown knob {red}; expected `upload` or `dirty`"
        ));
    }
    if !presentmon_skip_ms.is_finite() || presentmon_skip_ms < 0.0 {
        return Err(format!(
            "--presentmon-skip-ms must be a finite value >= 0, got {presentmon_skip_ms}"
        ));
    }
    if control && !red.is_empty() {
        return Err("--control cannot be combined with --red".to_string());
    }
    if control && experiment_flag {
        return Err(
            "--control cannot be combined with --tile-size, --commit-moves, --patch-union, or --progressive"
                .to_string(),
        );
    }

    let warmup = warmup.unwrap_or(if mode == "cold" { 0 } else { 3 });

    Ok(Config {
        size,
        width,
        height,
        layers,
        content,
        strokes,
        moves,
        brush_px,
        warmup,
        mode,
        red,
        demo,
        control,
        tile_size,
        commit_moves,
        patch_union,
        progressive,
        presentmon_csv,
        presentmon_skip_ms,
        fixture_png,
        out_path,
    })
}

fn next_value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    args.next().ok_or_else(|| format!("{flag} needs a value"))
}

fn parse_usize(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<usize, String> {
    let raw = next_value(args, flag)?;
    raw.parse::<usize>()
        .map_err(|_| format!("{flag}: not a number: {raw}"))
}

#[cfg(test)]
mod tests {
    use super::parse_args_from;

    fn argument_list(arguments: &[&str]) -> Vec<String> {
        arguments.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn input_flag_is_rejected() {
        let arguments = ["--input".to_string()];
        assert_eq!(
            parse_args_from(arguments).err().as_deref(),
            Some("unknown argument: --input")
        );
    }

    #[test]
    fn size_only_keeps_the_square_plain_single_layer_fixture() {
        let config = parse_args_from(argument_list(&["--size", "2048"])).expect("valid");
        assert_eq!(
            (config.size, config.width, config.height),
            (2048, 2048, 2048)
        );
        assert_eq!(config.layers, 1);
        assert_eq!(config.content, "plain");
    }

    #[test]
    fn defaults_match_the_recorded_recipe() {
        let config = parse_args_from(argument_list(&[])).expect("valid");
        assert_eq!(
            (config.size, config.width, config.height),
            (4096, 4096, 4096)
        );
        assert_eq!((config.layers, config.content.as_str()), (1, "plain"));
    }

    #[test]
    fn width_height_and_layers_form_the_canonical_fixture() {
        let config = parse_args_from(argument_list(&[
            "--width", "4096", "--height", "1024", "--layers", "3",
        ]))
        .expect("valid");
        assert_eq!(
            (config.width, config.height, config.layers),
            (4096, 1024, 3)
        );
        assert_eq!(config.content, "canonical");
    }

    #[test]
    fn one_layer_can_opt_into_patterned_content() {
        let config = parse_args_from(argument_list(&["--layers", "1", "--content", "canonical"]))
            .expect("valid");
        assert_eq!((config.layers, config.content.as_str()), (1, "canonical"));
    }

    #[test]
    fn fixture_and_layer_bounds_are_rejected() {
        for (arguments, message) in [
            (
                argument_list(&["--width", "0"]),
                "--width must be 1..=16384",
            ),
            (
                argument_list(&["--height", "16385"]),
                "--height must be 1..=16384",
            ),
            (argument_list(&["--layers", "0"]), "--layers must be 1..=4"),
            (argument_list(&["--layers", "5"]), "--layers must be 1..=4"),
            (
                argument_list(&["--content", "checkerboard"]),
                "--content: expected plain or canonical",
            ),
        ] {
            let error = parse_args_from(arguments).err().expect("rejected");
            assert!(
                error.starts_with(message),
                "{error} does not name {message}"
            );
        }
    }

    #[test]
    fn fixture_png_is_optional_and_captured() {
        let config =
            parse_args_from(argument_list(&["--fixture-png", "fixture.png"])).expect("valid");
        assert_eq!(config.fixture_png.as_deref(), Some("fixture.png"));
        assert_eq!(
            parse_args_from(argument_list(&["--fixture-png"]))
                .err()
                .as_deref(),
            Some("--fixture-png needs a value")
        );
    }
}
