use rusqlite::{Connection, Result, params};
use std::path::PathBuf;

/// A single todo item returned from the database.
pub struct Todo {
    pub id: i64,
    pub title: String,
    pub completed: bool,
    pub list_id: Option<i64>,
    pub list_title: Option<String>,
}

/// A list that groups todos.
pub struct List {
    pub id: i64,
    pub title: String,
}

/// Wraps a SQLite connection and exposes todo and list operations.
pub struct TodoDb {
    conn: Connection,
}

impl TodoDb {
    /// Opens (or creates) the database at ~/.todo-mcp/todos.db.
    pub fn open() -> Result<Self> {
        let path = db_path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).ok();
        }
        let conn = Connection::open(&path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS lists (
                id    INTEGER PRIMARY KEY AUTOINCREMENT,
                title TEXT NOT NULL UNIQUE
            );
            CREATE TABLE IF NOT EXISTS todos (
                id        INTEGER PRIMARY KEY AUTOINCREMENT,
                title     TEXT NOT NULL,
                completed INTEGER NOT NULL DEFAULT 0,
                list_id   INTEGER REFERENCES lists(id)
            );",
        )?;
        // Migrate: add list_id column to todos if it was created before lists existed.
        conn.execute_batch(
            "ALTER TABLE todos ADD COLUMN list_id INTEGER REFERENCES lists(id);",
        )
        .ok(); // ignore error if column already exists
        Ok(Self { conn })
    }

    // ── List operations ──────────────────────────────────────────────────────

    /// Create a new list. Returns its id.
    pub fn create_list(&self, title: &str) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO lists (title) VALUES (?1)",
            params![title],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Return all lists ordered by id.
    pub fn all_lists(&self) -> Result<Vec<List>> {
        let mut stmt = self.conn.prepare("SELECT id, title FROM lists ORDER BY id")?;
        let lists = stmt
            .query_map([], |row| Ok(List { id: row.get(0)?, title: row.get(1)? }))?
            .collect::<Result<Vec<_>>>()?;
        Ok(lists)
    }

    /// Resolve a list by id (numeric string) or title. Returns None if not found.
    pub fn find_list(&self, name_or_id: &str) -> Result<Option<List>> {
        // Try numeric id first.
        if let Ok(id) = name_or_id.trim().parse::<i64>() {
            let result = self.conn.query_row(
                "SELECT id, title FROM lists WHERE id = ?1",
                params![id],
                |row| Ok(List { id: row.get(0)?, title: row.get(1)? }),
            );
            match result {
                Ok(list) => return Ok(Some(list)),
                Err(rusqlite::Error::QueryReturnedNoRows) => {}
                Err(e) => return Err(e),
            }
        }
        // Fall back to title match (case-insensitive).
        let result = self.conn.query_row(
            "SELECT id, title FROM lists WHERE lower(title) = lower(?1)",
            params![name_or_id],
            |row| Ok(List { id: row.get(0)?, title: row.get(1)? }),
        );
        match result {
            Ok(list) => Ok(Some(list)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    // ── Todo operations ──────────────────────────────────────────────────────

    /// Insert a new todo, optionally in a list. Returns the new todo id.
    pub fn create(&self, title: &str, list_id: Option<i64>) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO todos (title, list_id) VALUES (?1, ?2)",
            params![title, list_id],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Return todos, optionally filtered by list_id.
    pub fn list(&self, list_id: Option<i64>) -> Result<Vec<Todo>> {
        let sql = "SELECT t.id, t.title, t.completed, t.list_id, l.title
                   FROM todos t LEFT JOIN lists l ON t.list_id = l.id
                   WHERE (?1 IS NULL OR t.list_id = ?1)
                   ORDER BY t.id";
        let mut stmt = self.conn.prepare(sql)?;
        let todos = stmt
            .query_map(params![list_id], |row| {
                Ok(Todo {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    completed: row.get::<_, i64>(2)? != 0,
                    list_id: row.get(3)?,
                    list_title: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>>>()?;
        Ok(todos)
    }

    /// Mark a todo as completed. Returns false if no row matched.
    pub fn complete(&self, id: i64) -> Result<bool> {
        let changed = self.conn.execute(
            "UPDATE todos SET completed = 1 WHERE id = ?1",
            params![id],
        )?;
        Ok(changed > 0)
    }

    /// Permanently delete a todo. Returns false if no row matched.
    pub fn delete(&self, id: i64) -> Result<bool> {
        let changed = self.conn.execute(
            "DELETE FROM todos WHERE id = ?1",
            params![id],
        )?;
        Ok(changed > 0)
    }
}

fn db_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".todo-mcp")
        .join("todos.db")
}
