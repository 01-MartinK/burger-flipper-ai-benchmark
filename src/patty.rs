//! Patties: dropping them on the grill, cooking, flipping, serving and binning.

use std::f32::consts::PI;

use bevy::color::Mix;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;

use crate::GameState;
use crate::audio::{PlaySfx, Sfx};
use crate::effects::{Burst, ParticleKind};
use crate::game::{Feedback, GameRng, Shift, running};
use crate::hud::{GREEN, ORANGE, Popup, RED, YELLOW};
use crate::kitchen::{
    BIN_MOUTH, GrillCursor, ORDER_PASS, RingMaterials, SLOTS, SlotRing, WARMER_SPOTS,
};
use crate::orders::{self, Claim, Order, Warmer};
use crate::rules::{self, Doneness, Quality, Verdict};

pub const PATTY_RADIUS: f32 = 0.85;
const THICKNESS: f32 = 0.34;
/// Height of a patty's centre when it lies on the grate.
const REST_HEIGHT: f32 = THICKNESS / 2.0 + 0.01;
const DROP_HEIGHT: f32 = 3.0;
const DROP_TIME: f32 = 0.3;
const FLIP_TIME: f32 = 0.5;
const FLIP_HEIGHT: f32 = 1.6;
const LEAVE_TIME: f32 = 0.55;
const SQUASH_TIME: f32 = 0.25;
/// How long a patty burns before it gets scraped into the bin automatically.
const FIRE_TIME: f32 = 2.0;
/// Horizontal offsets of the grill marks seared into each side.
const MARK_OFFSETS: [f32; 4] = [-0.48, -0.16, 0.16, 0.48];

pub fn plugin(app: &mut App) {
    app.init_resource::<Grill>()
        .add_systems(Startup, load_patty_assets)
        .add_systems(OnEnter(GameState::Playing), clear_grill)
        .add_systems(
            Update,
            (
                (handle_clicks, cook, highlight_slots)
                    .chain()
                    .run_if(in_state(GameState::Playing)),
                // Patties already in the air finish their moves behind the game-over screen.
                (animate, emit_particles, update_looks)
                    .chain()
                    .run_if(not(in_state(GameState::Title))),
            )
                .chain()
                .run_if(running),
        );
}

/// Which patty occupies each grill slot.
#[derive(Resource, Default)]
pub struct Grill {
    slots: [Option<Entity>; SLOTS.len()],
}

