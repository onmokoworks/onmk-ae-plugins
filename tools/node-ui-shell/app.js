const elements = {
  subtitle: document.getElementById("subtitle"),
  productLabel: document.getElementById("productLabel"),
  schemaLabel: document.getElementById("schemaLabel"),
  statusLine: document.getElementById("statusLine"),
  catalogCount: document.getElementById("catalogCount"),
  catalogList: document.getElementById("catalogList"),
  publishedCount: document.getElementById("publishedCount"),
  publishedList: document.getElementById("publishedList"),
  graphCanvas: document.getElementById("graphCanvas"),
  edgeLayer: document.getElementById("edgeLayer"),
  nodeLayer: document.getElementById("nodeLayer"),
  selectedNodeType: document.getElementById("selectedNodeType"),
  inspectorBody: document.getElementById("inspectorBody"),
  diagnosticCount: document.getElementById("diagnosticCount"),
  diagnosticList: document.getElementById("diagnosticList"),
  loadFileButton: document.getElementById("loadFileButton"),
  pasteButton: document.getElementById("pasteButton"),
  particleDemoButton: document.getElementById("particleDemoButton"),
  latticeDemoButton: document.getElementById("latticeDemoButton"),
  saveBootstrapButton: document.getElementById("saveBootstrapButton"),
  saveSnapshotButton: document.getElementById("saveSnapshotButton"),
  fileInput: document.getElementById("fileInput"),
  pasteDialog: document.getElementById("pasteDialog"),
  pasteInput: document.getElementById("pasteInput"),
  applyPasteButton: document.getElementById("applyPasteButton"),
};

const EMPTY_CATALOG = {
  catalog_version: 0,
  graph_schema_version: 0,
  product_id: "unknown",
  product_label: "Unknown",
  nodes: [],
};
const PARTICLELAB_SNAPSHOT_VERSION = 2;
const LATTICELAB_SNAPSHOT_VERSION = 1;
const HOST_FLOAT_SLOT_COUNT = 4;

let appState = {
  payload: createParticleDemoPayload(),
  selectedNodeId: null,
  layout: new Map(),
};

