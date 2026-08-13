use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::engine::ParticleEngineConfig;
use crate::graph::{
    compile_particle_graph_with_compatible_published_values, migrate_graph_document_value,
    GraphDocument, GraphPublishedValueOverride,
};
use crate::particle::{AppearanceConfig, ChildConfig, EmitterConfig, EmitterType, PhysicsConfig};
use crate::renderer::{
    ApplyMode, BlendMode, ImageColorMode, ImageFitMode, ImageSamplingConfig, ParticleShape,
    RenderConfig, TimeSamplingMode,
};

/// Current preset format version. Missing fields in older presets are filled
/// through `#[serde(default)]`; bump this when migrations or defaults change
/// behavior.
pub(crate) const PRESET_VERSION: u32 = 6;

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub(crate) struct PresetColor {
    pub(crate) alpha: u8,
    pub(crate) red: u8,
    pub(crate) green: u8,
    pub(crate) blue: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct PresetSnapshot {
    pub(crate) version: u32,
    pub(crate) name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) graph_document: Option<GraphDocument>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) graph_published_values: Vec<GraphPublishedValueOverride>,
    // ---- Emitter ----
    pub(crate) emitter_type: i32,
    pub(crate) emit_mode: i32,
    pub(crate) position_point: (f32, f32),
    pub(crate) position_z: f64,
    pub(crate) image_proxy_scale: i32,
    pub(crate) path_sample_density: f64,
    pub(crate) grid_res_x: i32,
    pub(crate) grid_res_y: i32,
    pub(crate) grid_res_z: i32,
    pub(crate) emitter_size_linked: bool,
    pub(crate) emitter_size_x: f64,
    pub(crate) emitter_size_y: f64,
    pub(crate) emitter_size_z: f64,
    pub(crate) birth_rate: f64,
    pub(crate) lifespan: f64,
    pub(crate) lifespan_var: f64,
    // ---- Motion ----
    pub(crate) speed: f64,
    pub(crate) speed_var: f64,
    pub(crate) direction_x: f64,
    pub(crate) direction_y: f64,
    pub(crate) direction_z: f64,
    pub(crate) spread: f64,
    pub(crate) initial_size: f64,
    pub(crate) size_var: f64,
    pub(crate) rotation: f64,
    pub(crate) rotation_speed: f64,
    // ---- Physics ----
    pub(crate) gravity_strength: f64,
    pub(crate) wind_x: f64,
    pub(crate) wind_y: f64,
    pub(crate) turb_strength: f64,
    pub(crate) turb_scale: f64,
    pub(crate) turb_speed: f64,
    pub(crate) air_resistance: f64,
    pub(crate) bounce_enabled: bool,
    pub(crate) bounce_damping: f64,
    // ---- Appearance ----
    pub(crate) color_mode: i32,
    pub(crate) color_start: PresetColor,
    pub(crate) color_end: PresetColor,
    pub(crate) opacity_curve_preset: i32,
    pub(crate) opacity_start: f64,
    pub(crate) opacity_mid_a: f64,
    pub(crate) opacity_mid_b: f64,
    pub(crate) opacity_end: f64,
    pub(crate) size_curve_preset: i32,
    pub(crate) size_life_start: f64,
    pub(crate) size_life_mid_a: f64,
    pub(crate) size_life_mid_b: f64,
    pub(crate) size_life_end: f64,
    // ---- Rendering ----
    pub(crate) shape: i32,
    pub(crate) sprite_time_sampling: i32,
    pub(crate) sprite_frame_count: i32,
    pub(crate) image_color_mode: i32,
    pub(crate) image_fit_mode: i32,
    pub(crate) use_source_alpha: bool,
    pub(crate) source_premultiplied: bool,
    pub(crate) image_alpha_clip: f64,
    pub(crate) blend_mode: i32,
    pub(crate) motion_blur: f64,
    pub(crate) edge_softness: f64,
    pub(crate) dof_enabled: bool,
    pub(crate) dof_focal_dist: f64,
    pub(crate) dof_aperture: f64,
    pub(crate) size_multiplier: f64,
    pub(crate) composite_on_orig: bool,
    pub(crate) apply_mode: i32,
    pub(crate) rotation_variation: f64,
    pub(crate) opacity_variation: f64,
    // ---- Child ----
    pub(crate) child_enabled: bool,
    pub(crate) child_count: i32,
    pub(crate) child_inherit_vel: f64,
    pub(crate) child_lifespan: f64,
    pub(crate) child_speed: f64,
    pub(crate) child_spread: f64,
    pub(crate) child_size_scale: f64,
    // ---- System ----
    pub(crate) seed: i32,
}

