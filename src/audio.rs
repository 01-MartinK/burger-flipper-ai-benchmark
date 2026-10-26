//! Sound effects synthesized at startup, so the game ships without asset files.

use std::f32::consts::{PI, TAU};
use std::sync::Arc;
use std::time::Duration;

use bevy::audio::{AddAudioSource, ChannelCount, SampleRate, Source, Volume};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;

use crate::GameState;
use crate::game::GameRng;
use crate::patty::Patty;

const SAMPLE_RATE: u32 = 44_100;

pub fn plugin(app: &mut App) {
    app.add_audio_source::<Sound>()
        .add_message::<PlaySfx>()
        .add_systems(Startup, (synthesize_sounds, start_sizzle_loop).chain())
        .add_systems(Update, (play_sfx, update_sizzle));
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Sfx {
    Drop,
    Land,
    Flip,
    Serve,
    Warmer,
    OrderUp,
    NewOrder,
    Trash,
    Fire,
    Strike,
    Nope,
    Start,
    GameOver,
}

/// Request to play a sound effect, optionally pitched up or down.
#[derive(Message, Clone, Copy)]
pub struct PlaySfx {
    pub sfx: Sfx,
    pub pitch: f32,
}

impl PlaySfx {
    pub fn pitched(sfx: Sfx, pitch: f32) -> Self {
        Self { sfx, pitch }
    }
}

impl From<Sfx> for PlaySfx {
    fn from(sfx: Sfx) -> Self {
        Self::pitched(sfx, 1.0)
    }
}

/// A mono buffer of synthesized samples.
#[derive(Asset, TypePath)]
pub struct Sound {
    samples: Arc<[f32]>,
}

pub struct SoundDecoder {
    samples: Arc<[f32]>,
    position: usize,
}

impl Iterator for SoundDecoder {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        let sample = self.samples.get(self.position).copied();
        self.position += 1;
        sample
    }
}

impl Source for SoundDecoder {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> ChannelCount {
        ChannelCount::new(1).unwrap()
    }

    fn sample_rate(&self) -> SampleRate {
        SampleRate::new(SAMPLE_RATE).unwrap()
    }

    fn total_duration(&self) -> Option<Duration> {
        Some(Duration::from_secs_f32(
            self.samples.len() as f32 / SAMPLE_RATE as f32,
        ))
    }
}

impl Decodable for Sound {
    type Decoder = SoundDecoder;

    fn decoder(&self) -> SoundDecoder {
        SoundDecoder {
            samples: self.samples.clone(),
            position: 0,
        }
    }
}

struct Effect {
    sound: Handle<Sound>,
    volume: f32,
    /// Random pitch variation, so repeated sounds don't get monotonous.
    jitter: f32,
}

#[derive(Resource)]
struct SoundBank {
    effects: HashMap<Sfx, Effect>,
    sizzle: Handle<Sound>,
}

/// The looping sizzle that gets louder with more patties on the grill.
#[derive(Component)]
struct SizzleLoop;

fn synthesize_sounds(mut commands: Commands, mut sounds: ResMut<Assets<Sound>>) {
    let mut add = |sfx, samples: Vec<f32>, volume, jitter| {
        let sound = sounds.add(Sound {
            samples: samples.into(),
        });
        (
            sfx,
            Effect {
                sound,
                volume,
                jitter,
            },
        )
    };
    let effects = HashMap::from_iter([
        add(Sfx::Drop, drop_sound(), 0.8, 0.08),
        add(Sfx::Land, land_sound(), 0.6, 0.1),
        add(Sfx::Flip, flip_sound(), 0.5, 0.12),
        add(Sfx::Serve, ding(), 0.45, 0.0),
        add(Sfx::Warmer, clink(), 0.5, 0.06),
        add(Sfx::OrderUp, service_bell(), 0.5, 0.0),
        add(Sfx::NewOrder, ticket_chirp(), 0.35, 0.0),
        add(Sfx::Trash, thud(), 0.7, 0.08),
        add(Sfx::Fire, whoosh(), 0.8, 0.05),
        add(Sfx::Strike, buzzer(), 0.35, 0.0),
        add(Sfx::Nope, nope(), 0.4, 0.0),
        add(Sfx::Start, fanfare(), 0.4, 0.0),
        add(Sfx::GameOver, sad_trombone(), 0.45, 0.0),
    ]);
    let sizzle = sounds.add(Sound {
        samples: sizzle_loop().into(),
    });
    commands.insert_resource(SoundBank { effects, sizzle });
}

fn start_sizzle_loop(mut commands: Commands, bank: Res<SoundBank>) {
    commands.spawn((
        AudioPlayer(bank.sizzle.clone()),
        PlaybackSettings::LOOP.with_volume(Volume::Linear(0.0)),
        SizzleLoop,
    ));
}

