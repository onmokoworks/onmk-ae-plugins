mod render;
mod state;

use after_effects as ae;
use depth_onnx_core::{load_manifest, DepthEngine, InferenceRequest, USER_ERROR_PREFIX};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use state::{resolve_model_selection, ModelCatalog, Resolution, SmoothingCache};
use std::sync::OnceLock;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
pub(crate) enum Params {
    Model,
    BrowseModel,
    Resolution,
    Normalization,
    Invert,
    Smoothing,
}

#[derive(Default)]
struct Plugin {
    _pad: u8,
}

#[derive(Default, Serialize, Deserialize)]
struct Instance {
    custom_path: String,
}

static CATALOG: Lazy<Mutex<ModelCatalog>> =
    Lazy::new(|| Mutex::new(ModelCatalog::scan()));
static ENGINE: OnceLock<Result<Mutex<DepthEngine>, String>> = OnceLock::new();
static SMOOTHING: Lazy<SmoothingCache> = Lazy::new(SmoothingCache::new);

ae::define_effect!(Plugin, Instance, Params);

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        params: &mut ae::Parameters<Params>,
        _: ae::InData,
        _: ae::OutData,
    ) -> Result<(), ae::Error> {
        let catalog = CATALOG.lock();
        let model_options: Vec<&str> = if catalog.bundled_models.is_empty() {
            vec!["(no models found)"]
        } else {
            catalog
                .bundled_models
                .iter()
                .map(|m| m.manifest.label.as_str())
                .collect()
        };

        params.add(Params::Model, "Model", ae::PopupDef::setup(|f| {
            f.set_options(&model_options);
            f.set_default(1);
        }))?;

        params.add_with_flags(
            Params::BrowseModel,
            "Browse Model Folder",
            ae::ButtonDef::setup(|f| {
                f.set_label("Choose…");
            }),
            ae::ParamFlag::SUPERVISE,
            ae::ParamUIFlags::empty(),
        )?;

        params.add(Params::Resolution, "Resolution", ae::PopupDef::setup(|f| {
            f.set_options(&[
                "266 (preview)",
                "392 (preview HQ)",
                "518 (final)",
            ]);
            f.set_default(1);
        }))?;

        params.add(Params::Normalization, "Normalization", ae::PopupDef::setup(|f| {
            f.set_options(&["Per-frame", "Fixed range"]);
            f.set_default(1);
        }))?;

        params.add(Params::Invert, "Invert", ae::CheckBoxDef::setup(|f| {
            f.set_default(false);
            f.set_label("Invert");
        }))?;

        params.add(
            Params::Smoothing,
            "Temporal Smoothing",
            ae::FloatSliderDef::setup(|f| {
                f.set_slider_min(0.0);
                f.set_slider_max(0.95);
                f.set_valid_min(0.0);
                f.set_valid_max(0.95);
                f.set_default(0.0);
                f.set_value(f.default());
                f.set_precision(2);
            }),
        )?;

        Ok(())
    }

    fn handle_command(
        &mut self,
        cmd: ae::Command,
        in_data: ae::InData,
        mut out_data: ae::OutData,
        _: &mut ae::Parameters<Params>,
    ) -> Result<(), ae::Error> {
        match cmd {
            ae::Command::About => {
                out_data.set_return_msg(
                    "Depth ONNX  0.9\r\rMonocular depth via ONNX Runtime.",
                );
            }
            ae::Command::GlobalSetup => {
                out_data.set_out_flag(ae::OutFlags::DeepColorAware, true);
                out_data.set_out_flag(ae::OutFlags::PixIndependent, true);
                out_data.set_out_flag(ae::OutFlags::UseOutputExtent, true);
                out_data.set_out_flag2(ae::OutFlags2::SupportsSmartRender, true);
                out_data.set_out_flag2(ae::OutFlags2::FloatColorAware, true);
                out_data.set_out_flag2(ae::OutFlags2::SupportsThreadedRendering, true);
                out_data.set_out_flag2(
                    ae::OutFlags2::SupportsGetFlattenedSequenceData,
                    true,
                );
            }
            ae::Command::SequenceSetdown => {
                SMOOTHING.remove(in_data.effect_ref().as_ptr() as u64);
            }
            ae::Command::GlobalSetdown => SMOOTHING.clear(),
            ae::Command::QueryDynamicFlags => {}
            _ => {}
        }
        Ok(())
    }
}

