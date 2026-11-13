//! The counter: customers walk in, line up, and wait for someone to take their
//! order. Afterwards they hang about at the back until their burger is served.

use std::f32::consts::PI;

use bevy::light::NotShadowCaster;
use bevy::prelude::*;

use crate::GameState;
use crate::audio::{PlaySfx, Sfx};
use crate::effects::{Burst, ParticleKind};
use crate::game::{Feedback, GameRng, Shift, running};
use crate::hud::{GREEN, Popup, RED, YELLOW, blend};
use crate::ingredients::{Cook, IngredientAssets, Piece, spawn_piece};
use crate::kitchen::surface;
use crate::orders::{OrderClosed, Outcome, TakeOrder};
use crate::rules::{self, Ingredient, Quality};
use crate::stations::{Alerts, Cursor, Hoverable, Station, View, ray_distance};

const ORIGIN: Vec3 = View::Counter.origin();
/// Height of the dining room floor; the counter top is at zero.
const FLOOR: f32 = -3.3;
/// Where customers stand in line, and where they wait for their food.
const QUEUE_Z: f32 = 0.4;
const QUEUE_X: [f32; rules::QUEUE_SPOTS] = [-4.8, -1.6, 1.6, 4.8];
const WAIT_Z: f32 = -4.6;
const WAIT_X: [f32; rules::MAX_OPEN_ORDERS] = [-6.4, -3.2, 0.0, 3.2, 6.4];
const DOOR: Vec3 = Vec3::new(13.5, FLOOR, -6.5);
const EXIT: Vec3 = Vec3::new(-14.5, FLOOR, -6.5);
const WALK_SPEED: f32 = 7.0;
/// The order spots fill up in: from the middle outwards.
const QUEUE_ORDER: [usize; rules::QUEUE_SPOTS] = [1, 2, 0, 3];
const WAIT_ORDER: [usize; rules::MAX_OPEN_ORDERS] = [2, 1, 3, 0, 4];
/// Heights on a customer, measured from their feet.
const CHEST: f32 = 2.8;
const HEAD: f32 = 4.4;
const BUBBLE: f32 = 5.9;
/// Where order messages pop up.
const POPUP: Vec2 = Vec2::new(640.0, 250.0);

const NAMES: [&str; 24] = [
    "ANN", "BOB", "CHEN", "DANA", "ELI", "FAY", "GUS", "HAL", "IVY", "JO", "KAI", "LEO", "MAX",
    "NIA", "OTTO", "PIA", "RAY", "SUE", "TOM", "UMA", "VIC", "WES", "YUKI", "ZOE",
];

pub fn plugin(app: &mut App) {
    app.init_resource::<Lobby>()
        .add_systems(Startup, (spawn_dining_room, load_customer_assets))
        .add_systems(OnEnter(GameState::Playing), reset_lobby)
        .add_systems(
            Update,
            (
                let_customers_in,
                take_orders,
                see_customers_off,
                move_customers,
                style_bubbles,
            )
                .chain()
                .run_if(in_state(GameState::Playing).and_then(running)),
        );
}

