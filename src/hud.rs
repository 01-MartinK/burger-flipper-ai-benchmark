//! Everything drawn over the 3D scene: score panel, patty gauges, floating
//! popups, and the title, pause and game-over screens.

use bevy::color::Mix;
use bevy::prelude::*;

use crate::GameState;
use crate::audio::{PlaySfx, Sfx};
use crate::game::{HighScore, Shift};
use crate::kitchen::{BIN_MOUTH, MainCamera, SLOTS, warmer_centre};
use crate::orders::Warmer;
use crate::patty::{PATTY_RADIUS, Patty, Phase};
use crate::rules::{self, Verdict};

pub const GREEN: Color = Color::srgb(0.35, 0.9, 0.35);
pub const YELLOW: Color = Color::srgb(1.0, 0.85, 0.2);
pub const ORANGE: Color = Color::srgb(1.0, 0.55, 0.15);
pub const RED: Color = Color::srgb(1.0, 0.28, 0.22);
const GOLD: Color = Color::srgb(1.0, 0.75, 0.25);
const CREAM: Color = Color::srgb(1.0, 0.95, 0.85);
const PANEL: Color = Color::srgba(0.08, 0.06, 0.05, 0.85);

const POPUP_LIFETIME: f32 = 1.3;
const POPUP_WIDTH: f32 = 520.0;
const GAUGE_WIDTH: f32 = 96.0;
/// Seconds the game-over screen ignores clicks, so a frantic click doesn't skip it.
const GAME_OVER_LOCK: f32 = 1.2;

pub fn plugin(app: &mut App) {
    app.add_message::<Popup>()
        .init_resource::<InputLock>()
        .add_systems(
            Startup,
            (
                spawn_score_panel,
                spawn_controls_hint,
                spawn_pause_screen,
                spawn_world_labels,
            ),
        )
        .add_systems(OnEnter(GameState::Title), spawn_title_screen)
        .add_systems(OnEnter(GameState::GameOver), spawn_game_over_screen)
        .add_systems(
            Update,
            (
                start_shift_on_click.run_if(not(in_state(GameState::Playing))),
                update_score_panel,
                (attach_gauges, update_gauges).chain(),
                (spawn_popups, animate_popups).chain(),
                place_world_labels,
                show_pause_screen,
                blink,
            ),
        );
}

/// Mixes two colours perceptually.
pub fn blend(from: Color, to: Color, t: f32) -> Color {
    Oklaba::from(from).mix(&Oklaba::from(to), t).into()
}

/// Floating text, either above a point in the kitchen or at a fixed spot on screen.
#[derive(Message, Clone)]
pub struct Popup {
    text: String,
    color: Color,
    anchor: Anchor,
    size: f32,
}

#[derive(Clone, Copy)]
enum Anchor {
    World(Vec3),
    Screen(Vec2),
}

impl Popup {
    pub fn world(text: impl Into<String>, at: Vec3, color: Color) -> Self {
        Self {
            text: text.into(),
            color,
            anchor: Anchor::World(at),
            size: 26.0,
        }
    }

    pub fn screen(text: impl Into<String>, at: Vec2, color: Color) -> Self {
        Self {
            text: text.into(),
            color,
            anchor: Anchor::Screen(at),
            size: 28.0,
        }
    }

    pub fn big(mut self) -> Self {
        self.size *= 1.4;
        self
    }
}

#[derive(Component)]
struct FloatingText {
    anchor: Anchor,
    color: Color,
    age: f32,
}

/// Cook gauge shown under a patty on the grill.
#[derive(Component)]
struct Gauge {
    patty: Entity,
    marker: Entity,
    label: Entity,
}

#[derive(Component)]
enum Readout {
    Score,
    Streak,
    Warmer,
    Clock,
}

#[derive(Component)]
struct StrikeBox(u32);

/// Caption pinned to a point in the kitchen.
#[derive(Component)]
struct WorldLabel(Vec3);

#[derive(Component)]
struct PauseScreen;

#[derive(Component)]
struct Blink;

/// Real seconds left before the start/restart click is accepted.
#[derive(Resource, Default)]
struct InputLock(f32);