function createParticleDemoPayload() {
  return {
    version: 1,
    catalog: {
      catalog_version: 1,
      graph_schema_version: 1,
      product_id: "particlelab",
      product_label: "ParticleLab",
      nodes: [
        catalogNode(
          "particle.output",
          "Output",
          [
            connectionSocket("emitter", "Emitter"),
            connectionSocket("motion", "Motion"),
            connectionSocket("physics", "Physics"),
            connectionSocket("appearance", "Appearance"),
            connectionSocket("render", "Render"),
            connectionSocket("child", "Child"),
            connectionSocket("system", "System"),
          ],
          [],
          [],
        ),
        catalogNode(
          "particle.emitter",
          "Emitter",
          [],
          [connectionSocket("out", "Out")],
          [
            enumSocket("emitter_type", "Emitter Type", [
              ["point", "Point"],
              ["box", "Box"],
              ["sphere", "Sphere"],
              ["grid", "Grid"],
              ["layer_alpha", "Layer Alpha"],
              ["path", "Path"],
            ]),
            valueSocket("position", "Position", "vector3"),
            valueSocket("size", "Size", "vector3"),
            valueSocket("birth_rate", "Birth Rate", "float"),
            valueSocket("lifespan", "Lifespan", "float"),
            valueSocket("lifespan_variation", "Lifespan Variation", "float"),
            valueSocket("emit_all_at_start", "Emit All At Start", "boolean"),
            valueSocket("grid_resolution", "Grid Resolution", "vector3"),
          ],
        ),
        catalogNode(
          "particle.motion",
          "Motion",
          [],
          [connectionSocket("out", "Out")],
          [
            valueSocket("initial_speed", "Initial Speed", "float"),
            valueSocket("speed_variation", "Speed Variation", "float"),
            valueSocket("initial_direction", "Initial Direction", "vector3"),
            valueSocket("spread_degrees", "Spread", "float"),
            valueSocket("initial_size", "Initial Size", "float"),
            valueSocket("size_variation", "Size Variation", "float"),
            valueSocket("initial_rotation", "Initial Rotation", "float"),
            valueSocket("rotation_variation", "Rotation Variation", "float"),
            valueSocket("rotation_speed", "Rotation Speed", "float"),
            valueSocket("opacity_variation", "Opacity Variation", "float"),
            valueSocket("sprite_frame_count", "Sprite Frame Count", "integer"),
            valueSocket("sprite_time_sampling", "Sprite Time Sampling", "integer"),
          ],
        ),
        catalogNode(
          "particle.physics",
          "Physics",
          [],
          [connectionSocket("out", "Out")],
          [
            valueSocket("gravity", "Gravity", "vector3"),
            valueSocket("wind", "Wind", "vector3"),
            valueSocket("air_resistance", "Air Resistance", "float"),
            valueSocket("turbulence_strength", "Turbulence Strength", "float"),
            valueSocket("turbulence_scale", "Turbulence Scale", "float"),
            valueSocket("turbulence_speed", "Turbulence Speed", "float"),
            valueSocket("bounce_floor_y", "Bounce Floor Y", "float"),
            valueSocket("bounce_enabled", "Bounce Enabled", "boolean"),
            valueSocket("bounce_damping", "Bounce Damping", "float"),
          ],
        ),
        catalogNode(
          "particle.appearance",
          "Appearance",
          [],
          [connectionSocket("out", "Out")],
          [
            valueSocket("color_start", "Color Start", "color"),
            valueSocket("color_end", "Color End", "color"),
            valueSocket("size_over_life", "Size Over Life", "color"),
            valueSocket("opacity_over_life", "Opacity Over Life", "color"),
          ],
        ),
        catalogNode(
          "particle.render",
          "Render",
          [],
          [connectionSocket("out", "Out")],
          [
            enumSocket("shape", "Shape", [
              ["circle", "Circle"],
              ["square", "Square"],
              ["triangle", "Triangle"],
              ["star", "Star"],
              ["line", "Line"],
              ["image", "Image"],
            ]),
            enumSocket("blend_mode", "Blend Mode", [
              ["normal", "Normal"],
              ["add", "Add"],
              ["screen", "Screen"],
            ]),
            valueSocket("motion_blur", "Motion Blur", "float"),
            valueSocket("edge_softness", "Edge Softness", "float"),
            valueSocket("dof_enabled", "Depth Of Field", "boolean"),
            valueSocket("dof_focal_distance", "DOF Focal Distance", "float"),
            valueSocket("dof_aperture", "DOF Aperture", "float"),
            valueSocket("composite_on_original", "Composite On Original", "boolean"),
            enumSocket("apply_mode", "Apply Mode", [
              ["on_transparent", "On Transparent"],
              ["normal", "Normal"],
              ["add", "Add"],
              ["screen", "Screen"],
            ]),
            valueSocket("size_multiplier", "Size Multiplier", "float"),
            enumSocket("time_sampling", "Time Sampling", [
              ["current_time", "Current Time"],
              ["birth_time", "Birth Time"],
              ["random_still", "Random Still"],
              ["random_play", "Random Play"],
              ["cycle", "Cycle"],
            ]),
            enumSocket("image_color_mode", "Image Color Mode", [
              ["tint", "Tint"],
              ["source", "Source"],
            ]),
            enumSocket("image_fit_mode", "Image Fit Mode", [
              ["contain", "Contain"],
              ["stretch", "Stretch"],
            ]),
            valueSocket("use_source_alpha", "Use Source Alpha", "boolean"),
            valueSocket("source_premultiplied", "Source Premultiplied", "boolean"),
            valueSocket("alpha_clip", "Alpha Clip", "float"),
          ],
        ),
        catalogNode(
          "particle.child",
          "Child",
          [],
          [connectionSocket("out", "Out")],
          [
            valueSocket("enabled", "Enabled", "boolean"),
            valueSocket("count", "Count", "integer"),
            valueSocket("inherit_velocity", "Inherit Velocity", "float"),
            valueSocket("lifespan", "Lifespan", "float"),
            valueSocket("initial_speed", "Initial Speed", "float"),
            valueSocket("spread_degrees", "Spread", "float"),
            valueSocket("size_scale", "Size Scale", "float"),
          ],
        ),
        catalogNode(
          "particle.system",
          "System",
          [],
          [connectionSocket("out", "Out")],
          [valueSocket("seed", "Seed", "integer")],
        ),
      ],
    },
    state: {
      version: 2,
      enabled: true,
      document: {
        schema_version: 1,
        output_node: 1,
        nodes: [
          { id: 1, version: 1, label: "Output", type: "particle.output", data: {} },
          {
            id: 2,
            version: 1,
            label: "Emitter",
            type: "particle.emitter",
            data: {
              emitter_type: "point",
              position: [50, 50, 0],
              size: [0, 0, 0],
              birth_rate: 180,
              lifespan: 1.6,
              lifespan_variation: 0.15,
              emit_all_at_start: false,
              grid_resolution: [8, 8, 1],
            },
          },
          {
            id: 3,
            version: 1,
            label: "Motion",
            type: "particle.motion",
            data: {
              initial_speed: 240,
              speed_variation: 0,
              initial_direction: [0, -1, 0],
              spread_degrees: 18,
              initial_size: 9,
              size_variation: 0.2,
              initial_rotation: 0,
              rotation_variation: 0,
              rotation_speed: 0,
              opacity_variation: 0,
              sprite_frame_count: 1,
              sprite_time_sampling: 0,
            },
          },
          {
            id: 4,
            version: 1,
            label: "Physics",
            type: "particle.physics",
            data: {
              gravity: [0, 160, 0],
              wind: [0, 0, 0],
              air_resistance: 0.3,
              turbulence_strength: 12,
              turbulence_scale: 0.75,
              turbulence_speed: 1,
              bounce_floor_y: 10000,
              bounce_enabled: false,
              bounce_damping: 0.5,
            },
          },
          {
            id: 5,
            version: 1,
            label: "Appearance",
            type: "particle.appearance",
            data: {
              color_start: [1, 1, 1, 1],
              color_end: [1, 1, 1, 0],
              size_over_life: [1, 1, 0.65, 0.35],
              opacity_over_life: [1, 0.9, 0.45, 0],
            },
          },
          {
            id: 6,
            version: 1,
            label: "Render",
            type: "particle.render",
            data: {
              shape: "circle",
              blend_mode: "normal",
              motion_blur: 0.2,
              edge_softness: 0,
              dof_enabled: false,
              dof_focal_distance: 0,
              dof_aperture: 5,
              composite_on_original: true,
              apply_mode: "normal",
              size_multiplier: 1.15,
              time_sampling: "current_time",
              image_color_mode: "tint",
              image_fit_mode: "contain",
              use_source_alpha: true,
              source_premultiplied: true,
              alpha_clip: 0.01,
            },
          },
          {
            id: 7,
            version: 1,
            label: "Child",
            type: "particle.child",
            data: {
              enabled: false,
              count: 3,
              inherit_velocity: 0.65,
              lifespan: 0.5,
              initial_speed: 80,
              spread_degrees: 110,
              size_scale: 0.4,
            },
          },
          {
            id: 8,
            version: 1,
            label: "System",
            type: "particle.system",
            data: { seed: 12345 },
          },
        ],
        edges: [
          { from: { node: 2, socket: "out" }, to: { node: 1, socket: "emitter" } },
          { from: { node: 3, socket: "out" }, to: { node: 1, socket: "motion" } },
          { from: { node: 4, socket: "out" }, to: { node: 1, socket: "physics" } },
          { from: { node: 5, socket: "out" }, to: { node: 1, socket: "appearance" } },
          { from: { node: 6, socket: "out" }, to: { node: 1, socket: "render" } },
          { from: { node: 7, socket: "out" }, to: { node: 1, socket: "child" } },
          { from: { node: 8, socket: "out" }, to: { node: 1, socket: "system" } },
        ],
        published_params: [
          {
            stable_id: "birth_rate",
            label: "Birth Rate",
            target: { node: 2, socket: "birth_rate" },
            value_type: "float",
            default_value: { type: "float", value: 180 },
          },
        ],
      },
      published_values: [{ stable_id: "birth_rate", value: { type: "float", value: 180 } }],
      host_float_bindings: [{ stable_id: "birth_rate", slot: 1 }],
    },
  };
}

