#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use crate::engine::ParticleEngineConfig;
use crate::node_graph_core::{
    self, CoreGraphEdge, CoreGraphNode, NodeUiCatalog, NodeUiCatalogEntry, NodeUiConnectionSocket,
    NodeUiValueSocket, NodeUiValueType,
};
use crate::particle::EmitterType;
use crate::renderer::{
    ApplyMode, BlendMode, ImageColorMode, ImageFitMode, ParticleShape, TimeSamplingMode,
};

pub(crate) const GRAPH_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub(crate) struct NodeId(pub(crate) u64);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct GraphDocument {
    pub(crate) schema_version: u32,
    pub(crate) output_node: NodeId,
    pub(crate) nodes: Vec<GraphNode>,
    pub(crate) edges: Vec<GraphEdge>,
    #[serde(default)]
    pub(crate) published_params: Vec<GraphPublishedParam>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct GraphNode {
    pub(crate) id: NodeId,
    pub(crate) version: u32,
    #[serde(default)]
    pub(crate) label: String,
    #[serde(flatten)]
    pub(crate) kind: ParticleGraphNode,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct GraphEdge {
    pub(crate) from: GraphSocket,
    pub(crate) to: GraphSocket,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct GraphSocket {
    pub(crate) node: NodeId,
    pub(crate) socket: String,
}

impl CoreGraphNode for GraphNode {
    type Id = NodeId;

    fn id(&self) -> Self::Id {
        self.id
    }
}

impl CoreGraphEdge for GraphEdge {
    type Id = NodeId;

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
pub(crate) struct GraphPublishedParam {
    pub(crate) stable_id: String,
    pub(crate) label: String,
    pub(crate) target: GraphSocket,
    pub(crate) value_type: GraphPublishedValueType,
    pub(crate) default_value: GraphPublishedValue,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct GraphPublishedValueOverride {
    pub(crate) stable_id: String,
    pub(crate) value: GraphPublishedValue,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum GraphPublishedValueType {
    Float,
    Integer,
    Boolean,
    Color,
    Vector3,
    Enum,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub(crate) enum GraphPublishedValue {
    #[serde(rename = "float")]
    Float(f32),
    #[serde(rename = "integer")]
    Integer(i32),
    #[serde(rename = "boolean")]
    Boolean(bool),
    #[serde(rename = "color")]
    Color([f32; 4]),
    #[serde(rename = "vector3")]
    Vector3([f32; 3]),
    #[serde(rename = "enum")]
    Enum(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub(crate) enum ParticleGraphNode {
    #[serde(rename = "particle.output")]
    Output(OutputNode),
    #[serde(rename = "particle.emitter")]
    Emitter(EmitterNode),
    #[serde(rename = "particle.emitter.point")]
    PointEmitter(PointEmitterNode),
    #[serde(rename = "particle.motion")]
    Motion(MotionNode),
    #[serde(rename = "particle.physics")]
    Physics(PhysicsNode),
    #[serde(rename = "particle.appearance")]
    Appearance(AppearanceNode),
    #[serde(rename = "particle.render")]
    Render(RenderNode),
    #[serde(rename = "particle.child")]
    Child(ChildNode),
    #[serde(rename = "particle.system")]
    System(SystemNode),
}

impl ParticleGraphNode {
    pub(crate) fn node_type(&self) -> &'static str {
        match self {
            ParticleGraphNode::Output(_) => "particle.output",
            ParticleGraphNode::Emitter(_) => "particle.emitter",
            ParticleGraphNode::PointEmitter(_) => "particle.emitter.point",
            ParticleGraphNode::Motion(_) => "particle.motion",
            ParticleGraphNode::Physics(_) => "particle.physics",
            ParticleGraphNode::Appearance(_) => "particle.appearance",
            ParticleGraphNode::Render(_) => "particle.render",
            ParticleGraphNode::Child(_) => "particle.child",
            ParticleGraphNode::System(_) => "particle.system",
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct OutputNode {}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct EmitterNode {
    pub(crate) emitter_type: GraphEmitterType,
    pub(crate) position: [f32; 3],
    pub(crate) size: [f32; 3],
    pub(crate) birth_rate: f32,
    pub(crate) lifespan: f32,
    pub(crate) lifespan_variation: f32,
    pub(crate) emit_all_at_start: bool,
    pub(crate) grid_resolution: [u32; 3],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct PointEmitterNode {
    pub(crate) position: [f32; 3],
    pub(crate) birth_rate: f32,
    pub(crate) lifespan: f32,
    pub(crate) lifespan_variation: f32,
    pub(crate) emit_all_at_start: bool,
    pub(crate) grid_resolution: [u32; 3],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct MotionNode {
    pub(crate) initial_speed: f32,
    pub(crate) speed_variation: f32,
    pub(crate) initial_direction: [f32; 3],
    pub(crate) spread_degrees: f32,
    pub(crate) initial_size: f32,
    pub(crate) size_variation: f32,
    pub(crate) initial_rotation: f32,
    pub(crate) rotation_variation: f32,
    pub(crate) rotation_speed: f32,
    pub(crate) opacity_variation: f32,
    pub(crate) sprite_frame_count: u16,
    pub(crate) sprite_time_sampling: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct PhysicsNode {
    pub(crate) gravity: [f32; 3],
    pub(crate) wind: [f32; 3],
    pub(crate) air_resistance: f32,
    pub(crate) turbulence_strength: f32,
    pub(crate) turbulence_scale: f32,
    pub(crate) turbulence_speed: f32,
    pub(crate) bounce_floor_y: f32,
    pub(crate) bounce_enabled: bool,
    pub(crate) bounce_damping: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct AppearanceNode {
    pub(crate) color_start: [f32; 4],
    pub(crate) color_end: [f32; 4],
    pub(crate) size_over_life: [f32; 4],
    pub(crate) opacity_over_life: [f32; 4],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct RenderNode {
    pub(crate) shape: GraphParticleShape,
    pub(crate) blend_mode: GraphBlendMode,
    pub(crate) motion_blur: f32,
    pub(crate) edge_softness: f32,
    pub(crate) dof_enabled: bool,
    pub(crate) dof_focal_distance: f32,
    pub(crate) dof_aperture: f32,
    pub(crate) composite_on_original: bool,
    pub(crate) apply_mode: GraphApplyMode,
    pub(crate) size_multiplier: f32,
    pub(crate) time_sampling: GraphTimeSamplingMode,
    pub(crate) image_color_mode: GraphImageColorMode,
    pub(crate) image_fit_mode: GraphImageFitMode,
    pub(crate) use_source_alpha: bool,
    pub(crate) source_premultiplied: bool,
    pub(crate) alpha_clip: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ChildNode {
    pub(crate) enabled: bool,
    pub(crate) count: u32,
    pub(crate) inherit_velocity: f32,
    pub(crate) lifespan: f32,
    pub(crate) initial_speed: f32,
    pub(crate) spread_degrees: f32,
    pub(crate) size_scale: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct SystemNode {
    pub(crate) seed: u64,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum GraphEmitterType {
    Point,
    Box,
    Sphere,
    Grid,
    LayerAlpha,
    Path,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum GraphParticleShape {
    Circle,
    Square,
    Triangle,
    Star,
    Line,
    Image,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum GraphBlendMode {
    Normal,
    Add,
    Screen,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum GraphApplyMode {
    OnTransparent,
    Normal,
    Add,
    Screen,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum GraphTimeSamplingMode {
    CurrentTime,
    BirthTime,
    RandomStill,
    RandomPlay,
    Cycle,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum GraphImageColorMode {
    Tint,
    Source,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum GraphImageFitMode {
    Contain,
    Stretch,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum GraphCompileError {
    UnsupportedSchemaVersion(u32),
    DuplicateNodeId(NodeId),
    UnknownNode(NodeId),
    MissingOutputNode(NodeId),
    WrongNodeKind {
        node: NodeId,
        socket: &'static str,
        expected: &'static str,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum GraphPublishedParamError {
    DuplicateNodeId(NodeId),
    DuplicateStableId(String),
    EmptyStableId,
    InvalidEnumValue {
        stable_id: String,
        value: String,
    },
    UnknownNode(NodeId),
    UnknownStableId(String),
    UnsupportedSocket {
        node: NodeId,
        socket: String,
    },
    DefaultValueTypeMismatch {
        stable_id: String,
        expected: GraphPublishedValueType,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum GraphPublishedOverrideError {
    Compile(GraphCompileError),
    PublishedParam(GraphPublishedParamError),
}

impl GraphDocument {
    pub(crate) fn from_value(mut value: Value) -> Result<Self, serde_json::Error> {
        migrate_graph_document_value(&mut value);
        serde_json::from_value(value)
    }

    pub(crate) fn simple_point_emitter() -> Self {
        let config = ParticleEngineConfig::classic_default();
        let output = NodeId(1);
        let emitter = NodeId(2);
        let motion = NodeId(3);
        let physics = NodeId(4);
        let appearance = NodeId(5);
        let render = NodeId(6);
        let child = NodeId(7);

        Self {
            schema_version: GRAPH_SCHEMA_VERSION,
            output_node: output,
            nodes: vec![
                node(
                    output,
                    "Output",
                    ParticleGraphNode::Output(OutputNode::default()),
                ),
                node(
                    emitter,
                    "Point Emitter",
                    ParticleGraphNode::PointEmitter(PointEmitterNode {
                        position: config.emitter.position.to_array(),
                        birth_rate: config.emitter.birth_rate,
                        lifespan: config.emitter.lifespan,
                        lifespan_variation: config.emitter.lifespan_variation,
                        emit_all_at_start: config.emitter.emit_all_at_start,
                        grid_resolution: [
                            config.emitter.grid_res_x,
                            config.emitter.grid_res_y,
                            config.emitter.grid_res_z,
                        ],
                    }),
                ),
                node(
                    motion,
                    "Motion",
                    ParticleGraphNode::Motion(MotionNode {
                        initial_speed: config.emitter.initial_speed,
                        speed_variation: config.emitter.speed_variation,
                        initial_direction: config.emitter.initial_direction.to_array(),
                        spread_degrees: config.emitter.spread.to_degrees(),
                        initial_size: config.emitter.initial_size,
                        size_variation: config.emitter.size_variation,
                        initial_rotation: config.emitter.initial_rotation,
                        rotation_variation: config.emitter.rotation_variation,
                        rotation_speed: config.emitter.rotation_speed,
                        opacity_variation: config.emitter.opacity_variation,
                        sprite_frame_count: config.emitter.sprite_frame_count,
                        sprite_time_sampling: config.emitter.sprite_time_sampling,
                    }),
                ),
                node(
                    physics,
                    "Physics",
                    ParticleGraphNode::Physics(PhysicsNode {
                        gravity: config.physics.gravity.to_array(),
                        wind: config.physics.wind.to_array(),
                        air_resistance: config.physics.air_resistance,
                        turbulence_strength: config.physics.turbulence_strength,
                        turbulence_scale: config.physics.turbulence_scale,
                        turbulence_speed: config.physics.turbulence_speed,
                        bounce_floor_y: config.physics.bounce_floor_y,
                        bounce_enabled: config.physics.bounce_enabled,
                        bounce_damping: config.physics.bounce_damping,
                    }),
                ),
                node(
                    appearance,
                    "Appearance",
                    ParticleGraphNode::Appearance(AppearanceNode {
                        color_start: config.appearance.color_start,
                        color_end: config.appearance.color_end,
                        size_over_life: config.appearance.size_over_life,
                        opacity_over_life: config.appearance.opacity_over_life,
                    }),
                ),
                node(
                    render,
                    "Render",
                    ParticleGraphNode::Render(RenderNode {
                        shape: GraphParticleShape::Circle,
                        blend_mode: GraphBlendMode::Normal,
                        motion_blur: config.render.motion_blur,
                        edge_softness: config.render.edge_softness,
                        dof_enabled: config.render.dof_enabled,
                        dof_focal_distance: config.render.dof_focal_distance,
                        dof_aperture: config.render.dof_aperture,
                        composite_on_original: config.render.composite_on_original,
                        apply_mode: GraphApplyMode::Normal,
                        size_multiplier: config.render.size_multiplier,
                        time_sampling: GraphTimeSamplingMode::CurrentTime,
                        image_color_mode: GraphImageColorMode::Tint,
                        image_fit_mode: GraphImageFitMode::Contain,
                        use_source_alpha: config.render.image_sampling.use_source_alpha,
                        source_premultiplied: config.render.image_sampling.source_premultiplied,
                        alpha_clip: config.render.image_sampling.alpha_clip,
                    }),
                ),
                node(
                    child,
                    "Child",
                    ParticleGraphNode::Child(ChildNode {
                        enabled: config.child.enabled,
                        count: config.child.count,
                        inherit_velocity: config.child.inherit_velocity,
                        lifespan: config.child.lifespan,
                        initial_speed: config.child.initial_speed,
                        spread_degrees: config.child.spread.to_degrees(),
                        size_scale: config.child.size_scale,
                    }),
                ),
            ],
            edges: vec![
                edge(emitter, "out", output, "emitter"),
                edge(motion, "out", output, "motion"),
                edge(physics, "out", output, "physics"),
                edge(appearance, "out", output, "appearance"),
                edge(render, "out", output, "render"),
                edge(child, "out", output, "child"),
            ],
            published_params: Vec::new(),
        }
    }

    pub(crate) fn from_engine_config(config: &ParticleEngineConfig) -> Self {
        let output = NodeId(1);
        let emitter = NodeId(2);
        let motion = NodeId(3);
        let physics = NodeId(4);
        let appearance = NodeId(5);
        let render = NodeId(6);
        let child = NodeId(7);
        let system = NodeId(8);

        Self {
            schema_version: GRAPH_SCHEMA_VERSION,
            output_node: output,
            nodes: vec![
                node(
                    output,
                    "Output",
                    ParticleGraphNode::Output(OutputNode::default()),
                ),
                node(
                    emitter,
                    "Emitter",
                    ParticleGraphNode::Emitter(EmitterNode {
                        emitter_type: graph_emitter_type(config.emitter.emitter_type),
                        position: config.emitter.position.to_array(),
                        size: config.emitter.size.to_array(),
                        birth_rate: config.emitter.birth_rate,
                        lifespan: config.emitter.lifespan,
                        lifespan_variation: config.emitter.lifespan_variation,
                        emit_all_at_start: config.emitter.emit_all_at_start,
                        grid_resolution: [
                            config.emitter.grid_res_x,
                            config.emitter.grid_res_y,
                            config.emitter.grid_res_z,
                        ],
                    }),
                ),
                node(
                    motion,
                    "Motion",
                    ParticleGraphNode::Motion(MotionNode {
                        initial_speed: config.emitter.initial_speed,
                        speed_variation: config.emitter.speed_variation,
                        initial_direction: config.emitter.initial_direction.to_array(),
                        spread_degrees: config.emitter.spread.to_degrees(),
                        initial_size: config.emitter.initial_size,
                        size_variation: config.emitter.size_variation,
                        initial_rotation: config.emitter.initial_rotation,
                        rotation_variation: config.emitter.rotation_variation,
                        rotation_speed: config.emitter.rotation_speed,
                        opacity_variation: config.emitter.opacity_variation,
                        sprite_frame_count: config.emitter.sprite_frame_count,
                        sprite_time_sampling: config.emitter.sprite_time_sampling,
                    }),
                ),
                node(
                    physics,
                    "Physics",
                    ParticleGraphNode::Physics(PhysicsNode {
                        gravity: config.physics.gravity.to_array(),
                        wind: config.physics.wind.to_array(),
                        air_resistance: config.physics.air_resistance,
                        turbulence_strength: config.physics.turbulence_strength,
                        turbulence_scale: config.physics.turbulence_scale,
                        turbulence_speed: config.physics.turbulence_speed,
                        bounce_floor_y: config.physics.bounce_floor_y,
                        bounce_enabled: config.physics.bounce_enabled,
                        bounce_damping: config.physics.bounce_damping,
                    }),
                ),
                node(
                    appearance,
                    "Appearance",
                    ParticleGraphNode::Appearance(AppearanceNode {
                        color_start: config.appearance.color_start,
                        color_end: config.appearance.color_end,
                        size_over_life: config.appearance.size_over_life,
                        opacity_over_life: config.appearance.opacity_over_life,
                    }),
                ),
                node(
                    render,
                    "Render",
                    ParticleGraphNode::Render(RenderNode {
                        shape: graph_particle_shape(config.render.shape),
                        blend_mode: graph_blend_mode(config.render.blend_mode),
                        motion_blur: config.render.motion_blur,
                        edge_softness: config.render.edge_softness,
                        dof_enabled: config.render.dof_enabled,
                        dof_focal_distance: config.render.dof_focal_distance,
                        dof_aperture: config.render.dof_aperture,
                        composite_on_original: config.render.composite_on_original,
                        apply_mode: graph_apply_mode(config.render.apply_mode),
                        size_multiplier: config.render.size_multiplier,
                        time_sampling: graph_time_sampling(config.render.time_sampling),
                        image_color_mode: graph_image_color_mode(config.render.image_color_mode),
                        image_fit_mode: graph_image_fit_mode(config.render.image_fit_mode),
                        use_source_alpha: config.render.image_sampling.use_source_alpha,
                        source_premultiplied: config.render.image_sampling.source_premultiplied,
                        alpha_clip: config.render.image_sampling.alpha_clip,
                    }),
                ),
                node(
                    child,
                    "Child",
                    ParticleGraphNode::Child(ChildNode {
                        enabled: config.child.enabled,
                        count: config.child.count,
                        inherit_velocity: config.child.inherit_velocity,
                        lifespan: config.child.lifespan,
                        initial_speed: config.child.initial_speed,
                        spread_degrees: config.child.spread.to_degrees(),
                        size_scale: config.child.size_scale,
                    }),
                ),
                node(
                    system,
                    "System",
                    ParticleGraphNode::System(SystemNode { seed: config.seed }),
                ),
            ],
            edges: vec![
                edge(emitter, "out", output, "emitter"),
                edge(motion, "out", output, "motion"),
                edge(physics, "out", output, "physics"),
                edge(appearance, "out", output, "appearance"),
                edge(render, "out", output, "render"),
                edge(child, "out", output, "child"),
                edge(system, "out", output, "system"),
            ],
            published_params: Vec::new(),
        }
    }
}

pub(crate) fn migrate_graph_document_value(value: &mut Value) {
    let Some(obj) = value.as_object_mut() else {
        return;
    };

    obj.entry("schema_version".to_string())
        .or_insert_with(|| Value::from(GRAPH_SCHEMA_VERSION));
    obj.entry("published_params".to_string())
        .or_insert_with(|| Value::Array(Vec::new()));

    let schema_version = obj
        .get("schema_version")
        .and_then(Value::as_u64)
        .unwrap_or(GRAPH_SCHEMA_VERSION as u64);
    if schema_version > GRAPH_SCHEMA_VERSION as u64 {
        return;
    }
    if schema_version < GRAPH_SCHEMA_VERSION as u64 {
        obj.insert(
            "schema_version".to_string(),
            Value::from(GRAPH_SCHEMA_VERSION),
        );
    }

    let Some(nodes) = obj.get_mut("nodes").and_then(Value::as_array_mut) else {
        return;
    };
    for node in nodes {
        migrate_graph_node_value(node);
    }
}

fn migrate_graph_node_value(value: &mut Value) {
    let Some(obj) = value.as_object_mut() else {
        return;
    };
    obj.entry("version".to_string())
        .or_insert_with(|| Value::from(1));

    let Some(node_type) = obj.get("type").and_then(Value::as_str) else {
        return;
    };
    let migrated = match node_type {
        "output" => "particle.output",
        "emitter" => "particle.emitter",
        "emitter.point" => "particle.emitter.point",
        "motion" => "particle.motion",
        "physics" => "particle.physics",
        "appearance" => "particle.appearance",
        "render" => "particle.render",
        "child" => "particle.child",
        "system" => "particle.system",
        _ => return,
    };
    obj.insert("type".to_string(), Value::from(migrated));
}

pub(crate) fn particle_node_ui_catalog() -> NodeUiCatalog {
    NodeUiCatalog::new(
        "particlelab",
        "ParticleLab",
        GRAPH_SCHEMA_VERSION,
        vec![
            catalog_node(
                "particle.output",
                "Output",
                vec![
                    connection_socket("emitter", "Emitter"),
                    connection_socket("motion", "Motion"),
                    connection_socket("physics", "Physics"),
                    connection_socket("appearance", "Appearance"),
                    connection_socket("render", "Render"),
                    connection_socket("child", "Child"),
                    connection_socket("system", "System"),
                ],
                Vec::new(),
                Vec::new(),
            ),
            catalog_node(
                "particle.emitter",
                "Emitter",
                Vec::new(),
                vec![connection_socket("out", "Out")],
                vec![
                    enum_value_socket(
                        "emitter_type",
                        "Emitter Type",
                        &[
                            ("point", "Point"),
                            ("box", "Box"),
                            ("sphere", "Sphere"),
                            ("grid", "Grid"),
                            ("layer_alpha", "Layer Alpha"),
                            ("path", "Path"),
                        ],
                    ),
                    value_socket("position", "Position", GraphPublishedValueType::Vector3),
                    value_socket("size", "Size", GraphPublishedValueType::Vector3),
                    value_socket("birth_rate", "Birth Rate", GraphPublishedValueType::Float),
                    value_socket("lifespan", "Lifespan", GraphPublishedValueType::Float),
                    value_socket(
                        "lifespan_variation",
                        "Lifespan Variation",
                        GraphPublishedValueType::Float,
                    ),
                    value_socket(
                        "emit_all_at_start",
                        "Emit All At Start",
                        GraphPublishedValueType::Boolean,
                    ),
                    value_socket(
                        "grid_resolution",
                        "Grid Resolution",
                        GraphPublishedValueType::Vector3,
                    ),
                ],
            ),
            catalog_node(
                "particle.emitter.point",
                "Point Emitter",
                Vec::new(),
                vec![connection_socket("out", "Out")],
                vec![
                    value_socket("position", "Position", GraphPublishedValueType::Vector3),
                    value_socket("birth_rate", "Birth Rate", GraphPublishedValueType::Float),
                    value_socket("lifespan", "Lifespan", GraphPublishedValueType::Float),
                    value_socket(
                        "lifespan_variation",
                        "Lifespan Variation",
                        GraphPublishedValueType::Float,
                    ),
                    value_socket(
                        "emit_all_at_start",
                        "Emit All At Start",
                        GraphPublishedValueType::Boolean,
                    ),
                    value_socket(
                        "grid_resolution",
                        "Grid Resolution",
                        GraphPublishedValueType::Vector3,
                    ),
                ],
            ),
            catalog_node(
                "particle.motion",
                "Motion",
                Vec::new(),
                vec![connection_socket("out", "Out")],
                vec![
                    value_socket(
                        "initial_speed",
                        "Initial Speed",
                        GraphPublishedValueType::Float,
                    ),
                    value_socket(
                        "speed_variation",
                        "Speed Variation",
                        GraphPublishedValueType::Float,
                    ),
                    value_socket(
                        "initial_direction",
                        "Initial Direction",
                        GraphPublishedValueType::Vector3,
                    ),
                    value_socket("spread_degrees", "Spread", GraphPublishedValueType::Float),
                    value_socket(
                        "initial_size",
                        "Initial Size",
                        GraphPublishedValueType::Float,
                    ),
                    value_socket(
                        "size_variation",
                        "Size Variation",
                        GraphPublishedValueType::Float,
                    ),
                    value_socket(
                        "initial_rotation",
                        "Initial Rotation",
                        GraphPublishedValueType::Float,
                    ),
                    value_socket(
                        "rotation_variation",
                        "Rotation Variation",
                        GraphPublishedValueType::Float,
                    ),
                    value_socket(
                        "rotation_speed",
                        "Rotation Speed",
                        GraphPublishedValueType::Float,
                    ),
                    value_socket(
                        "opacity_variation",
                        "Opacity Variation",
                        GraphPublishedValueType::Float,
                    ),
                    value_socket(
                        "sprite_frame_count",
                        "Sprite Frame Count",
                        GraphPublishedValueType::Integer,
                    ),
                    value_socket(
                        "sprite_time_sampling",
                        "Sprite Time Sampling",
                        GraphPublishedValueType::Integer,
                    ),
                ],
            ),
            catalog_node(
                "particle.physics",
                "Physics",
                Vec::new(),
                vec![connection_socket("out", "Out")],
                vec![
                    value_socket("gravity", "Gravity", GraphPublishedValueType::Vector3),
                    value_socket("wind", "Wind", GraphPublishedValueType::Vector3),
                    value_socket(
                        "air_resistance",
                        "Air Resistance",
                        GraphPublishedValueType::Float,
                    ),
                    value_socket(
                        "turbulence_strength",
                        "Turbulence Strength",
                        GraphPublishedValueType::Float,
                    ),
                    value_socket(
                        "turbulence_scale",
                        "Turbulence Scale",
                        GraphPublishedValueType::Float,
                    ),
                    value_socket(
                        "turbulence_speed",
                        "Turbulence Speed",
                        GraphPublishedValueType::Float,
                    ),
                    value_socket(
                        "bounce_floor_y",
                        "Bounce Floor Y",
                        GraphPublishedValueType::Float,
                    ),
                    value_socket(
                        "bounce_enabled",
                        "Bounce Enabled",
                        GraphPublishedValueType::Boolean,
                    ),
                    value_socket(
                        "bounce_damping",
                        "Bounce Damping",
                        GraphPublishedValueType::Float,
                    ),
                ],
            ),
            catalog_node(
                "particle.appearance",
                "Appearance",
                Vec::new(),
                vec![connection_socket("out", "Out")],
                vec![
                    value_socket("color_start", "Color Start", GraphPublishedValueType::Color),
                    value_socket("color_end", "Color End", GraphPublishedValueType::Color),
                    value_socket(
                        "size_over_life",
                        "Size Over Life",
                        GraphPublishedValueType::Color,
                    ),
                    value_socket(
                        "opacity_over_life",
                        "Opacity Over Life",
                        GraphPublishedValueType::Color,
                    ),
                ],
            ),
            catalog_node(
                "particle.render",
                "Render",
                Vec::new(),
                vec![connection_socket("out", "Out")],
                vec![
                    enum_value_socket(
                        "shape",
                        "Shape",
                        &[
                            ("circle", "Circle"),
                            ("square", "Square"),
                            ("triangle", "Triangle"),
                            ("star", "Star"),
                            ("line", "Line"),
                            ("image", "Image"),
                        ],
                    ),
                    enum_value_socket(
                        "blend_mode",
                        "Blend Mode",
                        &[("normal", "Normal"), ("add", "Add"), ("screen", "Screen")],
                    ),
                    value_socket("motion_blur", "Motion Blur", GraphPublishedValueType::Float),
                    value_socket(
                        "edge_softness",
                        "Edge Softness",
                        GraphPublishedValueType::Float,
                    ),
                    value_socket(
                        "dof_enabled",
                        "Depth Of Field",
                        GraphPublishedValueType::Boolean,
                    ),
                    value_socket(
                        "dof_focal_distance",
                        "DOF Focal Distance",
                        GraphPublishedValueType::Float,
                    ),
                    value_socket(
                        "dof_aperture",
                        "DOF Aperture",
                        GraphPublishedValueType::Float,
                    ),
                    value_socket(
                        "composite_on_original",
                        "Composite On Original",
                        GraphPublishedValueType::Boolean,
                    ),
                    enum_value_socket(
                        "apply_mode",
                        "Apply Mode",
                        &[
                            ("on_transparent", "On Transparent"),
                            ("normal", "Normal"),
                            ("add", "Add"),
                            ("screen", "Screen"),
                        ],
                    ),
                    value_socket(
                        "size_multiplier",
                        "Size Multiplier",
                        GraphPublishedValueType::Float,
                    ),
                    enum_value_socket(
                        "time_sampling",
                        "Time Sampling",
                        &[
                            ("current_time", "Current Time"),
                            ("birth_time", "Birth Time"),
                            ("random_still", "Random Still"),
                            ("random_play", "Random Play"),
                            ("cycle", "Cycle"),
                        ],
                    ),
                    enum_value_socket(
                        "image_color_mode",
                        "Image Color Mode",
                        &[("tint", "Tint"), ("source", "Source")],
                    ),
                    enum_value_socket(
                        "image_fit_mode",
                        "Image Fit Mode",
                        &[("contain", "Contain"), ("stretch", "Stretch")],
                    ),
                    value_socket(
                        "use_source_alpha",
                        "Use Source Alpha",
                        GraphPublishedValueType::Boolean,
                    ),
                    value_socket(
                        "source_premultiplied",
                        "Source Premultiplied",
                        GraphPublishedValueType::Boolean,
                    ),
                    value_socket("alpha_clip", "Alpha Clip", GraphPublishedValueType::Float),
                ],
            ),
            catalog_node(
                "particle.child",
                "Child",
                Vec::new(),
                vec![connection_socket("out", "Out")],
                vec![
                    value_socket("enabled", "Enabled", GraphPublishedValueType::Boolean),
                    value_socket("count", "Count", GraphPublishedValueType::Integer),
                    value_socket(
                        "inherit_velocity",
                        "Inherit Velocity",
                        GraphPublishedValueType::Float,
                    ),
                    value_socket("lifespan", "Lifespan", GraphPublishedValueType::Float),
                    value_socket(
                        "initial_speed",
                        "Initial Speed",
                        GraphPublishedValueType::Float,
                    ),
                    value_socket("spread_degrees", "Spread", GraphPublishedValueType::Float),
                    value_socket("size_scale", "Size Scale", GraphPublishedValueType::Float),
                ],
            ),
            catalog_node(
                "particle.system",
                "System",
                Vec::new(),
                vec![connection_socket("out", "Out")],
                vec![value_socket(
                    "seed",
                    "Seed",
                    GraphPublishedValueType::Integer,
                )],
            ),
        ],
    )
}

fn catalog_node(
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

fn connection_socket(socket: &str, label: &str) -> NodeUiConnectionSocket {
    NodeUiConnectionSocket::new(socket, label)
}

fn value_socket(
    socket: &str,
    label: &str,
    value_type: GraphPublishedValueType,
) -> NodeUiValueSocket {
    NodeUiValueSocket::value(socket, label, node_ui_value_type(value_type))
}

fn enum_value_socket(socket: &str, label: &str, options: &[(&str, &str)]) -> NodeUiValueSocket {
    NodeUiValueSocket::enumeration(socket, label, options)
}

fn node_ui_value_type(value_type: GraphPublishedValueType) -> NodeUiValueType {
    match value_type {
        GraphPublishedValueType::Float => NodeUiValueType::Float,
        GraphPublishedValueType::Integer => NodeUiValueType::Integer,
        GraphPublishedValueType::Boolean => NodeUiValueType::Boolean,
        GraphPublishedValueType::Color => NodeUiValueType::Color,
        GraphPublishedValueType::Vector3 => NodeUiValueType::Vector3,
        GraphPublishedValueType::Enum => NodeUiValueType::Enum,
    }
}

pub(crate) fn compile_particle_graph(
    document: &GraphDocument,
) -> Result<ParticleEngineConfig, GraphCompileError> {
    if document.schema_version != GRAPH_SCHEMA_VERSION {
        return Err(GraphCompileError::UnsupportedSchemaVersion(
            document.schema_version,
        ));
    }

    let nodes = node_map(document)?;
    let output = nodes
        .get(&document.output_node)
        .copied()
        .ok_or(GraphCompileError::MissingOutputNode(document.output_node))?;
    if !matches!(output.kind, ParticleGraphNode::Output(_)) {
        return Err(GraphCompileError::WrongNodeKind {
            node: output.id,
            socket: "output",
            expected: "particle.output",
        });
    }

    let mut config = ParticleEngineConfig::classic_default();

    if let Some(node) = output_input(document, &nodes, output.id, "emitter")? {
        match &node.kind {
            ParticleGraphNode::Emitter(data) => apply_emitter(&mut config, data),
            ParticleGraphNode::PointEmitter(data) => apply_point_emitter(&mut config, data),
            _ => return wrong(node.id, "emitter", "particle.emitter"),
        }
    }
    if let Some(node) = output_input(document, &nodes, output.id, "motion")? {
        match &node.kind {
            ParticleGraphNode::Motion(data) => apply_motion(&mut config, data),
            _ => return wrong(node.id, "motion", "particle.motion"),
        }
    }
    if let Some(node) = output_input(document, &nodes, output.id, "physics")? {
        match &node.kind {
            ParticleGraphNode::Physics(data) => apply_physics(&mut config, data),
            _ => return wrong(node.id, "physics", "particle.physics"),
        }
    }
    if let Some(node) = output_input(document, &nodes, output.id, "appearance")? {
        match &node.kind {
            ParticleGraphNode::Appearance(data) => apply_appearance(&mut config, data),
            _ => return wrong(node.id, "appearance", "particle.appearance"),
        }
    }
    if let Some(node) = output_input(document, &nodes, output.id, "render")? {
        match &node.kind {
            ParticleGraphNode::Render(data) => apply_render(&mut config, data),
            _ => return wrong(node.id, "render", "particle.render"),
        }
    }
    if let Some(node) = output_input(document, &nodes, output.id, "child")? {
        match &node.kind {
            ParticleGraphNode::Child(data) => apply_child(&mut config, data),
            _ => return wrong(node.id, "child", "particle.child"),
        }
    }
    if let Some(node) = output_input(document, &nodes, output.id, "system")? {
        match &node.kind {
            ParticleGraphNode::System(data) => apply_system(&mut config, data),
            _ => return wrong(node.id, "system", "particle.system"),
        }
    }

    Ok(config)
}

pub(crate) fn compile_particle_graph_with_published_values(
    document: &GraphDocument,
    overrides: &[GraphPublishedValueOverride],
) -> Result<ParticleEngineConfig, GraphPublishedOverrideError> {
    if overrides.is_empty() {
        return compile_particle_graph(document).map_err(GraphPublishedOverrideError::Compile);
    }

    let mut document = document.clone();
    apply_published_value_overrides(&mut document, overrides)?;
    compile_particle_graph(&document).map_err(GraphPublishedOverrideError::Compile)
}

pub(crate) fn compile_particle_graph_with_compatible_published_values(
    document: &GraphDocument,
    overrides: &[GraphPublishedValueOverride],
) -> Result<ParticleEngineConfig, GraphCompileError> {
    if overrides.is_empty() {
        return compile_particle_graph(document);
    }

    let mut document = document.clone();
    for value_override in overrides {
        let _ =
            apply_published_value_overrides(&mut document, std::slice::from_ref(value_override));
    }
    compile_particle_graph(&document)
}

pub(crate) fn apply_published_value_overrides(
    document: &mut GraphDocument,
    overrides: &[GraphPublishedValueOverride],
) -> Result<(), GraphPublishedOverrideError> {
    validate_published_params(document).map_err(GraphPublishedOverrideError::PublishedParam)?;

    for value_override in overrides {
        let published = document
            .published_params
            .iter()
            .find(|param| param.stable_id == value_override.stable_id)
            .cloned()
            .ok_or_else(|| {
                GraphPublishedOverrideError::PublishedParam(
                    GraphPublishedParamError::UnknownStableId(value_override.stable_id.clone()),
                )
            })?;
        if value_override.value.value_type() != published.value_type {
            return Err(GraphPublishedOverrideError::PublishedParam(
                GraphPublishedParamError::DefaultValueTypeMismatch {
                    stable_id: value_override.stable_id.clone(),
                    expected: published.value_type,
                },
            ));
        }
        let node = document
            .nodes
            .iter_mut()
            .find(|node| node.id == published.target.node)
            .ok_or_else(|| {
                GraphPublishedOverrideError::PublishedParam(GraphPublishedParamError::UnknownNode(
                    published.target.node,
                ))
            })?;
        apply_published_value_to_node(
            &mut node.kind,
            &published.target.socket,
            &published.stable_id,
            &value_override.value,
        )
        .map_err(GraphPublishedOverrideError::PublishedParam)?;
    }

    Ok(())
}

pub(crate) fn validate_published_params(
    document: &GraphDocument,
) -> Result<(), GraphPublishedParamError> {
    let nodes = node_map(document).map_err(|err| match err {
        GraphCompileError::DuplicateNodeId(node) => GraphPublishedParamError::DuplicateNodeId(node),
        GraphCompileError::UnknownNode(node) | GraphCompileError::MissingOutputNode(node) => {
            GraphPublishedParamError::UnknownNode(node)
        }
        GraphCompileError::UnsupportedSchemaVersion(_)
        | GraphCompileError::WrongNodeKind { .. } => GraphPublishedParamError::UnsupportedSocket {
            node: document.output_node,
            socket: "graph".to_string(),
        },
    })?;
    let mut stable_ids = std::collections::HashSet::new();

    for param in &document.published_params {
        if param.stable_id.trim().is_empty() {
            return Err(GraphPublishedParamError::EmptyStableId);
        }
        if !stable_ids.insert(param.stable_id.as_str()) {
            return Err(GraphPublishedParamError::DuplicateStableId(
                param.stable_id.clone(),
            ));
        }
        if param.default_value.value_type() != param.value_type {
            return Err(GraphPublishedParamError::DefaultValueTypeMismatch {
                stable_id: param.stable_id.clone(),
                expected: param.value_type,
            });
        }
        let Some(node) = nodes.get(&param.target.node).copied() else {
            return Err(GraphPublishedParamError::UnknownNode(param.target.node));
        };
        let Some(socket_type) = published_socket_type(&node.kind, &param.target.socket) else {
            return Err(GraphPublishedParamError::UnsupportedSocket {
                node: param.target.node,
                socket: param.target.socket.clone(),
            });
        };
        if socket_type != param.value_type {
            return Err(GraphPublishedParamError::DefaultValueTypeMismatch {
                stable_id: param.stable_id.clone(),
                expected: socket_type,
            });
        }
    }

    Ok(())
}

impl GraphPublishedValue {
    fn value_type(&self) -> GraphPublishedValueType {
        match self {
            GraphPublishedValue::Float(_) => GraphPublishedValueType::Float,
            GraphPublishedValue::Integer(_) => GraphPublishedValueType::Integer,
            GraphPublishedValue::Boolean(_) => GraphPublishedValueType::Boolean,
            GraphPublishedValue::Color(_) => GraphPublishedValueType::Color,
            GraphPublishedValue::Vector3(_) => GraphPublishedValueType::Vector3,
            GraphPublishedValue::Enum(_) => GraphPublishedValueType::Enum,
        }
    }
}

fn apply_published_value_to_node(
    kind: &mut ParticleGraphNode,
    socket: &str,
    stable_id: &str,
    value: &GraphPublishedValue,
) -> Result<(), GraphPublishedParamError> {
    match kind {
        ParticleGraphNode::Output(_) => {}
        ParticleGraphNode::Emitter(data) => match socket {
            "emitter_type" => {
                data.emitter_type = enum_emitter_type(stable_id, value)?;
                return Ok(());
            }
            "position" => {
                data.position = vector3_value(value);
                return Ok(());
            }
            "size" => {
                data.size = vector3_value(value);
                return Ok(());
            }
            "grid_resolution" => {
                data.grid_resolution = vector3_to_resolution(vector3_value(value));
                return Ok(());
            }
            "birth_rate" => {
                data.birth_rate = float_value(value);
                return Ok(());
            }
            "lifespan" => {
                data.lifespan = float_value(value);
                return Ok(());
            }
            "lifespan_variation" => {
                data.lifespan_variation = float_value(value);
                return Ok(());
            }
            "emit_all_at_start" => {
                data.emit_all_at_start = bool_value(value);
                return Ok(());
            }
            _ => {}
        },
        ParticleGraphNode::PointEmitter(data) => match socket {
            "position" => {
                data.position = vector3_value(value);
                return Ok(());
            }
            "grid_resolution" => {
                data.grid_resolution = vector3_to_resolution(vector3_value(value));
                return Ok(());
            }
            "birth_rate" => {
                data.birth_rate = float_value(value);
                return Ok(());
            }
            "lifespan" => {
                data.lifespan = float_value(value);
                return Ok(());
            }
            "lifespan_variation" => {
                data.lifespan_variation = float_value(value);
                return Ok(());
            }
            "emit_all_at_start" => {
                data.emit_all_at_start = bool_value(value);
                return Ok(());
            }
            _ => {}
        },
        ParticleGraphNode::Motion(data) => match socket {
            "initial_direction" => {
                data.initial_direction = vector3_value(value);
                return Ok(());
            }
            "initial_speed" => {
                data.initial_speed = float_value(value);
                return Ok(());
            }
            "speed_variation" => {
                data.speed_variation = float_value(value);
                return Ok(());
            }
            "spread_degrees" => {
                data.spread_degrees = float_value(value);
                return Ok(());
            }
            "initial_size" => {
                data.initial_size = float_value(value);
                return Ok(());
            }
            "size_variation" => {
                data.size_variation = float_value(value);
                return Ok(());
            }
            "initial_rotation" => {
                data.initial_rotation = float_value(value);
                return Ok(());
            }
            "rotation_variation" => {
                data.rotation_variation = float_value(value);
                return Ok(());
            }
            "rotation_speed" => {
                data.rotation_speed = float_value(value);
                return Ok(());
            }
            "opacity_variation" => {
                data.opacity_variation = float_value(value);
                return Ok(());
            }
            "sprite_frame_count" => {
                data.sprite_frame_count = integer_value(value).max(1) as u16;
                return Ok(());
            }
            "sprite_time_sampling" => {
                data.sprite_time_sampling = integer_value(value).clamp(0, 4) as u8;
                return Ok(());
            }
            _ => {}
        },
        ParticleGraphNode::Physics(data) => match socket {
            "gravity" => {
                data.gravity = vector3_value(value);
                return Ok(());
            }
            "wind" => {
                data.wind = vector3_value(value);
                return Ok(());
            }
            "air_resistance" => {
                data.air_resistance = float_value(value);
                return Ok(());
            }
            "turbulence_strength" => {
                data.turbulence_strength = float_value(value);
                return Ok(());
            }
            "turbulence_scale" => {
                data.turbulence_scale = float_value(value);
                return Ok(());
            }
            "turbulence_speed" => {
                data.turbulence_speed = float_value(value);
                return Ok(());
            }
            "bounce_floor_y" => {
                data.bounce_floor_y = float_value(value);
                return Ok(());
            }
            "bounce_damping" => {
                data.bounce_damping = float_value(value);
                return Ok(());
            }
            "bounce_enabled" => {
                data.bounce_enabled = bool_value(value);
                return Ok(());
            }
            _ => {}
        },
        ParticleGraphNode::Appearance(data) => match socket {
            "color_start" => {
                data.color_start = color_value(value);
                return Ok(());
            }
            "color_end" => {
                data.color_end = color_value(value);
                return Ok(());
            }
            "size_over_life" => {
                data.size_over_life = color_value(value);
                return Ok(());
            }
            "opacity_over_life" => {
                data.opacity_over_life = color_value(value);
                return Ok(());
            }
            _ => {}
        },
        ParticleGraphNode::Render(data) => match socket {
            "shape" => {
                data.shape = enum_particle_shape(stable_id, value)?;
                return Ok(());
            }
            "blend_mode" => {
                data.blend_mode = enum_blend_mode(stable_id, value)?;
                return Ok(());
            }
            "apply_mode" => {
                data.apply_mode = enum_apply_mode(stable_id, value)?;
                return Ok(());
            }
            "time_sampling" => {
                data.time_sampling = enum_time_sampling(stable_id, value)?;
                return Ok(());
            }
            "image_color_mode" => {
                data.image_color_mode = enum_image_color_mode(stable_id, value)?;
                return Ok(());
            }
            "image_fit_mode" => {
                data.image_fit_mode = enum_image_fit_mode(stable_id, value)?;
                return Ok(());
            }
            "motion_blur" => {
                data.motion_blur = float_value(value);
                return Ok(());
            }
            "edge_softness" => {
                data.edge_softness = float_value(value);
                return Ok(());
            }
            "dof_focal_distance" => {
                data.dof_focal_distance = float_value(value);
                return Ok(());
            }
            "dof_aperture" => {
                data.dof_aperture = float_value(value);
                return Ok(());
            }
            "size_multiplier" => {
                data.size_multiplier = float_value(value);
                return Ok(());
            }
            "alpha_clip" => {
                data.alpha_clip = float_value(value);
                return Ok(());
            }
            "dof_enabled" => {
                data.dof_enabled = bool_value(value);
                return Ok(());
            }
            "composite_on_original" => {
                data.composite_on_original = bool_value(value);
                return Ok(());
            }
            "use_source_alpha" => {
                data.use_source_alpha = bool_value(value);
                return Ok(());
            }
            "source_premultiplied" => {
                data.source_premultiplied = bool_value(value);
                return Ok(());
            }
            _ => {}
        },
        ParticleGraphNode::Child(data) => match socket {
            "enabled" => {
                data.enabled = bool_value(value);
                return Ok(());
            }
            "count" => {
                data.count = integer_value(value).max(0) as u32;
                return Ok(());
            }
            "inherit_velocity" => {
                data.inherit_velocity = float_value(value);
                return Ok(());
            }
            "lifespan" => {
                data.lifespan = float_value(value);
                return Ok(());
            }
            "initial_speed" => {
                data.initial_speed = float_value(value);
                return Ok(());
            }
            "spread_degrees" => {
                data.spread_degrees = float_value(value);
                return Ok(());
            }
            "size_scale" => {
                data.size_scale = float_value(value);
                return Ok(());
            }
            _ => {}
        },
        ParticleGraphNode::System(data) => {
            if socket == "seed" {
                data.seed = integer_value(value).max(0) as u64;
                return Ok(());
            }
        }
    }

    Err(GraphPublishedParamError::UnsupportedSocket {
        node: NodeId(0),
        socket: socket.to_string(),
    })
}

fn float_value(value: &GraphPublishedValue) -> f32 {
    match value {
        GraphPublishedValue::Float(value) => *value,
        _ => unreachable!("published value type checked before node application"),
    }
}

fn integer_value(value: &GraphPublishedValue) -> i32 {
    match value {
        GraphPublishedValue::Integer(value) => *value,
        _ => unreachable!("published value type checked before node application"),
    }
}

fn bool_value(value: &GraphPublishedValue) -> bool {
    match value {
        GraphPublishedValue::Boolean(value) => *value,
        _ => unreachable!("published value type checked before node application"),
    }
}

fn color_value(value: &GraphPublishedValue) -> [f32; 4] {
    match value {
        GraphPublishedValue::Color(value) => *value,
        _ => unreachable!("published value type checked before node application"),
    }
}

fn vector3_value(value: &GraphPublishedValue) -> [f32; 3] {
    match value {
        GraphPublishedValue::Vector3(value) => *value,
        _ => unreachable!("published value type checked before node application"),
    }
}

fn vector3_to_resolution(value: [f32; 3]) -> [u32; 3] {
    [
        value[0].round().max(1.0) as u32,
        value[1].round().max(1.0) as u32,
        value[2].round().max(1.0) as u32,
    ]
}

fn enum_str<'a>(
    stable_id: &str,
    value: &'a GraphPublishedValue,
) -> Result<&'a str, GraphPublishedParamError> {
    match value {
        GraphPublishedValue::Enum(value) => Ok(value.as_str()),
        _ => unreachable!("published value type checked before node application"),
    }
    .and_then(|value| {
        if value.trim().is_empty() {
            Err(GraphPublishedParamError::InvalidEnumValue {
                stable_id: stable_id.to_string(),
                value: value.to_string(),
            })
        } else {
            Ok(value)
        }
    })
}

fn invalid_enum<T>(stable_id: &str, value: &str) -> Result<T, GraphPublishedParamError> {
    Err(GraphPublishedParamError::InvalidEnumValue {
        stable_id: stable_id.to_string(),
        value: value.to_string(),
    })
}

fn enum_emitter_type(
    stable_id: &str,
    value: &GraphPublishedValue,
) -> Result<GraphEmitterType, GraphPublishedParamError> {
    match enum_str(stable_id, value)? {
        "point" => Ok(GraphEmitterType::Point),
        "box" => Ok(GraphEmitterType::Box),
        "sphere" => Ok(GraphEmitterType::Sphere),
        "grid" => Ok(GraphEmitterType::Grid),
        "layer_alpha" => Ok(GraphEmitterType::LayerAlpha),
        "path" => Ok(GraphEmitterType::Path),
        value => invalid_enum(stable_id, value),
    }
}

fn enum_particle_shape(
    stable_id: &str,
    value: &GraphPublishedValue,
) -> Result<GraphParticleShape, GraphPublishedParamError> {
    match enum_str(stable_id, value)? {
        "circle" => Ok(GraphParticleShape::Circle),
        "square" => Ok(GraphParticleShape::Square),
        "triangle" => Ok(GraphParticleShape::Triangle),
        "star" => Ok(GraphParticleShape::Star),
        "line" => Ok(GraphParticleShape::Line),
        "image" => Ok(GraphParticleShape::Image),
        value => invalid_enum(stable_id, value),
    }
}

fn enum_blend_mode(
    stable_id: &str,
    value: &GraphPublishedValue,
) -> Result<GraphBlendMode, GraphPublishedParamError> {
    match enum_str(stable_id, value)? {
        "normal" => Ok(GraphBlendMode::Normal),
        "add" => Ok(GraphBlendMode::Add),
        "screen" => Ok(GraphBlendMode::Screen),
        value => invalid_enum(stable_id, value),
    }
}

fn enum_apply_mode(
    stable_id: &str,
    value: &GraphPublishedValue,
) -> Result<GraphApplyMode, GraphPublishedParamError> {
    match enum_str(stable_id, value)? {
        "on_transparent" => Ok(GraphApplyMode::OnTransparent),
        "normal" => Ok(GraphApplyMode::Normal),
        "add" => Ok(GraphApplyMode::Add),
        "screen" => Ok(GraphApplyMode::Screen),
        value => invalid_enum(stable_id, value),
    }
}

fn enum_time_sampling(
    stable_id: &str,
    value: &GraphPublishedValue,
) -> Result<GraphTimeSamplingMode, GraphPublishedParamError> {
    match enum_str(stable_id, value)? {
        "current_time" => Ok(GraphTimeSamplingMode::CurrentTime),
        "birth_time" => Ok(GraphTimeSamplingMode::BirthTime),
        "random_still" => Ok(GraphTimeSamplingMode::RandomStill),
        "random_play" => Ok(GraphTimeSamplingMode::RandomPlay),
        "cycle" => Ok(GraphTimeSamplingMode::Cycle),
        value => invalid_enum(stable_id, value),
    }
}

fn enum_image_color_mode(
    stable_id: &str,
    value: &GraphPublishedValue,
) -> Result<GraphImageColorMode, GraphPublishedParamError> {
    match enum_str(stable_id, value)? {
        "tint" => Ok(GraphImageColorMode::Tint),
        "source" => Ok(GraphImageColorMode::Source),
        value => invalid_enum(stable_id, value),
    }
}

fn enum_image_fit_mode(
    stable_id: &str,
    value: &GraphPublishedValue,
) -> Result<GraphImageFitMode, GraphPublishedParamError> {
    match enum_str(stable_id, value)? {
        "contain" => Ok(GraphImageFitMode::Contain),
        "stretch" => Ok(GraphImageFitMode::Stretch),
        value => invalid_enum(stable_id, value),
    }
}

fn published_socket_type(
    kind: &ParticleGraphNode,
    socket: &str,
) -> Option<GraphPublishedValueType> {
    match kind {
        ParticleGraphNode::Output(_) => None,
        ParticleGraphNode::Emitter(_) => match socket {
            "emitter_type" => Some(GraphPublishedValueType::Enum),
            "position" | "size" | "grid_resolution" => Some(GraphPublishedValueType::Vector3),
            "birth_rate" | "lifespan" | "lifespan_variation" => {
                Some(GraphPublishedValueType::Float)
            }
            "emit_all_at_start" => Some(GraphPublishedValueType::Boolean),
            _ => None,
        },
        ParticleGraphNode::PointEmitter(_) => match socket {
            "position" | "grid_resolution" => Some(GraphPublishedValueType::Vector3),
            "birth_rate" | "lifespan" | "lifespan_variation" => {
                Some(GraphPublishedValueType::Float)
            }
            "emit_all_at_start" => Some(GraphPublishedValueType::Boolean),
            _ => None,
        },
        ParticleGraphNode::Motion(_) => match socket {
            "initial_direction" => Some(GraphPublishedValueType::Vector3),
            "initial_speed" | "speed_variation" | "spread_degrees" | "initial_size"
            | "size_variation" | "initial_rotation" | "rotation_variation" | "rotation_speed"
            | "opacity_variation" => Some(GraphPublishedValueType::Float),
            "sprite_frame_count" | "sprite_time_sampling" => Some(GraphPublishedValueType::Integer),
            _ => None,
        },
        ParticleGraphNode::Physics(_) => match socket {
            "gravity" | "wind" => Some(GraphPublishedValueType::Vector3),
            "air_resistance"
            | "turbulence_strength"
            | "turbulence_scale"
            | "turbulence_speed"
            | "bounce_floor_y"
            | "bounce_damping" => Some(GraphPublishedValueType::Float),
            "bounce_enabled" => Some(GraphPublishedValueType::Boolean),
            _ => None,
        },
        ParticleGraphNode::Appearance(_) => match socket {
            "color_start" | "color_end" => Some(GraphPublishedValueType::Color),
            "size_over_life" | "opacity_over_life" => Some(GraphPublishedValueType::Color),
            _ => None,
        },
        ParticleGraphNode::Render(_) => match socket {
            "shape" | "blend_mode" | "apply_mode" | "time_sampling" | "image_color_mode"
            | "image_fit_mode" => Some(GraphPublishedValueType::Enum),
            "motion_blur" | "edge_softness" | "dof_focal_distance" | "dof_aperture"
            | "size_multiplier" | "alpha_clip" => Some(GraphPublishedValueType::Float),
            "dof_enabled"
            | "composite_on_original"
            | "use_source_alpha"
            | "source_premultiplied" => Some(GraphPublishedValueType::Boolean),
            _ => None,
        },
        ParticleGraphNode::Child(_) => match socket {
            "enabled" => Some(GraphPublishedValueType::Boolean),
            "count" => Some(GraphPublishedValueType::Integer),
            "inherit_velocity" | "lifespan" | "initial_speed" | "spread_degrees" | "size_scale" => {
                Some(GraphPublishedValueType::Float)
            }
            _ => None,
        },
        ParticleGraphNode::System(_) => match socket {
            "seed" => Some(GraphPublishedValueType::Integer),
            _ => None,
        },
    }
}

fn graph_emitter_type(value: EmitterType) -> GraphEmitterType {
    match value {
        EmitterType::Point => GraphEmitterType::Point,
        EmitterType::Box => GraphEmitterType::Box,
        EmitterType::Sphere => GraphEmitterType::Sphere,
        EmitterType::Grid => GraphEmitterType::Grid,
        EmitterType::LayerAlpha => GraphEmitterType::LayerAlpha,
        EmitterType::Path => GraphEmitterType::Path,
    }
}

fn graph_particle_shape(value: ParticleShape) -> GraphParticleShape {
    match value {
        ParticleShape::Circle => GraphParticleShape::Circle,
        ParticleShape::Square => GraphParticleShape::Square,
        ParticleShape::Triangle => GraphParticleShape::Triangle,
        ParticleShape::Star => GraphParticleShape::Star,
        ParticleShape::Line => GraphParticleShape::Line,
        ParticleShape::Image => GraphParticleShape::Image,
    }
}

fn graph_blend_mode(value: BlendMode) -> GraphBlendMode {
    match value {
        BlendMode::Normal => GraphBlendMode::Normal,
        BlendMode::Add => GraphBlendMode::Add,
        BlendMode::Screen => GraphBlendMode::Screen,
    }
}

fn graph_apply_mode(value: ApplyMode) -> GraphApplyMode {
    match value {
        ApplyMode::OnTransparent => GraphApplyMode::OnTransparent,
        ApplyMode::Normal => GraphApplyMode::Normal,
        ApplyMode::Add => GraphApplyMode::Add,
        ApplyMode::Screen => GraphApplyMode::Screen,
    }
}

fn graph_time_sampling(value: TimeSamplingMode) -> GraphTimeSamplingMode {
    match value {
        TimeSamplingMode::CurrentTime => GraphTimeSamplingMode::CurrentTime,
        TimeSamplingMode::BirthTime => GraphTimeSamplingMode::BirthTime,
        TimeSamplingMode::RandomStill => GraphTimeSamplingMode::RandomStill,
        TimeSamplingMode::RandomPlay => GraphTimeSamplingMode::RandomPlay,
        TimeSamplingMode::Cycle => GraphTimeSamplingMode::Cycle,
    }
}

fn graph_image_color_mode(value: ImageColorMode) -> GraphImageColorMode {
    match value {
        ImageColorMode::Tint => GraphImageColorMode::Tint,
        ImageColorMode::Source => GraphImageColorMode::Source,
    }
}

fn graph_image_fit_mode(value: ImageFitMode) -> GraphImageFitMode {
    match value {
        ImageFitMode::Contain => GraphImageFitMode::Contain,
        ImageFitMode::Stretch => GraphImageFitMode::Stretch,
    }
}

fn node(id: NodeId, label: &str, kind: ParticleGraphNode) -> GraphNode {
    GraphNode {
        id,
        version: 1,
        label: label.to_string(),
        kind,
    }
}

fn edge(from: NodeId, from_socket: &str, to: NodeId, to_socket: &str) -> GraphEdge {
    GraphEdge {
        from: GraphSocket {
            node: from,
            socket: from_socket.to_string(),
        },
        to: GraphSocket {
            node: to,
            socket: to_socket.to_string(),
        },
    }
}

fn node_map(document: &GraphDocument) -> Result<HashMap<NodeId, &GraphNode>, GraphCompileError> {
    node_graph_core::node_map(&document.nodes, GraphCompileError::DuplicateNodeId)
}

fn output_input<'a>(
    document: &'a GraphDocument,
    nodes: &HashMap<NodeId, &'a GraphNode>,
    output: NodeId,
    socket: &'static str,
) -> Result<Option<&'a GraphNode>, GraphCompileError> {
    node_graph_core::output_input(
        &document.edges,
        nodes,
        output,
        socket,
        GraphCompileError::UnknownNode,
    )
}

fn wrong<T>(
    node: NodeId,
    socket: &'static str,
    expected: &'static str,
) -> Result<T, GraphCompileError> {
    Err(GraphCompileError::WrongNodeKind {
        node,
        socket,
        expected,
    })
}

fn apply_emitter(config: &mut ParticleEngineConfig, data: &EmitterNode) {
    config.emitter.emitter_type = match data.emitter_type {
        GraphEmitterType::Point => EmitterType::Point,
        GraphEmitterType::Box => EmitterType::Box,
        GraphEmitterType::Sphere => EmitterType::Sphere,
        GraphEmitterType::Grid => EmitterType::Grid,
        GraphEmitterType::LayerAlpha => EmitterType::LayerAlpha,
        GraphEmitterType::Path => EmitterType::Path,
    };
    config.emitter.position = glam::Vec3::from_array(data.position);
    config.emitter.size = glam::Vec3::from_array(data.size);
    config.emitter.source_points = None;
    config.emitter.birth_rate = data.birth_rate;
    config.emitter.lifespan = data.lifespan;
    config.emitter.lifespan_variation = data.lifespan_variation;
    config.emitter.emit_all_at_start = data.emit_all_at_start;
    config.emitter.grid_res_x = data.grid_resolution[0].max(1);
    config.emitter.grid_res_y = data.grid_resolution[1].max(1);
    config.emitter.grid_res_z = data.grid_resolution[2].max(1);
}

fn apply_point_emitter(config: &mut ParticleEngineConfig, data: &PointEmitterNode) {
    config.emitter.emitter_type = EmitterType::Point;
    config.emitter.position = glam::Vec3::from_array(data.position);
    config.emitter.source_points = None;
    config.emitter.birth_rate = data.birth_rate;
    config.emitter.lifespan = data.lifespan;
    config.emitter.lifespan_variation = data.lifespan_variation;
    config.emitter.emit_all_at_start = data.emit_all_at_start;
    config.emitter.grid_res_x = data.grid_resolution[0].max(1);
    config.emitter.grid_res_y = data.grid_resolution[1].max(1);
    config.emitter.grid_res_z = data.grid_resolution[2].max(1);
}

fn apply_motion(config: &mut ParticleEngineConfig, data: &MotionNode) {
    config.emitter.initial_speed = data.initial_speed;
    config.emitter.speed_variation = data.speed_variation;
    config.emitter.initial_direction = glam::Vec3::from_array(data.initial_direction);
    config.emitter.spread = data.spread_degrees.to_radians();
    config.emitter.initial_size = data.initial_size;
    config.emitter.size_variation = data.size_variation;
    config.emitter.initial_rotation = data.initial_rotation;
    config.emitter.rotation_variation = data.rotation_variation;
    config.emitter.rotation_speed = data.rotation_speed;
    config.emitter.opacity_variation = data.opacity_variation;
    config.emitter.sprite_frame_count = data.sprite_frame_count.max(1);
    config.emitter.sprite_time_sampling = data.sprite_time_sampling;
}

fn apply_physics(config: &mut ParticleEngineConfig, data: &PhysicsNode) {
    config.physics.gravity = glam::Vec3::from_array(data.gravity);
    config.physics.wind = glam::Vec3::from_array(data.wind);
    config.physics.air_resistance = data.air_resistance;
    config.physics.turbulence_strength = data.turbulence_strength;
    config.physics.turbulence_scale = data.turbulence_scale;
    config.physics.turbulence_speed = data.turbulence_speed;
    config.physics.bounce_floor_y = data.bounce_floor_y;
    config.physics.bounce_enabled = data.bounce_enabled;
    config.physics.bounce_damping = data.bounce_damping;
}

fn apply_appearance(config: &mut ParticleEngineConfig, data: &AppearanceNode) {
    config.appearance.color_start = data.color_start;
    config.appearance.color_end = data.color_end;
    config.appearance.size_over_life = data.size_over_life;
    config.appearance.opacity_over_life = data.opacity_over_life;
}

fn apply_render(config: &mut ParticleEngineConfig, data: &RenderNode) {
    config.render.shape = match data.shape {
        GraphParticleShape::Circle => ParticleShape::Circle,
        GraphParticleShape::Square => ParticleShape::Square,
        GraphParticleShape::Triangle => ParticleShape::Triangle,
        GraphParticleShape::Star => ParticleShape::Star,
        GraphParticleShape::Line => ParticleShape::Line,
        GraphParticleShape::Image => ParticleShape::Image,
    };
    config.render.blend_mode = match data.blend_mode {
        GraphBlendMode::Normal => BlendMode::Normal,
        GraphBlendMode::Add => BlendMode::Add,
        GraphBlendMode::Screen => BlendMode::Screen,
    };
    config.render.motion_blur = data.motion_blur;
    config.render.edge_softness = data.edge_softness;
    config.render.dof_enabled = data.dof_enabled;
    config.render.dof_focal_distance = data.dof_focal_distance;
    config.render.dof_aperture = data.dof_aperture;
    config.render.composite_on_original = data.composite_on_original;
    config.render.apply_mode = match data.apply_mode {
        GraphApplyMode::OnTransparent => ApplyMode::OnTransparent,
        GraphApplyMode::Normal => ApplyMode::Normal,
        GraphApplyMode::Add => ApplyMode::Add,
        GraphApplyMode::Screen => ApplyMode::Screen,
    };
    config.render.size_multiplier = data.size_multiplier;
    config.render.time_sampling = match data.time_sampling {
        GraphTimeSamplingMode::CurrentTime => TimeSamplingMode::CurrentTime,
        GraphTimeSamplingMode::BirthTime => TimeSamplingMode::BirthTime,
        GraphTimeSamplingMode::RandomStill => TimeSamplingMode::RandomStill,
        GraphTimeSamplingMode::RandomPlay => TimeSamplingMode::RandomPlay,
        GraphTimeSamplingMode::Cycle => TimeSamplingMode::Cycle,
    };
    config.render.image_color_mode = match data.image_color_mode {
        GraphImageColorMode::Tint => ImageColorMode::Tint,
        GraphImageColorMode::Source => ImageColorMode::Source,
    };
    config.render.image_fit_mode = match data.image_fit_mode {
        GraphImageFitMode::Contain => ImageFitMode::Contain,
        GraphImageFitMode::Stretch => ImageFitMode::Stretch,
    };
    config.render.image_sampling.use_source_alpha = data.use_source_alpha;
    config.render.image_sampling.source_premultiplied = data.source_premultiplied;
    config.render.image_sampling.alpha_clip = data.alpha_clip;
}

fn apply_child(config: &mut ParticleEngineConfig, data: &ChildNode) {
    config.child.enabled = data.enabled;
    config.child.count = data.count;
    config.child.inherit_velocity = data.inherit_velocity;
    config.child.lifespan = data.lifespan;
    config.child.initial_speed = data.initial_speed;
    config.child.spread = data.spread_degrees.to_radians();
    config.child.size_scale = data.size_scale;
}

fn apply_system(config: &mut ParticleEngineConfig, data: &SystemNode) {
    config.seed = data.seed;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(a: f32, b: f32) {
        assert!((a - b).abs() < 0.0001, "{a} != {b}");
    }

    fn assert_vec3_close(a: glam::Vec3, b: glam::Vec3) {
        assert!((a - b).length() < 0.0001, "{a:?} != {b:?}");
    }

    fn assert_arr4_close(a: [f32; 4], b: [f32; 4]) {
        for (&a, &b) in a.iter().zip(b.iter()) {
            assert_close(a, b);
        }
    }

    fn catalog_entry<'a>(catalog: &'a NodeUiCatalog, node_type: &str) -> &'a NodeUiCatalogEntry {
        catalog
            .nodes
            .iter()
            .find(|entry| entry.node_type == node_type)
            .unwrap_or_else(|| panic!("missing catalog node type {node_type}"))
    }

    fn assert_runtime_independent_engine_matches(
        actual: &ParticleEngineConfig,
        expected: &ParticleEngineConfig,
    ) {
        assert_eq!(
            std::mem::discriminant(&actual.emitter.emitter_type),
            std::mem::discriminant(&expected.emitter.emitter_type)
        );
        assert_vec3_close(actual.emitter.position, expected.emitter.position);
        assert_vec3_close(actual.emitter.size, expected.emitter.size);
        assert_close(actual.emitter.birth_rate, expected.emitter.birth_rate);
        assert_close(actual.emitter.lifespan, expected.emitter.lifespan);
        assert_close(
            actual.emitter.lifespan_variation,
            expected.emitter.lifespan_variation,
        );
        assert_close(actual.emitter.initial_speed, expected.emitter.initial_speed);
        assert_close(
            actual.emitter.speed_variation,
            expected.emitter.speed_variation,
        );
        assert_vec3_close(
            actual.emitter.initial_direction,
            expected.emitter.initial_direction,
        );
        assert_close(actual.emitter.spread, expected.emitter.spread);
        assert_close(actual.emitter.initial_size, expected.emitter.initial_size);
        assert_close(
            actual.emitter.size_variation,
            expected.emitter.size_variation,
        );
        assert_close(
            actual.emitter.initial_rotation,
            expected.emitter.initial_rotation,
        );
        assert_close(
            actual.emitter.rotation_variation,
            expected.emitter.rotation_variation,
        );
        assert_close(
            actual.emitter.rotation_speed,
            expected.emitter.rotation_speed,
        );
        assert_eq!(
            actual.emitter.sprite_frame_count,
            expected.emitter.sprite_frame_count
        );
        assert_eq!(
            actual.emitter.sprite_time_sampling,
            expected.emitter.sprite_time_sampling
        );
        assert_close(
            actual.emitter.opacity_variation,
            expected.emitter.opacity_variation,
        );
        assert_eq!(actual.emitter.grid_res_x, expected.emitter.grid_res_x);
        assert_eq!(actual.emitter.grid_res_y, expected.emitter.grid_res_y);
        assert_eq!(actual.emitter.grid_res_z, expected.emitter.grid_res_z);
        assert_eq!(
            actual.emitter.emit_all_at_start,
            expected.emitter.emit_all_at_start
        );

        assert_vec3_close(actual.physics.gravity, expected.physics.gravity);
        assert_vec3_close(actual.physics.wind, expected.physics.wind);
        assert_close(
            actual.physics.air_resistance,
            expected.physics.air_resistance,
        );
        assert_close(
            actual.physics.turbulence_strength,
            expected.physics.turbulence_strength,
        );
        assert_close(
            actual.physics.turbulence_scale,
            expected.physics.turbulence_scale,
        );
        assert_close(
            actual.physics.turbulence_speed,
            expected.physics.turbulence_speed,
        );
        assert_close(
            actual.physics.bounce_floor_y,
            expected.physics.bounce_floor_y,
        );
        assert_eq!(
            actual.physics.bounce_enabled,
            expected.physics.bounce_enabled
        );
        assert_close(
            actual.physics.bounce_damping,
            expected.physics.bounce_damping,
        );

        assert_arr4_close(
            actual.appearance.color_start,
            expected.appearance.color_start,
        );
        assert_arr4_close(actual.appearance.color_end, expected.appearance.color_end);
        assert_arr4_close(
            actual.appearance.size_over_life,
            expected.appearance.size_over_life,
        );
        assert_arr4_close(
            actual.appearance.opacity_over_life,
            expected.appearance.opacity_over_life,
        );

        assert_eq!(actual.child.enabled, expected.child.enabled);
        assert_eq!(actual.child.count, expected.child.count);
        assert_close(
            actual.child.inherit_velocity,
            expected.child.inherit_velocity,
        );
        assert_close(actual.child.lifespan, expected.child.lifespan);
        assert_close(actual.child.initial_speed, expected.child.initial_speed);
        assert_close(actual.child.spread, expected.child.spread);
        assert_close(actual.child.size_scale, expected.child.size_scale);

        assert_eq!(
            std::mem::discriminant(&actual.render.shape),
            std::mem::discriminant(&expected.render.shape)
        );
        assert_eq!(
            std::mem::discriminant(&actual.render.blend_mode),
            std::mem::discriminant(&expected.render.blend_mode)
        );
        assert_close(actual.render.motion_blur, expected.render.motion_blur);
        assert_close(actual.render.edge_softness, expected.render.edge_softness);
        assert_eq!(actual.render.dof_enabled, expected.render.dof_enabled);
        assert_close(
            actual.render.dof_focal_distance,
            expected.render.dof_focal_distance,
        );
        assert_close(actual.render.dof_aperture, expected.render.dof_aperture);
        assert_eq!(
            actual.render.composite_on_original,
            expected.render.composite_on_original
        );
        assert_eq!(
            std::mem::discriminant(&actual.render.apply_mode),
            std::mem::discriminant(&expected.render.apply_mode)
        );
        assert_close(
            actual.render.size_multiplier,
            expected.render.size_multiplier,
        );
        assert_eq!(
            std::mem::discriminant(&actual.render.time_sampling),
            std::mem::discriminant(&expected.render.time_sampling)
        );
        assert_eq!(
            std::mem::discriminant(&actual.render.image_color_mode),
            std::mem::discriminant(&expected.render.image_color_mode)
        );
        assert_eq!(
            std::mem::discriminant(&actual.render.image_fit_mode),
            std::mem::discriminant(&expected.render.image_fit_mode)
        );
        assert_eq!(
            actual.render.image_sampling.use_source_alpha,
            expected.render.image_sampling.use_source_alpha
        );
        assert_eq!(
            actual.render.image_sampling.source_premultiplied,
            expected.render.image_sampling.source_premultiplied
        );
        assert_close(
            actual.render.image_sampling.alpha_clip,
            expected.render.image_sampling.alpha_clip,
        );
        assert_eq!(actual.seed, expected.seed);
    }

    fn published_float(stable_id: &str, node: NodeId, socket: &str) -> GraphPublishedParam {
        GraphPublishedParam {
            stable_id: stable_id.to_string(),
            label: stable_id.to_string(),
            target: GraphSocket {
                node,
                socket: socket.to_string(),
            },
            value_type: GraphPublishedValueType::Float,
            default_value: GraphPublishedValue::Float(1.0),
        }
    }

    #[test]
    fn particle_node_ui_catalog_describes_current_graph_surface() {
        let catalog = particle_node_ui_catalog();

        assert_eq!(
            catalog.catalog_version,
            node_graph_core::NODE_UI_CATALOG_VERSION
        );
        assert_eq!(catalog.graph_schema_version, GRAPH_SCHEMA_VERSION);
        assert_eq!(catalog.product_id, "particlelab");
        assert_eq!(catalog.nodes.len(), 9);

        let output = catalog_entry(&catalog, "particle.output");
        assert!(output.output_sockets.is_empty());
        assert!(output
            .input_sockets
            .iter()
            .any(|socket| socket.socket == "emitter"));
        assert!(output
            .input_sockets
            .iter()
            .any(|socket| socket.socket == "system"));

        let emitter = catalog_entry(&catalog, "particle.emitter");
        let emitter_type = emitter
            .value_sockets
            .iter()
            .find(|socket| socket.socket == "emitter_type")
            .unwrap();
        assert_eq!(emitter_type.value_type, NodeUiValueType::Enum);
        assert!(emitter_type
            .enum_options
            .iter()
            .any(|option| option.value == "layer_alpha"));
        assert!(emitter
            .value_sockets
            .iter()
            .any(|socket| socket.socket == "birth_rate"
                && socket.value_type == NodeUiValueType::Float));

        let render = catalog_entry(&catalog, "particle.render");
        assert!(render
            .value_sockets
            .iter()
            .any(|socket| socket.socket == "apply_mode"
                && socket
                    .enum_options
                    .iter()
                    .any(|option| option.value == "on_transparent")));

        let json = serde_json::to_string(&catalog).unwrap();
        assert!(json.contains("\"particle.render\""));
        assert!(json.contains("\"catalog_version\":1"));
    }

    #[test]
    fn particle_node_ui_catalog_value_sockets_match_published_socket_validation() {
        let catalog = particle_node_ui_catalog();
        let mut samples = std::collections::HashMap::new();

        for document in [
            GraphDocument::from_engine_config(&ParticleEngineConfig::classic_default()),
            GraphDocument::simple_point_emitter(),
        ] {
            for node in document.nodes {
                samples
                    .entry(node.kind.node_type().to_string())
                    .or_insert(node.kind);
            }
        }

        for entry in &catalog.nodes {
            let Some(kind) = samples.get(&entry.node_type) else {
                assert!(
                    entry.value_sockets.is_empty(),
                    "missing sample for catalog node {}",
                    entry.node_type
                );
                continue;
            };

            for socket in &entry.value_sockets {
                assert_eq!(
                    published_socket_type(kind, &socket.socket).map(node_ui_value_type),
                    Some(socket.value_type),
                    "{}.{}",
                    entry.node_type,
                    socket.socket
                );
                if socket.value_type == NodeUiValueType::Enum {
                    assert!(
                        !socket.enum_options.is_empty(),
                        "{}.{} enum socket lacks options",
                        entry.node_type,
                        socket.socket
                    );
                } else {
                    assert!(
                        socket.enum_options.is_empty(),
                        "{}.{} non-enum socket has options",
                        entry.node_type,
                        socket.socket
                    );
                }
            }
        }
    }

    #[test]
    fn simple_point_emitter_graph_compiles_to_classic_default() {
        let document = GraphDocument::simple_point_emitter();
        let config = compile_particle_graph(&document).unwrap();
        let default = ParticleEngineConfig::classic_default();

        assert!(matches!(config.emitter.emitter_type, EmitterType::Point));
        assert_vec3_close(config.emitter.position, default.emitter.position);
        assert_eq!(config.emitter.birth_rate, default.emitter.birth_rate);
        assert_eq!(config.emitter.sprite_frame_count, 1);
        assert_eq!(config.physics.gravity, default.physics.gravity);
        assert_eq!(
            config.appearance.opacity_over_life,
            default.appearance.opacity_over_life
        );
        assert!(matches!(config.render.shape, ParticleShape::Circle));
        assert_eq!(config.render.apply_mode, ApplyMode::Normal);
        assert_eq!(config.child.enabled, default.child.enabled);
    }

    #[test]
    fn graph_document_roundtrips_as_versioned_json() {
        let document = GraphDocument::simple_point_emitter();
        let json = serde_json::to_string(&document).unwrap();
        assert!(json.contains("\"schema_version\":1"));
        assert!(json.contains("particle.emitter.point"));
        assert!(json.contains("published_params"));

        let decoded: GraphDocument = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.schema_version, GRAPH_SCHEMA_VERSION);
        assert_eq!(decoded.nodes.len(), document.nodes.len());
        assert!(decoded.published_params.is_empty());
        compile_particle_graph(&decoded).unwrap();
    }

    #[test]
    fn missing_published_params_defaults_to_empty_for_old_graph_json() {
        let document = GraphDocument::simple_point_emitter();
        let mut value = serde_json::to_value(&document).unwrap();
        value.as_object_mut().unwrap().remove("published_params");

        let decoded: GraphDocument = serde_json::from_value(value).unwrap();
        assert!(decoded.published_params.is_empty());
        validate_published_params(&decoded).unwrap();
    }

    #[test]
    fn graph_document_from_value_migrates_old_graph_json_shape() {
        let document = GraphDocument::simple_point_emitter();
        let mut value = serde_json::to_value(&document).unwrap();
        let obj = value.as_object_mut().unwrap();
        obj.remove("schema_version");
        obj.remove("published_params");
        for node in obj.get_mut("nodes").unwrap().as_array_mut().unwrap() {
            let node_obj = node.as_object_mut().unwrap();
            let node_type = node_obj
                .get("type")
                .and_then(Value::as_str)
                .unwrap()
                .strip_prefix("particle.")
                .unwrap()
                .to_string();
            node_obj.insert("type".to_string(), Value::from(node_type));
            node_obj.remove("version");
        }

        let decoded = GraphDocument::from_value(value).unwrap();

        assert_eq!(decoded.schema_version, GRAPH_SCHEMA_VERSION);
        assert!(decoded.published_params.is_empty());
        assert!(decoded.nodes.iter().all(|node| node.version == 1));
        compile_particle_graph(&decoded).unwrap();
    }

    #[test]
    fn graph_document_from_value_keeps_future_schema_for_compiler_rejection() {
        let document = GraphDocument::simple_point_emitter();
        let mut value = serde_json::to_value(&document).unwrap();
        value.as_object_mut().unwrap().insert(
            "schema_version".to_string(),
            Value::from(GRAPH_SCHEMA_VERSION + 100),
        );

        let decoded = GraphDocument::from_value(value).unwrap();
        let err = compile_particle_graph(&decoded).unwrap_err();

        assert_eq!(
            err,
            GraphCompileError::UnsupportedSchemaVersion(GRAPH_SCHEMA_VERSION + 100)
        );
    }

    #[test]
    fn engine_config_exports_to_graph_document_and_compiles_back() {
        let mut config = ParticleEngineConfig::classic_default();
        config.emitter.emitter_type = EmitterType::Grid;
        config.emitter.position = glam::Vec3::new(120.0, 45.0, 8.0);
        config.emitter.size = glam::Vec3::new(80.0, 40.0, 20.0);
        config.emitter.birth_rate = 333.0;
        config.emitter.lifespan = 2.5;
        config.emitter.lifespan_variation = 0.33;
        config.emitter.initial_speed = 77.0;
        config.emitter.speed_variation = 0.42;
        config.emitter.initial_direction = glam::Vec3::new(1.0, 0.25, 0.5);
        config.emitter.spread = 35.0_f32.to_radians();
        config.emitter.initial_size = 12.0;
        config.emitter.size_variation = 0.5;
        config.emitter.initial_rotation = 15.0;
        config.emitter.rotation_variation = 20.0;
        config.emitter.rotation_speed = 8.0;
        config.emitter.opacity_variation = 0.7;
        config.emitter.sprite_frame_count = 6;
        config.emitter.sprite_time_sampling = 3;
        config.emitter.grid_res_x = 12;
        config.emitter.grid_res_y = 5;
        config.emitter.grid_res_z = 2;
        config.emitter.emit_all_at_start = true;
        config.physics.gravity = glam::Vec3::new(0.0, 220.0, 4.0);
        config.physics.wind = glam::Vec3::new(12.0, -3.0, 2.0);
        config.physics.air_resistance = 0.12;
        config.physics.turbulence_strength = 44.0;
        config.physics.turbulence_scale = 1.5;
        config.physics.turbulence_speed = 0.25;
        config.physics.bounce_floor_y = 900.0;
        config.physics.bounce_enabled = true;
        config.physics.bounce_damping = 0.4;
        config.appearance.color_start = [0.2, 0.3, 0.4, 0.8];
        config.appearance.color_end = [0.9, 0.8, 0.7, 0.2];
        config.appearance.size_over_life = [0.1, 0.8, 1.1, 0.0];
        config.appearance.opacity_over_life = [0.0, 1.0, 0.6, 0.1];
        config.child.enabled = true;
        config.child.count = 9;
        config.child.inherit_velocity = 0.22;
        config.child.lifespan = 0.9;
        config.child.initial_speed = 55.0;
        config.child.spread = 45.0_f32.to_radians();
        config.child.size_scale = 0.25;
        config.render.shape = ParticleShape::Image;
        config.render.blend_mode = BlendMode::Screen;
        config.render.motion_blur = 0.65;
        config.render.edge_softness = 0.1;
        config.render.dof_enabled = true;
        config.render.dof_focal_distance = 700.0;
        config.render.dof_aperture = 2.2;
        config.render.composite_on_original = false;
        config.render.apply_mode = ApplyMode::Add;
        config.render.size_multiplier = 1.8;
        config.render.time_sampling = TimeSamplingMode::RandomPlay;
        config.render.image_color_mode = ImageColorMode::Source;
        config.render.image_fit_mode = ImageFitMode::Stretch;
        config.render.image_sampling.use_source_alpha = false;
        config.render.image_sampling.source_premultiplied = false;
        config.render.image_sampling.alpha_clip = 0.2;
        config.seed = 8765;

        let document = GraphDocument::from_engine_config(&config);
        let json = serde_json::to_string(&document).unwrap();
        assert!(json.contains("particle.emitter"));
        assert!(json.contains("particle.system"));

        let compiled = compile_particle_graph(&document).unwrap();
        assert_runtime_independent_engine_matches(&compiled, &config);
    }

    #[test]
    fn generic_emitter_node_preserves_all_emitter_types() {
        for emitter_type in [
            EmitterType::Point,
            EmitterType::Box,
            EmitterType::Sphere,
            EmitterType::Grid,
            EmitterType::LayerAlpha,
            EmitterType::Path,
        ] {
            let mut config = ParticleEngineConfig::classic_default();
            config.emitter.emitter_type = emitter_type;
            config.emitter.size = glam::Vec3::new(10.0, 20.0, 30.0);

            let document = GraphDocument::from_engine_config(&config);
            let compiled = compile_particle_graph(&document).unwrap();

            assert_eq!(
                std::mem::discriminant(&compiled.emitter.emitter_type),
                std::mem::discriminant(&emitter_type)
            );
            assert_vec3_close(compiled.emitter.size, config.emitter.size);
        }
    }

    #[test]
    fn published_param_validation_accepts_known_socket() {
        let mut document =
            GraphDocument::from_engine_config(&ParticleEngineConfig::classic_default());
        document
            .published_params
            .push(published_float("birth_rate", NodeId(2), "birth_rate"));

        validate_published_params(&document).unwrap();

        let json = serde_json::to_string(&document).unwrap();
        assert!(json.contains("\"stable_id\":\"birth_rate\""));
    }

    #[test]
    fn published_value_overrides_compile_through_graph_document() {
        let mut document =
            GraphDocument::from_engine_config(&ParticleEngineConfig::classic_default());
        document
            .published_params
            .push(published_float("birth_rate", NodeId(2), "birth_rate"));

        let config = compile_particle_graph_with_published_values(
            &document,
            &[GraphPublishedValueOverride {
                stable_id: "birth_rate".to_string(),
                value: GraphPublishedValue::Float(42.0),
            }],
        )
        .unwrap();

        assert_eq!(config.emitter.birth_rate, 42.0);
    }

    #[test]
    fn published_value_overrides_support_enum_sockets() {
        let mut document =
            GraphDocument::from_engine_config(&ParticleEngineConfig::classic_default());
        document.published_params.push(GraphPublishedParam {
            stable_id: "shape".to_string(),
            label: "Shape".to_string(),
            target: GraphSocket {
                node: NodeId(6),
                socket: "shape".to_string(),
            },
            value_type: GraphPublishedValueType::Enum,
            default_value: GraphPublishedValue::Enum("circle".to_string()),
        });

        let config = compile_particle_graph_with_published_values(
            &document,
            &[GraphPublishedValueOverride {
                stable_id: "shape".to_string(),
                value: GraphPublishedValue::Enum("star".to_string()),
            }],
        )
        .unwrap();

        assert!(matches!(config.render.shape, ParticleShape::Star));
    }

    #[test]
    fn published_value_overrides_reject_unknown_stable_ids() {
        let document = GraphDocument::from_engine_config(&ParticleEngineConfig::classic_default());

        let err = compile_particle_graph_with_published_values(
            &document,
            &[GraphPublishedValueOverride {
                stable_id: "missing".to_string(),
                value: GraphPublishedValue::Float(42.0),
            }],
        )
        .unwrap_err();

        assert_eq!(
            err,
            GraphPublishedOverrideError::PublishedParam(GraphPublishedParamError::UnknownStableId(
                "missing".to_string()
            ))
        );
    }

    #[test]
    fn published_value_overrides_reject_type_mismatches() {
        let mut document =
            GraphDocument::from_engine_config(&ParticleEngineConfig::classic_default());
        document
            .published_params
            .push(published_float("birth_rate", NodeId(2), "birth_rate"));

        let err = compile_particle_graph_with_published_values(
            &document,
            &[GraphPublishedValueOverride {
                stable_id: "birth_rate".to_string(),
                value: GraphPublishedValue::Integer(42),
            }],
        )
        .unwrap_err();

        assert_eq!(
            err,
            GraphPublishedOverrideError::PublishedParam(
                GraphPublishedParamError::DefaultValueTypeMismatch {
                    stable_id: "birth_rate".to_string(),
                    expected: GraphPublishedValueType::Float
                }
            )
        );
    }

    #[test]
    fn compatible_published_value_overrides_ignore_stale_values() {
        let mut document =
            GraphDocument::from_engine_config(&ParticleEngineConfig::classic_default());
        document
            .published_params
            .push(published_float("birth_rate", NodeId(2), "birth_rate"));

        let config = compile_particle_graph_with_compatible_published_values(
            &document,
            &[
                GraphPublishedValueOverride {
                    stable_id: "missing".to_string(),
                    value: GraphPublishedValue::Float(10.0),
                },
                GraphPublishedValueOverride {
                    stable_id: "birth_rate".to_string(),
                    value: GraphPublishedValue::Integer(12),
                },
                GraphPublishedValueOverride {
                    stable_id: "birth_rate".to_string(),
                    value: GraphPublishedValue::Float(24.0),
                },
            ],
        )
        .unwrap();

        assert_eq!(config.emitter.birth_rate, 24.0);
    }

    #[test]
    fn published_param_validation_rejects_duplicate_ids() {
        let mut document =
            GraphDocument::from_engine_config(&ParticleEngineConfig::classic_default());
        document
            .published_params
            .push(published_float("birth_rate", NodeId(2), "birth_rate"));
        document
            .published_params
            .push(published_float("birth_rate", NodeId(3), "initial_speed"));

        let err = validate_published_params(&document).unwrap_err();
        assert_eq!(
            err,
            GraphPublishedParamError::DuplicateStableId("birth_rate".to_string())
        );
    }

    #[test]
    fn published_param_validation_rejects_unknown_socket() {
        let mut document =
            GraphDocument::from_engine_config(&ParticleEngineConfig::classic_default());
        document
            .published_params
            .push(published_float("bad", NodeId(2), "not_a_socket"));

        let err = validate_published_params(&document).unwrap_err();
        assert_eq!(
            err,
            GraphPublishedParamError::UnsupportedSocket {
                node: NodeId(2),
                socket: "not_a_socket".to_string(),
            }
        );
    }

    #[test]
    fn published_param_validation_rejects_type_mismatch() {
        let mut document =
            GraphDocument::from_engine_config(&ParticleEngineConfig::classic_default());
        document.published_params.push(GraphPublishedParam {
            stable_id: "birth_rate".to_string(),
            label: "Birth Rate".to_string(),
            target: GraphSocket {
                node: NodeId(2),
                socket: "birth_rate".to_string(),
            },
            value_type: GraphPublishedValueType::Integer,
            default_value: GraphPublishedValue::Integer(10),
        });

        let err = validate_published_params(&document).unwrap_err();
        assert_eq!(
            err,
            GraphPublishedParamError::DefaultValueTypeMismatch {
                stable_id: "birth_rate".to_string(),
                expected: GraphPublishedValueType::Float,
            }
        );
    }

    #[test]
    fn compiler_rejects_missing_output_node() {
        let mut document = GraphDocument::simple_point_emitter();
        document.output_node = NodeId(99);

        let err = compile_particle_graph(&document).unwrap_err();
        assert_eq!(err, GraphCompileError::MissingOutputNode(NodeId(99)));
    }
}
