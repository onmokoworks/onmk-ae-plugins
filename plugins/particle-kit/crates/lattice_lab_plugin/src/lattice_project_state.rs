#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::lattice::{
    compile_lattice_graph, lattice_node_ui_catalog, LatticeEngineConfig, LatticeGraphCompileError,
    LatticeGraphDocument,
};
use crate::node_graph_core::NodeUiCatalog;

pub(crate) const LATTICE_PROJECT_STATE_VERSION: u16 = 1;
const LATTICE_PROJECT_STATE_FIRST_JSON_VERSION: u16 = 1;
pub(crate) const LATTICE_NODE_UI_GRAPH_STATE_VERSION: u32 = 1;
pub(crate) const LATTICE_NODE_UI_BOOTSTRAP_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LatticeEngineConfigSource {
    DefaultConfig,
    GraphDocument,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum LatticeProjectEditError {
    MissingGraphDocument,
    Compile(LatticeGraphCompileError),
    UnsupportedNodeUiSnapshotVersion(u32),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct LatticeNodeUiGraphStateSnapshot {
    pub(crate) version: u32,
    pub(crate) enabled: bool,
    pub(crate) document: Option<LatticeGraphDocument>,
}

impl Default for LatticeNodeUiGraphStateSnapshot {
    fn default() -> Self {
        Self {
            version: LATTICE_NODE_UI_GRAPH_STATE_VERSION,
            enabled: false,
            document: None,
        }
    }
}

impl LatticeNodeUiGraphStateSnapshot {
    pub(crate) fn from_json(contents: &str) -> Result<Self, serde_json::Error> {
        let value: Value = serde_json::from_str(contents)?;
        Self::from_value(value)
    }

    pub(crate) fn from_node_ui_json(contents: &str) -> Result<Self, serde_json::Error> {
        let value: Value = serde_json::from_str(contents)?;
        Self::from_node_ui_value(value)
    }

    pub(crate) fn from_value(mut value: Value) -> Result<Self, serde_json::Error> {
        migrate_lattice_node_ui_graph_state_snapshot_value(&mut value);
        serde_json::from_value(value)
    }

    pub(crate) fn from_node_ui_value(value: Value) -> Result<Self, serde_json::Error> {
        if value.get("state").is_some() && value.get("catalog").is_some() {
            LatticeNodeUiBootstrapPayload::from_value(value).map(|payload| payload.state)
        } else {
            Self::from_value(value)
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct LatticeNodeUiBootstrapPayload {
    pub(crate) version: u32,
    pub(crate) catalog: NodeUiCatalog,
    pub(crate) state: LatticeNodeUiGraphStateSnapshot,
}

impl LatticeNodeUiBootstrapPayload {
    pub(crate) fn new(state: LatticeNodeUiGraphStateSnapshot) -> Self {
        Self {
            version: LATTICE_NODE_UI_BOOTSTRAP_VERSION,
            catalog: lattice_node_ui_catalog(),
            state,
        }
    }

    pub(crate) fn from_json(contents: &str) -> Result<Self, serde_json::Error> {
        let value: Value = serde_json::from_str(contents)?;
        Self::from_value(value)
    }

    pub(crate) fn from_value(mut value: Value) -> Result<Self, serde_json::Error> {
        if let Some(state) = value.get_mut("state") {
            migrate_lattice_node_ui_graph_state_snapshot_value(state);
        }
        serde_json::from_value(value)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct LatticeLabProjectState {
    pub(crate) graph: LatticeGraphProjectState,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct LatticeGraphProjectState {
    pub(crate) enabled: bool,
    pub(crate) document: Option<LatticeGraphDocument>,
}

impl LatticeLabProjectState {
    pub(crate) fn engine_config(&self) -> (LatticeEngineConfig, LatticeEngineConfigSource) {
        if let Some((config, source)) = self.engine_config_override() {
            (config, source)
        } else {
            (
                LatticeEngineConfig::default_v1().normalized_for_render(),
                LatticeEngineConfigSource::DefaultConfig,
            )
        }
    }

    pub(crate) fn engine_config_override(
        &self,
    ) -> Option<(LatticeEngineConfig, LatticeEngineConfigSource)> {
        if !self.graph.enabled {
            return None;
        }
        let document = self.graph.document.as_ref()?;
        compile_lattice_graph(document)
            .ok()
            .map(|config| (config, LatticeEngineConfigSource::GraphDocument))
    }

    pub(crate) fn replace_graph_document(&mut self, document: Option<LatticeGraphDocument>) {
        self.graph.enabled = document.is_some();
        self.graph.document = document;
    }

    pub(crate) fn commit_node_graph_document(
        &mut self,
        document: LatticeGraphDocument,
    ) -> Result<(), LatticeProjectEditError> {
        compile_lattice_graph(&document).map_err(LatticeProjectEditError::Compile)?;
        self.graph.enabled = true;
        self.graph.document = Some(document);
        Ok(())
    }

    pub(crate) fn set_graph_enabled(&mut self, enabled: bool) {
        self.graph.enabled = enabled && self.graph.document.is_some();
    }

    pub(crate) fn node_ui_graph_state_snapshot(&self) -> LatticeNodeUiGraphStateSnapshot {
        LatticeNodeUiGraphStateSnapshot {
            version: LATTICE_NODE_UI_GRAPH_STATE_VERSION,
            enabled: self.graph.enabled && self.graph.document.is_some(),
            document: self.graph.document.clone(),
        }
    }

    pub(crate) fn node_ui_bootstrap_payload(&self) -> LatticeNodeUiBootstrapPayload {
        LatticeNodeUiBootstrapPayload::new(self.node_ui_graph_state_snapshot())
    }

    pub(crate) fn commit_node_ui_graph_state_snapshot(
        &mut self,
        snapshot: LatticeNodeUiGraphStateSnapshot,
    ) -> Result<(), LatticeProjectEditError> {
        if snapshot.version > LATTICE_NODE_UI_GRAPH_STATE_VERSION {
            return Err(LatticeProjectEditError::UnsupportedNodeUiSnapshotVersion(
                snapshot.version,
            ));
        }

        let Some(document) = snapshot.document else {
            self.replace_graph_document(None);
            return Ok(());
        };

        compile_lattice_graph(&document).map_err(LatticeProjectEditError::Compile)?;
        self.graph.document = Some(document);
        self.set_graph_enabled(snapshot.enabled);
        Ok(())
    }

    pub(crate) fn flatten_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }

    pub(crate) fn unflatten_bytes(
        version: u16,
        serialized: &[u8],
    ) -> Result<Self, serde_json::Error> {
        match version {
            0 if serialized.is_empty() => Ok(Self::default()),
            version
                if (LATTICE_PROJECT_STATE_FIRST_JSON_VERSION..=LATTICE_PROJECT_STATE_VERSION)
                    .contains(&version) =>
            {
                let mut value: Value = serde_json::from_slice(serialized)?;
                migrate_lattice_project_state_value(&mut value);
                serde_json::from_value(value)
            }
            _ => Ok(Self::default()),
        }
    }
}

fn migrate_lattice_node_ui_graph_state_snapshot_value(value: &mut Value) {
    let Some(obj) = value.as_object_mut() else {
        return;
    };

    let source_version = obj.get("version").and_then(Value::as_u64).unwrap_or(0);
    if source_version <= LATTICE_NODE_UI_GRAPH_STATE_VERSION as u64 {
        obj.insert(
            "version".to_string(),
            Value::from(LATTICE_NODE_UI_GRAPH_STATE_VERSION),
        );
    }
    if !obj.contains_key("document") {
        if let Some(document) = obj.get("graph_document").cloned() {
            obj.insert("document".to_string(), document);
        }
    }
    let has_document = obj
        .get("document")
        .is_some_and(|document| !document.is_null());
    if has_document && !obj.contains_key("enabled") {
        obj.insert("enabled".to_string(), Value::Bool(true));
    }
}

fn migrate_lattice_project_state_value(value: &mut Value) {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_lattice_project_state_uses_default_config() {
        let state = LatticeLabProjectState::default();
        let (config, source) = state.engine_config();

        assert_eq!(source, LatticeEngineConfigSource::DefaultConfig);
        assert_eq!(
            config.points.grid_resolution,
            LatticeEngineConfig::default_v1().points.grid_resolution
        );
        assert!(state.engine_config_override().is_none());
    }

    #[test]
    fn enabled_lattice_graph_document_overrides_default_config() {
        let mut document = LatticeGraphDocument::simple_grid_network();
        for node in &mut document.nodes {
            if let crate::lattice::LatticeGraphNodeKind::GridPoints(data) = &mut node.kind {
                data.resolution = [3, 4, 5];
            }
        }

        let mut state = LatticeLabProjectState::default();
        state.commit_node_graph_document(document).unwrap();
        let (config, source) = state.engine_config();

        assert_eq!(source, LatticeEngineConfigSource::GraphDocument);
        assert_eq!(config.points.grid_resolution, [3, 4, 5]);
    }

    #[test]
    fn lattice_project_state_roundtrips_sequence_data() {
        let mut state = LatticeLabProjectState::default();
        state
            .commit_node_graph_document(LatticeGraphDocument::simple_grid_network())
            .unwrap();

        let bytes = state.flatten_bytes().unwrap();
        let decoded =
            LatticeLabProjectState::unflatten_bytes(LATTICE_PROJECT_STATE_VERSION, &bytes).unwrap();

        assert!(decoded.graph.enabled);
        assert!(decoded.graph.document.is_some());
        assert_eq!(
            decoded.engine_config().1,
            LatticeEngineConfigSource::GraphDocument
        );
    }

    #[test]
    fn lattice_project_state_unflatten_migrates_missing_enabled_flag() {
        let mut state = LatticeLabProjectState::default();
        state.replace_graph_document(Some(LatticeGraphDocument::simple_grid_network()));
        let mut value = serde_json::to_value(&state).unwrap();
        value
            .get_mut("graph")
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove("enabled");

        let bytes = serde_json::to_vec(&value).unwrap();
        let decoded =
            LatticeLabProjectState::unflatten_bytes(LATTICE_PROJECT_STATE_VERSION, &bytes).unwrap();

        assert!(decoded.graph.enabled);
        assert_eq!(
            decoded.engine_config().1,
            LatticeEngineConfigSource::GraphDocument
        );
    }

    #[test]
    fn lattice_node_ui_snapshot_commit_is_atomic() {
        let mut state = LatticeLabProjectState::default();
        state
            .commit_node_graph_document(LatticeGraphDocument::simple_grid_network())
            .unwrap();
        let original_document = state.graph.document.as_ref().unwrap().output_node;

        let mut snapshot = state.node_ui_graph_state_snapshot();
        let mut invalid = LatticeGraphDocument::simple_grid_network();
        invalid.schema_version += 1;
        snapshot.document = Some(invalid);

        let err = state
            .commit_node_ui_graph_state_snapshot(snapshot)
            .unwrap_err();

        assert!(matches!(err, LatticeProjectEditError::Compile(_)));
        assert!(state.graph.enabled);
        assert_eq!(
            state.graph.document.as_ref().unwrap().output_node,
            original_document
        );
    }

    #[test]
    fn lattice_node_ui_snapshot_rejects_future_versions_without_mutating_state() {
        let mut state = LatticeLabProjectState::default();
        state
            .commit_node_graph_document(LatticeGraphDocument::simple_grid_network())
            .unwrap();
        let original_document = state.graph.document.as_ref().unwrap().output_node;

        let mut snapshot = state.node_ui_graph_state_snapshot();
        snapshot.version = LATTICE_NODE_UI_GRAPH_STATE_VERSION + 1;
        snapshot.enabled = false;

        let err = state
            .commit_node_ui_graph_state_snapshot(snapshot)
            .unwrap_err();

        assert_eq!(
            err,
            LatticeProjectEditError::UnsupportedNodeUiSnapshotVersion(
                LATTICE_NODE_UI_GRAPH_STATE_VERSION + 1
            )
        );
        assert!(state.graph.enabled);
        assert_eq!(
            state.graph.document.as_ref().unwrap().output_node,
            original_document
        );
    }

    #[test]
    fn lattice_node_ui_bootstrap_payload_carries_catalog_and_importable_state() {
        let mut state = LatticeLabProjectState::default();
        state
            .commit_node_graph_document(LatticeGraphDocument::simple_grid_network())
            .unwrap();

        let payload = state.node_ui_bootstrap_payload();
        assert_eq!(payload.version, LATTICE_NODE_UI_BOOTSTRAP_VERSION);
        assert_eq!(payload.catalog.product_id, "latticelab");
        assert!(payload
            .catalog
            .nodes
            .iter()
            .any(|entry| entry.node_type == "lattice.render"));

        let json = serde_json::to_string(&payload).unwrap();
        let decoded_payload = LatticeNodeUiBootstrapPayload::from_json(&json).unwrap();
        assert_eq!(decoded_payload.catalog.product_id, "latticelab");

        let snapshot = LatticeNodeUiGraphStateSnapshot::from_node_ui_json(&json).unwrap();
        let mut restored = LatticeLabProjectState::default();
        restored
            .commit_node_ui_graph_state_snapshot(snapshot)
            .unwrap();

        assert_eq!(
            restored.engine_config().1,
            LatticeEngineConfigSource::GraphDocument
        );
    }

    #[test]
    fn node_ui_shell_lattice_fixture_imports_through_project_state() {
        let json = include_str!("../../../tools/node-ui-shell/fixtures/latticelab-bootstrap.json");
        let payload = LatticeNodeUiBootstrapPayload::from_json(json).unwrap();

        assert_eq!(payload.catalog.product_id, "latticelab");
        assert_eq!(payload.state.version, LATTICE_NODE_UI_GRAPH_STATE_VERSION);
        assert!(payload.state.document.is_some());

        let snapshot = LatticeNodeUiGraphStateSnapshot::from_node_ui_json(json).unwrap();
        let mut restored = LatticeLabProjectState::default();
        restored
            .commit_node_ui_graph_state_snapshot(snapshot)
            .unwrap();
        let (config, source) = restored.engine_config();

        assert_eq!(source, LatticeEngineConfigSource::GraphDocument);
        assert_eq!(config.points.grid_resolution, [10, 10, 1]);
    }

    #[test]
    fn lattice_node_ui_import_still_accepts_plain_snapshot_json() {
        let mut state = LatticeLabProjectState::default();
        state
            .commit_node_graph_document(LatticeGraphDocument::simple_grid_network())
            .unwrap();

        let json = serde_json::to_string(&state.node_ui_graph_state_snapshot()).unwrap();
        let snapshot = LatticeNodeUiGraphStateSnapshot::from_node_ui_json(&json).unwrap();
        let mut restored = LatticeLabProjectState::default();
        restored
            .commit_node_ui_graph_state_snapshot(snapshot)
            .unwrap();

        assert_eq!(
            restored.engine_config().1,
            LatticeEngineConfigSource::GraphDocument
        );
    }

    #[test]
    fn lattice_node_ui_snapshot_migrates_graph_document_alias() {
        let value = serde_json::json!({
            "graph_document": LatticeGraphDocument::simple_grid_network()
        });

        let snapshot = LatticeNodeUiGraphStateSnapshot::from_value(value).unwrap();

        assert_eq!(snapshot.version, LATTICE_NODE_UI_GRAPH_STATE_VERSION);
        assert!(snapshot.enabled);
        assert!(snapshot.document.is_some());
    }
}
