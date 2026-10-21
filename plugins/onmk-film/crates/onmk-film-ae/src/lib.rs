use after_effects as ae;
use onmk_film_core::{FilmParams, FilmStock, Gauge, InputColorspace, LookPreset};

use std::sync::atomic::{AtomicUsize,Ordering};
const IDLE_FRAME_BUDGET:usize=256*1024*1024;
static IDLE_FRAME_BYTES:AtomicUsize=AtomicUsize::new(0);

struct FrameCache<'a> {
    frames:(Vec<f32>,Vec<f32>),
    bytes:usize,
    used:&'a AtomicUsize,
    limit:usize,
}
impl<'a> FrameCache<'a> {
    const fn new(used:&'a AtomicUsize,limit:usize)->Self {
        Self{frames:(Vec::new(),Vec::new()),bytes:0,used,limit}
    }
    fn take(&mut self)->(Vec<f32>,Vec<f32>) {
        self.used.fetch_sub(self.bytes,Ordering::Relaxed);
        self.bytes=0;
        std::mem::take(&mut self.frames)
    }
    fn store(&mut self,frames:(Vec<f32>,Vec<f32>)) {
        drop(self.take());
        let Some(bytes)=frames.0.capacity().checked_add(frames.1.capacity()).and_then(|n|n.checked_mul(std::mem::size_of::<f32>())) else{return;};
        if bytes>self.limit {return;}
        if self.used.fetch_update(Ordering::Relaxed,Ordering::Relaxed,|used| {
            used.checked_add(bytes).filter(|&total|total<=self.limit)
        }).is_ok() {self.frames=frames;self.bytes=bytes;}
    }
}
impl Drop for FrameCache<'_> {
    fn drop(&mut self) {self.used.fetch_sub(self.bytes,Ordering::Relaxed);}
}
thread_local! {
    static FRAME_BUFFERS: std::cell::RefCell<FrameCache<'static>> =
        std::cell::RefCell::new(FrameCache::new(&IDLE_FRAME_BYTES,IDLE_FRAME_BUDGET));
}

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    SourceStart,
    InputCs,
    Bypass,
    UseGpu,
    Quality,
    SourceEnd,

    FilmStart,
    Stock,
    Exposure,
    Character,
    Contrast,
    Shoulder,
    Saturation,
    Signature,
    SkinLift,
    SkinSat,
    SkinHue,
    FilmEnd,

    HalStart,
    HalEnable,
    HalStrength,
    HalRadius,
    HalThreshold,
    HalColor,
    HalFlicker,
    HalEnd,

    BloomStart,
    BloomEnable,
    BloomStrength,
    BloomRadius,
    BloomThreshold,
    Diffusion,
    BloomEnd,

    BleedStart,
    BleedEnable,
    Irradiation,
    IrradiationRadius,
    DyeCloud,
    DyeRadius,
    Interlayer,
    BleedEnd,

    GrainStart,
    GrainEnable,
    Gauge,
    GrainAmount,
    GrainClump,
    GrainSize,
    GrainMotion,
    GrainShadows,
    GrainMids,
    GrainHighlights,
    PrintGrain,
    PrintGrainAmount,
    PrintGrainSize,
    GrainEnd,

    SharpStart,
    MtfEnable,
    MtfSoftness,
    Acutance,
    SharpEnd,

    OpticsStart,
    CaEnable,
    CaAmount,
    CaFalloff,
    VignetteEnable,
    VignetteAmount,
    VignetteRadius,
    VignetteSoftness,
    LeakAmount,
    LeakAngle,
    LeakSoftness,
    LeakFlicker,
    LeakColor,
    OpticsEnd,

    MotionStart,
    WeaveEnable,
    WeaveAmount,
    MotionEnd,

    AgeStart,
    CrossoverEnable,
    Crossover,
    CrossoverAxis,
    MottleEnable,
    MottleAmount,
    MottleSize,
    MottleStatic,
    AgeEnd,

    LookStart,
    Look,
    LookEnd,
}

#[derive(Default)]
struct Plugin;

#[derive(Clone, Copy, Default)]
struct PreRenderState {
    time: i32,
}

ae::define_effect!(Plugin, (), Params);

fn slider(min: f32, max: f32, default: f32) -> ae::FloatSliderDef<'static> {
    ae::FloatSliderDef::setup(|f| {
        f.set_valid_min(min);
        f.set_valid_max(max);
        f.set_slider_min(min);
        f.set_slider_max(max);
        f.set_default(default as f64);
        f.set_precision(2);
    })
}

