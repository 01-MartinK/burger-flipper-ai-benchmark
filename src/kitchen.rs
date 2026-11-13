//! The grill station: camera, lights, grill, bin, raw trays and the pass to the
//! prep table, plus picking which grill slot the mouse points at.

use std::f32::consts::FRAC_PI_2;

use bevy::input::InputSystems;
use bevy::light::{CascadeShadowConfigBuilder, NotShadowCaster};
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;

use crate::effects::CameraRig;
use crate::game::GameRng;
use crate::hud::world_label;
use crate::ingredients::{IngredientAssets, Pile, Shelf, StockLabel, spawn_tray};
use crate::rules::Protein;
use crate::stations::{self, Cursor, Station, View};

/// Centres of the six cooking spots, back row first.
pub const SLOTS: [Vec3; 6] = [
    Vec3::new(-2.4, 0.0, -1.15),
    Vec3::new(0.0, 0.0, -1.15),
    Vec3::new(2.4, 0.0, -1.15),
    Vec3::new(-2.4, 0.0, 1.15),
    Vec3::new(0.0, 0.0, 1.15),
    Vec3::new(2.4, 0.0, 1.15),
];
/// How close to a slot centre the cursor has to be to target it.
const SLOT_PICK_RADIUS: f32 = 1.1;
/// The cursor is projected onto a plane at about the height of a patty's top.
const PICK_HEIGHT: f32 = 0.3;

/// Centre of the trash bin's opening.
pub const BIN_MOUTH: Vec3 = Vec3::new(-5.45, 0.95, 1.2);
/// The tray cooked food is sent to; it ends up on the prep table.
pub const PASS_TRAY: Vec3 = Vec3::new(5.65, -0.5, 0.9);
/// Trays of raw patties and bacon next to the grill.
pub const RAW_TRAYS: [(Protein, Vec3); 2] = [
    (Protein::Patty, Vec3::new(-5.7, -0.5, -3.3)),
    (Protein::Bacon, Vec3::new(-5.7, -0.5, -1.6)),
];
pub const RAW_TRAY_SIZE: Vec2 = Vec2::new(1.7, 1.5);

const GRILL_SIZE: Vec2 = Vec2::new(8.2, 5.0);
const COAL_GLOW: [LinearRgba; 3] = [
    LinearRgba::rgb(0.35, 0.05, 0.008),
    LinearRgba::rgb(1.6, 0.35, 0.05),
    LinearRgba::rgb(3.5, 1.0, 0.15),
];

pub fn plugin(app: &mut App) {
    app.init_resource::<GrillCursor>()
        .insert_resource(ClearColor(Color::srgb(0.09, 0.07, 0.06)))
        .insert_resource(GlobalAmbientLight {
            color: Color::srgb(1.0, 0.9, 0.8),
            brightness: 300.0,
            ..default()
        })
        .add_systems(
            Startup,
            (
                spawn_camera_and_lights,
                spawn_counter,
                spawn_grill,
                spawn_bin,
                spawn_trays,
            ),
        )
        .add_systems(
            PreUpdate,
            track_cursor
                .after(InputSystems)
                .after(stations::track_cursor),
        )
        .add_systems(Update, flicker_coals);
}

#[derive(Component)]
pub struct MainCamera;

/// The grill slot under the mouse cursor, if any.
#[derive(Resource, Default)]
pub struct GrillCursor {
    pub slot: Option<usize>,
}

/// Outline around a grill slot, used to show what a click would do.
#[derive(Component)]
pub struct SlotRing(pub usize);

#[derive(Resource)]
pub struct RingMaterials {
    pub idle: Handle<StandardMaterial>,
    pub place: Handle<StandardMaterial>,
    pub flip: Handle<StandardMaterial>,
    pub serve: Handle<StandardMaterial>,
    pub trash: Handle<StandardMaterial>,
}

#[derive(Resource)]
struct CoalMaterials([Handle<StandardMaterial>; 3]);

/// Glowing frame under the raw tray the grill is being loaded from.
#[derive(Component)]
pub struct LoadMarker(pub Protein);

pub fn surface(color: Color, metallic: f32, roughness: f32) -> StandardMaterial {
    StandardMaterial {
        base_color: color,
        metallic,
        perceptual_roughness: roughness,
        ..default()
    }
}

