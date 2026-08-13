use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::Hash;

pub const NODE_UI_CATALOG_VERSION: u32 = 1;

pub trait CoreGraphNode {
    type Id: Copy + Eq + Hash;

    fn id(&self) -> Self::Id;
}

pub trait CoreGraphEdge {
    type Id: Copy + Eq + Hash;

    fn from_node(&self) -> Self::Id;
    fn to_node(&self) -> Self::Id;
    fn to_socket(&self) -> &str;
}

pub fn node_map<'a, N, E>(
    nodes: &'a [N],
    duplicate_error: impl Fn(N::Id) -> E,
) -> Result<HashMap<N::Id, &'a N>, E>
where
    N: CoreGraphNode,
{
    let mut map = HashMap::new();
    for node in nodes {
        let id = node.id();
        if map.insert(id, node).is_some() {
            return Err(duplicate_error(id));
        }
    }
    Ok(map)
}

pub fn output_input<'a, Id, N, Edge, E>(
    edges: &'a [Edge],
    nodes: &HashMap<Id, &'a N>,
    output: Id,
    socket: &str,
    unknown_node_error: impl Fn(Id) -> E,
) -> Result<Option<&'a N>, E>
where
    Id: Copy + Eq + Hash,
    Edge: CoreGraphEdge<Id = Id>,
{
    let Some(edge) = edges
        .iter()
        .find(|edge| edge.to_node() == output && edge.to_socket() == socket)
    else {
        return Ok(None);
    };

    nodes
        .get(&edge.from_node())
        .copied()
        .map(Some)
        .ok_or_else(|| unknown_node_error(edge.from_node()))
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NodeUiCatalog {
    pub catalog_version: u32,
    pub graph_schema_version: u32,
    pub product_id: String,
    pub product_label: String,
    pub nodes: Vec<NodeUiCatalogEntry>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NodeUiCatalogEntry {
    pub node_type: String,
    pub label: String,
    pub version: u32,
    pub input_sockets: Vec<NodeUiConnectionSocket>,
    pub output_sockets: Vec<NodeUiConnectionSocket>,
    pub value_sockets: Vec<NodeUiValueSocket>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NodeUiConnectionSocket {
    pub socket: String,
    pub label: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NodeUiValueSocket {
    pub socket: String,
    pub label: String,
    pub value_type: NodeUiValueType,
    pub enum_options: Vec<NodeUiEnumOption>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NodeUiEnumOption {
    pub value: String,
    pub label: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeUiValueType {
    Float,
    Integer,
    Boolean,
    Color,
    Vector3,
    Enum,
}

impl NodeUiCatalog {
    pub fn new(
        product_id: &str,
        product_label: &str,
        graph_schema_version: u32,
        nodes: Vec<NodeUiCatalogEntry>,
    ) -> Self {
        Self {
            catalog_version: NODE_UI_CATALOG_VERSION,
            graph_schema_version,
            product_id: product_id.to_string(),
            product_label: product_label.to_string(),
            nodes,
        }
    }
}

impl NodeUiCatalogEntry {
    pub fn new(
        node_type: &str,
        label: &str,
        input_sockets: Vec<NodeUiConnectionSocket>,
        output_sockets: Vec<NodeUiConnectionSocket>,
        value_sockets: Vec<NodeUiValueSocket>,
    ) -> Self {
        Self {
            node_type: node_type.to_string(),
            label: label.to_string(),
            version: 1,
            input_sockets,
            output_sockets,
            value_sockets,
        }
    }
}

impl NodeUiConnectionSocket {
    pub fn new(socket: &str, label: &str) -> Self {
        Self {
            socket: socket.to_string(),
            label: label.to_string(),
        }
    }
}

impl NodeUiValueSocket {
    pub fn value(socket: &str, label: &str, value_type: NodeUiValueType) -> Self {
        Self {
            socket: socket.to_string(),
            label: label.to_string(),
            value_type,
            enum_options: Vec::new(),
        }
    }

    pub fn enumeration(socket: &str, label: &str, options: &[(&str, &str)]) -> Self {
        Self {
            socket: socket.to_string(),
            label: label.to_string(),
            value_type: NodeUiValueType::Enum,
            enum_options: options
                .iter()
                .map(|(value, label)| NodeUiEnumOption {
                    value: (*value).to_string(),
                    label: (*label).to_string(),
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy)]
    struct TestNode {
        id: u32,
    }

    impl CoreGraphNode for TestNode {
        type Id = u32;

        fn id(&self) -> Self::Id {
            self.id
        }
    }

    struct TestEdge {
        from: u32,
        to: u32,
        socket: &'static str,
    }

    impl CoreGraphEdge for TestEdge {
        type Id = u32;

        fn from_node(&self) -> Self::Id {
            self.from
        }

        fn to_node(&self) -> Self::Id {
            self.to
        }

        fn to_socket(&self) -> &str {
            self.socket
        }
    }

    #[test]
    fn node_map_rejects_duplicate_ids() {
        let nodes = [TestNode { id: 1 }, TestNode { id: 1 }];

        let result = node_map(&nodes, |id| id);

        assert!(matches!(result, Err(1)));
    }

    #[test]
    fn output_input_resolves_connected_source_node() {
        let nodes = [TestNode { id: 1 }, TestNode { id: 2 }];
        let map = node_map(&nodes, |id| id).unwrap();
        let edges = [TestEdge {
            from: 1,
            to: 2,
            socket: "input",
        }];

        let input = output_input(&edges, &map, 2, "input", |id| id).unwrap();

        assert_eq!(input.unwrap().id, 1);
    }

    #[test]
    fn catalog_builders_fill_versioned_metadata() {
        let catalog = NodeUiCatalog::new(
            "test",
            "Test",
            7,
            vec![NodeUiCatalogEntry::new(
                "test.node",
                "Node",
                vec![NodeUiConnectionSocket::new("in", "In")],
                vec![NodeUiConnectionSocket::new("out", "Out")],
                vec![NodeUiValueSocket::enumeration(
                    "mode",
                    "Mode",
                    &[("a", "A"), ("b", "B")],
                )],
            )],
        );

        assert_eq!(catalog.catalog_version, NODE_UI_CATALOG_VERSION);
        assert_eq!(catalog.graph_schema_version, 7);
        assert_eq!(catalog.nodes[0].version, 1);
        assert_eq!(catalog.nodes[0].value_sockets[0].enum_options.len(), 2);
    }
}