function createLatticeDemoPayload() {
  return {
    version: 1,
    catalog: {
      catalog_version: 1,
      graph_schema_version: 1,
      product_id: "latticelab",
      product_label: "Lattice Lab",
      nodes: [
        catalogNode(
          "lattice.output",
          "Output",
          [
            connectionSocket("points", "Points"),
            connectionSocket("noise", "Noise"),
            connectionSocket("links", "Links"),
            connectionSocket("mesh", "Mesh"),
            connectionSocket("render", "Render"),
            connectionSocket("system", "System"),
          ],
          [],
          [],
        ),
        catalogNode(
          "lattice.points.grid",
          "Grid Points",
          [],
          [connectionSocket("out", "Out")],
          [
            valueSocket("resolution", "Resolution", "vector3"),
            valueSocket("spacing", "Spacing", "float"),
            valueSocket("max_points", "Max Points", "integer"),
          ],
        ),
        catalogNode(
          "lattice.noise",
          "Noise",
          [],
          [connectionSocket("out", "Out")],
          [
            valueSocket("enabled", "Enabled", "boolean"),
            valueSocket("amplitude", "Amplitude", "float"),
            valueSocket("frequency", "Frequency", "float"),
            valueSocket("speed", "Speed", "float"),
            valueSocket("octaves", "Octaves", "integer"),
            valueSocket("axis_scale", "Axis Scale", "vector3"),
          ],
        ),
        catalogNode(
          "lattice.links",
          "Links",
          [],
          [connectionSocket("out", "Out")],
          [
            valueSocket("enabled", "Enabled", "boolean"),
            valueSocket("max_distance", "Max Distance", "float"),
            valueSocket("width", "Width", "float"),
            valueSocket("opacity_falloff", "Opacity Falloff", "float"),
            valueSocket("color", "Color", "color"),
          ],
        ),
        catalogNode(
          "lattice.mesh",
          "Mesh",
          [],
          [connectionSocket("out", "Out")],
          [
            valueSocket("enabled", "Enabled", "boolean"),
            valueSocket("max_edge", "Max Edge", "float"),
            valueSocket("opacity", "Opacity", "float"),
            valueSocket("color", "Color", "color"),
          ],
        ),
        catalogNode(
          "lattice.render",
          "Render",
          [],
          [connectionSocket("out", "Out")],
          [
            valueSocket("point_size", "Point Size", "float"),
            valueSocket("point_color", "Point Color", "color"),
            enumSocket("blend_mode", "Blend Mode", [
              ["normal", "Normal"],
              ["add", "Add"],
              ["screen", "Screen"],
            ]),
          ],
        ),
        catalogNode(
          "lattice.system",
          "System",
          [],
          [connectionSocket("out", "Out")],
          [valueSocket("seed", "Seed", "integer")],
        ),
      ],
    },
    state: {
      version: 1,
      enabled: true,
      document: {
        schema_version: 1,
        output_node: 1,
        nodes: [
          { id: 1, version: 1, label: "Output", type: "lattice.output", data: {} },
          {
            id: 2,
            version: 1,
            label: "Grid Points",
            type: "lattice.points.grid",
            data: { resolution: [10, 10, 1], spacing: 50, max_points: 5000 },
          },
          {
            id: 3,
            version: 1,
            label: "Noise",
            type: "lattice.noise",
            data: {
              enabled: false,
              amplitude: 50,
              frequency: 0.01,
              speed: 1,
              octaves: 2,
              axis_scale: [1, 1, 1],
            },
          },
          {
            id: 4,
            version: 1,
            label: "Links",
            type: "lattice.links",
            data: {
              enabled: true,
              max_distance: 120,
              width: 1,
              opacity_falloff: 0.8,
              color: [1, 1, 1, 1],
            },
          },
          {
            id: 5,
            version: 1,
            label: "Mesh",
            type: "lattice.mesh",
            data: {
              enabled: false,
              max_edge: 150,
              opacity: 0.3,
              color: [0.39215687, 0.5882353, 1, 1],
            },
          },
          {
            id: 6,
            version: 1,
            label: "Render",
            type: "lattice.render",
            data: { point_size: 4, point_color: [1, 1, 1, 1], blend_mode: "normal" },
          },
          {
            id: 7,
            version: 1,
            label: "System",
            type: "lattice.system",
            data: { seed: 12345 },
          },
        ],
        edges: [
          { from: { node: 2, socket: "out" }, to: { node: 1, socket: "points" } },
          { from: { node: 3, socket: "out" }, to: { node: 1, socket: "noise" } },
          { from: { node: 4, socket: "out" }, to: { node: 1, socket: "links" } },
          { from: { node: 5, socket: "out" }, to: { node: 1, socket: "mesh" } },
          { from: { node: 6, socket: "out" }, to: { node: 1, socket: "render" } },
          { from: { node: 7, socket: "out" }, to: { node: 1, socket: "system" } },
        ],
      },
    },
  };
}

