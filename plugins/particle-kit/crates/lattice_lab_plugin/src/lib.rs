use after_effects as ae;
use std::fs;
use std::path::PathBuf;

#[cfg(windows)]
mod file_dialog {
    use std::path::PathBuf;
    use std::process::Command;

    pub fn open_file_dialog(initial_dir: &str, title: &str) -> Option<PathBuf> {
        let script = format!(
            "Add-Type -AssemblyName System.Windows.Forms; \
             $d = New-Object System.Windows.Forms.OpenFileDialog; \
             $d.InitialDirectory = '{}'; \
             $d.Filter = 'JSON (*.json)|*.json|All Files|*.*'; \
             $d.Title = '{}'; \
             if ($d.ShowDialog() -eq 'OK') {{ Write-Output $d.FileName }}",
            initial_dir.replace('\'', "''"),
            title.replace('\'', "''")
        );
        let output = Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .output()
            .ok()?;
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if path.is_empty() {
            None
        } else {
            Some(PathBuf::from(path))
        }
    }

    pub fn save_file_dialog(initial_dir: &str, title: &str, default_name: &str) -> Option<PathBuf> {
        let script = format!(
            "Add-Type -AssemblyName System.Windows.Forms; \
             $d = New-Object System.Windows.Forms.SaveFileDialog; \
             $d.InitialDirectory = '{}'; \
             $d.Filter = 'JSON (*.json)|*.json'; \
             $d.Title = '{}'; \
             $d.FileName = '{}'; \
             $d.DefaultExt = 'json'; \
             if ($d.ShowDialog() -eq 'OK') {{ Write-Output $d.FileName }}",
            initial_dir.replace('\'', "''"),
            title.replace('\'', "''"),
            default_name.replace('\'', "''")
        );
        let output = Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .output()
            .ok()?;
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if path.is_empty() {
            None
        } else {
            Some(PathBuf::from(path))
        }
    }
}

// ---- File dialog via osascript (macOS) ----
#[cfg(target_os = "macos")]
mod file_dialog {
    use std::path::PathBuf;
    use std::process::Command;

    fn applescript_string(text: &str) -> String {
        format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
    }

    fn run(script: &str) -> Option<PathBuf> {
        let output = Command::new("osascript")
            .args(["-e", script])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if path_str.is_empty() {
            None
        } else {
            Some(PathBuf::from(path_str))
        }
    }

    pub fn open_file_dialog(initial_dir: &str, title: &str) -> Option<PathBuf> {
        run(&format!(
            "activate\nPOSIX path of (choose file with prompt {} default location (POSIX file {}) of type {{\"json\", \"public.json\"}})",
            applescript_string(title),
            applescript_string(initial_dir)
        ))
    }

    pub fn save_file_dialog(initial_dir: &str, title: &str, default_name: &str) -> Option<PathBuf> {
        let path = run(&format!(
            "activate\nPOSIX path of (choose file name with prompt {} default name {} default location (POSIX file {}))",
            applescript_string(title),
            applescript_string(default_name),
            applescript_string(initial_dir)
        ))?;
        // Match the Windows dialog's DefaultExt = json.
        if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
        {
            Some(path)
        } else {
            Some(path.with_extension("json"))
        }
    }
}

mod lattice;
mod lattice_ae_adapter;
mod lattice_project_state;
mod node_graph_core {
    pub(crate) use particlelab_engine_core::node_graph_core::*;
}
mod render_core {
    pub(crate) use particlelab_engine_core::render_core::*;
}

use lattice::LatticeGraphDocument;
use lattice_ae_adapter::{
    render_lattice_lab_8bit, resolve_lattice_lab_engine_config, LatticeLabAdapterError,
    LatticeLabHostBlendMode, LatticeLabHostParams, LatticeLabParam as Params,
    LatticeLabRenderRequest,
};
use lattice_project_state::{
    LatticeLabProjectState, LatticeNodeUiGraphStateSnapshot, LATTICE_PROJECT_STATE_VERSION,
};

const NODE_UI_SHELL_INDEX_HTML: &str = include_str!("../../../tools/node-ui-shell/index.html");
const NODE_UI_SHELL_APP_JS: &str = include_str!("../../../tools/node-ui-shell/app.js");
const NODE_UI_SHELL_STYLES_CSS: &str = include_str!("../../../tools/node-ui-shell/styles.css");
const NODE_UI_SHELL_STARTUP_JS: &str =
    include_str!("../../../tools/node-ui-shell/startup-payload.js");

#[derive(Default)]
struct Plugin;

