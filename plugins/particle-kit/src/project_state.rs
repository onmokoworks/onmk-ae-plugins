use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::engine::ParticleEngineConfig;
use crate::graph::{
    compile_particle_graph, compile_particle_graph_with_compatible_published_values,
    compile_particle_graph_with_published_values, migrate_graph_document_value,
    particle_node_ui_catalog, validate_published_params, GraphCompileError, GraphDocument,
    GraphPublishedOverrideError, GraphPublishedParamError, GraphPublishedValueOverride,
    GraphPublishedValueType,
};
use crate::node_graph_core::NodeUiCatalog;

pub(crate) const PROJECT_STATE_VERSION: u16 = 3;
const PROJECT_STATE_FIRST_JSON_VERSION: u16 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EngineConfigSource {
    ClassicParams,
    GraphDocument,
}

pub(crate) const NODE_UI_GRAPH_STATE_VERSION: u32 = 2;
pub(crate) const NODE_UI_BOOTSTRAP_VERSION: u32 = 1;
pub(crate) const PUBLISHED_HOST_FLOAT_SLOT_COUNT: u8 = 4;

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum GraphProjectEditError {
    MissingGraphDocument,
    Compile(GraphCompileError),
    HostBinding(GraphPublishedHostBindingError),
    PublishedParam(GraphPublishedParamError),
    PublishedValue(GraphPublishedOverrideError),
    UnsupportedNodeUiSnapshotVersion(u32),
}

#[allow(dead_code)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum GraphPublishedHostBindingError {
    DuplicateStableId(String),
    DuplicateSlot(u8),
    InvalidSlot(u8),
    MissingGraphDocument,
    UnknownStableId(String),
    UnsupportedValueType {
        stable_id: String,
        expected: GraphPublishedValueType,
    },
}

