use rusqlite::{params, Connection, Result};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Score {
    pub name: String,
    pub time: i32,
}

fn db_path() -> PathBuf {
    let mut path = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("bombicat");
    path.push("scores.sqlite");
    path
}

fn open() -> Result<Connection> {
    let path = db_path();
    if let Some(p) = path.parent() {
        let _ = std::fs::create_dir_all(p);
    }
    let conn = Connection::open(path)?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS scores (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            score INTEGER NOT NULL,
            level INTEGER NOT NULL
        )",
        [],
    )?;
    Ok(conn)
}

pub fn insert(name: &str, time: i32, level: usize) -> Result<()> {
    let conn = open()?;
    conn.execute(
        "INSERT INTO scores (name, score, level) VALUES (?1, ?2, ?3)",
        params![name, time, level as i32],
    )?;
    Ok(())
}

pub fn top_ten(level: usize) -> Vec<Score> {
    let Ok(conn) = open() else { return vec![] };
    let Ok(mut stmt) = conn.prepare(
        "SELECT name, score FROM scores WHERE level = ?1 ORDER BY score ASC LIMIT 10",
    ) else {
        return vec![];
    };
    stmt.query_map(params![level as i32], |row| {
        Ok(Score { name: row.get(0)?, time: row.get(1)? })
    })
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
}

pub fn reset_all() -> Result<()> {
    let conn = open()?;
    conn.execute("DELETE FROM scores", [])?;
    Ok(())
}
