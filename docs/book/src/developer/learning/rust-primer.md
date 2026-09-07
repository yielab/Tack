# Rust for Backend Developers

This chapter covers the Rust concepts you will encounter repeatedly in Tack's codebase. It assumes you already know at least one backend language well. We are not starting from zero — we are translating.

---

## The ownership model — no GC, no manual malloc

Rust manages memory without a garbage collector and without `malloc`/`free`. It does this through a compile-time rule: every value has exactly one *owner*. When the owner goes out of scope, the value is freed. The compiler enforces this at compile time, not at runtime.

Think of it like a library book checkout system. Only one person can hold a book at a time. You can lend it out temporarily (borrowing), but the person who checked it out is responsible for returning it. When they leave, the book goes back to the shelf automatically — no librarian (garbage collector) needed.

```rust
let x = String::from("hello"); // x owns the string
// memory is allocated here

// end of scope — x goes out of scope, string is freed
// no GC needed, no memory leak possible
```

**Moving** — ownership transfers:

```rust
let x = String::from("hello");
let y = x;  // y now owns the string; x is no longer valid
// println!("{}", x);  // this would not compile: x was moved
```

In JavaScript you would never think about this — both variables would point to the same string. In Rust, the compiler prevents you from using `x` after the move, which eliminates entire classes of bugs (use-after-free, double-free).

**Borrowing** — temporary access without taking ownership:

```rust
fn print_title(title: &str) {
    println!("{}", title);
}

let item_title = String::from("Build login page");
print_title(&item_title);  // borrow — print_title sees it, does not own it
println!("{}", item_title); // still valid — we only lent it
```

The `&` means "reference" (borrow). The function sees the value but does not take ownership. You will see `&str`, `&Pool`, `&AppState` constantly in Tack's handler code.

**Why this matters practically:** The compiler catches data races at compile time. If two async tasks could write to the same data simultaneously without synchronization, Rust will refuse to compile. This is why Tack's WebSocket broadcast channel (`broadcast::Sender<BoardEvent>`) uses a typed channel rather than shared mutable state — Rust's rules guide you toward the correct concurrency pattern.

---

## Structs and impls — not classes, but similar

Rust does not have classes. It has structs (data) and `impl` blocks (behavior). The combination is equivalent to a class without inheritance.

```rust
// From crates/tack-core/src/models.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    pub id: Uuid,
    pub project_id: Uuid,
    pub parent_id: Option<Uuid>,
    pub title: String,
    pub description: Option<String>,
    pub item_type: ItemType,
    pub status: String,
    pub priority: Priority,
    pub estimate: Option<f64>,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    // ...
}
```

Compare this to what you already know:

- **Python**: equivalent to a `@dataclass` or Pydantic model
- **TypeScript**: equivalent to an `interface` with all fields required
- **Java**: equivalent to a POJO record — `record Item(UUID id, UUID projectId, ...)`

The `#[derive(...)]` line above the struct is a *derive macro* — the compiler auto-generates trait implementations for those capabilities. `Serialize`/`Deserialize` comes from serde and handles JSON automatically. `Debug` gives you `{:?}` formatting for logging. `Clone` lets you copy the value. You do not write any of this code — the `#[derive]` handles it.

Methods go in a separate `impl` block:

```rust
impl WorkflowConfig {
    pub fn validate_transition(&self, from: &str, to: &str) -> Result<(), CoreError> {
        // self is a reference to the WorkflowConfig instance
        let from_exists = self.statuses.iter().any(|s| s.name == from);
        // ...
    }
}
```

`&self` is like Python's `self` or Java's `this`. There is no `new` keyword — constructors are just associated functions that return `Self`:

```rust
impl Item {
    pub fn new(project_id: Uuid, title: String) -> Self {
        Item {
            id: Uuid::new_v4(),
            project_id,
            title,
            // ...
        }
    }
}
```

There is no inheritance. If two types share behavior, they share a *trait* (covered below). This forces composition over inheritance, which tends to produce simpler code for a codebase like Tack.

---

## Enums with data — the Rust superpower

Rust enums are not Java enums. They can carry data, making them the most expressive type in the language.

**`Option<T>` — nullable values:**

```rust
pub parent_id: Option<Uuid>,   // Some(uuid) or None
pub description: Option<String>, // Some("text") or None
```

This is equivalent to `T | null` in TypeScript, `Optional<T>` in Java, or `T | None` in Python. The critical difference: you cannot accidentally use a `None` value as if it were `Some`. The compiler forces you to check:

```rust
// Wrong — won't compile:
let id: Option<Uuid> = item.parent_id;
let str = id.to_string(); // Error: Option<Uuid> has no to_string()

// Right:
if let Some(parent_id) = item.parent_id {
    // parent_id is a Uuid here, unwrapped
}

// Or:
let parent_str = item.parent_id.map(|id| id.to_string());
```

**`Result<T, E>` — typed errors:**

```rust
pub fn validate_transition(&self, from: &str, to: &str) -> Result<(), CoreError>
```

`Result` is either `Ok(value)` or `Err(error)`. This is like a forced try/catch that is visible in the type signature. Compare to:

- **TypeScript**: similar to `Either<Error, T>` from functional libraries
- **Java**: like a checked exception that shows in the method signature
- **Go**: like `(T, error)` return tuples

**Pattern matching** — exhaustive, compiler-enforced:

```rust
match item.priority {
    Priority::Critical => handle_urgent(item),
    Priority::High     => handle_high(item),
    Priority::Medium   => handle_normal(item),
    Priority::Low      => handle_low(item),
    Priority::None     => handle_no_priority(item),
    // If you add a new Priority variant and forget to handle it here,
    // the code will not compile. The compiler is your checklist.
}
```