fn check(default: bool, label: &'static str) -> ae::CheckBoxDef<'static> {
    ae::CheckBoxDef::setup(|f| {
        f.set_default(default);
        f.set_label(label);
    })
}

fn color_def(r: u8, g: u8, b: u8) -> ae::ColorDef<'static> {
    ae::ColorDef::setup(move |f| {
        f.set_default(ae::Pixel8 {
            alpha: 255,
            red: r,
            green: g,
            blue: b,
        });
    })
}

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        p: &mut ae::Parameters<Params>,
        _: ae::InData,
        _: ae::OutData,
    ) -> Result<(), ae::Error> {
        p.add_group(
            Params::SourceStart,
            Params::SourceEnd,
            "Source",
            false,
            |p| {
                p.add(
                    Params::InputCs,
                    "Input CS",
                    ae::PopupDef::setup(|f| {
                        f.set_options(&[
                            "Rec.709", "sRGB", "Linear", "Bypass", "S-Log3", "LogC3", "V-Log",
                            "C-Log3",
                        ]);
                        f.set_default(1);
                    }),
                )?;
                p.add(Params::Bypass, "Bypass", check(false, "Bypass"))?;
                p.add(Params::UseGpu, "Use GPU", check(true, "Metal"))?;
                p.add(Params::Quality, "Quality", slider(0.25, 1.0, 1.0))
            },
        )?;

        p.add_group(Params::FilmStart, Params::FilmEnd, "Film", false, |p| {
            p.add(
                Params::Stock,
                "Stock",
                ae::PopupDef::setup(|f| {
                    f.set_options(&[
                        "Solstice 50",
                        "Harbor 250",
                        "Lantern 200",
                        "Nocturne 500",
                        "Ash 250",
                        "Velvet 400",
                        "Prism 100",
                        "Amber 200",
                        "Mist 400",
                        "Neon 800",
                        "Chrome 50",
                        "Ash 400",
                        "Print",
                    ]);
                    f.set_default(2);
                }),
            )?;
            p.add(Params::Exposure, "Exposure", slider(-4.0, 4.0, 0.0))?;
            p.add(Params::Character, "Character", slider(0.0, 2.0, 1.0))?;
            p.add(Params::Contrast, "Print Contrast", slider(0.5, 2.0, 1.0))?;
            p.add(Params::Shoulder, "Shoulder", slider(0.0, 1.0, 0.0))?;
            p.add(Params::Saturation, "Saturation", slider(0.0, 2.0, 1.0))?;
            p.add(Params::Signature, "Signature", slider(0.0, 1.0, 0.0))?;
            p.add(Params::SkinLift, "Skin Lift", slider(0.5, 1.5, 1.0))?;
            p.add(Params::SkinSat, "Skin Sat", slider(0.0, 2.0, 1.0))?;
            p.add(Params::SkinHue, "Skin Hue", slider(-30.0, 30.0, 0.0))
        })?;

        p.add_group(Params::HalStart, Params::HalEnd, "Halation", true, |p| {
            p.add(Params::HalEnable, "Enable", check(true, "Enable"))?;
            p.add(Params::HalStrength, "Strength", slider(0.0, 2.0, 0.5))?;
            p.add(Params::HalRadius, "Radius", slider(0.002, 0.06, 0.01))?;
            p.add(Params::HalThreshold, "Threshold", slider(0.1, 2.0, 1.0))?;
            p.add(Params::HalColor, "Color", color_def(255, 64, 20))?;
            p.add(Params::HalFlicker, "Flicker", slider(0.0, 1.0, 0.0))
        })?;

        p.add_group(Params::BloomStart, Params::BloomEnd, "Bloom", true, |p| {
            p.add(Params::BloomEnable, "Enable", check(false, "Enable"))?;
            p.add(Params::BloomStrength, "Strength", slider(0.0, 1.0, 0.3))?;
            p.add(Params::BloomRadius, "Radius", slider(0.002, 0.15, 0.03))?;
            p.add(Params::BloomThreshold, "Threshold", slider(0.0, 2.0, 0.6))?;
            p.add(Params::Diffusion, "Diffusion", slider(0.0, 1.0, 0.0))
        })?;

        p.add_group(Params::BleedStart, Params::BleedEnd, "Bleed", true, |p| {
            p.add(Params::BleedEnable, "Enable", check(true, "Enable"))?;
            p.add(Params::Irradiation, "Irradiation", slider(0.0, 1.0, 0.35))?;
            p.add(
                Params::IrradiationRadius,
                "Irradiation Radius",
                slider(0.0005, 0.02, 0.004),
            )?;
            p.add(Params::DyeCloud, "Dye Cloud", slider(0.0, 1.0, 0.35))?;
            p.add(Params::DyeRadius, "Dye Radius", slider(0.0005, 0.02, 0.003))?;
            p.add(Params::Interlayer, "Interlayer", slider(0.0, 1.0, 0.4))
        })?;

        p.add_group(Params::GrainStart, Params::GrainEnd, "Grain", true, |p| {
            p.add(Params::GrainEnable, "Enable", check(true, "Enable"))?;
            p.add(
                Params::Gauge,
                "Gauge",
                ae::PopupDef::setup(|f| {
                    f.set_options(&["65 mm", "35 mm", "16 mm", "8 mm"]);
                    f.set_default(2);
                }),
            )?;
            p.add(Params::GrainAmount, "Amount", slider(0.0, 2.0, 0.5))?;
            p.add(Params::GrainClump, "Clump", slider(0.0, 1.0, 0.5))?;
            p.add(Params::GrainSize, "Size", slider(0.25, 4.0, 1.0))?;
            p.add(
                Params::GrainMotion,
                "Motion",
                ae::PopupDef::setup(|f| {
                    f.set_options(&["Per Frame", "Static"]);
                    f.set_default(1);
                }),
            )?;
            p.add(Params::GrainShadows, "Shadows", slider(0.0, 2.0, 1.0))?;
            p.add(Params::GrainMids, "Midtones", slider(0.0, 2.0, 1.0))?;
            p.add(Params::GrainHighlights, "Highlights", slider(0.0, 2.0, 1.0))?;
            p.add(Params::PrintGrain, "Print Grain", check(false, "Enable"))?;
            p.add(
                Params::PrintGrainAmount,
                "Print Amount",
                slider(0.0, 2.0, 0.35),
            )?;
            p.add(Params::PrintGrainSize, "Print Size", slider(0.25, 4.0, 1.0))
        })?;

        p.add_group(Params::SharpStart, Params::SharpEnd, "Sharpness", true, |p| {
            p.add(Params::MtfEnable, "Enable", check(true, "Enable"))?;
            p.add(Params::MtfSoftness, "MTF Softness", slider(0.0, 1.0, 0.35))?;
            p.add(Params::Acutance, "Acutance", slider(0.0, 1.5, 0.5))
        })?;

        p.add_group(Params::OpticsStart, Params::OpticsEnd, "Optics", true, |p| {
            p.add(Params::CaEnable, "CA Enable", check(false, "Enable"))?;
            p.add(Params::CaAmount, "CA Amount", slider(0.0, 1.0, 0.3))?;
            p.add(Params::CaFalloff, "CA Falloff", slider(0.0, 3.0, 1.0))?;
            p.add(
                Params::VignetteEnable,
                "Vignette Enable",
                check(false, "Enable"),
            )?;
            p.add(Params::VignetteAmount, "Vignette", slider(0.0, 1.0, 0.0))?;
            p.add(Params::VignetteRadius, "Vig Radius", slider(0.0, 1.0, 0.55))?;
            p.add(Params::VignetteSoftness, "Vig Soft", slider(0.0, 1.0, 0.6))?;
            p.add(Params::LeakAmount, "Light Leak", slider(0.0, 1.0, 0.0))?;
            p.add(Params::LeakAngle, "Leak Angle", slider(0.0, 360.0, 0.0))?;
            p.add(Params::LeakSoftness, "Leak Soft", slider(0.0, 1.0, 0.55))?;
            p.add(Params::LeakFlicker, "Leak Flicker", slider(0.0, 1.0, 0.0))?;
            p.add(Params::LeakColor, "Leak Color", color_def(255, 107, 41))
        })?;

        p.add_group(Params::MotionStart, Params::MotionEnd, "Motion", true, |p| {
            p.add(Params::WeaveEnable, "Gate Weave", check(false, "Enable"))?;
            p.add(Params::WeaveAmount, "Weave Amount", slider(0.0, 1.0, 0.5))
        })?;

        p.add_group(Params::AgeStart, Params::AgeEnd, "Age", true, |p| {
            p.add(
                Params::CrossoverEnable,
                "Crossover Enable",
                check(false, "Enable"),
            )?;
            p.add(Params::Crossover, "Crossover", slider(-1.0, 1.0, 0.0))?;
            p.add(Params::CrossoverAxis, "Axis", slider(0.0, 360.0, 0.0))?;
            p.add(Params::MottleEnable, "Mottle Enable", check(false, "Enable"))?;
            p.add(Params::MottleAmount, "Mottle", slider(0.0, 2.0, 0.0))?;
            p.add(Params::MottleSize, "Mottle Size", slider(0.25, 4.0, 1.0))?;
            p.add(Params::MottleStatic, "Mottle Static", check(false, "Static"))
        })?;

        p.add_group(Params::LookStart, Params::LookEnd, "Look", false, |p| {
            p.add(
                Params::Look,
                "Look",
                ae::PopupDef::setup(|f| {
                    f.set_options(&[
                        "Neutral",
                        "Classic",
                        "Teal & Amber",
                        "Print Master",
                        "Neon Night",
                        "Faded Pastel",
                        "Custom",
                    ]);
                    f.set_default(1);
                }),
            )
        })?;
        Ok(())
    }

    fn handle_command(
        &mut self,
        cmd: ae::Command,
        input: ae::InData,
        mut output: ae::OutData,
        params: &mut ae::Parameters<Params>,
    ) -> Result<(), ae::Error> {
        match cmd {
            ae::Command::About => {
                output.set_return_msg(
                    "onmk Film v0.5\rFilm response, emulsion scatter, density grain, optics.\rRust + Metal.",
                )
            }
            ae::Command::Render {
                in_layer,
                mut out_layer,
            } => render_world(
                &in_layer,
                &mut out_layer,
                &read(params, input.current_time())?,
            )?,
            ae::Command::SmartPreRender { mut extra } => {
                let min_dim = input.width().min(input.height()).max(1) as f32;
                let mut margin_r = 0.0f32;
                if params.get(Params::HalEnable)?.as_checkbox()?.value() {
                    margin_r = margin_r.max(val(params, Params::HalRadius)?);
                }
                if params.get(Params::BloomEnable)?.as_checkbox()?.value() {
                    margin_r = margin_r.max(val(params, Params::BloomRadius)?);
                }
                if params.get(Params::BleedEnable)?.as_checkbox()?.value() {
                    margin_r = margin_r
                        .max(val(params, Params::IrradiationRadius)?)
                        .max(val(params, Params::DyeRadius)?);
                }
                let margin = (margin_r * min_dim).ceil() as i32;
                pre_render(&input, &mut extra, margin)?
            }
            ae::Command::SmartRender { extra } => smart_render(&extra, params)?,
            ae::Command::SmartRenderGpu { .. } => return Err(ae::Error::BadCallbackParameter),
            _ => {}
        }
        Ok(())
    }
}

