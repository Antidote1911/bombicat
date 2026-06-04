use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, PartialEq)]
pub enum SolvabilityStatus {
    Testing,
    Solvable { count: usize, total: usize },
    Unsolvable { total: usize },
}

pub const MIN_W: usize = 5;
pub const MAX_W: usize = 60;
pub const MIN_H: usize = 5;
pub const MAX_H: usize = 40;

#[derive(Serialize, Deserialize)]
struct GridFile {
    version: u32,
    width: usize,
    height: usize,
    bombs: Vec<[usize; 2]>,
}

pub struct EditorState {
    pub width: usize,
    pub height: usize,
    pub width_buf: String,
    pub height_buf: String,
    pub bombs: Vec<bool>,
    pub path: Option<PathBuf>,
    pub dirty: bool,
    pub error: Option<String>,
    pub drag_value: Option<bool>,
    pub solvability: Option<SolvabilityStatus>,
    pub changed: bool,
}

impl EditorState {
    pub fn new() -> Self {
        let (w, h) = (20usize, 15usize);
        Self {
            width: w, height: h,
            width_buf: w.to_string(),
            height_buf: h.to_string(),
            bombs: vec![false; w * h],
            path: None, dirty: false, error: None,
            drag_value: None, solvability: None, changed: false,
        }
    }

    pub fn bomb_count(&self) -> usize {
        self.bombs.iter().filter(|&&b| b).count()
    }

    pub fn apply_size(&mut self) -> bool {
        let nw = self.width_buf.parse::<usize>().unwrap_or(self.width).clamp(MIN_W, MAX_W);
        let nh = self.height_buf.parse::<usize>().unwrap_or(self.height).clamp(MIN_H, MAX_H);
        if nw == self.width && nh == self.height { return false; }
        let (ow, oh) = (self.width, self.height);
        let mut nb = vec![false; nw * nh];
        for y in 0..oh.min(nh) {
            for x in 0..ow.min(nw) {
                nb[y * nw + x] = self.bombs[y * ow + x];
            }
        }
        self.width = nw; self.height = nh;
        self.width_buf = nw.to_string(); self.height_buf = nh.to_string();
        self.bombs = nb;
        self.dirty = true; self.error = None; self.solvability = None; self.changed = true;
        true
    }

    pub fn paint(&mut self, x: usize, y: usize, value: bool) {
        if x < self.width && y < self.height {
            let idx = y * self.width + x;
            if self.bombs[idx] != value {
                self.bombs[idx] = value;
                self.dirty = true;
                self.solvability = None;
                self.changed = true;
            }
        }
    }

    pub fn clear(&mut self) {
        self.bombs.fill(false);
        self.dirty = true;
        self.solvability = None;
        self.changed = true;
    }

    pub fn save(&mut self, path: PathBuf) {
        let gf = GridFile {
            version: 1,
            width: self.width, height: self.height,
            bombs: self.bombs.iter().enumerate()
                .filter(|(_, &b)| b)
                .map(|(i, _)| [i % self.width, i / self.width])
                .collect(),
        };
        match serde_json::to_string_pretty(&gf) {
            Ok(json) => match std::fs::write(&path, json) {
                Ok(()) => { self.path = Some(path); self.dirty = false; self.error = None; }
                Err(e) => self.error = Some(format!("Erreur d'écriture : {e}")),
            },
            Err(e) => self.error = Some(format!("Erreur : {e}")),
        }
    }

    pub fn load(path: PathBuf) -> Result<Self, String> {
        let json = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let gf: GridFile = serde_json::from_str(&json).map_err(|e| e.to_string())?;
        let w = gf.width.clamp(MIN_W, MAX_W);
        let h = gf.height.clamp(MIN_H, MAX_H);
        let mut bombs = vec![false; w * h];
        for [x, y] in &gf.bombs {
            if *x < w && *y < h { bombs[y * w + x] = true; }
        }
        Ok(Self {
            width: w, height: h,
            width_buf: w.to_string(), height_buf: h.to_string(),
            bombs, path: Some(path), dirty: false, error: None,
            drag_value: None, solvability: None, changed: true,
        })
    }
}