fn spawn_camera_and_lights(mut commands: Commands) {
    let view = View::Grill.camera();
    commands.spawn((
        Camera3d::default(),
        view,
        CameraRig { base: view },
        Bloom::NATURAL,
        MainCamera,
    ));
    commands.spawn((
        DirectionalLight {
            color: Color::srgb(1.0, 0.95, 0.88),
            illuminance: 7_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(-4.0, 10.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
        CascadeShadowConfigBuilder {
            maximum_distance: 30.0,
            first_cascade_far_bound: 14.0,
            ..default()
        }
        .build(),
    ));
    // Warm light from the coals.
    commands.spawn((
        PointLight {
            color: Color::srgb(1.0, 0.5, 0.2),
            intensity: 120_000.0,
            range: 8.0,
            ..default()
        },
        Transform::from_xyz(0.0, 0.8, 0.0),
    ));
}

fn spawn_counter(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    spawn_backdrop(
        &mut commands,
        &mut meshes,
        &mut materials,
        Vec3::ZERO,
        surface(Color::srgb(0.24, 0.25, 0.27), 0.3, 0.45),
        Color::srgb(0.12, 0.3, 0.29),
    );
}

/// A worktop with a diner-style tiled wall behind it: coloured tiles with light
/// grout, every row offset by half a tile. The worktop's surface is 0.55 below `origin`.
pub fn spawn_backdrop(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    origin: Vec3,
    worktop: StandardMaterial,
    tiles: Color,
) {
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(30.0, 1.0, 16.0))),
        MeshMaterial3d(materials.add(worktop)),
        Transform::from_translation(origin + Vec3::new(0.0, -1.05, 0.0)),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(30.0, 8.0, 0.3))),
        MeshMaterial3d(materials.add(surface(tiles, 0.0, 0.25))),
        Transform::from_translation(origin + Vec3::new(0.0, 3.4, -6.0)),
    ));
    let grout = materials.add(surface(Color::srgb(0.75, 0.73, 0.68), 0.0, 0.9));
    let row_line = meshes.add(Cuboid::new(30.0, 0.04, 0.04));
    let joint = meshes.add(Cuboid::new(0.04, 0.5, 0.04));
    for row in 0..10 {
        let y = -0.55 + row as f32 * 0.5;
        commands.spawn((
            Mesh3d(row_line.clone()),
            MeshMaterial3d(grout.clone()),
            Transform::from_translation(origin + Vec3::new(0.0, y, -5.84)),
        ));
        let offset = if row % 2 == 0 { 0.0 } else { 0.5 };
        for column in -12..12 {
            commands.spawn((
                Mesh3d(joint.clone()),
                MeshMaterial3d(grout.clone()),
                Transform::from_translation(
                    origin + Vec3::new(column as f32 + offset, y + 0.25, -5.84),
                ),
            ));
        }
    }
}

