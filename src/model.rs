use rand::prelude::*;
use rayon::prelude::*;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

pub const FLAG_NONE: u8 = 0;
pub const FLAG_SET: u8 = 1;
pub const FLAG_QUESTION: u8 = 2;

#[derive(Clone, Debug, Default)]
pub struct Field {
    pub cat: bool,
    pub discovered: bool,
    pub flag: u8, // FLAG_NONE, FLAG_SET ou FLAG_QUESTION
    pub neighbours: u8,
}

/// Appelle `f` pour chacune des (au plus) 8 cases voisines de `idx`.
#[inline]
fn for_each_neighbour(width: usize, height: usize, idx: usize, mut f: impl FnMut(usize)) {
    let x = idx % width;
    let y = idx / width;
    let x0 = x.saturating_sub(1);
    let x1 = (x + 1).min(width - 1);
    let y0 = y.saturating_sub(1);
    let y1 = (y + 1).min(height - 1);
    for ny in y0..=y1 {
        for nx in x0..=x1 {
            let nidx = ny * width + nx;
            if nidx != idx {
                f(nidx);
            }
        }
    }
}

// ── modèle de jeu ─────────────────────────────────────────────────────────

pub struct Model {
    pub width: usize,
    pub height: usize,
    total_cats: usize,
    discovered_count: usize,
    flag_count: usize,
    exploded: Option<usize>,
    pub data: Vec<Field>,
}

impl Model {
    pub fn new() -> Self {
        Self {
            width: 0,
            height: 0,
            total_cats: 0,
            discovered_count: 0,
            flag_count: 0,
            exploded: None,
            data: Vec::new(),
        }
    }

    /// Grille vide (les chats sont placés au premier clic, voir `install`).
    pub fn reset(&mut self, width: usize, height: usize, cats: usize) {
        self.width = width;
        self.height = height;
        self.total_cats = cats;
        self.discovered_count = 0;
        self.flag_count = 0;
        self.exploded = None;
        self.data = vec![Field::default(); width * height];
    }

    /// Installe une grille générée en conservant les marques (drapeaux, `?`)
    /// que le joueur a posées avant son premier clic.
    pub fn install(&mut self, mut data: Vec<Field>) {
        debug_assert_eq!(data.len(), self.data.len());
        for (new, old) in data.iter_mut().zip(self.data.iter()) {
            new.flag = old.flag;
            new.discovered = false;
        }
        self.total_cats = data.iter().filter(|f| f.cat).count();
        self.discovered_count = 0;
        self.exploded = None;
        self.data = data;
    }

    pub fn field(&self, x: usize, y: usize) -> &Field {
        &self.data[y * self.width + x]
    }

    pub fn total_cats(&self) -> usize {
        self.total_cats
    }

    /// Chats restants à marquer (peut être négatif si trop de drapeaux).
    pub fn cats_left(&self) -> i32 {
        self.total_cats as i32 - self.flag_count as i32
    }

    /// Case du chat qui a explosé, si la partie est perdue.
    pub fn exploded(&self) -> Option<(usize, usize)> {
        self.exploded.map(|idx| (idx % self.width, idx / self.width))
    }

    pub fn is_won(&self) -> bool {
        self.exploded.is_none()
            && self.discovered_count + self.total_cats == self.width * self.height
    }

    /// Découvre une case ; si elle est vide (0 voisin), propage aux voisines.
    /// Les cases marquées ne sont jamais découvertes.
    pub fn reveal(&mut self, x: usize, y: usize) {
        let (width, height) = (self.width, self.height);
        let mut stack = vec![y * width + x];
        while let Some(idx) = stack.pop() {
            let f = &mut self.data[idx];
            if f.discovered || f.flag != FLAG_NONE {
                continue;
            }
            f.discovered = true;
            if f.cat {
                self.exploded.get_or_insert(idx);
                continue;
            }
            self.discovered_count += 1;
            if f.neighbours == 0 {
                for_each_neighbour(width, height, idx, |n| stack.push(n));
            }
        }
    }

