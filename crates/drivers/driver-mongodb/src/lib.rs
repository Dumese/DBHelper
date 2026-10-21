//! MongoDB 驱动（官方异步 crate）。
//!
//! 覆盖：库/集合树、文档分页浏览、CRUD、聚合管道、索引列表。

use bson::{doc, Bson, Document};
use dbhelper_models::{ConnectionConfig, DbKind};
use futures_util::TryStreamExt;
use mongodb::options::{FindOptions, IndexOptions};
use mongodb::{Client, Collection, IndexModel};
use serde_json::Value as Json;
use std::time::Instant;
use thiserror::Error;
use tokio::sync::RwLock;

#[derive(Debug, Error)]
pub enum MongoError {
    #[error("连接失败: {0}")]
    Connect(String),
    #[error("未连接")]
    NotConnected,
    #[error("操作失败: {0}")]
    Operation(String),
    #[error("JSON 解析失败: {0}")]
    Json(#[from] serde_json::Error),
}

pub type MongoResult<T> = Result<T, MongoError>;

pub struct MongoDriver {
    config: ConnectionConfig,
    client: RwLock<Option<Client>>,
    password: String,
}

#[derive(Debug, Clone, Default)]
pub struct CollectionStat {
    pub name: String,
    pub count: u64,
    pub avg_size: f64,
}

impl MongoDriver {
    pub fn new(config: ConnectionConfig) -> Self {
        Self { config, client: RwLock::new(None), password: String::new() }
    }

    pub fn kind(&self) -> DbKind {
        DbKind::Mongodb
    }

    pub fn set_password(&mut self, password: String) {
        self.password = password;
    }

    /// 关闭连接：丢弃底层 Client，连接池随之释放。
    pub async fn close(&self) {
        *self.client.write().await = None;
    }

    fn uri(&self) -> String {
        let db = if self.config.database.is_empty() { "admin" } else { &self.config.database };
        if self.config.username.is_empty() {
            format!("mongodb://{}:{}/{}", self.config.host, self.config.port, db)
        } else {
            format!(
                "mongodb://{}:{}@{}:{}/{}?authSource=admin",
                self.config.username,
                urlencode(&self.password),
                self.config.host,
                self.config.port,
                db
            )
        }
    }

    pub async fn connect(&self) -> MongoResult<()> {
        let mut opts = mongodb::options::ClientOptions::parse(self.uri())
            .await
            .map_err(|e| MongoError::Connect(e.to_string()))?;
        opts.connect_timeout = Some(std::time::Duration::from_millis(self.config.connect_timeout_ms));
        opts.app_name = Some("DBHelper".into());
        let client = Client::with_options(opts).map_err(|e| MongoError::Connect(e.to_string()))?;
        client
            .database("admin")
            .run_command(doc! { "ping": 1 }, None)
            .await
            .map_err(|e| MongoError::Connect(e.to_string()))?;
        *self.client.write().await = Some(client);
        Ok(())
    }

    async fn client(&self) -> MongoResult<Client> {
        self.client.read().await.clone().ok_or(MongoError::NotConnected)
    }

    pub async fn ping(&self) -> MongoResult<u64> {
        let client = self.client().await?;
        let start = Instant::now();
        client
            .database("admin")
            .run_command(doc! { "ping": 1 }, None)
            .await
            .map_err(|e| MongoError::Operation(e.to_string()))?;
        Ok(start.elapsed().as_millis() as u64)
    }

    pub async fn databases(&self) -> MongoResult<Vec<String>> {
        let client = self.client().await?;
        let names = client
            .list_database_names(None, None)
            .await
            .map_err(|e| MongoError::Operation(e.to_string()))?;
        Ok(names.into_iter().filter(|n| !matches!(n.as_str(), "admin" | "local" | "config")).collect())
    }