fn label(value: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(value),
        TextFont::from_font_size(FontSize::Px(size)),
        TextColor(color),
        TextLayout::justify(Justify::Center),
        TextShadow {
            offset: Vec2::splat((size / 14.0).max(1.0)),
            color: Color::srgba(0.0, 0.0, 0.0, 0.75),
        },
    )
}

fn full_screen() -> Node {
    Node {
        position_type: PositionType::Absolute,
        width: percent(100),
        height: percent(100),
        flex_direction: FlexDirection::Column,
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        row_gap: px(14),
        ..default()
    }
}

fn spawn_score_panel(mut commands: Commands) {
    let strike_box = |index| {
        (
            Text::new("X"),
            TextFont::from_font_size(FontSize::Px(16.0)),
            TextColor(Color::NONE),
            TextLayout::justify(Justify::Center),
            Node {
                width: px(26),
                height: px(26),
                padding: UiRect::top(px(2)),
                border: UiRect::all(px(2)),
                border_radius: BorderRadius::all(px(5)),
                ..default()
            },
            BorderColor::all(Color::srgb(0.45, 0.4, 0.36)),
            StrikeBox(index),
        )
    };
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: px(12),
            top: px(12),
            width: px(240),
            padding: UiRect::all(px(12)),
            flex_direction: FlexDirection::Column,
            row_gap: px(4),
            border_radius: BorderRadius::all(px(10)),
            ..default()
        },
        BackgroundColor(PANEL),
        children![
            (
                Text::new("SCORE"),
                TextFont::from_font_size(FontSize::Px(14.0)),
                TextColor(CREAM.with_alpha(0.7)),
            ),
            (
                Text::new("0"),
                TextFont::from_font_size(FontSize::Px(40.0)),
                TextColor(GOLD),
                Readout::Score,
            ),
            (
                Text::new(""),
                TextFont::from_font_size(FontSize::Px(16.0)),
                TextColor(ORANGE),
                Readout::Streak,
            ),
            (
                Node {
                    column_gap: px(6),
                    align_items: AlignItems::Center,
                    margin: UiRect::vertical(px(4)),
                    ..default()
                },
                children![
                    (
                        Text::new("STRIKES"),
                        TextFont::from_font_size(FontSize::Px(14.0)),
                        TextColor(CREAM.with_alpha(0.7)),
                        Node {
                            margin: UiRect::right(px(4)),
                            ..default()
                        },
                    ),
                    strike_box(0),
                    strike_box(1),
                    strike_box(2),
                ],
            ),
            (
                Text::new(""),
                TextFont::from_font_size(FontSize::Px(14.0)),
                TextColor(CREAM),
                Readout::Warmer,
            ),
            (
                Text::new(""),
                TextFont::from_font_size(FontSize::Px(14.0)),
                TextColor(CREAM),
                Readout::Clock,
            ),
        ],
    ));
}

fn spawn_controls_hint(mut commands: Commands) {
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            bottom: px(10),
            width: percent(100),
            justify_content: JustifyContent::Center,
            ..default()
        },
        children![label(
            "CLICK GRILL: drop a patty   |   CLICK PATTY: flip it, or serve it when READY   |   P: pause",
            14.0,
            CREAM.with_alpha(0.75),
        )],
    ));
}

fn spawn_world_labels(mut commands: Commands) {
    let captions = [
        ("TRASH", BIN_MOUTH.with_y(-0.55) + Vec3::Z * 1.0),
        ("WARMER", warmer_centre().with_y(-0.45) + Vec3::Z * 1.1),
    ];
    for (text, at) in captions {
        commands.spawn((
            label(text, 15.0, CREAM.with_alpha(0.8)),
            Node {
                position_type: PositionType::Absolute,
                width: px(160),
                ..default()
            },
            Visibility::Hidden,
            WorldLabel(at),
        ));
    }
}

fn place_world_labels(
    camera: Single<(&Camera, &GlobalTransform), With<MainCamera>>,
    mut labels: Query<(&WorldLabel, &mut Node, &mut Visibility)>,
) {
    let (camera, camera_transform) = *camera;
    for (label, mut node, mut visibility) in &mut labels {
        let Ok(position) = camera.world_to_viewport(camera_transform, label.0) else {
            continue;
        };
        node.left = px(position.x - 80.0);
        node.top = px(position.y);
        visibility.set_if_neq(Visibility::Inherited);
    }
}

