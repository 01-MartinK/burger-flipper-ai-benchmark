//! The seven ingredients: their 3D models, the pantry that tracks how much of
//! each is on the trays, and the piles and flying pieces that show it.

use std::f32::consts::{FRAC_PI_4, TAU};

use bevy::prelude::*;

use crate::GameState;
use crate::audio::{PlaySfx, Sfx};
use crate::hud::{CREAM, RED};
use crate::rules::{self, Ingredient, Protein, Quality};

/// Most pieces drawn in one pile; the label next to it has the real count.
const MAX_SHOWN: usize = 6;

pub fn plugin(app: &mut App) {
    app.init_resource::<Pantry>()
        .add_systems(PreStartup, load_ingredient_assets)
        .add_systems(OnEnter(GameState::Playing), restock)
        .add_systems(
            Update,
            (refresh_piles, refresh_stock_labels, animate_flights),
        );
}

/// Flat colour standing in for an ingredient on tickets and labels.
pub fn color(ingredient: Ingredient) -> Color {
    match ingredient {
        Ingredient::Bun => Color::srgb(0.87, 0.6, 0.27),
        Ingredient::Patty => Color::srgb(0.42, 0.22, 0.11),
        Ingredient::Cheese => Color::srgb(1.0, 0.78, 0.1),
        Ingredient::Lettuce => Color::srgb(0.42, 0.74, 0.22),
        Ingredient::Tomato => Color::srgb(0.88, 0.2, 0.14),
        Ingredient::Onions => Color::srgb(0.8, 0.62, 0.86),
        Ingredient::Bacon => Color::srgb(0.66, 0.16, 0.14),
    }
}

/// Which side of the kitchen a count refers to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shelf {
    /// Stock loaded from storage. Patties and bacon here are still raw.
    Stock,
    /// What can go on a burger right now: stock, but only grilled patties and bacon.
    Ready,
}

/// One portion of an ingredient, on its way onto (or off) a burger.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Portion {
    pub ingredient: Ingredient,
    /// How well it was grilled, for patties and bacon.
    pub quality: Option<Quality>,
}

/// Everything on the trays.
#[derive(Resource)]
pub struct Pantry {
    stock: [u32; 7],
    /// Grilled patties and bacon waiting on the prep table, oldest first.
    cooked: [Vec<Quality>; 2],
}

impl Default for Pantry {
    /// The trays as they are at the start of a shift.
    fn default() -> Self {
        Self {
            stock: rules::STARTER_STOCK,
            cooked: default(),
        }
    }
}

impl Pantry {
    pub fn count(&self, ingredient: Ingredient, shelf: Shelf) -> u32 {
        match (shelf, ingredient.protein()) {
            (Shelf::Ready, Some(protein)) => self.cooked[protein.index()].len() as u32,
            _ => self.stock[ingredient.index()],
        }
    }

    /// Loads a pack from storage and returns how many portions fitted on the tray.
    pub fn add_pack(&mut self, ingredient: Ingredient) -> u32 {
        let stock = &mut self.stock[ingredient.index()];
        let added = rules::PACK_SIZE.min(rules::TRAY_CAPACITY.saturating_sub(*stock));
        *stock += added;
        added
    }

    /// Takes a raw patty or strip of bacon for the grill.
    pub fn take_raw(&mut self, protein: Protein) -> bool {
        let stock = &mut self.stock[protein.ingredient().index()];
        let available = *stock > 0;
        *stock = stock.saturating_sub(1);
        available
    }

    pub fn has_room_for_cooked(&self, protein: Protein) -> bool {
        self.cooked[protein.index()].len() < rules::COOKED_CAPACITY
    }

    pub fn add_cooked(&mut self, protein: Protein, quality: Quality) {
        self.cooked[protein.index()].push(quality);
    }

    /// Takes a portion off the prep table trays.
    pub fn take(&mut self, ingredient: Ingredient) -> Option<Portion> {
        let quality = match ingredient.protein() {
            Some(protein) => {
                let cooked = &mut self.cooked[protein.index()];
                if cooked.is_empty() {
                    return None;
                }
                Some(cooked.remove(0))
            }
            None => {
                let stock = &mut self.stock[ingredient.index()];
                if *stock == 0 {
                    return None;
                }
                *stock -= 1;
                None
            }
        };
        Some(Portion {
            ingredient,
            quality,
        })
    }

