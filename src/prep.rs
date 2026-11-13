//! The prep table: trays of ingredients along the back, a plate in the middle.
//! Ingredients are dragged from the trays and dropped on the plate to stack a
//! burger for the selected ticket; the bell sends it out.

use bevy::prelude::*;

use crate::GameState;
use crate::audio::{PlaySfx, Sfx};
use crate::effects::{Burst, ParticleKind};
use crate::game::{Feedback, running};
use crate::hud::{Popup, RED, YELLOW, world_label};
use crate::ingredients::{
    Flight, IngredientAssets, Pantry, Piece, Pile, Portion, Shelf, StockLabel, spawn_piece,
    spawn_tray,
};
use crate::kitchen::{spawn_backdrop, surface};
use crate::orders::{Order, SelectedOrder, ServeBurger};
use crate::rules::Ingredient;
use crate::stations::{Cursor, Hoverable, Station, View, ray_hits_box};

const ORIGIN: Vec3 = View::Prep.origin();
const TRAY_SIZE: Vec2 = Vec2::new(1.6, 1.8);
const TRAY_Z: f32 = -3.3;
/// Middle of the plate, on the cutting board.
const PLATE: Vec3 = Vec3::new(1.0, 0.2, 1.0);
const PLATE_RADIUS: f32 = 1.6;
/// How far outside the plate a drop still lands on it.
const DROP_MARGIN: f32 = 0.5;
/// Layers can sit this far off the middle of the plate...
const MAX_OFFSET: f32 = 0.8;
/// ...and anything closer than this counts as dead centre.
const NEAT_OFFSET: f32 = 0.12;
/// How high above the stack a dragged piece is carried.
const CARRY_HEIGHT: f32 = 0.9;
const DROP_TIME: f32 = 0.16;
const BELL: Vec3 = Vec3::new(4.9, 0.0, 1.9);
const BIN: Vec3 = Vec3::new(6.7, 0.0, -0.3);
/// Where the model of the ordered burger floats.
const SAMPLE: Vec3 = Vec3::new(-3.7, 0.25, 1.0);
const SAMPLE_SCALE: f32 = 0.55;
/// Where messages about the plate pop up.
const POPUP: Vec2 = Vec2::new(690.0, 330.0);

pub fn plugin(app: &mut App) {
    app.init_resource::<Plate>()
        .init_resource::<Hand>()
        .add_systems(Startup, spawn_prep_table)
        .add_systems(OnEnter(GameState::Playing), clear_table)
        .add_systems(
            Update,
            (reach, carry, settle_layers, show_sample)
                .chain()
                .run_if(in_state(GameState::Playing).and_then(running)),
        );
}

/// The burger being stacked on the plate, bottom layer first.
#[derive(Resource, Default)]
struct Plate {
    layers: Vec<Layer>,
}

struct Layer {
    portion: Portion,
    piece: Piece,
    model: Entity,
    /// Where the layer was dropped, relative to the middle of the plate.
    offset: Vec2,
}

impl Plate {
    /// Height of the top of the stack.
    fn height(&self) -> f32 {
        PLATE.y
            + self
                .layers
                .iter()
                .map(|layer| layer.piece.height())
                .sum::<f32>()
    }

    /// How untidy the stack is, from 0 (dead centre) to 1.
    fn sloppiness(&self) -> f32 {
        let total: f32 = self
            .layers
            .iter()
            .map(|layer| {
                ((layer.offset.length() - NEAT_OFFSET) / (MAX_OFFSET - NEAT_OFFSET)).clamp(0.0, 1.0)
            })
            .sum();
        total / self.layers.len().max(1) as f32
    }
}

/// The piece being dragged.
#[derive(Resource, Default)]
struct Hand(Option<Held>);

struct Held {
    portion: Portion,
    piece: Piece,
    model: Entity,
}

/// A piece dropping onto the stack.
#[derive(Component)]
struct Settling {
    from: Vec3,
    to: Vec3,
    age: f32,
}

#[derive(Component)]
struct Tray(Ingredient);

#[derive(Component)]
struct Bell;

#[derive(Component)]
struct Bin;

/// The floating model of the selected order, and the order it shows.
#[derive(Component, Default)]
struct Sample {
    order: Option<Entity>,
    pieces: Vec<Entity>,
}

#[derive(Component)]
struct SampleLabel;