fn val(p: &ae::Parameters<Params>, id: Params) -> Result<f32, ae::Error> {
    Ok(p.get(id)?.as_float_slider()?.value() as f32)
}

fn read_color(p: &ae::Parameters<Params>, id: Params) -> Result<[f32; 3], ae::Error> {
    let c = p.get(id)?.as_color()?.value();
    Ok([
        c.red as f32 / 255.0,
        c.green as f32 / 255.0,
        c.blue as f32 / 255.0,
    ])
}

fn read(p: &ae::Parameters<Params>, time: i32) -> Result<FilmParams, ae::Error> {
    let mut x = FilmParams::default();
    x.bypass = p.get(Params::Bypass)?.as_checkbox()?.value();
    x.use_gpu = p.get(Params::UseGpu)?.as_checkbox()?.value();
    x.quality = val(p, Params::Quality)?;
    x.input_cs = InputColorspace::from_popup(p.get(Params::InputCs)?.as_popup()?.value());
    x.stock = FilmStock::from_popup(p.get(Params::Stock)?.as_popup()?.value());
    x.exposure = val(p, Params::Exposure)?;
    x.character = val(p, Params::Character)?;
    x.print_contrast = val(p, Params::Contrast)?;
    x.shoulder = val(p, Params::Shoulder)?;
    x.saturation = val(p, Params::Saturation)?;
    x.signature = val(p, Params::Signature)?;
    x.skin_lift = val(p, Params::SkinLift)?;
    x.skin_sat = val(p, Params::SkinSat)?;
    x.skin_hue = val(p, Params::SkinHue)?;

    x.halation_enable = p.get(Params::HalEnable)?.as_checkbox()?.value();
    x.halation_strength = val(p, Params::HalStrength)?;
    x.halation_radius = val(p, Params::HalRadius)?;
    x.halation_threshold = val(p, Params::HalThreshold)?;
    x.halation_color = read_color(p, Params::HalColor)?;
    x.halation_flicker = val(p, Params::HalFlicker)?;

    x.bloom_enable = p.get(Params::BloomEnable)?.as_checkbox()?.value();
    x.bloom_strength = val(p, Params::BloomStrength)?;
    x.bloom_radius = val(p, Params::BloomRadius)?;
    x.bloom_threshold = val(p, Params::BloomThreshold)?;
    x.diffusion = val(p, Params::Diffusion)?;

    x.bleed_enable = p.get(Params::BleedEnable)?.as_checkbox()?.value();
    x.irradiation = val(p, Params::Irradiation)?;
    x.irradiation_radius = val(p, Params::IrradiationRadius)?;
    x.dye_cloud = val(p, Params::DyeCloud)?;
    x.dye_radius = val(p, Params::DyeRadius)?;
    x.interlayer = val(p, Params::Interlayer)?;

    x.grain_enable = p.get(Params::GrainEnable)?.as_checkbox()?.value();
    x.gauge = Gauge::from_popup(p.get(Params::Gauge)?.as_popup()?.value());
    x.grain_amount = val(p, Params::GrainAmount)?;
    x.grain_clump = val(p, Params::GrainClump)?;
    x.grain_size = val(p, Params::GrainSize)?;
    x.grain_shadows = val(p, Params::GrainShadows)?;
    x.grain_mids = val(p, Params::GrainMids)?;
    x.grain_highlights = val(p, Params::GrainHighlights)?;
    x.print_grain = p.get(Params::PrintGrain)?.as_checkbox()?.value();
    x.print_grain_amount = val(p, Params::PrintGrainAmount)?;
    x.print_grain_size = val(p, Params::PrintGrainSize)?;

    x.mtf_enable = p.get(Params::MtfEnable)?.as_checkbox()?.value();
    x.mtf_softness = val(p, Params::MtfSoftness)?;
    x.acutance = val(p, Params::Acutance)?;

    x.ca_enable = p.get(Params::CaEnable)?.as_checkbox()?.value();
    x.ca_amount = val(p, Params::CaAmount)?;
    x.ca_falloff = val(p, Params::CaFalloff)?;
    x.vignette_enable = p.get(Params::VignetteEnable)?.as_checkbox()?.value();
    x.vignette_amount = val(p, Params::VignetteAmount)?;
    x.vignette_radius = val(p, Params::VignetteRadius)?;
    x.vignette_softness = val(p, Params::VignetteSoftness)?;
    x.leak_amount = val(p, Params::LeakAmount)?;
    x.leak_angle = val(p, Params::LeakAngle)?;
    x.leak_softness = val(p, Params::LeakSoftness)?;
    x.leak_flicker = val(p, Params::LeakFlicker)?;
    x.leak_color = read_color(p, Params::LeakColor)?;

    x.weave_enable = p.get(Params::WeaveEnable)?.as_checkbox()?.value();
    x.weave_amount = val(p, Params::WeaveAmount)?;

    x.crossover_enable = p.get(Params::CrossoverEnable)?.as_checkbox()?.value();
    x.crossover = val(p, Params::Crossover)?;
    x.crossover_axis = val(p, Params::CrossoverAxis)?;
    x.mottle_enable = p.get(Params::MottleEnable)?.as_checkbox()?.value();
    x.mottle_amount = val(p, Params::MottleAmount)?;
    x.mottle_size = val(p, Params::MottleSize)?;
    x.mottle_static = p.get(Params::MottleStatic)?.as_checkbox()?.value();

    x.look = LookPreset::from_popup(p.get(Params::Look)?.as_popup()?.value());
    // Apply look only for named presets; Custom keeps panel values.
    if !matches!(x.look, LookPreset::Custom | LookPreset::Neutral) {
        x.apply_look();
    }

    x.frame = if p.get(Params::GrainMotion)?.as_popup()?.value() == 2 {
        0
    } else {
        time.max(0) as u32
    };
    Ok(x)
}

