//! The four stations of the restaurant and getting between them.
//!
//! Every station is its own 3D set, spaced out along the X axis. One camera pans
//! between them, so the grill keeps cooking while the player is somewhere else.

use bevy::input::InputSystems;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::GameState;
use crate::audio::{PlaySfx, Sfx};
use crate::effects::CameraRig;
use crate::game::running;
use crate::hud::{CREAM, ORANGE, blend, label};
use crate::kitchen::MainCamera;

/// Seconds the camera takes to pan to another station.
const PAN_TIME: f32 = 0.45;
/// How far through a pan the new station starts taking clicks.
const SETTLED: f32 = 0.75;

const TAB_IDLE: Color = Color::srgba(0.16, 0.1, 0.07, 0.92);
const TAB_ACTIVE: Color = Color::srgb(0.72, 0.36, 0.1);

pub fn plugin(app: &mut App) {
    app.init_resource::<Station>()
        .init_resource::<Cursor>()
        .init_resource::<Alerts>()
        .add_systems(Startup, spawn_tab_bar)
        .add_systems(OnEnter(GameState::Playing), open_counter)
        .add_systems(PreUpdate, track_cursor.after(InputSystems))
        .add_systems(
            Update,
            (
                switch_station.run_if(in_state(GameState::Playing).and_then(running)),
                pan_camera,
                style_tabs,
                ease_hover,
            )
                .chain(),
        );
}

/// A station of the restaurant, and the camera view that shows it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum View {
    Counter,
    Storage,
    #[default]
    Grill,
    Prep,
}

impl View {
    pub const ALL: [Self; 4] = [Self::Counter, Self::Storage, Self::Grill, Self::Prep];

    const fn index(self) -> usize {
        self as usize
    }

    const fn title(self) -> &'static str {
        match self {
            Self::Counter => "1  COUNTER",
            Self::Storage => "2  STORAGE",
            Self::Grill => "3  GRILLING",
            Self::Prep => "4  PREP TABLE",
        }
    }

    const fn hint(self) -> &'static str {
        match self {
            Self::Counter => "CLICK A CUSTOMER IN LINE TO TAKE THEIR ORDER",
            Self::Storage => {
                "CLICK A CRATE TO LOAD A PACK ONTO THE TRAYS   |   RAW PATTIES AND BACON GO TO THE GRILL"
            }
            Self::Grill => {
                "CLICK A RAW TRAY TO PICK PATTY OR BACON   |   CLICK THE GRILL TO DROP IT   |   CLICK FOOD TO FLIP IT, OR TO SEND IT TO PREP WHEN READY"
            }
            Self::Prep => {
                "DRAG INGREDIENTS FROM THE TRAYS ONTO THE PLATE, BOTTOM BUN FIRST   |   RING THE BELL TO SERVE THE SELECTED TICKET"
            }
        }
    }

    /// Where the station's set is built.
    pub const fn origin(self) -> Vec3 {
        match self {
            Self::Counter => Vec3::new(-90.0, 0.0, 0.0),
            Self::Storage => Vec3::new(-45.0, 0.0, 0.0),
            Self::Grill => Vec3::ZERO,
            Self::Prep => Vec3::new(45.0, 0.0, 0.0),
        }
    }

    pub fn camera(self) -> Transform {
        let (eye, target) = match self {
            Self::Counter => (Vec3::new(0.0, 5.5, 10.5), Vec3::new(0.0, 0.8, -2.0)),
            Self::Storage => (Vec3::new(-1.5, 4.5, 14.0), Vec3::new(-1.5, 2.0, 0.0)),
            Self::Grill => (Vec3::new(0.0, 8.4, 6.5), Vec3::new(0.0, -0.7, -1.2)),
            Self::Prep => (Vec3::new(0.0, 8.6, 7.0), Vec3::new(0.0, -0.3, -0.6)),
        };
        Transform::from_translation(self.origin() + eye).looking_at(self.origin() + target, Vec3::Y)
    }
}

/// Which station the player is at, and the camera pan towards it.
#[derive(Resource)]
pub struct Station {
    current: View,
    from: Transform,
    pose: Transform,
    progress: f32,
}

