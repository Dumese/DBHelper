//! Redis 驱动（redis-rs / tokio）。
//!
//! 覆盖：键空间 SCAN（16 个逻辑库）、五种数据类型读取与编辑、TTL 管理、
//! INFO 指标解析（用于状态栏与键空间列表）。

use dbhelper_models::{ConnectionConfig, DbKind};
use redis::aio::ConnectionManager;
use redis::{AsyncCommands, Value as RedisValue};
use serde_json::Value as Json;
use std::time::Instant;
use thiserror::Error;
use tokio::sync::RwLock;

#[derive(Debug, Error)]
pub enum RedisError {
    #[error("连接失败: {0}")]
    Connect(String),
    #[error("未连接")]
    NotConnected,
    #[error("命令失败: {0}")]
    Command(String),
}

pub type RedisResult<T> = Result<T, RedisError>;

#[derive(Debug, Clone, Default)]
pub struct KeySpaceRow {
    pub db: u8,
    pub keys: u64,
    pub expires: u64,
}

#[derive(Debug, Clone, Default)]
pub struct RedisInfo {
    pub version: String,
    pub mode: String,
    pub connected_clients: u64,
    pub used_memory_human: String,
    pub keyspace: Vec<KeySpaceRow>,
    pub keyspace_hits: u64,
    pub keyspace_misses: u64,
    pub role: String,
}

