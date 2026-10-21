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
        if *kind != Some(k) {
            if !buf.is_empty() {
                // 交给外层 flush 处理：此处直接把已有内容打包
            }
            *kind = Some(k);
        }
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
    let mut start_line = 1usize;
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
                    out.push((start_line, stmt.to_string()));
                }
                buf.clear();
                start_line = line;
            }
            _ => buf.push(c),
        }
        i += 1;
    }

    let tail = buf.trim();
    if !tail.is_empty() {
        out.push((start_line, tail.to_string()));
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

    #[test]
    fn highlight_marks_keywords() {
        let mut st = LexState::default();
        let toks = highlight_line("SELECT a FROM t -- hi", &mut st);
        assert_eq!(toks[0].1, TokenKind::Keyword);
        assert_eq!(toks.last().unwrap().1, TokenKind::Comment);
    }
}
