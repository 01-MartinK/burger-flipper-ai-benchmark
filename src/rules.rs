//! Tuning constants and pure gameplay rules.
//!
//! Nothing in here touches the ECS, so the rules can be unit tested directly.

/// Cook level gained per second by the side touching the grill.
pub const COOK_RATE: f32 = 0.2;
/// A side is cooked once it reaches this level...
pub const DONE_AT: f32 = 0.7;
/// ...stays perfect until this level...
pub const OVERDONE_AT: f32 = 1.0;
/// ...and is ruined from here on.
pub const BURNT_AT: f32 = 1.3;
/// A neglected patty bursts into flames at this level.
pub const IGNITES_AT: f32 = 1.8;
/// Highest cook level shown on a patty's gauge.
pub const GAUGE_MAX: f32 = 1.5;

pub const MAX_STRIKES: u32 = 3;
pub const MAX_STREAK: u32 = 5;
pub const MAX_OPEN_ORDERS: usize = 6;
pub const WARMER_CAPACITY: u32 = 4;
pub const TRASH_PENALTY: i32 = 25;

/// How cooked a single side of a patty is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Doneness {
    Raw,
    Perfect,
    Overdone,
    Burnt,
}

impl Doneness {
    pub fn of(level: f32) -> Self {
        if level < DONE_AT {
            Self::Raw
        } else if level < OVERDONE_AT {
            Self::Perfect
        } else if level < BURNT_AT {
            Self::Overdone
        } else {
            Self::Burnt
        }
    }
}

/// How good a served patty is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quality {
    /// Both sides perfect.
    Perfect,
    /// One side a little overdone.
    Good,
    /// Both sides overdone.
    Dry,
}

/// What a click on a patty sitting on the grill should do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// The side on the grill still needs time.
    Cooking,
    /// The side on the grill is done but the top is still raw: flip it!
    Flip,
    /// Both sides are done: serve it.
    Ready(Quality),
    /// One side is burnt: only good for the bin.
    Ruined,
}

/// Judges a patty from the cook levels of the side on the grill and the side facing up.
pub fn judge(down: f32, up: f32) -> Verdict {
    use Doneness::*;
    match (Doneness::of(down), Doneness::of(up)) {
        (Burnt, _) | (_, Burnt) => Verdict::Ruined,
        (Raw, _) => Verdict::Cooking,
        (_, Raw) => Verdict::Flip,
        (Perfect, Perfect) => Verdict::Ready(Quality::Perfect),
        (Overdone, Overdone) => Verdict::Ready(Quality::Dry),
        _ => Verdict::Ready(Quality::Good),
    }
}

/// Points for serving a patty. `streak` is the run of perfect serves including this one;
/// every perfect patty in a row is worth 50 more than the last, up to `MAX_STREAK`.
pub fn serve_points(quality: Quality, streak: u32) -> i32 {
    match quality {
        Quality::Perfect => 100 + 50 * (streak.clamp(1, MAX_STREAK) as i32 - 1),
        Quality::Good => 60,
        Quality::Dry => 30,
    }
}

/// Bonus for completing an order, including a tip for the patience left (0..=1).
pub fn order_bonus(patties: u32, patience_left: f32) -> i32 {
    50 * patties as i32 + (patience_left.clamp(0.0, 1.0) * 100.0).round() as i32
}

/// Seconds until the next customer shows up; shrinks as the shift goes on.
pub fn order_interval(elapsed: f32) -> f32 {
    (7.0 - elapsed * 0.03).max(3.0)
}

/// Number of patties in a new order. `roll` must be uniform in `[0, 1)`.
pub fn order_size(elapsed: f32, roll: f32) -> u32 {
    let multi = (elapsed / 120.0).min(0.6);
    if roll < multi * 0.35 {
        3
    } else if roll < multi {
        2
    } else {
        1
    }
}

/// How many seconds a customer waits for their whole order.
pub fn order_patience(elapsed: f32, patties: u32) -> f32 {
    (34.0 - elapsed * 0.06).max(20.0) + 7.0 * patties.saturating_sub(1) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doneness_thresholds() {
        assert_eq!(Doneness::of(0.0), Doneness::Raw);
        assert_eq!(Doneness::of(DONE_AT - 0.01), Doneness::Raw);
        assert_eq!(Doneness::of(DONE_AT), Doneness::Perfect);
        assert_eq!(Doneness::of(OVERDONE_AT), Doneness::Overdone);
        assert_eq!(Doneness::of(BURNT_AT), Doneness::Burnt);
        assert_eq!(Doneness::of(IGNITES_AT), Doneness::Burnt);
    }

    #[test]
    fn judging_a_patty() {
        assert_eq!(judge(0.2, 0.0), Verdict::Cooking);
        assert_eq!(judge(0.2, 0.8), Verdict::Cooking);
        assert_eq!(judge(0.8, 0.1), Verdict::Flip);
        assert_eq!(judge(0.8, 0.9), Verdict::Ready(Quality::Perfect));
        assert_eq!(judge(1.1, 0.9), Verdict::Ready(Quality::Good));
        assert_eq!(judge(0.9, 1.1), Verdict::Ready(Quality::Good));
        assert_eq!(judge(1.1, 1.2), Verdict::Ready(Quality::Dry));
        assert_eq!(judge(1.4, 0.0), Verdict::Ruined);
        assert_eq!(judge(0.8, 1.5), Verdict::Ruined);
    }

    #[test]
    fn perfect_streaks_earn_more() {
        assert_eq!(serve_points(Quality::Perfect, 1), 100);
        assert_eq!(serve_points(Quality::Perfect, 3), 200);
        assert_eq!(
            serve_points(Quality::Perfect, 99),
            serve_points(Quality::Perfect, MAX_STREAK)
        );
        assert_eq!(serve_points(Quality::Good, 4), 60);
        assert!(serve_points(Quality::Dry, 1) < serve_points(Quality::Good, 1));
    }

    #[test]
    fn order_bonus_rewards_speed() {
        assert_eq!(order_bonus(1, 0.0), 50);
        assert_eq!(order_bonus(2, 1.0), 200);
        assert_eq!(order_bonus(1, 7.0), order_bonus(1, 1.0));
    }

    #[test]
    fn difficulty_ramps_up_and_levels_off() {
        assert!(order_interval(0.0) > order_interval(60.0));
        assert_eq!(order_interval(10_000.0), 3.0);
        assert!(order_patience(0.0, 1) > order_patience(120.0, 1));
        assert_eq!(order_patience(10_000.0, 1), 20.0);
        assert!(order_patience(0.0, 3) > order_patience(0.0, 1));
    }

    #[test]
    fn early_orders_are_single_patties() {
        for i in 0..100 {
            assert_eq!(order_size(0.0, i as f32 / 100.0), 1);
        }
        let late: Vec<u32> = (0..100)
            .map(|i| order_size(600.0, i as f32 / 100.0))
            .collect();
        assert!(late.contains(&1) && late.contains(&2) && late.contains(&3));
    }
}