ae::define_effect!(Plugin, LatticeLabProjectState, Params);

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        params: &mut ae::Parameters<Params>,
        _in_data: ae::InData,
        _out_data: ae::OutData,
    ) -> Result<(), ae::Error> {
        let defaults = LatticeLabHostParams::default();

        params.add_group(
            Params::PointsGroupStart,
            Params::PointsGroupEnd,
            "Points",
            false,
            |params| {
                add_slider(
                    params,
                    Params::GridResolutionX,
                    "Grid Resolution X",
                    1,
                    512,
                    1,
                    128,
                    defaults.grid_resolution[0] as i32,
                )?;
                add_slider(
                    params,
                    Params::GridResolutionY,
                    "Grid Resolution Y",
                    1,
                    512,
                    1,
                    128,
                    defaults.grid_resolution[1] as i32,
                )?;
                add_slider(
                    params,
                    Params::GridResolutionZ,
                    "Grid Resolution Z",
                    1,
                    512,
                    1,
                    64,
                    defaults.grid_resolution[2] as i32,
                )?;
                add_float(
                    params,
                    Params::GridSpacing,
                    "Grid Spacing",
                    0.0,
                    10_000.0,
                    1.0,
                    500.0,
                    defaults.grid_spacing,
                    1,
                )?;
                add_slider(
                    params,
                    Params::MaxPoints,
                    "Max Points",
                    1,
                    20_000,
                    100,
                    20_000,
                    defaults.max_points as i32,
                )?;
                Ok(())
            },
        )?;

        params.add_group(
            Params::NoiseGroupStart,
            Params::NoiseGroupEnd,
            "Noise",
            true,
            |params| {
                add_checkbox(
                    params,
                    Params::NoiseEnabled,
                    "Enable Noise",
                    defaults.noise_enabled,
                )?;
                add_float(
                    params,
                    Params::NoiseAmplitude,
                    "Amplitude",
                    0.0,
                    10_000.0,
                    0.0,
                    500.0,
                    defaults.noise_amplitude,
                    1,
                )?;
                add_float(
                    params,
                    Params::NoiseFrequency,
                    "Frequency",
                    0.0001,
                    100.0,
                    0.001,
                    2.0,
                    defaults.noise_frequency,
                    3,
                )?;
                add_float(
                    params,
                    Params::NoiseSpeed,
                    "Speed",
                    0.0,
                    100.0,
                    0.0,
                    10.0,
                    defaults.noise_speed,
                    2,
                )?;
                add_slider(
                    params,
                    Params::NoiseOctaves,
                    "Octaves",
                    1,
                    8,
                    1,
                    8,
                    defaults.noise_octaves as i32,
                )?;
                add_float(
                    params,
                    Params::NoiseAxisScaleX,
                    "Axis Scale X",
                    0.0,
                    10.0,
                    0.0,
                    2.0,
                    defaults.noise_axis_scale[0],
                    2,
                )?;
                add_float(
                    params,
                    Params::NoiseAxisScaleY,
                    "Axis Scale Y",
                    0.0,
                    10.0,
                    0.0,
                    2.0,
                    defaults.noise_axis_scale[1],
                    2,
                )?;
                add_float(
                    params,
                    Params::NoiseAxisScaleZ,
                    "Axis Scale Z",
                    0.0,
                    10.0,
                    0.0,
                    2.0,
                    defaults.noise_axis_scale[2],
                    2,
                )?;
                Ok(())
            },
        )?;

        params.add_group(
            Params::LinksGroupStart,
            Params::LinksGroupEnd,
            "Links",
            true,
            |params| {
                add_checkbox(
                    params,
                    Params::LinksEnabled,
                    "Enable Links",
                    defaults.links_enabled,
                )?;
                add_float(
                    params,
                    Params::LinksMaxDistance,
                    "Max Distance",
                    0.0,
                    10_000.0,
                    0.0,
                    1_000.0,
                    defaults.links_max_distance,
                    1,
                )?;
                add_float(
                    params,
                    Params::LinksWidth,
                    "Width",
                    0.0,
                    1_000.0,
                    0.1,
                    20.0,
                    defaults.links_width,
                    2,
                )?;
                add_float(
                    params,
                    Params::LinksOpacityFalloff,
                    "Opacity Falloff",
                    0.0,
                    1.0,
                    0.0,
                    1.0,
                    defaults.links_opacity_falloff,
                    2,
                )?;
                add_color(
                    params,
                    Params::LinksColor,
                    "Link Color",
                    defaults.links_color,
                )?;
                Ok(())
            },
        )?;

        params.add_group(
            Params::MeshGroupStart,
            Params::MeshGroupEnd,
            "Mesh",
            true,
            |params| {
                add_checkbox(
                    params,
                    Params::MeshEnabled,
                    "Enable Mesh",
                    defaults.mesh_enabled,
                )?;
                add_float(
                    params,
                    Params::MeshMaxEdge,
                    "Max Edge Length",
                    0.0,
                    10_000.0,
                    0.0,
                    1_000.0,
                    defaults.mesh_max_edge,
                    1,
                )?;
                add_float(
                    params,
                    Params::MeshOpacity,
                    "Mesh Opacity",
                    0.0,
                    1.0,
                    0.0,
                    1.0,
                    defaults.mesh_opacity,
                    2,
                )?;
                add_color(params, Params::MeshColor, "Mesh Color", defaults.mesh_color)?;
                Ok(())
            },
        )?;

        params.add_group(
            Params::RenderGroupStart,
            Params::RenderGroupEnd,
            "Render",
            false,
            |params| {
                add_float(
                    params,
                    Params::PointSize,
                    "Point Size",
                    0.0,
                    1_000.0,
                    0.1,
                    50.0,
                    defaults.point_size,
                    1,
                )?;
                add_color(
                    params,
                    Params::PointColor,
                    "Point Color",
                    defaults.point_color,
                )?;
                add_popup(
                    params,
                    Params::BlendMode,
                    "Blend Mode",
                    &["Normal", "Add", "Screen"],
                    blend_mode_to_popup(defaults.blend_mode),
                )?;
                Ok(())
            },
        )?;

        params.add_group(
            Params::SystemGroupStart,
            Params::SystemGroupEnd,
            "System",
            true,
            |params| {
                add_slider(
                    params,
                    Params::Seed,
                    "Seed",
                    0,
                    1_000_000,
                    0,
                    10_000,
                    defaults.seed.min(1_000_000) as i32,
                )?;
                Ok(())
            },
        )?;

        params.add_group(
            Params::NodeGraphGroupStart,
            Params::NodeGraphGroupEnd,
            "Node Graph",
            true,
            |params| {
                add_button(
                    params,
                    Params::ExportNodeGraphState,
                    "Export Node Graph",
                    "Export",
                )?;
                add_button(
                    params,
                    Params::ImportNodeGraphState,
                    "Import Node Graph",
                    "Import",
                )?;
                add_button(
                    params,
                    Params::SeedNodeGraphFromParams,
                    "Seed From Current Params",
                    "Seed",
                )?;
                add_button(
                    params,
                    Params::DisableNodeGraph,
                    "Disable Node Graph",
                    "Disable",
                )?;
                Ok(())
            },
        )?;

        params.add_group(
            Params::NodeUiSidecarGroupStart,
            Params::NodeUiSidecarGroupEnd,
            "Node UI Sidecar",
            true,
            |params| {
                add_button(
                    params,
                    Params::OpenNodeUiShell,
                    "Open Node UI Shell",
                    "Open Shell",
                )?;
                Ok(())
            },
        )
    }

    fn handle_command(
        &mut self,
        command: ae::Command,
        _in_data: ae::InData,
        mut out_data: ae::OutData,
        _params: &mut ae::Parameters<Params>,
    ) -> Result<(), ae::Error> {
        match command {
            ae::Command::About => {
                out_data.set_return_msg(
                    "Lattice Lab v0.1\rCompanion effect for Particle Kit engine-core experiments.",
                );
            }
            ae::Command::GlobalSetup => {
                out_data.set_out_flag(ae::OutFlags::PixIndependent, true);
                out_data.set_out_flag(ae::OutFlags::NonParamVary, true);
                out_data.set_out_flag(ae::OutFlags::DeepColorAware, true);
                out_data.set_out_flag(ae::OutFlags::SequenceDataNeedsFlattening, true);
                out_data.set_out_flag2(ae::OutFlags2::ParamGroupStartCollapsedFlag, true);
                out_data.set_out_flag2(ae::OutFlags2::SupportsThreadedRendering, true);
                out_data.set_out_flag2(ae::OutFlags2::SupportsGetFlattenedSequenceData, true);
            }
            _ => {}
        }
        Ok(())
    }
}

