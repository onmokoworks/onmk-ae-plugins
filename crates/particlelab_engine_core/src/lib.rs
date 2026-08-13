pub mod engine;
pub mod node_graph_core;
pub mod particle;
pub mod render_core;
pub mod renderer;

pub mod prelude {
    pub use crate::engine::{
        render_particle_engine_8bit, ParticleEngineConfig, ParticleRenderPlan,
        ParticleRuntimeInputs,
    };
    pub use crate::node_graph_core::{
        node_map, output_input, CoreGraphEdge, CoreGraphNode, NodeUiCatalog, NodeUiCatalogEntry,
        NodeUiConnectionSocket, NodeUiEnumOption, NodeUiValueSocket, NodeUiValueType,
        NODE_UI_CATALOG_VERSION,
    };
    pub use crate::particle::{
        AppearanceConfig, ChildConfig, EmitterConfig, EmitterType, Particle, ParticleSystem,
        PhysicsConfig,
    };
    pub use crate::render_core::{
        blend_argb8_pixel, ArgbBlendMode, RenderFrame, RenderSurface, RenderSurfaceError,
    };
    pub use crate::renderer::{
        apply_final_composite, blit_argb_into, invert_camera_matrix, project_point_3d,
        render_particles_8bit, ApplyMode, BlendMode, CameraProjection, ImageColorMode,
        ImageFitMode, ImageSamplingConfig, MipLevel, ParticleShape, RenderConfig, SpriteImage,
        TimeSamplingMode,
    };
}