fn play_sfx(
    mut commands: Commands,
    mut requests: MessageReader<PlaySfx>,
    bank: Res<SoundBank>,
    mut rng: ResMut<GameRng>,
) {
    for request in requests.read() {
        let Some(effect) = bank.effects.get(&request.sfx) else {
            continue;
        };
        let speed = request.pitch * (1.0 + rng.range(-effect.jitter, effect.jitter));
        commands.spawn((
            AudioPlayer(effect.sound.clone()),
            PlaybackSettings::DESPAWN
                .with_volume(Volume::Linear(effect.volume))
                .with_speed(speed),
        ));
    }
}

fn update_sizzle(
    patties: Query<&Patty>,
    state: Res<State<GameState>>,
    game_time: Res<Time<Virtual>>,
    real_time: Res<Time<Real>>,
    mut sink: Single<&mut AudioSink, With<SizzleLoop>>,
) {
    let on_grill = patties.iter().filter(|patty| patty.is_on_grill()).count() as f32;
    let target = if *state.get() == GameState::Playing && !game_time.is_paused() {
        (on_grill * 0.08).min(0.4)
    } else {
        0.0
    };
    let current = sink.volume().to_linear();
    let blend = (real_time.delta_secs() * 4.0).min(1.0);
    sink.set_volume(Volume::Linear(current + (target - current) * blend));
}

// --- Synthesis -------------------------------------------------------------

/// Deterministic white noise.
struct Noise(u32);

impl Noise {
    /// Uniform in `[-1, 1)`.
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.0 >> 8) as f32 / (1 << 23) as f32 - 1.0
    }
}

/// Renders `seconds` of audio by calling `sample` with the time of every sample.
fn render(seconds: f32, mut sample: impl FnMut(f32) -> f32) -> Vec<f32> {
    let count = (seconds * SAMPLE_RATE as f32) as usize;
    (0..count)
        .map(|i| sample(i as f32 / SAMPLE_RATE as f32).clamp(-1.0, 1.0))
        .collect()
}

fn sine(frequency: f32, t: f32) -> f32 {
    (TAU * frequency * t).sin()
}

/// Linear fade-in over `attack` seconds.
fn attack(t: f32, attack: f32) -> f32 {
    (t / attack).min(1.0)
}

/// Fades out over the last `release` seconds of a sound lasting `length`.
fn release(t: f32, length: f32, release: f32) -> f32 {
    ((length - t) / release).clamp(0.0, 1.0)
}

/// High-passed noise with random crackles: the sound of fat hitting a hot grill.
struct Sizzle {
    noise: Noise,
    low: f32,
    crackle: f32,
    crackle_chance: f32,
}

impl Sizzle {
    fn new(seed: u32, crackle_chance: f32) -> Self {
        Self {
            noise: Noise(seed),
            low: 0.0,
            crackle: 0.0,
            crackle_chance,
        }
    }

    fn next(&mut self) -> f32 {
        let white = self.noise.next();
        self.low += 0.35 * (white - self.low);
        if self.noise.next() * 0.5 + 0.5 < self.crackle_chance {
            self.crackle = 1.0;
        }
        self.crackle *= 0.997;
        (white - self.low) * (0.25 + 0.75 * self.crackle * self.crackle)
    }
}

fn sizzle_loop() -> Vec<f32> {
    let mut sizzle = Sizzle::new(7, 0.0008);
    let length = 3.0;
    render(length, |t| {
        // Fade the loop's ends a touch so the seam doesn't click.
        let seam = attack(t, 0.02) * release(t, length, 0.02);
        sizzle.next() * 0.7 * (0.85 + 0.15 * sine(0.67, t)) * seam
    })
}

/// A dull impact whose pitch drops from `from` to `to` Hz.
fn thump(t: f32, from: f32, to: f32, decay: f32) -> f32 {
    let falloff = 20.0;
    let phase = TAU * (to * t + (from - to) / falloff * (1.0 - (-falloff * t).exp()));
    phase.sin() * (-decay * t).exp()
}

fn drop_sound() -> Vec<f32> {
    let mut sizzle = Sizzle::new(11, 0.004);
    render(0.8, |t| {
        thump(t, 140.0, 60.0, 18.0) * 0.8
            + sizzle.next() * 0.9 * attack(t, 0.015) * (-4.0 * t).exp()
    })
}

fn land_sound() -> Vec<f32> {
    let mut sizzle = Sizzle::new(23, 0.004);
    render(0.55, |t| {
        thump(t, 120.0, 70.0, 24.0) * 0.5 + sizzle.next() * 0.8 * attack(t, 0.01) * (-6.0 * t).exp()
    })
}

fn flip_sound() -> Vec<f32> {
    let mut noise = Noise(5);
    let mut low = 0.0;
    let length = 0.3;
    render(length, |t| {
        let progress = t / length;
        // Low-passed noise whose cutoff rises: a spatula swoosh.
        low += (0.02 + 0.2 * progress) * (noise.next() - low);
        let swoosh = low * 2.5 * (PI * progress).sin().powi(2);
        let scrape = sine(2400.0, t) * (-60.0 * t).exp() * 0.2;
        swoosh + scrape
    })
}

