//! SQL 词法分析、语法着色、语句切分、危险语句识别与格式化。
//!
//! 全部为纯函数，UI 层（编辑器着色）与 core 层（安全校验）共用同一套规则，
//! 避免「显示一种、执行另一种」的偏差。

use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Keyword,
    Type,
    Function,
    Str,
    Number,
    Comment,
    Operator,
    Punct,
    Ident,
    Variable,
    Space,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct LexState {
    pub in_block_comment: bool,
    pub in_string: Option<char>,
}

const KEYWORDS: &[&str] = &[
    "ADD", "AFTER", "ALL", "ALTER", "ANALYZE", "AND", "AS", "ASC", "BEFORE", "BEGIN", "BETWEEN",
    "BY", "CALL", "CASCADE", "CASE", "CAST", "CHANGE", "CHARACTER", "CHECK", "COLLATE", "COLUMN",
    "COMMIT", "CONSTRAINT", "CREATE", "CROSS", "CURRENT_DATE", "CURRENT_TIME", "CURRENT_TIMESTAMP",
    "DATABASE", "DEFAULT", "DELETE", "DESC", "DESCRIBE", "DISTINCT", "DROP", "ELSE", "END",
    "ENGINE", "ESCAPE", "EXCEPT", "EXISTS", "EXPLAIN", "FALSE", "FETCH", "FOR", "FOREIGN", "FROM",
    "FULL", "FULLTEXT", "FUNCTION", "GRANT", "GROUP", "HAVING", "IF", "IGNORE", "IN", "INDEX",
    "INNER", "INSERT", "INTERSECT", "INTO", "IS", "JOIN", "KEY", "LEFT", "LIKE", "LIMIT", "LOCK",
    "MERGE", "NATURAL", "NOT", "NULL", "OFFSET", "ON", "OR", "ORDER", "OUTER", "OVER", "PARTITION",
    "PRIMARY", "PROCEDURE", "REFERENCES", "RENAME", "REPLACE", "RETURNING", "REVOKE", "RIGHT",
    "ROLLBACK", "ROW", "ROWS", "SAVEPOINT", "SCHEMA", "SELECT", "SET", "SHOW", "TABLE", "TABLES",
    "TEMPORARY", "THEN", "TO", "TOP", "TRANSACTION", "TRIGGER", "TRUE", "TRUNCATE", "UNION",
    "UNIQUE", "UNSIGNED", "UPDATE", "USE", "USING", "VALUES", "VIEW", "WHEN", "WHERE", "WITH",
    "AUTO_INCREMENT", "COMMENT", "MODIFY", "START", "USER", "GRANTED", "IDENTIFIED", "PASSWORD",
];

const TYPES: &[&str] = &[
    "BIGINT", "BINARY", "BIT", "BLOB", "BOOL", "BOOLEAN", "CHAR", "DATE", "DATETIME", "DECIMAL",
    "DOUBLE", "ENUM", "FLOAT", "INT", "INTEGER", "JSON", "JSONB", "LONGTEXT", "MEDIUMINT",
    "MEDIUMTEXT", "NUMBER", "NUMERIC", "REAL", "SMALLINT", "TEXT", "TIME", "TIMESTAMP",
    "TIMESTAMPTZ", "TINYINT", "TINYTEXT", "UUID", "VARBINARY", "VARCHAR", "VARCHAR2", "XML",
    "NVARCHAR", "NVARCHAR2", "CLOB", "RAW", "MONEY", "BYTEA", "SERIAL", "BIGSERIAL",
];

const FUNCTIONS: &[&str] = &[
    "AVG", "COUNT", "MAX", "MIN", "SUM", "COALESCE", "CONCAT", "DATE_FORMAT", "IFNULL",
    "NOW", "ROUND", "SUBSTRING", "TRIM", "UPPER", "LOWER", "LENGTH", "GROUP_CONCAT", "JSON_EXTRACT",
    "ROW_NUMBER", "RANK", "DENSE_RANK", "LAG", "LEAD", "ABS", "FLOOR", "CEIL", "DATE", "YEAR",
    "MONTH", "DAY", "STRFTIME", "TOTAL", "RANDOM", "CAST", "NULLIF", "GREATEST", "LEAST",
    "SUM", "STDDEV", "VARIANCE", "TO_CHAR", "TO_DATE", "NVL", "DECODE", "SYSDATE", "GETDATE",
];

fn is_word_start(c: char) -> bool {
    c.is_alphabetic() || c == '_' || c == '@' || c == '#' || c == '$'
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$' || c == '@' || c == '#'
}

