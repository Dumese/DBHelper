# DBHelper

> Windows 原生数据库连接与管理工具 · Rust + iced（无 Electron / 无 WebView / 无运行时）

DBHelper 对标 Navicat 与 DBeaver，采用 Notion 式面包屑导航，覆盖 5 种关系型数据库
（MySQL、PostgreSQL、Oracle、SQL Server、SQLite）与 3 种 NoSQL（Redis、MongoDB、
Elasticsearch），共 **8 种数据库**全部内置驱动，不通过 feature flag 裁剪。

---

## 1. 构建与运行

前置条件：

- Windows 10/11 x64
- Rust stable（MSVC toolchain，本文档验证于 1.99.1）
- Visual Studio Build Tools（MSVC 链接器）

```bash
# 1) 主程序（release）
cargo build --release -p dbhelper
# 产物：target/release/dbhelper.exe

# 2) 安装包（单文件 setup.exe，内嵌主程序）
cargo build --release -p dbhelper-installer
# 产物：target/release/dbhelper-setup.exe
```

> 安装器在编译期通过 `include_bytes!` 嵌入 `target/release/dbhelper.exe`，
> 因此**必须先构建主程序**再构建安装器。安装器会复制程序、创建桌面与开始菜单
> 快捷方式，并写入 `HKCU\...\Uninstall\DBHelper` 卸载项；
> 卸载命令为 `dbhelper.exe --uninstall`。

开发模式：

```bash
cargo run -p dbhelper
```

## 2. 目录结构

```
dbhelper/
├─ crates/
│  ├─ app/                      入口、窗口、--uninstall
│  ├─ ui/                       设计令牌、组件、自绘编辑器/网格/图表/ER、10 个界面
│  ├─ core/                     应用装配、连接注册表、查询编排、导出、调度、本地库
│  ├─ models/                   领域模型（连接 / 结果集 / 设置 / 历史）
│  ├─ utils/                    Vault、SQL 词法分析、路径、格式化、日志
│  └─ drivers/
│     ├─ rel_common/            关系型统一 trait：RelationalDriver / RowWriter
│     ├─ driver-sqlite/         sqlx + 参数化写入（本机可验证）
│     ├─ driver-mysql/          sqlx
│     ├─ driver-postgres/       sqlx
│     ├─ driver-mssql/          tiberius（纯 Rust TDS）
│     ├─ driver-oracle/         rust-oracle（ODPI-C，运行期需 Instant Client）
│     ├─ driver-redis/          redis-rs（键空间 / 五种类型 / TTL）
│     ├─ driver-mongodb/        mongodb 官方异步驱动
│     └─ driver-elasticsearch/  reqwest + serde_json（REST DSL）
├─ tests/                       驱动冒烟测试
└─ installer/                   setup.exe（iced 编写，内嵌负载）
```

## 3. 八种数据库支持

| 数据库 | 驱动 | 连接 | 对象浏览 | 查询 | 说明 |
|---|---|---|---|---|---|
| MySQL | sqlx | ✅ | ✅ | ✅ + 流式 + 行内编辑* | |
| PostgreSQL | sqlx | ✅ | ✅ | ✅ + 流式 + 行内编辑* | |
| SQLite | sqlx | ✅ | ✅ | ✅ + 流式 + 行内编辑 | |
| SQL Server | tiberius | ✅ | ✅ | ✅ | 纯 Rust TDS，不依赖 ODBC |
| Oracle | rust-oracle | ✅ | ✅ | ✅ | 运行期需 Oracle Instant Client（23ai 支持 thin 模式） |
| Redis | redis-rs | ✅ | 键空间 db0–db15 | 五种类型读写 / TTL / 命令 | |
| MongoDB | mongodb | ✅ | 库 / 集合 / 文档 | 查询、聚合管道、删除 | |
| Elasticsearch | reqwest | ✅ | 索引列表 | DSL 查询 / 命中结果 | |