fn depth_from_format(format: ae::PixelFormat) -> Result<i16, ae::Error> {
    match format {
        ae::PixelFormat::Argb32 => Ok(8),
        ae::PixelFormat::Argb64 => Ok(16),
        ae::PixelFormat::Argb128 => Ok(32),
        _ => Err(ae::Error::BadCallbackParameter),
    }
}

#[test]
fn cpu_world_formats_are_explicit() {
    assert_eq!(depth_from_format(ae::PixelFormat::Argb32).unwrap(), 8);
    assert_eq!(depth_from_format(ae::PixelFormat::Argb64).unwrap(), 16);
    assert_eq!(depth_from_format(ae::PixelFormat::Argb128).unwrap(), 32);
    assert!(depth_from_format(ae::PixelFormat::GpuBgra128).is_err());
}

fn checked_world_geometry(w:usize,h:usize,row_bytes:isize,channel:usize,address:usize)->Result<(),ae::Error> {
    if w>i32::MAX as usize || h>i32::MAX as usize || row_bytes==i32::MIN as isize {return Err(ae::Error::BadCallbackParameter);}
    if w==0 || h==0 {return Ok(());}
    let stride=row_bytes.unsigned_abs();
    let row=w.checked_mul(4).and_then(|v|v.checked_mul(channel)).ok_or(ae::Error::OutOfMemory)?;
    let end=(h-1).checked_mul(stride).and_then(|v|v.checked_add(row)).ok_or(ae::Error::OutOfMemory)?;
    if stride<row || end>isize::MAX as usize || address==0 || address%channel!=0 || stride%channel!=0 {
        return Err(ae::Error::BadCallbackParameter);
    }
    Ok(())
}

