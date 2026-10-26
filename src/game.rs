//! Shift bookkeeping shared by every system: score, strikes, randomness and pausing.

use std::f32::consts::TAU;
use std::time::{SystemTime, UNIX_EPOCH};

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::GameState;
use crate::audio::PlaySfx;
use crate::effects::{Burst, Shake};
use crate::hud::Popup;
use crate::rules::{self, Quality};

pub fn plugin(app: &mut App) {
    app.init_resource::<Shift>()
        .init_resource::<HighScore>()
        .insert_resource(GameRng::from_time())
        .add_systems(OnEnter(GameState::Playing), start_shift)
        .add_systems(
            Update,
            (
                toggle_pause.run_if(in_state(GameState::Playing)),
                (tick_clock, check_strikes)
                    .chain()
                    .run_if(in_state(GameState::Playing).and_then(running)),
            ),
        );
}

/// Run condition: the game clock is ticking.
pub fn running(time: Res<Time<Virtual>>) -> bool {
    !time.is_paused()
}

/// Score and statistics of the current shift.
#[derive(Resource, Default, Debug)]
pub struct Shift {
    pub score: i32,
    /// Current run of perfect serves.
    pub streak: u32,
    pub best_streak: u32,
    pub strikes: u32,
    pub served: u32,
    pub perfect: u32,
    pub orders_filled: u32,
    pub elapsed: f32,
}

impl Shift {
    pub fn add_points(&mut self, points: i32) {
        self.score = (self.score + points).max(0);
    }

    /// Records a served patty and returns the points it earned.
    pub fn serve(&mut self, quality: Quality) -> i32 {
        self.served += 1;
        if quality == Quality::Perfect {
            self.perfect += 1;
            self.streak += 1;
            self.best_streak = self.best_streak.max(self.streak);
        } else {
            self.streak = 0;
        }
        let points = rules::serve_points(quality, self.streak);
        self.add_points(points);
        points
    }

    /// Records a patty scraped into the bin.
    pub fn trash(&mut self) {
        self.add_points(-rules::TRASH_PENALTY);
        self.streak = 0;
    }

    pub fn strike(&mut self) {
        self.strikes += 1;
        self.streak = 0;
    }
}

/// Everything gameplay systems use to give the player feedback.
#[derive(SystemParam)]
pub struct Feedback<'w> {
    pub sfx: MessageWriter<'w, PlaySfx>,
    pub popups: MessageWriter<'w, Popup>,
    pub bursts: MessageWriter<'w, Burst>,
    pub shake: ResMut<'w, Shake>,
}

/// Best score across shifts of this session.
#[derive(Resource, Default)]
pub struct HighScore(pub i32);

impl HighScore {
    /// Records the score of a finished shift; returns whether it set a new record.
    pub fn record(&mut self, score: i32) -> bool {
        let new_record = score > self.0;
        self.0 = self.0.max(score);
        new_record
    }
}

/// Small, fast xorshift generator; plenty for particles and customers.
#[derive(Resource)]
pub struct GameRng(u64);

impl GameRng {
    fn from_time() -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0x9E37_79B9_7F4A_7C15, |d| d.as_nanos() as u64);
        Self(seed | 1)
    }

    /// Uniform in `[0, 1)`.
    pub fn f32(&mut self) -> f32 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 40) as f32 / (1u64 << 24) as f32
    }

    pub fn range(&mut self, min: f32, max: f32) -> f32 {
        min + (max - min) * self.f32()
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        let index = (self.f32() * items.len() as f32) as usize;
        &items[index.min(items.len() - 1)]
    }

    /// Random unit vector in the plane.
    pub fn direction(&mut self) -> Vec2 {
        Vec2::from_angle(self.range(0.0, TAU))
    }
}

fn start_shift(mut shift: ResMut<Shift>) {
    *shift = Shift::default();
}

fn tick_clock(time: Res<Time>, mut shift: ResMut<Shift>) {
    shift.elapsed += time.delta_secs();
}

fn check_strikes(shift: Res<Shift>, mut next: ResMut<NextState<GameState>>) {
    if shift.strikes >= rules::MAX_STRIKES {
        next.set(GameState::GameOver);
    }
}

fn toggle_pause(keys: Res<ButtonInput<KeyCode>>, mut time: ResMut<Time<Virtual>>) {
    if keys.any_just_pressed([KeyCode::KeyP, KeyCode::Escape]) {
        if time.is_paused() {
            time.unpause();
        } else {
            time.pause();
        }
    }
}
