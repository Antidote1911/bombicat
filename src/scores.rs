use rusqlite::{params, Connection, Result};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub struct Score {
    pub name: String,
    pub time_ms: i64,
}

/// Les dix meilleurs temps d'un niveau pour un jeu de règles donné.
#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    /// Signature des règles (voir `Preferences::rules`), ex. `20x15/40`.
    pub rules: String,
    pub scores: Vec<Score>,
}

/// Règles en vigueur jusqu'à la version 1.1, par niveau : les scores
/// enregistrés avant l'ajout de la colonne `rules` leur sont rattachés.
const LEGACY_RULES: [&str; 6] = [
    "15x10/15", "20x15/25", "25x15/35", "27x20/120", "35x22/190", "40x25/250",
];

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
    init(&conn)?;
    Ok(conn)
}

/// Crée la table si besoin et met à niveau une base d'une version précédente.
/// Aucun score n'est supprimé ; la colonne `score` (secondes) est conservée
/// pour qu'une ancienne version du jeu puisse toujours lire la base.
fn init(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS scores (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            score INTEGER NOT NULL,
            level INTEGER NOT NULL
        )",
        [],
    )?;

    let columns: Vec<String> = conn
        .prepare("SELECT name FROM pragma_table_info('scores')")?
        .query_map([], |row| row.get(0))?
        .collect::<Result<_>>()?;
    let has = |name: &str| columns.iter().any(|c| c == name);

    if !has("time_ms") {
        conn.execute("ALTER TABLE scores ADD COLUMN time_ms INTEGER", [])?;
    }
    if !has("rules") {
        conn.execute("ALTER TABLE scores ADD COLUMN rules TEXT", [])?;
    }
    conn.execute("UPDATE scores SET time_ms = score * 1000 WHERE time_ms IS NULL", [])?;
    for (level, rules) in LEGACY_RULES.iter().enumerate() {
        conn.execute(
            "UPDATE scores SET rules = ?1 WHERE rules IS NULL AND level = ?2",
            params![rules, level as i64],
        )?;
    }
    conn.execute("UPDATE scores SET rules = '?' WHERE rules IS NULL", [])?;
    Ok(())
}

fn insert_into(conn: &Connection, name: &str, time_ms: i64, level: usize, rules: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO scores (name, score, level, time_ms, rules) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![name, time_ms / 1000, level as i64, time_ms, rules],
    )?;
    Ok(())
}

/// Tous les classements d'un niveau : celui des règles `current` en premier
/// (même vide), puis ceux obtenus avec d'autres règles.
fn tables_from(conn: &Connection, level: usize, current: &str) -> Result<Vec<Table>> {
    let mut tables = vec![Table { rules: current.to_string(), scores: Vec::new() }];
    let mut stmt = conn.prepare(
        "SELECT name, time_ms, rules FROM scores WHERE level = ?1 ORDER BY time_ms ASC, id ASC",
    )?;
    let rows = stmt.query_map(params![level as i64], |row| {
        Ok((Score { name: row.get(0)?, time_ms: row.get(1)? }, row.get::<_, String>(2)?))
    })?;
    for row in rows {
        let (score, rules) = row?;
        let index = match tables.iter().position(|t| t.rules == rules) {
            Some(index) => index,
            None => {
                tables.push(Table { rules, scores: Vec::new() });
                tables.len() - 1
            }
        };
        if tables[index].scores.len() < 10 {
            tables[index].scores.push(score);
        }
    }
    Ok(tables)
}

pub fn insert(name: &str, time_ms: i64, level: usize, rules: &str) -> Result<()> {
    insert_into(&open()?, name, time_ms, level, rules)
}

pub fn tables(level: usize, current: &str) -> Vec<Table> {
    open()
        .and_then(|conn| tables_from(&conn, level, current))
        .unwrap_or_else(|_| vec![Table { rules: current.to_string(), scores: Vec::new() }])
}

pub fn reset_all() -> Result<()> {
    let conn = open()?;
    conn.execute("DELETE FROM scores", [])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Tous les tests utilisent une base en mémoire : jamais celle du joueur.

    #[test]
    fn old_database_is_upgraded_without_losing_scores() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE scores (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL, score INTEGER NOT NULL, level INTEGER NOT NULL
             );
             INSERT INTO scores (name, score, level) VALUES
                ('Ana', 42, 1), ('Bob', 37, 1), ('Cy', 300, 5);",
        )
        .unwrap();

        init(&conn).unwrap();
        init(&conn).unwrap(); // rejouable sans effet

        let count: i64 = conn.query_row("SELECT COUNT(*) FROM scores", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 3);

        // Niveau 1 : les règles ont changé (25 → 40 chats) ; les anciens
        // scores restent visibles, à part.
        let tables = tables_from(&conn, 1, "20x15/40").unwrap();
        assert_eq!(tables.len(), 2);
        assert_eq!(tables[0], Table { rules: "20x15/40".into(), scores: vec![] });
        assert_eq!(tables[1].rules, "20x15/25");
        assert_eq!(
            tables[1].scores,
            vec![
                Score { name: "Bob".into(), time_ms: 37_000 },
                Score { name: "Ana".into(), time_ms: 42_000 },
            ]
        );

        // Niveau 5 : règles inchangées, l'ancien score reste dans le classement.
        let tables = tables_from(&conn, 5, "40x25/250").unwrap();
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].scores, vec![Score { name: "Cy".into(), time_ms: 300_000 }]);
    }

    #[test]
    fn scores_are_sorted_to_the_millisecond_and_capped_at_ten() {
        let conn = Connection::open_in_memory().unwrap();
        init(&conn).unwrap();
        for i in 0..12 {
            insert_into(&conn, &format!("J{i}"), 50_000 - i * 100, 0, "15x10/15").unwrap();
        }
        insert_into(&conn, "Avancé", 1_000, 0, "15x10/15+").unwrap();

        let tables = tables_from(&conn, 0, "15x10/15").unwrap();
        assert_eq!(tables[0].scores.len(), 10);
        assert_eq!(tables[0].scores[0], Score { name: "J11".into(), time_ms: 48_900 });
        assert_eq!(tables[1].rules, "15x10/15+");
        assert_eq!(tables[1].scores.len(), 1);

        // La colonne historique en secondes reste renseignée.
        let secs: i64 = conn
            .query_row("SELECT score FROM scores WHERE name = 'J11'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(secs, 48);
    }
}