\* 行内编辑的参数化写入当前在 SQLite 驱动中完整实现（`RowWriter`），
MySQL / PostgreSQL 已有查询与浏览能力，行内提交会在 UI 中明确提示
「该驱动暂不支持行内编辑」，不会静默失败。

## 4. 安全设计

- 连接密码只存在于内存与 AES-256-GCM 密文中，Vault 文件为 `%APPDATA%\DBHelper\vault.bin`；
- 主密钥优先存 Windows Credential Manager，不可用时退化为 DPAPI（当前用户绑定）加密文件；
- 只读连接在 `core::run_query` 执行前拦截写语句（不依赖 UI 判断）；
- `DROP / TRUNCATE / DELETE / UPDATE / ALTER / GRANT` 触发二次确认，并异步预估影响行数；
- 行内提交与批量导入使用 `sqlx::QueryBuilder` 参数化绑定，无字符串拼接注入面；
- TLS 默认 `Preferred`，`VerifyFull` 时不做 `trust_cert`。

## 5. 性能与稳定性

- 查询结果经有界通道（容量 8 × 200 行）回传，UI 侧丢弃接收端即为「停止」；
- 单连接并发查询受 `Semaphore(4)` 限制；
- 每条语句有 `statement_timeout`（默认 30s）与 `max_rows`（默认 1000）双重限制；
- 状态栏由 `sysinfo` 每秒采样内存，超过阈值（默认 500MB）给出告警；
- 单库异常被隔离为 `StreamEvent::Failed`，不影响其它连接与 UI 线程。

## 6. 首启动行为

首次启动为空状态：不预置任何连接与演示数据。在连接页新建连接即可开始使用；
连接配置、分组与设置保存在 `%APPDATA%\DBHelper\dbhelper.db`。

## 7. 测试

```bash
cargo test -p dbhelper-utils -p driver-sqlite      # 单元测试 + SQLite 冒烟
cargo test --test drivers_smoke -- --ignored       # 其余 7 种（需自备服务端与环境变量）
```

环境变量：`DBHELPER_MYSQL_URL`、`DBHELPER_PG_URL`、`DBHELPER_MSSQL_HOST`、
`DBHELPER_ORACLE_DSN`、`DBHELPER_REDIS_URL`、`DBHELPER_MONGO_URI`、`DBHELPER_ES_URL`。

## 8. 与原型/需求文档的偏差

见交付说明中的「待确认项与取舍」清单。

## 9. 外观、字体与窗口

- 设计尺寸 1440×900（原型图同尺寸），最小 1200×760；整体按原型的外壳度量排布
  （标题栏 32、面包屑 40、页签 36、侧栏 240、右侧面板 360、表格行高 26）。
- 字体遵循设计规范：界面 **Noto Sans SC**、衬线 **Noto Serif SC**、等宽 **JetBrains Mono**；
  旧配置中的 Inter/Consolas 会在启动时自动迁移。
- 无边框窗口自绘标题栏：右上角按钮与 **F11** 最大化（按工作区，Win32 `ShowWindow`），
  右边缘/下边缘/右下角可拖拽缩放；编辑器与结果区按 2:3 分配高度，网格列宽随窗口自适应。

## 10. 开发与验证脚本

仓库内附带两个仅用于开发期实机验证的脚本（不参与打包）：

```bash
# 启动 → 等待 → PrintWindow 截图 → 退出（一次完成，避免子进程随终端回收）
python scripts/capture.py .shots/query.png 8

# 脚本化交互：启动 → 点击/截图序列 → 退出
python scripts/ui.py run .shots/actions.txt
```

`actions.txt` 支持 `wait <秒>`、`click <x> <y>`、`move <x> <y>`、`key <SendKeys>`、`shot <png>`，
坐标按 iced 的逻辑像素（窗口 1080×680）给出，脚本内部按窗口 DPI（150%）换算成物理像素。

实机截图见 `docs/screenshots/`。