/// 对单行着色。跨行的块注释与字符串通过 `state` 传递。
pub fn highlight_line(line: &str, state: &mut LexState) -> Vec<(String, TokenKind)> {
    let chars: Vec<char> = line.chars().collect();
    let mut out: Vec<(String, TokenKind)> = Vec::new();
    let mut buf = String::new();
    let mut buf_kind: Option<TokenKind> = None;
    let mut i = 0usize;

    macro_rules! flush {
        () => {
            if !buf.is_empty() {
                out.push((std::mem::take(&mut buf), buf_kind.unwrap_or(TokenKind::Ident)));
                buf_kind = None;
            }
        };
    }

    fn push(buf: &mut String, kind: &mut Option<TokenKind>, k: TokenKind, c: char) {
        // 类型切换时由调用方负责 flush（见运算符/标点分支的 next_kind 判断），
        // 这里只记录类型并累积字符。
        *kind = Some(k);
        buf.push(c);
    }

    while i < chars.len() {
        let c = chars[i];

        // 块注释延续
        if state.in_block_comment {
            if c == '*' && chars.get(i + 1) == Some(&'/') {
                push(&mut buf, &mut buf_kind, TokenKind::Comment, c);
                push(&mut buf, &mut buf_kind, TokenKind::Comment, '/');
                i += 2;
                state.in_block_comment = false;
            } else {
                push(&mut buf, &mut buf_kind, TokenKind::Comment, c);
                i += 1;
            }
            continue;
        }

        // 字符串延续
        if let Some(q) = state.in_string {
            if c == q {
                push(&mut buf, &mut buf_kind, TokenKind::Str, c);
                // 处理 '' 转义
                if chars.get(i + 1) == Some(&q) {
                    push(&mut buf, &mut buf_kind, TokenKind::Str, q);
                    i += 2;
                    continue;
                }
                state.in_string = None;
            } else {
                push(&mut buf, &mut buf_kind, TokenKind::Str, c);
            }
            i += 1;
            continue;
        }

        // 行注释
        if c == '-' && chars.get(i + 1) == Some(&'-') {
            flush!();
            let rest: String = chars[i..].iter().collect();
            out.push((rest, TokenKind::Comment));
            return out;
        }

        // 块注释开始
        if c == '/' && chars.get(i + 1) == Some(&'*') {
            flush!();
            push(&mut buf, &mut buf_kind, TokenKind::Comment, c);
            push(&mut buf, &mut buf_kind, TokenKind::Comment, '*');
            i += 2;
            state.in_block_comment = true;
            continue;
        }

        // 字符串开始
        if c == '\'' || c == '"' || c == '`' {
            flush!();
            push(&mut buf, &mut buf_kind, TokenKind::Str, c);
            state.in_string = Some(c);
            i += 1;
            continue;
        }

        // 变量 :name / ?
        if c == ':' && chars.get(i + 1).is_some_and(|n| is_word_start(*n)) {
            flush!();
            let mut j = i;
            buf_kind = Some(TokenKind::Variable);
            while j < chars.len() && (j == i || is_word_char(chars[j])) {
                buf.push(chars[j]);
                j += 1;
            }
            flush!();
            i = j;
            continue;
        }

        // 数字
        if c.is_ascii_digit() {
            flush!();
            let mut j = i;
            buf_kind = Some(TokenKind::Number);
            while j < chars.len()
                && (chars[j].is_ascii_digit() || chars[j] == '.' || chars[j] == 'e' || chars[j] == 'E')
            {
                buf.push(chars[j]);
                j += 1;
            }
            flush!();
            i = j;
            continue;
        }

        // 单词
        if is_word_start(c) {
            let mut j = i;
            let mut word = String::new();
            while j < chars.len() && is_word_char(chars[j]) {
                word.push(chars[j]);
                j += 1;
            }
            // 判断函数（后接左括号）
            let mut k = j;
            while k < chars.len() && chars[k] == ' ' {
                k += 1;
            }
            let upper = word.to_uppercase();
            let kind = if KEYWORDS.contains(&upper.as_str()) {
                TokenKind::Keyword
            } else if TYPES.contains(&upper.as_str()) {
                TokenKind::Type
            } else if FUNCTIONS.contains(&upper.as_str()) && chars.get(k) == Some(&'(') {
                TokenKind::Function
            } else if c == '@' || c == '#' {
                TokenKind::Variable
            } else {
                TokenKind::Ident
            };
            flush!();
            buf_kind = Some(kind);
            buf.push_str(&word);
            flush!();
            i = j;
            continue;
        }

        // 运算符 / 标点
        let kind = if "()[]{},;.".contains(c) {
            TokenKind::Punct
        } else if "+-*/%=<>!|&^~".contains(c) {
            TokenKind::Operator
        } else if c.is_whitespace() {
            TokenKind::Space
        } else {
            TokenKind::Ident
        };
        // 类型与缓冲区当前内容不一致时，必须先把已有内容打包。
        // 否则字符串字面量（Str）会被后续标点覆盖成 Punct，整段着色错误。
        if !buf.is_empty() && buf_kind != Some(kind) {
            flush!();
        }
        push(&mut buf, &mut buf_kind, kind, c);
        // 连续同类合并仅在非空白时进行；空白单独成段
        if kind == TokenKind::Space {
            flush!();
        } else if let Some(next) = chars.get(i + 1) {
            let next_kind = if "()[]{},;.".contains(*next) {
                TokenKind::Punct
            } else if "+-*/%=<>!|&^~".contains(*next) {
                TokenKind::Operator
            } else {
                TokenKind::Ident
            };
            if next_kind != kind {
                flush!();
            }
        } else {
            flush!();
        }
        i += 1;
    }

    flush!();
    out
}

/// 便捷封装：整段 SQL 着色（按行）。
pub fn highlight_lines(sql: &str) -> Vec<Vec<(String, TokenKind)>> {
    let mut state = LexState::default();
    sql.split('\n').map(|line| highlight_line(line, &mut state)).collect()
}