    pub async fn collections(&self, db: &str) -> MongoResult<Vec<CollectionStat>> {
        let client = self.client().await?;
        let database = client.database(db);
        let names = database
            .list_collection_names(None)
            .await
            .map_err(|e| MongoError::Operation(e.to_string()))?;
        let mut out = Vec::new();
        for name in names {
            let coll: Collection<Document> = database.collection(&name);
            let count = coll
                .estimated_document_count(None)
                .await
                .unwrap_or(0);
            out.push(CollectionStat { name, count, avg_size: 0.0 });
        }
        out.sort_by(|a, b| b.count.cmp(&a.count));
        Ok(out)
    }

    pub async fn collection_stats(&self, db: &str, coll: &str) -> MongoResult<Option<Json>> {
        let client = self.client().await?;
        let res = client
            .database(db)
            .run_command(doc! { "collStats": coll }, None)
            .await
            .map_err(|e| MongoError::Operation(e.to_string()))?;
        let doc: Document = res;
        Ok(serde_json::to_value(doc).ok())
    }

    pub async fn find(
        &self,
        db: &str,
        coll: &str,
        filter: Json,
        skip: u64,
        limit: u64,
    ) -> MongoResult<Vec<Json>> {
        let client = self.client().await?;
        let collection: Collection<Document> = client.database(db).collection(coll);
        let filter: Document = json_to_doc(&filter)?;
        let options = FindOptions::builder()
            .skip(Some(skip))
            .limit(Some(limit as i64))
            .build();
        let mut cursor = collection
            .find(filter, options)
            .await
            .map_err(|e| MongoError::Operation(e.to_string()))?;
        let mut out = Vec::new();
        while let Some(doc) = cursor.try_next().await.map_err(|e| MongoError::Operation(e.to_string()))? {
            out.push(serde_json::to_value(doc)?);
        }
        Ok(out)
    }

    pub async fn aggregate(&self, db: &str, coll: &str, pipeline: Json) -> MongoResult<Vec<Json>> {
        let client = self.client().await?;
        let collection: Collection<Document> = client.database(db).collection(coll);
        let stages: Vec<Document> = match pipeline {
            Json::Array(items) => items
                .iter()
                .map(json_to_doc)
                .collect::<MongoResult<Vec<Document>>>()?,
            _ => return Err(MongoError::Operation("聚合管道必须是数组".into())),
        };
        let mut cursor = collection
            .aggregate(stages, None)
            .await
            .map_err(|e| MongoError::Operation(e.to_string()))?;
        let mut out = Vec::new();
        while let Some(doc) = cursor.try_next().await.map_err(|e| MongoError::Operation(e.to_string()))? {
            out.push(serde_json::to_value(doc)?);
        }
        Ok(out)
    }

    pub async fn insert_one(&self, db: &str, coll: &str, doc_json: Json) -> MongoResult<String> {
        let client = self.client().await?;
        let collection: Collection<Document> = client.database(db).collection(coll);
        let document = json_to_doc(&doc_json)?;
        let res = collection
            .insert_one(document, None)
            .await
            .map_err(|e| MongoError::Operation(e.to_string()))?;
        Ok(format!("{:?}", res.inserted_id))
    }

    pub async fn replace_by_id(&self, db: &str, coll: &str, id: Json, doc_json: Json) -> MongoResult<u64> {
        let client = self.client().await?;
        let collection: Collection<Document> = client.database(db).collection(coll);
        let mut document = json_to_doc(&doc_json)?;
        document.insert("_id", json_to_bson(&id)?);
        let filter = doc! { "_id": json_to_bson(&id)? };
        let res = collection
            .replace_one(filter, document, None)
            .await
            .map_err(|e| MongoError::Operation(e.to_string()))?;
        Ok(res.modified_count)
    }