    /// Returns a portion to the tray it came from.
    pub fn put_back(&mut self, portion: Portion) {
        match (portion.ingredient.protein(), portion.quality) {
            (Some(protein), Some(quality)) => self.cooked[protein.index()].insert(0, quality),
            _ => self.stock[portion.ingredient.index()] += 1,
        }
    }

    /// The pieces to draw for a pile of this ingredient, bottom first.
    fn pile(&self, ingredient: Ingredient, shelf: Shelf) -> Vec<Piece> {
        match (shelf, ingredient.protein()) {
            (Shelf::Ready, Some(protein)) => self.cooked[protein.index()]
                .iter()
                .take(MAX_SHOWN)
                .map(|&quality| Piece::grilled(protein, Cook::Done(quality)))
                .collect(),
            _ => {
                let shown = (self.stock[ingredient.index()] as usize).min(MAX_SHOWN);
                vec![Piece::in_stock(ingredient); shown]
            }
        }
    }
}

fn restock(mut pantry: ResMut<Pantry>) {
    *pantry = Pantry::default();
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cook {
    Raw,
    Done(Quality),
}

/// A 3D model of one layer of a burger.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Piece {
    BunBottom,
    BunTop,
    Patty(Cook),
    Cheese,
    Lettuce,
    Tomato,
    Onions,
    Bacon(Cook),
}

impl Piece {
    fn grilled(protein: Protein, cook: Cook) -> Self {
        match protein {
            Protein::Patty => Self::Patty(cook),
            Protein::Bacon => Self::Bacon(cook),
        }
    }

    /// How an ingredient looks while it sits in a crate or on a stock tray.
    pub fn in_stock(ingredient: Ingredient) -> Self {
        match ingredient {
            Ingredient::Bun => Self::BunTop,
            Ingredient::Patty => Self::Patty(Cook::Raw),
            Ingredient::Cheese => Self::Cheese,
            Ingredient::Lettuce => Self::Lettuce,
            Ingredient::Tomato => Self::Tomato,
            Ingredient::Onions => Self::Onions,
            Ingredient::Bacon => Self::Bacon(Cook::Raw),
        }
    }

    /// How a portion looks on a burger. Buns are a base or a lid.
    pub fn on_burger(portion: Portion, lid: bool) -> Self {
        let cook = Cook::Done(portion.quality.unwrap_or(Quality::Perfect));
        match portion.ingredient {
            Ingredient::Bun if lid => Self::BunTop,
            Ingredient::Bun => Self::BunBottom,
            Ingredient::Patty => Self::Patty(cook),
            Ingredient::Bacon => Self::Bacon(cook),
            other => Self::in_stock(other),
        }
    }

    /// How much taller the piece makes a stack.
    pub fn height(self) -> f32 {
        match self {
            Self::BunBottom => 0.3,
            Self::BunTop => 0.55,
            Self::Patty(_) => 0.28,
            Self::Cheese => 0.07,
            Self::Lettuce => 0.1,
            Self::Tomato => 0.11,
            Self::Onions => 0.12,
            Self::Bacon(_) => 0.09,
        }
    }

    /// Spacing of this piece in a pile on a tray, where bun lids nest into each other.
    fn pile_height(self) -> f32 {
        match self {
            Self::BunTop => 0.3,
            other => other.height(),
        }
    }
}

/// Shared meshes and materials all ingredient models are assembled from.
#[derive(Resource)]
pub struct IngredientAssets {
    /// Cylinder of radius 1 and height 1.
    pub disc: Handle<Mesh>,
    /// Sphere of radius 1.
    pub ball: Handle<Mesh>,
    /// Cube with sides of 1.
    pub cube: Handle<Mesh>,
    ring: Handle<Mesh>,
    pub steel: Handle<StandardMaterial>,
    bun: Handle<StandardMaterial>,
    crumb: Handle<StandardMaterial>,
    seed: Handle<StandardMaterial>,
    raw_meat: Handle<StandardMaterial>,
    meat: [Handle<StandardMaterial>; 3],
    cheese: Handle<StandardMaterial>,
    lettuce: Handle<StandardMaterial>,
    lettuce_dark: Handle<StandardMaterial>,
    tomato: Handle<StandardMaterial>,
    tomato_flesh: Handle<StandardMaterial>,
    onion: Handle<StandardMaterial>,
    raw_bacon: Handle<StandardMaterial>,
    bacon: [Handle<StandardMaterial>; 3],
    raw_fat: Handle<StandardMaterial>,
    fat: Handle<StandardMaterial>,
}