#[allow(dead_code)]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct GraphPublishedHostFloatBinding {
    pub(crate) stable_id: String,
    pub(crate) slot: u8,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct NodeUiGraphStateSnapshot {
    pub(crate) version: u32,
    pub(crate) enabled: bool,
    pub(crate) document: Option<GraphDocument>,
    pub(crate) published_values: Vec<GraphPublishedValueOverride>,
    pub(crate) host_float_bindings: Vec<GraphPublishedHostFloatBinding>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct NodeUiBootstrapPayload {
    pub(crate) version: u32,
    pub(crate) catalog: NodeUiCatalog,
    pub(crate) state: NodeUiGraphStateSnapshot,
}

impl Default for NodeUiGraphStateSnapshot {
    fn default() -> Self {
        Self {
            version: NODE_UI_GRAPH_STATE_VERSION,
            enabled: false,
            document: None,
            published_values: Vec::new(),
            host_float_bindings: Vec::new(),
        }
    }
}

#[allow(dead_code)]
impl NodeUiGraphStateSnapshot {
    pub(crate) fn from_json(contents: &str) -> Result<Self, serde_json::Error> {
        let value: Value = serde_json::from_str(contents)?;
        Self::from_value(value)
    }

    pub(crate) fn from_node_ui_json(contents: &str) -> Result<Self, serde_json::Error> {
        let value: Value = serde_json::from_str(contents)?;
        Self::from_node_ui_value(value)
    }

    pub(crate) fn from_value(mut value: Value) -> Result<Self, serde_json::Error> {
        migrate_node_ui_graph_state_snapshot_value(&mut value);
        serde_json::from_value(value)
    }

    pub(crate) fn from_node_ui_value(value: Value) -> Result<Self, serde_json::Error> {
        if value.get("state").is_some() && value.get("catalog").is_some() {
            NodeUiBootstrapPayload::from_value(value).map(|payload| payload.state)
        } else {
            Self::from_value(value)
        }
    }
}

#[allow(dead_code)]
impl NodeUiBootstrapPayload {
    pub(crate) fn new(state: NodeUiGraphStateSnapshot) -> Self {
        Self {
            version: NODE_UI_BOOTSTRAP_VERSION,
            catalog: particle_node_ui_catalog(),
            state,
        }
    }

    pub(crate) fn from_json(contents: &str) -> Result<Self, serde_json::Error> {
        let value: Value = serde_json::from_str(contents)?;
        Self::from_value(value)
    }

    pub(crate) fn from_value(mut value: Value) -> Result<Self, serde_json::Error> {
        if let Some(state) = value.get_mut("state") {
            migrate_node_ui_graph_state_snapshot_value(state);
        }
        serde_json::from_value(value)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ParticleLabProjectState {
    pub(crate) graph: GraphProjectState,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct GraphProjectState {
    pub(crate) enabled: bool,
    pub(crate) document: Option<GraphDocument>,
    pub(crate) published_values: Vec<GraphPublishedValueOverride>,
    pub(crate) host_float_bindings: Vec<GraphPublishedHostFloatBinding>,
}

impl ParticleLabProjectState {
    pub(crate) fn engine_config_override(
        &self,
    ) -> Option<(ParticleEngineConfig, EngineConfigSource)> {
        if !self.graph.enabled {
            return None;
        }
        let document = self.graph.document.as_ref()?;
        compile_particle_graph_with_compatible_published_values(
            document,
            &self.graph.published_values,
        )
        .ok()
        .map(|config| (config, EngineConfigSource::GraphDocument))
    }

    pub(crate) fn replace_graph_document(&mut self, document: Option<GraphDocument>) {
        self.graph.enabled = document.is_some();
        self.graph.document = document;
        self.graph.published_values.clear();
        self.graph.host_float_bindings.clear();
    }

    #[allow(dead_code)]
    pub(crate) fn commit_node_graph_document(
        &mut self,
        document: GraphDocument,
    ) -> Result<(), GraphProjectEditError> {
        validate_committable_graph_document(&document)?;
        let published_values = compatible_published_values(&document, &self.graph.published_values);
        let host_float_bindings =
            compatible_host_float_bindings(&document, &self.graph.host_float_bindings);
        self.graph.enabled = true;
        self.graph.document = Some(document);
        self.graph.published_values = published_values;
        self.graph.host_float_bindings = host_float_bindings;
        Ok(())
    }

    #[allow(dead_code)]
    pub(crate) fn set_graph_enabled(&mut self, enabled: bool) {
        self.graph.enabled = enabled && self.graph.document.is_some();
    }

    #[allow(dead_code)]
    pub(crate) fn set_graph_published_value_override(
        &mut self,
        value_override: GraphPublishedValueOverride,
    ) -> Result<(), GraphProjectEditError> {
        let document = self
            .graph
            .document
            .as_ref()
            .ok_or(GraphProjectEditError::MissingGraphDocument)?;
        compile_particle_graph_with_published_values(
            document,
            std::slice::from_ref(&value_override),
        )
        .map_err(GraphProjectEditError::PublishedValue)?;
        upsert_published_value_override(&mut self.graph.published_values, value_override);
        self.graph.enabled = true;
        Ok(())
    }

    #[allow(dead_code)]
    pub(crate) fn clear_graph_published_value_override(&mut self, stable_id: &str) -> bool {
        let original_len = self.graph.published_values.len();
        self.graph
            .published_values
            .retain(|value_override| value_override.stable_id != stable_id);
        self.graph.published_values.len() != original_len
    }

    #[allow(dead_code)]
    pub(crate) fn bind_graph_published_float_to_host_slot(
        &mut self,
        stable_id: String,
        slot: u8,
    ) -> Result<(), GraphProjectEditError> {
        let document = self
            .graph
            .document
            .as_ref()
            .ok_or(GraphProjectEditError::HostBinding(
                GraphPublishedHostBindingError::MissingGraphDocument,
            ))?;
        validate_host_float_binding(
            document,
            &GraphPublishedHostFloatBinding {
                stable_id: stable_id.clone(),
                slot,
            },
        )
        .map_err(GraphProjectEditError::HostBinding)?;
        upsert_host_float_binding(
            &mut self.graph.host_float_bindings,
            GraphPublishedHostFloatBinding { stable_id, slot },
        );
        Ok(())
    }

    #[allow(dead_code)]
    pub(crate) fn clear_graph_published_host_float_binding(&mut self, slot: u8) -> bool {
        let original_len = self.graph.host_float_bindings.len();
        self.graph
            .host_float_bindings
            .retain(|binding| binding.slot != slot);
        self.graph.host_float_bindings.len() != original_len
    }

    #[allow(dead_code)]
    pub(crate) fn node_ui_graph_state_snapshot(&self) -> NodeUiGraphStateSnapshot {
        NodeUiGraphStateSnapshot {
            version: NODE_UI_GRAPH_STATE_VERSION,
            enabled: self.graph.enabled && self.graph.document.is_some(),
            document: self.graph.document.clone(),
            published_values: self.graph.published_values.clone(),
            host_float_bindings: self.graph.host_float_bindings.clone(),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn node_ui_bootstrap_payload(&self) -> NodeUiBootstrapPayload {
        NodeUiBootstrapPayload::new(self.node_ui_graph_state_snapshot())
    }

    #[allow(dead_code)]
    pub(crate) fn commit_node_ui_graph_state_snapshot(
        &mut self,
        snapshot: NodeUiGraphStateSnapshot,
    ) -> Result<(), GraphProjectEditError> {
        if snapshot.version > NODE_UI_GRAPH_STATE_VERSION {
            return Err(GraphProjectEditError::UnsupportedNodeUiSnapshotVersion(
                snapshot.version,
            ));
        }

        let Some(document) = snapshot.document else {
            self.replace_graph_document(None);
            return Ok(());
        };

        validate_committable_graph_document(&document)?;
        let published_values = compatible_published_values(&document, &snapshot.published_values);
        let host_float_bindings =
            compatible_host_float_bindings(&document, &snapshot.host_float_bindings);
        self.graph.document = Some(document);
        self.graph.published_values = published_values;
        self.graph.host_float_bindings = host_float_bindings;
        self.set_graph_enabled(snapshot.enabled);
        Ok(())
    }

    #[allow(dead_code)]
    pub(crate) fn replace_graph_from_engine_config(&mut self, config: &ParticleEngineConfig) {
        self.replace_graph_document(Some(GraphDocument::from_engine_config(config)));
    }

    pub(crate) fn flatten_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }

    #[allow(dead_code)]
    pub(crate) fn engine_config_override_with_host_values(
        &self,
        host_values: &[GraphPublishedValueOverride],
    ) -> Option<(ParticleEngineConfig, EngineConfigSource)> {
        if !self.graph.enabled {
            return None;
        }
        let document = self.graph.document.as_ref()?;
        let values = effective_published_values(
            document,
            &self.graph.published_values,
            &self.graph.host_float_bindings,
            host_values,
        );
        compile_particle_graph_with_compatible_published_values(document, &values)
            .ok()
            .map(|config| (config, EngineConfigSource::GraphDocument))
    }

    pub(crate) fn unflatten_bytes(
        version: u16,
        serialized: &[u8],
    ) -> Result<Self, serde_json::Error> {
        match version {
            0 if serialized.is_empty() => Ok(Self::default()),
            version
                if (PROJECT_STATE_FIRST_JSON_VERSION..=PROJECT_STATE_VERSION)
                    .contains(&version) =>
            {
                let mut value: Value = serde_json::from_slice(serialized)?;
                migrate_project_state_value(&mut value);
                serde_json::from_value(value)
            }
            _ => Ok(Self::default()),
        }
    }
}

#[allow(dead_code)]
fn validate_committable_graph_document(
    document: &GraphDocument,
) -> Result<(), GraphProjectEditError> {
    compile_particle_graph(document).map_err(GraphProjectEditError::Compile)?;
    validate_published_params(document).map_err(GraphProjectEditError::PublishedParam)?;
    Ok(())
}

#[allow(dead_code)]
fn compatible_published_values(
    document: &GraphDocument,
    values: &[GraphPublishedValueOverride],
) -> Vec<GraphPublishedValueOverride> {
    let mut retained = Vec::new();
    for value_override in values {
        if compile_particle_graph_with_published_values(
            document,
            std::slice::from_ref(value_override),
        )
        .is_ok()
        {
            upsert_published_value_override(&mut retained, value_override.clone());
        }
    }
    retained
}

#[allow(dead_code)]
fn effective_published_values(
    document: &GraphDocument,
    stored_values: &[GraphPublishedValueOverride],
    host_float_bindings: &[GraphPublishedHostFloatBinding],
    host_values: &[GraphPublishedValueOverride],
) -> Vec<GraphPublishedValueOverride> {
    let mut values = compatible_published_values(document, stored_values);
    let host_float_bindings = compatible_host_float_bindings(document, host_float_bindings);

    for binding in host_float_bindings {
        let Some(host_value) = host_values
            .iter()
            .find(|host_value| host_value.stable_id == binding.stable_id)
        else {
            continue;
        };
        if compile_particle_graph_with_published_values(document, std::slice::from_ref(host_value))
            .is_ok()
        {
            upsert_published_value_override(&mut values, host_value.clone());
        }
    }

    values
}

#[allow(dead_code)]
fn upsert_published_value_override(
    values: &mut Vec<GraphPublishedValueOverride>,
    value_override: GraphPublishedValueOverride,
) {
    if let Some(existing) = values
        .iter_mut()
        .find(|existing| existing.stable_id == value_override.stable_id)
    {
        *existing = value_override;
    } else {
        values.push(value_override);
    }
}

#[allow(dead_code)]
fn validate_host_float_binding(
    document: &GraphDocument,
    binding: &GraphPublishedHostFloatBinding,
) -> Result<(), GraphPublishedHostBindingError> {
    if binding.slot == 0 || binding.slot > PUBLISHED_HOST_FLOAT_SLOT_COUNT {
        return Err(GraphPublishedHostBindingError::InvalidSlot(binding.slot));
    }

    let Some(published) = document
        .published_params
        .iter()
        .find(|param| param.stable_id == binding.stable_id)
    else {
        return Err(GraphPublishedHostBindingError::UnknownStableId(
            binding.stable_id.clone(),
        ));
    };

    if published.value_type != GraphPublishedValueType::Float {
        return Err(GraphPublishedHostBindingError::UnsupportedValueType {
            stable_id: binding.stable_id.clone(),
            expected: GraphPublishedValueType::Float,
        });
    }

    Ok(())
}

#[allow(dead_code)]
fn compatible_host_float_bindings(
    document: &GraphDocument,
    bindings: &[GraphPublishedHostFloatBinding],
) -> Vec<GraphPublishedHostFloatBinding> {
    let mut retained = Vec::new();
    for binding in bindings {
        if validate_host_float_binding(document, binding).is_ok() {
            upsert_host_float_binding(&mut retained, binding.clone());
        }
    }
    retained
}

#[allow(dead_code)]
fn upsert_host_float_binding(
    bindings: &mut Vec<GraphPublishedHostFloatBinding>,
    binding: GraphPublishedHostFloatBinding,
) {
    bindings.retain(|existing| {
        existing.slot != binding.slot && existing.stable_id != binding.stable_id
    });
    bindings.push(binding);
}

#[allow(dead_code)]
fn migrate_node_ui_graph_state_snapshot_value(value: &mut Value) {
    let Some(obj) = value.as_object_mut() else {
        return;
    };

    let source_version = obj.get("version").and_then(Value::as_u64).unwrap_or(0);
    if source_version <= NODE_UI_GRAPH_STATE_VERSION as u64 {
        obj.insert(
            "version".to_string(),
            Value::from(NODE_UI_GRAPH_STATE_VERSION),
        );
    }
    if !obj.contains_key("document") {
        if let Some(document) = obj.get("graph_document").cloned() {
            obj.insert("document".to_string(), document);
        }
    }
    if !obj.contains_key("published_values") {
        let published_values = obj
            .get("graph_published_values")
            .cloned()
            .unwrap_or_else(|| Value::Array(Vec::new()));
        obj.insert("published_values".to_string(), published_values);
    }
    obj.entry("host_float_bindings".to_string())
        .or_insert_with(|| Value::Array(Vec::new()));

    let has_document = obj
        .get("document")
        .is_some_and(|document| !document.is_null());
    if has_document && !obj.contains_key("enabled") {
        obj.insert("enabled".to_string(), Value::Bool(true));
    }
    if let Some(document) = obj.get_mut("document") {
        migrate_graph_document_value(document);
    }
}

fn migrate_project_state_value(value: &mut Value) {
    let Some(obj) = value.as_object_mut() else {
        return;
    };
    let Some(graph) = obj.get_mut("graph").and_then(Value::as_object_mut) else {
        return;
    };
    let has_document = graph
        .get("document")
        .is_some_and(|document| !document.is_null());
    if has_document && !graph.contains_key("enabled") {
        graph.insert("enabled".to_string(), Value::Bool(true));
    }
    graph
        .entry("published_values")
        .or_insert_with(|| Value::Array(Vec::new()));
    graph
        .entry("host_float_bindings")
        .or_insert_with(|| Value::Array(Vec::new()));
    if let Some(document) = graph.get_mut("document") {
        migrate_graph_document_value(document);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{
        GraphPublishedParam, GraphPublishedValue, GraphPublishedValueOverride,
        GraphPublishedValueType, GraphSocket, NodeId,
    };

    fn document_with_birth_rate_published() -> GraphDocument {
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
        document
    }

    fn document_with_integer_seed_published() -> GraphDocument {
        let mut document = document_with_birth_rate_published();
        document.published_params.push(GraphPublishedParam {
            stable_id: "seed".to_string(),
            label: "Seed".to_string(),
            target: GraphSocket {
                node: NodeId(8),
                socket: "seed".to_string(),
            },
            value_type: GraphPublishedValueType::Integer,
            default_value: GraphPublishedValue::Integer(12345),
        });
        document
    }

    fn birth_rate_override(value: f32) -> GraphPublishedValueOverride {
        GraphPublishedValueOverride {
            stable_id: "birth_rate".to_string(),
            value: GraphPublishedValue::Float(value),
        }
    }

    #[test]
    fn default_project_state_uses_classic_params() {
        let state = ParticleLabProjectState::default();
        assert!(state.engine_config_override().is_none());
    }

    #[test]
    fn enabled_graph_document_overrides_classic_params() {
        let mut state = ParticleLabProjectState::default();
        state.replace_graph_document(Some(GraphDocument::simple_point_emitter()));

        let (config, source) = state.engine_config_override().unwrap();
        assert_eq!(source, EngineConfigSource::GraphDocument);
        assert_eq!(config.emitter.birth_rate, 180.0);
    }

    #[test]
    fn disabled_graph_document_keeps_classic_params_active() {
        let mut state = ParticleLabProjectState::default();
        state.replace_graph_document(Some(GraphDocument::simple_point_emitter()));
        state.graph.enabled = false;

        assert!(state.engine_config_override().is_none());
    }

    #[test]
    fn invalid_graph_document_keeps_classic_params_active() {
        let mut document = GraphDocument::simple_point_emitter();
        document.schema_version += 1;

        let mut state = ParticleLabProjectState::default();
        state.replace_graph_document(Some(document));

        assert!(state.engine_config_override().is_none());
    }

    #[test]
    fn project_state_roundtrips_for_ae_sequence_data() {
        let mut state = ParticleLabProjectState::default();
        state.replace_graph_document(Some(GraphDocument::simple_point_emitter()));

        let bytes = state.flatten_bytes().unwrap();
        let decoded =
            ParticleLabProjectState::unflatten_bytes(PROJECT_STATE_VERSION, &bytes).unwrap();

        assert!(decoded.graph.enabled);
        assert!(decoded.graph.document.is_some());
        assert!(decoded.engine_config_override().is_some());
    }

    #[test]
    fn project_state_unflatten_migrates_embedded_graph_document() {
        let mut state = ParticleLabProjectState::default();
        state.replace_graph_document(Some(GraphDocument::simple_point_emitter()));
        let mut value = serde_json::to_value(&state).unwrap();
        let graph = value
            .as_object_mut()
            .unwrap()
            .get_mut("graph")
            .unwrap()
            .as_object_mut()
            .unwrap();
        graph.remove("enabled");
        let document = graph.get_mut("document").unwrap().as_object_mut().unwrap();
        document.remove("schema_version");
        document.remove("published_params");
        for node in document.get_mut("nodes").unwrap().as_array_mut().unwrap() {
            node.as_object_mut().unwrap().remove("version");
        }

        let bytes = serde_json::to_vec(&value).unwrap();
        let decoded =
            ParticleLabProjectState::unflatten_bytes(PROJECT_STATE_VERSION, &bytes).unwrap();

        assert!(decoded.graph.enabled);
        let document = decoded.graph.document.as_ref().unwrap();
        assert_eq!(document.schema_version, crate::graph::GRAPH_SCHEMA_VERSION);
        assert!(document.nodes.iter().all(|node| node.version == 1));
        assert!(decoded.engine_config_override().is_some());
    }

    #[test]
    fn project_state_roundtrips_graph_published_params() {
        let document = document_with_birth_rate_published();

        let mut state = ParticleLabProjectState::default();
        state.replace_graph_document(Some(document));

        let bytes = state.flatten_bytes().unwrap();
        let decoded =
            ParticleLabProjectState::unflatten_bytes(PROJECT_STATE_VERSION, &bytes).unwrap();

        let decoded_document = decoded.graph.document.as_ref().unwrap();
        assert_eq!(decoded_document.published_params.len(), 1);
        assert_eq!(decoded_document.published_params[0].stable_id, "birth_rate");
        assert!(decoded.engine_config_override().is_some());
    }

    #[test]
    fn project_state_applies_graph_published_value_overrides() {
        let document = document_with_birth_rate_published();

        let mut state = ParticleLabProjectState::default();
        state.replace_graph_document(Some(document));
        state.graph.published_values.push(birth_rate_override(42.0));

        let (config, source) = state.engine_config_override().unwrap();

        assert_eq!(source, EngineConfigSource::GraphDocument);
        assert_eq!(config.emitter.birth_rate, 42.0);
    }

    #[test]
    fn project_state_roundtrips_graph_published_value_overrides() {
        let document = document_with_birth_rate_published();

        let mut state = ParticleLabProjectState::default();
        state.replace_graph_document(Some(document));
        state.graph.published_values.push(birth_rate_override(33.0));

        let bytes = state.flatten_bytes().unwrap();
        let decoded =
            ParticleLabProjectState::unflatten_bytes(PROJECT_STATE_VERSION, &bytes).unwrap();
        let (config, source) = decoded.engine_config_override().unwrap();

        assert_eq!(source, EngineConfigSource::GraphDocument);
        assert_eq!(config.emitter.birth_rate, 33.0);
        assert_eq!(decoded.graph.published_values.len(), 1);
    }

    #[test]
    fn project_state_unflatten_reads_v1_without_published_value_overrides() {
        let mut state = ParticleLabProjectState::default();
        state.replace_graph_document(Some(GraphDocument::simple_point_emitter()));
        let mut value = serde_json::to_value(&state).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .get_mut("graph")
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove("published_values");

        let bytes = serde_json::to_vec(&value).unwrap();
        let decoded = ParticleLabProjectState::unflatten_bytes(1, &bytes).unwrap();

        assert!(decoded.graph.enabled);
        assert!(decoded.graph.published_values.is_empty());
        assert!(decoded.graph.host_float_bindings.is_empty());
        assert!(decoded.engine_config_override().is_some());
    }

    #[test]
    fn project_state_unflatten_reads_v2_without_host_float_bindings() {
        let mut state = ParticleLabProjectState::default();
        state
            .commit_node_graph_document(document_with_birth_rate_published())
            .unwrap();
        state
            .set_graph_published_value_override(birth_rate_override(24.0))
            .unwrap();
        state
            .bind_graph_published_float_to_host_slot("birth_rate".to_string(), 1)
            .unwrap();
        let mut value = serde_json::to_value(&state).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .get_mut("graph")
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove("host_float_bindings");

        let bytes = serde_json::to_vec(&value).unwrap();
        let decoded = ParticleLabProjectState::unflatten_bytes(2, &bytes).unwrap();
        let (config, source) = decoded.engine_config_override().unwrap();

        assert_eq!(source, EngineConfigSource::GraphDocument);
        assert_eq!(config.emitter.birth_rate, 24.0);
        assert!(decoded.graph.host_float_bindings.is_empty());
    }

    #[test]
    fn node_ui_commit_preserves_only_compatible_published_value_overrides() {
        let mut state = ParticleLabProjectState::default();
        state
            .commit_node_graph_document(document_with_birth_rate_published())
            .unwrap();
        state
            .set_graph_published_value_override(birth_rate_override(42.0))
            .unwrap();

        state
            .commit_node_graph_document(document_with_birth_rate_published())
            .unwrap();
        let (config, source) = state.engine_config_override().unwrap();

        assert_eq!(source, EngineConfigSource::GraphDocument);
        assert_eq!(config.emitter.birth_rate, 42.0);
        assert_eq!(state.graph.published_values.len(), 1);

        state
            .commit_node_graph_document(GraphDocument::simple_point_emitter())
            .unwrap();
        let (config, source) = state.engine_config_override().unwrap();

        assert_eq!(source, EngineConfigSource::GraphDocument);
        assert_eq!(config.emitter.birth_rate, 180.0);
        assert!(state.graph.published_values.is_empty());
    }

    #[test]
    fn node_ui_commit_rejects_invalid_document_without_mutating_state() {
        let mut state = ParticleLabProjectState::default();
        state
            .commit_node_graph_document(document_with_birth_rate_published())
            .unwrap();
        state
            .set_graph_published_value_override(birth_rate_override(42.0))
            .unwrap();

        let mut invalid = GraphDocument::simple_point_emitter();
        invalid.schema_version += 1;
        let err = state.commit_node_graph_document(invalid).unwrap_err();
        let (config, source) = state.engine_config_override().unwrap();

        assert!(matches!(err, GraphProjectEditError::Compile(_)));
        assert_eq!(source, EngineConfigSource::GraphDocument);
        assert_eq!(config.emitter.birth_rate, 42.0);
        assert_eq!(state.graph.published_values.len(), 1);
    }

    #[test]
    fn node_ui_published_value_overrides_are_strict_and_deduplicated() {
        let mut state = ParticleLabProjectState::default();
        let err = state
            .set_graph_published_value_override(birth_rate_override(12.0))
            .unwrap_err();
        assert_eq!(err, GraphProjectEditError::MissingGraphDocument);

        state
            .commit_node_graph_document(document_with_birth_rate_published())
            .unwrap();
        state
            .set_graph_published_value_override(birth_rate_override(42.0))
            .unwrap();
        state
            .set_graph_published_value_override(birth_rate_override(12.0))
            .unwrap();

        let (config, _) = state.engine_config_override().unwrap();
        assert_eq!(config.emitter.birth_rate, 12.0);
        assert_eq!(state.graph.published_values.len(), 1);

        let err = state
            .set_graph_published_value_override(GraphPublishedValueOverride {
                stable_id: "removed_control".to_string(),
                value: GraphPublishedValue::Float(99.0),
            })
            .unwrap_err();

        assert!(matches!(err, GraphProjectEditError::PublishedValue(_)));
        assert_eq!(state.graph.published_values.len(), 1);
        assert!(state.clear_graph_published_value_override("birth_rate"));
        assert!(!state.clear_graph_published_value_override("birth_rate"));
        let (config, _) = state.engine_config_override().unwrap();
        assert_eq!(config.emitter.birth_rate, 180.0);
    }

    #[test]
    fn node_ui_graph_enabled_flag_requires_a_document() {
        let mut state = ParticleLabProjectState::default();

        state.set_graph_enabled(true);
        assert!(!state.graph.enabled);

        state
            .commit_node_graph_document(document_with_birth_rate_published())
            .unwrap();
        assert!(state.graph.enabled);

        state.set_graph_enabled(false);
        assert!(!state.graph.enabled);
        assert!(state.engine_config_override().is_none());

        state.set_graph_enabled(true);
        assert!(state.graph.enabled);
        assert!(state.engine_config_override().is_some());
    }

    #[test]
    fn host_float_binding_validates_slots_and_published_value_types() {
        let mut state = ParticleLabProjectState::default();
        let err = state
            .bind_graph_published_float_to_host_slot("birth_rate".to_string(), 1)
            .unwrap_err();
        assert_eq!(
            err,
            GraphProjectEditError::HostBinding(
                GraphPublishedHostBindingError::MissingGraphDocument
            )
        );

        state
            .commit_node_graph_document(document_with_integer_seed_published())
            .unwrap();
        let err = state
            .bind_graph_published_float_to_host_slot("birth_rate".to_string(), 0)
            .unwrap_err();
        assert_eq!(
            err,
            GraphProjectEditError::HostBinding(GraphPublishedHostBindingError::InvalidSlot(0))
        );

        let err = state
            .bind_graph_published_float_to_host_slot("removed".to_string(), 1)
            .unwrap_err();
        assert_eq!(
            err,
            GraphProjectEditError::HostBinding(GraphPublishedHostBindingError::UnknownStableId(
                "removed".to_string()
            ))
        );

        let err = state
            .bind_graph_published_float_to_host_slot("seed".to_string(), 1)
            .unwrap_err();
        assert_eq!(
            err,
            GraphProjectEditError::HostBinding(
                GraphPublishedHostBindingError::UnsupportedValueType {
                    stable_id: "seed".to_string(),
                    expected: GraphPublishedValueType::Float,
                }
            )
        );
    }

    #[test]
    fn host_float_bindings_roundtrip_and_survive_compatible_graph_edits() {
        let mut state = ParticleLabProjectState::default();
        state
            .commit_node_graph_document(document_with_birth_rate_published())
            .unwrap();
        state
            .bind_graph_published_float_to_host_slot("birth_rate".to_string(), 1)
            .unwrap();

        let bytes = state.flatten_bytes().unwrap();
        let decoded =
            ParticleLabProjectState::unflatten_bytes(PROJECT_STATE_VERSION, &bytes).unwrap();

        assert_eq!(decoded.graph.host_float_bindings.len(), 1);
        assert_eq!(decoded.graph.host_float_bindings[0].stable_id, "birth_rate");
        assert_eq!(decoded.graph.host_float_bindings[0].slot, 1);

        state
            .commit_node_graph_document(document_with_birth_rate_published())
            .unwrap();
        assert_eq!(state.graph.host_float_bindings.len(), 1);

        state
            .commit_node_graph_document(GraphDocument::simple_point_emitter())
            .unwrap();
        assert!(state.graph.host_float_bindings.is_empty());
    }

    #[test]
    fn host_float_values_override_stored_published_values_when_bound() {
        let mut state = ParticleLabProjectState::default();
        state
            .commit_node_graph_document(document_with_birth_rate_published())
            .unwrap();
        state
            .set_graph_published_value_override(birth_rate_override(24.0))
            .unwrap();
        state
            .bind_graph_published_float_to_host_slot("birth_rate".to_string(), 1)
            .unwrap();

        let (stored_config, _) = state.engine_config_override().unwrap();
        let (host_config, source) = state
            .engine_config_override_with_host_values(&[birth_rate_override(99.0)])
            .unwrap();

        assert_eq!(stored_config.emitter.birth_rate, 24.0);
        assert_eq!(source, EngineConfigSource::GraphDocument);
        assert_eq!(host_config.emitter.birth_rate, 99.0);

        let (ignored_config, _) = state
            .engine_config_override_with_host_values(&[GraphPublishedValueOverride {
                stable_id: "removed_control".to_string(),
                value: GraphPublishedValue::Float(5.0),
            }])
            .unwrap();
        assert_eq!(ignored_config.emitter.birth_rate, 24.0);
    }

    #[test]
    fn node_ui_graph_state_snapshot_roundtrips_for_external_ui_json() {
        let mut state = ParticleLabProjectState::default();
        state
            .commit_node_graph_document(document_with_birth_rate_published())
            .unwrap();
        state
            .set_graph_published_value_override(birth_rate_override(88.0))
            .unwrap();
        state
            .bind_graph_published_float_to_host_slot("birth_rate".to_string(), 1)
            .unwrap();

        let json = serde_json::to_string(&state.node_ui_graph_state_snapshot()).unwrap();
        assert!(json.contains("\"version\":2"));
        assert!(json.contains("\"host_float_bindings\""));

        let snapshot = NodeUiGraphStateSnapshot::from_json(&json).unwrap();
        let mut restored = ParticleLabProjectState::default();
        restored
            .commit_node_ui_graph_state_snapshot(snapshot)
            .unwrap();
        let (config, source) = restored.engine_config_override().unwrap();

        assert_eq!(source, EngineConfigSource::GraphDocument);
        assert_eq!(config.emitter.birth_rate, 88.0);
        assert_eq!(restored.graph.published_values.len(), 1);
        assert_eq!(restored.graph.host_float_bindings.len(), 1);
    }

    #[test]
    fn node_ui_bootstrap_payload_carries_catalog_and_importable_state() {
        let mut state = ParticleLabProjectState::default();
        state
            .commit_node_graph_document(document_with_birth_rate_published())
            .unwrap();
        state
            .set_graph_published_value_override(birth_rate_override(77.0))
            .unwrap();
        state
            .bind_graph_published_float_to_host_slot("birth_rate".to_string(), 1)
            .unwrap();

        let payload = state.node_ui_bootstrap_payload();
        assert_eq!(payload.version, NODE_UI_BOOTSTRAP_VERSION);
        assert_eq!(
            payload.catalog.catalog_version,
            crate::node_graph_core::NODE_UI_CATALOG_VERSION
        );
        assert_eq!(payload.catalog.product_id, "particlelab");
        assert!(payload
            .catalog
            .nodes
            .iter()
            .any(|entry| entry.node_type == "particle.render"));
        assert!(payload.state.enabled);

        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"catalog\""));
        assert!(json.contains("\"state\""));

        let decoded_payload = NodeUiBootstrapPayload::from_json(&json).unwrap();
        assert_eq!(decoded_payload.catalog.product_id, "particlelab");

        let snapshot = NodeUiGraphStateSnapshot::from_node_ui_json(&json).unwrap();
        let mut restored = ParticleLabProjectState::default();
        restored
            .commit_node_ui_graph_state_snapshot(snapshot)
            .unwrap();
        let (config, source) = restored.engine_config_override().unwrap();

        assert_eq!(source, EngineConfigSource::GraphDocument);
        assert_eq!(config.emitter.birth_rate, 77.0);
        assert_eq!(restored.graph.host_float_bindings.len(), 1);
    }

    #[test]
    fn node_ui_shell_particle_fixture_imports_through_project_state() {
        let json = include_str!("../tools/node-ui-shell/fixtures/particlelab-bootstrap.json");
        let payload = NodeUiBootstrapPayload::from_json(json).unwrap();

        assert_eq!(payload.catalog.product_id, "particlelab");
        assert_eq!(payload.state.version, NODE_UI_GRAPH_STATE_VERSION);
        assert!(payload
            .state
            .document
            .as_ref()
            .unwrap()
            .published_params
            .iter()
            .any(|param| param.stable_id == "birth_rate"));

        let snapshot = NodeUiGraphStateSnapshot::from_node_ui_json(json).unwrap();
        let mut restored = ParticleLabProjectState::default();
        restored
            .commit_node_ui_graph_state_snapshot(snapshot)
            .unwrap();
        let (config, source) = restored.engine_config_override().unwrap();

        assert_eq!(source, EngineConfigSource::GraphDocument);
        assert_eq!(config.emitter.birth_rate, 180.0);
        assert_eq!(restored.graph.host_float_bindings.len(), 1);
    }

    #[test]
    fn node_ui_import_still_accepts_plain_snapshot_json() {
        let mut state = ParticleLabProjectState::default();
        state
            .commit_node_graph_document(document_with_birth_rate_published())
            .unwrap();
        state
            .set_graph_published_value_override(birth_rate_override(66.0))
            .unwrap();

        let json = serde_json::to_string(&state.node_ui_graph_state_snapshot()).unwrap();
        let snapshot = NodeUiGraphStateSnapshot::from_node_ui_json(&json).unwrap();
        let mut restored = ParticleLabProjectState::default();
        restored
            .commit_node_ui_graph_state_snapshot(snapshot)
            .unwrap();
        let (config, source) = restored.engine_config_override().unwrap();

        assert_eq!(source, EngineConfigSource::GraphDocument);
        assert_eq!(config.emitter.birth_rate, 66.0);
    }

    #[test]
    fn node_ui_graph_state_snapshot_migrates_preset_field_aliases() {
        let mut value = serde_json::json!({
            "graph_published_values": [birth_rate_override(55.0)]
        });
        value.as_object_mut().unwrap().insert(
            "graph_document".to_string(),
            serde_json::to_value(document_with_birth_rate_published()).unwrap(),
        );

        let snapshot = NodeUiGraphStateSnapshot::from_value(value).unwrap();
        let mut state = ParticleLabProjectState::default();
        state
            .commit_node_ui_graph_state_snapshot(snapshot.clone())
            .unwrap();
        let (config, source) = state.engine_config_override().unwrap();

        assert_eq!(snapshot.version, NODE_UI_GRAPH_STATE_VERSION);
        assert!(snapshot.enabled);
        assert_eq!(source, EngineConfigSource::GraphDocument);
        assert_eq!(config.emitter.birth_rate, 55.0);
    }

    #[test]
    fn node_ui_graph_state_snapshot_migrates_embedded_old_graph_json() {
        let mut value = serde_json::json!({});
        value.as_object_mut().unwrap().insert(
            "document".to_string(),
            serde_json::to_value(GraphDocument::simple_point_emitter()).unwrap(),
        );
        let document = value
            .as_object_mut()
            .unwrap()
            .get_mut("document")
            .unwrap()
            .as_object_mut()
            .unwrap();
        document.remove("schema_version");
        document.remove("published_params");
        for node in document.get_mut("nodes").unwrap().as_array_mut().unwrap() {
            node.as_object_mut().unwrap().remove("version");
        }

        let snapshot = NodeUiGraphStateSnapshot::from_value(value).unwrap();
        let document = snapshot.document.as_ref().unwrap();

        assert_eq!(snapshot.version, NODE_UI_GRAPH_STATE_VERSION);
        assert!(snapshot.enabled);
        assert!(snapshot.published_values.is_empty());
        assert_eq!(document.schema_version, crate::graph::GRAPH_SCHEMA_VERSION);
        assert!(document.nodes.iter().all(|node| node.version == 1));
    }

    #[test]
    fn node_ui_graph_state_snapshot_commit_is_atomic() {
        let mut state = ParticleLabProjectState::default();
        state
            .commit_node_graph_document(document_with_birth_rate_published())
            .unwrap();
        state
            .set_graph_published_value_override(birth_rate_override(42.0))
            .unwrap();

        let mut snapshot = state.node_ui_graph_state_snapshot();
        let mut invalid = GraphDocument::simple_point_emitter();
        invalid.schema_version += 1;
        snapshot.document = Some(invalid);
        snapshot.published_values.clear();

        let err = state
            .commit_node_ui_graph_state_snapshot(snapshot)
            .unwrap_err();
        let (config, source) = state.engine_config_override().unwrap();

        assert!(matches!(err, GraphProjectEditError::Compile(_)));
        assert_eq!(source, EngineConfigSource::GraphDocument);
        assert_eq!(config.emitter.birth_rate, 42.0);
        assert_eq!(state.graph.published_values.len(), 1);
    }

    #[test]
    fn node_ui_graph_state_snapshot_rejects_future_versions_without_mutating_state() {
        let mut state = ParticleLabProjectState::default();
        state
            .commit_node_graph_document(document_with_birth_rate_published())
            .unwrap();
        state
            .set_graph_published_value_override(birth_rate_override(42.0))
            .unwrap();
        let original = state.clone();

        let mut snapshot = state.node_ui_graph_state_snapshot();
        snapshot.version = NODE_UI_GRAPH_STATE_VERSION + 1;
        snapshot.published_values = vec![birth_rate_override(12.0)];

        let err = state
            .commit_node_ui_graph_state_snapshot(snapshot)
            .unwrap_err();

        assert_eq!(
            err,
            GraphProjectEditError::UnsupportedNodeUiSnapshotVersion(
                NODE_UI_GRAPH_STATE_VERSION + 1
            )
        );
        assert_eq!(state.graph.enabled, original.graph.enabled);
        assert_eq!(
            state
                .graph
                .document
                .as_ref()
                .map(|document| document.output_node),
            original
                .graph
                .document
                .as_ref()
                .map(|document| document.output_node)
        );
        assert_eq!(
            state.engine_config_override().unwrap().0.emitter.birth_rate,
            42.0
        );
        assert_eq!(state.graph.published_values.len(), 1);
        assert_eq!(state.graph.published_values[0].stable_id, "birth_rate");
        assert_eq!(
            state.graph.host_float_bindings,
            original.graph.host_float_bindings
        );
    }

    #[test]
    fn node_ui_graph_state_snapshot_without_document_disables_graph() {
        let mut state = ParticleLabProjectState::default();
        state
            .commit_node_graph_document(document_with_birth_rate_published())
            .unwrap();
        state
            .set_graph_published_value_override(birth_rate_override(42.0))
            .unwrap();

        state
            .commit_node_ui_graph_state_snapshot(NodeUiGraphStateSnapshot::default())
            .unwrap();

        assert!(!state.graph.enabled);
        assert!(state.graph.document.is_none());
        assert!(state.graph.published_values.is_empty());
        assert!(state.engine_config_override().is_none());
    }

    #[test]
    fn project_state_ignores_stale_published_value_overrides() {
        let document = document_with_birth_rate_published();

        let mut state = ParticleLabProjectState::default();
        state.replace_graph_document(Some(document));
        state
            .graph
            .published_values
            .push(GraphPublishedValueOverride {
                stable_id: "removed_control".to_string(),
                value: GraphPublishedValue::Float(999.0),
            });
        state
            .graph
            .published_values
            .push(GraphPublishedValueOverride {
                stable_id: "birth_rate".to_string(),
                value: GraphPublishedValue::Integer(12),
            });

        let (config, source) = state.engine_config_override().unwrap();

        assert_eq!(source, EngineConfigSource::GraphDocument);
        assert_eq!(config.emitter.birth_rate, 180.0);
    }

    #[test]
    fn project_state_can_seed_graph_from_classic_engine_config() {
        let mut config = ParticleEngineConfig::classic_default();
        config.emitter.birth_rate = 42.0;
        config.seed = 9876;

        let mut state = ParticleLabProjectState::default();
        state.replace_graph_from_engine_config(&config);

        let (compiled, source) = state.engine_config_override().unwrap();
        assert_eq!(source, EngineConfigSource::GraphDocument);
        assert_eq!(compiled.emitter.birth_rate, 42.0);
        assert_eq!(compiled.seed, 9876);
    }

    #[test]
    fn unknown_project_state_version_falls_back_to_classic_params() {
        let decoded = ParticleLabProjectState::unflatten_bytes(99, b"{}").unwrap();
        assert!(decoded.engine_config_override().is_none());
    }
}
