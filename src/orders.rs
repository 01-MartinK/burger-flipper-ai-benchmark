//! Customers and their order tickets, plus the warmer that holds spare patties.

use bevy::prelude::*;

use crate::GameState;
use crate::audio::Sfx;
use crate::game::{Feedback, GameRng, Shift, running};
use crate::hud::{GREEN, Popup, RED, YELLOW, blend};
use crate::rules;

const CUSTOMERS: [&str; 24] = [
    "ANN", "BOB", "CHEN", "DANA", "ELI", "FAY", "GUS", "HAL", "IVY", "JO", "KAI", "LEO", "MAX",
    "NIA", "OTTO", "PIA", "RAY", "SUE", "TOM", "UMA", "VIC", "WES", "YUKI", "ZOE",
];

/// How long a finished or abandoned ticket stays on the rail.
const LINGER_TIME: f32 = 0.8;
/// Where order messages pop up: just below the ticket rail.
const RAIL_POPUP: Vec2 = Vec2::new(760.0, 180.0);
const TICKET_WIDTH: f32 = 132.0;

const PAPER: Color = Color::srgb(0.98, 0.96, 0.9);
const INK: Color = Color::srgb(0.25, 0.2, 0.17);
const BUN: Color = Color::srgb(0.87, 0.6, 0.27);
const LETTUCE: Color = Color::srgb(0.45, 0.75, 0.25);
const MEAT: Color = Color::srgb(0.42, 0.22, 0.11);

pub fn plugin(app: &mut App) {
    app.init_resource::<Warmer>()
        .init_resource::<OrderClock>()
        .add_systems(Startup, spawn_rail)
        .add_systems(OnEnter(GameState::Playing), reset_orders)
        .add_systems(
            Update,
            (
                spawn_orders,
                fill_from_warmer,
                wait_for_food,
                clear_closed_tickets,
                style_tickets,
                style_burger_icons,
                style_patience_bars,
            )
                .chain()
                .run_if(in_state(GameState::Playing).and_then(running)),
        );
}

/// A customer's order. Lives on the ticket node that shows it.
#[derive(Component)]
pub struct Order {
    id: u32,
    customer: &'static str,
    needed: u32,
    filled: u32,
    patience: f32,
    max_patience: f32,
    /// Seconds since the ticket was posted.
    age: f32,
    /// How the order ended, and how long ago.
    closed: Option<(Outcome, f32)>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Served,
    WalkedOut,
}

impl Order {
    fn is_open(&self) -> bool {
        self.closed.is_none() && self.filled < self.needed
    }

    fn patience_left(&self) -> f32 {
        (self.patience / self.max_patience).clamp(0.0, 1.0)
    }
}

/// Spare patties kept warm until the next order comes in.
#[derive(Resource, Default)]
pub struct Warmer {
    pub count: u32,
}

/// Where a served patty goes.
pub enum Claim {
    Order,
    /// Into the warmer, at this spot on the tray.
    Warmer(usize),
}

/// Hands a freshly served patty to the oldest open order, or puts it in the warmer.
/// Returns `None` when nobody wants it and the warmer is full.
pub fn claim(orders: &mut Query<&mut Order>, warmer: &mut Warmer) -> Option<Claim> {
    if let Some(mut order) = orders
        .iter_mut()
        .filter(|order| order.is_open())
        .min_by_key(|order| order.id)
    {
        order.filled += 1;
        return Some(Claim::Order);
    }
    if warmer.count < rules::WARMER_CAPACITY {
        warmer.count += 1;
        return Some(Claim::Warmer(warmer.count as usize - 1));
    }
    None
}

#[derive(Resource, Default)]
struct OrderClock {
    until_next: f32,
    next_id: u32,
}

#[derive(Component)]
struct OrderRail;

/// One piece (bun, lettuce or patty) of a burger icon on a ticket.
#[derive(Component)]
struct BurgerPiece {
    order: Entity,
    /// Which burger of the order this piece belongs to.
    index: u32,
    color: Color,
}

#[derive(Component)]
struct PatienceBar {
    order: Entity,
}