fn tray_spot(ingredient: Ingredient) -> Vec3 {
    Vec3::new(-4.4 + 1.8 * ingredient.index() as f32, 0.06, TRAY_Z)
}

fn spawn_prep_table(
    mut commands: Commands,
    assets: Res<IngredientAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Butcher block table in front of a cream tiled wall.
    spawn_backdrop(
        &mut commands,
        &mut meshes,
        &mut materials,
        ORIGIN + Vec3::Y * 0.55,
        surface(Color::srgb(0.62, 0.42, 0.24), 0.0, 0.55),
        Color::srgb(0.85, 0.8, 0.68),
    );
    let mut paint = |r: f32, g: f32, b: f32, metallic: f32, roughness: f32| {
        materials.add(surface(Color::srgb(r, g, b), metallic, roughness))
    };
    let board = paint(0.8, 0.62, 0.4, 0.0, 0.6);
    let china = paint(0.95, 0.95, 0.92, 0.0, 0.25);
    let slate = paint(0.16, 0.17, 0.2, 0.0, 0.8);
    let brass = paint(0.95, 0.72, 0.25, 0.9, 0.25);
    let black = paint(0.06, 0.06, 0.07, 0.2, 0.5);
    let green = paint(0.2, 0.3, 0.25, 0.1, 0.6);
    let rim = meshes.add(Torus {
        minor_radius: 0.06,
        major_radius: PLATE_RADIUS - 0.06,
    });

    // Trays along the back, one per ingredient.
    for ingredient in Ingredient::ALL {
        let at = ORIGIN + tray_spot(ingredient);
        let tray = spawn_tray(&mut commands, &assets, at, TRAY_SIZE);
        commands
            .entity(tray)
            .insert((Hoverable::default(), Tray(ingredient)))
            .with_child((
                Transform::default(),
                Visibility::default(),
                Pile::new(ingredient, Shelf::Ready, 0.62),
            ));
        commands.spawn((
            world_label("", at + Vec3::new(0.0, -0.06, TRAY_SIZE.y / 2.0 + 0.15)),
            StockLabel {
                ingredient,
                shelf: Shelf::Ready,
            },
        ));
    }

    let part = |commands: &mut Commands,
                mesh: &Handle<Mesh>,
                material: &Handle<StandardMaterial>,
                at: Vec3,
                size: Vec3| {
        commands
            .spawn((
                Mesh3d(mesh.clone()),
                MeshMaterial3d(material.clone()),
                Transform::from_translation(at).with_scale(size),
            ))
            .id()
    };

    // Cutting board and plate.
    let plate = ORIGIN + PLATE;
    part(
        &mut commands,
        &assets.cube,
        &board,
        plate.with_y(0.06),
        Vec3::new(4.8, 0.12, 4.4),
    );
    part(
        &mut commands,
        &assets.disc,
        &china,
        plate - Vec3::Y * 0.04,
        Vec3::new(PLATE_RADIUS, 0.08, PLATE_RADIUS),
    );
    part(&mut commands, &rim, &china, plate, Vec3::ONE);

    // The bell that sends the burger out.
    let bell = commands
        .spawn((
            Transform::from_translation(ORIGIN + BELL),
            Visibility::default(),
            Hoverable::default(),
            Bell,
        ))
        .id();
    for (mesh, material, at, size) in [
        (
            &assets.disc,
            &black,
            Vec3::Y * 0.06,
            Vec3::new(0.75, 0.12, 0.75),
        ),
        (
            &assets.ball,
            &brass,
            Vec3::Y * 0.1,
            Vec3::new(0.6, 0.5, 0.6),
        ),
        (&assets.ball, &black, Vec3::Y * 0.62, Vec3::splat(0.1)),
    ] {
        let piece = part(&mut commands, mesh, material, at, size);
        commands.entity(piece).insert(ChildOf(bell));
    }
    commands.spawn(world_label("SERVE", ORIGIN + BELL + Vec3::Z * 0.95));

    // A bin for burgers that went wrong.
    let bin = commands
        .spawn((
            Transform::from_translation(ORIGIN + BIN),
            Visibility::default(),
            Hoverable::default(),
            Bin,
        ))
        .id();
    for (material, at, size) in [
        (&green, Vec3::Y * 0.55, Vec3::new(0.75, 1.1, 0.75)),
        (&black, Vec3::Y * 1.1, Vec3::new(0.64, 0.02, 0.64)),
    ] {
        let piece = part(&mut commands, &assets.disc, material, at, size);
        commands.entity(piece).insert(ChildOf(bin));
    }
    commands.spawn(world_label("TRASH", ORIGIN + BIN + Vec3::Z * 1.0));

    // The stand the ordered burger floats above.
    part(
        &mut commands,
        &assets.disc,
        &slate,
        ORIGIN + SAMPLE.with_y(0.06),
        Vec3::new(1.15, 0.12, 1.15),
    );
    commands.spawn((
        Transform::from_translation(ORIGIN + SAMPLE),
        Visibility::default(),
        Sample::default(),
    ));
    commands.spawn((
        world_label("", ORIGIN + SAMPLE.with_y(0.0) + Vec3::Z * 1.3),
        SampleLabel,
    ));

    commands.spawn((
        PointLight {
            color: Color::srgb(1.0, 0.93, 0.82),
            intensity: 1_500_000.0,
            range: 30.0,
            ..default()
        },
        Transform::from_translation(ORIGIN + Vec3::new(0.0, 7.0, 3.0)),
    ));
}