impl AdobePluginInstance for Instance {
    fn flatten(&self) -> Result<(u16, Vec<u8>), ae::Error> {
        bincode::serde::encode_to_vec(self, bincode::config::legacy())
            .map(|bytes| (1, bytes))
            .map_err(|_| ae::Error::InternalStructDamaged)
    }

    fn unflatten(version: u16, bytes: &[u8]) -> Result<Self, ae::Error> {
        if version != 1 {
            return Err(ae::Error::InternalStructDamaged);
        }
        bincode::serde::decode_from_slice(bytes, bincode::config::legacy())
            .map(|(instance, _)| instance)
            .map_err(|_| ae::Error::InternalStructDamaged)
    }

    fn render(
        &self,
        _: &mut PluginState,
        _: &ae::Layer,
        _: &mut ae::Layer,
    ) -> Result<(), ae::Error> {
        Ok(())
    }

    fn handle_command(
        &mut self,
        plugin: &mut PluginState,
        cmd: ae::Command,
    ) -> Result<(), ae::Error> {
        let in_data = &plugin.in_data;

        match cmd {
            ae::Command::SmartPreRender { mut extra } => {
                let req = extra.output_request();
                match extra.callbacks().checkout_layer(
                    0,
                    0,
                    &req,
                    in_data.current_time(),
                    in_data.time_step(),
                    in_data.time_scale(),
                ) {
                    Ok(in_result) => {
                        let _ = extra.union_result_rect(in_result.result_rect.into());
                        let _ = extra.union_max_result_rect(in_result.max_result_rect.into());
                    }
                    Err(err) => {
                        show_user_error(
                            &mut plugin.out_data,
                            &format!("pre-render failed: {err:?}"),
                        );
                    }
                }
            }
            ae::Command::SmartRender { mut extra } => {
                let cb = extra.callbacks();
                let Some(input_world) = cb.checkout_layer_pixels(0)? else {
                    return Ok(());
                };

                return with_checkin(|| {
                let model_popup = plugin.params.get(Params::Model)?.as_popup()?.value();
                let custom_path = plugin
                    .sequence
                    .as_ref()
                    .map(|instance| instance.custom_path.as_str())
                    .unwrap_or("");
                let resolution =
                    Resolution::from_popup(plugin.params.get(Params::Resolution)?.as_popup()?.value());
                let invert = plugin.params.get(Params::Invert)?.as_checkbox()?.value();
                let smoothing = plugin
                    .params
                    .get(Params::Smoothing)?
                    .as_float_slider()?
                    .value() as f32;

                let Some(mut output_world) = cb.checkout_output()? else {
                    return Ok(());
                };

                // Malformed worlds and allocation failures are errors, not successful passthrough.
                let rgba = render::world_to_float_rgba(&input_world)?;

                let bundled = CATALOG.lock().bundled_models.clone();
                let (model_dir, manifest, model_error) =
                    resolve_model_selection(model_popup, &custom_path, &bundled);

                if let Some(error) = model_error {
                    show_user_error(&mut plugin.out_data, &error);
                    return output_world.copy_from(&input_world, None, None);
                }

                let (model_dir, manifest) = (model_dir.unwrap(), manifest.unwrap());
                let mut engine = match engine() {
                    Ok(guard) => guard,
                    Err(err) => {
                        show_user_error(&mut plugin.out_data, &err);
                        return output_world.copy_from(&input_world, None, None);
                    }
                };
                engine.configure(model_dir, manifest);
                let request = InferenceRequest {
                    input_size: resolution.size(),
                    src_width: input_world.width() as i32,
                    src_height: input_world.height() as i32,
                    src_rgba: &rgba,
                    src_is_argb: true,
                };
                match engine.run(request) {
                    Ok(mut result) => {
                        normalize_depth(&mut result.depth, result.d_min, result.d_max)?;
                        let cache_key = in_data.effect_ref().as_ptr() as u64;
                        SMOOTHING.blend(cache_key, resolution.size(), &mut result.depth, smoothing)?;
                        render::write_depth_to_world(
                            &mut output_world,
                            &result.depth,
                            result.width,
                            invert,
                        )?;
                    }
                    Err(err) => {
                        show_user_error(&mut plugin.out_data, &err.to_string());
                        output_world.copy_from(&input_world, None, None)?;
                    }
                }

                Ok(())
                }, || cb.checkin_layer_pixels(0));
            }
            ae::Command::UserChangedParam { param_index } => {
                if plugin.params.type_at(param_index) == Params::BrowseModel {
                    #[cfg(target_os = "macos")]
                    {
                        if let Some(path) = rfd::FileDialog::new().pick_folder() {
                            let picked = path.display().to_string();
                            if load_manifest(&picked).is_err() {
                                show_user_error(
                                    &mut plugin.out_data,
                                    "invalid model folder (manifest.json missing or invalid)",
                                );
                                return Ok(());
                            }
                            if let Some(instance) = plugin.sequence.as_mut() {
                                instance.custom_path = picked;
                                plugin.out_data.set_force_rerender();
                            }
                        }
                    }
                }
            }
            _ => {}
        }

        Ok(())
    }
}

