//! Burger Flipper: a small arcade cooking game built with Bevy.
//!
//! Drop patties on the grill, flip them at the right moment and serve them
//! before the customers run out of patience. Three strikes and you're fired.

mod audio;
mod effects;
mod game;
mod hud;
mod kitchen;
mod orders;
mod patty;
mod rules;

use bevy::prelude::*;

/// Top-level flow of the game.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GameState {
    #[default]
    Title,
    Playing,
    GameOver,
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Burger Flipper".into(),
                resolution: (1280, 720).into(),
                ..default()
            }),
            ..default()
        }))
        .init_state::<GameState>()
        .add_plugins((
            game::plugin,
            audio::plugin,
            effects::plugin,
            kitchen::plugin,
            patty::plugin,
            orders::plugin,
            hud::plugin,
        ))
        .run();
}