fn spawn_pause_screen(mut commands: Commands) {
    commands.spawn((
        full_screen(),
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
        GlobalZIndex(20),
        Visibility::Hidden,
        PauseScreen,
        children![
            label("PAUSED", 64.0, CREAM),
            label("Press P to keep cooking", 22.0, CREAM),
        ],
    ));
}

fn spawn_title_screen(mut commands: Commands, high_score: Res<HighScore>) {
    let rules_text = [
        "1. Click an empty spot on the grill to drop a patty.",
        "2. The gauge under a patty shows the side touching the grill.",
        "3. Flip it when the gauge reaches green.",
        "4. When it says READY, click it to serve the oldest order.",
        "5. Customers who wait too long and patties that catch fire",
        "   are strikes. Three strikes and you're fired!",
    ];
    commands
        .spawn((
            full_screen(),
            BackgroundColor(Color::srgba(0.05, 0.03, 0.02, 0.7)),
            GlobalZIndex(20),
            DespawnOnExit(GameState::Title),
        ))
        .with_children(|screen| {
            screen.spawn(label("BURGER FLIPPER", 84.0, GOLD));
            screen.spawn(label("Flip 'em. Serve 'em. Don't burn 'em.", 24.0, CREAM));
            screen
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: px(6),
                        padding: UiRect::all(px(18)),
                        margin: UiRect::vertical(px(12)),
                        border_radius: BorderRadius::all(px(12)),
                        ..default()
                    },
                    BackgroundColor(PANEL),
                ))
                .with_children(|panel| {
                    for line in rules_text {
                        panel.spawn((
                            Text::new(line),
                            TextFont::from_font_size(FontSize::Px(18.0)),
                            TextColor(CREAM),
                        ));
                    }
                });
            screen.spawn((label("CLICK TO START", 30.0, CREAM), Blink));
            if high_score.0 > 0 {
                screen.spawn(label(format!("BEST {}", high_score.0), 20.0, GOLD));
            }
        });
}

fn spawn_game_over_screen(
    mut commands: Commands,
    shift: Res<Shift>,
    mut high_score: ResMut<HighScore>,
    mut lock: ResMut<InputLock>,
    mut sfx: MessageWriter<PlaySfx>,
) {
    let new_record = high_score.record(shift.score);
    lock.0 = GAME_OVER_LOCK;
    sfx.write(Sfx::GameOver.into());
    let seconds = shift.elapsed as u32;
    let stats = [
        format!(
            "Patties served: {} ({} perfect)",
            shift.served, shift.perfect
        ),
        format!("Orders filled: {}", shift.orders_filled),
        format!("Best perfect streak: x{}", shift.best_streak),
        format!("Shift length: {}:{:02}", seconds / 60, seconds % 60),
    ];
    commands
        .spawn((
            full_screen(),
            BackgroundColor(Color::srgba(0.1, 0.02, 0.02, 0.78)),
            GlobalZIndex(20),
            DespawnOnExit(GameState::GameOver),
        ))
        .with_children(|screen| {
            screen.spawn(label("YOU'RE FIRED!", 80.0, RED));
            screen.spawn(label(format!("FINAL SCORE {}", shift.score), 40.0, GOLD));
            if new_record {
                screen.spawn(label("NEW RECORD!", 28.0, GREEN));
            } else {
                screen.spawn(label(format!("BEST {}", high_score.0), 22.0, CREAM));
            }
            for line in stats {
                screen.spawn(label(line, 20.0, CREAM));
            }
            screen.spawn((label("CLICK TO TRY AGAIN", 28.0, CREAM), Blink));
        });
}

fn start_shift_on_click(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time<Real>>,
    mut lock: ResMut<InputLock>,
    mut next: ResMut<NextState<GameState>>,
    mut sfx: MessageWriter<PlaySfx>,
) {
    lock.0 -= time.delta_secs();
    if lock.0 > 0.0 {
        return;
    }
    if mouse.just_pressed(MouseButton::Left)
        || keys.any_just_pressed([KeyCode::Space, KeyCode::Enter])
    {
        next.set(GameState::Playing);
        sfx.write(Sfx::Start.into());
    }
}

