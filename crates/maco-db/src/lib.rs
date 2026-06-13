//! maco 业务库（`maco.db`）连接池与各表 Repository。

pub mod pool;
pub mod repos;

pub use pool::{MacoDb, init_pool, wal_checkpoint, wal_checkpoint_adk};
pub use repos::settings::seed_defaults;
pub use repos::*;