fn clear_table(mut commands: Commands, mut plate: ResMut<Plate>, mut hand: ResMut<Hand>) {
    for layer in plate.layers.drain(..) {
        commands.entity(layer.model).despawn();
    }
    if let Some(held) = hand.0.take() {
        commands.entity(held.model).despawn();
    }
}

/// Handles a press of the mouse button: ring the bell, empty the plate, lift the
/// top layer back off the burger, or take a piece from a tray.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn reach(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    station: Res<Station>,
    cursor: Res<Cursor>,
    assets: Res<IngredientAssets>,
    selected: Res<SelectedOrder>,
    mut pantry: ResMut<Pantry>,
    mut plate: ResMut<Plate>,
    mut hand: ResMut<Hand>,
    mut targets: Query<(
        &mut Hoverable,
        &Transform,
        Option<&Tray>,
        Has<Bell>,
        Has<Bin>,
    )>,
    mut burgers: MessageWriter<ServeBurger>,
    mut feedback: Feedback,
) {
    let ray = cursor
        .ray()
        .filter(|_| station.is_at(View::Prep) && hand.0.is_none());
    let hit =
        |centre: Vec3, half_size: Vec3| ray.and_then(|ray| ray_hits_box(ray, centre, half_size));
    let pressed = mouse.just_pressed(MouseButton::Left);

    // Whatever the cursor is on swells a little.
    let mut tray = None;
    let mut bell = false;
    let mut bin = false;
    for (mut hoverable, transform, is_tray, is_bell, is_bin) in &mut targets {
        let at = transform.translation;
        let hot = if let Some(Tray(ingredient)) = is_tray {
            let over = hit(
                at + Vec3::Y * 0.35,
                Vec3::new(TRAY_SIZE.x / 2.0, 0.5, TRAY_SIZE.y / 2.0),
            )
            .is_some();
            if over {
                tray = Some(*ingredient);
            }
            over
        } else if is_bell {
            bell = hit(at + Vec3::Y * 0.35, Vec3::new(0.8, 0.45, 0.8)).is_some();
            bell
        } else if is_bin {
            bin = hit(at + Vec3::Y * 0.55, Vec3::new(0.8, 0.6, 0.8)).is_some();
            bin
        } else {
            false
        };
        if hoverable.hot != hot {
            hoverable.hot = hot;
        }
    }
    if !pressed {
        return;
    }

    if bell {
        if plate.layers.len() < 2 {
            feedback.sfx.write(Sfx::Nope.into());
            feedback
                .popups
                .write(Popup::screen("STACK A BURGER FIRST", POPUP, YELLOW));
        } else if selected.0.is_none() {
            feedback.sfx.write(Sfx::Nope.into());
            feedback.popups.write(Popup::screen(
                "NOBODY ORDERED YET - TAKE AN ORDER AT THE COUNTER",
                POPUP,
                YELLOW,
            ));
        } else {
            burgers.write(ServeBurger {
                built: plate
                    .layers
                    .iter()
                    .map(|layer| layer.portion.ingredient)
                    .collect(),
                cooked: plate
                    .layers
                    .iter()
                    .filter_map(|layer| layer.portion.quality)
                    .collect(),
                sloppiness: plate.sloppiness(),
            });
            feedback.sfx.write(Sfx::Serve.into());
            feedback.bursts.write(
                Burst::new(ParticleKind::Sparkle, ORIGIN + PLATE + Vec3::Y * 0.5, 20).spread(0.8),
            );
            // Off it goes, over the pass to the customer.
            let away = ORIGIN + Vec3::new(PLATE.x - 4.0, 5.0, -9.0);
            send_plate_to(&mut commands, &mut plate, away, 0.15);
        }
        return;
    }
    if bin {
        if !plate.layers.is_empty() {
            let bin_mouth = ORIGIN + BIN + Vec3::Y * 0.9;
            send_plate_to(&mut commands, &mut plate, bin_mouth, 0.4);
            feedback.sfx.write(Sfx::Trash.into());
        }
        return;
    }

    // Pressing on the burger lifts its top layer off again.
    let stack_height = plate.height() - PLATE.y;
    let on_burger = !plate.layers.is_empty()
        && hit(
            ORIGIN + PLATE + Vec3::Y * stack_height / 2.0,
            Vec3::new(1.15, stack_height / 2.0 + 0.15, 1.15),
        )
        .is_some();
    if on_burger {
        if let Some(layer) = plate.layers.pop() {
            commands.entity(layer.model).remove::<Settling>();
            hand.0 = Some(Held {
                portion: layer.portion,
                piece: layer.piece,
                model: layer.model,
            });
            feedback.sfx.write(PlaySfx::pitched(Sfx::Flip, 1.2));
        }
        return;
    }

    let Some(ingredient) = tray else {
        return;
    };
    let Some(portion) = pantry.take(ingredient) else {
        let advice = match ingredient.protein() {
            Some(_) => "GRILL SOME FIRST",
            None => "RESTOCK IN STORAGE",
        };
        feedback.sfx.write(Sfx::Nope.into());
        feedback.popups.write(Popup::world(
            format!("NO {} - {advice}", ingredient.name()),
            ORIGIN + tray_spot(ingredient),
            RED,
        ));
        return;
    };
    // The first bun is the base of the burger; any after that is a lid.
    let has_base = plate
        .layers
        .iter()
        .any(|layer| layer.portion.ingredient == Ingredient::Bun);
    let piece = Piece::on_burger(portion, has_base);
    let model = spawn_piece(
        &mut commands,
        &assets,
        piece,
        Transform::from_translation(ORIGIN + tray_spot(ingredient) + Vec3::Y * 0.4),
    );
    hand.0 = Some(Held {
        portion,
        piece,
        model,
    });
    feedback.sfx.write(PlaySfx::pitched(Sfx::Flip, 1.2));
}

