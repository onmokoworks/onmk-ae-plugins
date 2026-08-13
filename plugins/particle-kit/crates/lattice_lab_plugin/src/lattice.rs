#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::node_graph_core::{
    self, CoreGraphEdge, CoreGraphNode, NodeUiCatalog, NodeUiCatalogEntry, NodeUiConnectionSocket,
    NodeUiValueSocket, NodeUiValueType,
};
use crate::render_core::{
    blend_argb8_pixel, ArgbBlendMode, RenderFrame, RenderSurface, RenderSurfaceError,
};

pub(crate) const LATTICE_GRAPH_SCHEMA_VERSION: u32 = 1;
const MAX_LATTICE_GRID_RESOLUTION: u32 = 512;
const MAX_LATTICE_RENDER_POINTS: usize = 20_000;
const MAX_LATTICE_RENDER_EDGES: usize = 200_000;
const MAX_LATTICE_DISTANCE: f32 = 1_000_000.0;

#[derive(Clone, Debug)]
pub(crate) struct LatticeEngineConfig {
    pub(crate) points: LatticePointSourceConfig,
    pub(crate) noise: LatticeNoiseConfig,
    pub(crate) links: LatticeLinkConfig,
    pub(crate) mesh: LatticeMeshConfig,
    pub(crate) render: LatticeRenderConfig,
    pub(crate) seed: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct LatticeRuntimeInputs {
    pub(crate) frame: RenderFrame,
}

impl LatticeRuntimeInputs {
    pub(crate) fn new(
        output_width: usize,
        output_height: usize,
        time: f32,
    ) -> Result<Self, RenderSurfaceError> {
        Ok(Self {
            frame: RenderFrame::argb8(output_width, output_height, 0, 0, time, 0.0)?,
        })
    }

    pub(crate) fn argb8_with_row_bytes(
        output_width: usize,
        output_height: usize,
        row_bytes: usize,
        time: f32,
    ) -> Result<Self, RenderSurfaceError> {
        Ok(Self {
            frame: RenderFrame::argb8_with_row_bytes(
                output_width,
                output_height,
                row_bytes,
                0,
                0,
                time,
                0.0,
            )?,
        })
    }

