//! 核心服务层：应用装配、连接注册表、查询编排、度量、导入导出、调度。
//!
//! 设计约束（对应需求 §三 / §七）：
//! - 所有数据库 I/O 在 tokio 任务中执行，UI 线程零阻塞；
//! - 查询结果经**有界通道**（容量 8 批 × 200 行）回传，天然背压；
//! - 单连接并发查询受信号量限制（≤4），超限排队而非压垮数据库；
//! - 单库异常被隔离为 `StreamEvent::Failed`，不影响其它连接与 UI。

pub mod export;
pub mod registry;
pub mod store;

use dbhelper_models::*;
use dbhelper_utils::{AppDirs, Vault};
use registry::{Handle, Registry};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use store::Store;
use tokio::sync::{mpsc, RwLock};

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Store(#[from] store::StoreError),
}

pub type CoreResult<T> = Result<T, CoreError>;

/// 运行期度量（状态栏与监控页共用）。
#[derive(Debug, Clone, Default)]
pub struct Metrics {
    pub rss_mb: f32,
    pub connections: usize,
    pub last_query_ms: u64,
    pub query_total: u64,
    /// 每秒 QPS 采样（最近 120 个点）
    pub qps: VecDeque<f32>,
    /// 活跃连接采样（最近 120 个点）
    pub conns: VecDeque<f32>,
    /// 最近查询耗时（慢查询 Top N 数据源）
    pub recent: VecDeque<SlowQuery>,
    pub alert: Option<String>,
    pub peak_qps: f32,
}

pub struct AppCore {
    pub dirs: AppDirs,
    pub store: Store,
    pub vault: std::sync::Mutex<Vault>,
    pub settings: parking_lot::RwLock<Settings>,
    pub registry: Registry,
    pub metrics: Arc<RwLock<Metrics>>,
    pub connections: parking_lot::RwLock<Vec<ConnectionConfig>>,
    query_counter: Arc<AtomicU64>,
}

impl AppCore {
    pub async fn boot() -> CoreResult<Arc<Self>> {
        let dirs = dbhelper_utils::app_dirs();
        dbhelper_utils::init_tracing(&dirs.logs);

        tracing::info!("boot: 打开本地库 {}", dirs.app_db.display());
        let store = Store::open(&dirs.app_db).await?;
        tracing::info!("boot: 打开 Vault");
        let vault = Vault::open(&dirs.vault)
            .map_err(|e| CoreError::Message(format!("Vault 初始化失败：{e}")))?;

        tracing::info!("boot: 读取设置");
        let settings = match store.meta_get("settings").await? {
            Some(json) => serde_json::from_str(&json).unwrap_or_default(),
            None => {
                let mut s = Settings::default();
                s.log_dir = dirs.logs.to_string_lossy().to_string();
                s.config_path = dirs.config.to_string_lossy().to_string();
                let _ = store
                    .meta_set("settings", &serde_json::to_string(&s).unwrap_or_default())
                    .await;
                s
            }
        };

        tracing::info!("boot: 读取连接");
        let connections = store.load_connections().await?;

        for group in connections.iter().map(|c| c.group.clone()).collect::<Vec<_>>() {
            if !group.is_empty() {
                let _ = store.ensure_group(&group).await;
            }
        }

        let core = Arc::new(Self {
            dirs,
            store,
            vault: std::sync::Mutex::new(vault),
            settings: parking_lot::RwLock::new(settings),
            registry: Registry::new(),
            metrics: Arc::new(RwLock::new(Metrics::default())),
            connections: parking_lot::RwLock::new(connections),
            query_counter: Arc::new(AtomicU64::new(0)),
        });

        tracing::info!("boot: 完成，连接数 {}", core.connections().len());
        core.spawn_sampler();
        Ok(core)
    }

