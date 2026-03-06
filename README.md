# crane-mcp

An MCP server for managing todos and lists. It gives your AI agent a persistent, structured task list it can read and update during a session. You keep a running list of things to do, your agent has context on them, and you stay focused without switching apps or breaking flow.

Use cases:
- Keep a running task list your agent can consult and update without you prompting it every time
- Review what you completed at the end of a session
- Stay in the terminal and stay focused on the work, not on managing the list

Data is stored in a local SQLite database at `~/.crane-mcp/todos.db`.

---

## Prerequisites

- [Rust](https://rustup.rs) (stable toolchain)
- An MCP-compatible client (Claude Desktop, Cursor, or similar)

---

## Getting Started

Build the binary:

```sh
cargo build --release
```

The binary will be at `./target/release/crane-mcp`.

Add it to your MCP client configuration. For Claude Desktop, edit `~/Library/Application Support/Claude/claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "crane-mcp": {
      "command": "/absolute/path/to/crane-mcp"
    }
  }
}
```

For Cursor, add it under `Settings > MCP` using the same command path.

Restart the client. The tools listed below will be available to the agent.

---

## Tools

### `ping`

Check that the server is reachable.

No parameters. Returns `"pong"`.

---

### `list_create`

Create a named list to group todos under.

| Parameter | Type   | Required | Description          |
|-----------|--------|----------|----------------------|
| `title`   | string | yes      | Name of the new list |

Returns the created list id and title.

---

### `list_all`

Show all lists.

No parameters. Returns each list as `#id - title`.

---

### `todo_create`

Create a todo item, optionally placed in a list.

| Parameter | Type   | Required | Description                                      |
|-----------|--------|----------|--------------------------------------------------|
| `title`   | string | yes      | Text of the todo                                 |
| `list`    | string | no       | List title or numeric id to add the todo into    |

Returns the created todo id and title.

---

### `todo_list`

List todos. Returns all todos by default, grouped by list.

| Parameter | Type   | Required | Description                                       |
|-----------|--------|----------|---------------------------------------------------|
| `list`    | string | no       | Filter by list title or numeric id                |

Each todo shows its id, completion status, and title.

---

### `todo_complete`

Mark a todo as completed.

| Parameter | Type    | Required | Description        |
|-----------|---------|----------|--------------------|
| `id`      | integer | yes      | Id of the todo     |

---

### `todo_delete`

Permanently delete a todo.

| Parameter | Type    | Required | Description        |
|-----------|---------|----------|--------------------|
| `id`      | integer | yes      | Id of the todo     |

---

## CLI Wrapper with mcporter

mcporter wraps any stdio MCP server and exposes its tools as shell subcommands. This lets you call crane-mcp tools directly from the terminal without an AI agent in the loop.

Install mcporter following the instructions in its repository, then wrap the crane-mcp binary:

```sh
mcporter --server ./target/release/crane-mcp todo_list
mcporter --server ./target/release/crane-mcp todo_create --title "write tests"
mcporter --server ./target/release/crane-mcp todo_complete --id 1
```

mcporter starts the server process, sends the tool call over stdio using the MCP protocol, prints the response, and exits. No persistent process or separate client needed.

To avoid typing the server path each time, set an alias:

```sh
alias todos='mcporter --server /absolute/path/to/crane-mcp'
```

Then use it as:

```sh
todos todo_list
todos todo_create --title "review PR"
todos list_all
```