fn resize_frame(frame:&mut Vec<f32>,w:usize,h:usize)->Result<(),ae::Error> {
    let n=w.checked_mul(h).and_then(|v|v.checked_mul(4)).ok_or(ae::Error::OutOfMemory)?;
    frame.try_reserve_exact(n.saturating_sub(frame.len())).map_err(|_|ae::Error::OutOfMemory)?;
    frame.resize(n,0.0);
    Ok(())
}

fn render_world(
    input: &ae::Layer,
    output: &mut ae::Layer,
    p: &FilmParams,
) -> Result<(), ae::Error> {
    let input_depth = depth_from_format(input.pixel_format()?)?;
    let output_depth = depth_from_format(output.pixel_format()?)?;
    for (layer,depth) in [(input,input_depth),(&*output,output_depth)] {
        checked_world_geometry(layer.width(),layer.height(),layer.row_bytes(),(depth/8) as usize,unsafe {layer.data_ptr()} as usize)?;
    }
    let w = input.width().min(output.width());
    let h = input.height().min(output.height());
    if w==0 || h==0 {return Ok(());}
    let (mut src, mut dst) = FRAME_BUFFERS.with(|buffers| {
        let mut buffers = buffers.borrow_mut();
        buffers.take()
    });
    resize_frame(&mut src,w,h)?;
    resize_frame(&mut dst,w,h)?;
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) * 4;
            match input_depth {
                8 => {
                    let q = input.as_pixel8(x, y);
                    src[i] = q.red as f32 / 255.;
                    src[i + 1] = q.green as f32 / 255.;
                    src[i + 2] = q.blue as f32 / 255.;
                    src[i + 3] = q.alpha as f32 / 255.;
                }
                16 => {
                    let q = input.as_pixel16(x, y);
                    let m = ae::MAX_CHANNEL16 as f32;
                    src[i] = q.red as f32 / m;
                    src[i + 1] = q.green as f32 / m;
                    src[i + 2] = q.blue as f32 / m;
                    src[i + 3] = q.alpha as f32 / m;
                }
                32 => {
                    let q = input.as_pixel32(x, y);
                    src[i] = q.red;
                    src[i + 1] = q.green;
                    src[i + 2] = q.blue;
                    src[i + 3] = q.alpha;
                }
                _ => return Err(ae::Error::BadCallbackParameter),
            }
        }
    }

    onmk_film_core::render_with_backends(
        &src,
        &mut dst,
        w,
        h,
        p,
        |source, target, bw, bh, radius, quality| {
            let gpu_radius = (radius * quality.clamp(0.25, 1.0) * 0.5).round().max(1.0) as u32;
            if p.use_gpu && bw >= 64 && bh >= 64 {
                if let Ok(result) =
                    onmk_film_gpu::blur_rgba(source, bw as u32, bh as u32, gpu_radius)
                {
                    target.copy_from_slice(&result);
                    return;
                }
            }
            onmk_film_core::blur_cpu(source, target, bw, bh, radius, quality);
        },
        |buffer, gw, gh, grain| {
            if !grain.grain_enable || grain.grain_amount <= 1.0e-5 {
                return;
            }
            let (grain_base, mono) = match grain.stock {
                FilmStock::DaylightFine | FilmStock::Chrome50 => (0.5, false),
                FilmStock::Daylight250 => (0.85, false),
                FilmStock::Tungsten200 => (0.9, false),
                FilmStock::Tungsten500 | FilmStock::Neon800 => (1.4, false),
                FilmStock::Portrait400 | FilmStock::Pastel400 => (1.05, false),
                FilmStock::Vivid100 => (0.5, false),
                FilmStock::Amber200 => (0.9, false),
                FilmStock::Mono250 => (0.95, true),
                FilmStock::Mono400 => (1.2, true),
                FilmStock::PrintStock => (0.35, false),
            };
            let seed = grain
                .seed
                .wrapping_add(grain.frame.wrapping_mul(0x9e37_79b9))
                .wrapping_add(0x00a1_1ce5);
            let gpu = onmk_film_gpu::GrainParams {
                seed,
                mono: mono as u32,
                cell: (3.5 * grain.grain_size.max(0.25) / grain.gauge.scale()).max(0.75),
                amount: grain.grain_amount,
                clump: grain.grain_clump.clamp(0.0, 1.0),
                shadows: grain.grain_shadows,
                mids: grain.grain_mids,
                highlights: grain.grain_highlights,
                grain_base,
            };
            if !grain.use_gpu
                || grain.print_grain
                || onmk_film_gpu::grain_rgba(buffer, gw as u32, gh as u32, gpu).is_err()
            {
                onmk_film_core::grain_cpu(buffer, gw, gh, grain);
            }
        },
    );

    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) * 4;
            match output_depth {
                8 => {
                    let q = output.as_pixel8_mut(x, y);
                    q.red = (dst[i] * 255.).clamp(0., 255.) as u8;
                    q.green = (dst[i + 1] * 255.).clamp(0., 255.) as u8;
                    q.blue = (dst[i + 2] * 255.).clamp(0., 255.) as u8;
                    q.alpha = (dst[i + 3] * 255.).clamp(0., 255.) as u8;
                }
                16 => {
                    let m = ae::MAX_CHANNEL16 as f32;
                    let q = output.as_pixel16_mut(x, y);
                    q.red = (dst[i] * m).clamp(0., m) as u16;
                    q.green = (dst[i + 1] * m).clamp(0., m) as u16;
                    q.blue = (dst[i + 2] * m).clamp(0., m) as u16;
                    q.alpha = (dst[i + 3] * m).clamp(0., m) as u16;
                }
                32 => {
                    let q = output.as_pixel32_mut(x, y);
                    q.red = dst[i];
                    q.green = dst[i + 1];
                    q.blue = dst[i + 2];
                    q.alpha = dst[i + 3];
                }
                _ => return Err(ae::Error::BadCallbackParameter),
            }
        }
    }
    FRAME_BUFFERS.with(|buffers| {
        let mut buffers = buffers.borrow_mut();
        buffers.store((src,dst));
    });
    Ok(())
}