/// Who stands where, and when the next customer shows up.
#[derive(Resource, Default)]
struct Lobby {
    queue: [Option<Entity>; rules::QUEUE_SPOTS],
    waiting: [Option<Entity>; rules::MAX_OPEN_ORDERS],
    until_next: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Stage {
    /// Walking to a spot in the line.
    Arriving(usize),
    /// In line, waiting for someone to take the order.
    InLine(usize),
    /// Order taken; walking to a spot at the back.
    SteppingBack(usize),
    WaitingForFood(usize),
    Leaving {
        happy: bool,
    },
}

#[derive(Component)]
struct Customer {
    name: &'static str,
    /// What they are going to order, bottom bun first.
    recipe: Vec<Ingredient>,
    stage: Stage,
    /// Where they are walking to, on the floor.
    target: Vec3,
    /// Seconds left before they give up on the line.
    patience: f32,
    max_patience: f32,
    /// Distance walked, which drives the bob of their step.
    stride: f32,
    bubble: Entity,
    bubble_material: Handle<StandardMaterial>,
}

#[derive(Resource)]
struct CustomerAssets {
    body: Handle<Mesh>,
    limb: Handle<Mesh>,
    shirts: Vec<Handle<StandardMaterial>>,
    skins: Vec<Handle<StandardMaterial>>,
    hairs: Vec<Handle<StandardMaterial>>,
    trousers: Handle<StandardMaterial>,
    ink: Handle<StandardMaterial>,
}

fn load_customer_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut palette = |colors: &[(f32, f32, f32)]| -> Vec<Handle<StandardMaterial>> {
        colors
            .iter()
            .map(|&(r, g, b)| materials.add(surface(Color::srgb(r, g, b), 0.0, 0.75)))
            .collect()
    };
    let shirts = palette(&[
        (0.85, 0.25, 0.22),
        (0.2, 0.5, 0.82),
        (0.95, 0.7, 0.15),
        (0.3, 0.68, 0.42),
        (0.6, 0.36, 0.78),
        (0.92, 0.48, 0.2),
        (0.2, 0.7, 0.72),
        (0.9, 0.45, 0.62),
    ]);
    let skins = palette(&[
        (0.98, 0.82, 0.68),
        (0.9, 0.7, 0.52),
        (0.74, 0.52, 0.36),
        (0.55, 0.37, 0.25),
        (0.38, 0.25, 0.17),
    ]);
    let hairs = palette(&[
        (0.12, 0.09, 0.07),
        (0.36, 0.2, 0.1),
        (0.75, 0.55, 0.2),
        (0.6, 0.6, 0.62),
        (0.7, 0.25, 0.12),
    ]);
    let trousers = palette(&[(0.16, 0.18, 0.26)]).remove(0);
    let ink = palette(&[(0.08, 0.06, 0.06)]).remove(0);
    commands.insert_resource(CustomerAssets {
        body: meshes.add(Capsule3d::new(0.75, 0.9)),
        limb: meshes.add(Capsule3d::new(0.19, 1.0)),
        shirts,
        skins,
        hairs,
        trousers,
        ink,
    });
}