You will see `match` throughout Tack's error handling and workflow code.

**The `?` operator — short-circuit error propagation:**

```rust
pub async fn get_item(&self, id: Uuid) -> Result<Option<Item>, sqlx::Error> {
    let row = sqlx::query_as::<_, ItemRow>(/* ... */)
        .fetch_optional(self.pool())
        .await?;  // <-- the ? here

    Ok(row.map(|r| r.into_item()))
}
```

The `?` means: "if this is `Err(e)`, return `Err(e)` immediately from the current function; if it is `Ok(value)`, unwrap it and continue." It is like a one-line `try/catch` that re-throws. Every handler in Tack uses this pattern — you will read it everywhere.

---

## Traits — interfaces with superpowers

Traits define shared behavior. The closest analogies:

- **Java**: interfaces
- **Go**: interfaces (but explicit in Rust)
- **Python**: abstract base classes or protocols
- **TypeScript**: interfaces (structural typing)

The most important traits in Tack are auto-derived via `#[derive]`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project { ... }
```

| Trait | What it does | Analogy |
|-------|-------------|---------|
| `Debug` | `{:?}` formatting for `tracing::debug!` | Python `__repr__` |
| `Clone` | `.clone()` to copy the value | Java `.clone()`, Python `copy.copy()` |
| `Serialize` | Convert to JSON (via serde) | Jackson, json.dumps, JSON.stringify |
| `Deserialize` | Parse from JSON (via serde) | Jackson, json.loads, JSON.parse |
| `PartialEq` | `==` comparison | Java `.equals()`, Python `__eq__` |
| `Default` | A sensible zero value | Java default field values |

The `Display` trait controls how a type formats itself as a string (like Python's `__str__`). Tack uses this for enums so they serialize correctly:

```rust
impl std::fmt::Display for ProjectType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Software    => write!(f, "software"),
            Self::Construction => write!(f, "construction"),
            Self::Personal    => write!(f, "personal"),
            // ...
        }
    }
}
```

You implement `IntoResponse` from Axum on your error type to teach Axum how to turn it into an HTTP response — more on this in the Axum chapter.

---

## The module system

Rust's module system works differently from Node's `require` / Python's `import`. The key rules:

```rust
mod items;          // includes crates/tack-db/src/repo/items.rs as a submodule
pub use items::*;   // re-export everything public from items
```

`pub` controls visibility — only `pub` items are accessible outside the module. Everything else is private by default (stricter than Python, similar to `private` in Java).

```rust
use tack_core::models::{Item, Project, ItemType};
use chrono::Utc;
use uuid::Uuid;
```

`use` is like Python's `from x import y` or TypeScript's `import { y } from 'x'`. It brings names into scope without needing to write the full path every time.

**Crates** are the compilation unit — analogous to npm packages, Python packages, or Maven artifacts. Tack's workspace has six crates, plus a runner binary and a desktop shell built separately (see the [Architecture Overview](../README.md#1-the-layering-rule) and [Crate Tour](../crate-tour.md) for the full picture); the four at the center of the layering rule are:

```
crates/
├── tack-core/   # pure business logic, zero I/O
├── tack-db/     # database access layer
├── tack-api/    # HTTP server
└── tack-cli/    # command-line tool
```

Each crate has its own `Cargo.toml` (equivalent to `package.json` or `pom.xml`). A crate can depend on other crates in the workspace. `tack-api` depends on `tack-db` and `tack-core`; `tack-db` depends on `tack-core`. This hard boundary enforces the architectural rule that HTTP concerns do not leak into database code, and database concerns do not leak into pure business logic.

---

## Error handling in practice

Tack uses `thiserror` to define typed error enums, and `anyhow` in main/CLI code where any error is acceptable.

```rust
// From crates/tack-core/src/error.rs

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("Item not found: {0}")]
    ItemNotFound(Uuid),

    #[error("Invalid status transition from '{from}' to '{to}'")]
    InvalidTransition { from: String, to: String },

    #[error("WIP limit exceeded for column '{column}': limit is {limit}, current is {current}")]
    WipLimitExceeded { column: String, limit: usize, current: usize },

    #[error("Dependency cycle detected involving item {0}")]
    DependencyCycle(Uuid),
}
```

The `#[error("...")]` attribute generates the `Display` implementation — the string you see in logs and API responses. The `{0}`, `{from}`, `{column}` are interpolated from the enum variant's fields.

The `ApiError` type in `tack-api` wraps `CoreError` and implements `IntoResponse` to map each variant to the correct HTTP status code:

```rust
CoreError::ItemNotFound(_) => (StatusCode::NOT_FOUND, err.to_string()),
CoreError::InvalidTransition { .. } => (StatusCode::BAD_REQUEST, err.to_string()),
CoreError::WipLimitExceeded { .. }  => (StatusCode::BAD_REQUEST, err.to_string()),
```

This is exhaustive — if a new `CoreError` variant is added without handling it here, the code will not compile. That is the point.

The `?` operator chains these cleanly:

```rust
pub async fn update_item(/* ... */) -> ApiResult<Json<serde_json::Value>> {
    let project = state.repo.get_project(project_id).await?;  // sqlx::Error → ApiError
    project.workflow.validate_transition(&old_status, &new_status)?;  // CoreError → ApiError
    // ...
}
```

Each `?` uses the `From` trait conversions defined on `ApiError` to automatically convert from `sqlx::Error` or `CoreError` into `ApiError`, then return early. The net effect: error paths are explicit in the type signature and boilerplate-free in the body.