/// 按 `;` 切分语句，忽略字符串与注释内的分号。返回 (起始行, 语句文本)。
pub fn split_statements(sql: &str) -> Vec<(usize, String)> {
    let chars: Vec<char> = sql.chars().collect();
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut i = 0usize;
    let mut line = 1usize;
    // 当前语句首个非空白字符所在的行（1 基）。
    // 必须在遇到该字符时记录——用「上一条语句结束行 + 1」推算会在语句以 `;`
    // 结束于最后一行时越界（模糊测试曾捕获）。
    let mut stmt_line: Option<usize> = None;
    let mut in_line_comment = false;
    let mut in_block = false;
    let mut in_str: Option<char> = None;
    let mut depth = 0i32;

    while i < chars.len() {
        let c = chars[i];
        if c == '\n' {
            line += 1;
            in_line_comment = false;
        }
        // 记录本条语句首个非空白字符所在行
        if stmt_line.is_none() && !c.is_whitespace() {
            stmt_line = Some(line);
        }

        if in_line_comment {
            buf.push(c);
            i += 1;
            continue;
        }
        if in_block {
            buf.push(c);
            if c == '*' && chars.get(i + 1) == Some(&'/') {
                buf.push('/');
                i += 2;
                in_block = false;
                continue;
            }
            i += 1;
            continue;
        }
        if let Some(q) = in_str {
            buf.push(c);
            if c == q {
                if chars.get(i + 1) == Some(&q) {
                    buf.push(q);
                    i += 2;
                    continue;
                }
                in_str = None;
            }
            i += 1;
            continue;
        }

        match c {
            '-' if chars.get(i + 1) == Some(&'-') => {
                in_line_comment = true;
                buf.push(c);
            }
            '/' if chars.get(i + 1) == Some(&'*') => {
                in_block = true;
                buf.push(c);
                buf.push('*');
                i += 2;
                continue;
            }
            '\'' | '"' | '`' => {
                in_str = Some(c);
                buf.push(c);
            }
            '(' => {
                depth += 1;
                buf.push(c);
            }
            ')' => {
                depth -= 1;
                buf.push(c);
            }
            ';' if depth <= 0 => {
                let stmt = buf.trim();
                if !stmt.is_empty() {
                    // 行号取本条语句首个非空白字符所在行（1 基）
                    let l = stmt_line.unwrap_or(line);
                    out.push((l, stmt.to_string()));
                }
                buf.clear();
                stmt_line = None;
            }
            _ => buf.push(c),
        }
        i += 1;
    }

    let tail = buf.trim();
    if !tail.is_empty() {
        let l = stmt_line.unwrap_or(line);
        out.push((l, tail.to_string()));
    }
    out
}

/// 去掉注释后的首个关键字序列。
pub fn leading_keywords(sql: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut in_block = false;
    let mut in_line = false;
    let mut in_str: Option<char> = None;
    let mut cur = String::new();
    let chars: Vec<char> = sql.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        if in_line {
            if c == '\n' {
                in_line = false;
            }
            i += 1;
            continue;
        }
        if in_block {
            if c == '*' && chars.get(i + 1) == Some(&'/') {
                in_block = false;
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }
        if let Some(q) = in_str {
            if c == q {
                in_str = None;
            }
            i += 1;
            continue;
        }
        match c {
            '-' if chars.get(i + 1) == Some(&'-') => {
                in_line = true;
                i += 2;
                continue;
            }
            '/' if chars.get(i + 1) == Some(&'*') => {
                in_block = true;
                i += 2;
                continue;
            }
            '\'' | '"' | '`' => {
                in_str = Some(c);
                i += 1;
                continue;
            }
            c if c.is_alphabetic() => {
                cur.push(c.to_ascii_uppercase());
                i += 1;
                continue;
            }
            _ => {}
        }
        if !cur.is_empty() {
            words.push(std::mem::take(&mut cur));
            if words.len() >= 6 {
                break;
            }
        }
        i += 1;
    }
    if !cur.is_empty() && words.len() < 6 {
        words.push(cur);
    }
    words
}

/// 危险语句信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DangerInfo {
    /// 操作名，如 DROP TABLE / DELETE
    pub action: String,
    /// 是否缺少 WHERE（DELETE / UPDATE）
    pub missing_where: bool,
    /// 目标对象（尽力解析）
    pub target: Option<String>,
}

impl DangerInfo {
    pub fn summary(&self) -> String {
        if self.missing_where {
            format!("{}（无 WHERE 条件，将影响全表）", self.action)
        } else {
            self.action.clone()
        }
    }
}

/// 判断是否为需要二次确认的高危语句。仅读语句返回 None。
pub fn danger_of(sql: &str) -> Option<DangerInfo> {
    let kws = leading_keywords(sql);
    let first = kws.first()?.as_str();
    let upper = sql.to_uppercase();
    let has_where = contains_word(&upper, "WHERE");

    let target = kws.get(2).cloned().or_else(|| kws.get(1).cloned());

    match first {
        "DROP" => Some(DangerInfo {
            action: format!(
                "DROP {}",
                kws.get(1).cloned().unwrap_or_else(|| "OBJECT".into())
            ),
            missing_where: false,
            target,
        }),
        "TRUNCATE" => Some(DangerInfo {
            action: "TRUNCATE TABLE".into(),
            missing_where: false,
            target: kws.get(2).cloned(),
        }),
        "DELETE" => Some(DangerInfo {
            action: "DELETE".into(),
            missing_where: !has_where,
            target,
        }),
        "UPDATE" => Some(DangerInfo {
            action: "UPDATE".into(),
            missing_where: !has_where,
            target,
        }),
        "ALTER" => Some(DangerInfo {
            action: format!(
                "ALTER {}",
                kws.get(1).cloned().unwrap_or_else(|| "OBJECT".into())
            ),
            missing_where: false,
            target,
        }),
        "CREATE" => {
            // CREATE 不具破坏性，但 DROP+CREATE 组合在其他语句中会各自触发
            None
        }
        "GRANT" | "REVOKE" => Some(DangerInfo {
            action: first.to_string(),
            missing_where: false,
            target,
        }),
        _ => None,
    }
}

