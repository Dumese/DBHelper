//! 八种数据库的连接冒烟测试。
//!
//! SQLite 使用临时文件、无需外部依赖，默认执行；
//! 其余 7 种需要自备服务端，通过环境变量提供地址后以 `--ignored` 运行：
//!
//! ```bash
//! cargo test --test drivers_smoke -- --ignored --nocapture
//! ```

use dbhelper_models::{ConnectionConfig, DbKind};
use rel_common::RelationalDriver;

fn cfg(kind: DbKind) -> ConnectionConfig {
    let mut c = ConnectionConfig::new(1, "smoke", kind);
    c.connect_timeout_ms = 5_000;
    c
}

#[tokio::test]
async fn sqlite_connects_and_queries() {
    let path = std::env::temp_dir().join("dbhelper_smoke.sqlite");
    let _ = std::fs::remove_file(&path);
    let mut c = cfg(DbKind::Sqlite);
    c.database = path.to_string_lossy().to_string();
    let driver = driver_sqlite::SqliteDriver::new(c);
    driver.connect().await.expect("连接 SQLite");
    assert!(driver.ping().await.is_ok());
    driver
        .execute("CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT)", std::time::Duration::from_secs(5))
        .await
        .expect("建表");
    driver
        .execute("INSERT INTO t (id, name) VALUES (1, 'dbhelper')", std::time::Duration::from_secs(5))
        .await
        .expect("写入");
    let out = driver
        .query("SELECT id, name FROM t", 10, std::time::Duration::from_secs(5))
        .await
        .expect("查询");
    assert_eq!(out.rows.len(), 1);
    assert_eq!(out.rows[0][1].display(), "dbhelper");
    driver.close().await;
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
#[ignore = "需要可访问的 MySQL 服务端（DBHELPER_MYSQL_HOST/PORT/USER/PASS/DB）"]
async fn mysql_connects() {
    let mut c = cfg(DbKind::Mysql);
    c.host = std::env::var("DBHELPER_MYSQL_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    c.port = std::env::var("DBHELPER_MYSQL_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(3306);
    c.username = std::env::var("DBHELPER_MYSQL_USER").unwrap_or_else(|_| "root".into());
    c.database = std::env::var("DBHELPER_MYSQL_DB").unwrap_or_else(|_| "mysql".into());
    let mut driver = driver_mysql::MysqlDriver::new(c);
    let password = std::env::var("DBHELPER_MYSQL_PASS").unwrap_or_default();
    driver.set_password(password);
    driver.connect().await.expect("连接 MySQL");
    assert!(driver.ping().await.is_ok());
    let out = driver.query("SELECT 1", 1, std::time::Duration::from_secs(5)).await.expect("查询");
    assert_eq!(out.rows.len(), 1);
    driver.close().await;
}

#[tokio::test]
#[ignore = "需要可访问的 PostgreSQL 服务端（DBHELPER_PG_*）"]
async fn postgres_connects() {
    let mut c = cfg(DbKind::Postgres);
    c.host = std::env::var("DBHELPER_PG_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    c.port = std::env::var("DBHELPER_PG_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(5432);
    c.username = std::env::var("DBHELPER_PG_USER").unwrap_or_else(|_| "postgres".into());
    c.database = std::env::var("DBHELPER_PG_DB").unwrap_or_else(|_| "postgres".into());
    let mut driver = driver_postgres::PostgresDriver::new(c);
    driver.set_password(std::env::var("DBHELPER_PG_PASS").unwrap_or_default());
    driver.connect().await.expect("连接 PostgreSQL");
    assert!(driver.ping().await.is_ok());
    driver.close().await;
}

#[tokio::test]
#[ignore = "需要 SQL Server（DBHELPER_MSSQL_*）"]
async fn mssql_connects() {
    let mut c = cfg(DbKind::SqlServer);
    c.host = std::env::var("DBHELPER_MSSQL_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    c.port = std::env::var("DBHELPER_MSSQL_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(1433);
    c.username = std::env::var("DBHELPER_MSSQL_USER").unwrap_or_else(|_| "sa".into());
    c.database = std::env::var("DBHELPER_MSSQL_DB").unwrap_or_else(|_| "master".into());
    let mut driver = driver_mssql::MssqlDriver::new(c);
    driver.set_password(std::env::var("DBHELPER_MSSQL_PASS").unwrap_or_default());
    driver.connect().await.expect("连接 SQL Server");
    assert!(driver.ping().await.is_ok());
    driver.close().await;
}

#[tokio::test]
#[ignore = "需要 Oracle Instant Client 与 DBHELPER_ORACLE_* 环境变量"]
async fn oracle_connects() {
    let mut c = cfg(DbKind::Oracle);
    c.host = std::env::var("DBHELPER_ORACLE_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    c.port = std::env::var("DBHELPER_ORACLE_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(1521);
    c.database = std::env::var("DBHELPER_ORACLE_SERVICE").unwrap_or_else(|_| "ORCL".into());
    c.username = std::env::var("DBHELPER_ORACLE_USER").unwrap_or_else(|_| "system".into());
    let mut driver = driver_oracle::OracleDriver::new(c);
    driver.set_password(std::env::var("DBHELPER_ORACLE_PASS").unwrap_or_default());
    driver.connect().await.expect("连接 Oracle");
    assert!(driver.ping().await.is_ok());
    driver.close().await;
}

#[tokio::test]
#[ignore = "需要可访问的 Redis（DBHELPER_REDIS_HOST/PORT/PASS）"]
async fn redis_connects() {
    let mut c = cfg(DbKind::Redis);
    c.host = std::env::var("DBHELPER_REDIS_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    c.port = std::env::var("DBHELPER_REDIS_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(6379);
    let mut driver = driver_redis::RedisDriver::new(c);
    driver.set_password(std::env::var("DBHELPER_REDIS_PASS").unwrap_or_default());
    driver.connect_default().await.expect("连接 Redis");
    assert!(driver.ping().await.is_ok());
    let info = driver.info().await.expect("读取 INFO");
    assert!(info.version.starts_with("Redis"));
}

#[tokio::test]
#[ignore = "需要可访问的 MongoDB（DBHELPER_MONGO_HOST/PORT/USER/PASS/DB）"]
async fn mongodb_connects() {
    let mut c = cfg(DbKind::Mongodb);
    c.host = std::env::var("DBHELPER_MONGO_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    c.port = std::env::var("DBHELPER_MONGO_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(27017);
    c.database = std::env::var("DBHELPER_MONGO_DB").unwrap_or_else(|_| "admin".into());
    c.username = std::env::var("DBHELPER_MONGO_USER").unwrap_or_default();
    let mut driver = driver_mongodb::MongoDriver::new(c);
    driver.set_password(std::env::var("DBHELPER_MONGO_PASS").unwrap_or_default());
    driver.connect().await.expect("连接 MongoDB");
    assert!(driver.ping().await.is_ok());
    let _ = driver.databases().await.expect("列出数据库");
}

#[tokio::test]
#[ignore = "需要可访问的 Elasticsearch（DBHELPER_ES_HOST/PORT）"]
async fn elasticsearch_connects() {
    let mut c = cfg(DbKind::Elasticsearch);
    c.host = std::env::var("DBHELPER_ES_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    c.port = std::env::var("DBHELPER_ES_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(9200);
    c.username = std::env::var("DBHELPER_ES_USER").unwrap_or_default();
    let mut driver = driver_elasticsearch::EsDriver::new(c).expect("构建客户端");
    driver.set_password(std::env::var("DBHELPER_ES_PASS").unwrap_or_default());
    assert!(driver.ping().await.is_ok(), "Elasticsearch 可达");
    let _ = driver.indices().await.expect("列出索引");
}