// Always release a successfully checked-out input, including early returns and
// fallible parameter/output operations. Preserve the render error if both fail.
fn with_checkin<E>(render: impl FnOnce() -> Result<(), E>, checkin: impl FnOnce() -> Result<(), E>) -> Result<(), E> {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(render));
    let cleanup = checkin();
    match result {
        Ok(result) => result.and(cleanup),
        Err(payload) => std::panic::resume_unwind(payload),
    }
}

#[cfg(test)]
mod checkout_tests {
    use super::with_checkin;
    use std::cell::Cell;

    #[test]
    fn missing_model_notification_and_fallback_cleanup_contract() {
        // Direct adapter contract: no runtime/model installation or host inference claim.
        let (directory, manifest, error) = super::resolve_model_selection(1, "", &[]);
        assert!(directory.is_none() && manifest.is_none());
        let error = error.expect("missing catalog must explain failure");
        assert!(error.contains("no models found"));
        for copy_fails in [false, true] {
            for checkin_fails in [false, true] {
                let mut raw: super::ae::sys::PF_OutData = unsafe { std::mem::zeroed() };
                let mut out = super::ae::OutData::from_raw(&mut raw);
                let order = std::cell::RefCell::new(Vec::new());
                let source = [32768u16, 16385, 16387, 1];
                let mut destination = [0u16; 4];
                let result = with_checkin(|| {
                    super::show_user_error(&mut out, &error);
                    order.borrow_mut().push("copy");
                    if copy_fails { Err("copy") } else { destination.copy_from_slice(&source); Ok(()) }
                }, || {
                    order.borrow_mut().push("checkin");
                    if checkin_fails { Err("checkin") } else { Ok(()) }
                });
                assert_eq!(*order.borrow(), ["copy", "checkin"]);
                assert_eq!(result, if copy_fails { Err("copy") } else if checkin_fails { Err("checkin") } else { Ok(()) });
                if !copy_fails { assert_eq!(destination, source); }
                let bytes: Vec<u8> = raw.return_msg.iter().map(|&v|v as u8).collect();
                let end = bytes.iter().position(|&v|v==0).unwrap();
                assert_eq!(std::str::from_utf8(&bytes[..end]).unwrap(), format!("{}{}",super::USER_ERROR_PREFIX,error));
                assert_ne!(raw.out_flags & super::ae::sys::PF_OutFlag_DISPLAY_ERROR_MESSAGE as i32,0);
            }
        }
    }

    #[test]
    fn user_error_is_bounded_terminated_and_preserves_utf8() {
        for message in ["missing model".to_owned(), "x".repeat(600), "モデル😀".repeat(100), "bad\0path".to_owned()] {
            let mut raw: super::ae::sys::PF_OutData = unsafe { std::mem::zeroed() };
            raw.return_msg.fill(88);
            let mut out = super::ae::OutData::from_raw(&mut raw);
            super::show_user_error(&mut out, &message);
            let bytes: Vec<u8> = raw.return_msg.iter().map(|&v| v as u8).collect();
            let end = bytes.iter().position(|&v| v == 0).expect("terminated message");
            assert!(end <= 255);
            let text = std::str::from_utf8(&bytes[..end]).unwrap();
            assert!(text.starts_with(super::USER_ERROR_PREFIX));
            assert!(bytes[end..].iter().all(|&v| v == 0));
            assert_ne!(raw.out_flags & super::ae::sys::PF_OutFlag_DISPLAY_ERROR_MESSAGE as i32, 0);
            if message == "missing model" { assert_eq!(text, "Depth ONNX: missing model"); }
        }
    }

