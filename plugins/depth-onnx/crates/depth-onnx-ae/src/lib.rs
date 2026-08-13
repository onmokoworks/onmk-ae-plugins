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
        _: ae::InData,
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
            }
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
                    cb.checkin_layer_pixels(0)?;
                    return Ok(());
                };

                let rgba = match render::world_to_float_rgba(&input_world) {
                    Some(buf) => buf,
                    None => {
                        output_world.copy_from(&input_world, None, None)?;
                        cb.checkin_layer_pixels(0)?;
                        return Ok(());
                    }
                };

                let bundled = CATALOG.lock().bundled_models.clone();
                let (model_dir, manifest, model_error) =
                    resolve_model_selection(model_popup, &custom_path, &bundled);

                if let Some(error) = model_error {
                    show_user_error(&mut plugin.out_data, &error);
                    cb.checkin_layer_pixels(0)?;
                    return Ok(());
                }

                let (model_dir, manifest) = (model_dir.unwrap(), manifest.unwrap());
                let mut engine = match engine() {
                    Ok(guard) => guard,
                    Err(err) => {
                        show_user_error(&mut plugin.out_data, &err);
                        cb.checkin_layer_pixels(0)?;
                        return Ok(());
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
                        normalize_depth(&mut result.depth, result.d_min, result.d_max);
                        let cache_key = in_data.effect_ref().as_ptr() as u64;
                        SMOOTHING.blend(cache_key, resolution.size(), &mut result.depth, smoothing);
                        render::write_depth_to_world(
                            &mut output_world,
                            &result.depth,
                            result.width,
                            invert,
                        )?;
                    }
                    Err(err) => {
                        show_user_error(&mut plugin.out_data, &err.to_string());
                    }
                }

                cb.checkin_layer_pixels(0)?;
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

fn normalize_depth(depth: &mut [f32], lo: f32, hi: f32) {
    let scale = if hi > lo { 1.0 / (hi - lo) } else { 0.0 };
    for v in depth.iter_mut() {
        *v = (*v - lo) * scale;
    }
}

fn show_user_error(out_data: &mut ae::OutData, message: &str) {
    out_data.set_return_msg(&format!("{USER_ERROR_PREFIX}{message}"));
    out_data.set_out_flag(ae::OutFlags::DisplayErrorMessage, true);
}
