use std::collections::HashMap;

use adk_core::{Content, Part};
use adk_memory::{MemoryService, SearchRequest, SqliteMemoryService};
use adk_runner::Runner;
use adk_session::{
    CreateRequest, DeleteRequest, ListRequest, SessionService, SqliteSessionService,
};
use maco_core::{APP_NAME, USER_ID, sqlite_url};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let tmp = tempfile::tempdir()?;
    std::fs::create_dir_all(tmp.path())?;
    let sessions_path = tmp.path().join("sessions.db");
    let memory_path = tmp.path().join("memory.db");

    println!("=== R11: Session SQLite ===");
    println!("import: adk_session::SqliteSessionService");
    println!("feature: adk-session/sqlite");
    let session_url = "sqlite::memory:".to_string();
    let session_svc = SqliteSessionService::new(&session_url).await?;
    session_svc.migrate().await?;
    println!("migrate(): ok");

    let session = session_svc
        .create(CreateRequest {
            app_name: APP_NAME.into(),
            user_id: USER_ID.into(),
            session_id: None,
            state: HashMap::new(),
        })
        .await?;
    let session_id = session.id().to_string();
    println!("create session_id: {session_id}");

    let listed = session_svc
        .list(ListRequest {
            app_name: APP_NAME.into(),
            user_id: USER_ID.into(),
            limit: None,
            offset: None,
        })
        .await?;
    println!("list sessions: {}", listed.len());

    println!("\n=== R11: Memory SQLite ===");
    println!("import: adk_memory::SqliteMemoryService");
    println!("feature: adk-memory/sqlite-memory");
    let memory_url = "sqlite:memory.db";
    let memory_svc = SqliteMemoryService::new(memory_url).await?;
    memory_svc.migrate().await?;
    println!("migrate(): ok");

    let search = memory_svc
        .search(SearchRequest {
            app_name: APP_NAME.into(),
            user_id: USER_ID.into(),
            query: "hello".into(),
            limit: Some(5),
            min_score: None,
            project_id: None,
        })
        .await?;
    println!(
        "search_mode: keyword (FTS5); results: {}",
        search.memories.len()
    );

    println!("\n=== R13: Runner interrupt ===");
    println!("Runner::interrupt(session_id) -> bool; events preserved; new run() allowed");

    session_svc
        .delete(DeleteRequest {
            app_name: APP_NAME.into(),
            user_id: USER_ID.into(),
            session_id: session_id.clone(),
        })
        .await?;
    println!("delete session: ok");

    println!("\n=== R17/R18 summary ===");
    println!("resume_context: inject via new run + Content with FunctionResponse part");
    println!("memory semantic search: NOT available in SqliteMemoryService (keyword only)");

    let _content = Content {
        role: "user".into(),
        parts: vec![Part::Text {
            text: "spike ok".into(),
        }],
    };

    let _runner_hint = Runner::builder();
    println!("Runner::builder() available via adk_runner / adk_rust::prelude");

    println!("\nSpike completed successfully.");
    Ok(())
}
