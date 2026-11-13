//! Burger Flipper: a small restaurant game built with Bevy.
//!
//! Take orders at the counter, stock up in storage, grill patties and bacon,
//! and stack burgers at the prep table before the customers run out of
//! patience. Three strikes and you're fired.

mod audio;
mod counter;
mod effects;
mod game;
mod hud;
mod ingredients;
mod kitchen;
mod orders;
mod patty;
mod prep;
mod rules;
mod stations;
mod storage;

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
            ingredients::plugin,
            stations::plugin,
            kitchen::plugin,
            patty::plugin,
            orders::plugin,
            counter::plugin,
            storage::plugin,
            prep::plugin,
            hud::plugin,
        ))
        .run();
}
