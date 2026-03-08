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
        Self::init(conn)
    }

    /// Opens an in-memory database (useful for tests).
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::init(conn)
    }

    fn init(conn: Connection) -> Result<Self> {
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
        conn.execute_batch("ALTER TABLE todos ADD COLUMN list_id INTEGER REFERENCES lists(id);")
            .ok(); // ignore error if column already exists
        Ok(Self { conn })
    }

    // ── List operations ──────────────────────────────────────────────────────

    /// Create a new list. Returns its id.
    pub fn create_list(&self, title: &str) -> Result<i64> {
        // Validate title is not empty or whitespace.
        if title.trim().is_empty() {
            return Err(rusqlite::Error::InvalidQuery); // or a custom error type
        }
        self.conn
            .execute("INSERT INTO lists (title) VALUES (?1)", params![title])?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Return all lists ordered by id.
    pub fn all_lists(&self) -> Result<Vec<List>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, title FROM lists ORDER BY id")?;
        let lists = stmt
            .query_map([], |row| {
                Ok(List {
                    id: row.get(0)?,
                    title: row.get(1)?,
                })
            })?
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
                |row| {
                    Ok(List {
                        id: row.get(0)?,
                        title: row.get(1)?,
                    })
                },
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
            |row| {
                Ok(List {
                    id: row.get(0)?,
                    title: row.get(1)?,
                })
            },
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
        let changed = self
            .conn
            .execute("UPDATE todos SET completed = 1 WHERE id = ?1", params![id])?;
        Ok(changed > 0)
    }

    /// Permanently delete a todo. Returns false if no row matched.
    pub fn delete(&self, id: i64) -> Result<bool> {
        let changed = self
            .conn
            .execute("DELETE FROM todos WHERE id = ?1", params![id])?;
        Ok(changed > 0)
    }
}

fn db_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".todo-mcp")
        .join("todos.db")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> TodoDb {
        TodoDb::open_in_memory().expect("in-memory db")
    }

    // ── List tests ────────────────────────────────────────────────────────────

    #[test]
    fn create_list_returns_id() {
        let db = db();
        let id = db.create_list("Inbox").unwrap();
        assert!(id > 0);
    }

    #[test]
    fn create_list_duplicate_title_errors() {
        let db = db();
        db.create_list("Inbox").unwrap();
        assert!(db.create_list("Inbox").is_err());
    }

    #[test]
    fn create_list_no_title_errors() {
        let db = db();
        assert!(db.create_list("").is_err());
    }

    #[test]
    fn all_lists_empty() {
        let db = db();
        assert!(db.all_lists().unwrap().is_empty());
    }

    #[test]
    fn all_lists_returns_all_in_order() {
        let db = db();
        db.create_list("Today").unwrap();
        db.create_list("Tomorrow").unwrap();
        let lists = db.all_lists().unwrap();
        assert_eq!(lists.len(), 2);
        assert_eq!(lists[0].title, "Today");
        assert_eq!(lists[1].title, "Tomorrow");
    }

    #[test]
    fn find_list_by_numeric_id() {
        let db = db();
        let id = db.create_list("Work").unwrap();
        let found = db.find_list(&id.to_string()).unwrap().unwrap();
        assert_eq!(found.title, "Work");
    }

    #[test]
    fn find_list_by_title() {
        let db = db();
        db.create_list("Inbox").unwrap();
        let found = db.find_list("Inbox").unwrap().unwrap();
        assert_eq!(found.title, "Inbox");
    }

    #[test]
    fn find_list_case_insensitive() {
        let db = db();
        db.create_list("Inbox").unwrap();
        let found = db.find_list("INBOX").unwrap().unwrap();
        assert_eq!(found.title, "Inbox");
    }

    #[test]
    fn find_list_not_found_returns_none() {
        let db = db();
        assert!(db.find_list("ghost").unwrap().is_none());
    }

    // ── Todo tests ────────────────────────────────────────────────────────────

    #[test]
    fn create_todo_without_list() {
        let db = db();
        let id = db.create("Buy milk", None).unwrap();
        assert!(id > 0);
        let todos = db.list(None).unwrap();
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].title, "Buy milk");
        assert!(todos[0].list_id.is_none());
    }

    #[test]
    fn create_todo_with_list() {
        let db = db();
        let list_id = db.create_list("Errands").unwrap();
        let todo_id = db.create("Buy milk", Some(list_id)).unwrap();
        assert!(todo_id > 0);
        let todos = db.list(Some(list_id)).unwrap();
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].list_id, Some(list_id));
    }

    #[test]
    fn list_unfiltered_returns_all() {
        let db = db();
        let list_id = db.create_list("A").unwrap();
        db.create("One", None).unwrap();
        db.create("Two", Some(list_id)).unwrap();
        assert_eq!(db.list(None).unwrap().len(), 2);
    }

    #[test]
    fn list_filtered_by_list_id() {
        let db = db();
        let list_a = db.create_list("A").unwrap();
        let list_b = db.create_list("B").unwrap();
        db.create("In A", Some(list_a)).unwrap();
        db.create("In B", Some(list_b)).unwrap();
        let result = db.list(Some(list_a)).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].title, "In A");
    }

    #[test]
    fn complete_marks_todo() {
        let db = db();
        let id = db.create("Task", None).unwrap();
        assert!(db.complete(id).unwrap());
        let todos = db.list(None).unwrap();
        assert!(todos[0].completed);
    }

    #[test]
    fn complete_returns_false_for_unknown_id() {
        let db = db();
        assert!(!db.complete(999).unwrap());
    }

    #[test]
    fn delete_removes_todo() {
        let db = db();
        let id = db.create("Temp", None).unwrap();
        assert!(db.delete(id).unwrap());
        assert!(db.list(None).unwrap().is_empty());
    }

    #[test]
    fn delete_returns_false_for_unknown_id() {
        let db = db();
        assert!(!db.delete(999).unwrap());
    }
}