fn load_ingredient_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut matte = |r: f32, g: f32, b: f32, roughness: f32| {
        materials.add(StandardMaterial {
            base_color: Color::srgb(r, g, b),
            perceptual_roughness: roughness,
            ..default()
        })
    };
    let assets = IngredientAssets {
        disc: meshes.add(Cylinder::new(1.0, 1.0).mesh().resolution(40)),
        ball: meshes.add(Sphere::new(1.0).mesh().uv(32, 16)),
        cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        ring: meshes.add(Torus {
            minor_radius: 0.06,
            major_radius: 0.34,
        }),
        bun: matte(0.86, 0.56, 0.22, 0.55),
        crumb: matte(0.95, 0.82, 0.58, 0.8),
        seed: matte(0.98, 0.93, 0.78, 0.6),
        raw_meat: matte(0.78, 0.32, 0.35, 0.35),
        meat: [
            matte(0.5, 0.29, 0.13, 0.7),
            matte(0.4, 0.22, 0.1, 0.8),
            matte(0.27, 0.15, 0.08, 0.9),
        ],
        cheese: matte(1.0, 0.76, 0.1, 0.4),
        lettuce: matte(0.42, 0.74, 0.2, 0.6),
        lettuce_dark: matte(0.25, 0.58, 0.14, 0.6),
        tomato: matte(0.85, 0.13, 0.1, 0.35),
        tomato_flesh: matte(0.96, 0.4, 0.3, 0.3),
        onion: matte(0.93, 0.86, 0.95, 0.4),
        raw_bacon: matte(0.9, 0.5, 0.5, 0.4),
        bacon: [
            matte(0.62, 0.17, 0.11, 0.6),
            matte(0.48, 0.13, 0.08, 0.7),
            matte(0.32, 0.09, 0.06, 0.85),
        ],
        raw_fat: matte(0.98, 0.88, 0.84, 0.4),
        fat: matte(0.93, 0.72, 0.48, 0.6),
        steel: materials.add(StandardMaterial {
            base_color: Color::srgb(0.66, 0.68, 0.71),
            metallic: 0.85,
            perceptual_roughness: 0.32,
            ..default()
        }),
    };
    commands.insert_resource(assets);
}

fn quality_index(quality: Quality) -> usize {
    match quality {
        Quality::Perfect => 0,
        Quality::Good => 1,
        Quality::Dry => 2,
    }
}

