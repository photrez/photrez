use super::{Config, PATCH_COMMIT_MOVES, USAGE};
use photrez::render::canvas_bridge::{MAX_CANVAS_DIM, TILE_SIZE};

pub(super) fn parse_args() -> Result<Config, String> {
    parse_args_from(std::env::args().skip(1))
}

fn parse_args_from(arguments: impl IntoIterator<Item = String>) -> Result<Config, String> {
    let mut size = 4096usize;
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
    let mut out_path = None;

    let mut args = arguments.into_iter();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--size" => size = parse_usize(&mut args, "--size")?,
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

    #[test]
    fn input_flag_is_rejected() {
        let arguments = ["--input".to_string()];
        assert_eq!(
            parse_args_from(arguments).err().as_deref(),
            Some("unknown argument: --input")
        );
    }
}