    pub async fn delete_by_id(&self, db: &str, coll: &str, id: Json) -> MongoResult<u64> {
        let client = self.client().await?;
        let collection: Collection<Document> = client.database(db).collection(coll);
        let filter = doc! { "_id": json_to_bson(&id)? };
        let res = collection
            .delete_one(filter, None)
            .await
            .map_err(|e| MongoError::Operation(e.to_string()))?;
        Ok(res.deleted_count)
    }

    pub async fn indexes(&self, db: &str, coll: &str) -> MongoResult<Vec<Json>> {
        let client = self.client().await?;
        let collection: Collection<Document> = client.database(db).collection(coll);
        let mut cursor = collection
            .list_indexes(None)
            .await
            .map_err(|e| MongoError::Operation(e.to_string()))?;
        let mut out = Vec::new();
        while let Some(idx) = cursor.try_next().await.map_err(|e| MongoError::Operation(e.to_string()))? {
            out.push(serde_json::to_value(idx).unwrap_or(Json::Null));
        }
        Ok(out)
    }

    pub async fn create_index(&self, db: &str, coll: &str, keys: Json, unique: bool) -> MongoResult<String> {
        let client = self.client().await?;
        let collection: Collection<Document> = client.database(db).collection(coll);
        let keys: Document = json_to_doc(&keys)?;
        let model = IndexModel::builder()
            .keys(keys)
            .options(IndexOptions::builder().unique(unique).build())
            .build();
        let name = collection
            .create_index(model, None)
            .await
            .map_err(|e| MongoError::Operation(e.to_string()))?;
        Ok(name.index_name)
    }

    pub async fn drop_index(&self, db: &str, coll: &str, name: &str) -> MongoResult<()> {
        let client = self.client().await?;
        let collection: Collection<Document> = client.database(db).collection(coll);
        collection
            .drop_index(name, None)
            .await
            .map_err(|e| MongoError::Operation(e.to_string()))
    }

    pub async fn server_version(&self) -> MongoResult<String> {
        let client = self.client().await?;
        let res = client
            .database("admin")
            .run_command(doc! { "buildInfo": 1 }, None)
            .await
            .map_err(|e| MongoError::Operation(e.to_string()))?;
        Ok(res.get_str("version").unwrap_or("unknown").to_string())
    }
}

pub fn json_to_doc(value: &Json) -> MongoResult<Document> {
    match value {
        Json::Object(map) => {
            let mut doc = Document::new();
            for (k, v) in map {
                doc.insert(k.clone(), json_to_bson(v)?);
            }
            Ok(doc)
        }
        _ => Err(MongoError::Operation("需要一个 JSON 对象".into())),
    }
}

pub fn json_to_bson(value: &Json) -> MongoResult<Bson> {
    Ok(match value {
        Json::Null => Bson::Null,
        Json::Bool(b) => Bson::Boolean(*b),
        Json::Number(n) => {
            if let Some(i) = n.as_i64() {
                Bson::Int64(i)
            } else if let Some(f) = n.as_f64() {
                Bson::Double(f)
            } else {
                Bson::Null
            }
        }
        Json::String(s) => {
            // 支持 Extended JSON 的 $oid / $date
            Bson::String(s.clone())
        }
        Json::Array(items) => Bson::Array(items.iter().map(json_to_bson).collect::<MongoResult<Vec<_>>>()?),
        Json::Object(map) => {
            if map.len() == 1 {
                if let Some(Json::String(hex)) = map.get("$oid") {
                    if let Ok(oid) = hex.parse::<bson::oid::ObjectId>() {
                        return Ok(Bson::ObjectId(oid));
                    }
                }
                if let Some(v) = map.get("$numberLong") {
                    if let Some(s) = v.as_str() {
                        if let Ok(i) = s.parse::<i64>() {
                            return Ok(Bson::Int64(i));
                        }
                    }
                }
            }
            let mut doc = Document::new();
            for (k, v) in map {
                doc.insert(k.clone(), json_to_bson(v)?);
            }
            Bson::Document(doc)
        }
    })
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
