use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{Implementation, ServerCapabilities, ServerInfo},
    schemars::JsonSchema,
    tool, tool_handler, tool_router,
    transport::stdio,
};
use serde::Deserialize;
use std::sync::{Arc, Mutex};

mod db;
use db::TodoDb;

// ── Parameter structs ─────────────────────────────────────────────────────────

#[derive(Deserialize, JsonSchema)]
struct CreateTodoParams {
    /// The title of the todo item.
    title: String,
    /// The list to add it to — accepts a list title (e.g. "Inbox") or numeric id.
    /// If omitted the todo is created without a list.
    list: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct ListTodosParams {
    /// Filter by list — accepts a list title or numeric id.
    /// If omitted, all todos across all lists are returned.
    list: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct TodoIdParams {
    /// The id of the todo item.
    id: i64,
}

#[derive(Deserialize, JsonSchema)]
struct CreateListParams {
    /// The title of the new list.
    title: String,
}

// ── Server ────────────────────────────────────────────────────────────────────

#[derive(Clone)]
struct CraneMcp {
    tool_router: ToolRouter<Self>,
    todo_db: Arc<Mutex<TodoDb>>,
}

impl CraneMcp {
    fn new() -> Self {
        let todo_db = TodoDb::open().expect("failed to open todo database");
        Self {
            tool_router: Self::tool_router(),
            todo_db: Arc::new(Mutex::new(todo_db)),
        }
    }

    /// Resolve an optional "list" string to a list_id, returning an error string on failure.
    fn resolve_list(
        db: &TodoDb,
        list: &str,
    ) -> Result<i64, String> {
        match db.find_list(list) {
            Ok(Some(l)) => Ok(l.id),
            Ok(None) => Err(format!("No list found matching \"{list}\". Use list_all to see available lists.")),
            Err(e) => Err(format!("Error: {e}")),
        }
    }
}

// ── Tools ─────────────────────────────────────────────────────────────────────

#[tool_router]
impl CraneMcp {
    // ── List tools ────────────────────────────────────────────────────────────

    #[tool(description = "Create a new list (e.g. Today, Inbox, Tomorrow)")]
    async fn list_create(&self, Parameters(p): Parameters<CreateListParams>) -> String {
        match self.todo_db.lock().unwrap().create_list(&p.title) {
            Ok(id) => format!("Created list #{id}: {}", p.title),
            Err(e) => format!("Error: {e}"),
        }
    }

    #[tool(description = "Show all lists")]
    async fn list_all(&self) -> String {
        match self.todo_db.lock().unwrap().all_lists() {
            Ok(lists) if lists.is_empty() => "No lists yet. Use list_create to make one.".to_string(),
            Ok(lists) => lists
                .iter()
                .map(|l| format!("#{} — {}", l.id, l.title))
                .collect::<Vec<_>>()
                .join("\n"),
            Err(e) => format!("Error: {e}"),
        }
    }

    // ── Todo tools ────────────────────────────────────────────────────────────

    #[tool(name = "create", description = "Create a new todo item, optionally in a list (by title or id)")]
    async fn todo_create(&self, Parameters(p): Parameters<CreateTodoParams>) -> String {
        let db = self.todo_db.lock().unwrap();
        let list_id = match p.list.as_deref() {
            None => None,
            Some(list) => match Self::resolve_list(&db, list) {
                Ok(id) => Some(id),
                Err(e) => return e,
            },
        };
        match db.create(&p.title, list_id) {
            Ok(id) => {
                let list_note = p.list.map(|l| format!(" in \"{l}\"")).unwrap_or_default();
                format!("Created todo #{id}: {}{list_note}", p.title)
            }
            Err(e) => format!("Error: {e}"),
        }
    }

    #[tool(name = "list", description = "List todos, optionally filtered by list title or id")]
    async fn todo_list(&self, Parameters(p): Parameters<ListTodosParams>) -> String {
        let db = self.todo_db.lock().unwrap();
        let list_id = match p.list.as_deref() {
            None => None,
            Some(list) => match Self::resolve_list(&db, list) {
                Ok(id) => Some(id),
                Err(e) => return e,
            },
        };
        match db.list(list_id) {
            Ok(todos) if todos.is_empty() => "No todos found.".to_string(),
            Ok(todos) => {
                // Group todos by list title, preserving insertion order.
                let mut groups: Vec<(String, Vec<&db::Todo>)> = Vec::new();
                for todo in &todos {
                    let key = todo.list_title.clone().unwrap_or_else(|| "(No list)".to_string());
                    if let Some(group) = groups.iter_mut().find(|(k, _)| k == &key) {
                        group.1.push(todo);
                    } else {
                        groups.push((key, vec![todo]));
                    }
                }
                groups
                    .iter()
                    .map(|(list_name, items)| {
                        let bullets = items
                            .iter()
                            .map(|t| {
                                let check = if t.completed { "✅" } else { "⬜" };
                                format!("  • #{} {} {}", t.id, check, t.title)
                            })
                            .collect::<Vec<_>>()
                            .join("\n");
                        format!("{list_name}\n{bullets}")
                    })
                    .collect::<Vec<_>>()
                    .join("\n\n")
            }
            Err(e) => format!("Error: {e}"),
        }
    }

    #[tool(name = "complete", description = "Mark a todo as completed by its id")]
    async fn todo_complete(&self, Parameters(p): Parameters<TodoIdParams>) -> String {
        match self.todo_db.lock().unwrap().complete(p.id) {
            Ok(true) => format!("Todo #{} marked as completed.", p.id),
            Ok(false) => format!("No todo found with id #{}.", p.id),
            Err(e) => format!("Error: {e}"),
        }
    }

    #[tool(name = "delete", description = "Permanently delete a todo by its id")]
    async fn todo_delete(&self, Parameters(p): Parameters<TodoIdParams>) -> String {
        match self.todo_db.lock().unwrap().delete(p.id) {
            Ok(true) => format!("Todo #{} deleted.", p.id),
            Ok(false) => format!("No todo found with id #{}.", p.id),
            Err(e) => format!("Error: {e}"),
        }
    }
}

#[tool_handler]
impl ServerHandler for CraneMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("todo-mcp", "0.1.0"))
    }
}

#[tokio::main]
async fn main() {
    let transport = stdio();
    let server = CraneMcp::new()
        .serve(transport)
        .await
        .expect("server failed");
    server.waiting().await.ok();
}