fn contains_word(haystack: &str, word: &str) -> bool {
    let bytes = haystack.as_bytes();
    let w = word.as_bytes();
    let mut i = 0;
    while let Some(pos) = haystack[i..].find(word) {
        let start = i + pos;
        let end = start + w.len();
        let before_ok = start == 0 || !is_word_byte(bytes[start - 1]);
        let after_ok = end >= bytes.len() || !is_word_byte(bytes[end]);
        if before_ok && after_ok {
            return true;
        }
        i = end;
    }
    false
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// 是否为只读语句（用于只读连接拦截写操作）。
pub fn is_read_only(sql: &str) -> bool {
    let kw = leading_keywords(sql);
    match kw.first().map(String::as_str) {
        Some("SELECT") | Some("SHOW") | Some("DESCRIBE") | Some("DESC") | Some("EXPLAIN")
        | Some("PRAGMA") | Some("WITH") | Some("VALUES") => true,
        Some("EXEC") | Some("CALL") => false,
        _ => false,
    }
}

/// 轻量格式化：关键字大写、主要子句换行、括号内缩进。
pub fn format_sql(sql: &str) -> String {
    let lines = highlight_lines(sql);
    let mut flat = String::new();
    let mut state = LexState::default();
    for line in sql.split('\n') {
        let toks = highlight_line(line, &mut state);
        for (text, kind) in toks {
            match kind {
                TokenKind::Comment => {
                    if text.starts_with("--") {
                        flat.push('\n');
                    }
                    flat.push_str(&text);
                    flat.push(' ');
                }
                TokenKind::Space => flat.push(' '),
                TokenKind::Keyword | TokenKind::Type => {
                    flat.push_str(&text.to_uppercase());
                    flat.push(' ');
                }
                _ => {
                    flat.push_str(&text);
                    flat.push(' ');
                }
            }
        }
        flat.push(' ');
    }
    let _ = lines;

    let mut out = String::new();
    let mut indent = 0usize;
    let mut i = 0;
    let chars: Vec<char> = flat.chars().collect();
    let mut line = String::new();
    let mut pending_space = false;

    let newline_clauses = [
        "SELECT", "FROM", "WHERE", "GROUP BY", "ORDER BY", "HAVING", "LIMIT", "OFFSET", "VALUES",
        "SET", "RETURNING", "UNION", "UNION ALL", "EXCEPT", "INTERSECT",
    ];

    macro_rules! flush_line {
        () => {
            let t = line.trim_end().to_string();
            if !t.is_empty() {
                out.push_str(&"    ".repeat(indent));
                out.push_str(&t);
                out.push('\n');
            }
            line.clear();
        };
    }

    while i < chars.len() {
        let c = chars[i];
        if c == '\n' {
            flush_line!();
            i += 1;
            continue;
        }
        if c.is_whitespace() {
            pending_space = true;
            i += 1;
            continue;
        }
        if pending_space && !line.is_empty() && !",)".contains(c) {
            line.push(' ');
        }
        pending_space = false;

        if c == '(' {
            let mut look = i + 1;
            while look < chars.len() && chars[look].is_whitespace() {
                look += 1;
            }
            let next_word: String = chars[look..]
                .iter()
                .take_while(|ch| ch.is_alphabetic())
                .collect::<String>()
                .to_uppercase();
            if next_word == "SELECT" {
                indent += 1;
                line.push('(');
                flush_line!();
                i = look;
                continue;
            }
            line.push('(');
            i += 1;
            continue;
        }
        if c == ')' {
            if indent > 0 {
                let trimmed = line.trim_end();
                if !trimmed.is_empty() {
                    flush_line!();
                }
                indent -= 1;
                line.push(')');
                i += 1;
                continue;
            }
            line.push(')');
            i += 1;
            continue;
        }

        // 尝试匹配换行子句
        let rest: String = chars[i..].iter().take(20).collect::<String>().to_uppercase();
        let mut matched = None;
        for clause in newline_clauses {
            if rest.starts_with(clause) {
                let after = rest.chars().nth(clause.len());
                let boundary_ok = after.is_none_or(|c| !c.is_alphanumeric() && c != '_');
                if boundary_ok && !line.trim().is_empty() {
                    matched = Some(clause);
                    break;
                }
            }
        }
        if let Some(clause) = matched {
            flush_line!();
            line.push_str(clause);
            i += clause.len();
            // 跳过原文本中的对应字符数（大小写差异不影响字符数）
            continue;
        }

        line.push(c);
        i += 1;
    }
    flush_line!();
    out.trim_end().to_string()
}

/// 语句片段的字节区间（用于编辑器高亮当前语句）。
pub fn statement_ranges(sql: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut in_str: Option<char> = None;
    let mut in_line = false;
    let mut in_block = false;
    let bytes: Vec<char> = sql.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if in_line {
            if c == '\n' {
                in_line = false;
            }
            i += 1;
            continue;
        }
        if in_block {
            if c == '*' && bytes.get(i + 1) == Some(&'/') {
                in_block = false;
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }
        if let Some(q) = in_str {
            if c == q {
                in_str = None;
            }
            i += 1;
            continue;
        }
        match c {
            '-' if bytes.get(i + 1) == Some(&'-') => in_line = true,
            '/' if bytes.get(i + 1) == Some(&'*') => in_block = true,
            '\'' | '"' | '`' => in_str = Some(c),
            ';' => {
                let end = sql.char_indices().nth(i + 1).map(|(b, _)| b).unwrap_or(sql.len());
                out.push(start..end);
                start = end;
            }
            _ => {}
        }
        i += 1;
    }
    if start < sql.len() {
        out.push(start..sql.len());
    }
    out
}

/// 对象设计器：对比新旧表结构，生成方言对应的 ALTER 语句集。
///
/// 覆盖三类变更：加列 / 删列 / 改列（类型、可空、默认值）。
/// SQLite 仅支持 ADD/DROP COLUMN，修改列会跳过并在设计器中提示重建表。
pub fn alter_table_plan(kind: dbhelper_models::DbKind, old: &dbhelper_models::TableDef, new: &dbhelper_models::TableDef) -> Vec<String> {
    use dbhelper_models::{ColumnDef, DbKind};
    let table = &new.name;
    // 复用 rel_common::quote_ident 的转义规则，避免两套实现漂移。
    // （rel_common 依赖 models，与 utils 同层；此处按方言内联等价实现，
    //   规则必须与 rel_common 保持一致：反引号/方括号/双引号各自转义。）
    let quote = |ident: &str| -> String {
        match kind {
            // 反引号需转义为双反引号，否则列名里的 ` 可闭合引号造成标识符注入
            DbKind::Mysql | DbKind::Sqlite => format!("`{}`", ident.replace('`', "``")),
            DbKind::SqlServer => format!("[{}]", ident.replace(']', "]]")),
            // 标准 SQL 双引号转义
            _ => format!("\"{}\"", ident.replace('"', "\"\"")),
        }
    };
    let col_type = |c: &ColumnDef| -> String { c.data_type.clone() };
    let mut stmts = Vec::new();

    for col in &new.columns {
        match old.columns.iter().find(|c| c.name == col.name) {
            None => {
                let mut d = format!("ALTER TABLE {} ADD COLUMN {} {}", quote(table), quote(&col.name), col_type(col));
                if !col.nullable {
                    d.push_str(" NOT NULL");
                }
                if let Some(def) = &col.default {
                    d.push_str(&format!(" DEFAULT {def}"));
                }
                stmts.push(d);
            }
            Some(old_col) => {
                let type_changed = col_type(old_col).to_lowercase() != col_type(col).to_lowercase();
                let null_changed = old_col.nullable != col.nullable;
                let def_changed = old_col.default != col.default;
                if !(type_changed || null_changed || def_changed) {
                    continue;
                }
                match kind {
                    DbKind::Sqlite => {
                        // SQLite 无 MODIFY COLUMN，跳过（设计器会提示重建表）
                    }
                    DbKind::Postgres => {
                        if type_changed {
                            stmts.push(format!(
                                "ALTER TABLE {} ALTER COLUMN {} TYPE {}",
                                quote(table),
                                quote(&col.name),
                                col_type(col)
                            ));
                        }
                        if null_changed {
                            stmts.push(format!(
                                "ALTER TABLE {} ALTER COLUMN {} {} NOT NULL",
                                quote(table),
                                quote(&col.name),
                                if col.nullable { "DROP" } else { "SET" }
                            ));
                        }
                        if def_changed {
                            match &col.default {
                                Some(def) => stmts.push(format!(
                                    "ALTER TABLE {} ALTER COLUMN {} SET DEFAULT {def}",
                                    quote(table),
                                    quote(&col.name)
                                )),
                                None => stmts.push(format!(
                                    "ALTER TABLE {} ALTER COLUMN {} DROP DEFAULT",
                                    quote(table),
                                    quote(&col.name)
                                )),
                            }
                        }
                    }
                    _ => {
                        // MySQL / SQL Server / Oracle
                        let mut d = format!(
                            "ALTER TABLE {} {} COLUMN {} {}",
                            quote(table),
                            if kind == DbKind::SqlServer { "ALTER" } else { "MODIFY" },
                            quote(&col.name),
                            col_type(col)
                        );
                        if !col.nullable {
                            d.push_str(" NOT NULL");
                        }
                        if let Some(def) = &col.default {
                            d.push_str(&format!(" DEFAULT {def}"));
                        }
                        stmts.push(d);
                    }
                }
            }
        }
    }

    for col in &old.columns {
        if !new.columns.iter().any(|c| c.name == col.name) {
            stmts.push(format!("ALTER TABLE {} DROP COLUMN {}", quote(table), quote(&col.name)));
        }
    }

    stmts
}

/// 表名标识符转义（用于把表名安全地拼进 SQL）。
///
/// 统一用标准 SQL 双引号并转义内部双引号；MySQL/SQLite 同样接受双引号
/// （在 ANSI 模式下），必要时由各驱动自行改写。
pub fn quote_table_name(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// SQL 诊断结果（行号为 0 基）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub line: usize,
    pub message: String,
}

/// 轻量 SQL 诊断：括号 / 引号平衡 + 高危写语句提示。
///
/// 纯函数、无副作用，便于白盒测试。
/// 注意：实现中**不得**按字节切片 SQL——SQL 可能含多字节字符，
/// 按字节切到非 UTF-8 边界会 panic（release 是 panic=abort，会终止整个 GUI）。
pub fn diagnose(sql: &str) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    let mut depth = 0i32;
    let mut in_str = false;
    let mut line = 0usize;
    for ch in sql.chars() {
        if ch == '\n' {
            line += 1;
            continue;
        }
        if in_str {
            if ch == '\'' {
                in_str = false;
            }
            continue;
        }
        match ch {
            '\'' => in_str = true,
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth < 0 {
                    diags.push(Diagnostic {
                        line,
                        message: "括号不匹配：多余的 ')'".into(),
                    });
                    depth = 0;
                }
            }
            _ => {}
        }
    }
    if in_str {
        diags.push(Diagnostic { line, message: "字符串未闭合".into() });
    }
    if depth > 0 {
        diags.push(Diagnostic {
            line,
            message: format!("括号未闭合：缺少 {depth} 个 ')'"),
        });
    }
    // split_statements 的首项是 1 起的行号（不是字节偏移！）
    for (stmt_line, stmt) in split_statements(sql) {
        if stmt.trim().is_empty() {
            continue;
        }
        if let Some(danger) = danger_of(stmt.trim()) {
            if danger.missing_where {
                diags.push(Diagnostic {
                    line: stmt_line.saturating_sub(1),
                    message: "高危：写语句缺少 WHERE 子句".into(),
                });
            }
        }
    }
    diags
}