impl Default for PresetSnapshot {
    // Keep these defaults in sync with `params_setup`; omitted fields in older
    // presets are materialized from here before being applied to AE params or
    // compiled into the engine.
    fn default() -> Self {
        Self {
            version: PRESET_VERSION,
            name: String::new(),
            graph_document: None,
            graph_published_values: Vec::new(),
            emitter_type: 1,
            emit_mode: 1,
            position_point: (50.0, 50.0),
            position_z: 0.0,
            image_proxy_scale: 3,
            path_sample_density: 10.0,
            grid_res_x: 8,
            grid_res_y: 8,
            grid_res_z: 1,
            emitter_size_linked: true,
            emitter_size_x: 0.0,
            emitter_size_y: 0.0,
            emitter_size_z: 0.0,
            birth_rate: 180.0,
            lifespan: 1.6,
            lifespan_var: 0.15,
            speed: 240.0,
            speed_var: 0.0,
            direction_x: 0.0,
            direction_y: -1.0,
            direction_z: 0.0,
            spread: 18.0,
            initial_size: 9.0,
            size_var: 0.2,
            rotation: 0.0,
            rotation_speed: 0.0,
            gravity_strength: 160.0,
            wind_x: 0.0,
            wind_y: 0.0,
            turb_strength: 12.0,
            turb_scale: 0.75,
            turb_speed: 1.0,
            air_resistance: 0.3,
            bounce_enabled: false,
            bounce_damping: 0.5,
            color_mode: 2,
            color_start: PresetColor {
                alpha: 255,
                red: 255,
                green: 255,
                blue: 255,
            },
            color_end: PresetColor {
                alpha: 255,
                red: 255,
                green: 255,
                blue: 255,
            },
            opacity_curve_preset: 3,
            opacity_start: 100.0,
            opacity_mid_a: 90.0,
            opacity_mid_b: 45.0,
            opacity_end: 0.0,
            size_curve_preset: 3,
            size_life_start: 1.0,
            size_life_mid_a: 1.0,
            size_life_mid_b: 0.65,
            size_life_end: 0.35,
            shape: 1,
            sprite_time_sampling: 1,
            sprite_frame_count: 1,
            image_color_mode: 1,
            image_fit_mode: 1,
            use_source_alpha: true,
            source_premultiplied: true,
            image_alpha_clip: 0.01,
            blend_mode: 1,
            motion_blur: 0.2,
            edge_softness: 0.0,
            dof_enabled: false,
            dof_focal_dist: 0.0,
            dof_aperture: 5.0,
            size_multiplier: 1.15,
            composite_on_orig: true,
            apply_mode: 2,
            rotation_variation: 0.0,
            opacity_variation: 0.0,
            child_enabled: false,
            child_count: 3,
            child_inherit_vel: 0.65,
            child_lifespan: 0.5,
            child_speed: 80.0,
            child_spread: 110.0,
            child_size_scale: 0.4,
            seed: 12345,
        }
    }
}

impl PresetSnapshot {
    pub(crate) fn to_engine_config(&self) -> ParticleEngineConfig {
        if let Some(document) = &self.graph_document {
            if let Ok(config) = compile_particle_graph_with_compatible_published_values(
                document,
                &self.graph_published_values,
            ) {
                return config;
            }
        }
        self.to_classic_engine_config()
    }