impl AdobePluginInstance for LatticeLabProjectState {
    fn flatten(&self) -> Result<(u16, Vec<u8>), ae::Error> {
        self.flatten_bytes()
            .map(|bytes| (LATTICE_PROJECT_STATE_VERSION, bytes))
            .map_err(|_| ae::Error::Generic)
    }

    fn unflatten(version: u16, serialized: &[u8]) -> Result<Self, ae::Error> {
        LatticeLabProjectState::unflatten_bytes(version, serialized).map_err(|_| ae::Error::Generic)
    }

    fn render(
        &self,
        plugin: &mut PluginState,
        _in_layer: &ae::Layer,
        out_layer: &mut ae::Layer,
    ) -> Result<(), ae::Error> {
        render_lattice(plugin.params, self, &plugin.in_data, out_layer)
    }

    fn handle_command(
        &mut self,
        plugin: &mut PluginState,
        command: ae::Command,
    ) -> Result<(), ae::Error> {
        if let ae::Command::UserChangedParam { param_index } = command {
            handle_user_changed_param(param_index, plugin.params, self, &mut plugin.out_data)?;
        }
        Ok(())
    }
}

fn handle_user_changed_param(
    param_index: usize,
    params: &mut ae::Parameters<Params>,
    state: &mut LatticeLabProjectState,
    out_data: &mut ae::OutData,
) -> Result<(), ae::Error> {
    match params.type_at(param_index) {
        Params::SeedNodeGraphFromParams => {
            let host_params = host_params_from_params(params)?;
            let document =
                LatticeGraphDocument::from_engine_config(&host_params.to_engine_config());
            state
                .commit_node_graph_document(document)
                .map_err(|_| ae::Error::InvalidParms)?;
            out_data.set_return_msg("Lattice Lab node graph seeded from current parameters.");
            out_data.set_force_rerender();
        }
        Params::DisableNodeGraph => {
            state.set_graph_enabled(false);
            out_data.set_return_msg("Lattice Lab node graph disabled.");
            out_data.set_force_rerender();
        }
        Params::ExportNodeGraphState => export_lattice_node_graph_state(state, out_data)?,
        Params::ImportNodeGraphState => import_lattice_node_graph_state(state, out_data)?,
        Params::OpenNodeUiShell => open_lattice_node_ui_shell(state, out_data)?,
        _ => {}
    }
    Ok(())
}

