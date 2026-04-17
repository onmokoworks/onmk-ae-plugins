use glam::{FloatExt, Quat, Vec2, Vec3};
use std::sync::Arc;

const MIN_SIM_STEP: f32 = 1.0 / 30.0;
const MAX_LIVE_PARTICLES: usize = 8_000;

#[derive(Clone, Copy, Debug)]
pub enum EmitterType {
    Point,
    Box,
    Sphere,
    Grid,
    LayerAlpha,
    Path,
}

#[derive(Clone, Debug)]
pub struct EmitterConfig {
    pub emitter_type: EmitterType,
    pub position: Vec3,
    pub size: Vec3,
    pub source_points: Option<Arc<Vec<Vec3>>>,
    pub birth_rate: f32,
    pub lifespan: f32,
    pub lifespan_variation: f32,
    pub initial_speed: f32,
    pub speed_variation: f32,
    pub initial_direction: Vec3,
    pub spread: f32,
    pub initial_size: f32,
    pub size_variation: f32,
    pub initial_rotation: f32,
    pub rotation_speed: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct PhysicsConfig {
    pub gravity: Vec3,
    pub wind: Vec3,
    pub air_resistance: f32,
    pub turbulence_strength: f32,
    pub turbulence_scale: f32,
    pub turbulence_speed: f32,
    pub bounce_floor_y: f32,
    pub bounce_enabled: bool,
    pub bounce_damping: f32,
}

#[derive(Clone, Copy, Debug)]
enum ForceField {
    Gravity(Vec3),
    Wind(Vec3),
    Turbulence {
        strength: f32,
        scale: f32,
        speed: f32,
    },
}

#[derive(Clone, Copy, Debug)]
pub struct AppearanceConfig {
    pub color_start: [f32; 4],
    pub color_end: [f32; 4],
    pub size_over_life: [f32; 4],
    pub opacity_over_life: [f32; 4],
}

#[derive(Clone, Copy, Debug)]
pub struct ChildConfig {
    pub enabled: bool,
    pub count: u32,
    pub inherit_velocity: f32,
    pub lifespan: f32,
    pub initial_speed: f32,
    pub spread: f32,
    pub size_scale: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct Particle {
    pub position: Vec3,
    pub velocity: Vec3,
    pub color: [f32; 4],
    pub size: f32,
    pub rotation: f32,
    pub age: f32,
    pub lifespan: f32,
}

pub struct ParticleSystem {
    emitter: EmitterConfig,
    physics: PhysicsConfig,
    appearance: AppearanceConfig,
    child: ChildConfig,
    seed: u64,
    particles: Vec<Particle>,
}

impl ParticleSystem {
    pub fn new(
        emitter: EmitterConfig,
        physics: PhysicsConfig,
        appearance: AppearanceConfig,
        child: ChildConfig,
        seed: u64,
    ) -> Self {
        Self {
            emitter,
            physics,
            appearance,
            child,
            seed,
            particles: Vec::new(),
        }
    }

    pub fn simulate_to_time(&mut self, time: f32, dt: f32) {
        self.particles.clear();

        let max_lifetime = self.max_particle_lifetime();
        let start_time = (time - max_lifetime).max(0.0);
        let spawn_stride = self.spawn_stride_for_window(start_time, time, dt);
        self.reserve_particles(max_lifetime, spawn_stride);
        self.spawn_particles_for_window(start_time, time, dt, spawn_stride);
    }

    pub fn get_particles(&self) -> &[Particle] {
        &self.particles
    }

    fn reserve_particles(&mut self, max_lifetime: f32, spawn_stride: u64) {
        let parent_count = (self.emitter.birth_rate.max(0.0) * max_lifetime).ceil() as usize;
        let child_multiplier = if self.child.enabled {
            1usize.saturating_add(self.child.count as usize)
        } else {
            1
        };
        let estimated = parent_count
            .saturating_mul(child_multiplier)
            .saturating_div(spawn_stride.max(1) as usize);
        self.particles.reserve(estimated.min(MAX_LIVE_PARTICLES));
    }