/// Throws everything on the plate towards a point and clears the plate.
fn send_plate_to(commands: &mut Commands, plate: &mut Plate, to: Vec3, end_scale: f32) {
    let mut height = PLATE.y;
    for (index, layer) in plate.layers.drain(..).enumerate() {
        let from = ORIGIN + Vec3::new(PLATE.x + layer.offset.x, height, PLATE.z + layer.offset.y);
        height += layer.piece.height();
        commands
            .entity(layer.model)
            .remove::<Settling>()
            .insert(Flight {
                from,
                to,
                arc: 2.0,
                duration: 0.5,
                age: -0.02 * index as f32,
                start_scale: 1.0,
                end_scale,
                sound: None,
            });
    }
}

/// Moves the held piece with the cursor, and drops it when the button is let go:
/// onto the stack if it is over the plate, back onto its tray otherwise.
#[allow(clippy::too_many_arguments)]
fn carry(
    mut commands: Commands,
    time: Res<Time>,
    mouse: Res<ButtonInput<MouseButton>>,
    station: Res<Station>,
    cursor: Res<Cursor>,
    mut pantry: ResMut<Pantry>,
    mut plate: ResMut<Plate>,
    mut hand: ResMut<Hand>,
    mut transforms: Query<&mut Transform>,
    mut sfx: MessageWriter<PlaySfx>,
) {
    let Some(held) = &hand.0 else {
        return;
    };
    let Ok(mut transform) = transforms.get_mut(held.model) else {
        hand.0 = None;
        return;
    };
    let top = plate.height();
    let at_table = station.is_at(View::Prep);
    if at_table && mouse.pressed(MouseButton::Left) {
        if let Some(point) = cursor.on_plane(top + CARRY_HEIGHT) {
            let ease = 1.0 - (-25.0 * time.delta_secs()).exp();
            transform.translation = transform.translation.lerp(point, ease);
            // Swing a little with the movement.
            let lag = point - transform.translation;
            transform.rotation =
                Quat::from_rotation_z(-lag.x * 0.25) * Quat::from_rotation_x(lag.z * 0.25);
        }
        return;
    }

    let Some(held) = hand.0.take() else {
        return;
    };
    transform.rotation = Quat::IDENTITY;
    let centre = (ORIGIN + PLATE).xz();
    let dropped_at = cursor
        .on_plane(top)
        .filter(|_| at_table)
        .map(|point| point.xz() - centre)
        .filter(|offset| offset.length() <= PLATE_RADIUS + DROP_MARGIN);
    let Some(offset) = dropped_at else {
        pantry.put_back(held.portion);
        commands.entity(held.model).despawn();
        sfx.write(PlaySfx::pitched(Sfx::Flip, 0.8));
        return;
    };
    let offset = offset.clamp_length_max(MAX_OFFSET);
    commands.entity(held.model).insert(Settling {
        from: transform.translation,
        to: ORIGIN + Vec3::new(PLATE.x + offset.x, top, PLATE.z + offset.y),
        age: 0.0,
    });
    plate.layers.push(Layer {
        portion: held.portion,
        piece: held.piece,
        model: held.model,
        offset,
    });
}