/// Spawns the model of a piece. Its origin is the middle of its underside and a
/// bun is one unit in radius, so `transform` places and sizes it.
pub fn spawn_piece(
    commands: &mut Commands,
    assets: &IngredientAssets,
    piece: Piece,
    transform: Transform,
) -> Entity {
    let root = commands.spawn((transform, Visibility::default())).id();
    let mut part = |mesh: &Handle<Mesh>,
                    material: &Handle<StandardMaterial>,
                    at: Vec3,
                    size: Vec3,
                    turn: f32| {
        commands.spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material.clone()),
            Transform::from_translation(at)
                .with_scale(size)
                .with_rotation(Quat::from_rotation_y(turn)),
            ChildOf(root),
        ));
    };
    let around = |index: usize, of: usize, radius: f32, y: f32| {
        let angle = TAU * index as f32 / of as f32;
        Vec3::new(radius * angle.cos(), y, radius * angle.sin())
    };
    match piece {
        Piece::BunBottom => {
            part(
                &assets.disc,
                &assets.bun,
                Vec3::Y * 0.14,
                Vec3::new(1.0, 0.28, 1.0),
                0.0,
            );
            // The cut face.
            part(
                &assets.disc,
                &assets.crumb,
                Vec3::Y * 0.29,
                Vec3::new(0.93, 0.02, 0.93),
                0.0,
            );
        }
        Piece::BunTop => {
            // A squashed ball; the half below the cut face hides in whatever is underneath.
            part(
                &assets.ball,
                &assets.bun,
                Vec3::ZERO,
                Vec3::new(1.0, 0.55, 1.0),
                0.0,
            );
            part(
                &assets.disc,
                &assets.crumb,
                Vec3::Y * 0.01,
                Vec3::new(0.97, 0.02, 0.97),
                0.0,
            );
            let seed = Vec3::new(0.075, 0.03, 0.045);
            part(&assets.ball, &assets.seed, Vec3::Y * 0.55, seed, 0.4);
            for index in 0..7 {
                let radius = 0.5_f32;
                let y = 0.55 * (1.0 - radius * radius).sqrt();
                part(
                    &assets.ball,
                    &assets.seed,
                    around(index, 7, radius, y),
                    seed,
                    index as f32 * 2.3,
                );
            }
        }
        Piece::Patty(cook) => {
            let material = match cook {
                Cook::Raw => &assets.raw_meat,
                Cook::Done(quality) => &assets.meat[quality_index(quality)],
            };
            part(
                &assets.disc,
                material,
                Vec3::Y * 0.14,
                Vec3::new(0.95, 0.28, 0.95),
                0.0,
            );
        }
        Piece::Cheese => part(
            &assets.cube,
            &assets.cheese,
            Vec3::Y * 0.035,
            Vec3::new(1.7, 0.06, 1.7),
            FRAC_PI_4,
        ),
        Piece::Lettuce => {
            part(
                &assets.disc,
                &assets.lettuce,
                Vec3::Y * 0.04,
                Vec3::new(1.02, 0.06, 1.02),
                0.0,
            );
            // Ruffles around the edge.
            for index in 0..9 {
                let material = if index % 2 == 0 {
                    &assets.lettuce_dark
                } else {
                    &assets.lettuce
                };
                part(
                    &assets.ball,
                    material,
                    around(index, 9, 0.92, 0.05),
                    Vec3::new(0.3, 0.07, 0.3),
                    0.0,
                );
            }
        }
        Piece::Tomato => {
            for index in 0..3 {
                let at = around(index, 3, 0.45, 0.05);
                part(
                    &assets.disc,
                    &assets.tomato,
                    at,
                    Vec3::new(0.47, 0.1, 0.47),
                    0.0,
                );
                part(
                    &assets.disc,
                    &assets.tomato_flesh,
                    at,
                    Vec3::new(0.34, 0.106, 0.34),
                    0.0,
                );
            }
        }
        Piece::Onions => {
            for index in 0..3 {
                let at = around(index, 3, 0.42, 0.06);
                part(&assets.ring, &assets.onion, at, Vec3::ONE, 0.0);
                part(&assets.ring, &assets.onion, at, Vec3::splat(0.6), 0.0);
            }
        }
        Piece::Bacon(cook) => {
            let (meat, fat) = match cook {
                Cook::Raw => (&assets.raw_bacon, &assets.raw_fat),
                Cook::Done(quality) => (&assets.bacon[quality_index(quality)], &assets.fat),
            };
            for (z, turn) in [(-0.27, 0.07), (0.27, -0.05)] {
                part(
                    &assets.cube,
                    meat,
                    Vec3::new(0.0, 0.04, z),
                    Vec3::new(1.9, 0.07, 0.42),
                    turn,
                );
                part(
                    &assets.cube,
                    fat,
                    Vec3::new(0.0, 0.043, z),
                    Vec3::new(1.9, 0.07, 0.1),
                    turn,
                );
            }
        }
    }
    root
}

/// Spawns an empty steel tray whose floor is at `at`. Piles and labels go on top.
pub fn spawn_tray(
    commands: &mut Commands,
    assets: &IngredientAssets,
    at: Vec3,
    size: Vec2,
) -> Entity {
    let wall = 0.22;
    let parts = [
        (Vec3::new(0.0, -0.03, 0.0), Vec3::new(size.x, 0.06, size.y)),
        (
            Vec3::new(0.0, wall / 2.0, size.y / 2.0),
            Vec3::new(size.x, wall, 0.06),
        ),
        (
            Vec3::new(0.0, wall / 2.0, -size.y / 2.0),
            Vec3::new(size.x, wall, 0.06),
        ),
        (
            Vec3::new(size.x / 2.0, wall / 2.0, 0.0),
            Vec3::new(0.06, wall, size.y),
        ),
        (
            Vec3::new(-size.x / 2.0, wall / 2.0, 0.0),
            Vec3::new(0.06, wall, size.y),
        ),
    ];
    let tray = commands
        .spawn((Transform::from_translation(at), Visibility::default()))
        .id();
    for (offset, scale) in parts {
        commands.spawn((
            Mesh3d(assets.cube.clone()),
            MeshMaterial3d(assets.steel.clone()),
            Transform::from_translation(offset).with_scale(scale),
            ChildOf(tray),
        ));
    }
    tray
}

