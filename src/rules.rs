//! Tuning constants and pure gameplay rules.
//!
//! Nothing in here touches the ECS, so the rules can be unit tested directly.

/// Cook level gained per second by the side of a patty touching the grill. Slow
/// enough to step away to another station while a side cooks.
pub const COOK_RATE: f32 = 0.11;
/// A side is cooked once it reaches this level...
pub const DONE_AT: f32 = 0.7;
/// ...stays perfect until this level...
pub const OVERDONE_AT: f32 = 1.0;
/// ...and is ruined from here on.
pub const BURNT_AT: f32 = 1.3;
/// Neglected food bursts into flames at this level.
pub const IGNITES_AT: f32 = 1.8;
/// Highest cook level shown on a gauge.
pub const GAUGE_MAX: f32 = 1.5;

pub const MAX_STRIKES: u32 = 3;
pub const MAX_STREAK: u32 = 5;
pub const TRASH_PENALTY: i32 = 25;

/// Customers who can stand in line at the counter.
pub const QUEUE_SPOTS: usize = 4;
/// Customers who can wait for their food at once, which is also the number of open tickets.
pub const MAX_OPEN_ORDERS: usize = 5;
/// Most portions of one ingredient a tray holds.
pub const TRAY_CAPACITY: u32 = 12;
/// Portions a storage crate hands out per click.
pub const PACK_SIZE: u32 = 4;
/// Most cooked patties (or bacon) that fit on the prep table.
pub const COOKED_CAPACITY: usize = 6;
/// Stock on the trays when a shift starts, indexed like [`Ingredient::ALL`].
pub const STARTER_STOCK: [u32; 7] = [6, 3, 4, 4, 4, 4, 2];
/// Most layers in an ordered burger, buns included.
pub const MAX_LAYERS: usize = 8;
/// Bacon and double patties only show up on tickets after these many seconds.
pub const BACON_FROM: f32 = 40.0;
pub const DOUBLE_FROM: f32 = 90.0;
/// A burger whose build score is below this gets sent back.
pub const REJECT_BELOW: u32 = 40;
pub const PERFECT_ORDER_BONUS: i32 = 150;

/// Everything a burger can be made of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Ingredient {
    Bun,
    Patty,
    Cheese,
    Lettuce,
    Tomato,
    Onions,
    Bacon,
}

impl Ingredient {
    pub const ALL: [Self; 7] = [
        Self::Bun,
        Self::Patty,
        Self::Cheese,
        Self::Lettuce,
        Self::Tomato,
        Self::Onions,
        Self::Bacon,
    ];

    pub const fn index(self) -> usize {
        self as usize
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Bun => "BUN",
            Self::Patty => "PATTY",
            Self::Cheese => "CHEESE",
            Self::Lettuce => "LETTUCE",
            Self::Tomato => "TOMATO",
            Self::Onions => "ONIONS",
            Self::Bacon => "BACON",
        }
    }

    /// The ingredients that have to be grilled before they go on a burger.
    pub const fn protein(self) -> Option<Protein> {
        match self {
            Self::Patty => Some(Protein::Patty),
            Self::Bacon => Some(Protein::Bacon),
            _ => None,
        }
    }
}

/// Something that cooks on the grill.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Protein {
    #[default]
    Patty,
    Bacon,
}

impl Protein {
    pub const ALL: [Self; 2] = [Self::Patty, Self::Bacon];

    pub const fn index(self) -> usize {
        self as usize
    }

    pub const fn ingredient(self) -> Ingredient {
        match self {
            Self::Patty => Ingredient::Patty,
            Self::Bacon => Ingredient::Bacon,
        }
    }

    /// Cook level gained per second; thin bacon cooks faster than a patty.
    pub const fn cook_rate(self) -> f32 {
        match self {
            Self::Patty => COOK_RATE,
            Self::Bacon => COOK_RATE * 1.5,
        }
    }
}

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

/// How well a patty or a portion of bacon was grilled.
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

/// Points for taking food off the grill. `streak` is the run of perfect ones including this
/// one; every perfect one in a row is worth 50 more than the last, up to `MAX_STREAK`.
pub fn serve_points(quality: Quality, streak: u32) -> i32 {
    match quality {
        Quality::Perfect => 100 + 50 * (streak.clamp(1, MAX_STREAK) as i32 - 1),
        Quality::Good => 60,
        Quality::Dry => 30,
    }
}

