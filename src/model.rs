use rand::prelude::*;
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

/// Niveau de raisonnement exigé du joueur par les grilles générées.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Logic {
    /// Un chiffre seul, ou deux chiffres voisins comparés.
    #[default]
    Basic,
    /// En plus : étude de tous les cas possibles sur une zone de la frontière.
    Advanced,
}

// ── modèle de jeu ─────────────────────────────────────────────────────────

pub struct Model {
    logic: Logic,
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
            logic: Logic::Basic,
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
    pub fn reset(&mut self, width: usize, height: usize, cats: usize, logic: Logic) {
        self.logic = logic;
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

    /// Fait tourner la marque d'une case cachée : rien → drapeau → `?` → rien
    /// (sans l'étape `?` si `allow_question` est faux).
    pub fn toggle_mark(&mut self, x: usize, y: usize, allow_question: bool) {
        let f = &mut self.data[y * self.width + x];
        if f.discovered {
            return;
        }
        if f.flag == FLAG_SET {
            self.flag_count -= 1;
        }
        f.flag = match f.flag {
            FLAG_NONE => FLAG_SET,
            FLAG_SET if allow_question => FLAG_QUESTION,
            _ => FLAG_NONE,
        };
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
        solver.logic = self.logic;
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

/// Calcule le nombre de voisins de chaque case à partir des chats.
fn build_fields(width: usize, height: usize, cats: &[bool]) -> Vec<Field> {
    let mut data = vec![Field::default(); width * height];
    for (idx, _) in cats.iter().enumerate().filter(|(_, &c)| c) {
        data[idx].cat = true;
        for_each_neighbour(width, height, idx, |n| data[n].neighbours += 1);
    }
    data
}

/// Place `count` chats parmi `candidates` (tirage sans remise).
fn place_cats(cats: &mut [bool], candidates: &[usize], count: usize, rng: &mut impl Rng) {
    for pick in rand::seq::index::sample(rng, candidates.len(), count) {
        cats[candidates[pick]] = true;
    }
}

#[cfg(test)]
fn random_grid(
    width: usize, height: usize, count: usize,
    candidates: &[usize], rng: &mut impl Rng,
) -> Vec<Field> {
    let mut cats = vec![false; width * height];
    place_cats(&mut cats, candidates, count, rng);
    build_fields(width, height, &cats)
}

/// Résultat d'une tentative de résolution.
struct Outcome {
    solved: bool,
    /// Nombre de cases que le solveur a pu découvrir.
    discovered: usize,
    /// Cases sur lesquelles le solveur n'a rien pu prouver.
    unknown: Vec<bool>,
}

fn solve(
    data: &[Field], width: usize, height: usize,
    total_cats: usize, start: usize, logic: Logic,
) -> Outcome {
    let mut solver =
        Solver::with_state(data, width, height, total_cats, vec![false; width * height]);
    solver.logic = logic;
    solver.mark_safe(start);
    solver.run();
    Outcome {
        solved: solver.is_solved(),
        discovered: solver.n_discovered,
        unknown: (0..width * height)
            .map(|i| !solver.discovered[i] && !solver.flagged[i])
            .collect(),
    }
}

#[cfg(test)]
fn grid_is_solvable(
    data: &[Field], width: usize, height: usize,
    total_cats: usize, start_x: usize, start_y: usize, logic: Logic,
) -> bool {
    solve(data, width, height, total_cats, start_y * width + start_x, logic).solved
}

/// Nombre de retouches tentées sur une grille avant de repartir de zéro :
/// une par tranche de 100 cases. Plus on retouche, plus c'est rapide, mais
/// plus les grilles s'écartent d'un tirage au hasard (elles deviennent plus
/// « aérées ») ; cette limite garde l'écart autour de 2 %.
fn max_repairs(cells: usize) -> usize {
    (cells / 100).max(1)
}

/// Demi-côté du carré de cases redistribuées à chaque retouche.
const REPAIR_RADIUS: usize = 2;

/// Une tentative : part d'un placement aléatoire puis le *retouche* tant que
/// le solveur reste bloqué. Chaque retouche redistribue au hasard les chats
/// d'un petit carré autour d'un point de blocage et n'est gardée que si le
/// solveur va au moins aussi loin qu'avant.
///
/// C'est bien plus efficace que de jeter toute la grille au premier blocage :
/// la partie déjà résoluble est conservée.
#[allow(clippy::too_many_arguments)]
fn try_generate(
    width: usize, height: usize, count: usize, start: usize,
    candidates: &[usize], is_start_zone: &[bool], logic: Logic,
    rng: &mut impl Rng, attempts: &AtomicU32, cancel: &AtomicBool,
) -> Option<Vec<Field>> {
    let n = width * height;
    let mut cats = vec![false; n];
    place_cats(&mut cats, candidates, count, rng);
    let mut data = build_fields(width, height, &cats);
    attempts.fetch_add(1, Ordering::Relaxed);
    let mut outcome = solve(&data, width, height, count, start, logic);

    let mut stuck: Vec<usize> = Vec::new();
    let mut zone: Vec<usize> = Vec::new();
    for _ in 0..max_repairs(n) {
        if outcome.solved {
            return Some(data);
        }
        if cancel.load(Ordering::Relaxed) {
            return None;
        }

        // Zone à retoucher : un petit carré autour d'une case indécise prise
        // au hasard au bord de la partie résolue. Le reste de la grille ne
        // bouge pas, pour rester aussi proche que possible d'un tirage au hasard.
        stuck.clear();
        stuck.extend((0..n).filter(|&i| outcome.unknown[i] && {
            let mut frontier = false;
            for_each_neighbour(width, height, i, |nb| frontier |= !outcome.unknown[nb]);
            frontier
        }));
        if stuck.is_empty() {
            stuck.extend((0..n).filter(|&i| outcome.unknown[i]));
        }
        let center = stuck[rng.gen_range(0..stuck.len())];
        let (cx, cy) = (center % width, center / width);
        zone.clear();
        for y in cy.saturating_sub(REPAIR_RADIUS)..=(cy + REPAIR_RADIUS).min(height - 1) {
            for x in cx.saturating_sub(REPAIR_RADIUS)..=(cx + REPAIR_RADIUS).min(width - 1) {
                let i = y * width + x;
                if !is_start_zone[i] {
                    zone.push(i);
                }
            }
        }
        let zone_cats = zone.iter().filter(|&&i| cats[i]).count();

        let mut candidate = cats.clone();
        for &i in &zone {
            candidate[i] = false;
        }
        place_cats(&mut candidate, &zone, zone_cats, rng);

        let candidate_data = build_fields(width, height, &candidate);
        attempts.fetch_add(1, Ordering::Relaxed);
        let candidate_outcome = solve(&candidate_data, width, height, count, start, logic);
        if candidate_outcome.discovered >= outcome.discovered {
            cats = candidate;
            data = candidate_data;
            outcome = candidate_outcome;
        }
    }
    outcome.solved.then_some(data)
}

/// Génère une grille garantie résoluble par logique pure depuis la case de
/// départ, qui est toujours vide (elle et ses 8 voisines sont sans chat).
///
/// `attempts` est incrémenté à chaque grille testée (lisible depuis un autre thread).
/// Renvoie `None` si `cancel` passe à `true` avant qu'une grille soit trouvée.
#[allow(clippy::too_many_arguments)]
pub fn generate_grid_no_guess(
    width: usize, height: usize, total_cats: usize,
    start_x: usize, start_y: usize,
    logic: Logic,
    attempts: &AtomicU32,
    cancel: &AtomicBool,
) -> Option<Vec<Field>> {
    let size = width * height;
    let start = start_y * width + start_x;

    let mut is_start_zone = vec![false; size];
    is_start_zone[start] = true;
    for_each_neighbour(width, height, start, |n| is_start_zone[n] = true);
    let candidates: Vec<usize> = (0..size).filter(|&i| !is_start_zone[i]).collect();
    let count = total_cats.min(candidates.len());

    let mut rng = StdRng::from_entropy();
    loop {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        let result = try_generate(
            width, height, count, start, &candidates, &is_start_zone,
            logic, &mut rng, attempts, cancel,
        );
        if result.is_some() {
            return result;
        }
    }
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
/// 3. le compte global de chats restants ;
/// 4. (`Logic::Advanced`) quand tout le reste est bloqué, l'étude de tous
///    les placements possibles sur chaque zone de la frontière.
struct Solver<'a> {
    logic: Logic,
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
            logic: Logic::Basic,
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
        debug_assert!(!self.data[idx].cat, "déduction fausse : case sûre qui est un chat");
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
        debug_assert!(self.data[idx].cat, "déduction fausse : chat qui n'en est pas un");
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
        loop {
            self.run_basic();
            if self.logic == Logic::Basic || self.is_solved() {
                return;
            }
            // L'aide préfère une déduction simple quand il y en a une.
            if self.collect_safe && !self.safe_found.is_empty() {
                return;
            }
            if !self.enumerate() {
                return;
            }
        }
    }

    /// Règle 4 : pour chaque zone de cases cachées reliées par des chiffres,
    /// essaie tous les placements de chats compatibles avec ces chiffres (et
    /// avec le nombre de chats restants). Une case sans chat dans tous les
    /// placements est sûre ; une case avec un chat dans tous est un chat.
    fn enumerate(&mut self) -> bool {
        const NONE: u32 = u32::MAX;
        let n = self.width * self.height;

        // Cases de la frontière et contraintes, en numérotation locale.
        let mut local = vec![NONE; n];
        let mut cells: Vec<usize> = Vec::new();
        let mut constraints: Vec<(Vec<u32>, u8)> = Vec::new();
        for idx in 0..n {
            let c = self.constraint_at(idx);
            if c.len == 0 {
                continue;
            }
            let ids = c.cells().iter().map(|&cell| {
                let cell = cell as usize;
                if local[cell] == NONE {
                    local[cell] = cells.len() as u32;
                    cells.push(cell);
                }
                local[cell]
            }).collect();
            constraints.push((ids, c.remaining));
        }
        if cells.is_empty() {
            return false;
        }

        // Zones indépendantes : cases reliées par une contrainte commune.
        let mut parent: Vec<u32> = (0..cells.len() as u32).collect();
        fn find(parent: &mut [u32], mut i: u32) -> u32 {
            while parent[i as usize] != i {
                parent[i as usize] = parent[parent[i as usize] as usize];
                i = parent[i as usize];
            }
            i
        }
        for (ids, _) in &constraints {
            let root = find(&mut parent, ids[0]);
            for &id in &ids[1..] {
                let other = find(&mut parent, id);
                parent[other as usize] = root;
            }
        }
        let mut zone_of_root = vec![NONE; cells.len()];
        let mut zones: Vec<Zone> = Vec::new();
        for id in 0..cells.len() as u32 {
            let root = find(&mut parent, id) as usize;
            if zone_of_root[root] == NONE {
                zone_of_root[root] = zones.len() as u32;
                zones.push(Zone::default());
            }
            let zone = &mut zones[zone_of_root[root] as usize];
            local[cells[id as usize]] = zone.cells.len() as u32; // rang dans la zone
            zone.cells.push(cells[id as usize]);
        }
        for (ids, remaining) in constraints {
            let root = find(&mut parent, ids[0]) as usize;
            let zone = &mut zones[zone_of_root[root] as usize];
            let members = ids.iter().map(|&id| local[cells[id as usize]]).collect();
            zone.constraints.push((members, remaining));
        }

        // Bornes globales : le nombre de chats d'une zone ne peut ni dépasser
        // ce qu'il reste, ni en laisser plus que les autres cases cachées.
        let hidden = n - self.n_discovered - self.n_flagged;
        let remaining = self.total_cats.saturating_sub(self.n_flagged);

        let mut progress = false;
        for zone in &zones {
            let outside = hidden - zone.cells.len();
            let min_cats = remaining.saturating_sub(outside);
            let Some(possible) = zone.possibilities(min_cats, remaining) else { continue };
            for (rank, &cell) in zone.cells.iter().enumerate() {
                match possible[rank] {
                    Possible::NEVER => progress |= self.mark_safe(cell),
                    Possible::ALWAYS => progress |= self.mark_cat(cell),
                    _ => {}
                }
            }
        }
        progress
    }

    fn run_basic(&mut self) {
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

/// Ce qu'on a vu pour une case au fil des placements possibles.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Possible(u8);

impl Possible {
    const UNSEEN: Possible = Possible(0);
    /// Sans chat dans tous les placements.
    const NEVER: Possible = Possible(1);
    /// Avec un chat dans tous les placements.
    const ALWAYS: Possible = Possible(2);
}

/// Zone de la frontière : des cases cachées et les chiffres qui les relient.
#[derive(Default)]
struct Zone {
    /// Indices des cases dans la grille.
    cells: Vec<usize>,
    /// (rangs des cases dans `cells`, chats à y placer).
    constraints: Vec<(Vec<u32>, u8)>,
}

impl Zone {
    /// Au-delà, la zone est abandonnée (le solveur ne conclut rien dessus).
    const NODE_BUDGET: u32 = 20_000;

    /// Parcourt tous les placements compatibles et dit, pour chaque case,
    /// si elle est toujours / jamais / parfois un chat.
    /// `None` si la zone est trop grosse pour être explorée.
    fn possibilities(&self, min_cats: usize, max_cats: usize) -> Option<Vec<Possible>> {
        let k = self.cells.len();
        let mut of_cell: Vec<Vec<u32>> = vec![Vec::new(); k];
        for (ci, (members, _)) in self.constraints.iter().enumerate() {
            for &m in members {
                of_cell[m as usize].push(ci as u32);
            }
        }
        let mut search = Search {
            of_cell,
            need: self.constraints.iter().map(|(_, r)| *r as i16).collect(),
            free: self.constraints.iter().map(|(m, _)| m.len() as i16).collect(),
            assigned: vec![false; k],
            seen: vec![Possible::UNSEEN; k],
            min_cats,
            max_cats,
            nodes: 0,
        };
        search.visit(0, 0).then_some(search.seen)
    }
}

struct Search {
    of_cell: Vec<Vec<u32>>,
    /// Par contrainte : chats encore à placer / cases pas encore décidées.
    need: Vec<i16>,
    free: Vec<i16>,
    assigned: Vec<bool>,
    seen: Vec<Possible>,
    min_cats: usize,
    max_cats: usize,
    nodes: u32,
}

impl Search {
    /// Renvoie `false` si le budget est épuisé.
    fn visit(&mut self, rank: usize, cats: usize) -> bool {
        self.nodes += 1;
        if self.nodes > Zone::NODE_BUDGET {
            return false;
        }
        let k = self.assigned.len();
        if rank == k {
            if cats >= self.min_cats {
                for (i, &is_cat) in self.assigned.iter().enumerate() {
                    self.seen[i].0 |= if is_cat { Possible::ALWAYS.0 } else { Possible::NEVER.0 };
                }
            }
            return true;
        }
        // Assez de cases restantes pour atteindre le minimum de chats ?
        if cats + (k - rank) < self.min_cats {
            return true;
        }
        for is_cat in [false, true] {
            if is_cat && cats + 1 > self.max_cats {
                continue;
            }
            let v = is_cat as i16;
            let mut ok = true;
            for &ci in &self.of_cell[rank] {
                let ci = ci as usize;
                self.free[ci] -= 1;
                self.need[ci] -= v;
                ok &= self.need[ci] >= 0 && self.need[ci] <= self.free[ci];
            }
            let completed = if ok {
                self.assigned[rank] = is_cat;
                self.visit(rank + 1, cats + is_cat as usize)
            } else {
                true
            };
            for &ci in &self.of_cell[rank] {
                let ci = ci as usize;
                self.free[ci] += 1;
                self.need[ci] += v;
            }
            if !completed {
                return false;
            }
        }
        true
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
        generate_grid_no_guess(width, height, cats, sx, sy, Logic::Basic, &attempts, &cancel).unwrap()
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
                let got = grid_is_solvable(&data, w, h, cats, sx, sy, Logic::Basic);
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
        assert!(generate_grid_no_guess(40, 25, 250, 5, 5, Logic::Basic, &attempts, &cancel).is_none());
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
        model.reset(w, h, cats, Logic::Basic);
        model.toggle_mark(1, 1, true); // drapeau, dans la zone sûre du clic en (0,0)
        assert_eq!(model.cats_left(), cats as i32 - 1);
        model.install(generate(w, h, cats, 0, 0));
        model.reveal(0, 0);
        assert_eq!(model.field(1, 1).flag, FLAG_SET);
        assert!(!model.field(1, 1).discovered);
        assert!(model.field(0, 0).discovered);
        assert_eq!(model.cats_left(), cats as i32 - 1);
        // drapeau → ? → rien
        model.toggle_mark(1, 1, true);
        assert_eq!(model.cats_left(), cats as i32);
        model.toggle_mark(1, 1, true);
        assert_eq!(model.field(1, 1).flag, FLAG_NONE);
        // sans points d'interrogation : drapeau → rien
        model.toggle_mark(1, 1, false);
        assert_eq!(model.field(1, 1).flag, FLAG_SET);
        model.toggle_mark(1, 1, false);
        assert_eq!(model.field(1, 1).flag, FLAG_NONE);
        assert_eq!(model.cats_left(), cats as i32);
    }

    #[test]
    fn revealing_a_cat_loses() {
        let (w, h, cats) = (15, 10, 15);
        let mut model = Model::new();
        model.reset(w, h, cats, Logic::Basic);
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
            model.reset(w, h, cats, Logic::Basic);
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
        model.reset(w, h, cats, Logic::Basic);
        model.install(generate(w, h, cats, 10, 7));
        model.reveal(10, 7);
        // Le joueur pose un drapeau (faux) sur chaque case que l'aide propose :
        // elle doit en proposer une autre tant qu'il en reste une non marquée.
        let mut marked = 0;
        while let Some((x, y)) = model.deducible_safe() {
            if model.field(x, y).flag != FLAG_NONE {
                break;
            }
            model.toggle_mark(x, y, true);
            marked += 1;
            assert!(marked <= w * h);
        }
        assert!(marked >= 1);
        let (x, y) = model.deducible_safe().expect("il reste des cases sûres marquées à tort");
        assert_eq!(model.field(x, y).flag, FLAG_SET);
        assert!(!model.field(x, y).cat);
    }

    #[test]
    fn advanced_logic_solves_more_and_stays_sound() {
        // Les `debug_assert!` de `mark_safe` / `mark_cat` vérifient que
        // chaque déduction est juste (lancer `cargo test` sans `--release`).
        let mut rng = StdRng::seed_from_u64(42);
        let (mut basic, mut advanced) = (0, 0);
        for &(w, h, cats) in &[(9, 9, 10), (16, 16, 40), (30, 16, 99), (27, 20, 120)] {
            for _ in 0..300 {
                let sx = rng.gen_range(0..w);
                let sy = rng.gen_range(0..h);
                let cand = candidates_for(w, h, sy * w + sx);
                let data = random_grid(w, h, cats, &cand, &mut rng);
                let b = grid_is_solvable(&data, w, h, cats, sx, sy, Logic::Basic);
                let a = grid_is_solvable(&data, w, h, cats, sx, sy, Logic::Advanced);
                assert!(a || !b, "le solveur avancé doit résoudre tout ce que le simple résout");
                basic += b as usize;
                advanced += a as usize;
            }
        }
        assert!(advanced > basic, "{advanced} contre {basic}");
    }

    #[test]
    fn advanced_grids_can_be_played_with_hints() {
        let (w, h, cats) = (27, 20, 120);
        let attempts = AtomicU32::new(0);
        let cancel = AtomicBool::new(false);
        let data = generate_grid_no_guess(w, h, cats, 3, 3, Logic::Advanced, &attempts, &cancel).unwrap();
        assert_eq!(data.iter().filter(|f| f.cat).count(), cats);
        let mut model = Model::new();
        model.reset(w, h, cats, Logic::Advanced);
        model.install(data);
        model.reveal(3, 3);
        while !model.is_won() {
            let (x, y) = model.deducible_safe().expect("une case doit toujours être déductible");
            model.reveal(x, y);
            assert!(model.exploded().is_none());
        }
    }

    #[test]
    fn chord_needs_matching_flags() {
        // Grille 3x3 avec un seul chat dans le coin (0,0).
        let (w, h) = (3, 3);
        let cand = vec![0];
        let mut rng = StdRng::seed_from_u64(1);
        let mut model = Model::new();
        model.reset(w, h, 1, Logic::Basic);
        model.install(random_grid(w, h, 1, &cand, &mut rng));
        model.reveal(1, 1); // un « 1 »
        assert_eq!(model.field(1, 1).neighbours, 1);
        assert!(!model.chord(1, 1), "pas de drapeau : rien ne doit se passer");
        model.toggle_mark(0, 0, true);
        assert!(model.chord(1, 1));
        assert!(model.is_won());
    }
}
