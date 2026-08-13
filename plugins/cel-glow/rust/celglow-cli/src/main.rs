use std::{
    fs,
    path::{Path, PathBuf},
    str::FromStr,
    sync::mpsc::channel,
};

use celglow_core::{render, DebugView, FrameBuf, Params};
use clap::{Args, Parser, Subcommand, ValueEnum};
use image::{ImageBuffer, Rgba};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};

mod gpu_preview;

#[derive(Parser)]
#[command(
    name = "celglow-cli",
    version,
    about = "CelGlow v2 standalone render harness"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Render a PNG using the CPU core.
    Render(RenderArgs),
    /// Render a fast approximate preview on the GPU.
    GpuPreview(RenderArgs),
    /// Print the complete default parameter TOML to stdout.
    DumpParams,
    /// Render every debug view into a directory.
    Contact(ContactArgs),
    /// Generate a synthetic input and all debug-view PNGs for visual inspection.
    Demo(DemoArgs),
    /// Re-render whenever the parameter TOML is saved.
    Watch(RenderArgs),
}

#[derive(Args, Clone)]
struct RenderArgs {
    #[arg(short = 'i', long)]
    input: PathBuf,
    #[arg(short = 'o', long)]
    output: PathBuf,
    #[arg(short = 'p', long)]
    params: Option<PathBuf>,
    #[arg(long, value_enum, default_value_t = ViewArg::Result)]
    view: ViewArg,
    #[arg(long = "set", value_name = "GROUP.KEY=VALUE")]
    overrides: Vec<String>,
}

#[derive(Args)]
struct ContactArgs {
    #[arg(short = 'i', long)]
    input: PathBuf,
    #[arg(short = 'o', long)]
    output: PathBuf,
    #[arg(short = 'p', long)]
    params: Option<PathBuf>,
    #[arg(long = "set", value_name = "GROUP.KEY=VALUE")]
    overrides: Vec<String>,
}

