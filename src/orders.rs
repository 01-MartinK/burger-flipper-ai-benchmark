//! Order tickets: posting them on the rail, picking the one to build, and
//! scoring the burger that gets served for it.

use bevy::prelude::*;

use crate::GameState;
use crate::audio::{PlaySfx, Sfx};
use crate::game::{Feedback, Shift, running};
use crate::hud::{GOLD, GREEN, ORANGE, Popup, RED, YELLOW, blend};
use crate::ingredients;
use crate::rules::{Ingredient, Quality, Scorecard};
use crate::stations::{Clickable, Cursor};

/// How long a finished or abandoned ticket stays on the rail.
const LINGER_TIME: f32 = 0.8;
/// Where order messages pop up: just below the ticket rail.
const RAIL_POPUP: Vec2 = Vec2::new(760.0, 215.0);
const TICKET_WIDTH: f32 = 124.0;

const PAPER: Color = Color::srgb(0.98, 0.96, 0.9);
const INK: Color = Color::srgb(0.25, 0.2, 0.17);

pub fn plugin(app: &mut App) {
    app.add_message::<TakeOrder>()
        .add_message::<ServeBurger>()
        .add_message::<OrderClosed>()
        .init_resource::<SelectedOrder>()
        .init_resource::<TicketCount>()
        .add_systems(Startup, spawn_rail)
        .add_systems(OnEnter(GameState::Playing), reset_orders)
        .add_systems(
            Update,
            (
                post_tickets,
                select_ticket,
                serve_burgers,
                wait_for_food,
                clear_closed_tickets,
                style_tickets,
                style_patience_bars,
            )
                .chain()
                .run_if(in_state(GameState::Playing).and_then(running)),
        );
}

/// A customer at the counter has said what they want.
#[derive(Message)]
pub struct TakeOrder {
    pub customer: Entity,
    pub name: &'static str,
    /// Layers of the burger, bottom bun first.
    pub recipe: Vec<Ingredient>,
}

/// The burger on the plate goes out for the selected ticket.
#[derive(Message)]
pub struct ServeBurger {
    /// Layers of the burger, bottom first.
    pub built: Vec<Ingredient>,
    /// How well each grilled layer was cooked.
    pub cooked: Vec<Quality>,
    /// How far off-centre the layers were dropped, 0..=1.
    pub sloppiness: f32,
}

/// An order ended, one way or another; its customer leaves.
#[derive(Message, Clone, Copy)]
pub struct OrderClosed {
    pub customer: Entity,
    pub outcome: Outcome,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Served,
    /// The burger was nothing like the order.
    Rejected,
    WalkedOut,
}

/// A customer's order. Lives on the ticket node that shows it.
#[derive(Component)]
pub struct Order {
    pub id: u32,
    pub name: &'static str,
    pub recipe: Vec<Ingredient>,
    customer: Entity,
    patience: f32,
    max_patience: f32,
    /// Seconds since the ticket was posted.
    age: f32,
    /// How the order ended, and how long ago.
    closed: Option<(Outcome, f32)>,
}

impl Order {
    pub fn is_open(&self) -> bool {
        self.closed.is_none()
    }

    fn patience_left(&self) -> f32 {
        (self.patience / self.max_patience).clamp(0.0, 1.0)
    }

    fn close(&mut self, outcome: Outcome, closed: &mut MessageWriter<OrderClosed>) {
        self.closed = Some((outcome, 0.0));
        closed.write(OrderClosed {
            customer: self.customer,
            outcome,
        });
    }
}

/// The ticket the prep table is working on.
#[derive(Resource, Default)]
pub struct SelectedOrder(pub Option<Entity>);

/// Tickets written this shift.
#[derive(Resource, Default)]
struct TicketCount(u32);

#[derive(Component)]
struct OrderRail;

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
            height: px(150),
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
    mut selected: ResMut<SelectedOrder>,
    mut count: ResMut<TicketCount>,
) {
    for order in &orders {
        commands.entity(order).despawn();
    }
    selected.0 = None;
    count.0 = 0;
}