/// Keeps a stack of pieces under this entity in step with the pantry.
#[derive(Component)]
pub struct Pile {
    ingredient: Ingredient,
    shelf: Shelf,
    /// Size of the pieces; 1.0 is burger sized.
    scale: f32,
    shown: Vec<Piece>,
    pieces: Vec<Entity>,
}

impl Pile {
    pub fn new(ingredient: Ingredient, shelf: Shelf, scale: f32) -> Self {
        Self {
            ingredient,
            shelf,
            scale,
            shown: Vec::new(),
            pieces: Vec::new(),
        }
    }
}

fn refresh_piles(
    mut commands: Commands,
    pantry: Res<Pantry>,
    assets: Res<IngredientAssets>,
    mut piles: Query<(Entity, &mut Pile)>,
) {
    for (entity, mut pile) in &mut piles {
        let wanted = pantry.pile(pile.ingredient, pile.shelf);
        if wanted == pile.shown {
            continue;
        }
        for piece in pile.pieces.drain(..) {
            commands.entity(piece).despawn();
        }
        let mut height = 0.0;
        for (index, &piece) in wanted.iter().enumerate() {
            // A slight turn per piece makes the pile look hand stacked.
            let transform = Transform::from_xyz(0.0, height, 0.0)
                .with_scale(Vec3::splat(pile.scale))
                .with_rotation(Quat::from_rotation_y(index as f32 * 0.5));
            let model = spawn_piece(&mut commands, &assets, piece, transform);
            commands.entity(model).insert(ChildOf(entity));
            pile.pieces.push(model);
            height += piece.pile_height() * pile.scale;
        }
        pile.shown = wanted;
    }
}

/// Text that shows how much of an ingredient is left.
#[derive(Component)]
pub struct StockLabel {
    pub ingredient: Ingredient,
    pub shelf: Shelf,
}

fn refresh_stock_labels(
    pantry: Res<Pantry>,
    mut labels: Query<(&StockLabel, &mut Text, &mut TextColor)>,
) {
    for (label, mut text, mut color) in &mut labels {
        let count = pantry.count(label.ingredient, label.shelf);
        let value = format!("{} x{count}", label.ingredient.name());
        if text.0 != value {
            text.0 = value;
            color.0 = if count == 0 { RED } else { CREAM };
        }
    }
}

/// Throws an entity along an arc and despawns it when it lands.
#[derive(Component)]
pub struct Flight {
    pub from: Vec3,
    pub to: Vec3,
    /// Extra height at the top of the arc.
    pub arc: f32,
    pub duration: f32,
    /// Seconds flown; start below zero to delay take-off.
    pub age: f32,
    pub start_scale: f32,
    pub end_scale: f32,
    /// Played on landing.
    pub sound: Option<Sfx>,
}

fn animate_flights(
    mut commands: Commands,
    time: Res<Time>,
    mut flights: Query<(Entity, &mut Flight, &mut Transform, &mut Visibility)>,
    mut sfx: MessageWriter<PlaySfx>,
) {
    for (entity, mut flight, mut transform, mut visibility) in &mut flights {
        flight.age += time.delta_secs();
        let progress = (flight.age / flight.duration).clamp(0.0, 1.0);
        let eased = progress * progress * (3.0 - 2.0 * progress);
        transform.translation = flight.from.lerp(flight.to, eased)
            + Vec3::Y * flight.arc * 4.0 * progress * (1.0 - progress);
        transform.scale = Vec3::splat(flight.start_scale.lerp(flight.end_scale, progress));
        visibility.set_if_neq(if flight.age < 0.0 {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        });
        if progress >= 1.0 {
            commands.entity(entity).despawn();
            if let Some(sound) = flight.sound {
                sfx.write(sound.into());
            }
        }
    }
}