    /// Sur un chiffre découvert entouré d'autant de drapeaux que sa valeur,
    /// découvre toutes les voisines non marquées. Renvoie `true` si l'action
    /// a été appliquée.
    pub fn chord(&mut self, x: usize, y: usize) -> bool {
        let idx = y * self.width + x;
        let f = &self.data[idx];
        if !f.discovered || f.cat || f.neighbours == 0 {
            return false;
        }
        let mut flags = 0u8;
        let mut targets = Vec::with_capacity(8);
        for_each_neighbour(self.width, self.height, idx, |n| {
            let nf = &self.data[n];
            if nf.flag == FLAG_SET {
                flags += 1;
            } else if !nf.discovered && nf.flag == FLAG_NONE {
                targets.push(n);
            }
        });
        if flags != f.neighbours || targets.is_empty() {
            return false;
        }
        for n in targets {
            self.reveal(n % self.width, n / self.width);
        }
        true
    }

    /// Fait tourner la marque d'une case cachée : rien → drapeau → `?` → rien.
    pub fn toggle_mark(&mut self, x: usize, y: usize) {
        let f = &mut self.data[y * self.width + x];
        if f.discovered {
            return;
        }
        if f.flag == FLAG_SET {
            self.flag_count -= 1;
        }
        f.flag = (f.flag + 1) % 3;
        if f.flag == FLAG_SET {
            self.flag_count += 1;
        }
    }

    /// À la victoire : pose un drapeau sur tous les chats restants.
    pub fn flag_remaining_cats(&mut self) {
        for f in &mut self.data {
            if f.cat && !f.discovered {
                f.flag = FLAG_SET;
            } else if !f.cat && f.flag != FLAG_NONE {
                f.flag = FLAG_NONE;
            }
        }
        self.flag_count = self.data.iter().filter(|f| f.flag == FLAG_SET).count();
    }

    /// Mode triche : tout découvrir.
    pub fn reveal_all(&mut self) {
        for f in &mut self.data {
            f.discovered = true;
        }
    }

    /// Case sûre que le joueur peut déduire de ce qui est visible.
    /// Ses drapeaux ne servent pas à la déduction (ils peuvent être faux).
    pub fn deducible_safe(&self) -> Option<(usize, usize)> {
        if self.exploded.is_some() || self.data.is_empty() {
            return None;
        }
        let discovered: Vec<bool> = self.data.iter().map(|f| f.discovered).collect();
        let mut solver =
            Solver::with_state(&self.data, self.width, self.height, self.total_cats, discovered);
        solver.collect_safe = true;
        solver.run();
        // De préférence une case que le joueur n'a pas marquée ; sinon une
        // case marquée à tort (son drapeau est faux).
        solver.safe_found.iter()
            .find(|&&idx| self.data[idx].flag == FLAG_NONE)
            .or(solver.safe_found.first())
            .map(|&idx| (idx % self.width, idx / self.width))
    }

    /// Case à suggérer au joueur : une case déductible si possible,
    /// sinon une case sûre au bord de la zone découverte.
    pub fn hint(&self) -> Option<(usize, usize)> {
        if let Some(pos) = self.deducible_safe() {
            return Some(pos);
        }
        let (width, height) = (self.width, self.height);
        let safe_hidden = |idx: &usize| !self.data[*idx].discovered && !self.data[*idx].cat;
        let on_frontier = |idx: &usize| {
            let mut found = false;
            for_each_neighbour(width, height, *idx, |n| found |= self.data[n].discovered);
            found
        };
        (0..self.data.len())
            .filter(safe_hidden)
            .find(on_frontier)
            .or_else(|| (0..self.data.len()).find(safe_hidden))
            .map(|idx| (idx % width, idx / width))
    }
}

// ── génération sans hasard ────────────────────────────────────────────────

/// Place `cats` chats parmi `candidates` (tirage sans remise) et calcule
/// le nombre de voisins de chaque case.
fn random_grid(
    width: usize, height: usize, cats: usize,
    candidates: &[usize], rng: &mut impl Rng,
) -> Vec<Field> {
    let mut data = vec![Field::default(); width * height];
    for pick in rand::seq::index::sample(rng, candidates.len(), cats) {
        let idx = candidates[pick];
        data[idx].cat = true;
        for_each_neighbour(width, height, idx, |n| data[n].neighbours += 1);
    }
    data
}