/// Builds a customer's order, bottom bun first. `roll` must return uniform numbers in `[0, 1)`.
/// Orders get taller as the shift goes on.
pub fn recipe(elapsed: f32, mut roll: impl FnMut() -> f32) -> Vec<Ingredient> {
    use Ingredient::*;
    let mut toppings = vec![Cheese, Lettuce, Tomato, Onions];
    if elapsed >= BACON_FROM {
        toppings.push(Bacon);
    }
    let most = (1 + (elapsed / 45.0) as usize).min(4);
    let wanted = (1 + (roll() * most as f32) as usize).min(most);
    let mut layers = vec![Bun, Patty];
    if elapsed >= DOUBLE_FROM && roll() < 0.25 {
        layers.push(Patty);
    }
    for _ in 0..wanted {
        let index = ((roll() * toppings.len() as f32) as usize).min(toppings.len() - 1);
        layers.push(toppings.swap_remove(index));
    }
    layers.push(Bun);
    debug_assert!(layers.len() <= MAX_LAYERS);
    layers
}

/// Seconds until the next customer walks in; shrinks as the shift goes on.
pub fn customer_interval(elapsed: f32) -> f32 {
    (16.0 - elapsed * 0.03).max(8.0)
}

/// How many seconds a customer stands in line before giving up.
pub fn queue_patience(elapsed: f32) -> f32 {
    (40.0 - elapsed * 0.04).max(25.0)
}

/// How many seconds a customer waits for their burger once the order is taken.
pub fn order_patience(elapsed: f32, layers: usize) -> f32 {
    (90.0 - elapsed * 0.08).max(55.0) + 4.0 * layers as f32
}

/// How a customer rates their burger; every part is out of 100.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scorecard {
    pub waiting: u32,
    pub grilling: u32,
    pub building: u32,
}

impl Scorecard {
    pub fn new(
        patience_left: f32,
        recipe: &[Ingredient],
        built: &[Ingredient],
        cooked: &[Quality],
        sloppiness: f32,
    ) -> Self {
        Self {
            waiting: waiting_score(patience_left),
            grilling: grilling_score(recipe, cooked),
            building: building_score(recipe, built, sloppiness),
        }
    }

    /// The customer sends back a burger that is nothing like what they ordered.
    pub fn is_rejected(&self) -> bool {
        self.building < REJECT_BELOW
    }

    pub fn is_perfect(&self) -> bool {
        self.waiting >= 90 && self.grilling >= 90 && self.building >= 90
    }

    pub fn points(&self) -> i32 {
        let bonus = if self.is_perfect() {
            PERFECT_ORDER_BONUS
        } else {
            0
        };
        (self.waiting + self.grilling + self.building) as i32 + bonus
    }
}

/// Full marks while more than half of the customer's patience (0..=1) is left.
pub fn waiting_score(patience_left: f32) -> u32 {
    ((patience_left.clamp(0.0, 1.0) * 2.0).min(1.0) * 100.0).round() as u32
}

/// Average quality of the grilled layers. Grilled layers the order asked for
/// but the burger lacks count as zero.
pub fn grilling_score(recipe: &[Ingredient], cooked: &[Quality]) -> u32 {
    let wanted = recipe
        .iter()
        .filter(|layer| layer.protein().is_some())
        .count()
        .max(cooked.len());
    if wanted == 0 {
        return 100;
    }
    let total: u32 = cooked
        .iter()
        .map(|quality| match quality {
            Quality::Perfect => 100,
            Quality::Good => 70,
            Quality::Dry => 40,
        })
        .sum();
    total / wanted as u32
}

/// How closely the stack matches the order, layer by layer and in the right sequence,
/// marked down by up to 30% for a `sloppiness` (0..=1) of layers dropped off-centre.
pub fn building_score(recipe: &[Ingredient], built: &[Ingredient], sloppiness: f32) -> u32 {
    let longest = recipe.len().max(built.len());
    if longest == 0 {
        return 0;
    }
    let accuracy = common_layers(recipe, built) as f32 / longest as f32;
    (accuracy * (1.0 - 0.3 * sloppiness.clamp(0.0, 1.0)) * 100.0).round() as u32
}