fn post_tickets(
    mut commands: Commands,
    mut taken: MessageReader<TakeOrder>,
    shift: Res<Shift>,
    mut count: ResMut<TicketCount>,
    rail: Single<Entity, With<OrderRail>>,
    mut sfx: MessageWriter<PlaySfx>,
) {
    for order in taken.read() {
        count.0 += 1;
        let patience = crate::rules::order_patience(shift.elapsed, order.recipe.len());
        spawn_ticket(
            &mut commands,
            *rail,
            Order {
                id: count.0,
                name: order.name,
                recipe: order.recipe.clone(),
                customer: order.customer,
                patience,
                max_patience: patience,
                age: 0.0,
                closed: None,
            },
        );
        sfx.write(Sfx::NewOrder.into());
    }
}

/// Size and corner rounding of a layer drawn on a ticket.
fn ticket_layer(ingredient: Ingredient, lid: bool) -> (f32, f32, BorderRadius) {
    match ingredient {
        Ingredient::Bun if lid => (70.0, 14.0, BorderRadius::top(px(14))),
        Ingredient::Bun => (70.0, 8.0, BorderRadius::bottom(px(5))),
        Ingredient::Patty => (74.0, 10.0, BorderRadius::all(px(4))),
        Ingredient::Cheese => (84.0, 4.0, BorderRadius::all(px(1))),
        Ingredient::Lettuce => (88.0, 5.0, BorderRadius::all(px(3))),
        Ingredient::Tomato => (66.0, 6.0, BorderRadius::all(px(3))),
        Ingredient::Onions => (60.0, 4.0, BorderRadius::all(px(2))),
        Ingredient::Bacon => (92.0, 4.0, BorderRadius::all(px(1))),
    }
}