/// Génère une grille garantie résoluble par logique pure depuis la case de
/// départ, qui est toujours vide (elle et ses 8 voisines sont sans chat).
/// Les candidates sont testées en parallèle via Rayon.
///
/// `attempts` est incrémenté à chaque grille testée (lisible en parallèle).
/// Renvoie `None` si `cancel` passe à `true` avant qu'une grille soit trouvée.
pub fn generate_grid_no_guess(
    width: usize, height: usize, total_cats: usize,
    start_x: usize, start_y: usize,
    attempts: &AtomicU32,
    cancel: &AtomicBool,
) -> Option<Vec<Field>> {
    let size = width * height;
    let start = start_y * width + start_x;

    let mut is_safe = vec![false; size];
    is_safe[start] = true;
    for_each_neighbour(width, height, start, |n| is_safe[n] = true);
    let candidates: Vec<usize> = (0..size).filter(|&i| !is_safe[i]).collect();
    let cats = total_cats.min(candidates.len());

    let batch_size = (rayon::current_num_threads() * 4).max(16);
    let mut rng = StdRng::from_entropy();

    loop {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        let seeds: Vec<u64> = (0..batch_size).map(|_| rng.gen()).collect();

        let result = seeds.into_par_iter().find_map_any(|seed| {
            if cancel.load(Ordering::Relaxed) {
                return None;
            }
            attempts.fetch_add(1, Ordering::Relaxed);
            let mut rng = StdRng::seed_from_u64(seed);
            let data = random_grid(width, height, cats, &candidates, &mut rng);
            grid_is_solvable(&data, width, height, cats, start_x, start_y).then_some(data)
        });

        if result.is_some() {
            return result;
        }
    }
}

fn grid_is_solvable(
    data: &[Field], width: usize, height: usize,
    total_cats: usize, start_x: usize, start_y: usize,
) -> bool {
    let mut solver =
        Solver::with_state(data, width, height, total_cats, vec![false; width * height]);
    solver.mark_safe(start_y * width + start_x);
    solver.run();
    solver.is_solved()
}

// ── solveur logique ───────────────────────────────────────────────────────

/// « Parmi ces `len` cases cachées, exactement `remaining` sont des chats. »
#[derive(Clone, Copy, Default)]
struct Constraint {
    cells: [u32; 8],
    len: u8,
    remaining: u8,
}

impl Constraint {
    fn cells(&self) -> &[u32] {
        &self.cells[..self.len as usize]
    }

    fn contains(&self, cell: u32) -> bool {
        self.cells().contains(&cell)
    }
}

/// Simule ce qu'un joueur peut déduire sans jamais deviner.
///
/// Règles appliquées jusqu'à ce que plus rien ne bouge :
/// 1. un chiffre dont toutes les cases cachées sont des chats / sont sûres ;
/// 2. deux chiffres proches A et B avec `ra ≥ rb` : comme
///    `chats(A−B) − chats(B−A) = ra − rb`, si `ra − rb = |A−B|` alors
///    toutes les cases de A−B sont des chats et celles de B−A sont sûres ;
/// 3. le compte global de chats restants.
struct Solver<'a> {
    data: &'a [Field],
    width: usize,
    height: usize,
    total_cats: usize,
    discovered: Vec<bool>,
    flagged: Vec<bool>,
    n_discovered: usize,
    n_flagged: usize,
    constraints: Vec<Constraint>,
    stack: Vec<usize>,
    /// Mode « aide » : noter les cases prouvées sûres au lieu de les découvrir.
    collect_safe: bool,
    safe_found: Vec<usize>,
    is_safe_found: Vec<bool>,
}