    fn spawn_sampler(self: &Arc<Self>) {
        let core = self.clone();
        tokio::spawn(async move {
            let mut sys = sysinfo::System::new();
            let pid = sysinfo::get_current_pid().ok();
            let mut ticker = tokio::time::interval(Duration::from_secs(1));
            let mut last_total = 0u64;
            loop {
                ticker.tick().await;
                // 只刷新自身进程——刷新全系统进程列表每秒开销可达上百毫秒
                if let Some(pid) = pid {
                    sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]), true);
                }
                let rss_mb = pid
                    .and_then(|pid| sys.process(pid))
                    .map(|p| p.memory() as f32 / 1024.0 / 1024.0)
                    .unwrap_or_else(|| sys.used_memory() as f32 / 1024.0 / 1024.0);
                let total = core.query_counter.load(Ordering::Relaxed);
                let delta = total.saturating_sub(last_total);
                last_total = total;
                let conns = core.registry.count().await as f32;
                let limit = core.settings.read().memory_limit_mb;

                tracing::debug!("sampler: rss={rss_mb:.1}MB conns={conns} qps_delta={delta}");
                let mut m = core.metrics.write().await;
                m.rss_mb = rss_mb;
                m.connections = conns as usize;
                m.query_total = total;
                if m.qps.len() >= 120 {
                    m.qps.pop_front();
                }
                m.qps.push_back(delta as f32);
                if m.conns.len() >= 120 {
                    m.conns.pop_front();
                }
                m.conns.push_back(conns);
                m.peak_qps = m.qps.iter().cloned().fold(0.0f32, f32::max);
                m.alert = if rss_mb as u64 > limit {
                    Some(format!(
                        "驻留内存 {rss_mb:.0} MB 已超过阈值 {limit} MB，后台任务将被中止"
                    ))
                } else {
                    None
                };
            }
        });
    }

    pub fn snapshot_metrics(&self) -> Metrics {
        self.metrics.try_read().map(|m| m.clone()).unwrap_or_default()
    }

    pub async fn save_settings(&self, settings: &Settings) -> CoreResult<()> {
        let json = serde_json::to_string(settings).map_err(|e| CoreError::Message(e.to_string()))?;
        self.store.meta_set("settings", &json).await?;
        *self.settings.write() = settings.clone();
        Ok(())
    }

    pub fn connections(&self) -> Vec<ConnectionConfig> {
        self.connections.read().clone()
    }

    pub fn connection(&self, id: ConnId) -> Option<ConnectionConfig> {
        self.connections.read().iter().find(|c| c.id == id).cloned()
    }

    pub async fn save_connection(&self, cfg: &ConnectionConfig, password: Option<&str>) -> CoreResult<()> {
        if let Some(pwd) = password {
            if let Ok(mut vault) = self.vault.lock() {
                if pwd.is_empty() {
                    vault.remove(&cfg.secret_key());
                } else {
                    vault.set(cfg.secret_key(), pwd);
                }
                let _ = vault.save();
            }
        }
        self.store.save_connection(cfg).await?;
        {
            let mut list = self.connections.write();
            match list.iter_mut().find(|c| c.id == cfg.id) {
                Some(existing) => *existing = cfg.clone(),
                None => list.push(cfg.clone()),
            }
        }
        if !cfg.group.is_empty() {
            let _ = self.store.ensure_group(&cfg.group).await;
        }
        Ok(())
    }

    pub async fn delete_connection(&self, id: ConnId) -> CoreResult<()> {
        self.registry.disconnect(id).await;
        self.store.delete_connection(id).await?;
        self.connections.write().retain(|c| c.id != id);
        Ok(())
    }

    pub fn password(&self, cfg: &ConnectionConfig) -> String {
        self.vault
            .lock()
            .ok()
            .and_then(|v| v.get(&cfg.secret_key()).map(str::to_string))
            .unwrap_or_default()
    }

    pub fn vault_status(&self) -> String {
        self.vault
            .lock()
            .map(|v| v.status_label())
            .unwrap_or_else(|_| "Vault 已锁定".into())
    }

    pub fn vault_len(&self) -> usize {
        self.vault.lock().map(|v| v.len()).unwrap_or(0)
    }

    /// 建立/复用连接。失败时返回可读错误，不 panic、不影响其它连接。
    pub async fn ensure_connected(self: &Arc<Self>, id: ConnId) -> CoreResult<Handle> {
        let cfg = self.connection(id).ok_or_else(|| CoreError::Message("连接不存在".into()))?;
        let password = self.password(&cfg);
        self.registry
            .handle(&cfg, password)
            .await
            .map_err(CoreError::Message)
    }

    pub async fn disconnect(&self, id: ConnId) {
        self.registry.disconnect(id).await;
    }

    /// 连接测试：只做握手与心跳，不执行任何写操作。
    pub async fn test_connection(
        self: &Arc<Self>,
        cfg: &ConnectionConfig,
        password: &str,
    ) -> Result<String, String> {
        match registry::build_relational(cfg, password.to_string()) {
            Some(driver) => {
                driver.connect().await.map_err(|e| e.user_message())?;
                let ms = driver.ping().await.map_err(|e| e.user_message())?;
                let info = driver.server_info().await.map(|i| i.version).unwrap_or_default();
                driver.close().await;
                Ok(format!(
                    "{} · 往返 {ms} ms",
                    if info.is_empty() { cfg.kind.label().to_string() } else { info }
                ))
            }
            None => match cfg.kind {
                DbKind::Redis => {
                    let mut d = driver_redis::RedisDriver::new(cfg.clone());
                    d.set_password(password.to_string());
                    d.connect_default().await.map_err(|e| e.to_string())?;
                    let ms = d.ping().await.map_err(|e| e.to_string())?;
                    let info = d.info().await.map(|i| i.version).unwrap_or_default();
                    Ok(format!("{info} · 往返 {ms} ms"))
                }
                DbKind::Mongodb => {
                    let mut d = driver_mongodb::MongoDriver::new(cfg.clone());
                    d.set_password(password.to_string());
                    d.connect().await.map_err(|e| e.to_string())?;
                    let ms = d.ping().await.map_err(|e| e.to_string())?;
                    let v = d.server_version().await.unwrap_or_default();
                    Ok(format!("MongoDB {v} · 往返 {ms} ms"))
                }
                DbKind::Elasticsearch => {
                    let mut d = driver_elasticsearch::EsDriver::new(cfg.clone()).map_err(|e| e.to_string())?;
                    d.set_password(password.to_string());
                    let ms = d.ping().await.map_err(|e| e.to_string())?;
                    let v = d
                        .cluster_info()
                        .await
                        .ok()
                        .and_then(|j| j.pointer("/version/number").and_then(|v| v.as_str()).map(str::to_string))
                        .unwrap_or_else(|| cfg.kind.label().into());
                    Ok(format!("Elasticsearch {v} · 往返 {ms} ms"))
                }
                _ => Err("该连接类型暂不支持连接测试".into()),
            },
        }
    }

    /// 启动一次流式查询，返回接收端。UI 侧丢弃该接收端即等于「停止」。
    pub async fn run_query(
        self: &Arc<Self>,
        conn_id: ConnId,
        sql: String,
    ) -> CoreResult<mpsc::Receiver<StreamEvent>> {
        let cfg = self.connection(conn_id).ok_or_else(|| CoreError::Message("连接不存在".into()))?;

        let statements = dbhelper_utils::sql::split_statements(&sql);
        if statements.is_empty() {
            return Err(CoreError::Message("没有可执行的语句".into()));
        }
        if cfg.readonly {
            if let Some((_, stmt)) = statements
                .iter()
                .find(|(_, s)| !dbhelper_utils::sql::is_read_only(s))
            {
                let head: String = stmt.chars().take(48).collect();
                return Err(CoreError::Message(format!(
                    "只读连接禁止写操作：{}…",
                    head.replace('\n', " ")
                )));
            }
        }

        let handle = self.ensure_connected(conn_id).await?;
        let permit = self.registry.permit(conn_id).await;
        let (out_tx, out_rx) = mpsc::channel::<StreamEvent>(8);
        let core = self.clone();
        let conn_name = cfg.name.clone();
        let limit = cfg.max_rows.max(1);
        let timeout = cfg.statement_timeout();

        tokio::spawn(async move {
            let _permit = permit;
            core.query_counter.fetch_add(1, Ordering::Relaxed);
            let start = std::time::Instant::now();
            let mut total_rows = 0u64;
            let mut ok = true;
            let mut error: Option<String> = None;

            let last = statements.len() - 1;
            for (idx, (_, stmt)) in statements.iter().enumerate() {
                if idx < last {
                    if let Handle::Rel(driver) = &handle {
                        if let Err(e) = driver.execute(stmt, timeout).await {
                            ok = false;
                            error = Some(e.user_message());
                            let _ = out_tx.send(StreamEvent::Failed(e.user_message())).await;
                            break;
                        }
                    }
                }
            }

            if ok {
                let target_sql = statements[last].1.clone();
                let from_table = rel_common::table_from_query(&target_sql);
                let (inner_tx, mut inner_rx) = mpsc::channel::<StreamEvent>(8);
                let stream_result: Result<(), String> = match &handle {
                    Handle::Rel(driver) => driver
                        .stream(target_sql.clone(), limit, timeout, inner_tx)
                        .await
                        .map_err(|e| e.user_message()),
                    _ => Err("该连接不支持 SQL 执行，请使用对应的专用视图".into()),
                };

                if let Err(e) = stream_result {
                    let _ = out_tx.send(StreamEvent::Failed(e.clone())).await;
                    ok = false;
                    error = Some(e);
                } else {
                    while let Some(mut event) = inner_rx.recv().await {
                        if let StreamEvent::Columns { columns, .. } = &mut event {
                            if cfg.kind == DbKind::Sqlite {
                                if let (Some(table), Handle::Rel(driver)) = (&from_table, &handle) {
                                    if let Ok(def) = driver.describe_table(None, table).await {
                                        for col in columns.iter_mut() {
                                            if let Some(known) = def.columns.iter().find(|c| c.name == col.name) {
                                                col.type_name = known.data_type.clone();
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if let StreamEvent::Batch(rows) = &event {
                            total_rows += rows.len() as u64;
                        }
                        let terminal = matches!(event, StreamEvent::Done { .. } | StreamEvent::Failed(_));
                        if let StreamEvent::Failed(e) = &event {
                            ok = false;
                            error = Some(e.clone());
                        }
                        if out_tx.send(event).await.is_err() {
                            break;
                        }
                        if terminal {
                            break;
                        }
                    }
                }
            }

            let elapsed = start.elapsed().as_millis() as u64;
            if let Ok(mut m) = core.metrics.try_write() {
                m.last_query_ms = elapsed;
                m.recent.push_front(SlowQuery {
                    elapsed_secs: elapsed as f64 / 1000.0,
                    session_id: conn_id.to_string(),
                    sql: statements.last().map(|s| s.1.clone()).unwrap_or_default(),
                });
                while m.recent.len() > 40 {
                    m.recent.pop_back();
                }
            }
            let entry = HistoryEntry {
                id: 0,
                conn_id,
                conn_name,
                sql,
                executed_at: dbhelper_utils::textfmt::now_secs(),
                elapsed_ms: elapsed,
                rows: total_rows,
                ok,
                error,
            };
            let _ = core.store.push_history(&entry).await;
        });

        Ok(out_rx)
    }

    /// 高危语句影响行数预估。
    pub async fn estimate_affected(self: &Arc<Self>, conn_id: ConnId, sql: &str) -> Option<u64> {
        let handle = self.ensure_connected(conn_id).await.ok()?;
        if let Handle::Rel(driver) = handle {
            driver.estimate_affected(sql).await.ok().flatten()
        } else {
            None
        }
    }
}