fn spawn_dining_room(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let sky = materials.add(StandardMaterial {
        base_color: Color::srgb(0.55, 0.8, 0.98),
        emissive: LinearRgba::rgb(0.35, 0.6, 0.9),
        ..default()
    });
    let screen = materials.add(StandardMaterial {
        base_color: Color::srgb(0.2, 0.75, 0.5),
        emissive: LinearRgba::rgb(0.1, 0.9, 0.5),
        ..default()
    });
    let mut paint = |r: f32, g: f32, b: f32, roughness: f32| {
        materials.add(surface(Color::srgb(r, g, b), 0.0, roughness))
    };
    let cream = paint(0.9, 0.86, 0.76, 0.5);
    let brick = paint(0.62, 0.2, 0.16, 0.5);
    let wall = paint(0.93, 0.8, 0.56, 0.9);
    let wood = paint(0.42, 0.22, 0.12, 0.6);
    let trim = paint(0.96, 0.94, 0.88, 0.5);
    let panel = paint(0.72, 0.16, 0.14, 0.5);
    let top = paint(0.58, 0.36, 0.2, 0.35);
    let dark = paint(0.14, 0.14, 0.16, 0.4);

    commands.spawn((
        PointLight {
            color: Color::srgb(1.0, 0.9, 0.75),
            intensity: 2_500_000.0,
            range: 40.0,
            ..default()
        },
        Transform::from_translation(ORIGIN + Vec3::new(0.0, 7.0, -1.0)),
    ));
    let mut block = |size: Vec3, at: Vec3, material: &Handle<StandardMaterial>| {
        commands.spawn((
            Mesh3d(meshes.add(Cuboid::from_size(size))),
            MeshMaterial3d(material.clone()),
            Transform::from_translation(ORIGIN + at),
        ));
    };

    // Checkerboard floor.
    block(
        Vec3::new(44.0, 0.4, 24.0),
        Vec3::new(0.0, FLOOR - 0.2, -2.0),
        &cream,
    );
    for column in -11..11 {
        for row in 0..6 {
            if (column + row) % 2 == 0 {
                block(
                    Vec3::new(2.0, 0.02, 2.0),
                    Vec3::new(
                        column as f32 * 2.0 + 1.0,
                        FLOOR + 0.01,
                        -9.0 + row as f32 * 2.0,
                    ),
                    &brick,
                );
            }
        }
    }

    // Back wall with wainscoting, two windows and the door customers come in by.
    block(
        Vec3::new(44.0, 14.0, 0.4),
        Vec3::new(0.0, FLOOR + 7.0, -10.2),
        &wall,
    );
    block(
        Vec3::new(44.0, 2.6, 0.2),
        Vec3::new(0.0, FLOOR + 1.3, -9.95),
        &wood,
    );
    block(
        Vec3::new(44.0, 0.16, 0.3),
        Vec3::new(0.0, FLOOR + 2.65, -9.95),
        &trim,
    );
    for x in [-5.5, 3.5] {
        let centre = Vec3::new(x, FLOOR + 5.6, -9.95);
        block(Vec3::new(5.6, 3.8, 0.2), centre, &trim);
        block(Vec3::new(5.1, 3.3, 0.24), centre, &sky);
        block(Vec3::new(0.14, 3.3, 0.3), centre, &trim);
        block(Vec3::new(5.1, 0.14, 0.3), centre, &trim);
    }
    block(
        Vec3::new(3.6, 7.0, 0.2),
        Vec3::new(11.5, FLOOR + 3.5, -9.95),
        &trim,
    );
    block(
        Vec3::new(3.1, 6.6, 0.24),
        Vec3::new(11.5, FLOOR + 3.3, -9.95),
        &wood,
    );
    block(
        Vec3::new(2.2, 2.2, 0.28),
        Vec3::new(11.5, FLOOR + 4.9, -9.95),
        &sky,
    );

    // The service counter the player stands behind.
    block(Vec3::new(32.0, 0.3, 4.4), Vec3::new(0.0, -0.15, 4.5), &top);
    block(Vec3::new(31.6, 3.2, 3.9), Vec3::new(0.0, -1.9, 4.6), &panel);
    block(
        Vec3::new(31.7, 0.2, 0.1),
        Vec3::new(0.0, -0.75, 2.62),
        &trim,
    );

    // A till, so it looks like a place that takes money.
    block(Vec3::new(1.7, 0.7, 1.3), Vec3::new(5.4, 0.35, 3.5), &dark);
    block(Vec3::new(1.3, 0.9, 0.12), Vec3::new(5.4, 1.1, 3.1), &dark);
    block(
        Vec3::new(1.1, 0.7, 0.04),
        Vec3::new(5.4, 1.1, 3.18),
        &screen,
    );
}