    fn to_classic_engine_config(&self) -> ParticleEngineConfig {
        let emitter_type = match self.emitter_type {
            1 => EmitterType::Point,
            2 => EmitterType::Box,
            3 => EmitterType::Sphere,
            4 => EmitterType::Grid,
            5 => EmitterType::LayerAlpha,
            6 => EmitterType::Path,
            _ => EmitterType::Point,
        };
        let emitter_size_x = self.emitter_size_x as f32;
        let emitter_size_y = if self.emitter_size_linked {
            emitter_size_x
        } else {
            self.emitter_size_y as f32
        };
        let emitter_size_z = if self.emitter_size_linked {
            emitter_size_x
        } else {
            self.emitter_size_z as f32
        };

        let color_end = if self.color_mode == 1 {
            self.color_start
        } else {
            self.color_end
        };

        let emitter = EmitterConfig {
            emitter_type,
            position: glam::Vec3::new(
                self.position_point.0,
                self.position_point.1,
                self.position_z as f32,
            ),
            size: glam::Vec3::new(emitter_size_x, emitter_size_y, emitter_size_z),
            source_points: None,
            birth_rate: self.birth_rate as f32,
            lifespan: self.lifespan as f32,
            lifespan_variation: self.lifespan_var as f32,
            initial_speed: self.speed as f32,
            speed_variation: self.speed_var as f32,
            initial_direction: glam::Vec3::new(
                self.direction_x as f32,
                self.direction_y as f32,
                self.direction_z as f32,
            )
            .normalize_or_zero(),
            spread: (self.spread as f32).to_radians(),
            initial_size: self.initial_size as f32,
            size_variation: self.size_var as f32,
            initial_rotation: self.rotation as f32,
            rotation_variation: self.rotation_variation as f32,
            rotation_speed: self.rotation_speed as f32,
            sprite_frame_count: self.sprite_frame_count.max(1) as u16,
            sprite_time_sampling: (self.sprite_time_sampling - 1).max(0) as u8,
            opacity_variation: self.opacity_variation as f32,
            grid_res_x: self.grid_res_x.max(1) as u32,
            grid_res_y: self.grid_res_y.max(1) as u32,
            grid_res_z: self.grid_res_z.max(1) as u32,
            emit_all_at_start: self.emit_mode == 2,
        };

        let physics = PhysicsConfig {
            gravity: glam::Vec3::new(0.0, self.gravity_strength as f32, 0.0),
            wind: glam::Vec3::new(self.wind_x as f32, self.wind_y as f32, 0.0),
            air_resistance: self.air_resistance as f32,
            turbulence_strength: self.turb_strength as f32,
            turbulence_scale: self.turb_scale as f32,
            turbulence_speed: self.turb_speed as f32,
            bounce_floor_y: 10000.0,
            bounce_enabled: self.bounce_enabled,
            bounce_damping: self.bounce_damping as f32,
        };

        let opacity_start = self.opacity_start as f32 / 100.0;
        let opacity_mid_a = self.opacity_mid_a as f32 / 100.0;
        let opacity_mid_b = self.opacity_mid_b as f32 / 100.0;
        let opacity_end = self.opacity_end as f32 / 100.0;

        let appearance = AppearanceConfig {
            color_start: [
                self.color_start.red as f32 / 255.0,
                self.color_start.green as f32 / 255.0,
                self.color_start.blue as f32 / 255.0,
                opacity_start,
            ],
            color_end: [
                color_end.red as f32 / 255.0,
                color_end.green as f32 / 255.0,
                color_end.blue as f32 / 255.0,
                opacity_end,
            ],
            size_over_life: [
                self.size_life_start as f32,
                self.size_life_mid_a as f32,
                self.size_life_mid_b as f32,
                self.size_life_end as f32,
            ],
            opacity_over_life: [opacity_start, opacity_mid_a, opacity_mid_b, opacity_end],
        };

        let child = ChildConfig {
            enabled: self.child_enabled,
            count: self.child_count.max(0) as u32,
            inherit_velocity: self.child_inherit_vel as f32,
            lifespan: self.child_lifespan as f32,
            initial_speed: self.child_speed as f32,
            spread: (self.child_spread as f32).to_radians(),
            size_scale: self.child_size_scale as f32,
        };

        let shape = match self.shape {
            1 => ParticleShape::Circle,
            2 => ParticleShape::Square,
            3 => ParticleShape::Triangle,
            4 => ParticleShape::Star,
            5 => ParticleShape::Line,
            6 => ParticleShape::Image,
            _ => ParticleShape::Circle,
        };
        let blend_mode = match self.blend_mode {
            1 => BlendMode::Normal,
            2 => BlendMode::Add,
            3 => BlendMode::Screen,
            _ => BlendMode::Normal,
        };

        let render = RenderConfig {
            width: 0,
            height: 0,
            row_stride: 0,
            origin_x: 0.0,
            origin_y: 0.0,
            frame_dt: 1.0 / 30.0,
            shape,
            blend_mode,
            motion_blur: self.motion_blur as f32,
            edge_softness: self.edge_softness as f32,
            dof_enabled: self.dof_enabled,
            dof_focal_distance: self.dof_focal_dist as f32,
            dof_aperture: self.dof_aperture as f32,
            composite_on_original: self.composite_on_orig,
            apply_mode: match self.apply_mode {
                2 => ApplyMode::Normal,
                3 => ApplyMode::Add,
                4 => ApplyMode::Screen,
                _ => ApplyMode::OnTransparent,
            },
            size_multiplier: self.size_multiplier as f32,
            sprite_images: Vec::new(),
            time_sampling: match self.sprite_time_sampling {
                2 => TimeSamplingMode::BirthTime,
                3 => TimeSamplingMode::RandomStill,
                4 => TimeSamplingMode::RandomPlay,
                5 => TimeSamplingMode::Cycle,
                _ => TimeSamplingMode::CurrentTime,
            },
            image_color_mode: match self.image_color_mode {
                2 => ImageColorMode::Source,
                _ => ImageColorMode::Tint,
            },
            image_fit_mode: match self.image_fit_mode {
                2 => ImageFitMode::Stretch,
                _ => ImageFitMode::Contain,
            },
            image_sampling: ImageSamplingConfig {
                use_source_alpha: self.use_source_alpha,
                source_premultiplied: self.source_premultiplied,
                alpha_clip: self.image_alpha_clip as f32,
            },
            camera_projection: None,
        };

        ParticleEngineConfig {
            emitter,
            physics,
            appearance,
            child,
            render,
            seed: self.seed.max(0) as u64,
        }
    }
}

