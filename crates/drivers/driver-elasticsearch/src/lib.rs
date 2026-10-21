//! Elasticsearch 驱动。
//!
//! 采用 `reqwest` + `serde_json` 直连 REST（官方 `elasticsearch` crate 与服务端
//! 版本耦合过紧，属规格书允许的兜底方案），完整保留 DSL 查询能力。

use dbhelper_models::{ConnectionConfig, DbKind};
use serde_json::{json, Value as Json};
use std::time::{Duration, Instant};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EsError {
    #[error("连接失败: {0}")]
    Connect(String),
    #[error("请求失败: {0}")]
    Request(String),
    #[error("响应解析失败: {0}")]
    Parse(String),
    #[error("Elasticsearch 返回错误: {0}")]
    Server(String),
}

pub type EsResult<T> = Result<T, EsError>;

#[derive(Debug, Clone, Default)]
pub struct IndexRow {
    pub name: String,
    pub health: String,
    pub docs: u64,
    pub store: String,
}

#[derive(Debug, Clone, Default)]
pub struct SearchHits {
    pub took_ms: u64,
    pub total: u64,
    pub hits: Vec<Json>,
    pub timed_out: bool,
}

pub struct EsDriver {
    config: ConnectionConfig,
    client: reqwest::Client,
    password: String,
}