function catalogNode(nodeType, label, inputSockets, outputSockets, valueSockets) {
  return {
    node_type: nodeType,
    label,
    version: 1,
    input_sockets: inputSockets,
    output_sockets: outputSockets,
    value_sockets: valueSockets,
  };
}

function connectionSocket(socket, label) {
  return { socket, label };
}

function valueSocket(socket, label, valueType) {
  return { socket, label, value_type: valueType, enum_options: [] };
}

function enumSocket(socket, label, options) {
  return {
    socket,
    label,
    value_type: "enum",
    enum_options: options.map(([value, optionLabel]) => ({ value, label: optionLabel })),
  };
}

function normalizePayload(input) {
  if (input && typeof input === "object" && input.catalog && input.state) {
    return {
      version: input.version ?? 1,
      catalog: normalizeCatalog(input.catalog),
      state: normalizeState(input.state),
    };
  }
  return {
    version: 1,
    catalog: EMPTY_CATALOG,
    state: normalizeState(input),
  };
}

function normalizeCatalog(catalog) {
  return {
    catalog_version: catalog?.catalog_version ?? 0,
    graph_schema_version: catalog?.graph_schema_version ?? 0,
    product_id: catalog?.product_id ?? "unknown",
    product_label: catalog?.product_label ?? "Unknown",
    nodes: Array.isArray(catalog?.nodes) ? catalog.nodes : [],
  };
}

function normalizeState(state) {
  return {
    version: state?.version ?? 0,
    enabled: Boolean(state?.enabled),
    document: state?.document ?? null,
    published_values: Array.isArray(state?.published_values) ? state.published_values : [],
    host_float_bindings: Array.isArray(state?.host_float_bindings)
      ? state.host_float_bindings
      : [],
  };
}

function documentNodes() {
  return Array.isArray(appState.payload.state.document?.nodes)
    ? appState.payload.state.document.nodes
    : [];
}

function documentEdges() {
  return Array.isArray(appState.payload.state.document?.edges)
    ? appState.payload.state.document.edges
    : [];
}

function nodeKey(id) {
  return typeof id === "object" ? JSON.stringify(id) : String(id);
}

function nodeType(node) {
  return node.type ?? node.kind?.type ?? "unknown";
}

function nodeData(node) {
  if (!node.data || typeof node.data !== "object") {
    node.data = {};
  }
  return node.data;
}

function catalogEntry(type) {
  return appState.payload.catalog.nodes.find((entry) => entry.node_type === type);
}

function status(message) {
  elements.statusLine.textContent = message;
}

function render() {
  computeLayout();
  renderSummary();
  renderCatalog();
  renderPublished();
  renderGraph();
  renderInspector();
  renderDiagnostics();
}