fn spawn_grill(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut rng: ResMut<GameRng>,
) {
    let body = materials.add(surface(Color::srgb(0.12, 0.12, 0.13), 0.7, 0.55));
    let steel = materials.add(surface(Color::srgb(0.7, 0.72, 0.75), 0.9, 0.3));
    let iron = materials.add(surface(Color::srgb(0.16, 0.13, 0.12), 0.8, 0.45));

    // Firebox floor; the coals sit on it, below the grate.
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(GRILL_SIZE.x + 0.4, 0.25, GRILL_SIZE.y + 0.4))),
        MeshMaterial3d(body),
        Transform::from_xyz(0.0, -0.43, 0.0),
    ));

    // Polished walls around the coals, rising just above the grate.
    let half = GRILL_SIZE / 2.0 + 0.1;
    let long = meshes.add(Cuboid::new(GRILL_SIZE.x + 0.6, 0.5, 0.2));
    let short = meshes.add(Cuboid::new(0.2, 0.5, GRILL_SIZE.y + 0.2));
    for z in [-half.y, half.y] {
        commands.spawn((
            Mesh3d(long.clone()),
            MeshMaterial3d(steel.clone()),
            Transform::from_xyz(0.0, -0.14, z),
        ));
    }
    for x in [-half.x, half.x] {
        commands.spawn((
            Mesh3d(short.clone()),
            MeshMaterial3d(steel.clone()),
            Transform::from_xyz(x, -0.14, 0.0),
        ));
    }

    // Glowing coal bed under the grate.
    let coal_materials = COAL_GLOW.map(|glow| {
        materials.add(StandardMaterial {
            base_color: Color::srgb(0.1, 0.03, 0.02),
            emissive: glow,
            perceptual_roughness: 1.0,
            ..default()
        })
    });
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(GRILL_SIZE.x, 0.04, GRILL_SIZE.y))),
        MeshMaterial3d(coal_materials[0].clone()),
        Transform::from_xyz(0.0, -0.29, 0.0),
    ));
    let coal = meshes.add(Sphere::new(1.0).mesh().ico(1).unwrap());
    for _ in 0..140 {
        let x = rng.range(-GRILL_SIZE.x / 2.0 + 0.2, GRILL_SIZE.x / 2.0 - 0.2);
        let z = rng.range(-GRILL_SIZE.y / 2.0 + 0.2, GRILL_SIZE.y / 2.0 - 0.2);
        let size = rng.range(0.12, 0.24);
        commands.spawn((
            Mesh3d(coal.clone()),
            MeshMaterial3d(rng.pick(&coal_materials).clone()),
            Transform::from_xyz(x, -0.27, z).with_scale(Vec3::new(
                size,
                size * 0.6,
                size * rng.range(0.8, 1.2),
            )),
            NotShadowCaster,
        ));
    }
    commands.insert_resource(CoalMaterials(coal_materials));

    // Grate bars running front to back.
    let bar = meshes.add(Cuboid::new(0.1, 0.07, GRILL_SIZE.y));
    let bars = 25;
    for i in 0..bars {
        let x = (i as f32 / (bars - 1) as f32 - 0.5) * (GRILL_SIZE.x - 0.3);
        commands.spawn((
            Mesh3d(bar.clone()),
            MeshMaterial3d(iron.clone()),
            Transform::from_xyz(x, -0.035, 0.0),
        ));
    }

    // Control knobs on the front of the firebox.
    let knob = meshes.add(Cylinder::new(0.17, 0.14));
    let knob_material = materials.add(surface(Color::srgb(0.08, 0.08, 0.08), 0.2, 0.5));
    let notch = meshes.add(Cuboid::new(0.04, 0.12, 0.02));
    let notch_material = materials.add(surface(Color::WHITE, 0.0, 0.5));
    for x in [-2.4, 0.0, 2.4] {
        commands
            .spawn((
                Mesh3d(knob.clone()),
                MeshMaterial3d(knob_material.clone()),
                Transform::from_xyz(x, -0.3, GRILL_SIZE.y / 2.0 + 0.27)
                    .with_rotation(Quat::from_rotation_x(FRAC_PI_2)),
            ))
            .with_child((
                Mesh3d(notch.clone()),
                MeshMaterial3d(notch_material.clone()),
                Transform::from_xyz(0.0, 0.07, -0.06),
            ));
    }

    // Slot outlines.
    let outline = |color: Color| StandardMaterial {
        base_color: color,
        alpha_mode: if color.alpha() < 1.0 {
            AlphaMode::Blend
        } else {
            AlphaMode::Opaque
        },
        unlit: true,
        ..default()
    };
    let rings = RingMaterials {
        idle: materials.add(outline(Color::srgba(1.0, 0.75, 0.5, 0.25))),
        place: materials.add(outline(Color::WHITE)),
        flip: materials.add(outline(Color::srgb(1.0, 0.85, 0.2))),
        serve: materials.add(outline(Color::srgb(0.3, 1.0, 0.4))),
        trash: materials.add(outline(Color::srgb(1.0, 0.25, 0.2))),
    };
    let ring = meshes.add(Torus {
        minor_radius: 0.035,
        major_radius: 1.0,
    });
    for (index, slot) in SLOTS.iter().enumerate() {
        commands.spawn((
            Mesh3d(ring.clone()),
            MeshMaterial3d(rings.idle.clone()),
            Transform::from_translation(*slot + Vec3::Y * 0.02),
            NotShadowCaster,
            SlotRing(index),
        ));
    }
    commands.insert_resource(rings);
}