fn export_lattice_node_graph_state(
    state: &LatticeLabProjectState,
    out_data: &mut ae::OutData,
) -> Result<(), ae::Error> {
    {
        let dir = lattice_node_graph_state_root_dir()?;
        fs::create_dir_all(&dir).map_err(|_| ae::Error::Generic)?;
        let dir_str = dir.to_string_lossy().to_string();
        let default_name = timestamped_json_name("latticelab_node_graph");
        if let Some(path) =
            file_dialog::save_file_dialog(&dir_str, "Export Lattice Lab Node Graph", &default_name)
        {
            let json = lattice_node_ui_export_json(state).map_err(|_| ae::Error::Generic)?;
            fs::write(&path, json).map_err(|_| ae::Error::Generic)?;
            out_data.set_return_msg(&format!(
                "Lattice Lab node graph exported: {}",
                path.file_name().unwrap_or_default().to_string_lossy()
            ));
        }
    }
    Ok(())
}

fn import_lattice_node_graph_state(
    state: &mut LatticeLabProjectState,
    out_data: &mut ae::OutData,
) -> Result<(), ae::Error> {
    {
        let dir = lattice_node_graph_state_root_dir()?;
        fs::create_dir_all(&dir).map_err(|_| ae::Error::Generic)?;
        let dir_str = dir.to_string_lossy().to_string();
        if let Some(path) = file_dialog::open_file_dialog(&dir_str, "Import Lattice Lab Node Graph")
        {
            let contents = fs::read_to_string(&path).map_err(|_| ae::Error::Generic)?;
            let snapshot = match LatticeNodeUiGraphStateSnapshot::from_node_ui_json(&contents) {
                Ok(snapshot) => snapshot,
                Err(_) => {
                    out_data.set_return_msg("Lattice Lab node graph import failed: invalid JSON.");
                    return Ok(());
                }
            };
            if state.commit_node_ui_graph_state_snapshot(snapshot).is_err() {
                out_data
                    .set_return_msg("Lattice Lab node graph import failed: incompatible graph.");
                return Ok(());
            }
            let label = path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            out_data.set_return_msg(&format!("Lattice Lab node graph imported: {}", label));
            out_data.set_out_flag(ae::OutFlags::RefreshUi, true);
            out_data.set_force_rerender();
        }
    }
    Ok(())
}

fn lattice_node_ui_export_json(
    state: &LatticeLabProjectState,
) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(&state.node_ui_bootstrap_payload())
}

fn node_ui_shell_assets() -> [(&'static str, &'static str); 4] {
    [
        ("index.html", NODE_UI_SHELL_INDEX_HTML),
        ("app.js", NODE_UI_SHELL_APP_JS),
        ("styles.css", NODE_UI_SHELL_STYLES_CSS),
        ("startup-payload.js", NODE_UI_SHELL_STARTUP_JS),
    ]
}

fn lattice_node_ui_shell_startup_payload_js(
    state: &LatticeLabProjectState,
) -> Result<String, ae::Error> {
    let json = serde_json::to_string_pretty(&state.node_ui_bootstrap_payload())
        .map_err(|_| ae::Error::Generic)?;
    Ok(format!(
        "window.PARTICLELAB_NODE_UI_BOOTSTRAP = {};\nwindow.PARTICLELAB_NODE_UI_BOOTSTRAP_SOURCE = \"Lattice Lab AE sidecar startup payload\";\n",
        json
    ))
}

fn open_lattice_node_ui_shell(
    state: &LatticeLabProjectState,
    out_data: &mut ae::OutData,
) -> Result<(), ae::Error> {
    match open_lattice_node_ui_shell_sidecar(state) {
        Ok(path) => {
            out_data.set_return_msg(&format!(
                "Lattice Lab Node UI shell opened: {}",
                path.file_name().unwrap_or_default().to_string_lossy()
            ));
            Ok(())
        }
        Err(err) => {
            out_data.set_return_msg("Lattice Lab Node UI shell open failed.");
            Err(err)
        }
    }
}

fn lattice_node_ui_shell_root_dir() -> Result<PathBuf, ae::Error> {
    Ok(lattice_lab_documents_dir()?.join("node-ui-shell"))
}

/// `Documents/Lattice Lab` in the user's home folder (Windows: USERPROFILE, macOS: HOME).
fn lattice_lab_documents_dir() -> Result<PathBuf, ae::Error> {
    #[cfg(windows)]
    let home = std::env::var_os("USERPROFILE");
    #[cfg(not(windows))]
    let home = std::env::var_os("HOME");
    let home = home.filter(|h| !h.is_empty()).ok_or(ae::Error::Generic)?;
    Ok(PathBuf::from(home).join("Documents").join("Lattice Lab"))
}

