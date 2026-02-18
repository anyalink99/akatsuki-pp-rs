# akatsuki-pp-rs

`akatsuki-pp-rs` is a Rust library for **osu! difficulty and performance (pp) calculation**.
It is based on [rosu-pp](https://github.com/MaxOhn/rosu-pp) and maintained by osu!Akatsuki with custom behavior for **Relax** and **Autopilot** use cases.

This README is written as a practical guide: **what runs where, in which order, and why**.

## What this library does

The crate calculates:

- **Difficulty attributes** (e.g., star rating and mode-specific internals)
- **Performance attributes** (pp and pp breakdown)

Supported modes:

- osu!standard
- osu!taiko
- osu!catch
- osu!mania
- osu!standard 2019 (used here for relax-related calculations)

Internally, each mode has its own implementation under `src/<mode>/...`, while the unified API lives under `src/any/...`.

## Where things happen

### 1) Beatmap decoding

You load and parse a `.osu` file into a `Beatmap`:

```rust
let map = akatsuki_pp::Beatmap::from_path("./resources/2785319.osu")?;
```

This stage only reads map data and prepares it for calculations.

### 2) Difficulty calculation

You create a `Difficulty` builder, set settings (mods, clock rate, etc.), then calculate:

```rust
let diff_attrs = akatsuki_pp::Difficulty::new()
    .mods(24) // HDHR
    .calculate(&map);
```

This produces difficulty attributes (including stars).

### 3) Performance calculation

You create a `Performance` builder from either:

- previously calculated difficulty attributes (fast path), or
- a map directly (full path)

Then you set score state / result assumptions and calculate pp.

```rust
let perf = akatsuki_pp::Performance::new(diff_attrs)
    .mods(24)
    .combo(789)
    .misses(2)
    .accuracy(99.2)
    .calculate();

let pp = perf.pp();
```

---

## The required order (and why)

Use this order:

1. **Decode map**
2. **Compute difficulty**
3. **Compute performance**

Why this order is preferred:

- Performance formulas depend on difficulty attributes.
- Reusing already computed difficulty attributes avoids repeated work and is faster.
- It keeps your pipeline deterministic: same map + same settings => same result path.

> Important: if you pass precomputed difficulty attributes into performance, keep relevant settings (mods/clock rate/etc.) consistent. Reusing mismatched attributes can produce invalid pp.

## Complete quick-start example

```rust
use akatsuki_pp::{Beatmap, Difficulty, Performance};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1) Decode
    let map = Beatmap::from_path("./resources/2785319.osu")?;

    // 2) Difficulty
    let diff = Difficulty::new()
        .mods(24) // HDHR
        .calculate(&map);

    // 3) Performance
    let perf = Performance::new(diff)
        .mods(24)
        .combo(789)
        .misses(2)
        .accuracy(99.2)
        .calculate();

    println!("Stars: {:.2}", perf.difficulty.stars());
    println!("PP: {:.2}", perf.pp());

    Ok(())
}
```

## Gradual / live calculation flow

For "play in progress" or replay-like scenarios, use gradual calculators:

- `GradualDifficulty`: emits difficulty snapshots object-by-object
- `GradualPerformance`: emits pp snapshots for changing score states

Why use gradual APIs:

- You can update values incrementally instead of recalculating from scratch.
- Useful for overlays, live tracking, analyzers, and simulation tools.

## Crate features

- `raw_strains`:
  - stores internal strain values in plain `Vec`
  - slightly faster, but can increase memory risk on maliciously long maps
- `sync`:
  - enables thread-sharing for some gradual types
  - small runtime overhead
- `tracing`:
  - logs beatmap decode errors with `tracing`

## Installation

In `Cargo.toml`:

```toml
[dependencies]
akatsuki-pp = "1.1.2"
```

Or with feature flags:

```toml
[dependencies]
akatsuki-pp = { version = "1.1.2", features = ["raw_strains", "tracing"] }
```

## Practical integration checklist

When integrating into a service or bot:

1. Parse map once and cache if possible.
2. Cache difficulty attributes per `(map_id/checksum, mods, clock_rate, mode settings)`.
3. Reuse cached difficulty attrs for pp requests.
4. Validate incoming score state fields before calculating pp.
5. Use gradual APIs only when you truly need progressive updates.

This minimizes latency and avoids accidental mismatch bugs.

## Accuracy and intent

The project inherits the accuracy/performance philosophy of `rosu-pp` while adding Akatsuki-specific behavior for relax/autopilot contexts.

So the intended usage is:

- default osu! pp/difficulty style pipeline
- plus server-specific behavior where Akatsuki requires it

## Useful links

- Repository: <https://github.com/osuAkatsuki/akatsuki-pp-rs>
- Upstream base: <https://github.com/MaxOhn/rosu-pp>
- Akatsuki: <https://akatsuki.gg>