fn spawn_rail(mut commands: Commands) {
    commands.spawn((
        OrderRail,
        Node {
            position_type: PositionType::Absolute,
            top: px(0),
            left: px(264),
            right: px(12),
            height: px(140),
            padding: UiRect::top(px(16)),
            column_gap: px(10),
            align_items: AlignItems::FlexStart,
            ..default()
        },
        children![(
            // The steel bar the tickets hang from.
            Node {
                position_type: PositionType::Absolute,
                top: px(6),
                left: px(0),
                right: px(0),
                height: px(8),
                border_radius: BorderRadius::all(px(4)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.62, 0.64, 0.68)),
        )],
    ));
}

fn reset_orders(
    mut commands: Commands,
    orders: Query<Entity, With<Order>>,
    mut warmer: ResMut<Warmer>,
    mut clock: ResMut<OrderClock>,
) {
    for order in &orders {
        commands.entity(order).despawn();
    }
    *warmer = Warmer::default();
    *clock = OrderClock {
        until_next: 0.5,
        next_id: 0,
    };
}

#[allow(clippy::too_many_arguments)]
fn spawn_orders(
    mut commands: Commands,
    time: Res<Time>,
    shift: Res<Shift>,
    mut clock: ResMut<OrderClock>,
    orders: Query<&Order>,
    rail: Single<Entity, With<OrderRail>>,
    mut rng: ResMut<GameRng>,
    mut feedback: Feedback,
) {
    clock.until_next -= time.delta_secs();
    let waiting = orders.iter().filter(|order| order.closed.is_none()).count();
    if clock.until_next > 0.0 || waiting >= rules::MAX_OPEN_ORDERS {
        return;
    }
    clock.until_next = rules::order_interval(shift.elapsed) * rng.range(0.8, 1.2);
    clock.next_id += 1;
    let needed = rules::order_size(shift.elapsed, rng.f32());
    let patience = rules::order_patience(shift.elapsed, needed);
    let order = Order {
        id: clock.next_id,
        customer: rng.pick(&CUSTOMERS),
        needed,
        filled: 0,
        patience,
        max_patience: patience,
        age: 0.0,
        closed: None,
    };
    spawn_ticket(&mut commands, *rail, order);
    feedback.sfx.write(Sfx::NewOrder.into());
}

fn spawn_ticket(commands: &mut Commands, rail: Entity, order: Order) {
    let header = format!("#{} {}", order.id, order.customer);
    let needed = order.needed;
    let ticket = commands
        .spawn((
            Node {
                width: px(TICKET_WIDTH),
                height: px(108),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(8)),
                row_gap: px(8),
                border_radius: BorderRadius::all(px(6)),
                ..default()
            },
            BackgroundColor(PAPER),
            UiTransform::default(),
            order,
            ChildOf(rail),
        ))
        .id();
    commands.entity(ticket).with_children(|ticket_node| {
        ticket_node.spawn((
            Text::new(header),
            TextFont::from_font_size(FontSize::Px(15.0)),
            TextColor(INK),
        ));
        ticket_node
            .spawn(Node {
                column_gap: px(4),
                ..default()
            })
            .with_children(|icons| {
                for index in 0..needed {
                    icons
                        .spawn(Node {
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::Center,
                            row_gap: px(1),
                            ..default()
                        })
                        .with_children(|burger| {
                            let pieces = [
                                (32.0, 13.0, BorderRadius::top(px(16)), BUN),
                                (36.0, 4.0, BorderRadius::all(px(2)), LETTUCE),
                                (32.0, 9.0, BorderRadius::all(px(3)), MEAT),
                                (32.0, 7.0, BorderRadius::bottom(px(4)), BUN),
                            ];
                            for (width, height, border_radius, color) in pieces {
                                burger.spawn((
                                    Node {
                                        width: px(width),
                                        height: px(height),
                                        border_radius,
                                        ..default()
                                    },
                                    BackgroundColor(color),
                                    BurgerPiece {
                                        order: ticket,
                                        index,
                                        color,
                                    },
                                ));
                            }
                        });
                }
            });
        // Patience bar, pinned to the bottom of the ticket.
        ticket_node
            .spawn((
                Node {
                    width: percent(100),
                    height: px(8),
                    margin: UiRect::top(auto()),
                    border_radius: BorderRadius::all(px(4)),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.85, 0.82, 0.75)),
            ))
            .with_child((
                Node {
                    width: percent(100),
                    height: percent(100),
                    border_radius: BorderRadius::all(px(4)),
                    ..default()
                },
                BackgroundColor(GREEN),
                PatienceBar { order: ticket },
            ));
    });
}

