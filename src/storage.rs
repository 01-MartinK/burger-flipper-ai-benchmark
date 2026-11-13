//! Storage: shelves of crates, one per ingredient. Clicking a crate loads a
//! pack of it onto the trays on the cart below.

use bevy::prelude::*;

use crate::GameState;
use crate::audio::Sfx;
use crate::game::{Feedback, running};
use crate::hud::{GREEN, Popup, RED, world_label};
use crate::ingredients::{
    self, Flight, IngredientAssets, Pantry, Piece, Pile, Shelf, StockLabel, spawn_piece, spawn_tray,
};
use crate::kitchen::surface;
use crate::rules::Ingredient;
use crate::stations::{Alerts, Cursor, Hoverable, Station, View, ray_hits_box};

const ORIGIN: Vec3 = View::Storage.origin();
const CRATE_SIZE: Vec3 = Vec3::new(2.8, 1.3, 1.8);
/// Height of the two shelf boards the crates stand on.
const SHELVES: [f32; 2] = [3.4, 1.2];
/// Each crate and the bottom centre of where it stands.
const CRATES: [(Ingredient, Vec3); 7] = [
    (Ingredient::Bun, Vec3::new(-5.1, SHELVES[0], 0.0)),
    (Ingredient::Cheese, Vec3::new(-1.7, SHELVES[0], 0.0)),
    (Ingredient::Lettuce, Vec3::new(1.7, SHELVES[0], 0.0)),
    (Ingredient::Tomato, Vec3::new(5.1, SHELVES[0], 0.0)),
    (Ingredient::Onions, Vec3::new(-3.4, SHELVES[1], 0.0)),
    (Ingredient::Patty, Vec3::new(0.0, SHELVES[1], 0.0)),
    (Ingredient::Bacon, Vec3::new(3.4, SHELVES[1], 0.0)),
];
const TRAY_SIZE: Vec2 = Vec2::new(1.55, 1.7);
const TRAY_Z: f32 = 3.2;

pub fn plugin(app: &mut App) {
    app.add_systems(Startup, spawn_storage).add_systems(
        Update,
        load_trays.run_if(in_state(GameState::Playing).and_then(running)),
    );
}

#[derive(Component)]
struct Crate(Ingredient);

/// Where the tray of an ingredient stands on the cart.
fn tray_spot(ingredient: Ingredient) -> Vec3 {
    Vec3::new(-5.4 + 1.8 * ingredient.index() as f32, 0.06, TRAY_Z)
}

