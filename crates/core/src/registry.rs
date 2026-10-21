//! 连接注册表：持有活动驱动实例、限制单连接并发、隔离单库异常。

use dbhelper_models::{ConnId, ConnectionConfig, DbKind};
use rel_common::RelationalDriver;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{OwnedSemaphorePermit, RwLock, Semaphore};

/// 单连接最大并发查询数（需求 §七-C：≤4）。
pub const MAX_CONCURRENT_QUERIES: usize = 4;

#[derive(Clone)]
pub enum Handle {
    Rel(Arc<dyn RelationalDriver>),
    Redis(Arc<driver_redis::RedisDriver>),
    Mongo(Arc<driver_mongodb::MongoDriver>),
    Es(Arc<driver_elasticsearch::EsDriver>),
}

impl Handle {
    pub fn kind(&self) -> DbKind {
        match self {
            Handle::Rel(d) => d.kind(),
            Handle::Redis(d) => d.kind(),
            Handle::Mongo(d) => d.kind(),
            Handle::Es(d) => d.kind(),
        }
    }

    pub fn rel(&self) -> Option<Arc<dyn RelationalDriver>> {
        match self {
            Handle::Rel(d) => Some(d.clone()),
            _ => None,
        }
    }

    pub fn redis(&self) -> Option<Arc<driver_redis::RedisDriver>> {
        match self {
            Handle::Redis(d) => Some(d.clone()),
            _ => None,
        }
    }

    pub fn mongo(&self) -> Option<Arc<driver_mongodb::MongoDriver>> {
        match self {
            Handle::Mongo(d) => Some(d.clone()),
            _ => None,
        }
    }

    pub fn es(&self) -> Option<Arc<driver_elasticsearch::EsDriver>> {
        match self {
            Handle::Es(d) => Some(d.clone()),
            _ => None,
        }
    }
}

pub fn build_relational(cfg: &ConnectionConfig, password: String) -> Option<Arc<dyn RelationalDriver>> {
    match cfg.kind {
        DbKind::Sqlite => Some(Arc::new(driver_sqlite::SqliteDriver::new(cfg.clone()))),
        DbKind::Mysql => {
            let mut d = driver_mysql::MysqlDriver::new(cfg.clone());
            d.set_password(password);
            Some(Arc::new(d))
        }
        DbKind::Postgres => {
            let mut d = driver_postgres::PostgresDriver::new(cfg.clone());
            d.set_password(password);
            Some(Arc::new(d))
        }
        DbKind::SqlServer => {
            let mut d = driver_mssql::MssqlDriver::new(cfg.clone());
            d.set_password(password);
            Some(Arc::new(d))
        }
        DbKind::Oracle => {
            let mut d = driver_oracle::OracleDriver::new(cfg.clone());
            d.set_password(password);
            Some(Arc::new(d))
        }
        _ => None,
    }
}

pub struct Registry {
    handles: RwLock<HashMap<ConnId, Handle>>,
    semaphores: RwLock<HashMap<ConnId, Arc<Semaphore>>>,
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

impl Registry {
    pub fn new() -> Self {
        Self { handles: RwLock::new(HashMap::new()), semaphores: RwLock::new(HashMap::new()) }
    }

    pub async fn count(&self) -> usize {
        self.handles.read().await.len()
    }

    pub async fn ids(&self) -> Vec<ConnId> {
        self.handles.read().await.keys().copied().collect()
    }

    pub async fn get(&self, id: ConnId) -> Option<Handle> {
        self.handles.read().await.get(&id).cloned()
    }

    /// 保证某连接已建立，返回可复用句柄。
    pub async fn handle(&self, cfg: &ConnectionConfig, password: String) -> Result<Handle, String> {
        if let Some(existing) = self.get(cfg.id).await {
            return Ok(existing);
        }

        let handle = match cfg.kind {
            DbKind::Redis => {
                let mut d = driver_redis::RedisDriver::new(cfg.clone());
                d.set_password(password);
                d.connect_default().await.map_err(|e| e.to_string())?;
                Handle::Redis(Arc::new(d))
            }
            DbKind::Mongodb => {
                let mut d = driver_mongodb::MongoDriver::new(cfg.clone());
                d.set_password(password);
                d.connect().await.map_err(|e| e.to_string())?;
                Handle::Mongo(Arc::new(d))
            }
            DbKind::Elasticsearch => {
                let mut d = driver_elasticsearch::EsDriver::new(cfg.clone())
                    .map_err(|e| e.to_string())?;
                d.set_password(password);
                d.connect().await.map_err(|e| e.to_string())?;
                Handle::Es(Arc::new(d))
            }
            _ => {
                let driver =
                    build_relational(cfg, password).ok_or_else(|| "不支持的数据库类型".to_string())?;
                driver.connect().await.map_err(|e| e.user_message())?;
                Handle::Rel(driver)
            }
        };

        self.handles.write().await.insert(cfg.id, handle.clone());
        Ok(handle)
    }

    pub async fn disconnect(&self, id: ConnId) {
        if let Some(handle) = self.handles.write().await.remove(&id) {
            match handle {
                Handle::Rel(d) => d.close().await,
                Handle::Redis(d) => d.close().await,
                Handle::Mongo(d) => d.close().await,
                // reqwest::Client 随句柄丢弃自动关闭底层连接池
                Handle::Es(_) => {}
            }
        }
        // 同时清理该连接对应的并发信号量，避免长期驻留
        self.semaphores.write().await.remove(&id);
    }

    pub async fn disconnect_all(&self) {
        let ids: Vec<ConnId> = self.handles.read().await.keys().copied().collect();
        for id in ids {
            self.disconnect(id).await;
        }
    }

    pub async fn permit(&self, id: ConnId) -> OwnedSemaphorePermit {
        let sem = {
            let mut map = self.semaphores.write().await;
            map.entry(id)
                .or_insert_with(|| Arc::new(Semaphore::new(MAX_CONCURRENT_QUERIES)))
                .clone()
        };
        sem.acquire_owned()
            .await
            .unwrap_or_else(|_| panic!("信号量不可关闭"))
    }
}