function renderSummary() {
  const catalog = appState.payload.catalog;
  const state = appState.payload.state;
  const doc = state.document;
  elements.productLabel.textContent = catalog.product_label || "Unknown";
  elements.schemaLabel.textContent = `catalog ${catalog.catalog_version} / graph ${
    doc?.schema_version ?? "-"
  }`;
  elements.subtitle.textContent = `${catalog.product_id || "unknown"} - ${
    documentNodes().length
  } nodes, ${documentEdges().length} edges`;
}

function renderCatalog() {
  const nodes = appState.payload.catalog.nodes;
  elements.catalogCount.textContent = String(nodes.length);
  elements.catalogList.replaceChildren();
  if (!nodes.length) {
    elements.catalogList.textContent = "No catalog in this JSON.";
    elements.catalogList.className = "list-empty";
    return;
  }
  elements.catalogList.className = "catalog-list";
  for (const entry of nodes) {
    const item = document.createElement("button");
    item.className = "catalog-item";
    item.type = "button";
    item.title = entry.node_type;
    item.innerHTML = `<span><strong>${escapeHtml(entry.label)}</strong><br />${escapeHtml(
      entry.node_type,
    )}</span><span class="socket-count">${entry.value_sockets.length}</span>`;
    item.addEventListener("click", () => {
      const match = documentNodes().find((node) => nodeType(node) === entry.node_type);
      if (match) {
        appState.selectedNodeId = nodeKey(match.id);
        render();
      }
    });
    elements.catalogList.appendChild(item);
  }
}

function renderPublished() {
  const state = appState.payload.state;
  const published = state.document?.published_params ?? [];
  elements.publishedCount.textContent = String(published.length);
  elements.publishedList.replaceChildren();
  if (!published.length) {
    elements.publishedList.className = "list-empty";
    elements.publishedList.textContent = "No published controls.";
    return;
  }
  elements.publishedList.className = "";
  for (const item of published) {
    const override = state.published_values.find((value) => value.stable_id === item.stable_id);
    const binding = state.host_float_bindings.find(
      (hostBinding) => hostBinding.stable_id === item.stable_id,
    );
    const row = document.createElement("div");
    row.className = "published-item";
    row.innerHTML = `<strong>${escapeHtml(item.label || item.stable_id)}</strong><br />
      <span>${escapeHtml(item.stable_id)} / ${escapeHtml(item.value_type)}</span><br />
      <span>${override ? "override set" : "default"}${
        binding ? ` / F${binding.slot}` : ""
      }</span>`;
    elements.publishedList.appendChild(row);
  }
}

function computeLayout() {
  appState.layout = new Map();
  const nodes = documentNodes();
  nodes.forEach((node, index) => {
    const column = index % 3;
    const row = Math.floor(index / 3);
    appState.layout.set(nodeKey(node.id), {
      x: 36 + column * 270,
      y: 36 + row * 160,
      width: 210,
      height: 104,
    });
  });
}

function renderGraph() {
  const nodes = documentNodes();
  elements.nodeLayer.replaceChildren();
  elements.edgeLayer.replaceChildren();
  elements.edgeLayer.setAttribute("width", "1200");
  elements.edgeLayer.setAttribute("height", "900");

  for (const edge of documentEdges()) {
    const from = appState.layout.get(nodeKey(edge.from?.node));
    const to = appState.layout.get(nodeKey(edge.to?.node));
    if (!from || !to) continue;
    const line = document.createElementNS("http://www.w3.org/2000/svg", "line");
    line.setAttribute("x1", String(from.x + from.width));
    line.setAttribute("y1", String(from.y + from.height / 2));
    line.setAttribute("x2", String(to.x));
    line.setAttribute("y2", String(to.y + to.height / 2));
    elements.edgeLayer.appendChild(line);
  }

  for (const node of nodes) {
    const rect = appState.layout.get(nodeKey(node.id));
    const type = nodeType(node);
    const catalog = catalogEntry(type);
    const nodeElement = document.createElement("article");
    nodeElement.className = `node${appState.selectedNodeId === nodeKey(node.id) ? " selected" : ""}`;
    nodeElement.style.left = `${rect.x}px`;
    nodeElement.style.top = `${rect.y}px`;
    nodeElement.innerHTML = `<button type="button">
      <div class="node-title"><strong>${escapeHtml(node.label || catalog?.label || "Node")}</strong>
      <span>${escapeHtml(type)}</span></div>
      <div class="node-body">${Object.keys(nodeData(node)).length} values</div>
      <div class="node-footer"><span>#${escapeHtml(nodeKey(node.id))}</span><span>v${
        node.version ?? 1
      }</span></div>
    </button>`;
    nodeElement.querySelector("button").addEventListener("click", () => {
      appState.selectedNodeId = nodeKey(node.id);
      render();
    });
    elements.nodeLayer.appendChild(nodeElement);
  }
}