pub(crate) fn load_preset_snapshot(contents: &str) -> Result<PresetSnapshot, serde_json::Error> {
    let mut value: Value = serde_json::from_str(contents)?;
    migrate_preset_value(&mut value);
    serde_json::from_value(value)
}

fn migrate_preset_value(value: &mut Value) {
    if let Some(obj) = value.as_object_mut() {
        let source_version = obj.get("version").and_then(Value::as_u64).unwrap_or(0);
        if !obj.contains_key("apply_mode") {
            let composite_on_orig = obj
                .get("composite_on_orig")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            obj.insert(
                "apply_mode".to_string(),
                Value::from(if composite_on_orig { 2 } else { 1 }),
            );
        }
        if source_version <= PRESET_VERSION as u64 {
            obj.insert("version".to_string(), Value::from(PRESET_VERSION));
        }
        obj.entry("graph_published_values")
            .or_insert_with(|| Value::Array(Vec::new()));
        if let Some(graph_document) = obj.get_mut("graph_document") {
            migrate_graph_document_value(graph_document);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{
        compile_particle_graph, GraphDocument, GraphPublishedParam, GraphPublishedValue,
        GraphPublishedValueOverride, GraphPublishedValueType, GraphSocket, NodeId,
        ParticleGraphNode,
    };

    #[test]
    fn preset_with_graph_document_compiles_graph_before_classic_fields() {
        let mut document = GraphDocument::simple_point_emitter();
        for node in &mut document.nodes {
            if let ParticleGraphNode::PointEmitter(data) = &mut node.kind {
                data.birth_rate = 12.0;
            }
        }

        let mut snapshot = PresetSnapshot::default();
        snapshot.birth_rate = 999.0;
        snapshot.graph_document = Some(document);

        let config = snapshot.to_engine_config();
        assert_eq!(config.emitter.birth_rate, 12.0);
    }

    #[test]
    fn invalid_graph_document_falls_back_to_classic_fields() {
        let mut document = GraphDocument::simple_point_emitter();
        document.schema_version += 1;

        let mut snapshot = PresetSnapshot::default();
        snapshot.birth_rate = 44.0;
        snapshot.graph_document = Some(document);

        let config = snapshot.to_engine_config();
        assert_eq!(config.emitter.birth_rate, 44.0);
    }

    #[test]
    fn graph_document_roundtrips_inside_preset_json() {
        let mut snapshot = PresetSnapshot::default();
        snapshot.graph_document = Some(GraphDocument::simple_point_emitter());

        let json = serde_json::to_string(&snapshot).unwrap();
        assert!(json.contains("\"graph_document\""));
        assert!(json.contains("particle.output"));

        let decoded = load_preset_snapshot(&json).unwrap();
        assert!(decoded.graph_document.is_some());
        assert_eq!(decoded.to_engine_config().emitter.birth_rate, 180.0);
    }

    #[test]
    fn graph_published_values_compile_inside_preset_json() {
        let mut document =
            GraphDocument::from_engine_config(&ParticleEngineConfig::classic_default());
        document.published_params.push(GraphPublishedParam {
            stable_id: "birth_rate".to_string(),
            label: "Birth Rate".to_string(),
            target: GraphSocket {
                node: NodeId(2),
                socket: "birth_rate".to_string(),
            },
            value_type: GraphPublishedValueType::Float,
            default_value: GraphPublishedValue::Float(180.0),
        });

        let mut snapshot = PresetSnapshot::default();
        snapshot.graph_document = Some(document);
        snapshot
            .graph_published_values
            .push(GraphPublishedValueOverride {
                stable_id: "birth_rate".to_string(),
                value: GraphPublishedValue::Float(24.0),
            });

        let json = serde_json::to_string(&snapshot).unwrap();
        assert!(json.contains("\"graph_published_values\""));
        let value: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(
            value.get("version").and_then(Value::as_u64),
            Some(PRESET_VERSION as u64)
        );

        let decoded = load_preset_snapshot(&json).unwrap();
        assert_eq!(decoded.to_engine_config().emitter.birth_rate, 24.0);
    }

    #[test]
    fn missing_graph_published_values_defaults_to_empty_for_old_presets() {
        let mut snapshot = PresetSnapshot::default();
        snapshot.graph_document = Some(GraphDocument::simple_point_emitter());
        let mut value = serde_json::to_value(&snapshot).unwrap();
        let obj = value.as_object_mut().unwrap();
        obj.insert("version".to_string(), Value::from(5));
        obj.remove("graph_published_values");

        let decoded = load_preset_snapshot(&value.to_string()).unwrap();

        assert_eq!(decoded.version, PRESET_VERSION);
        assert!(decoded.graph_published_values.is_empty());
        assert_eq!(decoded.to_engine_config().emitter.birth_rate, 180.0);
    }

    #[test]
    fn load_preset_snapshot_migrates_embedded_graph_document() {
        let mut snapshot = PresetSnapshot::default();
        snapshot.graph_document = Some(GraphDocument::simple_point_emitter());
        let mut value = serde_json::to_value(&snapshot).unwrap();
        let graph = value
            .as_object_mut()
            .unwrap()
            .get_mut("graph_document")
            .unwrap()
            .as_object_mut()
            .unwrap();
        graph.remove("schema_version");
        graph.remove("published_params");
        for node in graph.get_mut("nodes").unwrap().as_array_mut().unwrap() {
            node.as_object_mut().unwrap().remove("version");
        }

        let decoded = load_preset_snapshot(&value.to_string()).unwrap();
        let document = decoded.graph_document.unwrap();

        assert_eq!(document.schema_version, crate::graph::GRAPH_SCHEMA_VERSION);
        assert!(document.nodes.iter().all(|node| node.version == 1));
        assert_eq!(
            compile_particle_graph(&document)
                .unwrap()
                .emitter
                .birth_rate,
            180.0
        );
    }
}