#[derive(Component)]
pub struct Patty {
    pub slot: usize,
    /// Cook level of both sides; side 0 starts face down.
    pub sides: [f32; 2],
    /// Index of the side touching the grill.
    pub down: usize,
    pub phase: Phase,
    /// Time left on the squash animation after landing.
    squash: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Phase {
    Dropping {
        t: f32,
    },
    Cooking,
    Flipping {
        t: f32,
    },
    OnFire {
        t: f32,
    },
    /// Flying off the grill towards an order, the warmer or the bin.
    Leaving {
        t: f32,
        from: Vec3,
        destination: Destination,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Destination {
    Order,
    /// A spot on the warmer tray.
    Warmer(usize),
    Bin,
}

impl Destination {
    fn target(self) -> Vec3 {
        match self {
            Destination::Order => ORDER_PASS,
            Destination::Warmer(spot) => WARMER_SPOTS[spot] + Vec3::Y * 0.1,
            Destination::Bin => BIN_MOUTH - Vec3::Y * 0.6,
        }
    }
}

impl Patty {
    pub fn down_level(&self) -> f32 {
        self.sides[self.down]
    }

    pub fn up_level(&self) -> f32 {
        self.sides[1 - self.down]
    }

    pub fn verdict(&self) -> Verdict {
        rules::judge(self.down_level(), self.up_level())
    }

    /// Whether the patty is lying on the grill, sizzling away.
    pub fn is_on_grill(&self) -> bool {
        matches!(self.phase, Phase::Cooking | Phase::OnFire { .. })
    }
}

/// Per-patty materials, recoloured as each side cooks.
#[derive(Component)]
struct PattyLook {
    meat: [Handle<StandardMaterial>; 2],
    marks: [Handle<StandardMaterial>; 2],
}

#[derive(Resource)]
struct PattyAssets {
    half: Handle<Mesh>,
    marks: Vec<(Handle<Mesh>, f32)>,
}

fn load_patty_assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    let half = meshes.add(
        Cylinder::new(PATTY_RADIUS, THICKNESS / 2.0)
            .mesh()
            .resolution(48),
    );
    let marks = MARK_OFFSETS
        .iter()
        .map(|&x| {
            let length = 2.0 * (PATTY_RADIUS * PATTY_RADIUS - x * x).sqrt() * 0.85;
            (meshes.add(Cuboid::new(0.09, 0.008, length)), x)
        })
        .collect();
    commands.insert_resource(PattyAssets { half, marks });
}

fn spawn_patty(
    commands: &mut Commands,
    assets: &PattyAssets,
    materials: &mut Assets<StandardMaterial>,
    slot: usize,
) -> Entity {
    let meat = [(); 2].map(|_| {
        materials.add(StandardMaterial {
            base_color: meat_color(0.0),
            perceptual_roughness: meat_roughness(0.0),
            ..default()
        })
    });
    let marks = [(); 2].map(|_| {
        materials.add(StandardMaterial {
            base_color: mark_color(0.0),
            perceptual_roughness: 0.9,
            ..default()
        })
    });
    let patty = commands
        .spawn((
            Patty {
                slot,
                sides: [0.0; 2],
                down: 0,
                phase: Phase::Dropping { t: 0.0 },
                squash: 0.0,
            },
            PattyLook {
                meat: meat.clone(),
                marks: marks.clone(),
            },
            Transform::from_translation(SLOTS[slot] + Vec3::Y * (REST_HEIGHT + DROP_HEIGHT)),
            Visibility::default(),
        ))
        .id();
    for side in 0..2 {
        // Side 0 is the lower half with its outer face pointing down, side 1 the upper half.
        let outward = if side == 0 { -1.0 } else { 1.0 };
        commands
            .spawn((
                Mesh3d(assets.half.clone()),
                MeshMaterial3d(meat[side].clone()),
                Transform::from_xyz(0.0, outward * THICKNESS / 4.0, 0.0),
                ChildOf(patty),
            ))
            .with_children(|half| {
                for (mesh, x) in &assets.marks {
                    half.spawn((
                        Mesh3d(mesh.clone()),
                        MeshMaterial3d(marks[side].clone()),
                        Transform::from_xyz(*x, outward * (THICKNESS / 4.0 + 0.004), 0.0),
                        NotShadowCaster,
                    ));
                }
            });
    }
    patty
}

/// Colour of a patty side at a cook level: raw pink, golden brown, then charcoal.
fn meat_color(level: f32) -> Color {
    const STOPS: [(f32, Color); 5] = [
        (0.0, Color::srgb(0.78, 0.32, 0.35)),
        (0.4, Color::srgb(0.62, 0.38, 0.32)),
        (0.85, Color::srgb(0.5, 0.29, 0.13)),
        (1.2, Color::srgb(0.26, 0.14, 0.07)),
        (1.6, Color::srgb(0.06, 0.045, 0.04)),
    ];
    STOPS
        .windows(2)
        .find(|pair| level <= pair[1].0)
        .map_or(STOPS[STOPS.len() - 1].1, |pair| {
            let (from_level, from) = pair[0];
            let (to_level, to) = pair[1];
            let t = ((level - from_level) / (to_level - from_level)).max(0.0);
            Oklaba::from(from).mix(&Oklaba::from(to), t).into()
        })
}

/// Grill marks only show up once a side has had some time on the grate.
fn mark_color(level: f32) -> Color {
    let sear = ((level - 0.3) / 0.6).clamp(0.0, 1.0);
    Oklaba::from(meat_color(level))
        .mix(&Oklaba::from(Color::srgb(0.05, 0.03, 0.02)), sear * 0.85)
        .into()
}

/// Raw meat is glossy, charcoal is dull.
fn meat_roughness(level: f32) -> f32 {
    0.35 + 0.55 * (level / 1.6).clamp(0.0, 1.0)
}

fn resting_rotation(down: usize) -> Quat {
    Quat::from_rotation_x(PI * down as f32)
}

fn smoothstep(x: f32) -> f32 {
    x * x * (3.0 - 2.0 * x)
}

fn send_off(patty: &mut Patty, grill: &mut Grill, from: Vec3, destination: Destination) {
    grill.slots[patty.slot] = None;
    patty.phase = Phase::Leaving {
        t: 0.0,
        from,
        destination,
    };
}

fn clear_grill(
    mut commands: Commands,
    patties: Query<Entity, With<Patty>>,
    mut grill: ResMut<Grill>,
) {
    for patty in &patties {
        commands.entity(patty).despawn();
    }
    *grill = Grill::default();
}

#[allow(clippy::too_many_arguments)]
fn handle_clicks(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    cursor: Res<GrillCursor>,
    assets: Res<PattyAssets>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut grill: ResMut<Grill>,
    mut patties: Query<(&mut Patty, &Transform)>,
    mut orders: Query<&mut Order>,
    mut warmer: ResMut<Warmer>,
    mut shift: ResMut<Shift>,
    mut feedback: Feedback,
) {
    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }
    let Some(slot) = cursor.slot else {
        return;
    };
    let Some(entity) = grill.slots[slot] else {
        grill.slots[slot] = Some(spawn_patty(&mut commands, &assets, &mut materials, slot));
        return;
    };
    let Ok((mut patty, transform)) = patties.get_mut(entity) else {
        return;
    };
    let position = transform.translation;
    match (patty.phase, patty.verdict()) {
        (Phase::Cooking, Verdict::Cooking | Verdict::Flip) => {
            patty.phase = Phase::Flipping { t: 0.0 };
            feedback.sfx.write(Sfx::Flip.into());
        }
        (Phase::Cooking, Verdict::Ready(quality)) => {
            let Some(claim) = orders::claim(&mut orders, &mut warmer) else {
                feedback.sfx.write(Sfx::Nope.into());
                feedback
                    .popups
                    .write(Popup::world("WARMER FULL!", position, RED));
                return;
            };
            let points = shift.serve(quality);
            let (label, color) = match quality {
                Quality::Perfect if shift.streak > 1 => {
                    (format!("PERFECT x{} +{points}", shift.streak), GREEN)
                }
                Quality::Perfect => (format!("PERFECT +{points}"), GREEN),
                Quality::Good => (format!("GOOD +{points}"), YELLOW),
                Quality::Dry => (format!("A BIT DRY +{points}"), ORANGE),
            };
            feedback.popups.write(Popup::world(label, position, color));
            feedback.sfx.write(PlaySfx::pitched(
                Sfx::Serve,
                1.0 + 0.06 * shift.streak.min(rules::MAX_STREAK) as f32,
            ));
            feedback
                .bursts
                .write(Burst::new(ParticleKind::Sparkle, position, 16).spread(0.5));
            let destination = match claim {
                Claim::Order => Destination::Order,
                Claim::Warmer(spot) => Destination::Warmer(spot),
            };
            send_off(&mut patty, &mut grill, position, destination);
        }
        (Phase::Cooking, Verdict::Ruined) => {
            shift.trash();
            feedback.popups.write(Popup::world(
                format!("BURNT -{}", rules::TRASH_PENALTY),
                position,
                RED,
            ));
            feedback.sfx.write(Sfx::Flip.into());
            send_off(&mut patty, &mut grill, position, Destination::Bin);
        }
        (Phase::OnFire { .. }, _) => {
            feedback.sfx.write(Sfx::Flip.into());
            send_off(&mut patty, &mut grill, position, Destination::Bin);
        }
        _ => {}
    }
}

fn cook(
    time: Res<Time>,
    mut patties: Query<(&mut Patty, &Transform)>,
    mut grill: ResMut<Grill>,
    mut shift: ResMut<Shift>,
    mut feedback: Feedback,
) {
    let dt = time.delta_secs();
    for (mut patty, transform) in &mut patties {
        let down = patty.down;
        match patty.phase {
            Phase::Cooking => {
                patty.sides[down] += rules::COOK_RATE * dt;
                if patty.sides[down] >= rules::IGNITES_AT {
                    patty.phase = Phase::OnFire { t: 0.0 };
                    shift.strike();
                    feedback.shake.add(0.6);
                    feedback.sfx.write(Sfx::Fire.into());
                    feedback.sfx.write(Sfx::Strike.into());
                    feedback
                        .popups
                        .write(Popup::world("FIRE! STRIKE!", transform.translation, RED).big());
                }
            }
            Phase::OnFire { t } => {
                patty.sides[down] += rules::COOK_RATE * dt;
                patty.phase = Phase::OnFire { t: t + dt };
                if t + dt >= FIRE_TIME {
                    send_off(
                        &mut patty,
                        &mut grill,
                        transform.translation,
                        Destination::Bin,
                    );
                }
            }
            _ => {}
        }
    }
}

fn animate(
    mut commands: Commands,
    time: Res<Time>,
    mut patties: Query<(Entity, &mut Patty, &mut Transform)>,
    mut feedback: Feedback,
) {
    let dt = time.delta_secs();
    for (entity, mut patty, mut transform) in &mut patties {
        let home = SLOTS[patty.slot] + Vec3::Y * REST_HEIGHT;
        match patty.phase {
            Phase::Dropping { t } => {
                let t = t + dt;
                let progress = (t / DROP_TIME).min(1.0);
                transform.translation = home + Vec3::Y * DROP_HEIGHT * (1.0 - progress * progress);
                if progress < 1.0 {
                    patty.phase = Phase::Dropping { t };
                } else {
                    patty.phase = Phase::Cooking;
                    land(&mut patty, home, &mut feedback, Sfx::Drop);
                }
            }
            Phase::Flipping { t } => {
                let t = t + dt;
                let progress = (t / FLIP_TIME).min(1.0);
                transform.scale = Vec3::ONE;
                let turn = Quat::from_rotation_x(PI * smoothstep(progress));
                transform.rotation = turn * resting_rotation(patty.down);
                transform.translation =
                    home + Vec3::Y * FLIP_HEIGHT * 4.0 * progress * (1.0 - progress);
                if progress < 1.0 {
                    patty.phase = Phase::Flipping { t };
                } else {
                    patty.down = 1 - patty.down;
                    transform.rotation = resting_rotation(patty.down);
                    patty.phase = Phase::Cooking;
                    land(&mut patty, home, &mut feedback, Sfx::Land);
                }
            }
            Phase::Cooking | Phase::OnFire { .. } => {
                patty.squash = (patty.squash - dt).max(0.0);
                let squash = (patty.squash / SQUASH_TIME).powi(2);
                transform.scale =
                    Vec3::new(1.0 + 0.12 * squash, 1.0 - 0.3 * squash, 1.0 + 0.12 * squash);
                transform.translation = home - Vec3::Y * THICKNESS * 0.15 * squash;
            }
            Phase::Leaving {
                t,
                from,
                destination,
            } => {
                let t = t + dt;
                let progress = (t / LEAVE_TIME).min(1.0);
                let (arc, end_scale, spin) = match destination {
                    Destination::Order => (1.0, 0.15, 0.0),
                    Destination::Warmer(_) => (1.5, 0.45, 0.0),
                    Destination::Bin => (2.0, 0.6, 9.0),
                };
                transform.translation = from.lerp(destination.target(), smoothstep(progress))
                    + Vec3::Y * arc * 4.0 * progress * (1.0 - progress);
                transform.scale = Vec3::splat(1.0_f32.lerp(end_scale, progress));
                transform.rotate_z(spin * dt);
                if progress < 1.0 {
                    patty.phase = Phase::Leaving {
                        t,
                        from,
                        destination,
                    };
                    continue;
                }
                commands.entity(entity).despawn();
                match destination {
                    Destination::Bin => {
                        feedback.sfx.write(Sfx::Trash.into());
                        feedback
                            .bursts
                            .write(Burst::new(ParticleKind::Steam, BIN_MOUTH, 5).spread(0.4));
                    }
                    Destination::Warmer(_) => {
                        feedback.sfx.write(Sfx::Warmer.into());
                    }
                    Destination::Order => {}
                }
            }
        }
    }
}

/// Squash, sizzle and spatter when a patty hits the grill.
fn land(patty: &mut Patty, home: Vec3, feedback: &mut Feedback, sound: Sfx) {
    patty.squash = SQUASH_TIME;
    feedback.sfx.write(sound.into());
    feedback
        .bursts
        .write(Burst::new(ParticleKind::Grease, home.with_y(0.1), 12).spread(PATTY_RADIUS + 0.1));
    feedback
        .bursts
        .write(Burst::new(ParticleKind::Steam, home + Vec3::Y * 0.2, 3).spread(0.5));
}

fn emit_particles(
    time: Res<Time>,
    patties: Query<(&Patty, &Transform)>,
    mut rng: ResMut<GameRng>,
    mut bursts: MessageWriter<Burst>,
) {
    let dt = time.delta_secs();
    let mut emit = |rng: &mut GameRng, kind, at: Vec3, rate: f32, spread: f32| {
        // Stochastic rounding keeps the average rate right at any frame rate.
        let count = (rate * dt + rng.f32()) as u32;
        if count > 0 {
            bursts.write(Burst::new(kind, at, count).spread(spread));
        }
    };
    for (patty, transform) in &patties {
        let top = transform.translation + Vec3::Y * THICKNESS / 2.0;
        match patty.phase {
            Phase::Cooking => {
                let rim = rng.direction() * PATTY_RADIUS;
                let rim = transform.translation.with_y(0.08) + Vec3::new(rim.x, 0.0, rim.y);
                emit(&mut rng, ParticleKind::Grease, rim, 3.0, 0.05);
                let hottest = patty.down_level().max(patty.up_level());
                match Doneness::of(hottest) {
                    Doneness::Burnt => emit(&mut rng, ParticleKind::Smoke, top, 10.0, 0.6),
                    Doneness::Overdone => emit(&mut rng, ParticleKind::Steam, top, 6.0, 0.6),
                    _ => emit(&mut rng, ParticleKind::Steam, top, 1.5, 0.6),
                }
            }
            Phase::OnFire { .. } => {
                emit(&mut rng, ParticleKind::Flame, top, 50.0, 0.7);
                emit(&mut rng, ParticleKind::Ember, top, 8.0, 0.6);
                emit(
                    &mut rng,
                    ParticleKind::Smoke,
                    top + Vec3::Y * 0.4,
                    10.0,
                    0.5,
                );
            }
            _ => {}
        }
    }
}

fn update_looks(
    patties: Query<(&Patty, &PattyLook), Changed<Patty>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (patty, look) in &patties {
        for (side, &level) in patty.sides.iter().enumerate() {
            if let Some(mut meat) = materials.get_mut(&look.meat[side]) {
                meat.base_color = meat_color(level);
                meat.perceptual_roughness = meat_roughness(level);
            }
            if let Some(mut marks) = materials.get_mut(&look.marks[side]) {
                marks.base_color = mark_color(level);
            }
        }
    }
}

/// Outlines the hovered slot in the colour of what a click would do there.
fn highlight_slots(
    cursor: Res<GrillCursor>,
    grill: Res<Grill>,
    patties: Query<&Patty>,
    rings: Res<RingMaterials>,
    mut outlines: Query<(
        &SlotRing,
        &mut MeshMaterial3d<StandardMaterial>,
        &mut Visibility,
    )>,
) {
    for (ring, mut material, mut visibility) in &mut outlines {
        let hovered = cursor.slot == Some(ring.0);
        let patty = grill.slots[ring.0].and_then(|entity| patties.get(entity).ok());
        let style = match patty {
            None if hovered => Some(&rings.place),
            None => Some(&rings.idle),
            Some(patty) if hovered => match (patty.phase, patty.verdict()) {
                (Phase::OnFire { .. }, _) | (Phase::Cooking, Verdict::Ruined) => Some(&rings.trash),
                (Phase::Cooking, Verdict::Ready(_)) => Some(&rings.serve),
                (Phase::Cooking, _) => Some(&rings.flip),
                _ => None,
            },
            Some(_) => None,
        };
        let Some(handle) = style else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        if material.0 != *handle {
            material.0 = handle.clone();
        }
        visibility.set_if_neq(Visibility::Inherited);
    }
}