impl EsDriver {
    pub fn new(config: ConnectionConfig) -> EsResult<Self> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            reqwest::header::HeaderValue::from_static("application/json"),
        );
        let client = reqwest::Client::builder()
            .default_headers(headers)
            .danger_accept_invalid_certs(matches!(
                config.ssl,
                dbhelper_models::SslMode::Preferred
            ))
            .timeout(Duration::from_millis(config.statement_timeout_ms.max(5_000)))
            .build()
            .map_err(|e| EsError::Connect(e.to_string()))?;
        Ok(Self { config, client, password: String::new() })
    }

    pub fn kind(&self) -> DbKind {
        DbKind::Elasticsearch
    }

    pub fn set_password(&mut self, password: String) {
        self.password = password;
    }

    fn base(&self) -> String {
        let scheme = if self.config.ssl == dbhelper_models::SslMode::Disabled {
            "http"
        } else {
            "https"
        };
        format!("{scheme}://{}:{}", self.config.host, self.config.port)
    }

    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let url = format!("{}{}", self.base(), path);
        let mut req = self.client.request(method, url);
        if !self.config.username.is_empty() {
            req = req.basic_auth(&self.config.username, Some(&self.password));
        }
        req
    }

    pub async fn connect(&self) -> EsResult<()> {
        self.ping().await.map(|_| ())
    }

    pub async fn ping(&self) -> EsResult<u64> {
        let start = Instant::now();
        let res = self
            .request(reqwest::Method::GET, "/")
            .send()
            .await
            .map_err(|e| EsError::Connect(e.to_string()))?;
        if !res.status().is_success() {
            return Err(EsError::Connect(format!("HTTP {}", res.status())));
        }
        let body: Json = res.json().await.map_err(|e| EsError::Parse(e.to_string()))?;
        if let Some(err) = body.get("error") {
            return Err(EsError::Connect(err.to_string()));
        }
        Ok(start.elapsed().as_millis() as u64)
    }

    pub async fn cluster_info(&self) -> EsResult<Json> {
        let res = self
            .request(reqwest::Method::GET, "/")
            .send()
            .await
            .map_err(|e| EsError::Request(e.to_string()))?;
        Self::json(res).await
    }

    async fn json(res: reqwest::Response) -> EsResult<Json> {
        let status = res.status();
        let text = res.text().await.map_err(|e| EsError::Request(e.to_string()))?;
        let value: Json = serde_json::from_str(&text)
            .map_err(|e| EsError::Parse(format!("{e}: {}", text.chars().take(200).collect::<String>())))?;
        if !status.is_success() {
            let reason = value
                .pointer("/error/reason")
                .and_then(|v| v.as_str())
                .unwrap_or("请求被拒绝");
            return Err(EsError::Server(format!("{reason} (HTTP {status})")));
        }
        Ok(value)
    }

    pub async fn indices(&self) -> EsResult<Vec<IndexRow>> {
        let path = "/_cat/indices?format=json&h=index,health,docs.count,store.size&s=index";
        let res = self
            .request(reqwest::Method::GET, path)
            .send()
            .await
            .map_err(|e| EsError::Request(e.to_string()))?;
        let value = Self::json(res).await?;
        let mut out = Vec::new();
        if let Some(arr) = value.as_array() {
            for item in arr {
                let name = item.get("index").and_then(|v| v.as_str()).unwrap_or("").to_string();
                if name.starts_with('.') {
                    continue;
                }
                out.push(IndexRow {
                    name,
                    health: item.get("health").and_then(|v| v.as_str()).unwrap_or("-").to_string(),
                    docs: item
                        .get("docs.count")
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0),
                    store: item.get("store.size").and_then(|v| v.as_str()).unwrap_or("-").to_string(),
                });
            }
        }
        Ok(out)
    }

    pub async fn search(&self, index: &str, dsl: &Json) -> EsResult<SearchHits> {
        let path = format!("/{}/_search", urlencode(index));
        let res = self
            .request(reqwest::Method::POST, &path)
            .json(dsl)
            .send()
            .await
            .map_err(|e| EsError::Request(e.to_string()))?;
        let value = Self::json(res).await?;
        let took = value.get("took").and_then(|v| v.as_u64()).unwrap_or(0);
        let timed_out = value.get("timed_out").and_then(|v| v.as_bool()).unwrap_or(false);
        let total = value
            .pointer("/hits/total/value")
            .and_then(|v| v.as_u64())
            .or_else(|| value.pointer("/hits/total").and_then(|v| v.as_u64()))
            .unwrap_or(0);
        let hits = value
            .pointer("/hits/hits")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|hit| {
                        let mut obj = serde_json::Map::new();
                        if let Some(score) = hit.get("_score") {
                            obj.insert("_score".into(), score.clone());
                        }
                        if let Some(id) = hit.get("_id") {
                            obj.insert("_id".into(), id.clone());
                        }
                        if let Some(src) = hit.get("_source") {
                            if let Some(map) = src.as_object() {
                                for (k, v) in map {
                                    obj.insert(k.clone(), v.clone());
                                }
                            }
                        }
                        Json::Object(obj)
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(SearchHits { took_ms: took, total, hits, timed_out })
    }

    pub async fn mapping(&self, index: &str) -> EsResult<Json> {
        let path = format!("/{}/_mapping", urlencode(index));
        let res = self
            .request(reqwest::Method::GET, &path)
            .send()
            .await
            .map_err(|e| EsError::Request(e.to_string()))?;
        Self::json(res).await
    }

    pub async fn count(&self, index: &str) -> EsResult<u64> {
        let path = format!("/{}/_count", urlencode(index));
        let res = self
            .request(reqwest::Method::GET, &path)
            .send()
            .await
            .map_err(|e| EsError::Request(e.to_string()))?;
        let value = Self::json(res).await?;
        Ok(value.get("count").and_then(|v| v.as_u64()).unwrap_or(0))
    }

    pub async fn create_index(&self, index: &str, body: Json) -> EsResult<()> {
        let path = format!("/{}", urlencode(index));
        let res = self
            .request(reqwest::Method::PUT, &path)
            .json(&body)
            .send()
            .await
            .map_err(|e| EsError::Request(e.to_string()))?;
        Self::json(res).await.map(|_| ())
    }

    pub async fn delete_index(&self, index: &str) -> EsResult<()> {
        let path = format!("/{}", urlencode(index));
        let res = self
            .request(reqwest::Method::DELETE, &path)
            .send()
            .await
            .map_err(|e| EsError::Request(e.to_string()))?;
        Self::json(res).await.map(|_| ())
    }

    /// 校验 DSL 是否为合法 JSON 对象。
    pub fn validate_dsl(text: &str) -> Result<Json, String> {
        let value: Json = serde_json::from_str(text).map_err(|e| format!("DSL 不是合法 JSON：{e}"))?;
        if !value.is_object() {
            return Err("DSL 顶层必须是 JSON 对象".into());
        }
        Ok(value)
    }

    pub fn default_dsl(index: &str) -> Json {
        json!({
            "query": { "match_all": {} },
            "size": 50,
            "sort": [ { "@timestamp": { "order": "desc" } } ],
            "_source_hint": index
        })
    }
}

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'*' | b',' => {
                out.push(b as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}