fn update_score_panel(
    shift: Res<Shift>,
    warmer: Res<Warmer>,
    mut readouts: Query<(&Readout, &mut Text)>,
    mut strikes: Query<(
        &StrikeBox,
        &mut TextColor,
        &mut BackgroundColor,
        &mut BorderColor,
    )>,
) {
    for (readout, mut text) in &mut readouts {
        let value = match readout {
            Readout::Score => shift.score.to_string(),
            Readout::Streak if shift.streak > 1 => format!("PERFECT STREAK x{}", shift.streak),
            Readout::Streak => String::new(),
            Readout::Warmer => format!("WARMER {}/{}", warmer.count, rules::WARMER_CAPACITY),
            Readout::Clock => {
                let seconds = shift.elapsed as u32;
                format!("SHIFT {}:{:02}", seconds / 60, seconds % 60)
            }
        };
        if text.0 != value {
            text.0 = value;
        }
    }
    for (strike, mut color, mut background, mut border) in &mut strikes {
        let struck = strike.0 < shift.strikes;
        color.0 = if struck { Color::WHITE } else { Color::NONE };
        background.0 = if struck { RED } else { Color::NONE };
        *border = BorderColor::all(if struck {
            RED
        } else {
            Color::srgb(0.45, 0.4, 0.36)
        });
    }
}

fn attach_gauges(mut commands: Commands, patties: Query<Entity, Added<Patty>>) {
    let zones = [
        (0.0, rules::DONE_AT, Color::srgb(0.42, 0.36, 0.36)),
        (rules::DONE_AT, rules::OVERDONE_AT, GREEN),
        (rules::OVERDONE_AT, rules::BURNT_AT, ORANGE),
        (
            rules::BURNT_AT,
            rules::GAUGE_MAX,
            Color::srgb(0.75, 0.15, 0.12),
        ),
    ];
    for patty in &patties {
        let track = commands
            .spawn(Node {
                width: px(GAUGE_WIDTH),
                height: px(12),
                ..default()
            })
            .id();
        for (index, (from, to, color)) in zones.into_iter().enumerate() {
            let border_radius = match index {
                0 => BorderRadius::left(px(6)),
                3 => BorderRadius::right(px(6)),
                _ => BorderRadius::ZERO,
            };
            commands.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: percent(from / rules::GAUGE_MAX * 100.0),
                    width: percent((to - from) / rules::GAUGE_MAX * 100.0),
                    height: percent(100),
                    border_radius,
                    ..default()
                },
                BackgroundColor(color),
                ChildOf(track),
            ));
        }
        let marker = commands
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    top: px(-4),
                    width: px(5),
                    height: px(20),
                    margin: UiRect::left(px(-2.5)),
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(2)),
                    ..default()
                },
                BackgroundColor(Color::WHITE),
                BorderColor::all(Color::BLACK),
                ChildOf(track),
            ))
            .id();
        let label = commands.spawn(label("", 16.0, CREAM)).id();
        commands
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: px(GAUGE_WIDTH),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: px(2),
                    ..default()
                },
                Visibility::Hidden,
                Gauge {
                    patty,
                    marker,
                    label,
                },
            ))
            .add_children(&[track, label]);
    }
}

