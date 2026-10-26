# Burger Flipper

A small 3D arcade cooking game built with [Bevy](https://bevyengine.org) 0.20. Drop patties on a
charcoal grill, flip them at the right moment and serve them before your customers lose patience.

Everything is generated in code: meshes, materials, particles and sound effects are all procedural,
so there are no asset files.

## Running

```sh
cargo run --release
```

The first build compiles Bevy and takes a few minutes. Debug builds (`cargo run`) are also playable,
because dependencies are optimized in the dev profile.

## How to play

| Action                              | What it does                                     |
| ----------------------------------- | ------------------------------------------------ |
| Click an empty spot on the grill    | Drop a raw patty                                 |
| Click a patty                       | Flip it                                          |
| Click a patty marked **READY!**     | Serve it to the oldest order (or the warmer)     |
| Click a patty marked **BURNT!**     | Scrape it into the trash (-25 points)            |
| `P` / `Esc`                         | Pause                                            |

- The gauge under each patty shows how cooked the side **touching the grill** is: grey is raw,
  green is perfect, orange is overdone and red is burnt. The side facing up shows its grill marks.
- Flip when the gauge reaches green (the patty says **FLIP!**). Once both sides are done it says
  **READY!**.
- Order tickets hang at the top. Each one needs one to three patties and has a patience bar.
- Served patties go to the oldest open order. With no open orders they go to the warmer (four
  spots), and new orders are filled from the warmer first.
- A customer who runs out of patience, or a patty left to catch fire, is a strike. Three strikes
  and you're fired.

### Scoring

- Perfect patty (both sides green): 100 points, plus 50 for every perfect patty in a row (up to 300).
- One side overdone: 60. Both sides overdone: 30.
- Completed order: 50 per patty, plus a tip of up to 100 for speed.
- Orders arrive faster and get bigger as the shift goes on.

## Code layout

| File              | Contents                                                              |
| ----------------- | --------------------------------------------------------------------- |
| `src/main.rs`     | App setup and the `Title` / `Playing` / `GameOver` states             |
| `src/rules.rs`    | Tuning constants and pure game rules (with unit tests)                |
| `src/game.rs`     | Score, strikes, high score, random numbers and pausing                |
| `src/kitchen.rs`  | Camera, lights, grill, bin, warmer and mouse picking of grill slots   |
| `src/patty.rs`    | Patty lifecycle: dropping, cooking, flipping, serving, burning        |
| `src/orders.rs`   | Customers, order tickets and the warmer                               |
| `src/hud.rs`      | Score panel, cook gauges, popups and the title/pause/game-over screens |
| `src/effects.rs`  | Particles (grease, steam, smoke, fire, sparkles) and screen shake     |
| `src/audio.rs`    | Sound effects synthesized at startup                                  |

Gameplay balance (cook speed, doneness thresholds, order pacing, points) lives in
`src/rules.rs`. Run `cargo test` to check the rules after changing them.