fn spawn_customer(
    commands: &mut Commands,
    assets: &CustomerAssets,
    food: &IngredientAssets,
    materials: &mut Assets<StandardMaterial>,
    rng: &mut GameRng,
    customer: (Stage, Vec3, Vec<Ingredient>),
) -> Entity {
    let (stage, target, recipe) = customer;
    let shirt = rng.pick(&assets.shirts).clone();
    let skin = rng.pick(&assets.skins).clone();
    let hair = rng.pick(&assets.hairs).clone();
    let bubble_material = materials.add(StandardMaterial {
        base_color: GREEN,
        unlit: true,
        ..default()
    });

    // The "!" bubble that says they'd like to order.
    let bubble = commands
        .spawn((
            Mesh3d(food.ball.clone()),
            MeshMaterial3d(bubble_material.clone()),
            Transform::from_xyz(0.0, BUBBLE, 0.0).with_scale(Vec3::splat(0.48)),
            NotShadowCaster,
            Visibility::Hidden,
            children![
                (
                    Mesh3d(food.cube.clone()),
                    MeshMaterial3d(assets.ink.clone()),
                    Transform::from_xyz(0.0, 0.18, 0.92).with_scale(Vec3::new(0.24, 0.72, 0.2)),
                    NotShadowCaster,
                ),
                (
                    Mesh3d(food.ball.clone()),
                    MeshMaterial3d(assets.ink.clone()),
                    Transform::from_xyz(0.0, -0.46, 0.84).with_scale(Vec3::splat(0.15)),
                    NotShadowCaster,
                ),
            ],
        ))
        .id();

    let root = commands
        .spawn((
            Customer {
                name: rng.pick(&NAMES),
                recipe,
                stage,
                target,
                patience: 0.0,
                max_patience: 1.0,
                stride: 0.0,
                bubble,
                bubble_material,
            },
            Hoverable::default(),
            Transform::from_translation(ORIGIN + DOOR),
            Visibility::default(),
        ))
        .add_child(bubble)
        .id();
    let mut part =
        |mesh: &Handle<Mesh>, material: &Handle<StandardMaterial>, at: Vec3, size: Vec3| {
            commands.spawn((
                Mesh3d(mesh.clone()),
                MeshMaterial3d(material.clone()),
                Transform::from_translation(at).with_scale(size),
                ChildOf(root),
            ));
        };
    for side in [-1.0, 1.0] {
        part(
            &food.disc,
            &assets.trousers,
            Vec3::new(side * 0.32, 0.8, 0.0),
            Vec3::new(0.24, 1.6, 0.24),
        );
        part(
            &assets.limb,
            &shirt,
            Vec3::new(side * 0.93, CHEST + 0.1, 0.0),
            Vec3::ONE,
        );
        part(
            &food.ball,
            &skin,
            Vec3::new(side * 0.93, CHEST - 0.72, 0.0),
            Vec3::splat(0.21),
        );
        part(
            &food.ball,
            &assets.ink,
            Vec3::new(side * 0.22, HEAD + 0.06, 0.57),
            Vec3::splat(0.07),
        );
    }
    part(&assets.body, &shirt, Vec3::Y * CHEST, Vec3::ONE);
    part(&food.ball, &skin, Vec3::Y * HEAD, Vec3::splat(0.62));
    part(
        &food.ball,
        &hair,
        Vec3::new(0.0, HEAD + 0.2, -0.08),
        Vec3::new(0.66, 0.52, 0.66),
    );
    // A small smile.
    part(
        &food.cube,
        &assets.ink,
        Vec3::new(0.0, HEAD - 0.22, 0.58),
        Vec3::new(0.22, 0.04, 0.04),
    );
    root
}

fn reset_lobby(
    mut commands: Commands,
    customers: Query<Entity, With<Customer>>,
    mut lobby: ResMut<Lobby>,
) {
    for customer in &customers {
        commands.entity(customer).despawn();
    }
    *lobby = Lobby {
        until_next: 1.0,
        ..default()
    };
}

#[allow(clippy::too_many_arguments)]
fn let_customers_in(
    mut commands: Commands,
    time: Res<Time>,
    shift: Res<Shift>,
    assets: Res<CustomerAssets>,
    food: Res<IngredientAssets>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut lobby: ResMut<Lobby>,
    mut rng: ResMut<GameRng>,
    mut feedback: Feedback,
) {
    lobby.until_next -= time.delta_secs();
    if lobby.until_next > 0.0 {
        return;
    }
    let Some(spot) = QUEUE_ORDER
        .into_iter()
        .find(|&spot| lobby.queue[spot].is_none())
    else {
        return;
    };
    lobby.until_next = rules::customer_interval(shift.elapsed) * rng.range(0.8, 1.2);
    let recipe = rules::recipe(shift.elapsed, || rng.f32());
    let target = Vec3::new(QUEUE_X[spot], FLOOR, QUEUE_Z);
    let customer = spawn_customer(
        &mut commands,
        &assets,
        &food,
        &mut materials,
        &mut rng,
        (Stage::Arriving(spot), target, recipe),
    );
    lobby.queue[spot] = Some(customer);
    feedback.sfx.write(PlaySfx::pitched(Sfx::NewOrder, 0.8));
}