    fn max_particle_lifetime(&self) -> f32 {
        let parent_max = self.emitter.lifespan * (1.0 + self.emitter.lifespan_variation.clamp(0.0, 1.0));
        let child_max = if self.child.enabled { self.child.lifespan.max(0.0) } else { 0.0 };
        parent_max.max(child_max).max(0.01)
    }

    fn spawn_particles_for_window(&mut self, start_time: f32, end_time: f32, dt: f32, spawn_stride: u64) {
        let sample_dt = dt.max(MIN_SIM_STEP);
        let spawn_interval = (1.0 / self.emitter.birth_rate.max(1.0)).max(sample_dt * 0.25);
        let first_index = (start_time / spawn_interval).floor().max(0.0) as u64;
        let last_index = (end_time / spawn_interval).ceil().max(0.0) as u64;
        let spawn_stride = spawn_stride.max(1);

        for spawn_index in first_index..=last_index {
            if self.particles.len() >= MAX_LIVE_PARTICLES {
                break;
            }
            if (spawn_index - first_index) % spawn_stride != 0 {
                continue;
            }
            let spawn_time = spawn_index as f32 * spawn_interval;
            if spawn_time > end_time {
                break;
            }

            let mut particle = self.spawn_particle(spawn_time, spawn_index);
            let age = end_time - spawn_time;
            if age < 0.0 {
                continue;
            }
            Self::evaluate_particle(
                &mut particle,
                self.physics,
                self.appearance,
                self.emitter.rotation_speed,
                end_time,
                age,
            );

            if particle.age < particle.lifespan && particle.color[3] > 0.0 && particle.size > 0.01 {
                self.particles.push(particle);
                if self.child.enabled && self.particles.len() < MAX_LIVE_PARTICLES {
                    self.spawn_children(end_time, particle, spawn_index);
                }
            }
        }
    }

    fn spawn_stride_for_window(&self, start_time: f32, end_time: f32, dt: f32) -> u64 {
        let sample_dt = dt.max(MIN_SIM_STEP);
        let spawn_interval = (1.0 / self.emitter.birth_rate.max(1.0)).max(sample_dt * 0.25);
        let first_index = (start_time / spawn_interval).floor().max(0.0) as u64;
        let last_index = (end_time / spawn_interval).ceil().max(0.0) as u64;
        let parent_spawns = last_index.saturating_sub(first_index).saturating_add(1) as usize;
        let child_multiplier = if self.child.enabled {
            1usize.saturating_add(self.child.count as usize)
        } else {
            1
        };
        let estimated_total = parent_spawns.saturating_mul(child_multiplier);
        let stride = estimated_total.div_ceil(MAX_LIVE_PARTICLES).max(1);
        stride as u64
    }

    fn spawn_children(&mut self, time: f32, parent: Particle, index: u64) {
        for child_index in 0..self.child.count {
            if self.particles.len() >= MAX_LIVE_PARTICLES {
                break;
            }
            let spawn_seed = index.wrapping_mul(131) + child_index as u64;
            let mut particle = self.spawn_particle(time, spawn_seed);
            particle.position = parent.position;
            particle.velocity =
                parent.velocity * self.child.inherit_velocity + self.random_direction(time, child_index as u64) * self.child.initial_speed;
            particle.lifespan = self.child.lifespan.max(0.01);
            particle.size = (particle.size * self.child.size_scale).max(0.1);
            Self::evaluate_particle(
                &mut particle,
                self.physics,
                self.appearance,
                self.emitter.rotation_speed,
                time,
                0.0,
            );
            if particle.color[3] > 0.0 && particle.size > 0.01 {
                self.particles.push(particle);
            }
        }
    }