/// Opens a file or folder with the system shell (Explorer / Finder).
fn reveal_in_shell(path: &std::path::Path) -> Result<(), ae::Error> {
    #[cfg(windows)]
    let program = "explorer.exe";
    #[cfg(target_os = "macos")]
    let program = "open";
    std::process::Command::new(program)
        .arg(path.as_os_str())
        .spawn()
        .map_err(|_| ae::Error::Generic)?;
    Ok(())
}

fn install_lattice_node_ui_shell_assets(
    state: &LatticeLabProjectState,
) -> Result<PathBuf, ae::Error> {
    let dir = lattice_node_ui_shell_root_dir()?;
    fs::create_dir_all(&dir).map_err(|_| ae::Error::Generic)?;
    for (filename, contents) in node_ui_shell_assets() {
        fs::write(dir.join(filename), contents).map_err(|_| ae::Error::Generic)?;
    }
    fs::write(
        dir.join("startup-payload.js"),
        lattice_node_ui_shell_startup_payload_js(state)?,
    )
    .map_err(|_| ae::Error::Generic)?;
    Ok(dir.join("index.html"))
}

fn open_lattice_node_ui_shell_sidecar(
    state: &LatticeLabProjectState,
) -> Result<PathBuf, ae::Error> {
    let index_path = install_lattice_node_ui_shell_assets(state)?;
    reveal_in_shell(&index_path)?;
    Ok(index_path)
}

fn lattice_node_graph_state_root_dir() -> Result<PathBuf, ae::Error> {
    Ok(lattice_lab_documents_dir()?.join("node-graphs"))
}

fn timestamped_json_name(prefix: &str) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs());
    format!("{}_{}.json", prefix, now)
}

fn checked_output_layout(width:usize,height:usize,stride:usize,bpp:usize)->Result<(usize,usize),ae::Error> {
    if width==0 || height==0 {return Ok((0,0));}
    let row=width.checked_mul(bpp).ok_or(ae::Error::OutOfMemory)?;
    let length=stride.checked_mul(height).ok_or(ae::Error::OutOfMemory)?;
    if stride<row || length>isize::MAX as usize {return Err(ae::Error::BadCallbackParameter);}
    Ok((row,length))
}

fn reverse_output_rows(output:&mut [u8],row:usize,height:usize,stride:usize)->Result<(),ae::Error> {
    if row==0 || height==0 {return Ok(());}
    let (_,length)=checked_output_layout(row,height,stride,1)?;
    if output.len()<length {return Err(ae::Error::BadCallbackParameter);}
    for y in 0..height/2 {
        let (front,back)=output.split_at_mut((height-1-y)*stride);
        front[y*stride..y*stride+row].swap_with_slice(&mut back[..row]);
    }
    Ok(())
}

fn render_lattice(
    params: &mut ae::Parameters<Params>,
    state: &LatticeLabProjectState,
    in_data: &ae::InData,
    out_layer: &mut ae::Layer,
) -> Result<(), ae::Error> {
    let host_params = host_params_from_params(params)?;
    let (config, _) = resolve_lattice_lab_engine_config(&host_params, state);
    let width = out_layer.width() as usize;
    let height = out_layer.height() as usize;
    let time = current_time_sec(in_data);

    let format=out_layer.pixel_format()?;
    let bpp=match format {
        ae::PixelFormat::Argb32=>4, ae::PixelFormat::Argb64=>8, ae::PixelFormat::Argb128=>16,
        _=>return Err(ae::Error::BadCallbackParameter),
    };
    if width>i32::MAX as usize || height>i32::MAX as usize || out_layer.row_bytes()==i32::MIN as isize {
        return Err(ae::Error::BadCallbackParameter);
    }
    if width==0 || height==0 {return Ok(());}
    let stride=out_layer.buffer_stride();
    let (row,_)=checked_output_layout(width,height,stride,bpp)?;
    if unsafe {out_layer.data_ptr()}.is_null() {return Err(ae::Error::BadCallbackParameter);}
    let reverse=out_layer.row_bytes()<0;
    if format == ae::PixelFormat::Argb64 {
        let runtime=lattice::LatticeRuntimeInputs::new(width,height,time).map_err(|_|ae::Error::OutOfMemory)?;
        let plan=lattice::build_lattice_render_plan(&config,runtime);
        let output=out_layer.buffer_mut();
        output.fill(0);
        lattice::render_lattice_plan_16bit(&plan,output,stride).map_err(|_|ae::Error::BadCallbackParameter)?;
    } else if format == ae::PixelFormat::Argb32 {
        let request = LatticeLabRenderRequest {
            width,
            height,
            row_bytes: out_layer.buffer_stride(),
            time,
            clear_output: true,
        };
        let output = out_layer.buffer_mut();
        render_lattice_lab_8bit(&config, request, output).map_err(map_lattice_error)?;
    } else {
        let row_bytes = width
            .checked_mul(render_core::RenderSurface::ARGB8_BYTES_PER_PIXEL)
            .ok_or(ae::Error::OutOfMemory)?;
        let length=row_bytes.checked_mul(height).ok_or(ae::Error::OutOfMemory)?;
        let mut flat=Vec::new();
        flat.try_reserve_exact(length).map_err(|_|ae::Error::OutOfMemory)?;
        flat.resize(length,0);
        let request = LatticeLabRenderRequest {
            width,
            height,
            row_bytes,
            time,
            clear_output: true,
        };
        render_lattice_lab_8bit(&config, request, &mut flat).map_err(map_lattice_error)?;
        copy_argb8_to_32bpc(&flat,out_layer.buffer_mut(),width,height,stride);
    }
    if reverse {reverse_output_rows(out_layer.buffer_mut(),row,height,stride)?;}
    Ok(())
}