#[derive(Args)]
struct DemoArgs {
    #[arg(short = 'o', long, default_value = "demo-output")]
    output: PathBuf,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ViewArg {
    Result,
    Glow,
    Field,
    Rings,
    Texture,
    Source,
}

impl From<ViewArg> for DebugView {
    fn from(value: ViewArg) -> Self {
        match value {
            ViewArg::Result => Self::Result,
            ViewArg::Glow => Self::GlowOnly,
            ViewArg::Field => Self::Field,
            ViewArg::Rings => Self::Rings,
            ViewArg::Texture => Self::TextureMask,
            ViewArg::Source => Self::Source,
        }
    }
}

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        Some(Command::Render(args)) => render_command(&args),
        Some(Command::GpuPreview(args)) => gpu_preview_command(&args),
        Some(Command::DumpParams) => dump_params(),
        Some(Command::Contact(args)) => contact_command(&args),
        Some(Command::Demo(args)) => demo_command(&args),
        Some(Command::Watch(args)) => watch_command(&args),
        None => {
            Cli::parse_from(["celglow-cli", "--help"]);
            Ok(())
        }
    };
    if let Err(error) = result {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn dump_params() -> Result<(), String> {
    print!(
        "{}",
        toml::to_string_pretty(&Params::default()).map_err(|e| e.to_string())?
    );
    Ok(())
}

fn load_params(path: Option<&Path>, overrides: &[String]) -> Result<Params, String> {
    let initial = match path {
        Some(path) => {
            let contents = fs::read_to_string(path)
                .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
            toml::from_str::<Params>(&contents)
                .map_err(|e| format!("invalid TOML in {}: {e}", path.display()))?
        }
        None => Params::default(),
    };
    // Params に一度戻して既定値を補完してから --set を適用する。これにより
    // params TOML に存在しないグループも --set で指定できる。
    let mut value = toml::Value::try_from(initial).map_err(|e| e.to_string())?;
    for set in overrides {
        apply_override(&mut value, set)?;
    }
    let params: Params = value
        .try_into()
        .map_err(|e| format!("invalid parameters: {e}"))?;
    validate(&params)?;
    Ok(params)
}

fn apply_override(root: &mut toml::Value, assignment: &str) -> Result<(), String> {
    let (key, raw) = assignment
        .split_once('=')
        .ok_or_else(|| format!("--set must be group.key=value: {assignment}"))?;
    let mut keys = key.split('.');
    let group = keys
        .next()
        .ok_or_else(|| format!("invalid --set key: {key}"))?;
    let field = keys
        .next()
        .ok_or_else(|| format!("--set must use group.key: {key}"))?;
    if keys.next().is_some() {
        return Err(format!("--set must use group.key: {key}"));
    }
    let table = root
        .as_table_mut()
        .ok_or_else(|| "internal parameter TOML is not a table".to_string())?;
    let group_value = table
        .get_mut(group)
        .ok_or_else(|| format!("unknown parameter group: {group}"))?;
    let group_table = group_value
        .as_table_mut()
        .ok_or_else(|| format!("parameter group is not a table: {group}"))?;
    let old = group_table
        .get(field)
        .ok_or_else(|| format!("unknown parameter: {key}"))?;
    let new_value = match old {
        toml::Value::String(_) => toml::Value::String(raw.to_string()),
        toml::Value::Boolean(_) => toml::Value::Boolean(
            raw.parse()
                .map_err(|_| format!("{key} expects true or false"))?,
        ),
        toml::Value::Integer(_) => toml::Value::Integer(
            raw.parse()
                .map_err(|_| format!("{key} expects an integer"))?,
        ),
        toml::Value::Float(_) => {
            toml::Value::Float(raw.parse().map_err(|_| format!("{key} expects a number"))?)
        }
        toml::Value::Array(_) => {
            let wrapped = format!("value = {raw}");
            let mut parsed = toml::Value::from_str(&wrapped)
                .map_err(|_| format!("{key} expects a TOML array, e.g. [1.0, 0.5, 0.0]"))?;
            parsed
                .as_table_mut()
                .and_then(|table| table.remove("value"))
                .ok_or_else(|| format!("{key} expects a TOML array, e.g. [1.0, 0.5, 0.0]"))?
        }
        _ => return Err(format!("unsupported --set parameter: {key}")),
    };
    group_table.insert(field.to_string(), new_value);
    Ok(())
}

fn in_range(name: &str, value: f32, min: f32, max: f32) -> Result<(), String> {
    if value.is_finite() && (min..=max).contains(&value) {
        Ok(())
    } else {
        Err(format!("{name} must be in {min}..={max}"))
    }
}

fn validate(p: &Params) -> Result<(), String> {
    in_range("source.gain", p.source.gain, 0.0, 400.0)?;
    in_range("source.gamma", p.source.gamma, 0.2, 3.0)?;
    in_range("source.spread", p.source.spread, 0.0, 500.0)?;
    in_range("source.field_gamma", p.source.field_gamma, 0.2, 3.0)?;
    in_range("source.threshold", p.source.threshold, 0.0, 100.0)?;
    in_range("source.threshold_softness", p.source.threshold_softness, 0.0, 100.0)?;
    if !(1..=64).contains(&p.rings.ring_count) {
        return Err("rings.ring_count must be in 1..=64".into());
    }
    in_range("rings.distribution", p.rings.distribution, 0.3, 3.0)?;
    in_range("rings.line_width", p.rings.line_width, 5.0, 95.0)?;
    in_range("rings.line_hardness", p.rings.line_hardness, 0.0, 100.0)?;
    in_range("rings.core_level", p.rings.core_level, 10.0, 100.0)?;
    in_range("rings.core_softness", p.rings.core_softness, 0.0, 50.0)?;
    in_range("rings.outer_falloff", p.rings.outer_falloff, 0.0, 4.0)?;
    in_range(
        "rings.brightness_variation",
        p.rings.brightness_variation,
        0.0,
        100.0,
    )?;
    in_range("rings.color_scramble", p.rings.color_scramble, 0.0, 100.0)?;
    in_range("wobble.amount", p.wobble.amount, 0.0, 100.0)?;
    in_range("wobble.scale", p.wobble.scale, 2.0, 200.0)?;
    if !(1..=4).contains(&p.wobble.complexity) {
        return Err("wobble.complexity must be in 1..=4".into());
    }
    in_range("texture.border", p.texture.border, 0.0, 50.0)?;
    in_range("texture.influence", p.texture.influence, 0.0, 50.0)?;
    in_range("texture.scale", p.texture.scale, 2.0, 200.0)?;
    in_range("texture.sharpness", p.texture.sharpness, 0.0, 20.0)?;
    if !(1..=4).contains(&p.texture.complexity) {
        return Err("texture.complexity must be in 1..=4".into());
    }
    in_range("unevenness.amount", p.unevenness.amount, 0.0, 100.0)?;
    in_range("unevenness.scale", p.unevenness.scale, 50.0, 20_000.0)?;
    in_range("output.rainbow_cycles", p.output.rainbow_cycles, 0.25, 8.0)?;
    in_range(
        "output.rainbow_saturation",
        p.output.rainbow_saturation,
        0.0,
        100.0,
    )?;
    in_range("output.opacity", p.output.opacity, 0.0, 200.0)?;
    for (name, color) in [
        ("output.fill_color", p.output.fill_color),
        ("output.inner_color", p.output.inner_color),
        ("output.outer_color", p.output.outer_color),
    ] {
        for channel in color {
            in_range(name, channel, 0.0, 1.0)?;
        }
    }
    Ok(())
}

fn render_command(args: &RenderArgs) -> Result<(), String> {
    let params = load_params(args.params.as_deref(), &args.overrides)?;
    render_png(&args.input, &args.output, &params, args.view.into())
}

fn gpu_preview_command(args: &RenderArgs) -> Result<(), String> {
    if !matches!(args.view, ViewArg::Result | ViewArg::Glow) {
        return Err("gpu-preview currently supports only --view result or glow".into());
    }
    let params = load_params(args.params.as_deref(), &args.overrides)?;
    let image = image::open(&args.input)
        .map_err(|e| format!("cannot decode {}: {e}", args.input.display()))?
        .to_rgba8();
    let (w, h) = image.dimensions();
    let input: Vec<f32> = image.as_raw().iter().map(|&v| v as f32 / 255.0).collect();
    let output = gpu_preview::render(
        FrameBuf {
            w: w as usize,
            h: h as usize,
            rgba: &input,
        },
        &params,
        matches!(args.view, ViewArg::Glow),
    )?;
    save_rgba_f32(&args.output, w, h, &output)
}

fn render_png(
    input_path: &Path,
    output_path: &Path,
    params: &Params,
    view: DebugView,
) -> Result<(), String> {
    let image = image::open(input_path)
        .map_err(|e| format!("cannot decode {}: {e}", input_path.display()))?
        .to_rgba8();
    let (w, h) = image.dimensions();
    let input: Vec<f32> = image.as_raw().iter().map(|&v| v as f32 / 255.0).collect();
    let mut output = vec![0.0; input.len()];
    render(
        FrameBuf {
            w: w as usize,
            h: h as usize,
            rgba: &input,
        },
        (0, 0),
        (0, 0),
        (w as usize, h as usize),
        params,
        view,
        &mut output,
    )
    .map_err(|e| e.to_string())?;
    save_rgba_f32(output_path, w, h, &output)
}

fn save_rgba_f32(output_path: &Path, w: u32, h: u32, output: &[f32]) -> Result<(), String> {
    let bytes: Vec<u8> = output
        .iter()
        .map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
        .collect();
    let output_image: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::from_raw(w, h, bytes)
        .ok_or_else(|| "cannot create output image buffer".to_string())?;
    output_image
        .save(output_path)
        .map_err(|e| format!("cannot write {}: {e}", output_path.display()))?;
    eprintln!("rendered {} ({w}x{h})", output_path.display());
    Ok(())
}

fn contact_command(args: &ContactArgs) -> Result<(), String> {
    fs::create_dir_all(&args.output)
        .map_err(|e| format!("cannot create {}: {e}", args.output.display()))?;
    let params = load_params(args.params.as_deref(), &args.overrides)?;
    for (file, view) in [
        ("result.png", DebugView::Result),
        ("glow.png", DebugView::GlowOnly),
        ("field.png", DebugView::Field),
        ("rings.png", DebugView::Rings),
        ("texture.png", DebugView::TextureMask),
        ("source.png", DebugView::Source),
    ] {
        render_png(&args.input, &args.output.join(file), &params, view)?;
    }
    Ok(())
}

fn demo_command(args: &DemoArgs) -> Result<(), String> {
    fs::create_dir_all(&args.output)
        .map_err(|e| format!("cannot create {}: {e}", args.output.display()))?;
    let input_path = args.output.join("input.png");
    generate_demo_input(&input_path)?;
    let mut params = Params::default();
    params.source.spread = 260.0;
    params.source.gain = 180.0;
    params.rings.ring_count = 12;
    // The demo intentionally uses the default flat, single-colour cel bands.
    for (file, view) in [
        ("result.png", DebugView::Result),
        ("glow.png", DebugView::GlowOnly),
        ("field.png", DebugView::Field),
        ("rings.png", DebugView::Rings),
        ("texture.png", DebugView::TextureMask),
        ("source.png", DebugView::Source),
    ] {
        render_png(&input_path, &args.output.join(file), &params, view)?;
    }
    eprintln!("demo images written to {}", args.output.display());
    Ok(())
}

fn generate_demo_input(path: &Path) -> Result<(), String> {
    // Give the 260 px glow enough transparent checkout around the artwork so
    // the outermost ring is a real ring, not a canvas-edge crop.
    const PAD: i32 = 320;
    let mut image = ImageBuffer::<Rgba<u8>, Vec<u8>>::from_pixel(1152, 1152, Rgba([0, 0, 0, 0]));
    for x in 38..474 {
        let y = 255.0 + (x as f32 * 0.034).sin() * 92.0;
        let t = (x - 38) as f32 / 436.0;
        draw_disc(
            &mut image,
            x as i32 + PAD,
            y as i32 + PAD,
            13,
            [
                (255.0 * (1.0 - t)) as u8,
                (90.0 + 140.0 * t) as u8,
                255,
                220,
            ],
        );
    }
    for y in 84..430 {
        let x = 265.0 + (y as f32 * 0.045 + 1.2).cos() * 70.0;
        let t = (y - 84) as f32 / 346.0;
        draw_disc(
            &mut image,
            x as i32 + PAD,
            y as i32 + PAD,
            10,
            [
                255,
                (210.0 * (1.0 - t)) as u8,
                (65.0 + 160.0 * t) as u8,
                210,
            ],
        );
    }
    image
        .save(path)
        .map_err(|e| format!("cannot write {}: {e}", path.display()))
}

fn draw_disc(
    image: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
    cx: i32,
    cy: i32,
    radius: i32,
    color: [u8; 4],
) {
    for y in (cy - radius)..=(cy + radius) {
        for x in (cx - radius)..=(cx + radius) {
            if x < 0
                || y < 0
                || x >= image.width() as i32
                || y >= image.height() as i32
                || (x - cx) * (x - cx) + (y - cy) * (y - cy) > radius * radius
            {
                continue;
            }
            let target = image.get_pixel_mut(x as u32, y as u32);
            if color[3] >= target[3] {
                *target = Rgba(color);
            }
        }
    }
}

fn watch_command(args: &RenderArgs) -> Result<(), String> {
    let path = args
        .params
        .as_ref()
        .ok_or_else(|| "watch requires --params <FILE>".to_string())?
        .clone();
    render_command(args)?;
    let (tx, rx) = channel();
    let mut watcher: RecommendedWatcher =
        notify::recommended_watcher(tx).map_err(|e| e.to_string())?;
    watcher
        .watch(&path, RecursiveMode::NonRecursive)
        .map_err(|e| format!("cannot watch {}: {e}", path.display()))?;
    eprintln!("watching {} (Ctrl+C to stop)", path.display());
    loop {
        match rx.recv().map_err(|e| e.to_string())? {
            Ok(_) => {
                if let Err(error) = render_command(args) {
                    eprintln!("error: {error}");
                }
            }
            Err(error) => eprintln!("watch error: {error}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_render_round_trips_rgba8() {
        let root = std::env::temp_dir().join(format!("celglow-cli-test-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let input_path = root.join("input.png");
        let output_path = root.join("output.png");
        let pixels = vec![255, 0, 0, 255, 0, 128, 255, 64];
        ImageBuffer::<Rgba<u8>, _>::from_raw(2, 1, pixels.clone())
            .unwrap()
            .save(&input_path)
            .unwrap();
        render_png(
            &input_path,
            &output_path,
            &Params::default(),
            DebugView::Result,
        )
        .unwrap();
        assert_eq!(
            image::open(&output_path).unwrap().to_rgba8().into_raw(),
            pixels
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn override_uses_defaults_for_missing_groups() {
        let root =
            std::env::temp_dir().join(format!("celglow-cli-params-test-{}", std::process::id()));
        fs::write(&root, "[source]\nspread = 40.0\n").unwrap();
        let params = load_params(Some(&root), &["rings.ring_count=32".to_string()]).unwrap();
        assert_eq!(params.source.spread, 40.0);
        assert_eq!(params.rings.ring_count, 32);
        fs::remove_file(root).unwrap();
    }

    #[test]
    fn rings_view_produces_visible_pixels() {
        let root =
            std::env::temp_dir().join(format!("celglow-cli-rings-test-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let input_path = root.join("input.png");
        let output_path = root.join("rings.png");
        let mut pixels = vec![0_u8; 128 * 128 * 4];
        for y in 56..72 {
            for x in 56..72 {
                let i = (y * 128 + x) * 4;
                pixels[i..i + 4].copy_from_slice(&[255, 255, 255, 255]);
            }
        }
        ImageBuffer::<Rgba<u8>, _>::from_raw(128, 128, pixels)
            .unwrap()
            .save(&input_path)
            .unwrap();
        let mut params = Params::default();
        params.source.spread = 36.0;
        params.rings.ring_count = 12;
        render_png(&input_path, &output_path, &params, DebugView::Rings).unwrap();
        let result = image::open(&output_path).unwrap().to_rgba8().into_raw();
        assert!(result.chunks_exact(4).any(|pixel| pixel[0] > 16));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn demo_input_contains_visible_strokes() {
        let path =
            std::env::temp_dir().join(format!("celglow-demo-test-{}.png", std::process::id()));
        generate_demo_input(&path).unwrap();
        assert!(image::open(&path)
            .unwrap()
            .to_rgba8()
            .pixels()
            .any(|pixel| pixel[3] > 0));
        fs::remove_file(path).unwrap();
    }
}