#[allow(clippy::type_complexity)]
fn update_gauges(
    mut commands: Commands,
    time: Res<Time<Real>>,
    camera: Single<(&Camera, &GlobalTransform), With<MainCamera>>,
    patties: Query<&Patty>,
    mut gauges: Query<(Entity, &Gauge, &mut Node, &mut Visibility)>,
    mut markers: Query<&mut Node, Without<Gauge>>,
    mut labels: Query<(&mut Text, &mut TextColor)>,
) {
    let (camera, camera_transform) = *camera;
    for (entity, gauge, mut node, mut visibility) in &mut gauges {
        let patty = match patties.get(gauge.patty) {
            Ok(patty) if !matches!(patty.phase, Phase::Leaving { .. }) => patty,
            _ => {
                commands.entity(entity).despawn();
                continue;
            }
        };
        // Sit just in front of the patty's slot.
        let anchor = SLOTS[patty.slot] + Vec3::Z * (PATTY_RADIUS + 0.12);
        let Ok(position) = camera.world_to_viewport(camera_transform, anchor) else {
            continue;
        };
        node.left = px(position.x - GAUGE_WIDTH / 2.0);
        node.top = px(position.y);
        visibility.set_if_neq(Visibility::Inherited);

        if let Ok(mut marker) = markers.get_mut(gauge.marker) {
            marker.left = percent((patty.down_level() / rules::GAUGE_MAX).min(1.0) * 100.0);
        }
        let (text, color) = match (patty.phase, patty.verdict()) {
            (Phase::OnFire { .. }, _) => ("FIRE!", RED),
            (Phase::Cooking, Verdict::Flip) => ("FLIP!", YELLOW),
            (Phase::Cooking, Verdict::Ready(_)) => ("READY!", GREEN),
            (Phase::Cooking, Verdict::Ruined) => ("BURNT!", RED),
            _ => ("", CREAM),
        };
        if let Ok((mut label, mut label_color)) = labels.get_mut(gauge.label) {
            if label.0 != text {
                label.0 = text.to_string();
            }
            let pulse = 0.7 + 0.3 * (time.elapsed_secs() * 10.0).sin();
            label_color.0 = color.with_alpha(pulse);
        }
    }
}

fn spawn_popups(mut commands: Commands, mut popups: MessageReader<Popup>) {
    for popup in popups.read() {
        commands.spawn((
            label(popup.text.clone(), popup.size, popup.color),
            Node {
                position_type: PositionType::Absolute,
                width: px(POPUP_WIDTH),
                ..default()
            },
            UiTransform::default(),
            GlobalZIndex(10),
            Visibility::Hidden,
            FloatingText {
                anchor: popup.anchor,
                color: popup.color,
                age: 0.0,
            },
        ));
    }
}

#[allow(clippy::type_complexity)]
fn animate_popups(
    mut commands: Commands,
    time: Res<Time<Real>>,
    camera: Single<(&Camera, &GlobalTransform), With<MainCamera>>,
    mut popups: Query<(
        Entity,
        &mut FloatingText,
        &mut Node,
        &mut TextColor,
        &mut TextShadow,
        &mut UiTransform,
        &mut Visibility,
    )>,
) {
    let (camera, camera_transform) = *camera;
    for (entity, mut popup, mut node, mut color, mut shadow, mut transform, mut visibility) in
        &mut popups
    {
        popup.age += time.delta_secs();
        let origin = match popup.anchor {
            Anchor::World(at) => camera.world_to_viewport(camera_transform, at).ok(),
            Anchor::Screen(at) => Some(at),
        };
        let Some(origin) = origin.filter(|_| popup.age < POPUP_LIFETIME) else {
            commands.entity(entity).despawn();
            continue;
        };
        let progress = popup.age / POPUP_LIFETIME;
        node.left = px(origin.x - POPUP_WIDTH / 2.0);
        node.top = px(origin.y - 60.0 - 50.0 * progress);
        // Pop in big, then settle.
        let pop = (1.0 - popup.age / 0.15).max(0.0);
        transform.scale = Vec2::splat(1.0 + 0.5 * pop * pop);
        let alpha = 1.0 - ((progress - 0.6) / 0.4).clamp(0.0, 1.0);
        color.0 = popup.color.with_alpha(alpha);
        shadow.color = shadow.color.with_alpha(0.75 * alpha);
        visibility.set_if_neq(Visibility::Inherited);
    }
}

fn show_pause_screen(
    state: Res<State<GameState>>,
    time: Res<Time<Virtual>>,
    mut screen: Single<&mut Visibility, With<PauseScreen>>,
) {
    let paused = *state.get() == GameState::Playing && time.is_paused();
    screen.set_if_neq(if paused {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    });
}

fn blink(time: Res<Time<Real>>, mut texts: Query<&mut TextColor, With<Blink>>) {
    let alpha = 0.6 + 0.4 * (time.elapsed_secs() * 4.0).sin();
    for mut color in &mut texts {
        color.0 = color.0.with_alpha(alpha);
    }
}