fn host_params_from_params(
    params: &mut ae::Parameters<Params>,
) -> Result<LatticeLabHostParams, ae::Error> {
    Ok(LatticeLabHostParams {
        grid_resolution: [
            params.get(Params::GridResolutionX)?.as_slider()?.value() as u32,
            params.get(Params::GridResolutionY)?.as_slider()?.value() as u32,
            params.get(Params::GridResolutionZ)?.as_slider()?.value() as u32,
        ],
        grid_spacing: params.get(Params::GridSpacing)?.as_float_slider()?.value() as f32,
        max_points: params.get(Params::MaxPoints)?.as_slider()?.value() as u32,
        noise_enabled: params.get(Params::NoiseEnabled)?.as_checkbox()?.value(),
        noise_amplitude: params
            .get(Params::NoiseAmplitude)?
            .as_float_slider()?
            .value() as f32,
        noise_frequency: params
            .get(Params::NoiseFrequency)?
            .as_float_slider()?
            .value() as f32,
        noise_speed: params.get(Params::NoiseSpeed)?.as_float_slider()?.value() as f32,
        noise_octaves: params.get(Params::NoiseOctaves)?.as_slider()?.value() as u8,
        noise_axis_scale: [
            params
                .get(Params::NoiseAxisScaleX)?
                .as_float_slider()?
                .value() as f32,
            params
                .get(Params::NoiseAxisScaleY)?
                .as_float_slider()?
                .value() as f32,
            params
                .get(Params::NoiseAxisScaleZ)?
                .as_float_slider()?
                .value() as f32,
        ],
        links_enabled: params.get(Params::LinksEnabled)?.as_checkbox()?.value(),
        links_max_distance: params
            .get(Params::LinksMaxDistance)?
            .as_float_slider()?
            .value() as f32,
        links_width: params.get(Params::LinksWidth)?.as_float_slider()?.value() as f32,
        links_opacity_falloff: params
            .get(Params::LinksOpacityFalloff)?
            .as_float_slider()?
            .value() as f32,
        links_color: color_to_array(params.get(Params::LinksColor)?.as_color()?.value()),
        mesh_enabled: params.get(Params::MeshEnabled)?.as_checkbox()?.value(),
        mesh_max_edge: params.get(Params::MeshMaxEdge)?.as_float_slider()?.value() as f32,
        mesh_opacity: params.get(Params::MeshOpacity)?.as_float_slider()?.value() as f32,
        mesh_color: color_to_array(params.get(Params::MeshColor)?.as_color()?.value()),
        point_size: params.get(Params::PointSize)?.as_float_slider()?.value() as f32,
        point_color: color_to_array(params.get(Params::PointColor)?.as_color()?.value()),
        blend_mode: blend_mode_from_popup(params.get(Params::BlendMode)?.as_popup()?.value()),
        seed: params.get(Params::Seed)?.as_slider()?.value().max(0) as u64,
    })
}

fn add_slider(
    params: &mut ae::Parameters<Params>,
    id: Params,
    name: &str,
    min: i32,
    max: i32,
    slider_min: i32,
    slider_max: i32,
    default: i32,
) -> Result<(), ae::Error> {
    params.add(
        id,
        name,
        ae::SliderDef::setup(|f| {
            f.set_valid_min(min);
            f.set_valid_max(max);
            f.set_slider_min(slider_min);
            f.set_slider_max(slider_max);
            f.set_default(default.into());
            f.set_value(default.into());
        }),
    )
}

fn add_float(
    params: &mut ae::Parameters<Params>,
    id: Params,
    name: &str,
    min: f32,
    max: f32,
    slider_min: f32,
    slider_max: f32,
    default: f32,
    precision: i16,
) -> Result<(), ae::Error> {
    params.add(
        id,
        name,
        ae::FloatSliderDef::setup(|f| {
            f.set_valid_min(min);
            f.set_valid_max(max);
            f.set_slider_min(slider_min);
            f.set_slider_max(slider_max);
            f.set_default(default.into());
            f.set_value(default.into());
            f.set_precision(precision);
        }),
    )
}

fn add_checkbox(
    params: &mut ae::Parameters<Params>,
    id: Params,
    name: &str,
    default: bool,
) -> Result<(), ae::Error> {
    params.add(
        id,
        name,
        ae::CheckBoxDef::setup(|f| {
            f.set_default(default);
            f.set_label("Enable");
        }),
    )
}

fn add_color(
    params: &mut ae::Parameters<Params>,
    id: Params,
    name: &str,
    default: [f32; 4],
) -> Result<(), ae::Error> {
    params.add(
        id,
        name,
        ae::ColorDef::setup(|f| {
            let default = array_to_color(default);
            f.set_default(default);
            f.set_value(default);
        }),
    )
}

