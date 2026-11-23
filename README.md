# Bombicat

[![Release](https://img.shields.io/github/v/release/Antidote1911/bombicat?style=flat-square)](https://github.com/Antidote1911/bombicat/releases/latest)
[![Build](https://img.shields.io/github/actions/workflow/status/Antidote1911/bombicat/release.yml?style=flat-square&label=build)](https://github.com/Antidote1911/bombicat/actions/workflows/release.yml)
[![Rust](https://img.shields.io/badge/Rust-2021-orange?style=flat-square&logo=rust)](https://www.rust-lang.org)
[![License: GPL v3](https://img.shields.io/badge/license-GPL%20v3-blue?style=flat-square)](LICENSE)

![Bombicat screenshot](bombicat.png)

A minesweeper with cats, written in Rust with [egui](https://github.com/emilk/egui).

[Lire en français](README_fr.md)

## Features

- **6 difficulty levels** — from Beginner to Chuck Norris
- **No-guess generation** — every grid is guaranteed solvable by pure logic, no guessing required
- **High scores** — top 10 times per level, stored locally in SQLite
- **Hint** — `Ctrl+H` points at a cell that can actually be deduced (the game then no longer counts for high scores)
- **Resizable window** — cell size follows the window size
- **Dark UI** — fully vector-rendered via egui

## Controls

| Action | Input |
|---|---|
| Reveal a cell | Left click (on release, on the cell under the pointer) |
| Place / remove a flag / `?` | Right click (cycles: none → flag → `?`) |
| Reveal the neighbours of a number whose flags are placed | Left click on the number, middle click, or left + right |
| Hint | `Ctrl+H` |
| New game | Smiley button or `Ctrl+N` |

## Levels

| Level | Grid | Cats |
|---|---|---|
| Beginner | 15 × 10 | 15 |
| Normal | 20 × 15 | 25 |
| Hard | 25 × 15 | 35 |
| Very Hard | 27 × 20 | 120 |
| Giant | 35 × 22 | 190 |
| Chuck Norris | 40 × 25 | 250 |

## Building and running

Prerequisite: [Rust](https://rustup.rs/) (edition 2021 or later)

```bash
cargo run --release
```

### Cheat mode

The `--cheat` flag turns middle-click into "reveal the entire grid" (the game then no longer counts for high scores):

```bash
cargo run --release -- --cheat
```

### Tests

```bash
cargo test --release
```

## No-guess algorithm

On each new game, the first click triggers grid generation in a background thread. The generator produces random cat placements in parallel (via Rayon) and feeds them to a logic solver that simulates what a player can deduce:

- **Local constraint**: if the number of hidden cells around a number equals its remaining cat count → all of them are cats (auto-flagged)
- **Local constraint**: if all neighbouring cats are already flagged → remaining hidden neighbours are safe (auto-revealed)
- **Constraint pairs**: for two nearby numbers A and B, if the difference of their remaining cats equals the number of cells only A touches → those cells are cats and the cells only B touches are safe
- **Global constraint**: if total remaining cats equals total hidden cells → all are cats; if zero cats remain → all are safe

The clicked cell and its 8 neighbours are always guaranteed cat-free. If the solver can reveal the entire grid without ever guessing, the layout is accepted. Otherwise a new placement is generated and the process repeats. The first four levels converge within a few dozen attempts; Giant and Chuck Norris need a few thousand, well under a second on a recent machine. Starting a new game cancels a generation in progress.

## Project structure

```
src/
  main.rs      — entry point, window setup
  app.rs       — UI logic (egui), click handling and modals
  model.rs     — game model, cat placement, no-guess solver
  scores.rs    — score persistence (SQLite via rusqlite)
  settings.rs  — user preferences (level, config path)
assets/
  images/
    cat.svg           — cat icon (mine)
    disarmed_red.png  — misplaced flag (revealed at game over)
    question.png      — question mark marker
```

## Persistent data

Data files are stored in standard system directories:

- **Config**: `$XDG_CONFIG_HOME/bombicat/config.json` (Linux) / `~/Library/Application Support/bombicat/config.json` (macOS) / `%APPDATA%\bombicat\config.json` (Windows)
- **Scores**: `$XDG_DATA_HOME/bombicat/scores.sqlite`

## License

[GNU General Public License v3.0](LICENSE)