/// 敏感列名判定（手机号 / 邮箱 / 身份证 / 密码 / 令牌等）。
pub fn is_sensitive_column(col: &str) -> bool {
    let key = col.to_lowercase();
    key.contains("phone")
        || key.contains("mobile")
        || key.contains("手机")
        || key.contains("email")
        || key.contains("mail")
        || key.contains("id_card")
        || key.contains("idcard")
        || key.contains("身份证")
        || key.contains("password")
        || key.contains("passwd")
        || key.contains("pwd")
        || key.contains("secret")
        || key.contains("token")
}

/// 数据脱敏：敏感列保留首尾各 2 字符，中间用 `*` 替代；非敏感列原样返回。
pub fn mask_value(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    match n {
        0 => String::new(),
        // 1~2 位：全打码
        1..=2 => "*".repeat(n),
        // 3~4 位：只保留首位，其余打码（否则首尾会重叠泄露）
        3..=4 => {
            let head: String = chars.iter().take(1).collect();
            format!("{head}{}", "*".repeat(n - 1))
        }
        // ≥5 位：保留首尾各 2 位
        _ => {
            let head: String = chars.iter().take(2).collect();
            let tail: String = chars.iter().skip(n - 2).collect();
            format!("{head}{}{tail}", "*".repeat(n - 4))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_statements() {
        let sql = "SELECT 1; SELECT ';' AS x;\n-- c;\nSELECT 3";
        let stmts = split_statements(sql);
        assert_eq!(stmts.len(), 3);
        assert!(stmts[1].1.contains("';'"));
    }

    #[test]
    fn detects_danger() {
        assert!(danger_of("DELETE FROM users").unwrap().missing_where);
        assert!(!danger_of("DELETE FROM users WHERE id = 1").unwrap().missing_where);
        assert!(danger_of("DROP TABLE users").is_some());
        assert!(danger_of("SELECT * FROM users").is_none());
        assert!(danger_of("UPDATE t SET a = 1 WHERE id = 2").is_some());
    }

    #[test]
    fn read_only_detection() {
        assert!(is_read_only("  -- x\n SELECT 1"));
        assert!(!is_read_only("DELETE FROM t"));
    }

    /// 回归：关键字与标识符必须分别着色（曾因 push 的空 flush 分支混成一段）。
    #[test]
    fn highlight_separates_keyword_and_identifier() {
        let mut st = LexState::default();
        let toks = highlight_line("SELECT name FROM t", &mut st);
        let kinds: Vec<TokenKind> = toks.iter().map(|(_, k)| *k).collect();
        let texts: Vec<&str> = toks.iter().map(|(t, _)| t.as_str()).collect();
        // SELECT 应单独成 Keyword 段
        let sel = texts.iter().position(|t| t.trim() == "SELECT").expect("应能找到 SELECT 段");
        assert_eq!(kinds[sel], TokenKind::Keyword, "SELECT 应为关键字：{texts:?}");
        // name 应是 Ident 段，不能被并进 SELECT
        let nm = texts.iter().position(|t| t.trim() == "name").expect("应能找到 name 段");
        assert_eq!(kinds[nm], TokenKind::Ident, "name 应为标识符：{texts:?}");
        assert_ne!(sel, nm, "关键字与标识符不能在同一段");
    }

    /// 数字、字符串、变量、注释的类型应各自独立。
    #[test]
    fn highlight_classifies_tokens() {
        let mut st = LexState::default();
        let toks = highlight_line("SELECT 42, 'txt', :v -- c", &mut st);
        let map: std::collections::HashMap<&str, TokenKind> =
            toks.iter().map(|(t, k)| (t.trim(), *k)).collect();
        assert_eq!(map.get("42"), Some(&TokenKind::Number), "{toks:?}");
        assert_eq!(map.get("'txt'"), Some(&TokenKind::Str), "{toks:?}");
        assert_eq!(map.get(":v"), Some(&TokenKind::Variable), "{toks:?}");
    }

    #[test]
    fn highlight_marks_keywords() {
        let mut st = LexState::default();
        let toks = highlight_line("SELECT a FROM t -- hi", &mut st);
        assert_eq!(toks[0].1, TokenKind::Keyword);
        assert_eq!(toks.last().unwrap().1, TokenKind::Comment);
    }

    // ---------------- alter_table_plan ----------------

    fn col(name: &str, ty: &str, nullable: bool, default: Option<&str>) -> dbhelper_models::ColumnDef {
        dbhelper_models::ColumnDef {
            name: name.into(),
            data_type: ty.into(),
            nullable,
            default: default.map(|s| s.into()),
            is_pk: false,
            is_unique: false,
            fk: None,
            extra: String::new(),
            comment: None,
            ordinal: 0,
        }
    }

    fn table(name: &str, cols: Vec<dbhelper_models::ColumnDef>) -> dbhelper_models::TableDef {
        dbhelper_models::TableDef {
            schema: None,
            name: name.into(),
            columns: cols,
            indexes: Vec::new(),
            row_estimate: None,
            comment: None,
        }
    }

    #[test]
    fn alter_add_and_drop_column() {
        let old = table("t", vec![col("id", "INT", false, None)]);
        let new = table("t", vec![col("id", "INT", false, None), col("name", "VARCHAR(64)", true, None)]);
        let stmts = alter_table_plan(dbhelper_models::DbKind::Mysql, &old, &new);
        assert_eq!(stmts.len(), 1, "只应有一条 ADD COLUMN：{stmts:?}");
        assert!(stmts[0].contains("ADD COLUMN"));
        assert!(stmts[0].contains("name"));

        // 删列
        let new2 = table("t", vec![col("id", "INT", false, None)]);
        let stmts2 = alter_table_plan(dbhelper_models::DbKind::Mysql, &old, &new2);
        assert_eq!(stmts2.len(), 0, "结构未变不应产生语句：{stmts2:?}");
    }

    #[test]
    fn alter_drop_column_emits_drop() {
        let old = table("t", vec![col("id", "INT", false, None), col("old", "INT", true, None)]);
        let new = table("t", vec![col("id", "INT", false, None)]);
        let stmts = alter_table_plan(dbhelper_models::DbKind::Postgres, &old, &new);
        assert_eq!(stmts.len(), 1);
        assert!(stmts[0].contains("DROP COLUMN"), "{:?}", stmts);
    }

    #[test]
    fn alter_type_change_uses_dialect_syntax() {
        let old = table("t", vec![col("id", "INT", false, None)]);
        let new = table("t", vec![col("id", "BIGINT", false, None)]);

        // MySQL：MODIFY COLUMN
        let my = alter_table_plan(dbhelper_models::DbKind::Mysql, &old, &new);
        assert!(my[0].contains("MODIFY"), "{:?}", my);

        // Postgres：ALTER COLUMN ... TYPE
        let pg = alter_table_plan(dbhelper_models::DbKind::Postgres, &old, &new);
        assert!(pg[0].contains("ALTER COLUMN"), "{:?}", pg);
        assert!(pg[0].contains("TYPE"), "{:?}", pg);

        // SQLite：无法改列类型 → 不产生语句
        let lite = alter_table_plan(dbhelper_models::DbKind::Sqlite, &old, &new);
        assert!(lite.is_empty(), "SQLite 不支持改列类型，应跳过：{lite:?}");
    }

    #[test]
    fn alter_nullability_and_default() {
        let old = table("t", vec![col("a", "INT", true, None)]);
        let new = table("t", vec![col("a", "INT", false, Some("0"))]);
        let stmts = alter_table_plan(dbhelper_models::DbKind::Postgres, &old, &new);
        assert!(stmts.iter().any(|s| s.contains("SET NOT NULL")), "{:?}", stmts);
        assert!(stmts.iter().any(|s| s.contains("SET DEFAULT")), "{:?}", stmts);
    }

    #[test]
    fn alter_identifiers_are_quoted() {
        let old = table("t", vec![col("id", "INT", false, None)]);
        let new = table("t", vec![col("id", "INT", false, None), col("we`ird", "INT", true, None)]);
        let stmts = alter_table_plan(dbhelper_models::DbKind::Mysql, &old, &new);
        // 反引号必须转义为双反引号，防止标识符注入
        assert!(stmts[0].contains("we``ird"), "{:?}", stmts);
    }

    #[test]
    fn alter_sqlserver_uses_bracket_quoting() {
        let old = table("t", vec![col("id", "INT", false, None)]);
        let new = table("t", vec![col("id", "INT", false, None), col("x", "INT", true, None)]);
        let stmts = alter_table_plan(dbhelper_models::DbKind::SqlServer, &old, &new);
        assert!(stmts[0].contains('['), "{:?}", stmts);
    }

    // ---------------- split_statements 行号语义 ----------------

    #[test]
    fn split_statements_returns_line_numbers_not_byte_offsets() {
        // 首项是 1 起的**语句起始行**；多字节字符时也不能是字节偏移
        let sql = "中文注释\nSELECT 1;\nUPDATE t SET a=1;";
        let stmts = split_statements(sql);
        let lines: Vec<usize> = stmts.iter().map(|(l, _)| *l).collect();
        // 首条语句从第 1 行起（含前导文本），UPDATE 在第 3 行
        assert_eq!(lines, vec![1, 3], "{lines:?}");
        for (l, _) in &stmts {
            assert!(*l <= sql.lines().count(), "行号 {l} 超出实际行数");
            assert!(*l >= 1, "行号应为 1 基");
        }
    }

    #[test]
    fn split_statements_line_numbers_point_to_statement_start() {
        let sql = "SELECT 1;\nDELETE FROM t;\nSELECT 2;";
        let lines: Vec<usize> = split_statements(sql).iter().map(|(l, _)| *l).collect();
        assert_eq!(lines, vec![1, 2, 3], "{lines:?}");
    }

    // ---------------- diagnose ----------------

    #[test]
    fn diagnose_clean_sql_has_no_findings() {
        assert!(diagnose("SELECT id, name FROM users WHERE id = 1").is_empty());
    }

    #[test]
    fn diagnose_detects_unbalanced_parenthesis() {
        let d = diagnose("SELECT (a, b FROM t");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].message.contains("括号未闭合"), "{d:?}");
    }

    #[test]
    fn diagnose_detects_extra_closing_parenthesis() {
        let d = diagnose("SELECT a) FROM t");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].message.contains("多余的"), "{d:?}");
    }

    #[test]
    fn diagnose_detects_unclosed_string() {
        let d = diagnose("SELECT * FROM t WHERE name = 'abc");
        assert!(d.iter().any(|x| x.message.contains("字符串未闭合")), "{d:?}");
    }

    #[test]
    fn diagnose_flags_dangerous_write_without_where() {
        let d = diagnose("DELETE FROM users");
        assert!(d.iter().any(|x| x.message.contains("缺少 WHERE")), "{d:?}");
        assert!(!diagnose("DELETE FROM users WHERE id = 1")
            .iter()
            .any(|x| x.message.contains("缺少 WHERE")));
    }

    /// 回归测试：SQL 含多字节字符时不得 panic（曾按字节切片导致整窗崩溃）。
    #[test]
    fn diagnose_handles_multibyte_sql_without_panic() {
        let cases = [
            "SELECT 中文 FROM 表",
            "SELECT '中文' FROM t WHERE 名称 = '值'",
            "中文",
            "SELECT * FROM t WHERE 名 = '中' AND (",
            "DELETE FROM 中文表",
        ];
        for sql in cases {
            let d = diagnose(sql);
            let lines = sql.lines().count().max(1);
            for item in &d {
                assert!(item.line < lines, "行号 {} 越界（SQL 共 {lines} 行）：{sql:?}", item.line);
            }
        }
    }

    #[test]
    fn diagnose_line_numbers_are_zero_based_and_in_range() {
        let sql = "SELECT 1;\nDELETE FROM t;\nSELECT 2;";
        let d = diagnose(sql);
        assert!(d.iter().any(|x| x.line == 1), "第二行的 DELETE 应报到行号 1：{d:?}");
    }

    #[test]
    fn quote_table_name_escapes_quotes() {
        assert_eq!(quote_table_name("users"), "\"users\"");
        // 内部双引号必须成对转义，否则可闭合引号造成注入
        assert_eq!(quote_table_name("a\"b"), "\"a\"\"b\"");
        // 注入载荷：转义后引号总数 = 2（外层）+ 2×(内部引号数)，
        // 即所有内部引号都被成对转义，字符串字面量无法提前闭合
        let payload = "x\"; DROP TABLE t; --";
        let out = quote_table_name(payload);
        let inner_quotes = out.matches('"').count() - 2;
        assert_eq!(inner_quotes, 2, "内部引号应成对转义：{out}");
        // 反转义后应还原为原始标识符
        let restored = out[1..out.len() - 1].replace("\"\"", "\"");
        assert_eq!(restored, payload);
    }

    /// 模糊测试：随机（可能含多字节/未闭合引号/深层括号）输入下，
    /// diagnose 不得 panic，且行号必须始终落在实际行数范围内。
    #[test]
    fn diagnose_fuzz_never_panics() {
        // 确定性伪随机（无需额外依赖）
        let mut seed: u64 = 0x2545F4914F6CDD1D;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let alphabet: Vec<char> = "\'()[];,.-中文测试abc 123\n\t\"#$%".chars().collect();
        for _ in 0..2000 {
            let len = (next() % 120) as usize;
            let mut sql = String::new();
            for _ in 0..len {
                sql.push(alphabet[(next() as usize) % alphabet.len()]);
            }
            let diags = diagnose(&sql);
            // 诊断行号为 0 基，允许等于「最后一个有内容的行号」；
            // 以换行结尾的 SQL，其末行之后仍可能报告错误。
            let lines = sql.split('\n').count();
            for d in &diags {
                assert!(d.line < lines, "行号 {} 越界（{lines} 行）：{sql:?}", d.line);
            }
            // split_statements 的行号同样必须有界
            for (l, _) in split_statements(&sql) {
                assert!(l >= 1 && l <= lines, "语句行号 {l} 越界（{lines} 行）：{sql:?}");
            }
        }
    }

    // ---------------- 脱敏 ----------------

    #[test]
    fn sensitive_column_detection() {
        for c in ["phone", "mobile_phone", "user_email", "ID_CARD", "password", "api_token", "手机号", "身份证"] {
            assert!(is_sensitive_column(c), "{c} 应判定为敏感列");
        }
        for c in ["id", "name", "created_at", "amount"] {
            assert!(!is_sensitive_column(c), "{c} 不应判定为敏感列");
        }
    }

    #[test]
    fn mask_value_keeps_head_and_tail() {
        assert_eq!(mask_value("13800138000"), "13*******00");
        assert_eq!(mask_value("ab"), "**");
        assert_eq!(mask_value("a"), "*");
        assert_eq!(mask_value(""), "");
        // 短值不能因首尾重叠而泄露（3~4 位只保留首位）
        assert_eq!(mask_value("abc"), "a**");
        assert_eq!(mask_value("abcd"), "a***");
        // 打码后长度应与原值一致
        for n in 1..12 {
            let src: String = std::iter::repeat('x').take(n).collect();
            assert_eq!(mask_value(&src).chars().count(), n, "长度应为 {n}");
        }
    }

    #[test]
    fn mask_value_is_multibyte_safe() {
        // 多字节字符按字符数处理，长度应与原值一致且不产生非法 UTF-8
        let masked = mask_value("张三丰丰");
        assert_eq!(masked.chars().count(), 4);
        assert!(std::str::from_utf8(masked.as_bytes()).is_ok());
    }
}