fn spawn_ticket(commands: &mut Commands, rail: Entity, order: Order) {
    let header = format!("#{} {}", order.id, order.name);
    let recipe = order.recipe.clone();
    let ticket = commands
        .spawn((
            Node {
                width: px(TICKET_WIDTH),
                height: px(128),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                padding: UiRect::all(px(6)),
                row_gap: px(5),
                border: UiRect::all(px(3)),
                border_radius: BorderRadius::all(px(7)),
                ..default()
            },
            BackgroundColor(PAPER),
            BorderColor::all(Color::NONE),
            UiTransform::default(),
            Clickable,
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
        // The burger, drawn lid first so it reads top to bottom.
        ticket_node
            .spawn(Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(1),
                ..default()
            })
            .with_children(|burger| {
                for (index, &ingredient) in recipe.iter().enumerate().rev() {
                    let (width, height, border_radius) = ticket_layer(ingredient, index > 0);
                    burger.spawn((
                        Node {
                            width: px(width),
                            height: px(height),
                            border_radius,
                            ..default()
                        },
                        BackgroundColor(ingredients::color(ingredient)),
                    ));
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

/// Clicking a ticket selects it; otherwise the oldest open ticket is up next.
fn select_ticket(
    mouse: Res<ButtonInput<MouseButton>>,
    cursor: Res<Cursor>,
    orders: Query<(Entity, &Order)>,
    mut selected: ResMut<SelectedOrder>,
    mut sfx: MessageWriter<PlaySfx>,
) {
    let is_open = |entity: Entity| orders.get(entity).is_ok_and(|(_, order)| order.is_open());
    if mouse.just_pressed(MouseButton::Left)
        && let Some(ticket) = cursor.button.filter(|&button| is_open(button))
        && selected.0 != Some(ticket)
    {
        selected.0 = Some(ticket);
        sfx.write(PlaySfx::pitched(Sfx::Flip, 1.3));
    }
    if !selected.0.is_some_and(is_open) {
        let oldest = orders
            .iter()
            .filter(|(_, order)| order.is_open())
            .min_by_key(|(_, order)| order.id)
            .map(|(entity, _)| entity);
        if selected.0 != oldest {
            selected.0 = oldest;
        }
    }
}

fn serve_burgers(
    mut burgers: MessageReader<ServeBurger>,
    selected: Res<SelectedOrder>,
    mut orders: Query<&mut Order>,
    mut shift: ResMut<Shift>,
    mut closed: MessageWriter<OrderClosed>,
    mut feedback: Feedback,
) {
    for burger in burgers.read() {
        let Some(mut order) = selected
            .0
            .and_then(|ticket| orders.get_mut(ticket).ok())
            .filter(|order| order.is_open())
        else {
            continue;
        };
        let card = Scorecard::new(
            order.patience_left(),
            &order.recipe,
            &burger.built,
            &burger.cooked,
            burger.sloppiness,
        );
        if card.is_rejected() {
            order.close(Outcome::Rejected, &mut closed);
            shift.strike();
            feedback.shake.add(0.5);
            feedback.sfx.write(Sfx::Strike.into());
            feedback.popups.write(
                Popup::screen(
                    format!("{}: THAT'S NOT MY ORDER! STRIKE", order.name),
                    RAIL_POPUP,
                    RED,
                )
                .big(),
            );
            continue;
        }
        let points = card.points();
        shift.add_points(points);
        shift.orders_filled += 1;
        order.close(Outcome::Served, &mut closed);
        feedback.sfx.write(Sfx::OrderUp.into());
        let (headline, color) = if card.is_perfect() {
            (format!("PERFECT ORDER FOR {}! +{points}", order.name), GOLD)
        } else {
            (format!("ORDER UP FOR {}! +{points}", order.name), GREEN)
        };
        feedback
            .popups
            .write(Popup::screen(headline, RAIL_POPUP, color));
        let lowest = card.waiting.min(card.grilling).min(card.building);
        feedback.popups.write(
            Popup::screen(
                format!(
                    "WAITING {}   GRILLING {}   BUILDING {}",
                    card.waiting, card.grilling, card.building
                ),
                RAIL_POPUP + Vec2::Y * 40.0,
                match lowest {
                    90.. => GREEN,
                    60.. => YELLOW,
                    _ => ORANGE,
                },
            )
            .small(),
        );
    }
}

fn wait_for_food(
    time: Res<Time>,
    mut orders: Query<&mut Order>,
    mut shift: ResMut<Shift>,
    mut closed: MessageWriter<OrderClosed>,
    mut feedback: Feedback,
) {
    let dt = time.delta_secs();
    for mut order in &mut orders {
        order.age += dt;
        if !order.is_open() {
            continue;
        }
        order.patience -= dt;
        if order.patience <= 0.0 {
            order.patience = 0.0;
            order.close(Outcome::WalkedOut, &mut closed);
            shift.strike();
            feedback.shake.add(0.5);
            feedback.sfx.write(Sfx::Strike.into());
            feedback.popups.write(
                Popup::screen(
                    format!("{} WALKED OUT! STRIKE", order.name),
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

/// Swings new tickets in, frames the selected one, flashes urgent ones and lifts
/// closed ones away.
fn style_tickets(
    selected: Res<SelectedOrder>,
    mut tickets: Query<(
        Entity,
        &Order,
        &mut BackgroundColor,
        &mut BorderColor,
        &mut UiTransform,
    )>,
) {
    for (entity, order, mut background, mut border, mut transform) in &mut tickets {
        let is_selected = selected.0 == Some(entity);
        let drop_in = (1.0 - order.age / 0.3).max(0.0);
        let swing = 8.0 * (-5.0 * order.age).exp() * (12.0 * order.age).cos();
        let mut offset = -60.0 * drop_in * drop_in + if is_selected { 8.0 } else { 0.0 };
        background.0 = match order.closed {
            Some((outcome, since)) => {
                offset -= 140.0 * (since / LINGER_TIME).powi(2);
                match outcome {
                    Outcome::Served => Color::srgb(0.7, 0.95, 0.65),
                    Outcome::Rejected | Outcome::WalkedOut => Color::srgb(1.0, 0.55, 0.5),
                }
            }
            None if order.patience_left() < 0.25 => {
                let pulse = 0.5 + 0.5 * (order.age * 10.0).sin();
                blend(PAPER, Color::srgb(1.0, 0.7, 0.65), pulse)
            }
            None => PAPER,
        };
        *border = BorderColor::all(if is_selected { GOLD } else { Color::NONE });
        transform.translation = Val2::px(0.0, offset);
        transform.rotation = Rot2::degrees(swing);
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