    fn spawn_particle(&self, time: f32, index: u64) -> Particle {
        let mut rng = fastrand::Rng::with_seed(self.seed ^ ((time.to_bits() as u64) << 1) ^ index.wrapping_mul(0x9E37_79B9));
        let lifespan = varied(self.emitter.lifespan, self.emitter.lifespan_variation, &mut rng).max(0.01);
        let speed = varied(self.emitter.initial_speed, self.emitter.speed_variation, &mut rng).max(0.0);
        let size = varied(self.emitter.initial_size, self.emitter.size_variation, &mut rng).max(0.1);
        let direction = self.random_direction_with_rng(&mut rng, self.emitter.spread.max(self.child.spread.min(self.emitter.spread)));
        let position = self.random_emitter_position(&mut rng);

        Particle {
            position,
            velocity: direction * speed,
            color: self.appearance.color_start,
            size,
            rotation: self.emitter.initial_rotation + self.emitter.rotation_speed * time,
            age: 0.0,
            lifespan,
        }
    }

    fn evaluate_particle(
        particle: &mut Particle,
        physics: PhysicsConfig,
        appearance: AppearanceConfig,
        rotation_speed: f32,
        time: f32,
        age: f32,
    ) {
        let age = age.max(0.0);
        let acceleration = Self::evaluate_force_stack(physics, particle.position, time);
        let initial_velocity = particle.velocity;

        // Apply air resistance as velocity damping
        let drag = physics.air_resistance.clamp(0.0, 20.0);
        let vel_damping = if drag > 0.0 { (-drag * age).exp() } else { 1.0 };

        particle.position += initial_velocity * age * vel_damping + 0.5 * acceleration * age * age;
        particle.velocity = initial_velocity * vel_damping + acceleration * age;
        particle.rotation += rotation_speed * age;
        particle.age = age;

        if physics.bounce_enabled && particle.position.y >= physics.bounce_floor_y {
            particle.position.y = physics.bounce_floor_y;
            particle.velocity.y = -particle.velocity.y.abs() * physics.bounce_damping.clamp(0.0, 1.0);
        }

        let life_t = (particle.age / particle.lifespan).clamp(0.0, 1.0);
        let size_mult = eval_curve(appearance.size_over_life, life_t);
        let alpha_mult = eval_curve(appearance.opacity_over_life, life_t);
        particle.size = particle.size.max(0.1) * size_mult.max(0.0);
        particle.color = lerp_color(appearance.color_start, appearance.color_end, life_t);
        particle.color[3] = alpha_mult.clamp(0.0, 1.0);
    }

    fn evaluate_force_stack(physics: PhysicsConfig, position: Vec3, time: f32) -> Vec3 {
        let mut acceleration = Vec3::ZERO;
        for force in Self::force_stack(physics) {
            acceleration += match force {
                ForceField::Gravity(value) => value,
                ForceField::Wind(value) => value,
                ForceField::Turbulence { strength, scale, speed } => {
                    Self::turbulence_force(strength, scale, speed, position, time)
                }
            };
        }
        acceleration
    }

    fn force_stack(physics: PhysicsConfig) -> [ForceField; 3] {
        [
            ForceField::Gravity(physics.gravity),
            ForceField::Wind(physics.wind),
            ForceField::Turbulence {
                strength: physics.turbulence_strength,
                scale: physics.turbulence_scale,
                speed: physics.turbulence_speed,
            },
        ]
    }

