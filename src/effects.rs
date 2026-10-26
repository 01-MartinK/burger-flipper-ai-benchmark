//! Particles (grease, smoke, flames, sparkles) and camera shake.

use bevy::light::NotShadowCaster;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;

use crate::game::GameRng;

pub fn plugin(app: &mut App) {
    app.add_message::<Burst>()
        .init_resource::<Shake>()
        .add_systems(Startup, load_particle_assets)
        .add_systems(Update, (spawn_bursts, update_particles, shake_camera));
}

/// Smallest scale a particle is drawn at.
const MIN_SIZE: f32 = 0.001;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ParticleKind {
    Grease,
    Steam,
    Smoke,
    Flame,
    Ember,
    Sparkle,
}

/// Request to spawn `count` particles of one kind around a point.
#[derive(Message, Clone, Copy)]
pub struct Burst {
    pub kind: ParticleKind,
    pub at: Vec3,
    pub count: u32,
    /// Radius of the disc (on the XZ plane) the particles start in.
    pub spread: f32,
}

impl Burst {
    pub fn new(kind: ParticleKind, at: Vec3, count: u32) -> Self {
        Self {
            kind,
            at,
            count,
            spread: 0.0,
        }
    }

    pub fn spread(mut self, radius: f32) -> Self {
        self.spread = radius;
        self
    }
}

/// Screen shake, using the "trauma" approach: shake grows with trauma squared.
#[derive(Resource, Default)]
pub struct Shake {
    trauma: f32,
}

impl Shake {
    pub fn add(&mut self, trauma: f32) {
        self.trauma = (self.trauma + trauma).min(1.0);
    }
}

/// Resting transform of the camera; shake is applied on top of it.
#[derive(Component)]
pub struct CameraRig {
    pub base: Transform,
}

#[derive(Component)]
struct Particle {
    velocity: Vec3,
    /// Downward acceleration; negative values make particles rise faster.
    gravity: f32,
    drag: f32,
    age: f32,
    lifetime: f32,
    start_size: f32,
    end_size: f32,
}

#[derive(Resource)]
struct ParticleAssets {
    mesh: Handle<Mesh>,
    materials: HashMap<ParticleKind, Handle<StandardMaterial>>,
}

fn load_particle_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Additive blending makes overlapping sparks and flames glow brighter, like real fire.
    let mut glowing = |color: LinearRgba| {
        materials.add(StandardMaterial {
            base_color: color.into(),
            alpha_mode: AlphaMode::Add,
            unlit: true,
            ..default()
        })
    };
    let grease = glowing(LinearRgba::rgb(3.0, 2.4, 1.2));
    let flame = glowing(LinearRgba::rgb(3.0, 0.9, 0.12));
    let ember = glowing(LinearRgba::rgb(4.0, 0.8, 0.08));
    let sparkle = glowing(LinearRgba::rgb(5.0, 4.0, 1.2));
    let mut hazy = |color: Color| {
        materials.add(StandardMaterial {
            base_color: color,
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        })
    };
    let steam = hazy(Color::srgba(0.9, 0.9, 0.9, 0.14));
    let smoke = hazy(Color::srgba(0.1, 0.09, 0.09, 0.5));

    commands.insert_resource(ParticleAssets {
        mesh: meshes.add(Sphere::new(1.0).mesh().ico(2).unwrap()),
        materials: HashMap::from_iter([
            (ParticleKind::Grease, grease),
            (ParticleKind::Steam, steam),
            (ParticleKind::Smoke, smoke),
            (ParticleKind::Flame, flame),
            (ParticleKind::Ember, ember),
            (ParticleKind::Sparkle, sparkle),
        ]),
    });
}

fn spawn_bursts(
    mut commands: Commands,
    mut bursts: MessageReader<Burst>,
    assets: Res<ParticleAssets>,
    mut rng: ResMut<GameRng>,
) {
    for burst in bursts.read() {
        let material = assets.materials[&burst.kind].clone();
        for _ in 0..burst.count {
            let offset = rng.direction() * burst.spread * rng.f32().sqrt();
            let at = burst.at + Vec3::new(offset.x, 0.0, offset.y);
            let particle = new_particle(burst.kind, &mut rng);
            commands.spawn((
                Mesh3d(assets.mesh.clone()),
                MeshMaterial3d(material.clone()),
                // Start tiny but never at zero scale, which would make the mesh transform singular.
                Transform::from_translation(at).with_scale(Vec3::splat(MIN_SIZE)),
                NotShadowCaster,
                particle,
            ));
        }
    }
}