    pub(crate) fn surface(&self) -> Result<RenderSurface, RenderSurfaceError> {
        Ok(self.frame.surface)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct LatticeRenderPlan {
    pub(crate) runtime: LatticeRuntimeInputs,
    pub(crate) blend_mode: LatticeBlendMode,
    pub(crate) points: Vec<LatticeRenderPoint>,
    pub(crate) links: Vec<LatticeRenderEdge>,
    pub(crate) mesh_edges: Vec<LatticeRenderEdge>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LatticeRenderPoint {
    pub(crate) position: [f32; 3],
    pub(crate) size: f32,
    pub(crate) color: [f32; 4],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LatticeRenderEdge {
    pub(crate) from: usize,
    pub(crate) to: usize,
    pub(crate) width: f32,
    pub(crate) color: [f32; 4],
}

#[derive(Clone, Debug)]
pub(crate) struct LatticePointSourceConfig {
    pub(crate) source: LatticePointSource,
    pub(crate) grid_resolution: [u32; 3],
    pub(crate) spacing: f32,
    pub(crate) max_points: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LatticePointSource {
    Grid,
}

#[derive(Clone, Debug)]
pub(crate) struct LatticeNoiseConfig {
    pub(crate) enabled: bool,
    pub(crate) amplitude: f32,
    pub(crate) frequency: f32,
    pub(crate) speed: f32,
    pub(crate) octaves: u8,
    pub(crate) axis_scale: [f32; 3],
}

#[derive(Clone, Debug)]
pub(crate) struct LatticeLinkConfig {
    pub(crate) enabled: bool,
    pub(crate) max_distance: f32,
    pub(crate) width: f32,
    pub(crate) opacity_falloff: f32,
    pub(crate) color: [f32; 4],
}

#[derive(Clone, Debug)]
pub(crate) struct LatticeMeshConfig {
    pub(crate) enabled: bool,
    pub(crate) max_edge: f32,
    pub(crate) opacity: f32,
    pub(crate) color: [f32; 4],
}

#[derive(Clone, Debug)]
pub(crate) struct LatticeRenderConfig {
    pub(crate) point_size: f32,
    pub(crate) point_color: [f32; 4],
    pub(crate) blend_mode: LatticeBlendMode,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub(crate) struct LatticeNodeId(pub(crate) u64);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct LatticeGraphDocument {
    pub(crate) schema_version: u32,
    pub(crate) output_node: LatticeNodeId,
    pub(crate) nodes: Vec<LatticeGraphNode>,
    pub(crate) edges: Vec<LatticeGraphEdge>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct LatticeGraphNode {
    pub(crate) id: LatticeNodeId,
    pub(crate) version: u32,
    #[serde(default)]
    pub(crate) label: String,
    #[serde(flatten)]
    pub(crate) kind: LatticeGraphNodeKind,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct LatticeGraphEdge {
    pub(crate) from: LatticeGraphSocket,
    pub(crate) to: LatticeGraphSocket,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct LatticeGraphSocket {
    pub(crate) node: LatticeNodeId,
    pub(crate) socket: String,
}

impl CoreGraphNode for LatticeGraphNode {
    type Id = LatticeNodeId;

    fn id(&self) -> Self::Id {
        self.id
    }
}

impl CoreGraphEdge for LatticeGraphEdge {
    type Id = LatticeNodeId;

    fn from_node(&self) -> Self::Id {
        self.from.node
    }

    fn to_node(&self) -> Self::Id {
        self.to.node
    }

    fn to_socket(&self) -> &str {
        &self.to.socket
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub(crate) enum LatticeGraphNodeKind {
    #[serde(rename = "lattice.output")]
    Output(LatticeOutputNode),
    #[serde(rename = "lattice.points.grid")]
    GridPoints(LatticeGridPointsNode),
    #[serde(rename = "lattice.noise")]
    Noise(LatticeNoiseNode),
    #[serde(rename = "lattice.links")]
    Links(LatticeLinksNode),
    #[serde(rename = "lattice.mesh")]
    Mesh(LatticeMeshNode),
    #[serde(rename = "lattice.render")]
    Render(LatticeRenderNode),
    #[serde(rename = "lattice.system")]
    System(LatticeSystemNode),
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct LatticeOutputNode {}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct LatticeGridPointsNode {
    pub(crate) resolution: [u32; 3],
    pub(crate) spacing: f32,
    pub(crate) max_points: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct LatticeNoiseNode {
    pub(crate) enabled: bool,
    pub(crate) amplitude: f32,
    pub(crate) frequency: f32,
    pub(crate) speed: f32,
    pub(crate) octaves: u8,
    pub(crate) axis_scale: [f32; 3],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct LatticeLinksNode {
    pub(crate) enabled: bool,
    pub(crate) max_distance: f32,
    pub(crate) width: f32,
    pub(crate) opacity_falloff: f32,
    pub(crate) color: [f32; 4],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct LatticeMeshNode {
    pub(crate) enabled: bool,
    pub(crate) max_edge: f32,
    pub(crate) opacity: f32,
    pub(crate) color: [f32; 4],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct LatticeRenderNode {
    pub(crate) point_size: f32,
    pub(crate) point_color: [f32; 4],
    pub(crate) blend_mode: LatticeGraphBlendMode,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct LatticeSystemNode {
    pub(crate) seed: u64,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LatticeGraphBlendMode {
    Normal,
    Add,
    Screen,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LatticeBlendMode {
    Normal,
    Add,
    Screen,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum LatticeGraphCompileError {
    UnsupportedSchemaVersion(u32),
    DuplicateNodeId(LatticeNodeId),
    UnknownNode(LatticeNodeId),
    MissingOutputNode(LatticeNodeId),
    WrongNodeKind {
        node: LatticeNodeId,
        socket: &'static str,
        expected: &'static str,
    },
}

impl LatticeEngineConfig {
    pub(crate) fn default_v1() -> Self {
        Self {
            points: LatticePointSourceConfig {
                source: LatticePointSource::Grid,
                grid_resolution: [10, 10, 1],
                spacing: 50.0,
                max_points: 5_000,
            },
            noise: LatticeNoiseConfig {
                enabled: false,
                amplitude: 50.0,
                frequency: 0.01,
                speed: 1.0,
                octaves: 2,
                axis_scale: [1.0, 1.0, 1.0],
            },
            links: LatticeLinkConfig {
                enabled: true,
                max_distance: 120.0,
                width: 1.0,
                opacity_falloff: 0.8,
                color: [1.0, 1.0, 1.0, 1.0],
            },
            mesh: LatticeMeshConfig {
                enabled: false,
                max_edge: 150.0,
                opacity: 0.3,
                color: [100.0 / 255.0, 150.0 / 255.0, 1.0, 1.0],
            },
            render: LatticeRenderConfig {
                point_size: 4.0,
                point_color: [1.0, 1.0, 1.0, 1.0],
                blend_mode: LatticeBlendMode::Normal,
            },
            seed: 12_345,
        }
    }

    pub(crate) fn normalized_for_render(mut self) -> Self {
        let defaults = Self::default_v1();

        self.points.grid_resolution = [
            self.points.grid_resolution[0].clamp(1, MAX_LATTICE_GRID_RESOLUTION),
            self.points.grid_resolution[1].clamp(1, MAX_LATTICE_GRID_RESOLUTION),
            self.points.grid_resolution[2].clamp(1, MAX_LATTICE_GRID_RESOLUTION),
        ];
        self.points.spacing = finite_clamp(
            self.points.spacing,
            defaults.points.spacing,
            0.01,
            MAX_LATTICE_DISTANCE,
        );
        self.points.max_points = self
            .points
            .max_points
            .clamp(1, MAX_LATTICE_RENDER_POINTS as u32);

        self.noise.amplitude = finite_clamp(
            self.noise.amplitude,
            defaults.noise.amplitude,
            0.0,
            MAX_LATTICE_DISTANCE,
        );
        self.noise.frequency = finite_clamp(
            self.noise.frequency,
            defaults.noise.frequency,
            0.0,
            10_000.0,
        );
        self.noise.speed = finite_or(self.noise.speed, defaults.noise.speed);
        self.noise.octaves = self.noise.octaves.clamp(1, 8);
        self.noise.axis_scale = finite_vec3_array(self.noise.axis_scale, defaults.noise.axis_scale);

        self.links.max_distance = finite_clamp(
            self.links.max_distance,
            defaults.links.max_distance,
            0.0,
            MAX_LATTICE_DISTANCE,
        );
        self.links.width = finite_clamp(self.links.width, defaults.links.width, 0.0, 1_000.0);
        self.links.opacity_falloff = finite_clamp(
            self.links.opacity_falloff,
            defaults.links.opacity_falloff,
            0.0,
            8.0,
        );
        self.links.color = finite_color(self.links.color, defaults.links.color);

        self.mesh.max_edge = finite_clamp(
            self.mesh.max_edge,
            defaults.mesh.max_edge,
            0.0,
            MAX_LATTICE_DISTANCE,
        );
        self.mesh.opacity = finite_clamp(self.mesh.opacity, defaults.mesh.opacity, 0.0, 1.0);
        self.mesh.color = finite_color(self.mesh.color, defaults.mesh.color);

        self.render.point_size = finite_clamp(
            self.render.point_size,
            defaults.render.point_size,
            0.0,
            1_000.0,
        );
        self.render.point_color =
            finite_color(self.render.point_color, defaults.render.point_color);

        self
    }
}

impl LatticeGraphDocument {
    pub(crate) fn simple_grid_network() -> Self {
        Self::from_engine_config(&LatticeEngineConfig::default_v1())
    }

    pub(crate) fn from_engine_config(config: &LatticeEngineConfig) -> Self {
        let config = config.clone().normalized_for_render();
        let output = LatticeNodeId(1);
        let points = LatticeNodeId(2);
        let noise = LatticeNodeId(3);
        let links = LatticeNodeId(4);
        let mesh = LatticeNodeId(5);
        let render = LatticeNodeId(6);
        let system = LatticeNodeId(7);

        Self {
            schema_version: LATTICE_GRAPH_SCHEMA_VERSION,
            output_node: output,
            nodes: vec![
                lattice_node(
                    output,
                    "Output",
                    LatticeGraphNodeKind::Output(LatticeOutputNode::default()),
                ),
                lattice_node(
                    points,
                    "Grid Points",
                    LatticeGraphNodeKind::GridPoints(LatticeGridPointsNode {
                        resolution: config.points.grid_resolution,
                        spacing: config.points.spacing,
                        max_points: config.points.max_points,
                    }),
                ),
                lattice_node(
                    noise,
                    "Noise",
                    LatticeGraphNodeKind::Noise(LatticeNoiseNode {
                        enabled: config.noise.enabled,
                        amplitude: config.noise.amplitude,
                        frequency: config.noise.frequency,
                        speed: config.noise.speed,
                        octaves: config.noise.octaves,
                        axis_scale: config.noise.axis_scale,
                    }),
                ),
                lattice_node(
                    links,
                    "Links",
                    LatticeGraphNodeKind::Links(LatticeLinksNode {
                        enabled: config.links.enabled,
                        max_distance: config.links.max_distance,
                        width: config.links.width,
                        opacity_falloff: config.links.opacity_falloff,
                        color: config.links.color,
                    }),
                ),
                lattice_node(
                    mesh,
                    "Mesh",
                    LatticeGraphNodeKind::Mesh(LatticeMeshNode {
                        enabled: config.mesh.enabled,
                        max_edge: config.mesh.max_edge,
                        opacity: config.mesh.opacity,
                        color: config.mesh.color,
                    }),
                ),
                lattice_node(
                    render,
                    "Render",
                    LatticeGraphNodeKind::Render(LatticeRenderNode {
                        point_size: config.render.point_size,
                        point_color: config.render.point_color,
                        blend_mode: lattice_blend_mode_to_graph(config.render.blend_mode),
                    }),
                ),
                lattice_node(
                    system,
                    "System",
                    LatticeGraphNodeKind::System(LatticeSystemNode { seed: config.seed }),
                ),
            ],
            edges: vec![
                lattice_edge(points, "out", output, "points"),
                lattice_edge(noise, "out", output, "noise"),
                lattice_edge(links, "out", output, "links"),
                lattice_edge(mesh, "out", output, "mesh"),
                lattice_edge(render, "out", output, "render"),
                lattice_edge(system, "out", output, "system"),
            ],
        }
    }
}

pub(crate) fn lattice_node_ui_catalog() -> NodeUiCatalog {
    NodeUiCatalog::new(
        "latticelab",
        "Lattice Lab",
        LATTICE_GRAPH_SCHEMA_VERSION,
        vec![
            lattice_catalog_node(
                "lattice.output",
                "Output",
                vec![
                    lattice_connection_socket("points", "Points"),
                    lattice_connection_socket("noise", "Noise"),
                    lattice_connection_socket("links", "Links"),
                    lattice_connection_socket("mesh", "Mesh"),
                    lattice_connection_socket("render", "Render"),
                    lattice_connection_socket("system", "System"),
                ],
                Vec::new(),
                Vec::new(),
            ),
            lattice_catalog_node(
                "lattice.points.grid",
                "Grid Points",
                Vec::new(),
                vec![lattice_connection_socket("out", "Out")],
                vec![
                    lattice_value_socket("resolution", "Resolution", NodeUiValueType::Vector3),
                    lattice_value_socket("spacing", "Spacing", NodeUiValueType::Float),
                    lattice_value_socket("max_points", "Max Points", NodeUiValueType::Integer),
                ],
            ),
            lattice_catalog_node(
                "lattice.noise",
                "Noise",
                Vec::new(),
                vec![lattice_connection_socket("out", "Out")],
                vec![
                    lattice_value_socket("enabled", "Enabled", NodeUiValueType::Boolean),
                    lattice_value_socket("amplitude", "Amplitude", NodeUiValueType::Float),
                    lattice_value_socket("frequency", "Frequency", NodeUiValueType::Float),
                    lattice_value_socket("speed", "Speed", NodeUiValueType::Float),
                    lattice_value_socket("octaves", "Octaves", NodeUiValueType::Integer),
                    lattice_value_socket("axis_scale", "Axis Scale", NodeUiValueType::Vector3),
                ],
            ),
            lattice_catalog_node(
                "lattice.links",
                "Links",
                Vec::new(),
                vec![lattice_connection_socket("out", "Out")],
                vec![
                    lattice_value_socket("enabled", "Enabled", NodeUiValueType::Boolean),
                    lattice_value_socket("max_distance", "Max Distance", NodeUiValueType::Float),
                    lattice_value_socket("width", "Width", NodeUiValueType::Float),
                    lattice_value_socket(
                        "opacity_falloff",
                        "Opacity Falloff",
                        NodeUiValueType::Float,
                    ),
                    lattice_value_socket("color", "Color", NodeUiValueType::Color),
                ],
            ),
            lattice_catalog_node(
                "lattice.mesh",
                "Mesh",
                Vec::new(),
                vec![lattice_connection_socket("out", "Out")],
                vec![
                    lattice_value_socket("enabled", "Enabled", NodeUiValueType::Boolean),
                    lattice_value_socket("max_edge", "Max Edge", NodeUiValueType::Float),
                    lattice_value_socket("opacity", "Opacity", NodeUiValueType::Float),
                    lattice_value_socket("color", "Color", NodeUiValueType::Color),
                ],
            ),
            lattice_catalog_node(
                "lattice.render",
                "Render",
                Vec::new(),
                vec![lattice_connection_socket("out", "Out")],
                vec![
                    lattice_value_socket("point_size", "Point Size", NodeUiValueType::Float),
                    lattice_value_socket("point_color", "Point Color", NodeUiValueType::Color),
                    NodeUiValueSocket::enumeration(
                        "blend_mode",
                        "Blend Mode",
                        &[("normal", "Normal"), ("add", "Add"), ("screen", "Screen")],
                    ),
                ],
            ),
            lattice_catalog_node(
                "lattice.system",
                "System",
                Vec::new(),
                vec![lattice_connection_socket("out", "Out")],
                vec![lattice_value_socket(
                    "seed",
                    "Seed",
                    NodeUiValueType::Integer,
                )],
            ),
        ],
    )
}

fn lattice_catalog_node(
    node_type: &str,
    label: &str,
    input_sockets: Vec<NodeUiConnectionSocket>,
    output_sockets: Vec<NodeUiConnectionSocket>,
    value_sockets: Vec<NodeUiValueSocket>,
) -> NodeUiCatalogEntry {
    NodeUiCatalogEntry::new(
        node_type,
        label,
        input_sockets,
        output_sockets,
        value_sockets,
    )
}

fn lattice_connection_socket(socket: &str, label: &str) -> NodeUiConnectionSocket {
    NodeUiConnectionSocket::new(socket, label)
}

fn lattice_value_socket(
    socket: &str,
    label: &str,
    value_type: NodeUiValueType,
) -> NodeUiValueSocket {
    NodeUiValueSocket::value(socket, label, value_type)
}

pub(crate) fn compile_lattice_graph(
    document: &LatticeGraphDocument,
) -> Result<LatticeEngineConfig, LatticeGraphCompileError> {
    if document.schema_version != LATTICE_GRAPH_SCHEMA_VERSION {
        return Err(LatticeGraphCompileError::UnsupportedSchemaVersion(
            document.schema_version,
        ));
    }

    let nodes = lattice_node_map(document)?;
    let output = nodes.get(&document.output_node).copied().ok_or(
        LatticeGraphCompileError::MissingOutputNode(document.output_node),
    )?;
    if !matches!(output.kind, LatticeGraphNodeKind::Output(_)) {
        return Err(LatticeGraphCompileError::WrongNodeKind {
            node: output.id,
            socket: "output",
            expected: "lattice.output",
        });
    }

    let mut config = LatticeEngineConfig::default_v1();

    if let Some(node) = lattice_output_input(document, &nodes, output.id, "points")? {
        match &node.kind {
            LatticeGraphNodeKind::GridPoints(data) => apply_grid_points(&mut config, data),
            _ => return lattice_wrong(node.id, "points", "lattice.points.grid"),
        }
    }
    if let Some(node) = lattice_output_input(document, &nodes, output.id, "noise")? {
        match &node.kind {
            LatticeGraphNodeKind::Noise(data) => apply_noise(&mut config, data),
            _ => return lattice_wrong(node.id, "noise", "lattice.noise"),
        }
    }
    if let Some(node) = lattice_output_input(document, &nodes, output.id, "links")? {
        match &node.kind {
            LatticeGraphNodeKind::Links(data) => apply_links(&mut config, data),
            _ => return lattice_wrong(node.id, "links", "lattice.links"),
        }
    }
    if let Some(node) = lattice_output_input(document, &nodes, output.id, "mesh")? {
        match &node.kind {
            LatticeGraphNodeKind::Mesh(data) => apply_mesh(&mut config, data),
            _ => return lattice_wrong(node.id, "mesh", "lattice.mesh"),
        }
    }
    if let Some(node) = lattice_output_input(document, &nodes, output.id, "render")? {
        match &node.kind {
            LatticeGraphNodeKind::Render(data) => apply_lattice_render(&mut config, data),
            _ => return lattice_wrong(node.id, "render", "lattice.render"),
        }
    }
    if let Some(node) = lattice_output_input(document, &nodes, output.id, "system")? {
        match &node.kind {
            LatticeGraphNodeKind::System(data) => config.seed = data.seed,
            _ => return lattice_wrong(node.id, "system", "lattice.system"),
        }
    }

    Ok(config)
}

pub(crate) fn build_lattice_render_plan(
    config: &LatticeEngineConfig,
    runtime: LatticeRuntimeInputs,
) -> LatticeRenderPlan {
    let config = config.clone().normalized_for_render();
    let mut runtime = runtime;
    runtime.frame.time = finite_or(runtime.frame.time, 0.0);
    let points = build_lattice_points(&config, &runtime);
    let links = if config.links.enabled {
        build_lattice_edges(
            &points,
            config.links.max_distance,
            config.links.width,
            config.links.color,
            config.links.opacity_falloff,
            MAX_LATTICE_RENDER_EDGES,
        )
    } else {
        Vec::new()
    };
    let mesh_edges = if config.mesh.enabled {
        let mut color = config.mesh.color;
        color[3] *= config.mesh.opacity;
        build_lattice_edges(
            &points,
            config.mesh.max_edge,
            1.0,
            color,
            0.0,
            MAX_LATTICE_RENDER_EDGES,
        )
    } else {
        Vec::new()
    };

    LatticeRenderPlan {
        runtime,
        blend_mode: config.render.blend_mode,
        points,
        links,
        mesh_edges,
    }
}

pub(crate) fn render_lattice_plan_8bit(plan: &LatticeRenderPlan, output: &mut [u8]) {
    let Ok(surface) = plan.runtime.surface() else {
        return;
    };
    if surface.is_empty() || !surface.fits_buffer(output) {
        return;
    }

    let width = surface.width;
    let height = surface.height;
    let row_bytes = surface.row_bytes;
    let blend_mode = lattice_blend_mode_to_renderer(plan.blend_mode);
    for edge in &plan.mesh_edges {
        draw_lattice_edge(plan, edge, output, row_bytes, width, height, blend_mode);
    }
    for edge in &plan.links {
        draw_lattice_edge(plan, edge, output, row_bytes, width, height, blend_mode);
    }
    for point in &plan.points {
        draw_lattice_point(point, output, row_bytes, width, height, blend_mode);
    }
}

fn lattice_node(id: LatticeNodeId, label: &str, kind: LatticeGraphNodeKind) -> LatticeGraphNode {
    LatticeGraphNode {
        id,
        version: 1,
        label: label.to_string(),
        kind,
    }
}

fn lattice_edge(
    from: LatticeNodeId,
    from_socket: &str,
    to: LatticeNodeId,
    to_socket: &str,
) -> LatticeGraphEdge {
    LatticeGraphEdge {
        from: LatticeGraphSocket {
            node: from,
            socket: from_socket.to_string(),
        },
        to: LatticeGraphSocket {
            node: to,
            socket: to_socket.to_string(),
        },
    }
}

fn lattice_node_map(
    document: &LatticeGraphDocument,
) -> Result<HashMap<LatticeNodeId, &LatticeGraphNode>, LatticeGraphCompileError> {
    node_graph_core::node_map(&document.nodes, LatticeGraphCompileError::DuplicateNodeId)
}

fn lattice_output_input<'a>(
    document: &'a LatticeGraphDocument,
    nodes: &HashMap<LatticeNodeId, &'a LatticeGraphNode>,
    output: LatticeNodeId,
    socket: &'static str,
) -> Result<Option<&'a LatticeGraphNode>, LatticeGraphCompileError> {
    node_graph_core::output_input(
        &document.edges,
        nodes,
        output,
        socket,
        LatticeGraphCompileError::UnknownNode,
    )
}

fn lattice_wrong<T>(
    node: LatticeNodeId,
    socket: &'static str,
    expected: &'static str,
) -> Result<T, LatticeGraphCompileError> {
    Err(LatticeGraphCompileError::WrongNodeKind {
        node,
        socket,
        expected,
    })
}

fn apply_grid_points(config: &mut LatticeEngineConfig, data: &LatticeGridPointsNode) {
    config.points.source = LatticePointSource::Grid;
    config.points.grid_resolution = [
        data.resolution[0].max(1),
        data.resolution[1].max(1),
        data.resolution[2].max(1),
    ];
    config.points.spacing = data.spacing.max(0.01);
    config.points.max_points = data.max_points.max(1);
}

fn apply_noise(config: &mut LatticeEngineConfig, data: &LatticeNoiseNode) {
    config.noise.enabled = data.enabled;
    config.noise.amplitude = data.amplitude;
    config.noise.frequency = data.frequency.max(0.0);
    config.noise.speed = data.speed;
    config.noise.octaves = data.octaves.max(1);
    config.noise.axis_scale = data.axis_scale;
}

fn apply_links(config: &mut LatticeEngineConfig, data: &LatticeLinksNode) {
    config.links.enabled = data.enabled;
    config.links.max_distance = data.max_distance.max(0.0);
    config.links.width = data.width.max(0.0);
    config.links.opacity_falloff = data.opacity_falloff.clamp(0.0, 1.0);
    config.links.color = data.color;
}

fn apply_mesh(config: &mut LatticeEngineConfig, data: &LatticeMeshNode) {
    config.mesh.enabled = data.enabled;
    config.mesh.max_edge = data.max_edge.max(0.0);
    config.mesh.opacity = data.opacity.clamp(0.0, 1.0);
    config.mesh.color = data.color;
}

fn apply_lattice_render(config: &mut LatticeEngineConfig, data: &LatticeRenderNode) {
    config.render.point_size = data.point_size.max(0.0);
    config.render.point_color = data.point_color;
    config.render.blend_mode = match data.blend_mode {
        LatticeGraphBlendMode::Normal => LatticeBlendMode::Normal,
        LatticeGraphBlendMode::Add => LatticeBlendMode::Add,
        LatticeGraphBlendMode::Screen => LatticeBlendMode::Screen,
    };
}

fn lattice_blend_mode_to_graph(mode: LatticeBlendMode) -> LatticeGraphBlendMode {
    match mode {
        LatticeBlendMode::Normal => LatticeGraphBlendMode::Normal,
        LatticeBlendMode::Add => LatticeGraphBlendMode::Add,
        LatticeBlendMode::Screen => LatticeGraphBlendMode::Screen,
    }
}

fn build_lattice_points(
    config: &LatticeEngineConfig,
    runtime: &LatticeRuntimeInputs,
) -> Vec<LatticeRenderPoint> {
    let [nx, ny, nz] = config.points.grid_resolution;
    let point_count = (nx as usize)
        .saturating_mul(ny as usize)
        .saturating_mul(nz as usize)
        .min(config.points.max_points as usize)
        .min(MAX_LATTICE_RENDER_POINTS);
    let mut points = Vec::with_capacity(point_count);
    let center_x = runtime.frame.surface.width as f32 * 0.5;
    let center_y = runtime.frame.surface.height as f32 * 0.5;
    let half_x = (nx.saturating_sub(1)) as f32 * 0.5;
    let half_y = (ny.saturating_sub(1)) as f32 * 0.5;
    let half_z = (nz.saturating_sub(1)) as f32 * 0.5;

    'z: for z in 0..nz {
        for y in 0..ny {
            for x in 0..nx {
                if points.len() >= point_count {
                    break 'z;
                }
                let point_index = points.len() as u32;
                let mut position = [
                    center_x + (x as f32 - half_x) * config.points.spacing,
                    center_y + (y as f32 - half_y) * config.points.spacing,
                    (z as f32 - half_z) * config.points.spacing,
                ];
                if config.noise.enabled {
                    let offset = lattice_noise_offset(
                        &config.noise,
                        config.seed,
                        point_index,
                        runtime.frame.time,
                    );
                    for axis in 0..3 {
                        position[axis] += offset[axis];
                    }
                }
                points.push(LatticeRenderPoint {
                    position,
                    size: config.render.point_size,
                    color: config.render.point_color,
                });
            }
        }
    }

    points
}

fn build_lattice_edges(
    points: &[LatticeRenderPoint],
    max_distance: f32,
    width: f32,
    color: [f32; 4],
    opacity_falloff: f32,
    max_edges: usize,
) -> Vec<LatticeRenderEdge> {
    if max_distance <= 0.0 || width <= 0.0 || color[3] <= 0.0 {
        return Vec::new();
    }

    let max_distance_sq = max_distance * max_distance;
    let mut edges = Vec::new();
    'outer: for from in 0..points.len() {
        for to in from + 1..points.len() {
            if edges.len() >= max_edges {
                break 'outer;
            }
            let distance_sq = distance_squared(points[from].position, points[to].position);
            if distance_sq <= max_distance_sq {
                let distance = distance_sq.sqrt();
                let mut edge_color = color;
                let normalized = (1.0 - distance / max_distance).clamp(0.0, 1.0);
                if opacity_falloff > 0.0 {
                    edge_color[3] *= normalized.powf(opacity_falloff);
                }
                edges.push(LatticeRenderEdge {
                    from,
                    to,
                    width,
                    color: edge_color,
                });
            }
        }
    }

    edges
}

fn draw_lattice_point(
    point: &LatticeRenderPoint,
    output: &mut [u8],
    row_bytes: usize,
    width: usize,
    height: usize,
    blend_mode: ArgbBlendMode,
) {
    if point.size <= 0.0 || point.color[3] <= 0.0 {
        return;
    }

    let radius = (point.size * 0.5).max(0.5);
    let draw_radius = radius + 1.0;
    let min_x = ((point.position[0] - draw_radius).floor() as isize).max(0) as usize;
    let max_x = ((point.position[0] + draw_radius).ceil() as isize)
        .min(width.saturating_sub(1) as isize) as usize;
    let min_y = ((point.position[1] - draw_radius).floor() as isize).max(0) as usize;
    let max_y = ((point.position[1] + draw_radius).ceil() as isize)
        .min(height.saturating_sub(1) as isize) as usize;

    if min_x > max_x || min_y > max_y {
        return;
    }

    for y in min_y..=max_y {
        let py = y as f32 + 0.5;
        for x in min_x..=max_x {
            let px = x as f32 + 0.5;
            let dx = px - point.position[0];
            let dy = py - point.position[1];
            let distance = (dx * dx + dy * dy).sqrt();
            let coverage = (radius + 0.5 - distance).clamp(0.0, 1.0);
            if coverage > 0.0 {
                blend_argb8_pixel(output, row_bytes, x, y, point.color, coverage, blend_mode);
            }
        }
    }
}

fn draw_lattice_edge(
    plan: &LatticeRenderPlan,
    edge: &LatticeRenderEdge,
    output: &mut [u8],
    row_bytes: usize,
    width: usize,
    height: usize,
    blend_mode: ArgbBlendMode,
) {
    if edge.width <= 0.0 || edge.color[3] <= 0.0 {
        return;
    }
    let (Some(from), Some(to)) = (plan.points.get(edge.from), plan.points.get(edge.to)) else {
        return;
    };

    let ax = from.position[0];
    let ay = from.position[1];
    let bx = to.position[0];
    let by = to.position[1];
    let radius = (edge.width * 0.5).max(0.5);
    let draw_radius = radius + 1.0;
    let min_x = ((ax.min(bx) - draw_radius).floor() as isize).max(0) as usize;
    let max_x =
        ((ax.max(bx) + draw_radius).ceil() as isize).min(width.saturating_sub(1) as isize) as usize;
    let min_y = ((ay.min(by) - draw_radius).floor() as isize).max(0) as usize;
    let max_y = ((ay.max(by) + draw_radius).ceil() as isize).min(height.saturating_sub(1) as isize)
        as usize;

    if min_x > max_x || min_y > max_y {
        return;
    }

    for y in min_y..=max_y {
        let py = y as f32 + 0.5;
        for x in min_x..=max_x {
            let px = x as f32 + 0.5;
            let distance = distance_to_segment(px, py, ax, ay, bx, by);
            let coverage = (radius + 0.5 - distance).clamp(0.0, 1.0);
            if coverage > 0.0 {
                blend_argb8_pixel(output, row_bytes, x, y, edge.color, coverage, blend_mode);
            }
        }
    }
}

fn distance_to_segment(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let abx = bx - ax;
    let aby = by - ay;
    let len_sq = abx * abx + aby * aby;
    if len_sq <= f32::EPSILON {
        let dx = px - ax;
        let dy = py - ay;
        return (dx * dx + dy * dy).sqrt();
    }
    let t = (((px - ax) * abx + (py - ay) * aby) / len_sq).clamp(0.0, 1.0);
    let cx = ax + abx * t;
    let cy = ay + aby * t;
    let dx = px - cx;
    let dy = py - cy;
    (dx * dx + dy * dy).sqrt()
}

fn lattice_blend_mode_to_renderer(mode: LatticeBlendMode) -> ArgbBlendMode {
    match mode {
        LatticeBlendMode::Normal => ArgbBlendMode::Normal,
        LatticeBlendMode::Add => ArgbBlendMode::Add,
        LatticeBlendMode::Screen => ArgbBlendMode::Screen,
    }
}

fn lattice_noise_offset(
    noise: &LatticeNoiseConfig,
    seed: u64,
    point_index: u32,
    time: f32,
) -> [f32; 3] {
    let mut offset = [0.0; 3];
    let frequency = noise.frequency.max(0.0001);
    let octaves = noise.octaves.clamp(1, 8);
    for octave in 0..octaves {
        let octave_scale = 1.0 / (octave as f32 + 1.0);
        for axis in 0..3u32 {
            let phase = hash_signed(seed, point_index, octave as u32, axis) * std::f32::consts::TAU;
            let wave = (phase + time * noise.speed * frequency * (octave as f32 + 1.0)).sin();
            offset[axis as usize] +=
                wave * noise.amplitude * octave_scale * noise.axis_scale[axis as usize];
        }
    }
    offset
}

fn distance_squared(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    dx * dx + dy * dy + dz * dz
}

fn hash_signed(seed: u64, point_index: u32, octave: u32, axis: u32) -> f32 {
    let mut value = (seed as u32)
        ^ ((seed >> 32) as u32)
        ^ point_index.wrapping_mul(0x9E37_79B9)
        ^ octave.wrapping_mul(0x85EB_CA6B)
        ^ axis.wrapping_mul(0xC2B2_AE35);
    value ^= value >> 16;
    value = value.wrapping_mul(0x7FEB_352D);
    value ^= value >> 15;
    value = value.wrapping_mul(0x846C_A68B);
    value ^= value >> 16;
    (value as f32 / u32::MAX as f32) * 2.0 - 1.0
}

fn finite_or(value: f32, default: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        default
    }
}

fn finite_clamp(value: f32, default: f32, min: f32, max: f32) -> f32 {
    finite_or(value, default).clamp(min, max)
}

fn finite_vec3_array(value: [f32; 3], default: [f32; 3]) -> [f32; 3] {
    [
        finite_or(value[0], default[0]),
        finite_or(value[1], default[1]),
        finite_or(value[2], default[2]),
    ]
}

fn finite_color(value: [f32; 4], default: [f32; 4]) -> [f32; 4] {
    [
        finite_clamp(value[0], default[0], 0.0, 1.0),
        finite_clamp(value[1], default[1], 0.0, 1.0),
        finite_clamp(value[2], default[2], 0.0, 1.0),
        finite_clamp(value[3], default[3], 0.0, 1.0),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < 0.0001, "{actual} != {expected}");
    }

    fn assert_color_close(actual: [f32; 4], expected: [f32; 4]) {
        for (&actual, &expected) in actual.iter().zip(expected.iter()) {
            assert_close(actual, expected);
        }
    }

    fn catalog_entry<'a>(catalog: &'a NodeUiCatalog, node_type: &str) -> &'a NodeUiCatalogEntry {
        catalog
            .nodes
            .iter()
            .find(|entry| entry.node_type == node_type)
            .unwrap_or_else(|| panic!("missing catalog node type {node_type}"))
    }

    #[test]
    fn simple_grid_network_compiles_to_lattice_default() {
        let document = LatticeGraphDocument::simple_grid_network();
        let config = compile_lattice_graph(&document).unwrap();
        let default = LatticeEngineConfig::default_v1();

        assert_eq!(config.points.source, LatticePointSource::Grid);
        assert_eq!(
            config.points.grid_resolution,
            default.points.grid_resolution
        );
        assert_close(config.points.spacing, default.points.spacing);
        assert_eq!(config.points.max_points, default.points.max_points);
        assert_eq!(config.noise.enabled, default.noise.enabled);
        assert_close(config.links.max_distance, default.links.max_distance);
        assert_eq!(config.mesh.enabled, default.mesh.enabled);
        assert_color_close(config.render.point_color, default.render.point_color);
        assert_eq!(config.render.blend_mode, LatticeBlendMode::Normal);
        assert_eq!(config.seed, default.seed);
    }

    #[test]
    fn lattice_graph_document_can_seed_from_engine_config() {
        let mut config = LatticeEngineConfig::default_v1();
        config.points.grid_resolution = [8, 5, 2];
        config.points.spacing = 72.0;
        config.noise.enabled = true;
        config.noise.amplitude = 19.0;
        config.links.max_distance = 88.0;
        config.links.color = [0.2, 0.3, 0.9, 0.7];
        config.mesh.enabled = true;
        config.render.point_size = 6.5;
        config.render.blend_mode = LatticeBlendMode::Screen;
        config.seed = 42;

        let document = LatticeGraphDocument::from_engine_config(&config);
        let compiled = compile_lattice_graph(&document).unwrap();

        assert_eq!(compiled.points.grid_resolution, [8, 5, 2]);
        assert_close(compiled.points.spacing, 72.0);
        assert_eq!(compiled.noise.enabled, true);
        assert_close(compiled.noise.amplitude, 19.0);
        assert_close(compiled.links.max_distance, 88.0);
        assert_color_close(compiled.links.color, [0.2, 0.3, 0.9, 0.7]);
        assert_eq!(compiled.mesh.enabled, true);
        assert_close(compiled.render.point_size, 6.5);
        assert_eq!(compiled.render.blend_mode, LatticeBlendMode::Screen);
        assert_eq!(compiled.seed, 42);
    }

    #[test]
    fn lattice_graph_json_uses_lattice_namespaces() {
        let document = LatticeGraphDocument::simple_grid_network();
        let json = serde_json::to_string(&document).unwrap();

        assert!(json.contains("lattice.points.grid"));
        assert!(json.contains("lattice.links"));
        assert!(!json.contains("particle."));

        let decoded: LatticeGraphDocument = serde_json::from_str(&json).unwrap();
        compile_lattice_graph(&decoded).unwrap();
    }

    #[test]
    fn lattice_node_ui_catalog_describes_lattice_graph_surface() {
        let catalog = lattice_node_ui_catalog();

        assert_eq!(
            catalog.catalog_version,
            node_graph_core::NODE_UI_CATALOG_VERSION
        );
        assert_eq!(catalog.graph_schema_version, LATTICE_GRAPH_SCHEMA_VERSION);
        assert_eq!(catalog.product_id, "latticelab");
        assert_eq!(catalog.nodes.len(), 7);

        let output = catalog_entry(&catalog, "lattice.output");
        assert!(output
            .input_sockets
            .iter()
            .any(|socket| socket.socket == "points"));
        assert!(output
            .input_sockets
            .iter()
            .any(|socket| socket.socket == "system"));

        let points = catalog_entry(&catalog, "lattice.points.grid");
        assert!(points
            .value_sockets
            .iter()
            .any(|socket| socket.socket == "resolution"
                && socket.value_type == NodeUiValueType::Vector3));
        assert!(points
            .output_sockets
            .iter()
            .any(|socket| socket.socket == "out"));

        let render = catalog_entry(&catalog, "lattice.render");
        let blend = render
            .value_sockets
            .iter()
            .find(|socket| socket.socket == "blend_mode")
            .unwrap();
        assert_eq!(blend.value_type, NodeUiValueType::Enum);
        assert!(blend
            .enum_options
            .iter()
            .any(|option| option.value == "screen"));

        let json = serde_json::to_string(&catalog).unwrap();
        assert!(json.contains("\"lattice.render\""));
        assert!(json.contains("\"catalog_version\":1"));
    }

    #[test]
    fn lattice_compiler_preserves_node_edits() {
        let mut document = LatticeGraphDocument::simple_grid_network();
        for node in &mut document.nodes {
            match &mut node.kind {
                LatticeGraphNodeKind::GridPoints(data) => {
                    data.resolution = [4, 5, 6];
                    data.spacing = 24.0;
                    data.max_points = 222;
                }
                LatticeGraphNodeKind::Noise(data) => {
                    data.enabled = true;
                    data.amplitude = 12.0;
                    data.frequency = 0.5;
                    data.speed = 3.0;
                    data.octaves = 4;
                    data.axis_scale = [1.0, 2.0, 3.0];
                }
                LatticeGraphNodeKind::Links(data) => {
                    data.max_distance = 88.0;
                    data.width = 2.5;
                    data.opacity_falloff = 0.25;
                    data.color = [0.1, 0.2, 0.3, 0.4];
                }
                LatticeGraphNodeKind::Mesh(data) => {
                    data.enabled = true;
                    data.max_edge = 99.0;
                    data.opacity = 0.75;
                    data.color = [0.9, 0.8, 0.7, 0.6];
                }
                LatticeGraphNodeKind::Render(data) => {
                    data.point_size = 7.0;
                    data.point_color = [0.25, 0.5, 0.75, 1.0];
                    data.blend_mode = LatticeGraphBlendMode::Add;
                }
                LatticeGraphNodeKind::System(data) => data.seed = 9_999,
                LatticeGraphNodeKind::Output(_) => {}
            }
        }

        let config = compile_lattice_graph(&document).unwrap();

        assert_eq!(config.points.grid_resolution, [4, 5, 6]);
        assert_close(config.points.spacing, 24.0);
        assert_eq!(config.points.max_points, 222);
        assert!(config.noise.enabled);
        assert_close(config.noise.frequency, 0.5);
        assert_eq!(config.noise.octaves, 4);
        assert_close(config.links.max_distance, 88.0);
        assert!(config.mesh.enabled);
        assert_close(config.mesh.opacity, 0.75);
        assert_close(config.render.point_size, 7.0);
        assert_eq!(config.render.blend_mode, LatticeBlendMode::Add);
        assert_eq!(config.seed, 9_999);
    }

    #[test]
    fn lattice_render_plan_generates_points_and_edges_from_compiled_config() {
        let document = LatticeGraphDocument::simple_grid_network();
        let config = compile_lattice_graph(&document).unwrap();
        let plan =
            build_lattice_render_plan(&config, LatticeRuntimeInputs::new(1_000, 800, 0.0).unwrap());

        assert_eq!(plan.points.len(), 100);
        assert!(!plan.links.is_empty());
        assert!(plan.mesh_edges.is_empty());
        assert_eq!(plan.blend_mode, LatticeBlendMode::Normal);
        assert_close(plan.points[0].position[0], 275.0);
        assert_close(plan.points[0].position[1], 175.0);
        assert_close(plan.points[0].size, config.render.point_size);
        assert_color_close(plan.points[0].color, config.render.point_color);
        for edge in &plan.links {
            assert!(edge.from < plan.points.len());
            assert!(edge.to < plan.points.len());
            assert!(edge.from < edge.to);
            assert!(edge.color[3] >= 0.0 && edge.color[3] <= config.links.color[3]);
        }
    }

    #[test]
    fn lattice_render_plan_caps_points_and_sanitizes_config() {
        let mut config = LatticeEngineConfig::default_v1();
        config.points.grid_resolution = [1_000, 1_000, 2];
        config.points.max_points = 12;
        config.points.spacing = f32::NAN;
        config.render.point_size = f32::INFINITY;
        config.render.point_color = [2.0, -1.0, 0.5, f32::NAN];
        config.links.enabled = false;

        let plan =
            build_lattice_render_plan(&config, LatticeRuntimeInputs::new(0, 0, f32::NAN).unwrap());

        assert_eq!(plan.points.len(), 12);
        assert!(plan.links.is_empty());
        assert_eq!(plan.runtime.frame.time, 0.0);
        assert_close(
            plan.points[0].size,
            LatticeEngineConfig::default_v1().render.point_size,
        );
        assert_color_close(plan.points[0].color, [1.0, 0.0, 0.5, 1.0]);
    }

    #[test]
    fn lattice_render_plan_applies_noise_and_mesh_edges() {
        let mut config = LatticeEngineConfig::default_v1();
        config.points.grid_resolution = [3, 3, 1];
        config.points.max_points = 9;
        config.noise.enabled = true;
        config.noise.amplitude = 10.0;
        config.noise.frequency = 0.5;
        config.noise.speed = 2.0;
        config.mesh.enabled = true;
        config.mesh.max_edge = 90.0;
        config.mesh.opacity = 0.5;

        let noisy =
            build_lattice_render_plan(&config, LatticeRuntimeInputs::new(300, 300, 1.0).unwrap());
        config.noise.enabled = false;
        let stable =
            build_lattice_render_plan(&config, LatticeRuntimeInputs::new(300, 300, 1.0).unwrap());

        assert_eq!(noisy.points.len(), stable.points.len());
        assert_ne!(noisy.points[0].position, stable.points[0].position);
        assert!(!noisy.mesh_edges.is_empty());
        assert!(noisy
            .mesh_edges
            .iter()
            .all(|edge| edge.color[3] <= config.mesh.opacity));
    }

    #[test]
    fn lattice_renderer_draws_links_before_points_into_argb_buffer() {
        let mut config = LatticeEngineConfig::default_v1();
        config.points.grid_resolution = [2, 1, 1];
        config.points.spacing = 4.0;
        config.points.max_points = 2;
        config.render.point_size = 1.0;
        config.render.point_color = [1.0, 0.0, 0.0, 1.0];
        config.links.enabled = true;
        config.links.max_distance = 8.0;
        config.links.width = 1.0;
        config.links.opacity_falloff = 0.0;
        config.links.color = [0.0, 1.0, 0.0, 0.75];

        let plan =
            build_lattice_render_plan(&config, LatticeRuntimeInputs::new(16, 8, 0.0).unwrap());
        let mut output = vec![0u8; 16 * 8 * 4];
        render_lattice_plan_8bit(&plan, &mut output);

        let left_point = (4 * 16 + 6) * 4;
        assert!(output[left_point] > 0);
        assert!(output[left_point + 1] > 0);
        let link_mid = (4 * 16 + 8) * 4;
        assert!(output[link_mid] > 0);
        assert!(output[link_mid + 2] > output[link_mid + 1]);
    }

    #[test]
    fn lattice_renderer_ignores_undersized_output_buffers() {
        let config = LatticeEngineConfig::default_v1();
        let plan =
            build_lattice_render_plan(&config, LatticeRuntimeInputs::new(16, 16, 0.0).unwrap());
        let mut output = vec![7u8; 15];
        render_lattice_plan_8bit(&plan, &mut output);
        assert_eq!(output, vec![7u8; 15]);
    }

    #[test]
    fn lattice_compiler_rejects_particle_schema_versions() {
        let mut document = LatticeGraphDocument::simple_grid_network();
        document.schema_version += 1;

        let err = compile_lattice_graph(&document).unwrap_err();
        assert_eq!(
            err,
            LatticeGraphCompileError::UnsupportedSchemaVersion(LATTICE_GRAPH_SCHEMA_VERSION + 1)
        );
    }

    #[test]
    fn lattice_compiler_rejects_missing_output_node() {
        let mut document = LatticeGraphDocument::simple_grid_network();
        document.output_node = LatticeNodeId(404);

        let err = compile_lattice_graph(&document).unwrap_err();
        assert_eq!(
            err,
            LatticeGraphCompileError::MissingOutputNode(LatticeNodeId(404))
        );
    }

    #[test]
    fn lattice_compiler_rejects_duplicate_node_ids() {
        let mut document = LatticeGraphDocument::simple_grid_network();
        document.nodes.push(document.nodes[0].clone());

        let err = compile_lattice_graph(&document).unwrap_err();
        assert_eq!(
            err,
            LatticeGraphCompileError::DuplicateNodeId(document.nodes[0].id)
        );
    }

    #[test]
    fn lattice_compiler_rejects_unknown_edge_sources() {
        let mut document = LatticeGraphDocument::simple_grid_network();
        let missing_node = LatticeNodeId(999);
        document
            .edges
            .iter_mut()
            .find(|edge| edge.to.socket == "points")
            .unwrap()
            .from
            .node = missing_node;

        let err = compile_lattice_graph(&document).unwrap_err();
        assert_eq!(err, LatticeGraphCompileError::UnknownNode(missing_node));
    }

    #[test]
    fn lattice_compiler_rejects_wrong_node_kind_for_socket() {
        let mut document = LatticeGraphDocument::simple_grid_network();
        let render_node = LatticeNodeId(6);
        document
            .edges
            .iter_mut()
            .find(|edge| edge.to.socket == "points")
            .unwrap()
            .from
            .node = render_node;

        let err = compile_lattice_graph(&document).unwrap_err();
        assert_eq!(
            err,
            LatticeGraphCompileError::WrongNodeKind {
                node: render_node,
                socket: "points",
                expected: "lattice.points.grid"
            }
        );
    }
}