function renderInspector() {
  const node = documentNodes().find((item) => nodeKey(item.id) === appState.selectedNodeId);
  elements.inspectorBody.replaceChildren();
  if (!node) {
    elements.selectedNodeType.textContent = "none";
    elements.inspectorBody.className = "list-empty";
    elements.inspectorBody.textContent = "Select a graph node.";
    return;
  }

  const type = nodeType(node);
  const catalog = catalogEntry(type);
  elements.selectedNodeType.textContent = type;
  elements.inspectorBody.className = "inspector-form";

  const labelRow = createTextField("Label", node.label ?? "", (value) => {
    node.label = value;
    render();
  });
  elements.inspectorBody.appendChild(labelRow);

  const data = nodeData(node);
  const valueSockets = catalog?.value_sockets?.length
    ? catalog.value_sockets
    : Object.keys(data).map((key) => ({ socket: key, label: key, value_type: inferValueType(data[key]) }));

  for (const socket of valueSockets) {
    elements.inspectorBody.appendChild(createValueEditor(data, socket));
  }
}

function createTextField(label, value, onChange) {
  const row = document.createElement("div");
  row.className = "field-row";
  const input = document.createElement("input");
  input.value = value;
  input.addEventListener("change", () => onChange(input.value));
  row.appendChild(labelElement(label));
  row.appendChild(input);
  return row;
}

function createValueEditor(data, socket) {
  const row = document.createElement("div");
  row.className = "field-row";
  row.appendChild(labelElement(socket.label || socket.socket));
  const current = data[socket.socket];

  if (socket.value_type === "boolean") {
    const input = document.createElement("input");
    input.type = "checkbox";
    input.checked = Boolean(current);
    input.addEventListener("change", () => {
      data[socket.socket] = input.checked;
      render();
    });
    row.appendChild(input);
    return row;
  }

  if (socket.value_type === "enum") {
    const select = document.createElement("select");
    for (const option of socket.enum_options || []) {
      const item = document.createElement("option");
      item.value = option.value;
      item.textContent = option.label || option.value;
      select.appendChild(item);
    }
    select.value = current ?? select.options[0]?.value ?? "";
    select.addEventListener("change", () => {
      data[socket.socket] = select.value;
      render();
    });
    row.appendChild(select);
    return row;
  }

  if (socket.value_type === "vector3" || socket.value_type === "color") {
    const input = document.createElement("input");
    input.value = Array.isArray(current) ? current.join(", ") : "";
    input.placeholder = socket.value_type === "color" ? "r, g, b, a" : "x, y, z";
    input.addEventListener("change", () => {
      data[socket.socket] = input.value
        .split(",")
        .map((part) => Number(part.trim()))
        .filter((value) => Number.isFinite(value));
      render();
    });
    row.appendChild(input);
    return row;
  }

  const input = document.createElement("input");
  input.type = socket.value_type === "integer" || socket.value_type === "float" ? "number" : "text";
  input.step = socket.value_type === "integer" ? "1" : "any";
  input.value = current ?? "";
  input.addEventListener("change", () => {
    if (socket.value_type === "integer") {
      data[socket.socket] = Number.parseInt(input.value, 10) || 0;
    } else if (socket.value_type === "float") {
      data[socket.socket] = Number.parseFloat(input.value) || 0;
    } else {
      data[socket.socket] = input.value;
    }
    render();
  });
  row.appendChild(input);
  return row;
}

function labelElement(text) {
  const label = document.createElement("label");
  label.textContent = text;
  return label;
}

function inferValueType(value) {
  if (typeof value === "boolean") return "boolean";
  if (typeof value === "number") return Number.isInteger(value) ? "integer" : "float";
  if (Array.isArray(value)) return value.length === 4 ? "color" : "vector3";
  return "string";
}

function renderDiagnostics() {
  const diagnostics = collectDiagnostics();
  elements.diagnosticCount.textContent = String(diagnostics.length);
  elements.diagnosticList.replaceChildren();
  if (!diagnostics.length) {
    elements.diagnosticList.innerHTML = '<div class="list-empty">No diagnostics.</div>';
    return;
  }
  for (const diagnostic of diagnostics) {
    const item = document.createElement("div");
    item.className = `diagnostic-item ${diagnostic.level}`;
    item.textContent = diagnostic.message;
    elements.diagnosticList.appendChild(item);
  }
}