fn pre_render(
    input: &ae::InData,
    extra: &mut ae::pf::PreRenderExtra,
    margin: i32,
) -> Result<(), ae::Error> {
    let mut req = extra.output_request();
    let rect: ae::Rect = req.rect.into();
    req.rect = ae::Rect {
        left: rect.left.saturating_sub(margin),
        top: rect.top.saturating_sub(margin),
        right: rect.right.saturating_add(margin),
        bottom: rect.bottom.saturating_add(margin),
    }
    .into();
    let cb = extra.callbacks();
    let r = cb.checkout_layer(
        0,
        0,
        &req,
        input.current_time(),
        input.time_step(),
        input.time_scale(),
    )?;
    extra.set_result_rect(r.result_rect.into());
    extra.set_max_result_rect(r.max_result_rect.into());
    extra.set_returns_extra_pixels(margin > 0);
    extra.set_pre_render_data(PreRenderState {
        time: input.current_time(),
    });
    Ok(())
}

fn with_pixel_checkin<T>(work:impl FnOnce()->Result<T,ae::Error>,checkin:impl FnOnce()->Result<(),ae::Error>)->Result<T,ae::Error> {
    let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(work))
        .unwrap_or(Err(ae::Error::InternalStructDamaged));
    let released=checkin();
    match result {Err(error)=>Err(error),Ok(value)=>released.map(|()|value)}
}

