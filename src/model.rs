use rand::prelude::*;
use rayon::prelude::*;
use std::sync::atomic::{AtomicU32, Ordering};

#[derive(Clone, Debug, Default)]
pub struct Field {
    pub cat: bool,
    pub discovered: bool,
    pub flag: u8, // 0=none, 1=flag, 2=question
    pub neighbours: u8,
}

pub struct Model {
    pub width: usize,
    pub height: usize,
    total_cats: usize,
    discovered_count: usize,
    pub data: Vec<Field>,
    rng: StdRng,
}

impl Model {
    pub fn new() -> Self {
        Self {
            width: 0,
            height: 0,
            total_cats: 0,
            discovered_count: 0,
            data: Vec::new(),
            rng: StdRng::from_entropy(),
        }
    }

    pub fn reset(&mut self, width: usize, height: usize, cats: usize) {
        self.width = width;
        self.height = height;
        self.total_cats = cats;
        self.discovered_count = 0;
        self.data = vec![Field::default(); width * height];
        self.rng = StdRng::from_entropy();
    }

    pub fn is_valid(&self, x: i32, y: i32) -> bool {
        x >= 0 && x < self.width as i32 && y >= 0 && y < self.height as i32
    }

    pub fn field(&self, x: usize, y: usize) -> &Field {
        &self.data[y * self.width + x]
    }

    #[allow(dead_code)]
    pub fn field_mut(&mut self, x: usize, y: usize) -> &mut Field {
        &mut self.data[y * self.width + x]
    }

    pub fn discover(&mut self, x: usize, y: usize) {
        let f = &mut self.data[y * self.width + x];
        if !f.discovered && f.flag == 0 {
            f.discovered = true;
            self.discovered_count += 1;
        }
    }

    pub fn disarm(&mut self, x: usize, y: usize) -> i32 {
        let f = &mut self.data[y * self.width + x];
        if f.discovered {
            return 0;
        }
        let old = f.flag;
        f.flag = (f.flag + 1) % 3;
        let new = f.flag;
        // returns delta for cat_display: -1 when flag placed, +1 when flag removed
        if old == 0 && new == 1 {
            -1
        } else if old == 1 && new == 2 {
            1
        } else {
            0
        }
    }

    pub fn get_flag(&self, x: i32, y: i32) -> bool {
        self.is_valid(x, y) && self.data[y as usize * self.width + x as usize].flag == 1
    }

    pub fn count_flags_around(&self, x: usize, y: usize) -> u8 {
        let mut count = 0u8;
        for dy in -1i32..=1 {
            for dx in -1i32..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                if self.get_flag(x as i32 + dx, y as i32 + dy) {
                    count += 1;
                }
            }
        }
        count
    }

    pub fn check_win(&self) -> bool {
        let size = self.width * self.height;
        (size - self.discovered_count).saturating_sub(self.total_cats) == 0
    }

    #[allow(dead_code)]
    pub fn total_cats(&self) -> usize {
        self.total_cats
    }

    pub fn any_discovered_cat(&self) -> bool {
        self.data.iter().any(|f| f.discovered && f.cat)
    }

    pub fn find_exploded(&self) -> Option<(usize, usize)> {
        self.data.iter().enumerate().find(|(_, f)| f.discovered && f.cat).map(|(id, _)| {
            (id % self.width, id / self.width)
        })
    }

}

// ── public generation entry point ────────────────────────────────────────

