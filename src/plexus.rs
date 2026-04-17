use glam::Vec3;

pub const MAX_PLEXUS_POINTS: usize = 10_000;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PointSourceType {
    Grid,
    Layer,
    ObjFile,
    AELights,
    Particles,
}

#[derive(Clone, Debug)]
pub struct PointGroupConfig {
    pub enabled: bool,
    pub source_type: PointSourceType,
    pub grid_res_x: u32,
    pub grid_res_y: u32,
    pub grid_res_z: u32,
    pub grid_spacing: f32,
    pub max_points: usize,
    pub source_points: Option<std::sync::Arc<Vec<Vec3>>>,
}

impl Default for PointGroupConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            source_type: PointSourceType::Grid,
            grid_res_x: 10,
            grid_res_y: 10,
            grid_res_z: 1,
            grid_spacing: 50.0,
            max_points: 5000,
            source_points: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PlexusConfig {
    pub point_a: PointGroupConfig,
    pub point_b: PointGroupConfig,
    pub point_size: f32,
    pub point_color: [f32; 4],
}

impl Default for PlexusConfig {
    fn default() -> Self {
        Self {
            point_a: PointGroupConfig::default(),
            point_b: PointGroupConfig { enabled: false, ..Default::default() },
            point_size: 4.0,
            point_color: [1.0, 1.0, 1.0, 1.0],
        }
    }
}

pub struct PlexusPointCloud {
    pub groups: [Vec<Vec3>; 2],
    pub colors: [[f32; 4]; 2],
}

pub fn generate_points(config: &PlexusConfig, center_x: f32, center_y: f32) -> PlexusPointCloud {
    let mut cloud = PlexusPointCloud {
        groups: [Vec::new(), Vec::new()],
        colors: [config.point_color, config.point_color],
    };

    if config.point_a.enabled {
        cloud.groups[0] = generate_group_points(&config.point_a, center_x, center_y);
    }
    if config.point_b.enabled {
        cloud.groups[1] = generate_group_points(&config.point_b, center_x, center_y);
    }

    cloud
}

fn generate_group_points(cfg: &PointGroupConfig, center_x: f32, center_y: f32) -> Vec<Vec3> {
    match cfg.source_type {
        PointSourceType::Grid => generate_grid_points(cfg, center_x, center_y),
        PointSourceType::Layer => {
            if let Some(ref pts) = cfg.source_points {
                let mut out = pts.as_ref().clone();
                out.truncate(cfg.max_points.min(MAX_PLEXUS_POINTS));
                out
            } else {
                Vec::new()
            }
        }
        _ => Vec::new(),
    }
}

fn generate_grid_points(cfg: &PointGroupConfig, center_x: f32, center_y: f32) -> Vec<Vec3> {
    let rx = cfg.grid_res_x.max(1);
    let ry = cfg.grid_res_y.max(1);
    let rz = cfg.grid_res_z.max(1);
    let total = (rx as usize) * (ry as usize) * (rz as usize);
    let cap = total.min(cfg.max_points).min(MAX_PLEXUS_POINTS);

    let mut points = Vec::with_capacity(cap);
    let spacing = cfg.grid_spacing;
    let offset_x = center_x - (rx as f32 - 1.0) * spacing * 0.5;
    let offset_y = center_y - (ry as f32 - 1.0) * spacing * 0.5;
    let offset_z = -(rz as f32 - 1.0) * spacing * 0.5;

    for iz in 0..rz {
        for iy in 0..ry {
            for ix in 0..rx {
                if points.len() >= cap {
                    return points;
                }
                points.push(Vec3::new(
                    offset_x + ix as f32 * spacing,
                    offset_y + iy as f32 * spacing,
                    offset_z + iz as f32 * spacing,
                ));
            }
        }
    }

    points
}