/// Drops released pieces onto the stack, with a little squash as they land.
fn settle_layers(
    mut commands: Commands,
    time: Res<Time>,
    mut layers: Query<(Entity, &mut Settling, &mut Transform)>,
    mut feedback: Feedback,
) {
    for (entity, mut settling, mut transform) in &mut layers {
        let landed = settling.age >= DROP_TIME;
        settling.age += time.delta_secs();
        let fall = (settling.age / DROP_TIME).min(1.0);
        transform.translation = settling.from.lerp(settling.to, fall * fall);
        let squash = if settling.age < DROP_TIME {
            0.0
        } else {
            (1.0 - (settling.age - DROP_TIME) / 0.18).max(0.0)
        };
        transform.scale = Vec3::new(1.0 + 0.1 * squash, 1.0 - 0.3 * squash, 1.0 + 0.1 * squash);
        if !landed && settling.age >= DROP_TIME {
            feedback.sfx.write(Sfx::Land.into());
        }
        if squash <= 0.0 && settling.age > DROP_TIME {
            transform.scale = Vec3::ONE;
            commands.entity(entity).remove::<Settling>();
        }
    }
}

/// Keeps a slowly turning, pulled-apart model of the selected order floating
/// next to the plate, to copy from.
fn show_sample(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<IngredientAssets>,
    selected: Res<SelectedOrder>,
    orders: Query<&Order>,
    mut sample: Single<(Entity, &mut Sample, &mut Transform)>,
    mut label: Single<&mut Text, With<SampleLabel>>,
) {
    let (entity, sample, transform) = &mut *sample;
    transform.rotate_y(0.5 * time.delta_secs());
    let order = selected
        .0
        .and_then(|ticket| orders.get(ticket).ok().map(|order| (ticket, order)));
    if sample.order == order.map(|(ticket, _)| ticket) {
        return;
    }
    sample.order = order.map(|(ticket, _)| ticket);
    for piece in sample.pieces.drain(..) {
        commands.entity(piece).despawn();
    }
    let Some((_, order)) = order else {
        label.0 = "NO ORDERS YET".to_string();
        return;
    };
    label.0 = format!("ORDER #{} - {}", order.id, order.name);
    // Spread the layers apart so each one can be told from the next.
    let gap = (3.0 / order.recipe.len() as f32).min(0.5);
    let mut height = 0.0;
    for (index, &ingredient) in order.recipe.iter().enumerate() {
        let portion = Portion {
            ingredient,
            quality: None,
        };
        let piece = Piece::on_burger(portion, index > 0);
        let model = spawn_piece(
            &mut commands,
            &assets,
            piece,
            Transform::from_xyz(0.0, height, 0.0).with_scale(Vec3::splat(SAMPLE_SCALE)),
        );
        commands.entity(model).insert(ChildOf(*entity));
        sample.pieces.push(model);
        height += piece.height() * SAMPLE_SCALE + gap;
    }
}