function collectDiagnostics() {
  const diagnostics = [];
  const state = appState.payload.state;
  const doc = state.document;
  if (!doc) {
    diagnostics.push({
      level: state.enabled ? "error" : "warn",
      message: "No graph document in state.",
    });
    return diagnostics;
  }
  if (!appState.payload.catalog.nodes.length) {
    diagnostics.push({ level: "warn", message: "No catalog is available for this payload." });
  }
  const ids = new Set();
  const nodesById = new Map();
  for (const node of documentNodes()) {
    const key = nodeKey(node.id);
    if (ids.has(key)) {
      diagnostics.push({ level: "error", message: `Duplicate node id ${key}.` });
    }
    ids.add(key);
    nodesById.set(key, node);
    const entry = catalogEntry(nodeType(node));
    if (!entry) {
      diagnostics.push({ level: "warn", message: `No catalog entry for ${nodeType(node)}.` });
      continue;
    }
    const data = nodeData(node);
    for (const socket of entry.value_sockets || []) {
      if (!Object.prototype.hasOwnProperty.call(data, socket.socket)) {
        diagnostics.push({
          level: "error",
          message: `${node.label || nodeType(node)} is missing value ${socket.socket}.`,
        });
        continue;
      }
      if (!valueMatchesSocket(data[socket.socket], socket)) {
        diagnostics.push({
          level: "error",
          message: `${node.label || nodeType(node)} value ${socket.socket} is not ${socket.value_type}.`,
        });
      }
    }
  }
  if (!ids.has(nodeKey(doc.output_node))) {
    diagnostics.push({ level: "error", message: `Output node ${nodeKey(doc.output_node)} missing.` });
  }
  for (const edge of documentEdges()) {
    const source = nodesById.get(nodeKey(edge.from?.node));
    const target = nodesById.get(nodeKey(edge.to?.node));
    if (!source) {
      diagnostics.push({ level: "error", message: `Edge source ${nodeKey(edge.from?.node)} missing.` });
    }
    if (!target) {
      diagnostics.push({ level: "error", message: `Edge target ${nodeKey(edge.to?.node)} missing.` });
    }
    const sourceEntry = source ? catalogEntry(nodeType(source)) : null;
    const targetEntry = target ? catalogEntry(nodeType(target)) : null;
    if (sourceEntry && !hasConnectionSocket(sourceEntry.output_sockets, edge.from?.socket)) {
      diagnostics.push({
        level: "error",
        message: `${nodeType(source)} has no output socket ${edge.from?.socket}.`,
      });
    }
    if (targetEntry && !hasConnectionSocket(targetEntry.input_sockets, edge.to?.socket)) {
      diagnostics.push({
        level: "error",
        message: `${nodeType(target)} has no input socket ${edge.to?.socket}.`,
      });
    }
  }
  collectPublishedDiagnostics(diagnostics, doc, nodesById);
  if (state.version > maxSnapshotVersion(appState.payload.catalog.product_id)) {
    diagnostics.push({ level: "warn", message: `Snapshot version ${state.version} may be newer than this shell.` });
  }
  return diagnostics;
}

function hasConnectionSocket(sockets, socket) {
  return Array.isArray(sockets) && sockets.some((entry) => entry.socket === socket);
}

function valueMatchesSocket(value, socket) {
  if (socket.value_type === "boolean") return typeof value === "boolean";
  if (socket.value_type === "float") return typeof value === "number" && Number.isFinite(value);
  if (socket.value_type === "integer") return Number.isInteger(value);
  if (socket.value_type === "vector3") {
    return Array.isArray(value) && value.length === 3 && value.every((item) => Number.isFinite(item));
  }
  if (socket.value_type === "color") {
    return Array.isArray(value) && value.length === 4 && value.every((item) => Number.isFinite(item));
  }
  if (socket.value_type === "enum") {
    return (
      typeof value === "string" &&
      (socket.enum_options || []).some((option) => option.value === value)
    );
  }
  return true;
}

function collectPublishedDiagnostics(diagnostics, doc, nodesById) {
  const published = Array.isArray(doc.published_params) ? doc.published_params : [];
  const stableIds = new Set();
  for (const item of published) {
    if (!item.stable_id) {
      diagnostics.push({ level: "error", message: "Published param has an empty stable id." });
      continue;
    }
    if (stableIds.has(item.stable_id)) {
      diagnostics.push({ level: "error", message: `Duplicate published id ${item.stable_id}.` });
    }
    stableIds.add(item.stable_id);
    const target = nodesById.get(nodeKey(item.target?.node));
    const targetSocket = target
      ? (catalogEntry(nodeType(target))?.value_sockets || []).find(
          (socket) => socket.socket === item.target?.socket,
        )
      : null;
    if (!target) {
      diagnostics.push({ level: "error", message: `Published target ${nodeKey(item.target?.node)} missing.` });
    } else if (!targetSocket) {
      diagnostics.push({
        level: "error",
        message: `Published target socket ${item.target?.socket} missing.`,
      });
    } else if (targetSocket.value_type !== item.value_type) {
      diagnostics.push({
        level: "error",
        message: `Published ${item.stable_id} type does not match target socket.`,
      });
    }
    if (!publishedValueMatchesType(item.default_value, item.value_type)) {
      diagnostics.push({
        level: "error",
        message: `Published ${item.stable_id} default value is not ${item.value_type}.`,
      });
    }
  }

  const valueIds = new Set();
  for (const override of appState.payload.state.published_values) {
    const publishedParam = published.find((item) => item.stable_id === override.stable_id);
    if (valueIds.has(override.stable_id)) {
      diagnostics.push({ level: "error", message: `Duplicate override ${override.stable_id}.` });
    }
    valueIds.add(override.stable_id);
    if (!publishedParam) {
      diagnostics.push({ level: "warn", message: `Override ${override.stable_id} has no published param.` });
    } else if (!publishedValueMatchesType(override.value, publishedParam.value_type)) {
      diagnostics.push({
        level: "error",
        message: `Override ${override.stable_id} is not ${publishedParam.value_type}.`,
      });
    }
  }

  const bindingSlots = new Set();
  const bindingIds = new Set();
  for (const binding of appState.payload.state.host_float_bindings) {
    const publishedParam = published.find((item) => item.stable_id === binding.stable_id);
    if (!Number.isInteger(binding.slot) || binding.slot < 1 || binding.slot > HOST_FLOAT_SLOT_COUNT) {
      diagnostics.push({ level: "error", message: `Host float slot ${binding.slot} is invalid.` });
    }
    if (bindingSlots.has(binding.slot)) {
      diagnostics.push({ level: "error", message: `Duplicate host float slot ${binding.slot}.` });
    }
    bindingSlots.add(binding.slot);
    if (bindingIds.has(binding.stable_id)) {
      diagnostics.push({ level: "error", message: `Duplicate host binding ${binding.stable_id}.` });
    }
    bindingIds.add(binding.stable_id);
    if (!publishedParam) {
      diagnostics.push({ level: "warn", message: `Host binding ${binding.stable_id} has no published param.` });
    } else if (publishedParam.value_type !== "float") {
      diagnostics.push({ level: "error", message: `Host binding ${binding.stable_id} is not a float.` });
    }
  }
}