fn spawn_storage(
    mut commands: Commands,
    assets: Res<IngredientAssets>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut paint = |r: f32, g: f32, b: f32, metallic: f32, roughness: f32| {
        materials.add(surface(Color::srgb(r, g, b), metallic, roughness))
    };
    let wall = paint(0.5, 0.58, 0.62, 0.0, 0.9);
    let stripe = paint(0.36, 0.44, 0.5, 0.0, 0.9);
    let board = paint(0.5, 0.33, 0.18, 0.0, 0.7);
    let post = paint(0.3, 0.31, 0.34, 0.6, 0.5);
    let cart = paint(0.42, 0.44, 0.47, 0.7, 0.45);
    let crate_wood = paint(0.72, 0.52, 0.3, 0.0, 0.8);
    let crate_dark = paint(0.55, 0.38, 0.2, 0.0, 0.8);
    let tags: Vec<_> = Ingredient::ALL
        .iter()
        .map(|&ingredient| {
            let color = ingredients::color(ingredient).to_srgba();
            paint(color.red, color.green, color.blue, 0.0, 0.6)
        })
        .collect();

    let cube = assets.cube.clone();
    let block =
        |commands: &mut Commands, size: Vec3, at: Vec3, material: &Handle<StandardMaterial>| {
            commands.spawn((
                Mesh3d(cube.clone()),
                MeshMaterial3d(material.clone()),
                Transform::from_translation(ORIGIN + at).with_scale(size),
            ));
        };

    // Walk-in pantry wall, with a darker band along the bottom.
    block(
        &mut commands,
        Vec3::new(40.0, 20.0, 0.4),
        Vec3::new(0.0, 4.0, -1.4),
        &wall,
    );
    block(
        &mut commands,
        Vec3::new(40.0, 4.0, 0.1),
        Vec3::new(0.0, -1.0, -1.18),
        &stripe,
    );
    // Shelving unit.
    for height in SHELVES {
        block(
            &mut commands,
            Vec3::new(14.4, 0.2, 2.2),
            Vec3::new(0.0, height - 0.1, 0.0),
            &board,
        );
    }
    block(
        &mut commands,
        Vec3::new(14.4, 0.2, 2.2),
        Vec3::new(0.0, 5.5, 0.0),
        &board,
    );
    for x in [-7.1, 7.1] {
        for z in [-0.9, 0.9] {
            block(
                &mut commands,
                Vec3::new(0.22, 10.0, 0.22),
                Vec3::new(x, 1.0, z),
                &post,
            );
        }
    }
    // The cart the trays ride on.
    block(
        &mut commands,
        Vec3::new(13.6, 3.0, 2.6),
        Vec3::new(0.0, -1.5, TRAY_Z),
        &cart,
    );
    block(
        &mut commands,
        Vec3::new(13.9, 0.12, 2.9),
        Vec3::new(0.0, 0.0, TRAY_Z),
        &assets.steel,
    );

    for (ingredient, at) in CRATES {
        spawn_crate(
            &mut commands,
            &assets,
            (ingredient, at),
            [&crate_wood, &crate_dark, &tags[ingredient.index()]],
        );
        commands.spawn(world_label(
            ingredient.name(),
            ORIGIN + at + Vec3::new(0.0, -0.02, CRATE_SIZE.z / 2.0 + 0.2),
        ));
    }

    for ingredient in Ingredient::ALL {
        let at = ORIGIN + tray_spot(ingredient);
        let tray = spawn_tray(&mut commands, &assets, at, TRAY_SIZE);
        commands.entity(tray).with_child((
            Transform::default(),
            Visibility::default(),
            Pile::new(ingredient, Shelf::Stock, 0.6),
        ));
        commands.spawn((
            world_label("", at + Vec3::new(0.0, -0.1, TRAY_SIZE.y / 2.0 + 0.2)),
            StockLabel {
                ingredient,
                shelf: Shelf::Stock,
            },
        ));
    }

    commands.spawn((
        PointLight {
            color: Color::srgb(0.95, 0.97, 1.0),
            intensity: 2_500_000.0,
            range: 40.0,
            ..default()
        },
        Transform::from_translation(ORIGIN + Vec3::new(0.0, 6.5, 8.0)),
    ));
}

/// An open crate with a low front, so the food piled inside shows.
fn spawn_crate(
    commands: &mut Commands,
    assets: &IngredientAssets,
    (ingredient, at): (Ingredient, Vec3),
    [wood, dark, tag]: [&Handle<StandardMaterial>; 3],
) {
    let size = CRATE_SIZE;
    let plank = 0.1;
    let root = commands
        .spawn((
            Transform::from_translation(ORIGIN + at),
            Visibility::default(),
            Hoverable::default(),
            Crate(ingredient),
        ))
        .id();
    let parts = [
        // Floor, back, sides, low front and the coloured tag on it.
        (
            Vec3::new(size.x, plank, size.z),
            Vec3::Y * plank / 2.0,
            dark,
        ),
        (
            Vec3::new(size.x, size.y, plank),
            Vec3::new(0.0, size.y / 2.0, -size.z / 2.0),
            dark,
        ),
        (
            Vec3::new(plank, size.y, size.z),
            Vec3::new(-size.x / 2.0, size.y / 2.0, 0.0),
            wood,
        ),
        (
            Vec3::new(plank, size.y, size.z),
            Vec3::new(size.x / 2.0, size.y / 2.0, 0.0),
            wood,
        ),
        (
            Vec3::new(size.x, size.y * 0.42, plank),
            Vec3::new(0.0, size.y * 0.21, size.z / 2.0),
            wood,
        ),
        (
            Vec3::new(size.x * 0.5, size.y * 0.26, 0.04),
            Vec3::new(0.0, size.y * 0.21, size.z / 2.0 + plank / 2.0),
            tag,
        ),
    ];
    for (scale, offset, material) in parts {
        commands.spawn((
            Mesh3d(assets.cube.clone()),
            MeshMaterial3d(material.clone()),
            Transform::from_translation(offset).with_scale(scale),
            ChildOf(root),
        ));
    }
    // Two piles of the ingredient inside. Flat ones sit on packing so they show
    // over the front of the crate.
    let piece = Piece::in_stock(ingredient);
    let (base, spacing) = match piece {
        Piece::BunTop => (plank, 0.3 * 0.62),
        other if other.height() > 0.2 => (plank, other.height() * 0.62),
        other => (size.y * 0.4, other.height().max(0.12) * 0.62),
    };
    for (pile, x) in [-0.66, 0.66].into_iter().enumerate() {
        for layer in 0..4 {
            let transform = Transform::from_xyz(x, base + layer as f32 * spacing, 0.1)
                .with_scale(Vec3::splat(0.62))
                .with_rotation(Quat::from_rotation_y((pile * 3 + layer) as f32 * 0.7));
            let model = spawn_piece(commands, assets, piece, transform);
            commands.entity(model).insert(ChildOf(root));
        }
    }
}