fn fill_from_warmer(
    mut warmer: ResMut<Warmer>,
    mut orders: Query<&mut Order>,
    mut feedback: Feedback,
) {
    while warmer.count > 0 {
        let Some(mut order) = orders
            .iter_mut()
            .filter(|order| order.is_open())
            .min_by_key(|order| order.id)
        else {
            break;
        };
        order.filled += 1;
        warmer.count -= 1;
        feedback.sfx.write(Sfx::Warmer.into());
    }
}

fn wait_for_food(
    time: Res<Time>,
    mut orders: Query<&mut Order>,
    mut shift: ResMut<Shift>,
    mut feedback: Feedback,
) {
    let dt = time.delta_secs();
    for mut order in &mut orders {
        order.age += dt;
        if order.closed.is_some() {
            continue;
        }
        if order.filled >= order.needed {
            let bonus = rules::order_bonus(order.needed, order.patience_left());
            shift.add_points(bonus);
            shift.orders_filled += 1;
            order.closed = Some((Outcome::Served, 0.0));
            feedback.sfx.write(Sfx::OrderUp.into());
            feedback.popups.write(Popup::screen(
                format!("ORDER UP FOR {}! +{bonus}", order.customer),
                RAIL_POPUP,
                GREEN,
            ));
            continue;
        }
        order.patience -= dt;
        if order.patience <= 0.0 {
            order.patience = 0.0;
            order.closed = Some((Outcome::WalkedOut, 0.0));
            shift.strike();
            feedback.shake.add(0.5);
            feedback.sfx.write(Sfx::Strike.into());
            feedback.popups.write(
                Popup::screen(
                    format!("{} WALKED OUT! STRIKE", order.customer),
                    RAIL_POPUP,
                    RED,
                )
                .big(),
            );
        }
    }
}

fn clear_closed_tickets(
    mut commands: Commands,
    time: Res<Time>,
    mut orders: Query<(Entity, &mut Order)>,
) {
    for (entity, mut order) in &mut orders {
        let Some((outcome, since)) = order.closed else {
            continue;
        };
        let since = since + time.delta_secs();
        if since >= LINGER_TIME {
            commands.entity(entity).despawn();
        } else {
            order.closed = Some((outcome, since));
        }
    }
}

/// Swings new tickets in, flashes urgent ones and lifts closed ones away.
fn style_tickets(mut tickets: Query<(&Order, &mut BackgroundColor, &mut UiTransform)>) {
    for (order, mut background, mut transform) in &mut tickets {
        let drop_in = (1.0 - order.age / 0.3).max(0.0);
        let swing = 8.0 * (-5.0 * order.age).exp() * (12.0 * order.age).cos();
        let mut offset = -60.0 * drop_in * drop_in;
        background.0 = match order.closed {
            Some((outcome, since)) => {
                offset -= 120.0 * (since / LINGER_TIME).powi(2);
                match outcome {
                    Outcome::Served => Color::srgb(0.7, 0.95, 0.65),
                    Outcome::WalkedOut => Color::srgb(1.0, 0.55, 0.5),
                }
            }
            None if order.patience_left() < 0.25 => {
                let pulse = 0.5 + 0.5 * (order.age * 10.0).sin();
                blend(PAPER, Color::srgb(1.0, 0.7, 0.65), pulse)
            }
            None => PAPER,
        };
        transform.translation = Val2::px(0.0, offset);
        transform.rotation = Rot2::degrees(swing);
    }
}

fn style_burger_icons(
    orders: Query<&Order>,
    mut pieces: Query<(&BurgerPiece, &mut BackgroundColor)>,
) {
    for (piece, mut background) in &mut pieces {
        let Ok(order) = orders.get(piece.order) else {
            continue;
        };
        // Burgers still to make are drawn faded.
        let alpha = if piece.index < order.filled {
            1.0
        } else {
            0.22
        };
        background.0 = piece.color.with_alpha(alpha);
    }
}

fn style_patience_bars(
    orders: Query<&Order>,
    mut bars: Query<(&PatienceBar, &mut Node, &mut BackgroundColor)>,
) {
    for (bar, mut node, mut background) in &mut bars {
        let Ok(order) = orders.get(bar.order) else {
            continue;
        };
        let left = order.patience_left();
        node.width = percent(left * 100.0);
        background.0 = if left > 0.5 {
            blend(YELLOW, GREEN, (left - 0.5) * 2.0)
        } else {
            blend(RED, YELLOW, left * 2.0)
        };
    }
}