    #[test]
    fn checked_out_input_is_released_before_panic_propagates() {
        let calls = Cell::new(0);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            with_checkin::<&str>(|| panic!("render panic"), || {
                calls.set(calls.get() + 1);
                Err("cleanup")
            })
        }));
        assert!(result.is_err());
        assert_eq!(calls.get(), 1);
        let payload = result.unwrap_err();
        assert_eq!(payload.downcast_ref::<&str>(), Some(&"render panic"));
    }

    #[test]
    fn checked_out_input_is_released_once_on_all_result_paths() {
        for render_result in [Ok(()), Err("render")] {
            for cleanup_result in [Ok(()), Err("cleanup")] {
                let calls = Cell::new(0);
                let result = with_checkin(|| render_result, || {
                    calls.set(calls.get() + 1);
                    cleanup_result
                });
                assert_eq!(calls.get(), 1);
                assert_eq!(result, render_result.and(cleanup_result));
            }
        }
    }
}

fn engine() -> Result<parking_lot::MutexGuard<'static, DepthEngine>, String> {
    let slot = ENGINE.get_or_init(|| {
        DepthEngine::new()
            .map(Mutex::new)
            .map_err(|e| e.to_string())
    });
    match slot {
        Ok(mutex) => Ok(mutex.lock()),
        Err(msg) => Err(msg.clone()),
    }
}

#[test]
fn finite_depth_extremes_normalize_without_nan_or_infinity() {
    for (lo,mid,hi) in [(-f32::MAX,0.0,f32::MAX),(0.0,f32::from_bits(1),f32::from_bits(2))] {
        let mut values=[lo,mid,hi];
        normalize_depth(&mut values,lo,hi).unwrap();
        assert_eq!(values,[0.0,0.5,1.0]);
    }
}

#[test]
fn depth_normalization_preserves_normal_math_and_rejects_invalid_results() {
    for (lo,hi) in [(0.0,1.0),(-2.0,7.0),(0.125,0.375),(3.0,3.0)] {
        let mut values:Vec<f32>=(0..31).map(|i|lo+(hi-lo)*i as f32/30.0).collect();
        let scale=if hi>lo {1.0/(hi-lo)}else{0.0};
        let expected:Vec<f32>=values.iter().map(|v|(*v-lo)*scale).collect();
        normalize_depth(&mut values,lo,hi).unwrap();
        assert_eq!(values,expected);
    }
    for (lo,hi,values) in [(1.0,0.0,vec![0.5]),(f32::NAN,1.0,vec![0.5]),
        (0.0,f32::INFINITY,vec![0.5]),(0.0,1.0,vec![0.5,f32::NAN]),(0.0,1.0,vec![2.0])] {
        let before:Vec<u32>=values.iter().map(|v|v.to_bits()).collect();
        let mut actual=values;
        assert!(normalize_depth(&mut actual,lo,hi).is_err());
        assert_eq!(actual.iter().map(|v|v.to_bits()).collect::<Vec<_>>(),before);
    }
}

fn normalize_depth(depth: &mut [f32], lo: f32, hi: f32) -> Result<(),ae::Error> {
    if !lo.is_finite() || !hi.is_finite() || hi<lo
        || depth.iter().any(|v|!v.is_finite() || *v<lo || *v>hi) {
        return Err(ae::Error::BadCallbackParameter);
    }
    if hi==lo {depth.fill(0.0); return Ok(());}
    let range=hi-lo;
    let scale=1.0/range;
    if range.is_finite() && scale.is_finite() {
        for v in depth.iter_mut() { *v=(*v-lo)*scale; }
    } else {
        let range=hi as f64-lo as f64;
        for v in depth.iter_mut() { *v=((*v as f64-lo as f64)/range) as f32; }
    }
    Ok(())
}

fn show_user_error(out_data: &mut ae::OutData, message: &str) {
    // The SDK wrapper asserts at 256 bytes and does not append a terminator.
    // Bound the message while building it; paths/runtime errors can be very long.
    let mut bounded = String::with_capacity(255);
    for ch in USER_ERROR_PREFIX.chars().chain(message.chars()) {
        let ch = if ch == '\0' { ' ' } else { ch };
        if bounded.len() + ch.len_utf8() > 255 { break; }
        bounded.push(ch);
    }
    out_data.as_mut().return_msg.fill(0);
    out_data.set_return_msg(&bounded);
    out_data.set_out_flag(ae::OutFlags::DisplayErrorMessage, true);
}