    fn random_emitter_position(&self, rng: &mut fastrand::Rng) -> Vec3 {
        match self.emitter.emitter_type {
            EmitterType::Point => self.emitter.position,
            EmitterType::Box => {
                let offset = Vec3::new(
                    rng.f32() - 0.5,
                    rng.f32() - 0.5,
                    rng.f32() - 0.5,
                ) * self.emitter.size;
                self.emitter.position + offset
            }
            EmitterType::Sphere => {
                let dir = random_unit_vector(rng);
                let radius = rng.f32();
                self.emitter.position + dir * (self.emitter.size * radius)
            }
            EmitterType::Grid => {
                let gx = (rng.i32(0..8) as f32 / 7.0) - 0.5;
                let gy = (rng.i32(0..8) as f32 / 7.0) - 0.5;
                let gz = (rng.i32(0..8) as f32 / 7.0) - 0.5;
                self.emitter.position + Vec3::new(
                    gx * self.emitter.size.x,
                    gy * self.emitter.size.y,
                    gz * self.emitter.size.z,
                )
            }
            EmitterType::LayerAlpha | EmitterType::Path => {
                if let Some(points) = &self.emitter.source_points {
                    if !points.is_empty() {
                        return self.emitter.position + points[rng.usize(0..points.len())];
                    }
                }
                self.emitter.position
            }
        }
    }

    fn random_direction(&self, time: f32, index: u64) -> Vec3 {
        let mut rng = fastrand::Rng::with_seed(self.seed ^ time.to_bits() as u64 ^ index);
        self.random_direction_with_rng(&mut rng, self.child.spread.max(self.emitter.spread))
    }

    fn random_direction_with_rng(&self, rng: &mut fastrand::Rng, spread: f32) -> Vec3 {
        let base = if self.emitter.initial_direction.length_squared() > 0.0 {
            self.emitter.initial_direction.normalize()
        } else {
            Vec3::Y
        };
        let axis = if base.z.abs() < 0.99 { Vec3::Z } else { Vec3::X };
        let tangent = base.cross(axis).normalize_or_zero();
        let bitangent = base.cross(tangent).normalize_or_zero();
        let angle = (rng.f32() - 0.5) * spread * 2.0;
        let radial = rng.f32() * std::f32::consts::TAU;
        let offset = tangent * radial.cos() + bitangent * radial.sin();
        (Quat::from_axis_angle(offset.normalize_or_zero().max(Vec3::X).normalize(), angle) * base).normalize_or_zero()
    }

    fn turbulence_force(strength: f32, scale: f32, speed: f32, position: Vec3, time: f32) -> Vec3 {
        if strength <= 0.0 {
            return Vec3::ZERO;
        }
        let scale = scale.max(0.0001);
        let t = time * speed;
        let sample = Vec3::new(
            ((position.x / scale) + t).sin(),
            ((position.y / scale) - t * 1.7).cos(),
            ((position.x + position.y) / scale + t * 0.7).sin(),
        );
        sample.normalize_or_zero() * strength
    }
}

fn varied(base: f32, variation: f32, rng: &mut fastrand::Rng) -> f32 {
    let offset = (rng.f32() - 0.5) * 2.0 * variation.clamp(0.0, 1.0);
    base * (1.0 + offset)
}

fn random_unit_vector(rng: &mut fastrand::Rng) -> Vec3 {
    let angle = rng.f32() * std::f32::consts::TAU;
    let z = (rng.f32() - 0.5) * 2.0;
    let r = (1.0 - z * z).sqrt();
    Vec3::new(r * angle.cos(), r * angle.sin(), z)
}

fn eval_curve(points: [f32; 4], t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t <= (1.0 / 3.0) {
        let local_t = t * 3.0;
        points[0].lerp(points[1], local_t)
    } else if t <= (2.0 / 3.0) {
        let local_t = (t - (1.0 / 3.0)) * 3.0;
        points[1].lerp(points[2], local_t)
    } else {
        let local_t = (t - (2.0 / 3.0)) * 3.0;
        points[2].lerp(points[3], local_t)
    }
}

fn lerp_color(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    [
        a[0].lerp(b[0], t),
        a[1].lerp(b[1], t),
        a[2].lerp(b[2], t),
        a[3].lerp(b[3], t),
    ]
}

#[allow(dead_code)]
fn _vec2(_v: Vec2) {}