/// Highlights the crate under the cursor; a click loads a pack of it onto its tray.
#[allow(clippy::too_many_arguments)]
fn load_trays(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    station: Res<Station>,
    cursor: Res<Cursor>,
    assets: Res<IngredientAssets>,
    mut pantry: ResMut<Pantry>,
    mut crates: Query<(Entity, &Crate, &mut Hoverable, &Transform)>,
    mut alerts: ResMut<Alerts>,
    mut feedback: Feedback,
) {
    let running_low = Ingredient::ALL
        .iter()
        .any(|&ingredient| pantry.count(ingredient, Shelf::Stock) == 0);
    alerts.set(View::Storage, running_low);

    let ray = cursor.ray().filter(|_| station.is_at(View::Storage));
    let pointed_at = ray.and_then(|ray| {
        crates
            .iter()
            .filter_map(|(entity, _, _, transform)| {
                let centre = transform.translation + Vec3::Y * CRATE_SIZE.y / 2.0;
                ray_hits_box(ray, centre, CRATE_SIZE / 2.0).map(|distance| (entity, distance))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(entity, _)| entity)
    });
    for (entity, crate_, mut hoverable, transform) in &mut crates {
        let hot = pointed_at == Some(entity);
        if hoverable.hot != hot {
            hoverable.hot = hot;
        }
        if !hot || !mouse.just_pressed(MouseButton::Left) {
            continue;
        }
        let ingredient = crate_.0;
        let above = transform.translation + Vec3::Y * (CRATE_SIZE.y + 0.3);
        let added = pantry.add_pack(ingredient);
        if added == 0 {
            feedback.sfx.write(Sfx::Nope.into());
            feedback
                .popups
                .write(Popup::world("TRAY FULL!", above, RED));
            continue;
        }
        feedback.sfx.write(Sfx::Flip.into());
        feedback.popups.write(Popup::world(
            format!("+{added} {}", ingredient.name()),
            above,
            GREEN,
        ));
        // The portions hop down onto the tray one after the other.
        for index in 0..added {
            let from = transform.translation + Vec3::Y * CRATE_SIZE.y * 0.6;
            let model = spawn_piece(
                &mut commands,
                &assets,
                Piece::in_stock(ingredient),
                Transform::from_translation(from).with_scale(Vec3::splat(0.6)),
            );
            commands.entity(model).insert((
                Visibility::Hidden,
                Flight {
                    from,
                    to: ORIGIN + tray_spot(ingredient) + Vec3::Y * 0.2,
                    arc: 1.4,
                    duration: 0.4,
                    age: -0.07 * index as f32,
                    start_scale: 0.6,
                    end_scale: 0.6,
                    sound: (index == 0).then_some(Sfx::Land),
                },
            ));
        }
    }
}