fn ding() -> Vec<f32> {
    render(0.7, |t| {
        let tone = sine(1318.5, t) * (-6.0 * t).exp()
            + sine(2637.0, t) * 0.35 * (-9.0 * t).exp()
            + sine(3955.5, t) * 0.12 * (-14.0 * t).exp();
        tone * attack(t, 0.003) * 0.6
    })
}

fn clink() -> Vec<f32> {
    render(0.3, |t| {
        (sine(1800.0, t) * (-25.0 * t).exp() + sine(2710.0, t) * 0.6 * (-32.0 * t).exp())
            * attack(t, 0.002)
            * 0.5
    })
}

/// A struck bell with slightly inharmonic overtones.
fn bell(frequency: f32, t: f32) -> f32 {
    if t < 0.0 {
        return 0.0;
    }
    (sine(frequency, t) * (-3.5 * t).exp()
        + sine(frequency * 2.76, t) * 0.45 * (-6.0 * t).exp()
        + sine(frequency * 5.4, t) * 0.2 * (-10.0 * t).exp())
        * attack(t, 0.002)
}

fn service_bell() -> Vec<f32> {
    render(1.2, |t| (bell(1568.0, t) + bell(2093.0, t - 0.14)) * 0.4)
}

fn ticket_chirp() -> Vec<f32> {
    let blip = |frequency: f32, t: f32, length: f32| {
        if (0.0..length).contains(&t) {
            sine(frequency, t) * attack(t, 0.005) * release(t, length, 0.02)
        } else {
            0.0
        }
    };
    render(0.25, |t| {
        (blip(1046.5, t, 0.07) + blip(1568.0, t - 0.09, 0.12)) * 0.5
    })
}

fn thud() -> Vec<f32> {
    let mut noise = Noise(31);
    let mut low = 0.0;
    render(0.5, |t| {
        low += 0.08 * (noise.next() - low);
        thump(t, 110.0, 50.0, 14.0) * 0.9 + low * 1.5 * (-12.0 * t).exp()
    })
}

fn whoosh() -> Vec<f32> {
    let mut noise = Noise(43);
    let mut sizzle = Sizzle::new(47, 0.01);
    let mut low = 0.0;
    render(1.2, |t| {
        low += 0.06 * (noise.next() - low);
        let body = low * 4.0 * (1.0 - (-20.0 * t).exp()) * (-2.5 * t).exp();
        body + sizzle.next() * 0.4 * (-3.0 * t).exp()
    })
}

fn buzzer() -> Vec<f32> {
    let length = 0.55;
    render(length, |t| {
        let square = |frequency: f32| (4.0 * sine(frequency, t)).tanh();
        (square(146.8) + square(151.0)) * 0.3 * attack(t, 0.01) * release(t, length, 0.08)
    })
}

fn nope() -> Vec<f32> {
    let blip = |frequency: f32, t: f32| {
        if (0.0..0.1).contains(&t) {
            (3.0 * sine(frequency, t)).tanh() * attack(t, 0.005) * release(t, 0.1, 0.03)
        } else {
            0.0
        }
    };
    render(0.3, |t| (blip(311.0, t) + blip(233.0, t - 0.13)) * 0.35)
}

/// A plucked note with a few harmonics.
fn pluck(frequency: f32, t: f32) -> f32 {
    if t < 0.0 {
        return 0.0;
    }
    (sine(frequency, t) + sine(frequency * 2.0, t) * 0.3 + sine(frequency * 3.0, t) * 0.1)
        * (-7.0 * t).exp()
        * attack(t, 0.004)
}

fn fanfare() -> Vec<f32> {
    let notes = [523.3, 659.3, 784.0, 1046.5];
    render(0.9, |t| {
        notes
            .iter()
            .enumerate()
            .map(|(i, &frequency)| pluck(frequency, t - i as f32 * 0.08))
            .sum::<f32>()
            * 0.3
    })
}

fn sad_trombone() -> Vec<f32> {
    // (start, length, frequency) of each "wah".
    let notes = [
        (0.0, 0.38, 392.0),
        (0.42, 0.38, 370.0),
        (0.84, 0.38, 349.2),
        (1.26, 1.0, 329.6),
    ];
    let length = 2.4;
    let mut phase = 0.0;
    render(length, |t| {
        let Some(&(start, duration, frequency)) = notes
            .iter()
            .find(|(start, duration, _)| (*start..start + duration).contains(&t))
        else {
            return 0.0;
        };
        let local = t - start;
        let vibrato = if duration > 0.5 {
            1.0 + 0.015 * sine(6.0, local) * attack(local, 0.3)
        } else {
            1.0
        };
        phase += TAU * frequency * vibrato / SAMPLE_RATE as f32;
        // A brassy tone from a handful of harmonics.
        let tone: f32 = (1..=6)
            .map(|k| (phase * k as f32).sin() * 0.75_f32.powi(k) / k as f32)
            .sum();
        tone * attack(local, 0.03) * release(local, duration, 0.08) * 0.9
    })
}