fn add_popup(
    params: &mut ae::Parameters<Params>,
    id: Params,
    name: &str,
    options: &[&str],
    default: i32,
) -> Result<(), ae::Error> {
    params.add(
        id,
        name,
        ae::PopupDef::setup(|f| {
            f.set_options(options);
            f.set_default(default);
            f.set_value(default);
        }),
    )
}

fn add_button(
    params: &mut ae::Parameters<Params>,
    id: Params,
    name: &str,
    label: &str,
) -> Result<(), ae::Error> {
    params.add(
        id,
        name,
        ae::ButtonDef::setup(|f| {
            f.set_label(label);
        }),
    )
}

fn current_time_sec(in_data: &ae::InData) -> f32 {
    let time = in_data.current_time() as f32;
    let scale = in_data.time_scale() as f32;
    if scale > 0.0 {
        time / scale
    } else {
        0.0
    }
}

fn color_to_array(value: ae::Pixel8) -> [f32; 4] {
    [
        value.red as f32 / 255.0,
        value.green as f32 / 255.0,
        value.blue as f32 / 255.0,
        value.alpha as f32 / 255.0,
    ]
}

fn array_to_color(value: [f32; 4]) -> ae::Pixel8 {
    ae::Pixel8 {
        alpha: (value[3].clamp(0.0, 1.0) * 255.0).round() as u8,
        red: (value[0].clamp(0.0, 1.0) * 255.0).round() as u8,
        green: (value[1].clamp(0.0, 1.0) * 255.0).round() as u8,
        blue: (value[2].clamp(0.0, 1.0) * 255.0).round() as u8,
    }
}

fn blend_mode_from_popup(value: i32) -> LatticeLabHostBlendMode {
    match value {
        2 => LatticeLabHostBlendMode::Add,
        3 => LatticeLabHostBlendMode::Screen,
        _ => LatticeLabHostBlendMode::Normal,
    }
}

fn blend_mode_to_popup(mode: LatticeLabHostBlendMode) -> i32 {
    match mode {
        LatticeLabHostBlendMode::Normal => 1,
        LatticeLabHostBlendMode::Add => 2,
        LatticeLabHostBlendMode::Screen => 3,
    }
}

fn map_lattice_error(error: LatticeLabAdapterError) -> ae::Error {
    match error {
        LatticeLabAdapterError::Surface(_) => ae::Error::OutOfMemory,
        LatticeLabAdapterError::OutputBufferTooSmall { .. } => ae::Error::BadCallbackParameter,
    }
}