impl RedisInfo {
    pub fn hit_rate(&self) -> f64 {
        let total = self.keyspace_hits + self.keyspace_misses;
        if total == 0 {
            0.0
        } else {
            self.keyspace_hits as f64 / total as f64
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyType {
    String,
    Hash,
    List,
    Set,
    ZSet,
    Stream,
    Unknown,
}

impl KeyType {
    pub fn from_str(s: &str) -> Self {
        match s {
            "string" => Self::String,
            "hash" => Self::Hash,
            "list" => Self::List,
            "set" => Self::Set,
            "zset" => Self::ZSet,
            "stream" => Self::Stream,
            _ => Self::Unknown,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::String => "STRING",
            Self::Hash => "HASH",
            Self::List => "LIST",
            Self::Set => "SET",
            Self::ZSet => "ZSET",
            Self::Stream => "STREAM",
            Self::Unknown => "UNKNOWN",
        }
    }
}

pub struct RedisDriver {
    config: ConnectionConfig,
    manager: RwLock<Option<ConnectionManager>>,
    /// 明文密码仅驻留内存，不落盘。
    password: String,
}

impl RedisDriver {
    pub fn new(config: ConnectionConfig) -> Self {
        Self { config, manager: RwLock::new(None), password: String::new() }
    }

    pub fn config(&self) -> &ConnectionConfig {
        &self.config
    }

    fn url(&self, db: u8) -> String {
        let scheme = if self.config.ssl == dbhelper_models::SslMode::Disabled {
            "redis"
        } else {
            "rediss"
        };
        if self.config.username.is_empty() {
            format!(
                "{scheme}://:{}@{}:{}/{}",
                urlencode(&self.secret()),
                self.config.host,
                self.config.port,
                db
            )
        } else {
            format!(
                "{scheme}://{}:{}@{}:{}/{}",
                self.config.username,
                urlencode(&self.secret()),
                self.config.host,
                self.config.port,
                db
            )
        }
    }

    /// 密码由调用方通过 `with_password` 注入。
    fn secret(&self) -> String {
        self.password.clone()
    }

    pub fn with_password(mut self, password: String) -> Self {
        self.password = password;
        self
    }

    pub async fn connect(&self, db: u8) -> RedisResult<()> {
        let client = redis::Client::open(self.url(db)).map_err(|e| RedisError::Connect(e.to_string()))?;
        let manager = client
            .get_connection_manager()
            .await
            .map_err(|e| RedisError::Connect(e.to_string()))?;
        *self.manager.write().await = Some(manager);
        Ok(())
    }

    pub async fn connect_default(&self) -> RedisResult<()> {
        self.connect(0).await
    }

    pub async fn select(&self, db: u8) -> RedisResult<()> {
        let mut conn = self.conn().await?;
        redis::cmd("SELECT")
            .arg(db)
            .query_async::<()>(&mut conn)
            .await
            .map_err(|e| RedisError::Command(e.to_string()))?;
        Ok(())
    }

    async fn conn(&self) -> RedisResult<ConnectionManager> {
        self.manager.read().await.clone().ok_or(RedisError::NotConnected)
    }

    pub async fn ping(&self) -> RedisResult<u64> {
        let mut conn = self.conn().await?;
        let start = Instant::now();
        redis::cmd("PING")
            .query_async::<String>(&mut conn)
            .await
            .map_err(|e| RedisError::Command(e.to_string()))?;
        Ok(start.elapsed().as_millis() as u64)
    }

    pub async fn info(&self) -> RedisResult<RedisInfo> {
        let mut conn = self.conn().await?;
        let text: String = redis::cmd("INFO")
            .query_async(&mut conn)
            .await
            .map_err(|e| RedisError::Command(e.to_string()))?;

        let mut info = RedisInfo::default();
        for line in text.lines() {
            if let Some((k, v)) = line.split_once(':') {
                match k {
                    "redis_version" => info.version = format!("Redis {}", v.trim()),
                    "redis_mode" => info.mode = v.trim().to_string(),
                    "connected_clients" => info.connected_clients = v.trim().parse().unwrap_or(0),
                    "used_memory_human" => info.used_memory_human = v.trim().to_string(),
                    "keyspace_hits" => info.keyspace_hits = v.trim().parse().unwrap_or(0),
                    "keyspace_misses" => info.keyspace_misses = v.trim().parse().unwrap_or(0),
                    "role" => info.role = v.trim().to_string(),
                    other if other.starts_with("db") => {
                        // db0:keys=1284,expires=12,avg_ttl=0
                        if let Ok(idx) = other[2..].parse::<u8>() {
                            let keys = v
                                .split(',')
                                .find_map(|p| p.strip_prefix("keys="))
                                .and_then(|n| n.parse().ok())
                                .unwrap_or(0);
                            let expires = v
                                .split(',')
                                .find_map(|p| p.strip_prefix("expires="))
                                .and_then(|n| n.parse().ok())
                                .unwrap_or(0);
                            info.keyspace.push(KeySpaceRow { db: idx, keys, expires });
                        }
                    }
                    _ => {}
                }
            }
        }
        info.keyspace.sort_by_key(|k| k.db);
        Ok(info)
    }

    /// 使用 SCAN 游标翻页，避免 KEYS 阻塞服务端。
    pub async fn scan(&self, db: u8, pattern: &str, cursor: u64, count: usize) -> RedisResult<(u64, Vec<String>)> {
        let mut conn = self.conn().await?;
        redis::cmd("SELECT").arg(db).query_async::<()>(&mut conn).await.ok();
        let (next, keys): (u64, Vec<String>) = redis::cmd("SCAN")
            .arg(cursor)
            .arg("MATCH")
            .arg(if pattern.is_empty() { "*" } else { pattern })
            .arg("COUNT")
            .arg(count)
            .query_async(&mut conn)
            .await
            .map_err(|e| RedisError::Command(e.to_string()))?;
        Ok((next, keys))
    }

    pub async fn key_type(&self, key: &str) -> RedisResult<KeyType> {
        let mut conn = self.conn().await?;
        let t: String = redis::cmd("TYPE")
            .arg(key)
            .query_async(&mut conn)
            .await
            .map_err(|e| RedisError::Command(e.to_string()))?;
        Ok(KeyType::from_str(&t))
    }

    pub async fn ttl(&self, key: &str) -> RedisResult<i64> {
        let mut conn = self.conn().await?;
        redis::cmd("TTL")
            .arg(key)
            .query_async(&mut conn)
            .await
            .map_err(|e| RedisError::Command(e.to_string()))
    }

    pub async fn memory_usage(&self, key: &str) -> RedisResult<u64> {
        let mut conn = self.conn().await?;
        let usage: u64 = redis::cmd("MEMORY")
            .arg("USAGE")
            .arg(key)
            .query_async(&mut conn)
            .await
            .unwrap_or(0);
        Ok(usage)
    }

    pub async fn encoding(&self, key: &str) -> RedisResult<String> {
        let mut conn = self.conn().await?;
        redis::cmd("OBJECT")
            .arg("ENCODING")
            .arg(key)
            .query_async(&mut conn)
            .await
            .map_err(|e| RedisError::Command(e.to_string()))
    }

    /// 按类型读取键内容，统一成 (field, value) 行，便于表格展示。
    pub async fn read_key(&self, key: &str) -> RedisResult<(KeyType, Vec<(String, String)>)> {
        let kt = self.key_type(key).await?;
        let mut conn = self.conn().await?;
        let rows = match kt {
            KeyType::Hash => {
                let map: std::collections::HashMap<String, String> = conn
                    .hgetall(key)
                    .await
                    .map_err(|e| RedisError::Command(e.to_string()))?;
                let mut v: Vec<(String, String)> = map.into_iter().collect();
                v.sort_by(|a, b| a.0.cmp(&b.0));
                v
            }
            KeyType::String => {
                let value: String = conn
                    .get(key)
                    .await
                    .map_err(|e| RedisError::Command(e.to_string()))?;
                vec![("(value)".into(), value)]
            }
            KeyType::List => {
                let items: Vec<String> = conn
                    .lrange(key, 0, -1)
                    .await
                    .map_err(|e| RedisError::Command(e.to_string()))?;
                items
                    .into_iter()
                    .enumerate()
                    .map(|(i, v)| (i.to_string(), v))
                    .collect()
            }
            KeyType::Set => {
                let items: Vec<String> = conn
                    .smembers(key)
                    .await
                    .map_err(|e| RedisError::Command(e.to_string()))?;
                items
                    .into_iter()
                    .enumerate()
                    .map(|(i, v)| (i.to_string(), v))
                    .collect()
            }
            KeyType::ZSet => {
                let items: Vec<(String, f64)> = conn
                    .zrange_withscores(key, 0, -1)
                    .await
                    .map_err(|e| RedisError::Command(e.to_string()))?;
                items.into_iter().map(|(m, s)| (format!("{s}"), m)).collect()
            }
            _ => Vec::new(),
        };
        Ok((kt, rows))
    }

    /// 写入键内容。Hash 用 field/value，String 忽略 field。
    pub async fn write_key(&self, key: &str, kt: KeyType, field: &str, value: &str) -> RedisResult<()> {
        let mut conn = self.conn().await?;
        match kt {
            KeyType::Hash => {
                redis::cmd("HSET")
                    .arg(key)
                    .arg(field)
                    .arg(value)
                    .query_async::<()>(&mut conn)
                    .await
                    .map_err(|e| RedisError::Command(e.to_string()))?;
            }
            KeyType::String => {
                redis::cmd("SET")
                    .arg(key)
                    .arg(value)
                    .query_async::<()>(&mut conn)
                    .await
                    .map_err(|e| RedisError::Command(e.to_string()))?;
            }
            KeyType::List => {
                redis::cmd("RPUSH")
                    .arg(key)
                    .arg(value)
                    .query_async::<()>(&mut conn)
                    .await
                    .map_err(|e| RedisError::Command(e.to_string()))?;
            }
            KeyType::Set => {
                redis::cmd("SADD")
                    .arg(key)
                    .arg(value)
                    .query_async::<()>(&mut conn)
                    .await
                    .map_err(|e| RedisError::Command(e.to_string()))?;
            }
            KeyType::ZSet => {
                let score = field.parse::<f64>().unwrap_or(0.0);
                redis::cmd("ZADD")
                    .arg(key)
                    .arg(score)
                    .arg(value)
                    .query_async::<()>(&mut conn)
                    .await
                    .map_err(|e| RedisError::Command(e.to_string()))?;
            }
            _ => return Err(RedisError::Command("不支持的类型".into())),
        }
        Ok(())
    }

    pub async fn delete(&self, key: &str) -> RedisResult<u64> {
        let mut conn = self.conn().await?;
        conn.del(key).await.map_err(|e| RedisError::Command(e.to_string()))
    }

    pub async fn expire(&self, key: &str, secs: i64) -> RedisResult<bool> {
        let mut conn = self.conn().await?;
        conn.expire(key, secs).await.map_err(|e| RedisError::Command(e.to_string()))
    }

    pub async fn persist(&self, key: &str) -> RedisResult<bool> {
        let mut conn = self.conn().await?;
        conn.persist(key).await.map_err(|e| RedisError::Command(e.to_string()))
    }

    /// 任意命令（命令历史/调试面板）。
    pub async fn raw_command(&self, args: &[String]) -> RedisResult<Json> {
        if args.is_empty() {
            return Err(RedisError::Command("命令为空".into()));
        }
        let mut conn = self.conn().await?;
        let mut cmd = redis::cmd(&args[0]);
        for a in &args[1..] {
            cmd.arg(a);
        }
        let value: RedisValue = cmd
            .query_async(&mut conn)
            .await
            .map_err(|e| RedisError::Command(e.to_string()))?;
        Ok(redis_to_json(&value))
    }

    pub fn kind(&self) -> DbKind {
        DbKind::Redis
    }
}

fn redis_to_json(value: &RedisValue) -> Json {
    match value {
        RedisValue::Nil => Json::Null,
        RedisValue::Int(i) => Json::from(*i),
        RedisValue::BulkString(s) => Json::from(String::from_utf8_lossy(s).to_string()),
        RedisValue::SimpleString(s) => Json::from(s.clone()),
        RedisValue::Okay => Json::from("OK"),
        RedisValue::Array(items) => Json::Array(items.iter().map(redis_to_json).collect()),
        RedisValue::Map(pairs) => {
            let mut map = serde_json::Map::new();
            for (k, v) in pairs {
                map.insert(format!("{k:?}"), redis_to_json(v));
            }
            Json::Object(map)
        }
        other => Json::from(format!("{other:?}")),
    }
}

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

impl RedisDriver {
    /// 明文密码仅驻留内存，不在磁盘出现。
    pub fn set_password(&mut self, password: String) {
        self.password = password;
    }

    /// 关闭连接：丢弃底层连接管理器，连接随之释放。
    pub async fn close(&self) {
        *self.manager.write().await = None;
    }
}
