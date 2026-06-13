//! maco HTTP 服务入口：`init` / `backup` 子命令与 Axum 路由装配。

mod artifact_routes;
mod auth;
mod auth_token_routes;
mod chat_routes;
mod directory_picker;
mod export;
mod job_routes;
mod mcp_routes;
mod memory_routes;
mod model_routes;
mod models_api;
mod openapi;
mod routes;
mod run_routes;
mod session_facade;
mod session_meta_view;
mod session_routes;
mod skill_routes;
mod skills_sync;
mod state;
mod system_routes;
mod tool_policy_routes;
mod usage_routes;
mod worker;

use std::net::SocketAddr;

use axum::{Router, middleware};
use clap::{Parser, Subcommand};
use maco_core::{MacoResult, ensure_data_dirs, load_config};
use maco_db::{
    McpServerRepo, ModelRecord, ModelRepo, SettingsRepo, seed_default_filesystem_mcp,
    wal_checkpoint, wal_checkpoint_adk,
};
use maco_governance::auth_disabled;
use maco_telemetry::init_maco_tracing;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::auth::auth_middleware;
use crate::openapi::ApiDoc;
use crate::routes::api_router;
use crate::state::AppState;

/// 命令行参数。
#[derive(Parser)]
#[command(name = "maco-server")]
struct Cli {
    /// 子命令：`init` 初始化库表，`backup` 备份数据文件。
    #[command(subcommand)]
    command: Option<Commands>,
    /// HTTP 绑定地址（无子命令时启动服务）。
    #[arg(long, default_value = "127.0.0.1:8080")]
    bind: String,
}

/// 子命令枚举。
#[derive(Subcommand)]
enum Commands {
    /// 初始化数据库与默认配置。
    Init,
    /// 备份 `~/.maco/data` 下 SQLite 与附件。
    Backup,
}

#[tokio::main]
async fn main() -> MacoResult<()> {
    let _telemetry = init_maco_tracing();

    let cli = Cli::parse();
    let cfg = load_config()?;
    ensure_data_dirs(&cfg.data)?;

    match cli.command {
        Some(Commands::Init) => run_init(&cfg.data).await?,
        Some(Commands::Backup) => run_backup(&cfg.data).await?,
        None => run_server(cli.bind, cfg.data).await?,
    }
    Ok(())
}

async fn run_init(paths: &maco_core::DataPaths) -> MacoResult<()> {
    let db = maco_db::init_pool(&paths.maco_db).await?;
    let settings = SettingsRepo::new(db.pool.clone());
    maco_db::seed_defaults(&settings).await?;
    seed_default_filesystem_mcp(&McpServerRepo::new(db.pool.clone()), &paths.tmp_dir).await?;
    let _ = maco_storage::AdkStorage::open(paths).await?;
    seed_default_model(&db).await?;
    tracing::info!("init complete at {}", paths.maco_db.display());
    Ok(())
}

async fn seed_default_model(db: &maco_db::MacoDb) -> MacoResult<()> {
    let repo = ModelRepo::new(db.pool.clone());
    if repo.get_default().await?.is_some() {
        return Ok(());
    }
    let now = chrono::Utc::now().to_rfc3339();
    repo.insert(&ModelRecord {
        id: ModelRepo::new_id(),
        name: "gpt-4o-mini".into(),
        provider: "openai".into(),
        model_id: "gpt-4o-mini".into(),
        base_url: None,
        api_key_env: String::new(),
        is_default: 1,
        enabled: 1,
        config: "{}".into(),
        created_at: now.clone(),
        updated_at: now,
    })
    .await?;
    Ok(())
}

async fn run_backup(paths: &maco_core::DataPaths) -> MacoResult<()> {
    tracing::info!("backup: WAL checkpoint (best-effort; stop server first for consistency)");
    if paths.maco_db.exists()
        && let Err(e) = wal_checkpoint(&paths.maco_db).await
    {
        tracing::warn!("backup: checkpoint maco.db failed: {e}");
    }
    if paths.sessions_db.exists()
        && let Err(e) = wal_checkpoint_adk(&paths.sessions_db).await
    {
        tracing::warn!("backup: checkpoint sessions.db failed: {e}");
    }
    if paths.memory_db.exists()
        && let Err(e) = wal_checkpoint_adk(&paths.memory_db).await
    {
        tracing::warn!("backup: checkpoint memory.db failed: {e}");
    }

    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
    let backup_root = paths
        .maco_db
        .parent()
        .map(|p| p.parent().unwrap_or(p).join("backups"))
        .unwrap_or_else(|| std::path::PathBuf::from("backups"));
    let dest = backup_root.join(stamp.to_string());
    std::fs::create_dir_all(&dest)
        .map_err(|e| maco_core::MacoError::config(format!("create backup dir: {e}")))?;

    for src in [&paths.maco_db, &paths.sessions_db, &paths.memory_db] {
        if src.exists() {
            let name = src.file_name().and_then(|s| s.to_str()).unwrap_or("db");
            if let Err(e) = std::fs::copy(src, dest.join(name)) {
                tracing::warn!("backup: copy {name} failed: {e}");
            }
        }
    }
    if paths.artifacts_dir.exists()
        && let Err(e) = copy_dir_recursive(&paths.artifacts_dir, &dest.join("artifacts"))
    {
        tracing::warn!("backup: copy artifacts failed: {e}");
    }

    tracing::info!(
        "backup complete (best-effort): {} — stop server before restore",
        dest.display()
    );
    Ok(())
}

fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> MacoResult<()> {
    std::fs::create_dir_all(dst)
        .map_err(|e| maco_core::MacoError::config(format!("mkdir {}: {e}", dst.display())))?;
    for entry in std::fs::read_dir(src)
        .map_err(|e| maco_core::MacoError::config(format!("read_dir: {e}")))?
    {
        let entry = entry.map_err(|e| maco_core::MacoError::config(format!("entry: {e}")))?;
        let ty = entry
            .file_type()
            .map_err(|e| maco_core::MacoError::config(format!("file_type: {e}")))?;
        let target = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)
                .map_err(|e| maco_core::MacoError::config(format!("copy file: {e}")))?;
        }
    }
    Ok(())
}

async fn run_server(bind: String, paths: maco_core::DataPaths) -> MacoResult<()> {
    if !bind.starts_with("127.0.0.1")
        && std::env::var("MACO_BIND_EXPLICIT").ok().as_deref() != Some("1")
    {
        return Err(maco_core::MacoError::config(
            "refusing non-localhost bind; set MACO_BIND_EXPLICIT=1",
        ));
    }
    let state = AppState::new(bind.clone(), &paths.maco_db, &paths).await?;
    worker::spawn_job_worker(state.repos.jobs.clone());
    let auth_on = !auth_disabled();
    if auth_on {
        tracing::info!("auth enabled (set MACO_AUTH_DISABLED=true to disable)");
    }
    let openapi = ApiDoc::openapi();
    let app = Router::new()
        .nest("/api", api_router())
        .merge(SwaggerUi::new("/api/docs").url("/api/openapi.json", openapi))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        .with_state(state);
    let addr: SocketAddr = bind
        .parse()
        .map_err(|e| maco_core::MacoError::config(format!("{e}")))?;
    tracing::info!("maco listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| maco_core::MacoError::config(format!("bind: {e}")))?;
    axum::serve(listener, app)
        .await
        .map_err(|e| maco_core::MacoError::Other(e.into()))?;
    Ok(())
}