fn copy_argb8_to_32bpc(flat: &[u8], buf: &mut [u8], width: usize, height: usize, stride: usize) {
    for y in 0..height {
        let src_row = y.saturating_mul(width).saturating_mul(4);
        let dst_row = y.saturating_mul(stride);
        for x in 0..width {
            let src = src_row + x * 4;
            let dst = dst_row + x * 16;
            if src + 3 < flat.len() && dst + 15 < buf.len() {
                for ch in 0..4 {
                    let bytes = (flat[src + ch] as f32 / 255.0).to_ne_bytes();
                    buf[dst + ch * 4] = bytes[0];
                    buf[dst + ch * 4 + 1] = bytes[1];
                    buf[dst + ch * 4 + 2] = bytes[2];
                    buf[dst + ch * 4 + 3] = bytes[3];
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_ae_adapter::LATTICE_LAB_EFFECT_IDENTITY;
    #[test]
    fn output_layout_and_row_reversal_are_checked() {
        for bpp in [4,8,16] {
            let row=2*bpp; let stride=row+3;
            let mut bytes:Vec<u8>=(0..stride*3).map(|v|v as u8).collect();
            let original=bytes.clone();
            assert_eq!(checked_output_layout(2,3,stride,bpp),Ok((row,stride*3)));
            reverse_output_rows(&mut bytes,row,3,stride).unwrap();
            for y in 0..3 {
                assert_eq!(&bytes[y*stride..y*stride+row],&original[(2-y)*stride..(2-y)*stride+row]);
                assert_eq!(&bytes[y*stride+row..(y+1)*stride],&original[y*stride+row..(y+1)*stride]);
            }
            reverse_output_rows(&mut bytes,row,3,stride).unwrap();
            assert_eq!(bytes,original);
        }
        let mut short=[99;7];
        assert!(reverse_output_rows(&mut short,4,2,4).is_err());
        assert_eq!(short,[99;7]);
        assert!(checked_output_layout(2,3,7,4).is_err());
        assert!(checked_output_layout(usize::MAX,2,8,16).is_err());
        assert_eq!(checked_output_layout(0,usize::MAX,0,16),Ok((0,0)));
    }

    #[test]
    fn sixteen_bit_render_retains_sub_eight_bit_point() {
        let mut plan=lattice::LatticeRenderPlan {
            runtime:lattice::LatticeRuntimeInputs::new(1,1,0.0).unwrap(),
            blend_mode:lattice::LatticeBlendMode::Normal,
            points:vec![lattice::LatticeRenderPoint {position:[0.5,0.5,0.0],size:2.0,color:[1.0,1.0,1.0,1.0/1024.0]}],
            links:vec![],mesh_edges:vec![],
        };
        let mut output=[0u8;8];
        lattice::render_lattice_plan_16bit(&plan,&mut output,8).unwrap();
        let values:Vec<u16>=output.chunks_exact(2).map(|b|u16::from_ne_bytes([b[0],b[1]])).collect();
        assert_eq!(values,[32;4],"full coverage alpha1/1024 must survive native16 rendering");
        plan.points.push(plan.points[0]);
        for (mode,rgb) in [(lattice::LatticeBlendMode::Normal,64),(lattice::LatticeBlendMode::Add,76),(lattice::LatticeBlendMode::Screen,58)] {
            plan.blend_mode=mode;
            let mut padded=[0u8;12]; padded[8..].fill(99);
            lattice::render_lattice_plan_16bit(&plan,&mut padded,12).unwrap();
            let values:Vec<u16>=padded[..8].chunks_exact(2).map(|b|u16::from_ne_bytes([b[0],b[1]])).collect();
            assert_eq!(values,[64,rgb,rgb,rgb]);
            assert_eq!(&padded[8..],&[99;4]);
        }
        let mut short=[99u8;7];
        assert!(lattice::render_lattice_plan_16bit(&plan,&mut short,8).is_err());
        assert_eq!(short,[99;7]);
    }

    #[test]
    fn plugin_identity_matches_lattice_lab_adapter() {
        assert_eq!(LATTICE_LAB_EFFECT_IDENTITY.name, "Lattice Lab");
        assert_eq!(LATTICE_LAB_EFFECT_IDENTITY.match_name, "LatticeLab");
        assert_eq!(LATTICE_LAB_EFFECT_IDENTITY.category, "Particle");
    }

    #[test]
    fn popup_values_roundtrip_blend_modes() {
        for mode in [
            LatticeLabHostBlendMode::Normal,
            LatticeLabHostBlendMode::Add,
            LatticeLabHostBlendMode::Screen,
        ] {
            assert_eq!(blend_mode_from_popup(blend_mode_to_popup(mode)), mode);
        }
    }

    #[test]
    fn color_helpers_roundtrip_argb8() {
        let pixel = ae::Pixel8 {
            alpha: 64,
            red: 128,
            green: 32,
            blue: 255,
        };
        let roundtrip = array_to_color(color_to_array(pixel));
        assert_eq!(roundtrip.alpha, pixel.alpha);
        assert_eq!(roundtrip.red, pixel.red);
        assert_eq!(roundtrip.green, pixel.green);
        assert_eq!(roundtrip.blue, pixel.blue);
    }

    #[test]
    fn exported_lattice_node_ui_json_imports_as_snapshot() {
        let mut state = LatticeLabProjectState::default();
        state
            .commit_node_graph_document(LatticeGraphDocument::simple_grid_network())
            .unwrap();

        let json = lattice_node_ui_export_json(&state).unwrap();
        let snapshot = LatticeNodeUiGraphStateSnapshot::from_node_ui_json(&json).unwrap();

        assert!(snapshot.enabled);
        assert!(snapshot.document.is_some());
    }

    #[test]
    fn lattice_node_ui_sidecar_assets_are_embedded_for_host_launch() {
        let assets = node_ui_shell_assets();
        assert_eq!(assets.len(), 4);
        assert!(assets.iter().any(|(filename, contents)| {
            *filename == "index.html"
                && contents.contains("Node UI Shell")
                && contents.contains("startup-payload.js")
                && contents.contains("app.js")
        }));
        assert!(assets.iter().any(|(filename, contents)| {
            *filename == "app.js"
                && contents.contains("createLatticeDemoPayload")
                && contents.contains("PARTICLELAB_NODE_UI_BOOTSTRAP")
                && contents.contains("canSaveCurrentPayload")
        }));
        assert!(assets.iter().any(|(filename, contents)| {
            *filename == "startup-payload.js"
                && contents.contains("PARTICLELAB_NODE_UI_BOOTSTRAP")
                && contents.contains("PARTICLELAB_NODE_UI_BOOTSTRAP_SOURCE")
        }));
    }

    #[test]
    fn lattice_node_ui_sidecar_startup_payload_carries_current_project_state() {
        let mut state = LatticeLabProjectState::default();
        state
            .commit_node_graph_document(LatticeGraphDocument::simple_grid_network())
            .unwrap();

        let startup = lattice_node_ui_shell_startup_payload_js(&state).unwrap();
        assert!(startup.starts_with("window.PARTICLELAB_NODE_UI_BOOTSTRAP = {"));
        assert!(startup.contains("\"product_id\": \"latticelab\""));
        assert!(startup.contains("\"enabled\": true"));
        assert!(startup.contains("\"document\""));
        assert!(startup.contains("Lattice Lab AE sidecar startup payload"));
    }

    #[test]
    fn timestamped_json_name_keeps_prefix_and_extension() {
        let name = timestamped_json_name("latticelab_node_graph");

        assert!(name.starts_with("latticelab_node_graph_"));
        assert!(name.ends_with(".json"));
    }
}