function publishedValueMatchesType(value, valueType) {
  if (!value || value.type !== valueType) return false;
  if (valueType === "enum") return typeof value.value === "string";
  return valueMatchesSocket(value.value, { value_type: valueType, enum_options: [] });
}

function maxSnapshotVersion(productId) {
  return productId === "latticelab" ? LATTICELAB_SNAPSHOT_VERSION : PARTICLELAB_SNAPSHOT_VERSION;
}

function parseAndLoadJson(text, sourceLabel) {
  try {
    const parsed = JSON.parse(text);
    appState.payload = normalizePayload(parsed);
    appState.selectedNodeId = documentNodes()[0] ? nodeKey(documentNodes()[0].id) : null;
    render();
    status(`Loaded ${sourceLabel}.`);
  } catch (error) {
    status(`JSON parse failed: ${error.message}`);
  }
}

function saveJson(filename, data) {
  const blob = new Blob([JSON.stringify(data, null, 2)], { type: "application/json" });
  const link = document.createElement("a");
  link.href = URL.createObjectURL(blob);
  link.download = filename;
  document.body.appendChild(link);
  link.click();
  link.remove();
  URL.revokeObjectURL(link.href);
}

function canSaveCurrentPayload() {
  const errors = collectDiagnostics().filter((diagnostic) => diagnostic.level === "error");
  if (errors.length) {
    status(`Fix ${errors.length} blocking diagnostic(s) before saving JSON.`);
    return false;
  }
  return true;
}

function escapeHtml(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

elements.loadFileButton.addEventListener("click", () => elements.fileInput.click());
elements.fileInput.addEventListener("change", async () => {
  const file = elements.fileInput.files?.[0];
  if (!file) return;
  parseAndLoadJson(await file.text(), file.name);
  elements.fileInput.value = "";
});

elements.pasteButton.addEventListener("click", () => {
  elements.pasteInput.value = JSON.stringify(appState.payload, null, 2);
  elements.pasteDialog.showModal();
});

elements.applyPasteButton.addEventListener("click", () => {
  parseAndLoadJson(elements.pasteInput.value, "pasted JSON");
  elements.pasteDialog.close();
});

elements.particleDemoButton.addEventListener("click", () => {
  appState.payload = createParticleDemoPayload();
  appState.selectedNodeId = "1";
  render();
  status("Loaded ParticleLab demo payload.");
});

elements.latticeDemoButton.addEventListener("click", () => {
  appState.payload = createLatticeDemoPayload();
  appState.selectedNodeId = "1";
  render();
  status("Loaded Lattice Lab demo payload.");
});

elements.saveBootstrapButton.addEventListener("click", () => {
  if (!canSaveCurrentPayload()) return;
  saveJson("node-ui-bootstrap.json", appState.payload);
  status("Saved bootstrap JSON.");
});

elements.saveSnapshotButton.addEventListener("click", () => {
  if (!canSaveCurrentPayload()) return;
  saveJson("node-ui-snapshot.json", appState.payload.state);
  status("Saved snapshot JSON.");
});

if (
  window.PARTICLELAB_NODE_UI_BOOTSTRAP &&
  typeof window.PARTICLELAB_NODE_UI_BOOTSTRAP === "object"
) {
  appState.payload = normalizePayload(window.PARTICLELAB_NODE_UI_BOOTSTRAP);
}

appState.selectedNodeId = documentNodes()[0] ? nodeKey(documentNodes()[0].id) : "1";
render();
if (window.PARTICLELAB_NODE_UI_BOOTSTRAP_SOURCE) {
  status(`Loaded ${window.PARTICLELAB_NODE_UI_BOOTSTRAP_SOURCE}.`);
}
