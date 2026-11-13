# Burger Flipper

A small 3D restaurant game built with [Bevy](https://bevyengine.org) 0.20. Take orders at the
counter, stock your trays in storage, grill patties and bacon, and stack burgers at the prep
table before your customers lose patience.

Everything is generated in code: meshes, materials, particles and sound effects are all procedural,
so there are no asset files.

## Running

```sh
cargo run --release
```

The first build compiles Bevy and takes a few minutes. Debug builds (`cargo run`) are also playable,
because dependencies are optimized in the dev profile.

## How to play

The restaurant has four stations, each its own 3D set. Switch with the tabs at the bottom of the
screen or the keys `1` to `4`. The grill keeps cooking while you are somewhere else, and a tab
flashes when its station needs you. `P` or `Esc` pauses.

| Station           | What you do there                                                          |
| ----------------- | -------------------------------------------------------------------------- |
| `1` Counter       | Click a customer in line to take their order. A ticket goes on the rail.   |
| `2` Storage       | Click a crate to load a pack of that ingredient onto its tray.             |
| `3` Grilling      | Click a raw tray to choose patty or bacon, click the grill to drop one, click it to flip, and click again when it says **READY!** to send it to the prep table. |
| `4` Prep table    | Drag ingredients from the trays onto the plate, then ring the bell.        |

- **Tickets** hang at the top and show the burger from lid to base, with a patience bar. The
  framed ticket is the one the prep table is building; click another ticket to switch. A model of
  the selected order floats next to the plate.
- **Ingredients** are bun, patty, cheese, lettuce, tomato, onions and bacon. Trays hold 12 portions
  and a crate hands out 4 per click. Raw patties and bacon go to the trays next to the grill; only
  grilled ones reach the prep table.
- **Grilling:** the gauge under each item shows how cooked the side **touching the grill** is: grey
  is raw, green is perfect, orange is overdone and red is burnt. Flip on green. Bacon cooks faster
  than patties. Burnt food only goes in the bin (-25 points).
- **Building:** the first bun you drop is the base, the next one the lid. Drop layers in the order
  on the ticket and near the middle of the plate. Press on the burger to lift the top layer off
  again; let go of a piece away from the plate to put it back on its tray. The bin next to the bell
  clears the plate.
- **Strikes:** a customer who gives up on the line, a customer whose order takes too long, a burger
  that is nothing like the order, and food left to catch fire. Three strikes and you're fired.

### Scoring

- Taking food off the grill: 100 for both sides perfect, plus 50 for every perfect one in a row (up
  to 300). One side overdone: 60. Both sides overdone: 30.
- Serving a burger: the customer scores **waiting** (full marks while more than half their patience
  is left), **grilling** (how well the patties and bacon in it were cooked) and **building** (right
  layers, right order, neatly stacked), each out of 100. You get the sum, plus 150 when all three
  are 90 or better.
- Customers arrive faster, wait less and order taller burgers as the shift goes on. Bacon shows up
  on tickets after 40 seconds and double patties after 90.

## Code layout

| File                 | Contents                                                              |
| -------------------- | --------------------------------------------------------------------- |
| `src/main.rs`        | App setup and the `Title` / `Playing` / `GameOver` states             |
| `src/rules.rs`       | Tuning constants and pure game rules (with unit tests)                |
| `src/game.rs`        | Score, strikes, high score, random numbers and pausing                |
| `src/stations.rs`    | The four views, the camera pan between them, tabs and mouse picking   |
| `src/ingredients.rs` | Ingredient models, the pantry (stock on the trays) and tray piles     |
| `src/counter.rs`     | Dining room, customers and taking orders                              |
| `src/storage.rs`     | Shelves, crates and loading the trays                                 |
| `src/kitchen.rs`     | Camera, lights, grill, bin and the trays around the grill             |
| `src/patty.rs`       | Food on the grill: dropping, cooking, flipping, sending off, burning  |
| `src/prep.rs`        | Prep table: dragging ingredients onto the plate and serving           |
| `src/orders.rs`      | Order tickets, the selected ticket and scoring served burgers         |
| `src/hud.rs`         | Score panel, cook gauges, popups and the title/pause/game-over screens |
| `src/effects.rs`     | Particles (grease, steam, smoke, fire, sparkles) and screen shake     |
| `src/audio.rs`       | Sound effects synthesized at startup                                  |

Gameplay balance (cook speed, doneness thresholds, recipes, pacing, points) lives in
`src/rules.rs`. Run `cargo test` to check the rules after changing them.