/// Highlights the customer under the cursor and takes their order on a click.
#[allow(clippy::too_many_arguments)]
fn take_orders(
    mouse: Res<ButtonInput<MouseButton>>,
    station: Res<Station>,
    cursor: Res<Cursor>,
    mut lobby: ResMut<Lobby>,
    mut customers: Query<(Entity, &mut Customer, &mut Hoverable, &Transform)>,
    mut orders: MessageWriter<TakeOrder>,
    mut alerts: ResMut<Alerts>,
    mut feedback: Feedback,
) {
    let ray = cursor.ray().filter(|_| station.is_at(View::Counter));
    let pointed_at = ray.and_then(|ray| {
        customers
            .iter()
            .filter(|(_, customer, ..)| matches!(customer.stage, Stage::InLine(_)))
            .map(|(entity, _, _, transform)| {
                let feet = transform.translation;
                let distance = (ray_distance(ray, feet + Vec3::Y * HEAD) - 0.75)
                    .min(ray_distance(ray, feet + Vec3::Y * CHEST) - 1.05)
                    .min(ray_distance(ray, feet + Vec3::Y * BUBBLE) - 0.6);
                (entity, distance)
            })
            .filter(|(_, distance)| *distance < 0.0)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(entity, _)| entity)
    });
    let mut in_line = false;
    for (entity, mut customer, mut hoverable, transform) in &mut customers {
        let hot = pointed_at == Some(entity);
        if hoverable.hot != hot {
            hoverable.hot = hot;
        }
        let Stage::InLine(spot) = customer.stage else {
            continue;
        };
        in_line = true;
        if !hot || !mouse.just_pressed(MouseButton::Left) {
            continue;
        }
        let Some(seat) = WAIT_ORDER
            .into_iter()
            .find(|&seat| lobby.waiting[seat].is_none())
        else {
            feedback.sfx.write(Sfx::Nope.into());
            feedback.popups.write(Popup::screen(
                "TOO MANY OPEN ORDERS - SERVE ONE FIRST",
                POPUP,
                RED,
            ));
            continue;
        };
        lobby.queue[spot] = None;
        lobby.waiting[seat] = Some(entity);
        customer.stage = Stage::SteppingBack(seat);
        customer.target = Vec3::new(WAIT_X[seat], FLOOR, WAIT_Z);
        orders.write(TakeOrder {
            customer: entity,
            name: customer.name,
            recipe: customer.recipe.clone(),
        });
        feedback.popups.write(Popup::world(
            "ORDER TAKEN!",
            transform.translation + Vec3::Y * HEAD,
            GREEN,
        ));
    }
    alerts.set(View::Counter, in_line);
}

/// Sends customers home when their order is served, sent back or timed out.
fn see_customers_off(
    mut commands: Commands,
    mut closed: MessageReader<OrderClosed>,
    food: Res<IngredientAssets>,
    mut lobby: ResMut<Lobby>,
    mut customers: Query<(&mut Customer, &Transform)>,
    mut bursts: MessageWriter<Burst>,
) {
    for order in closed.read() {
        let Ok((mut customer, transform)) = customers.get_mut(order.customer) else {
            continue;
        };
        for seat in &mut lobby.waiting {
            if *seat == Some(order.customer) {
                *seat = None;
            }
        }
        let happy = order.outcome == Outcome::Served;
        customer.stage = Stage::Leaving { happy };
        customer.target = EXIT;
        if !happy {
            continue;
        }
        bursts.write(
            Burst::new(
                ParticleKind::Sparkle,
                transform.translation + Vec3::Y * HEAD,
                18,
            )
            .spread(0.8),
        );
        // They carry their burger out.
        let mut height = 0.0;
        for piece in [
            Piece::BunBottom,
            Piece::Patty(Cook::Done(Quality::Perfect)),
            Piece::Lettuce,
            Piece::BunTop,
        ] {
            let at =
                Transform::from_xyz(0.0, CHEST + 0.1 + height, 0.95).with_scale(Vec3::splat(0.42));
            let model = spawn_piece(&mut commands, &food, piece, at);
            commands.entity(model).insert(ChildOf(order.customer));
            height += piece.height() * 0.42;
        }
    }
}