fn new_particle(kind: ParticleKind, rng: &mut GameRng) -> Particle {
    let sideways = rng.direction();
    let mut particle = Particle {
        velocity: Vec3::ZERO,
        gravity: 0.0,
        drag: 0.0,
        age: 0.0,
        lifetime: 1.0,
        start_size: 0.1,
        end_size: 0.0,
    };
    match kind {
        ParticleKind::Grease => {
            let speed = rng.range(0.4, 1.4);
            particle.velocity =
                Vec3::new(sideways.x * speed, rng.range(1.6, 3.0), sideways.y * speed);
            particle.gravity = 9.0;
            particle.lifetime = rng.range(0.35, 0.6);
            particle.start_size = 0.035;
            particle.end_size = 0.02;
        }
        ParticleKind::Steam | ParticleKind::Smoke => {
            let thick = kind == ParticleKind::Smoke;
            particle.velocity = Vec3::new(
                sideways.x * 0.15,
                rng.range(0.6, 1.1) * if thick { 1.4 } else { 1.0 },
                sideways.y * 0.15,
            );
            particle.gravity = -0.3;
            particle.drag = 0.5;
            particle.lifetime = rng.range(1.2, 2.0);
            particle.start_size = if thick { 0.18 } else { 0.1 };
            particle.end_size = if thick { 0.75 } else { 0.42 };
        }
        ParticleKind::Flame => {
            particle.velocity = Vec3::new(sideways.x * 0.3, rng.range(1.4, 2.4), sideways.y * 0.3);
            particle.gravity = -1.0;
            particle.drag = 1.0;
            particle.lifetime = rng.range(0.35, 0.6);
            particle.start_size = rng.range(0.16, 0.26);
            particle.end_size = 0.02;
        }
        ParticleKind::Ember => {
            particle.velocity = Vec3::new(sideways.x * 0.6, rng.range(2.0, 3.5), sideways.y * 0.6);
            particle.gravity = 1.5;
            particle.lifetime = rng.range(0.8, 1.4);
            particle.start_size = 0.035;
            particle.end_size = 0.01;
        }
        ParticleKind::Sparkle => {
            let speed = rng.range(1.0, 2.5);
            particle.velocity =
                Vec3::new(sideways.x * speed, rng.range(2.0, 4.0), sideways.y * speed);
            particle.gravity = 7.0;
            particle.lifetime = rng.range(0.5, 0.8);
            particle.start_size = 0.08;
            particle.end_size = 0.0;
        }
    }
    particle
}

fn update_particles(
    mut commands: Commands,
    time: Res<Time>,
    mut particles: Query<(Entity, &mut Particle, &mut Transform)>,
) {
    let dt = time.delta_secs();
    for (entity, mut particle, mut transform) in &mut particles {
        particle.age += dt;
        if particle.age >= particle.lifetime {
            commands.entity(entity).despawn();
            continue;
        }
        let gravity = particle.gravity;
        let drag = particle.drag;
        particle.velocity.y -= gravity * dt;
        particle.velocity *= 1.0 / (1.0 + drag * dt);
        transform.translation += particle.velocity * dt;

        let progress = particle.age / particle.lifetime;
        // Pop in quickly, then shrink away towards the end of the particle's life.
        let size = particle.start_size.lerp(particle.end_size, progress)
            * (progress * 8.0).min(1.0)
            * (1.0 - progress.powi(4));
        transform.scale = Vec3::splat(size.max(MIN_SIZE));
    }
}

fn shake_camera(
    time: Res<Time>,
    mut shake: ResMut<Shake>,
    camera: Single<(&mut Transform, &CameraRig)>,
) {
    let (mut transform, rig) = camera.into_inner();
    let t = time.elapsed_secs();
    let strength = shake.trauma * shake.trauma * 0.3;
    let offset = Vec3::new(
        (t * 37.0).sin(),
        (t * 41.0 + 1.3).sin(),
        (t * 29.0 + 2.1).sin(),
    ) * strength;
    transform.translation = rig.base.translation + offset;
    shake.trauma = (shake.trauma - time.delta_secs() * 1.5).max(0.0);
}