/// Generates a grid guaranteed to be solvable by pure logic.
/// Candidates are tested in parallel via Rayon.
/// `attempts` is incremented for each grid tested (caller may read it concurrently).
pub fn generate_grid_no_guess(
    width: usize, height: usize, total_cats: usize,
    start_x: usize, start_y: usize,
    attempts: &AtomicU32,
) -> Vec<Field> {
    let size = width * height;

    let safe: std::collections::HashSet<usize> = (-1i32..=1)
        .flat_map(|dy| (-1i32..=1).map(move |dx| (dx, dy)))
        .map(|(dx, dy)| (start_x as i32 + dx, start_y as i32 + dy))
        .filter(|&(nx, ny)| nx >= 0 && nx < width as i32 && ny >= 0 && ny < height as i32)
        .map(|(nx, ny)| ny as usize * width + nx as usize)
        .collect();

    let batch_size = (rayon::current_num_threads() * 4).max(16);
    let mut rng = StdRng::from_entropy();

    loop {
        let seeds: Vec<u64> = (0..batch_size).map(|_| rng.gen()).collect();

        let result = seeds.into_par_iter().find_map_any(|seed| {
            attempts.fetch_add(1, Ordering::Relaxed);

            let mut data = vec![Field::default(); size];
            let mut rng  = StdRng::seed_from_u64(seed);

            let mut placed = 0;
            while placed < total_cats {
                let id = rng.gen_range(0..size);
                if !data[id].cat && !safe.contains(&id) {
                    data[id].cat = true;
                    placed += 1;
                }
            }

            for y in 0..height {
                for x in 0..width {
                    let mut count = 0u8;
                    for dy in -1i32..=1 {
                        for dx in -1i32..=1 {
                            let nx = x as i32 + dx;
                            let ny = y as i32 + dy;
                            if nx >= 0 && nx < width as i32
                                && ny >= 0 && ny < height as i32
                                && data[ny as usize * width + nx as usize].cat
                            {
                                count += 1;
                            }
                        }
                    }
                    data[y * width + x].neighbours = count;
                }
            }

            if grid_is_solvable(&data, width, height, total_cats, start_x, start_y) {
                Some(data)
            } else {
                None
            }
        });

        if let Some(data) = result {
            return data;
        }
    }
}

// ── standalone solver (used in parallel tasks) ────────────────────────────

fn grid_simulate_reveal(
    data: &[Field], discovered: &mut Vec<bool>,
    width: usize, height: usize,
    start_x: usize, start_y: usize,
) {
    let mut stack = vec![(start_x, start_y)];
    while let Some((x, y)) = stack.pop() {
        let idx = y * width + x;
        if discovered[idx] || data[idx].cat { continue; }
        discovered[idx] = true;
        if data[idx].neighbours == 0 {
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    if dx == 0 && dy == 0 { continue; }
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    if nx >= 0 && nx < width as i32 && ny >= 0 && ny < height as i32 {
                        stack.push((nx as usize, ny as usize));
                    }
                }
            }
        }
    }
}

fn grid_is_solvable(
    data: &[Field], width: usize, height: usize,
    total_cats: usize, start_x: usize, start_y: usize,
) -> bool {
    let n = width * height;
    let mut discovered = vec![false; n];
    let mut flagged    = vec![false; n];

    grid_simulate_reveal(data, &mut discovered, width, height, start_x, start_y);

    loop {
        let mut progress = false;

        for y in 0..height {
            for x in 0..width {
                let idx = y * width + x;
                if !discovered[idx] || data[idx].cat { continue; }
                let nb = data[idx].neighbours as usize;
                if nb == 0 { continue; }

                let mut hidden: Vec<usize> = Vec::new();
                let mut flag_count = 0usize;

                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        if dx == 0 && dy == 0 { continue; }
                        let nx = x as i32 + dx;
                        let ny = y as i32 + dy;
                        if nx < 0 || nx >= width as i32 || ny < 0 || ny >= height as i32 { continue; }
                        let nidx = ny as usize * width + nx as usize;
                        if !discovered[nidx] {
                            if flagged[nidx] { flag_count += 1; }
                            else { hidden.push(nidx); }
                        }
                    }
                }

                // All remaining hidden neighbours are cats → flag them.
                if flag_count + hidden.len() == nb && !hidden.is_empty() {
                    for &nidx in &hidden { flagged[nidx] = true; progress = true; }
                }
                // All cats accounted for → reveal remaining hidden neighbours.
                if flag_count == nb && !hidden.is_empty() {
                    for &nidx in &hidden {
                        if !discovered[nidx] {
                            grid_simulate_reveal(
                                data, &mut discovered, width, height,
                                nidx % width, nidx / width,
                            );
                            progress = true;
                        }
                    }
                }
            }
        }

        // Global constraint: compare remaining cats vs remaining hidden cells.
        let flags_placed   = flagged.iter().filter(|&&f| f).count();
        let remaining_cats = total_cats.saturating_sub(flags_placed);
        let hidden_total: Vec<usize> = (0..n).filter(|&i| !discovered[i] && !flagged[i]).collect();

        if remaining_cats == hidden_total.len() && !hidden_total.is_empty() {
            for i in hidden_total { flagged[i] = true; progress = true; }
        } else if remaining_cats == 0 && !hidden_total.is_empty() {
            for i in hidden_total {
                grid_simulate_reveal(data, &mut discovered, width, height, i % width, i / width);
                progress = true;
            }
        }

        if !progress { break; }
    }

    data.iter().zip(discovered.iter()).all(|(f, &d)| f.cat || d)
}