impl Default for Station {
    fn default() -> Self {
        let pose = View::default().camera();
        Self {
            current: View::default(),
            from: pose,
            pose,
            progress: 1.0,
        }
    }
}

impl Station {
    /// Whether the player is at this station and the camera has (nearly) arrived.
    pub fn is_at(&self, view: View) -> bool {
        self.current == view && self.progress >= SETTLED
    }

    fn go(&mut self, view: View) {
        if self.current != view {
            self.current = view;
            self.from = self.pose;
            self.progress = 0.0;
        }
    }
}

/// Stations that want the player's attention; their tabs flash.
#[derive(Resource, Default)]
pub struct Alerts([bool; 4]);

impl Alerts {
    pub fn set(&mut self, view: View, alert: bool) {
        self.0[view.index()] = alert;
    }
}

/// Where the mouse points: at a button of the interface, or into the 3D scene.
#[derive(Resource, Default)]
pub struct Cursor {
    ray: Option<Ray3d>,
    /// The [`Clickable`] interface node under the cursor.
    pub button: Option<Entity>,
}

impl Cursor {
    /// The ray from the camera through the cursor, unless a button is in the way.
    pub fn ray(&self) -> Option<Ray3d> {
        self.ray.filter(|_| self.button.is_none())
    }

    /// The point under the cursor on a horizontal plane at this height.
    pub fn on_plane(&self, height: f32) -> Option<Vec3> {
        self.ray()?
            .plane_intersection_point(Vec3::Y * height, InfinitePlane3d::new(Vec3::Y))
    }
}

/// Distance along the ray at which it enters a box, if it does.
pub fn ray_hits_box(ray: Ray3d, centre: Vec3, half_size: Vec3) -> Option<f32> {
    let direction = *ray.direction;
    let mut enter = 0.0_f32;
    let mut exit = f32::INFINITY;
    for axis in 0..3 {
        let start = ray.origin[axis] - centre[axis];
        if direction[axis].abs() < 1e-6 {
            if start.abs() > half_size[axis] {
                return None;
            }
            continue;
        }
        let a = (-half_size[axis] - start) / direction[axis];
        let b = (half_size[axis] - start) / direction[axis];
        enter = enter.max(a.min(b));
        exit = exit.min(a.max(b));
    }
    (enter <= exit).then_some(enter)
}

/// How far the ray passes from a point in front of it.
pub fn ray_distance(ray: Ray3d, point: Vec3) -> f32 {
    let direction = *ray.direction;
    let along = (point - ray.origin).dot(direction);
    if along <= 0.0 {
        return f32::INFINITY;
    }
    (ray.origin + direction * along).distance(point)
}

/// An interface node that takes mouse clicks; whatever is behind it in the 3D scene doesn't.
#[derive(Component)]
pub struct Clickable;

/// Grows a little while `hot`, to show the cursor is on it.
#[derive(Component, Default)]
pub struct Hoverable {
    pub hot: bool,
}

#[derive(Component)]
struct TabBar;

#[derive(Component)]
struct Tab(View);

#[derive(Component)]
struct Hint;

fn spawn_tab_bar(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                bottom: px(10),
                width: percent(100),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(6),
                ..default()
            },
            GlobalZIndex(10),
            Visibility::Hidden,
            TabBar,
        ))
        .with_children(|bar| {
            bar.spawn((label("", 13.0, CREAM.with_alpha(0.85)), Hint));
            bar.spawn(Node {
                column_gap: px(10),
                ..default()
            })
            .with_children(|tabs| {
                for view in View::ALL {
                    tabs.spawn((
                        Node {
                            width: px(176),
                            height: px(42),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            border: UiRect::all(px(2)),
                            border_radius: BorderRadius::all(px(9)),
                            ..default()
                        },
                        BackgroundColor(TAB_IDLE),
                        BorderColor::all(CREAM.with_alpha(0.3)),
                        Clickable,
                        Tab(view),
                    ))
                    .with_child(label(view.title(), 17.0, CREAM));
                }
            });
        });
}