/// Length of the longest sequence of layers that appears in both stacks in the same order.
fn common_layers(a: &[Ingredient], b: &[Ingredient]) -> usize {
    let mut row = vec![0; b.len() + 1];
    for x in a {
        let mut diagonal = 0;
        for (j, y) in b.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = if x == y {
                diagonal + 1
            } else {
                above.max(row[j])
            };
            diagonal = above;
        }
    }
    row[b.len()]
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
    fn recipes_are_buns_around_a_patty() {
        use Ingredient::*;
        let mut seed = 0.37_f32;
        let mut roll = move || {
            seed = (seed * 7.13 + 0.31).fract();
            seed
        };
        for elapsed in [0.0, 30.0, 60.0, 120.0, 600.0] {
            for _ in 0..50 {
                let layers = recipe(elapsed, &mut roll);
                assert_eq!(layers.first(), Some(&Bun));
                assert_eq!(layers.last(), Some(&Bun));
                assert_eq!(layers[1], Patty);
                assert!((4..=MAX_LAYERS).contains(&layers.len()));
                for topping in [Cheese, Lettuce, Tomato, Onions, Bacon] {
                    assert!(layers.iter().filter(|&&layer| layer == topping).count() <= 1);
                }
                if elapsed < BACON_FROM {
                    assert!(!layers.contains(&Bacon));
                    assert_eq!(layers.len(), 4);
                }
            }
        }
    }

    #[test]
    fn waiting_is_forgiving_at_first() {
        assert_eq!(waiting_score(1.0), 100);
        assert_eq!(waiting_score(0.5), 100);
        assert_eq!(waiting_score(0.25), 50);
        assert_eq!(waiting_score(-1.0), 0);
    }

    #[test]
    fn grilling_averages_the_cooked_layers() {
        use Ingredient::*;
        let order = [Bun, Patty, Bacon, Bun];
        assert_eq!(grilling_score(&[Bun, Lettuce, Bun], &[]), 100);
        assert_eq!(
            grilling_score(&order, &[Quality::Perfect, Quality::Perfect]),
            100
        );
        assert_eq!(
            grilling_score(&order, &[Quality::Perfect, Quality::Dry]),
            70
        );
        // The bacon is missing.
        assert_eq!(grilling_score(&order, &[Quality::Perfect]), 50);
        assert_eq!(grilling_score(&order, &[]), 0);
    }

    #[test]
    fn building_rewards_the_right_layers_in_the_right_order() {
        use Ingredient::*;
        let order = [Bun, Patty, Cheese, Lettuce, Bun];
        assert_eq!(building_score(&order, &order, 0.0), 100);
        assert_eq!(building_score(&order, &order, 1.0), 70);
        // One layer missing, one swapped, one extra.
        assert_eq!(building_score(&order, &[Bun, Patty, Lettuce, Bun], 0.0), 80);
        assert_eq!(
            building_score(&order, &[Bun, Patty, Lettuce, Cheese, Bun], 0.0),
            80
        );
        assert_eq!(
            building_score(&order, &[Bun, Patty, Cheese, Lettuce, Tomato, Bun], 0.0),
            83
        );
        assert_eq!(building_score(&order, &[], 0.0), 0);
        assert!(building_score(&order, &[Bun, Bun], 0.0) >= REJECT_BELOW);
        assert!(building_score(&order, &[Tomato, Onions], 0.0) < REJECT_BELOW);
    }

    #[test]
    fn scorecards_pay_out() {
        use Ingredient::*;
        let order = [Bun, Patty, Cheese, Bun];
        let perfect = Scorecard::new(0.9, &order, &order, &[Quality::Perfect], 0.0);
        assert!(perfect.is_perfect() && !perfect.is_rejected());
        assert_eq!(perfect.points(), 300 + PERFECT_ORDER_BONUS);
        let late = Scorecard::new(0.1, &order, &order, &[Quality::Good], 0.5);
        assert_eq!((late.waiting, late.grilling, late.building), (20, 70, 85));
        assert_eq!(late.points(), 175);
        let wrong = Scorecard::new(0.9, &order, &[Lettuce], &[], 0.0);
        assert!(wrong.is_rejected());
    }

    #[test]
    fn bacon_cooks_faster_than_patties() {
        assert!(Protein::Bacon.cook_rate() > Protein::Patty.cook_rate());
        for protein in Protein::ALL {
            assert_eq!(protein.ingredient().protein(), Some(protein));
        }
        for (index, ingredient) in Ingredient::ALL.into_iter().enumerate() {
            assert_eq!(ingredient.index(), index);
        }
    }

    #[test]
    fn difficulty_ramps_up_and_levels_off() {
        assert!(customer_interval(0.0) > customer_interval(60.0));
        assert_eq!(customer_interval(10_000.0), 8.0);
        assert!(queue_patience(0.0) > queue_patience(120.0));
        assert_eq!(queue_patience(10_000.0), 25.0);
        assert!(order_patience(0.0, 4) > order_patience(120.0, 4));
        assert_eq!(order_patience(10_000.0, 4), 71.0);
        assert!(order_patience(0.0, 8) > order_patience(0.0, 4));
    }
}
