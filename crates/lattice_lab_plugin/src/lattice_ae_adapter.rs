#![allow(dead_code)]

use crate::lattice::{
    build_lattice_render_plan, render_lattice_plan_8bit, LatticeBlendMode, LatticeEngineConfig,
    LatticeLinkConfig, LatticeMeshConfig, LatticeNoiseConfig, LatticePointSource,
    LatticePointSourceConfig, LatticeRenderConfig, LatticeRuntimeInputs,
};
use crate::lattice_project_state::{LatticeEngineConfigSource, LatticeLabProjectState};
use crate::render_core::{RenderSurface, RenderSurfaceError};

pub(crate) const LATTICE_LAB_EFFECT_NAME: &str = "Lattice Lab";
pub(crate) const LATTICE_LAB_MATCH_NAME: &str = "LatticeLab";
pub(crate) const LATTICE_LAB_CATEGORY: &str = "Particle";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LatticeLabEffectIdentity {
    pub(crate) name: &'static str,
    pub(crate) match_name: &'static str,
    pub(crate) category: &'static str,
}

pub(crate) const LATTICE_LAB_EFFECT_IDENTITY: LatticeLabEffectIdentity = LatticeLabEffectIdentity {
    name: LATTICE_LAB_EFFECT_NAME,
    match_name: LATTICE_LAB_MATCH_NAME,
    category: LATTICE_LAB_CATEGORY,
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum LatticeLabParam {
    PointsGroupStart,
    GridResolutionX,
    GridResolutionY,
    GridResolutionZ,
    GridSpacing,
    MaxPoints,
    PointsGroupEnd,
    NoiseGroupStart,
    NoiseEnabled,
    NoiseAmplitude,
    NoiseFrequency,
    NoiseSpeed,
    NoiseOctaves,
    NoiseAxisScaleX,
    NoiseAxisScaleY,
    NoiseAxisScaleZ,
    NoiseGroupEnd,
    LinksGroupStart,
    LinksEnabled,
    LinksMaxDistance,
    LinksWidth,
    LinksOpacityFalloff,
    LinksColor,
    LinksGroupEnd,
    MeshGroupStart,
    MeshEnabled,
    MeshMaxEdge,
    MeshOpacity,
    MeshColor,
    MeshGroupEnd,
    RenderGroupStart,
    PointSize,
    PointColor,
    BlendMode,
    RenderGroupEnd,
    SystemGroupStart,
    Seed,
    SystemGroupEnd,
    NodeGraphGroupStart,
    ExportNodeGraphState,
    ImportNodeGraphState,
    SeedNodeGraphFromParams,
    DisableNodeGraph,
    NodeGraphGroupEnd,
    NodeUiSidecarGroupStart,
    OpenNodeUiShell,
    NodeUiSidecarGroupEnd,
}

pub(crate) const LATTICE_LAB_PARAM_ABI_ORDER: &[LatticeLabParam] = &[
    LatticeLabParam::PointsGroupStart,
    LatticeLabParam::GridResolutionX,
    LatticeLabParam::GridResolutionY,
    LatticeLabParam::GridResolutionZ,
    LatticeLabParam::GridSpacing,
    LatticeLabParam::MaxPoints,
    LatticeLabParam::PointsGroupEnd,
    LatticeLabParam::NoiseGroupStart,
    LatticeLabParam::NoiseEnabled,
    LatticeLabParam::NoiseAmplitude,
    LatticeLabParam::NoiseFrequency,
    LatticeLabParam::NoiseSpeed,
    LatticeLabParam::NoiseOctaves,
    LatticeLabParam::NoiseAxisScaleX,
    LatticeLabParam::NoiseAxisScaleY,
    LatticeLabParam::NoiseAxisScaleZ,
    LatticeLabParam::NoiseGroupEnd,
    LatticeLabParam::LinksGroupStart,
    LatticeLabParam::LinksEnabled,
    LatticeLabParam::LinksMaxDistance,
    LatticeLabParam::LinksWidth,
    LatticeLabParam::LinksOpacityFalloff,
    LatticeLabParam::LinksColor,
    LatticeLabParam::LinksGroupEnd,
    LatticeLabParam::MeshGroupStart,
    LatticeLabParam::MeshEnabled,
    LatticeLabParam::MeshMaxEdge,
    LatticeLabParam::MeshOpacity,
    LatticeLabParam::MeshColor,
    LatticeLabParam::MeshGroupEnd,
    LatticeLabParam::RenderGroupStart,
    LatticeLabParam::PointSize,
    LatticeLabParam::PointColor,
    LatticeLabParam::BlendMode,
    LatticeLabParam::RenderGroupEnd,
    LatticeLabParam::SystemGroupStart,
    LatticeLabParam::Seed,
    LatticeLabParam::SystemGroupEnd,
    LatticeLabParam::NodeGraphGroupStart,
    LatticeLabParam::ExportNodeGraphState,
    LatticeLabParam::ImportNodeGraphState,
    LatticeLabParam::SeedNodeGraphFromParams,
    LatticeLabParam::DisableNodeGraph,
    LatticeLabParam::NodeGraphGroupEnd,
    LatticeLabParam::NodeUiSidecarGroupStart,
    LatticeLabParam::OpenNodeUiShell,
    LatticeLabParam::NodeUiSidecarGroupEnd,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LatticeLabHostBlendMode {
    Normal,
    Add,
    Screen,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LatticeLabHostParams {
    pub(crate) grid_resolution: [u32; 3],
    pub(crate) grid_spacing: f32,
    pub(crate) max_points: u32,
    pub(crate) noise_enabled: bool,
    pub(crate) noise_amplitude: f32,
    pub(crate) noise_frequency: f32,
    pub(crate) noise_speed: f32,
    pub(crate) noise_octaves: u8,
    pub(crate) noise_axis_scale: [f32; 3],
    pub(crate) links_enabled: bool,
    pub(crate) links_max_distance: f32,
    pub(crate) links_width: f32,
    pub(crate) links_opacity_falloff: f32,
    pub(crate) links_color: [f32; 4],
    pub(crate) mesh_enabled: bool,
    pub(crate) mesh_max_edge: f32,
    pub(crate) mesh_opacity: f32,
    pub(crate) mesh_color: [f32; 4],
    pub(crate) point_size: f32,
    pub(crate) point_color: [f32; 4],
    pub(crate) blend_mode: LatticeLabHostBlendMode,
    pub(crate) seed: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LatticeLabAdapterConfigSource {
    HostParams,
    GraphDocument,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LatticeLabAdapterError {
    Surface(RenderSurfaceError),
    OutputBufferTooSmall { required: usize, actual: usize },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LatticeLabRenderRequest {
    pub(crate) width: usize,
    pub(crate) height: usize,
    pub(crate) row_bytes: usize,
    pub(crate) time: f32,
    pub(crate) clear_output: bool,
}

impl Default for LatticeLabHostParams {
    fn default() -> Self {
        Self::from_engine_config(&LatticeEngineConfig::default_v1())
    }
}

impl LatticeLabHostParams {
    pub(crate) fn from_engine_config(config: &LatticeEngineConfig) -> Self {
        Self {
            grid_resolution: config.points.grid_resolution,
            grid_spacing: config.points.spacing,
            max_points: config.points.max_points,
            noise_enabled: config.noise.enabled,
            noise_amplitude: config.noise.amplitude,
            noise_frequency: config.noise.frequency,
            noise_speed: config.noise.speed,
            noise_octaves: config.noise.octaves,
            noise_axis_scale: config.noise.axis_scale,
            links_enabled: config.links.enabled,
            links_max_distance: config.links.max_distance,
            links_width: config.links.width,
            links_opacity_falloff: config.links.opacity_falloff,
            links_color: config.links.color,
            mesh_enabled: config.mesh.enabled,
            mesh_max_edge: config.mesh.max_edge,
            mesh_opacity: config.mesh.opacity,
            mesh_color: config.mesh.color,
            point_size: config.render.point_size,
            point_color: config.render.point_color,
            blend_mode: host_blend_mode(config.render.blend_mode),
            seed: config.seed,
        }
    }

    pub(crate) fn to_engine_config(&self) -> LatticeEngineConfig {
        LatticeEngineConfig {
            points: LatticePointSourceConfig {
                source: LatticePointSource::Grid,
                grid_resolution: self.grid_resolution,
                spacing: self.grid_spacing,
                max_points: self.max_points,
            },
            noise: LatticeNoiseConfig {
                enabled: self.noise_enabled,
                amplitude: self.noise_amplitude,
                frequency: self.noise_frequency,
                speed: self.noise_speed,
                octaves: self.noise_octaves,
                axis_scale: self.noise_axis_scale,
            },
            links: LatticeLinkConfig {
                enabled: self.links_enabled,
                max_distance: self.links_max_distance,
                width: self.links_width,
                opacity_falloff: self.links_opacity_falloff,
                color: self.links_color,
            },
            mesh: LatticeMeshConfig {
                enabled: self.mesh_enabled,
                max_edge: self.mesh_max_edge,
                opacity: self.mesh_opacity,
                color: self.mesh_color,
            },
            render: LatticeRenderConfig {
                point_size: self.point_size,
                point_color: self.point_color,
                blend_mode: lattice_blend_mode(self.blend_mode),
            },
            seed: self.seed,
        }
    }
}

impl LatticeLabRenderRequest {
    pub(crate) fn packed_argb8(
        width: usize,
        height: usize,
        time: f32,
    ) -> Result<Self, RenderSurfaceError> {
        let surface = RenderSurface::argb8(width, height)?;
        Ok(Self {
            width,
            height,
            row_bytes: surface.row_bytes,
            time,
            clear_output: true,
        })
    }

    pub(crate) fn surface(self) -> Result<RenderSurface, RenderSurfaceError> {
        RenderSurface::argb8_with_row_bytes(self.width, self.height, self.row_bytes)
    }
}

pub(crate) fn resolve_lattice_lab_engine_config(
    host_params: &LatticeLabHostParams,
    project_state: &LatticeLabProjectState,
) -> (LatticeEngineConfig, LatticeLabAdapterConfigSource) {
    if let Some((config, LatticeEngineConfigSource::GraphDocument)) =
        project_state.engine_config_override()
    {
        (config, LatticeLabAdapterConfigSource::GraphDocument)
    } else {
        (
            host_params.to_engine_config().normalized_for_render(),
            LatticeLabAdapterConfigSource::HostParams,
        )
    }
}

pub(crate) fn render_lattice_lab_8bit(
    config: &LatticeEngineConfig,
    request: LatticeLabRenderRequest,
    output: &mut [u8],
) -> Result<(), LatticeLabAdapterError> {
    let surface = request.surface().map_err(LatticeLabAdapterError::Surface)?;
    if !surface.fits_buffer(output) {
        return Err(LatticeLabAdapterError::OutputBufferTooSmall {
            required: surface.len_bytes,
            actual: output.len(),
        });
    }
    if request.clear_output {
        for byte in &mut output[..surface.len_bytes] {
            *byte = 0;
        }
    }

    let runtime = LatticeRuntimeInputs::argb8_with_row_bytes(
        request.width,
        request.height,
        request.row_bytes,
        request.time,
    )
    .map_err(LatticeLabAdapterError::Surface)?;
    let plan = build_lattice_render_plan(config, runtime);
    render_lattice_plan_8bit(&plan, output);
    Ok(())
}

fn lattice_blend_mode(mode: LatticeLabHostBlendMode) -> LatticeBlendMode {
    match mode {
        LatticeLabHostBlendMode::Normal => LatticeBlendMode::Normal,
        LatticeLabHostBlendMode::Add => LatticeBlendMode::Add,
        LatticeLabHostBlendMode::Screen => LatticeBlendMode::Screen,
    }
}

fn host_blend_mode(mode: LatticeBlendMode) -> LatticeLabHostBlendMode {
    match mode {
        LatticeBlendMode::Normal => LatticeLabHostBlendMode::Normal,
        LatticeBlendMode::Add => LatticeLabHostBlendMode::Add,
        LatticeBlendMode::Screen => LatticeLabHostBlendMode::Screen,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lattice::{LatticeGraphBlendMode, LatticeGraphDocument, LatticeGraphNodeKind};
    use std::collections::HashSet;

    #[test]
    fn lattice_lab_identity_is_distinct_from_particlekit() {
        assert_eq!(LATTICE_LAB_EFFECT_IDENTITY.name, "Lattice Lab");
        assert_eq!(LATTICE_LAB_EFFECT_IDENTITY.match_name, "LatticeLab");
        assert_eq!(LATTICE_LAB_EFFECT_IDENTITY.category, "Particle");
        assert_ne!(LATTICE_LAB_EFFECT_IDENTITY.match_name, "ParticleKit");
    }

    #[test]
    fn lattice_lab_param_manifest_is_append_only_ordered() {
        assert_eq!(LATTICE_LAB_PARAM_ABI_ORDER.len(), 47);
        let mut seen = HashSet::new();
        for (index, &param) in LATTICE_LAB_PARAM_ABI_ORDER.iter().enumerate() {
            assert!(seen.insert(param), "duplicate Lattice param: {:?}", param);
            assert_eq!(param as usize, index);
        }
        assert_eq!(
            LATTICE_LAB_PARAM_ABI_ORDER.last().copied(),
            Some(LatticeLabParam::NodeUiSidecarGroupEnd)
        );
    }

    #[test]
    fn default_host_params_match_default_lattice_engine_config() {
        let host_params = LatticeLabHostParams::default();
        let config = host_params.to_engine_config().normalized_for_render();
        let expected = LatticeEngineConfig::default_v1().normalized_for_render();

        assert_eq!(
            config.points.grid_resolution,
            expected.points.grid_resolution
        );
        assert_eq!(config.points.max_points, expected.points.max_points);
        assert_eq!(config.noise.enabled, expected.noise.enabled);
        assert_eq!(config.links.enabled, expected.links.enabled);
        assert_eq!(config.mesh.enabled, expected.mesh.enabled);
        assert_eq!(config.render.blend_mode, expected.render.blend_mode);
        assert_eq!(config.seed, expected.seed);
        assert!((config.points.spacing - expected.points.spacing).abs() < 0.0001);
        assert!((config.render.point_size - expected.render.point_size).abs() < 0.0001);
    }

    #[test]
    fn graph_project_state_overrides_host_params_at_adapter_boundary() {
        let mut host_params = LatticeLabHostParams::default();
        host_params.grid_resolution = [2, 2, 1];

        let mut document = LatticeGraphDocument::simple_grid_network();
        for node in &mut document.nodes {
            if let LatticeGraphNodeKind::GridPoints(data) = &mut node.kind {
                data.resolution = [3, 4, 1];
            }
            if let LatticeGraphNodeKind::Render(data) = &mut node.kind {
                data.blend_mode = LatticeGraphBlendMode::Add;
            }
        }
        let mut state = LatticeLabProjectState::default();
        state.commit_node_graph_document(document).unwrap();

        let (config, source) = resolve_lattice_lab_engine_config(&host_params, &state);

        assert_eq!(source, LatticeLabAdapterConfigSource::GraphDocument);
        assert_eq!(config.points.grid_resolution, [3, 4, 1]);
        assert_eq!(config.render.blend_mode, LatticeBlendMode::Add);

        state.set_graph_enabled(false);
        let (config, source) = resolve_lattice_lab_engine_config(&host_params, &state);

        assert_eq!(source, LatticeLabAdapterConfigSource::HostParams);
        assert_eq!(config.points.grid_resolution, [2, 2, 1]);
    }

    #[test]
    fn lattice_lab_render_adapter_clears_and_draws_argb8_output() {
        let mut host_params = LatticeLabHostParams::default();
        host_params.grid_resolution = [3, 3, 1];
        host_params.grid_spacing = 16.0;
        host_params.links_enabled = false;
        host_params.mesh_enabled = false;
        host_params.point_size = 3.0;
        let config = host_params.to_engine_config();
        let request = LatticeLabRenderRequest {
            width: 64,
            height: 48,
            row_bytes: 80 * RenderSurface::ARGB8_BYTES_PER_PIXEL,
            time: 0.0,
            clear_output: true,
        };
        let surface = request.surface().unwrap();
        let mut output = vec![255; surface.len_bytes];

        render_lattice_lab_8bit(&config, request, &mut output).unwrap();

        assert!(output.iter().any(|&byte| byte != 0));
        assert!(output.iter().filter(|&&byte| byte == 0).count() > 100);
    }

    #[test]
    fn lattice_lab_render_adapter_rejects_undersized_output() {
        let config = LatticeLabHostParams::default().to_engine_config();
        let request = LatticeLabRenderRequest::packed_argb8(16, 16, 0.0).unwrap();
        let mut output = vec![0; request.surface().unwrap().len_bytes - 1];

        assert_eq!(
            render_lattice_lab_8bit(&config, request, &mut output),
            Err(LatticeLabAdapterError::OutputBufferTooSmall {
                required: 1024,
                actual: 1023,
            })
        );
    }
}