fn spawn_bin(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let height = BIN_MOUTH.y + 0.55;
    commands.spawn((
        Mesh3d(meshes.add(Cylinder::new(0.75, height))),
        MeshMaterial3d(materials.add(surface(Color::srgb(0.2, 0.3, 0.25), 0.1, 0.6))),
        Transform::from_xyz(BIN_MOUTH.x, BIN_MOUTH.y - height / 2.0, BIN_MOUTH.z),
    ));
    // Dark opening on top, and a rim around it.
    commands.spawn((
        Mesh3d(meshes.add(Cylinder::new(0.66, 0.02))),
        MeshMaterial3d(materials.add(surface(Color::srgb(0.02, 0.02, 0.02), 0.0, 1.0))),
        Transform::from_translation(BIN_MOUTH + Vec3::Y * 0.005),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Torus {
            minor_radius: 0.06,
            major_radius: 0.71,
        })),
        MeshMaterial3d(materials.add(surface(Color::srgb(0.25, 0.36, 0.3), 0.1, 0.5))),
        Transform::from_translation(BIN_MOUTH),
    ));
    commands.spawn(world_label(
        "TRASH",
        BIN_MOUTH.with_y(-0.55) + Vec3::Z * 1.0,
    ));
}

fn spawn_trays(
    mut commands: Commands,
    assets: Res<IngredientAssets>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Raw patties and bacon, stocked from storage.
    let marker = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.8, 0.2),
        emissive: LinearRgba::rgb(2.5, 1.6, 0.2),
        ..default()
    });
    for (protein, at) in RAW_TRAYS {
        let tray = spawn_tray(&mut commands, &assets, at, RAW_TRAY_SIZE);
        commands.entity(tray).with_child((
            Transform::from_xyz(0.0, 0.0, 0.0),
            Visibility::default(),
            Pile::new(protein.ingredient(), Shelf::Stock, 0.62),
        ));
        commands.spawn((
            Mesh3d(assets.cube.clone()),
            MeshMaterial3d(marker.clone()),
            Transform::from_translation(at - Vec3::Y * 0.04).with_scale(Vec3::new(
                RAW_TRAY_SIZE.x + 0.22,
                0.02,
                RAW_TRAY_SIZE.y + 0.22,
            )),
            NotShadowCaster,
            Visibility::Hidden,
            LoadMarker(protein),
        ));
        commands.spawn((
            world_label("", at + Vec3::new(0.0, 0.0, RAW_TRAY_SIZE.y / 2.0)),
            StockLabel {
                ingredient: protein.ingredient(),
                shelf: Shelf::Stock,
            },
        ));
    }

    // The pass: cooked food lands here on its way to the prep table.
    let size = Vec2::new(1.9, 1.9);
    let pass = spawn_tray(&mut commands, &assets, PASS_TRAY, size);
    commands.entity(pass).with_child((
        Mesh3d(assets.cube.clone()),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.25, 0.08, 0.04),
            emissive: LinearRgba::rgb(0.35, 0.06, 0.01),
            perceptual_roughness: 0.6,
            ..default()
        })),
        Transform::from_xyz(0.0, 0.01, 0.0).with_scale(Vec3::new(size.x - 0.2, 0.02, size.y - 0.2)),
    ));
    for (protein, x) in Protein::ALL.into_iter().zip([-0.45, 0.45]) {
        commands.entity(pass).with_child((
            Transform::from_xyz(x, 0.02, 0.0),
            Visibility::default(),
            Pile::new(protein.ingredient(), Shelf::Ready, 0.42),
        ));
    }
    commands.spawn(world_label(
        "TO PREP TABLE",
        PASS_TRAY + Vec3::Z * (size.y / 2.0 + 0.1),
    ));
}

fn track_cursor(pointer: Res<Cursor>, station: Res<Station>, mut cursor: ResMut<GrillCursor>) {
    let slot = pointer
        .on_plane(PICK_HEIGHT)
        .filter(|_| station.is_at(View::Grill))
        .and_then(|point| {
            SLOTS
                .iter()
                .enumerate()
                .map(|(index, slot)| (index, slot.xz().distance(point.xz())))
                .filter(|(_, distance)| *distance < SLOT_PICK_RADIUS)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(index, _)| index)
        });
    if cursor.slot != slot {
        cursor.slot = slot;
    }
}

fn flicker_coals(
    time: Res<Time>,
    coals: Res<CoalMaterials>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let t = time.elapsed_secs();
    for (index, handle) in coals.0.iter().enumerate() {
        let phase = index as f32 * 2.1;
        let glow = 1.0 + 0.2 * (t * 1.7 + phase).sin() + 0.1 * (t * 4.3 + phase * 1.3).sin();
        if let Some(mut material) = materials.get_mut(handle) {
            material.emissive = COAL_GLOW[index] * glow;
        }
    }
}