/// Every shift starts at the counter, where the first customer is about to walk in.
fn open_counter(mut station: ResMut<Station>) {
    station.go(View::Counter);
}

pub fn track_cursor(
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform), With<MainCamera>>,
    buttons: Query<
        (
            Entity,
            &ComputedNode,
            &UiGlobalTransform,
            &InheritedVisibility,
        ),
        With<Clickable>,
    >,
    mut cursor: ResMut<Cursor>,
) {
    let (camera, camera_transform) = *camera;
    cursor.ray = window
        .cursor_position()
        .and_then(|position| camera.viewport_to_world(camera_transform, position).ok());
    cursor.button = window.physical_cursor_position().and_then(|position| {
        buttons
            .iter()
            .find(|(_, node, transform, visibility)| {
                visibility.get() && node.contains_point(**transform, position)
            })
            .map(|(entity, ..)| entity)
    });
}

fn switch_station(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    cursor: Res<Cursor>,
    tabs: Query<&Tab>,
    mut station: ResMut<Station>,
    mut sfx: MessageWriter<PlaySfx>,
) {
    let digits = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
    ];
    let mut wanted = View::ALL
        .into_iter()
        .zip(digits)
        .find(|(_, key)| keys.just_pressed(*key))
        .map(|(view, _)| view);
    if mouse.just_pressed(MouseButton::Left)
        && let Some(tab) = cursor.button.and_then(|button| tabs.get(button).ok())
    {
        wanted = Some(tab.0);
    }
    if let Some(view) = wanted.filter(|view| *view != station.current) {
        station.go(view);
        sfx.write(PlaySfx::pitched(Sfx::Flip, 0.7));
    }
}

fn pan_camera(
    time: Res<Time<Real>>,
    mut station: ResMut<Station>,
    mut rig: Single<&mut CameraRig>,
) {
    if station.progress < 1.0 {
        station.progress = (station.progress + time.delta_secs() / PAN_TIME).min(1.0);
    }
    let t = station.progress;
    let eased = t * t * (3.0 - 2.0 * t);
    let to = station.current.camera();
    let pose = Transform {
        translation: station.from.translation.lerp(to.translation, eased),
        rotation: station.from.rotation.slerp(to.rotation, eased),
        scale: Vec3::ONE,
    };
    if station.pose != pose {
        station.pose = pose;
    }
    if rig.base != pose {
        rig.base = pose;
    }
}

fn style_tabs(
    state: Res<State<GameState>>,
    time: Res<Time<Real>>,
    station: Res<Station>,
    alerts: Res<Alerts>,
    mut bar: Single<&mut Visibility, With<TabBar>>,
    mut hint: Single<&mut Text, With<Hint>>,
    mut tabs: Query<(&Tab, &mut BackgroundColor, &mut BorderColor)>,
) {
    bar.set_if_neq(if *state.get() == GameState::Playing {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    });
    if hint.0 != station.current.hint() {
        hint.0 = station.current.hint().to_string();
    }
    let pulse = 0.5 + 0.5 * (time.elapsed_secs() * 7.0).sin();
    for (tab, mut background, mut border) in &mut tabs {
        let active = tab.0 == station.current;
        background.0 = if active {
            TAB_ACTIVE
        } else if alerts.0[tab.0.index()] {
            blend(TAB_IDLE, ORANGE, pulse)
        } else {
            TAB_IDLE
        };
        *border = BorderColor::all(CREAM.with_alpha(if active { 0.9 } else { 0.3 }));
    }
}

fn ease_hover(time: Res<Time<Real>>, mut hoverables: Query<(&Hoverable, &mut Transform)>) {
    let ease = 1.0 - (-18.0 * time.delta_secs()).exp();
    for (hoverable, mut transform) in &mut hoverables {
        let target = if hoverable.hot { 1.08 } else { 1.0 };
        let scale = transform.scale.x;
        if (scale - target).abs() > 0.001 {
            transform.scale = Vec3::splat(scale + (target - scale) * ease);
        }
    }
}