fn move_customers(
    mut commands: Commands,
    time: Res<Time>,
    mut shift: ResMut<Shift>,
    mut lobby: ResMut<Lobby>,
    mut customers: Query<(Entity, &mut Customer, &mut Transform)>,
    mut feedback: Feedback,
) {
    let dt = time.delta_secs();
    for (entity, mut customer, mut transform) in &mut customers {
        let feet = (transform.translation - ORIGIN).with_y(FLOOR);
        let to_target = customer.target - feet;
        let distance = to_target.length();
        let step = WALK_SPEED * dt;
        let walking = distance > 0.02;
        let mut position = if distance <= step {
            customer.target
        } else {
            feet + to_target / distance * step
        };
        // Turn to face the way they walk, and the counter once they stand still.
        let facing = if walking {
            to_target.x.atan2(to_target.z)
        } else {
            0.0
        };
        transform.rotation = transform
            .rotation
            .slerp(Quat::from_rotation_y(facing), (10.0 * dt).min(1.0));
        if walking {
            customer.stride += step;
            let hop = match customer.stage {
                Stage::Leaving { happy: true } => 0.5,
                _ => 0.14,
            };
            position.y += hop * (customer.stride * 1.6).sin().abs();
        }
        transform.translation = ORIGIN + position;

        match customer.stage {
            Stage::Arriving(spot) if !walking => {
                customer.stage = Stage::InLine(spot);
                customer.max_patience = rules::queue_patience(shift.elapsed);
                customer.patience = customer.max_patience;
            }
            Stage::InLine(spot) => {
                customer.patience -= dt;
                if customer.patience <= 0.0 {
                    lobby.queue[spot] = None;
                    customer.stage = Stage::Leaving { happy: false };
                    customer.target = EXIT;
                    shift.strike();
                    feedback.shake.add(0.5);
                    feedback.sfx.write(Sfx::Strike.into());
                    feedback.popups.write(
                        Popup::screen(
                            format!("{} GOT TIRED OF WAITING! STRIKE", customer.name),
                            POPUP,
                            RED,
                        )
                        .big(),
                    );
                }
            }
            Stage::SteppingBack(seat) if !walking => {
                customer.stage = Stage::WaitingForFood(seat);
            }
            Stage::Leaving { .. } if !walking => {
                commands.entity(entity).despawn();
            }
            _ => {}
        }
    }
}

/// Shows the order bubble over customers in line: it bobs, and turns from green
/// to red as they lose patience.
fn style_bubbles(
    time: Res<Time>,
    customers: Query<&Customer>,
    mut bubbles: Query<(&mut Transform, &mut Visibility)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let t = time.elapsed_secs();
    for customer in &customers {
        let Ok((mut transform, mut visibility)) = bubbles.get_mut(customer.bubble) else {
            continue;
        };
        if !matches!(customer.stage, Stage::InLine(_)) {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        }
        visibility.set_if_neq(Visibility::Inherited);
        let left = (customer.patience / customer.max_patience).clamp(0.0, 1.0);
        // Impatient customers bounce their bubble faster.
        let speed = 3.0 + 6.0 * (1.0 - left);
        transform.translation.y = BUBBLE + 0.12 * (t * speed + customer.stride).sin();
        transform.rotation = Quat::from_rotation_y(PI * 0.06 * (t * 2.0).sin());
        if let Some(mut material) = materials.get_mut(&customer.bubble_material) {
            material.base_color = if left > 0.5 {
                blend(YELLOW, GREEN, (left - 0.5) * 2.0)
            } else {
                blend(RED, YELLOW, left * 2.0)
            };
        }
    }
}