impl<'a> Solver<'a> {
    fn with_state(
        data: &'a [Field], width: usize, height: usize,
        total_cats: usize, discovered: Vec<bool>,
    ) -> Self {
        let n = width * height;
        let n_discovered = discovered.iter().filter(|&&d| d).count();
        Self {
            data, width, height, total_cats,
            discovered,
            flagged: vec![false; n],
            n_discovered,
            n_flagged: 0,
            constraints: vec![Constraint::default(); n],
            stack: Vec::new(),
            collect_safe: false,
            safe_found: Vec::new(),
            is_safe_found: vec![false; n],
        }
    }

    fn is_solved(&self) -> bool {
        self.n_discovered + self.total_cats == self.width * self.height
    }

    /// Une case prouvée sûre : on la découvre (avec propagation des zéros).
    /// Renvoie `true` si cela apporte quelque chose de nouveau.
    fn mark_safe(&mut self, idx: usize) -> bool {
        if self.discovered[idx] || self.data[idx].cat {
            return false;
        }
        if self.collect_safe {
            if self.is_safe_found[idx] {
                return false;
            }
            self.is_safe_found[idx] = true;
            self.safe_found.push(idx);
            return true;
        }
        self.stack.push(idx);
        while let Some(i) = self.stack.pop() {
            if self.discovered[i] || self.data[i].cat {
                continue;
            }
            self.discovered[i] = true;
            self.n_discovered += 1;
            if self.data[i].neighbours == 0 {
                let stack = &mut self.stack;
                for_each_neighbour(self.width, self.height, i, |n| stack.push(n));
            }
        }
        true
    }

    /// Une case prouvée être un chat.
    fn mark_cat(&mut self, idx: usize) -> bool {
        if self.flagged[idx] || self.discovered[idx] {
            return false;
        }
        self.flagged[idx] = true;
        self.n_flagged += 1;
        true
    }

    fn constraint_at(&self, idx: usize) -> Constraint {
        let mut c = Constraint::default();
        let f = &self.data[idx];
        if !self.discovered[idx] || f.cat || f.neighbours == 0 {
            return c;
        }
        let mut flags = 0u8;
        for_each_neighbour(self.width, self.height, idx, |n| {
            if self.discovered[n] {
                return;
            }
            if self.flagged[n] {
                flags += 1;
            } else {
                c.cells[c.len as usize] = n as u32;
                c.len += 1;
            }
        });
        c.remaining = f.neighbours.saturating_sub(flags);
        c
    }

    fn run(&mut self) {
        let n = self.width * self.height;
        loop {
            let mut progress = false;

            // Photographie des contraintes. Elles restent vraies même si des
            // cases sont résolues pendant le tour : les utiliser reste correct.
            for idx in 0..n {
                self.constraints[idx] = self.constraint_at(idx);
            }

            for idx in 0..n {
                let a = self.constraints[idx];
                if a.len == 0 {
                    continue;
                }

                // Règle 1 : contrainte seule.
                if a.remaining == a.len {
                    for &c in a.cells() {
                        progress |= self.mark_cat(c as usize);
                    }
                } else if a.remaining == 0 {
                    for &c in a.cells() {
                        progress |= self.mark_safe(c as usize);
                    }
                }

                // Règle 2 : paires. Deux chiffres ne partagent des voisines
                // que s'ils sont à 2 cases au plus l'un de l'autre.
                let x = idx % self.width;
                let y = idx / self.width;
                let x0 = x.saturating_sub(2);
                let x1 = (x + 2).min(self.width - 1);
                let y0 = y.saturating_sub(2);
                let y1 = (y + 2).min(self.height - 1);
                for ny in y0..=y1 {
                    for nx in x0..=x1 {
                        let jdx = ny * self.width + nx;
                        if jdx == idx {
                            continue;
                        }
                        let b = self.constraints[jdx];
                        if b.len == 0 || a.remaining < b.remaining {
                            continue;
                        }
                        let only_a = a.cells().iter().filter(|&&c| !b.contains(c)).count();
                        if only_a == a.len as usize {
                            continue; // aucune case commune
                        }
                        if (a.remaining - b.remaining) as usize != only_a {
                            continue;
                        }
                        for &c in a.cells() {
                            if !b.contains(c) {
                                progress |= self.mark_cat(c as usize);
                            }
                        }
                        for &c in b.cells() {
                            if !a.contains(c) {
                                progress |= self.mark_safe(c as usize);
                            }
                        }
                    }
                }
            }

            // Règle 3 : compte global.
            let hidden = n - self.n_discovered - self.n_flagged - self.safe_found.len();
            let remaining = self.total_cats.saturating_sub(self.n_flagged);
            if hidden > 0 && (remaining == hidden || remaining == 0) {
                for idx in 0..n {
                    if self.discovered[idx] || self.flagged[idx] || self.is_safe_found[idx] {
                        continue;
                    }
                    progress |= if remaining == 0 {
                        self.mark_safe(idx)
                    } else {
                        self.mark_cat(idx)
                    };
                }
            }

            if !progress {
                return;
            }
        }
    }
}

// ── tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Ancien solveur (toutes les paires, O(n²)), gardé comme référence :
    /// le nouveau doit accepter exactement les mêmes grilles.
    fn reference_is_solvable(
        data: &[Field], width: usize, height: usize,
        total_cats: usize, start_x: usize, start_y: usize,
    ) -> bool {
        fn reveal(data: &[Field], discovered: &mut [bool], width: usize, height: usize, start: usize) {
            let mut stack = vec![start];
            while let Some(idx) = stack.pop() {
                if discovered[idx] || data[idx].cat {
                    continue;
                }
                discovered[idx] = true;
                if data[idx].neighbours == 0 {
                    for_each_neighbour(width, height, idx, |n| stack.push(n));
                }
            }
        }
        fn hidden_around(
            idx: usize, width: usize, height: usize, discovered: &[bool], flagged: &[bool],
        ) -> (Vec<usize>, usize) {
            let mut hidden = Vec::new();
            let mut flags = 0;
            for_each_neighbour(width, height, idx, |n| {
                if !discovered[n] {
                    if flagged[n] { flags += 1 } else { hidden.push(n) }
                }
            });
            (hidden, flags)
        }

        let n = width * height;
        let mut discovered = vec![false; n];
        let mut flagged = vec![false; n];
        reveal(data, &mut discovered, width, height, start_y * width + start_x);

        loop {
            let mut progress = false;

            for idx in 0..n {
                if !discovered[idx] || data[idx].cat || data[idx].neighbours == 0 {
                    continue;
                }
                let nb = data[idx].neighbours as usize;
                let (hidden, flags) = hidden_around(idx, width, height, &discovered, &flagged);
                if hidden.is_empty() {
                    continue;
                }
                if flags + hidden.len() == nb {
                    for &h in &hidden { flagged[h] = true; progress = true; }
                }
                if flags == nb {
                    for &h in &hidden { reveal(data, &mut discovered, width, height, h); progress = true; }
                }
            }

            let mut constraints: Vec<(Vec<usize>, usize)> = Vec::new();
            for idx in 0..n {
                if !discovered[idx] || data[idx].cat || data[idx].neighbours == 0 {
                    continue;
                }
                let nb = data[idx].neighbours as usize;
                let (hidden, flags) = hidden_around(idx, width, height, &discovered, &flagged);
                if hidden.is_empty() || nb < flags {
                    continue;
                }
                constraints.push((hidden, nb - flags));
            }
            for i in 0..constraints.len() {
                for j in 0..constraints.len() {
                    if i == j { continue; }
                    let (ha, ra) = &constraints[i];
                    let (hb, rb) = &constraints[j];
                    if ra < rb { continue; }
                    let diff_a: Vec<usize> = ha.iter().filter(|c| !hb.contains(c)).copied().collect();
                    let diff_b: Vec<usize> = hb.iter().filter(|c| !ha.contains(c)).copied().collect();
                    if diff_a.is_empty() && diff_b.is_empty() { continue; }
                    if ra - rb == diff_a.len() {
                        for &c in &diff_a {
                            if !flagged[c] && !discovered[c] { flagged[c] = true; progress = true; }
                        }
                        for &c in &diff_b {
                            if !discovered[c] {
                                reveal(data, &mut discovered, width, height, c);
                                progress = true;
                            }
                        }
                    }
                }
            }

            let flags_placed = flagged.iter().filter(|&&f| f).count();
            let remaining = total_cats.saturating_sub(flags_placed);
            let hidden: Vec<usize> = (0..n).filter(|&i| !discovered[i] && !flagged[i]).collect();
            if !hidden.is_empty() {
                if remaining == hidden.len() {
                    for i in hidden { flagged[i] = true; progress = true; }
                } else if remaining == 0 {
                    for i in hidden { reveal(data, &mut discovered, width, height, i); progress = true; }
                }
            }

            if !progress { break; }
        }

        data.iter().zip(discovered.iter()).all(|(f, &d)| f.cat || d)
    }

    fn candidates_for(width: usize, height: usize, start: usize) -> Vec<usize> {
        let mut is_safe = vec![false; width * height];
        is_safe[start] = true;
        for_each_neighbour(width, height, start, |n| is_safe[n] = true);
        (0..width * height).filter(|&i| !is_safe[i]).collect()
    }

    fn generate(width: usize, height: usize, cats: usize, sx: usize, sy: usize) -> Vec<Field> {
        let attempts = AtomicU32::new(0);
        let cancel = AtomicBool::new(false);
        generate_grid_no_guess(width, height, cats, sx, sy, &attempts, &cancel).unwrap()
    }

    #[test]
    fn solver_matches_reference() {
        let mut rng = StdRng::seed_from_u64(1911);
        let mut solvable = 0;
        let mut total = 0;
        for &(w, h, cats) in &[
            (8, 8, 8), (9, 9, 10), (15, 10, 15), (15, 10, 25),
            (16, 16, 40), (20, 15, 45), (27, 20, 120), (30, 16, 99),
        ] {
            for _ in 0..400 {
                let sx = rng.gen_range(0..w);
                let sy = rng.gen_range(0..h);
                let cand = candidates_for(w, h, sy * w + sx);
                let data = random_grid(w, h, cats, &cand, &mut rng);
                let expected = reference_is_solvable(&data, w, h, cats, sx, sy);
                let got = grid_is_solvable(&data, w, h, cats, sx, sy);
                assert_eq!(got, expected, "désaccord sur une grille {w}x{h}, {cats} chats, départ ({sx},{sy})");
                solvable += expected as usize;
                total += 1;
            }
        }
        // Le test n'a de valeur que si les deux issues sont bien représentées.
        assert!(solvable > total / 20 && solvable < total * 19 / 20, "{solvable}/{total}");
    }

    #[test]
    fn random_grid_counts_neighbours() {
        let mut rng = StdRng::seed_from_u64(7);
        let (w, h) = (12, 9);
        let cand: Vec<usize> = (0..w * h).collect();
        let data = random_grid(w, h, 30, &cand, &mut rng);
        assert_eq!(data.iter().filter(|f| f.cat).count(), 30);
        for idx in 0..w * h {
            let mut expected = 0;
            for_each_neighbour(w, h, idx, |n| expected += data[n].cat as u8);
            assert_eq!(data[idx].neighbours, expected);
        }
    }

    #[test]
    fn generated_grid_is_valid() {
        let (w, h, cats) = (20, 15, 60);
        for &(sx, sy) in &[(0, 0), (10, 7), (19, 14), (0, 7)] {
            let data = generate(w, h, cats, sx, sy);
            assert_eq!(data.iter().filter(|f| f.cat).count(), cats);
            let start = sy * w + sx;
            assert!(!data[start].cat);
            assert_eq!(data[start].neighbours, 0);
            assert!(reference_is_solvable(&data, w, h, cats, sx, sy));
        }
    }

    #[test]
    fn generation_can_be_cancelled() {
        let attempts = AtomicU32::new(0);
        let cancel = AtomicBool::new(true);
        assert!(generate_grid_no_guess(40, 25, 250, 5, 5, &attempts, &cancel).is_none());
    }

    #[test]
    fn too_many_cats_does_not_hang() {
        // 3x3 : toutes les cases sont dans la zone sûre du centre.
        let data = generate(3, 3, 5, 1, 1);
        assert_eq!(data.iter().filter(|f| f.cat).count(), 0);
    }

    #[test]
    fn marks_survive_generation_and_block_reveal() {
        let (w, h, cats) = (15, 10, 15);
        let mut model = Model::new();
        model.reset(w, h, cats);
        model.toggle_mark(1, 1); // drapeau, dans la zone sûre du clic en (0,0)
        assert_eq!(model.cats_left(), cats as i32 - 1);
        model.install(generate(w, h, cats, 0, 0));
        model.reveal(0, 0);
        assert_eq!(model.field(1, 1).flag, FLAG_SET);
        assert!(!model.field(1, 1).discovered);
        assert!(model.field(0, 0).discovered);
        assert_eq!(model.cats_left(), cats as i32 - 1);
        // drapeau → ? → rien
        model.toggle_mark(1, 1);
        assert_eq!(model.cats_left(), cats as i32);
        model.toggle_mark(1, 1);
        assert_eq!(model.field(1, 1).flag, FLAG_NONE);
    }

    #[test]
    fn revealing_a_cat_loses() {
        let (w, h, cats) = (15, 10, 15);
        let mut model = Model::new();
        model.reset(w, h, cats);
        model.install(generate(w, h, cats, 7, 5));
        model.reveal(7, 5);
        assert!(model.exploded().is_none());
        let cat = model.data.iter().position(|f| f.cat).unwrap();
        model.reveal(cat % w, cat / w);
        assert_eq!(model.exploded(), Some((cat % w, cat / w)));
        assert!(!model.is_won());
    }

    #[test]
    fn following_hints_wins_without_guessing() {
        for &(w, h, cats) in &[(15, 10, 15), (27, 20, 120)] {
            let mut model = Model::new();
            model.reset(w, h, cats);
            model.install(generate(w, h, cats, w / 2, h / 2));
            model.reveal(w / 2, h / 2);
            let mut steps = 0;
            while !model.is_won() {
                let (x, y) = model.deducible_safe().expect("une case doit toujours être déductible");
                assert!(!model.field(x, y).cat && !model.field(x, y).discovered);
                model.reveal(x, y);
                steps += 1;
                assert!(steps <= w * h);
            }
            assert!(model.exploded().is_none());
            assert!(model.hint().is_none());
            model.flag_remaining_cats();
            assert_eq!(model.cats_left(), 0);
        }
    }

    #[test]
    fn hint_prefers_unmarked_cells() {
        let (w, h, cats) = (20, 15, 25);
        let mut model = Model::new();
        model.reset(w, h, cats);
        model.install(generate(w, h, cats, 10, 7));
        model.reveal(10, 7);
        // Le joueur pose un drapeau (faux) sur chaque case que l'aide propose :
        // elle doit en proposer une autre tant qu'il en reste une non marquée.
        let mut marked = 0;
        while let Some((x, y)) = model.deducible_safe() {
            if model.field(x, y).flag != FLAG_NONE {
                break;
            }
            model.toggle_mark(x, y);
            marked += 1;
            assert!(marked <= w * h);
        }
        assert!(marked >= 1);
        let (x, y) = model.deducible_safe().expect("il reste des cases sûres marquées à tort");
        assert_eq!(model.field(x, y).flag, FLAG_SET);
        assert!(!model.field(x, y).cat);
    }

    #[test]
    fn chord_needs_matching_flags() {
        // Grille 3x3 avec un seul chat dans le coin (0,0).
        let (w, h) = (3, 3);
        let cand = vec![0];
        let mut rng = StdRng::seed_from_u64(1);
        let mut model = Model::new();
        model.reset(w, h, 1);
        model.install(random_grid(w, h, 1, &cand, &mut rng));
        model.reveal(1, 1); // un « 1 »
        assert_eq!(model.field(1, 1).neighbours, 1);
        assert!(!model.chord(1, 1), "pas de drapeau : rien ne doit se passer");
        model.toggle_mark(0, 0);
        assert!(model.chord(1, 1));
        assert!(model.is_won());
    }
}