#[cfg(test)]
mod cleanup_tests {
    use super::*;
    #[test]
    fn idle_cache_budget_tracks_capacity_take_replacement_and_drop() {
        let used=AtomicUsize::new(0);
        let frames=|| (vec![0.0f32;8],vec![0.0f32;8]);
        let sample=frames();
        let bytes=(sample.0.capacity()+sample.1.capacity())*4;
        let mut a=FrameCache::new(&used,bytes*2);
        let mut b=FrameCache::new(&used,bytes*2);
        let mut c=FrameCache::new(&used,bytes*2);
        a.store(sample); b.store(frames()); c.store(frames());
        assert_eq!(used.load(Ordering::Relaxed),bytes*2);
        assert_eq!(c.bytes,0);
        let active=a.take(); assert_eq!(used.load(Ordering::Relaxed),bytes);
        c.store(frames()); assert_eq!(used.load(Ordering::Relaxed),bytes*2);
        drop(b); assert_eq!(used.load(Ordering::Relaxed),bytes);
        a.store(active); assert_eq!(used.load(Ordering::Relaxed),bytes*2);
        a.store((Vec::new(),Vec::new())); assert_eq!(used.load(Ordering::Relaxed),bytes);
        drop(c); assert_eq!(used.load(Ordering::Relaxed),0);
    }
    #[test]
    fn concurrent_idle_cache_admission_and_teardown_stay_bounded() {
        let used=AtomicUsize::new(0);
        let frame=|| (vec![0.0f32;8],vec![0.0f32;8]);
        let sample=frame();
        let bytes=(sample.0.capacity()+sample.1.capacity())*std::mem::size_of::<f32>();
        let barrier=std::sync::Barrier::new(4);
        std::thread::scope(|scope| {
            let mut handles=Vec::new();
            for _ in 0..4 {
                handles.push(scope.spawn(|| {
                    let mut cache=FrameCache::new(&used,bytes*2);
                    let mut valid=true;
                    for _ in 0..64 {
                        barrier.wait();
                        cache.store(frame());
                        barrier.wait();
                        // No thread releases its entry until all observers have checked.
                        let retained=used.load(Ordering::Relaxed);
                        barrier.wait();
                        let active=cache.take();
                        barrier.wait();
                        let released=used.load(Ordering::Relaxed);
                        barrier.wait();
                        // Delay assertions until every round has finished: never strand peers.
                        valid &= retained==bytes*2 && released==0;
                        drop(active);
                    }
                    cache.store(frame());
                    barrier.wait();
                    // Drop on worker exit must release the shared admission charge.
                    assert!(valid,"concurrent cache accounting violated");
                }));
            }
            for handle in handles {handle.join().unwrap();}
        });
        assert_eq!(used.load(Ordering::Relaxed),0);
    }
    #[test]
    fn world_geometry_and_frame_capacity_are_checked() {
        for channel in [1,2,4] {
            let stride=(17*4*channel+4) as isize;
            for sign in [-1,1] {assert!(checked_world_geometry(17,31,stride*sign,channel,16).is_ok());}
            assert!(checked_world_geometry(17,31,1,channel,16).is_err());
            assert!(checked_world_geometry(17,31,stride,channel,0).is_err());
        }
        assert!(checked_world_geometry(1,1,16,4,1).is_err());
        assert!(checked_world_geometry(1,1,i32::MIN as isize,1,16).is_err());
        assert!(checked_world_geometry(1,3,isize::MAX,1,16).is_err());
        assert!(checked_world_geometry(usize::MAX,1,4,1,16).is_err());
        let mut frame=vec![1.0;8];
        resize_frame(&mut frame,1,1).unwrap(); assert_eq!(frame,[1.0;4]);
        assert!(resize_frame(&mut frame,usize::MAX,2).is_err()); assert_eq!(frame,[1.0;4]);
        resize_frame(&mut frame,0,3).unwrap(); assert!(frame.is_empty());
    }
    #[test]
    fn input_checkin_preserves_primary_failure() {
        for outcome in 0..3 {for release_fails in [false,true] {
            let calls=std::cell::Cell::new(0);
            let result=with_pixel_checkin(||match outcome {
                0=>Ok(42),1=>Err(ae::Error::BadCallbackParameter),_=>panic!("injected render panic"),
            },||{calls.set(calls.get()+1);if release_fails {Err(ae::Error::Generic)}else{Ok(())}});
            assert_eq!(calls.get(),1);
            assert_eq!(result,match outcome {
                0=>if release_fails {Err(ae::Error::Generic)}else{Ok(42)},
                1=>Err(ae::Error::BadCallbackParameter),_=>Err(ae::Error::InternalStructDamaged),
            });
        }}
    }
}

fn smart_render(
    extra: &ae::pf::SmartRenderExtra,
    params: &ae::Parameters<Params>,
) -> Result<(), ae::Error> {
    let cb = extra.callbacks();
    let input = cb.checkout_layer_pixels(0)?.ok_or(ae::Error::Generic)?;
    with_pixel_checkin(|| {
        let mut output = cb.checkout_output()?.ok_or(ae::Error::Generic)?;
        let time = extra
            .pre_render_data::<PreRenderState>()
            .map(|x| x.time)
            .unwrap_or(0);
        render_world(&input, &mut output, &read(params, time)?)
    },||cb.checkin_layer_pixels(0))
}
