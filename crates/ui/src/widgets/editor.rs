//! 自绘 SQL 编辑器：行号、语法着色、选区、光标与点击定位。
//!
//! 键盘输入由应用层全局订阅接管（canvas 不参与焦点链），
//! 这样既保证 IME 可用，也让「编辑器聚焦」成为显式状态。

use crate::kit::Ctx;
use dbhelper_utils::sql::{highlight_line, LexState, TokenKind};
use iced::widget::canvas::{self, Canvas, Frame, Geometry, Path, Text};
use iced::{
    mouse, Alignment, Color, Element, Length, Point, Rectangle, Renderer, Theme,
};
use std::cell::Cell;

pub const GUTTER: f32 = 46.0;

pub struct Editor<'a, M> {
    pub text: &'a str,
    pub caret: usize,
    pub anchor: usize,
    pub ctx: Ctx,
    pub focused: bool,
    pub blink: bool,
    pub on_click: Box<dyn Fn(usize) -> M + 'a>,
    pub width: f32,
    pub min_height: f32,
}

impl<'a, M: Clone + 'a> Editor<'a, M> {
    pub fn new(
        text: &'a str,
        caret: usize,
        anchor: usize,
        ctx: Ctx,
        on_click: impl Fn(usize) -> M + 'a,
    ) -> Self {
        Self {
            text,
            caret,
            anchor,
            ctx,
            focused: false,
            blink: true,
            on_click: Box::new(on_click),
            width: 0.0,
            min_height: 220.0,
        }
    }

    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    pub fn blink(mut self, blink: bool) -> Self {
        self.blink = blink;
        self
    }

    pub fn min_height(mut self, h: f32) -> Self {
        self.min_height = h;
        self
    }

    pub fn view(self) -> Element<'a, M> {
        let lines = self.text.split('\n').count().max(1);
        let line_h = (self.ctx.f(12.0) * 1.5).max(16.0);
        let height = (lines as f32 * line_h + 24.0).max(self.min_height);
        let program = EditorProgram {
            text: self.text.to_string(),
            caret: self.caret,
            anchor: self.anchor,
            ctx: self.ctx,
            focused: self.focused,
            blink: self.blink,
            on_click: self.on_click,
            line_h,
            width: self.ctx.f(0.0) + 0.0,
        };
        Canvas::new(program)
            .width(Length::Fill)
            .height(Length::Fixed(height))
            .into()
    }
}

struct EditorProgram<'a, M> {
    text: String,
    caret: usize,
    anchor: usize,
    ctx: Ctx,
    focused: bool,
    blink: bool,
    on_click: Box<dyn Fn(usize) -> M + 'a>,
    line_h: f32,
    width: f32,
}

/// 编辑器绘制缓存：文本层（背景/行号/着色/选区）内容不变时直接复用，
/// 闪烁光标单独成层 —— 空闲重绘与光标闪烁都不再全量重着色。
pub struct EditorCache {
    key: Cell<u64>,
    layer: canvas::Cache,
}

impl Default for EditorCache {
    fn default() -> Self {
        Self {
            key: Cell::new(u64::MAX),
            layer: canvas::Cache::new(),
        }
    }
}

/// 文本层失效键：内容/选区/字体/主题/行高任一变化即重绘。
fn layer_key(text: &str, anchor: usize, caret: usize, focused: bool, ctx: &Ctx, line_h: f32) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    text.hash(&mut h);
    anchor.hash(&mut h);
    caret.hash(&mut h);
    focused.hash(&mut h);
    ctx.t.dark.hash(&mut h);
    ctx.scale.to_bits().hash(&mut h);
    line_h.to_bits().hash(&mut h);
    format!("{:?}", ctx.mono).hash(&mut h);
    h.finish()
}

fn token_color(kind: TokenKind, ctx: Ctx) -> Color {
    let t = ctx.t;
    match kind {
        TokenKind::Keyword => t.accent,
        TokenKind::Type => crate::theme::rgb(0x8B5CF6),
        TokenKind::Function => t.accent,
        TokenKind::Str => t.warning,
        TokenKind::Number => t.accent_pressed,
        TokenKind::Comment => t.text_tertiary,
        TokenKind::Variable => t.error,
        TokenKind::Operator | TokenKind::Punct | TokenKind::Ident | TokenKind::Space => t.text,
    }
}

/// 字符索引 → (行, 列)
pub fn index_to_line_col(text: &str, index: usize) -> (usize, usize) {
    let mut line = 0usize;
    let mut col = 0usize;
    for (i, ch) in text.chars().enumerate() {
        if i >= index {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// (行, 列) → 字符索引
pub fn line_col_to_index(text: &str, line: usize, col: usize) -> usize {
    let mut cur_line = 0usize;
    let mut cur_col = 0usize;
    for (i, ch) in text.chars().enumerate() {
        if cur_line == line && cur_col >= col {
            return i;
        }
        if ch == '\n' {
            if cur_line == line {
                return i;
            }
            cur_line += 1;
            cur_col = 0;
        } else {
            cur_col += 1;
        }
    }
    text.chars().count()
}

impl<'a, M: Clone + 'a> canvas::Program<M> for EditorProgram<'a, M> {
    type State = EditorCache;

    fn update(
        &self,
        _state: &mut EditorCache,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<M>> {
        if let canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) = event {
            if let Some(pos) = cursor.position_in(bounds) {
                let advance = (self.ctx.f(13.0) * 0.6).max(6.2);
                let line = ((pos.y - 12.0) / self.line_h).floor().max(0.0) as usize;
                let col = (((pos.x - GUTTER).max(0.0)) / advance).round() as usize;
                let line = line.min(self.text.split('\n').count().saturating_sub(1));
                let index = line_col_to_index(&self.text, line, col);
                return Some(canvas::Action::publish((self.on_click)(index)));
            }
        }
        None
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let ctx = self.ctx;
        let t = ctx.t;
        let size = ctx.f(13.0);
        let advance = (size * 0.6).max(6.2);
        let top = 12.0;

        let mut lex = LexState::default();
        let caret_pos = index_to_line_col(&self.text, self.caret);
        let anchor_pos = index_to_line_col(&self.text, self.anchor);
        let (sel_start, sel_end) = if self.anchor <= self.caret {
            (anchor_pos, caret_pos)
        } else {
            (caret_pos, anchor_pos)
        };

        // 文本层缓存：内容/选区/主题不变时复用，零重着色
        let key = layer_key(&self.text, self.anchor, self.caret, self.focused, &ctx, self.line_h);
        if state.key.get() != key {
            state.key.set(key);
            state.layer.clear();
        }

        let layer = state.layer.draw(renderer, bounds.size(), |frame| {
            // 背景与行号槽
            frame.fill_rectangle(Point::ORIGIN, bounds.size(), t.bg);
            frame.fill_rectangle(Point::ORIGIN, iced::Size::new(GUTTER - 10.0, bounds.height), t.soft);

            for (li, line) in self.text.split('\n').enumerate() {
                let y = top + li as f32 * self.line_h;

                // 选区
                if (sel_start, sel_end) != (caret_pos, caret_pos) && li >= sel_start.0 && li <= sel_end.0 {
                    let from = if li == sel_start.0 { sel_start.1 } else { 0 };
                    let to = if li == sel_end.0 {
                        sel_end.1
                    } else {
                        line.chars().count() + 1
                    };
                    if to > from {
                        let x0 = GUTTER + from as f32 * advance;
                        let x1 = GUTTER + to as f32 * advance;
                        frame.fill_rectangle(
                            Point::new(x0, y - 2.0),
                            iced::Size::new(x1 - x0, self.line_h),
                            crate::theme::rgba(0x2383E2, 0.16),
                        );
                    }
                }

                // 行号
                frame.fill_text(Text {
                    content: (li + 1).to_string(),
                    position: Point::new(GUTTER - 14.0, y),
                    color: t.text_tertiary,
                    size: iced::Pixels(ctx.f(12.0)),
                    font: ctx.mono,
                    align_x: Alignment::End.into(),
                    align_y: Alignment::Start.into(),
                    ..Default::default()
                });

                // 语法着色
                let tokens = highlight_line(line, &mut lex);
                let mut x = GUTTER;
                for (chunk, kind) in tokens {
                    if chunk.trim().is_empty() {
                        x += advance * chunk.chars().count() as f32;
                        continue;
                    }
                    frame.fill_text(Text {
                        content: chunk.clone(),
                        position: Point::new(x, y),
                        color: token_color(kind, ctx),
                        size: iced::Pixels(size),
                        font: ctx.mono,
                        align_x: Alignment::Start.into(),
                        align_y: Alignment::Start.into(),
                        ..Default::default()
                    });
                    x += advance * chunk.chars().count() as f32;
                }
            }
        });

        // 光标单独一层：闪烁只重绘这一个小矩形
        if self.focused && self.blink {
            let y = top + caret_pos.0 as f32 * self.line_h;
            let x = GUTTER + caret_pos.1 as f32 * advance;
            let mut caret = Frame::new(renderer, bounds.size());
            caret.fill_rectangle(
                Point::new(x, y - 1.0),
                iced::Size::new(1.6, self.line_h - 2.0),
                t.text,
            );
            vec![layer, caret.into_geometry()]
        } else {
            vec![layer]
        }
    }
}

// ---------------------------------------------------------------------------
// 文本编辑操作（纯函数，便于单测）
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum EditOp {
    Insert(String),
    Backspace,
    Delete,
    MoveLeft(bool),
    MoveRight(bool),
    MoveUp(bool),
    MoveDown(bool),
    Home(bool),
    End(bool),
    SelectAll,
    Newline,
    Tab,
    DeleteSelection,
}

pub fn apply_edit(text: &mut String, caret: &mut usize, anchor: &mut usize, op: EditOp) {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let (sel_lo, sel_hi) = if *anchor <= *caret { (*anchor, *caret) } else { (*caret, *anchor) };

    let splice = |text: &mut String, from: usize, to: usize, insert: &str| -> usize {
        let chars: Vec<char> = text.chars().collect();
        let mut s: String = chars[..from].iter().collect();
        s.push_str(insert);
        s.extend(chars[to..].iter());
        *text = s;
        from + insert.chars().count()
    };

    match op {
        EditOp::Insert(s) => {
            let from = if sel_hi > sel_lo { sel_lo } else { *caret };
            let to = if sel_hi > sel_lo { sel_hi } else { *caret };
            *caret = splice(text, from, to, &s);
            *anchor = *caret;
        }
        EditOp::Newline => {
            let indent = current_indent(&chars, sel_lo);
            let from = if sel_hi > sel_lo { sel_lo } else { *caret };
            let to = if sel_hi > sel_lo { sel_hi } else { *caret };
            *caret = splice(text, from, to, &format!("\n{indent}"));
            *anchor = *caret;
        }
        EditOp::Tab => {
            let from = if sel_hi > sel_lo { sel_lo } else { *caret };
            let to = if sel_hi > sel_lo { sel_hi } else { *caret };
            *caret = splice(text, from, to, "  ");
            *anchor = *caret;
        }
        EditOp::Backspace => {
            if sel_hi > sel_lo {
                *caret = splice(text, sel_lo, sel_hi, "");
            } else if *caret > 0 {
                *caret = splice(text, *caret - 1, *caret, "");
            }
            *anchor = *caret;
        }
        EditOp::Delete => {
            if sel_hi > sel_lo {
                *caret = splice(text, sel_lo, sel_hi, "");
            } else if *caret < len {
                *caret = splice(text, *caret, *caret + 1, "");
            }
            *anchor = *caret;
        }
        EditOp::DeleteSelection => {
            if sel_hi > sel_lo {
                *caret = splice(text, sel_lo, sel_hi, "");
            }
            *anchor = *caret;
        }
        EditOp::SelectAll => {
            *anchor = 0;
            *caret = len;
        }
        EditOp::MoveLeft(extend) => {
            let target = if sel_hi > sel_lo { sel_lo } else { caret.saturating_sub(1) };
            *caret = target;
            if !extend {
                *anchor = target;
            }
        }
        EditOp::MoveRight(extend) => {
            let target = if sel_hi > sel_lo { sel_hi } else { (*caret + 1).min(len) };
            *caret = target;
            if !extend {
                *anchor = target;
            }
        }
        EditOp::MoveUp(extend) => {
            let (line, col) = index_to_line_col(text, *caret);
            if line > 0 {
                let target = line_col_to_index(text, line - 1, col);
                *caret = target;
                if !extend {
                    *anchor = target;
                }
            }
        }
        EditOp::MoveDown(extend) => {
            let (line, col) = index_to_line_col(text, *caret);
            let total = text.split('\n').count();
            if line + 1 < total {
                let target = line_col_to_index(text, line + 1, col);
                *caret = target;
                if !extend {
                    *anchor = target;
                }
            }
        }
        EditOp::Home(extend) => {
            let (line, _) = index_to_line_col(text, *caret);
            let target = line_col_to_index(text, line, 0);
            *caret = target;
            if !extend {
                *anchor = target;
            }
        }
        EditOp::End(extend) => {
            let (line, _) = index_to_line_col(text, *caret);
            let line_len = text.split('\n').nth(line).map(|l| l.chars().count()).unwrap_or(0);
            let target = line_col_to_index(text, line, line_len);
            *caret = target;
            if !extend {
                *anchor = target;
            }
        }
    }
}

fn current_indent(chars: &[char], pos: usize) -> String {
    let mut start = pos;
    while start > 0 && chars.get(start - 1) != Some(&'\n') {
        start -= 1;
    }
    let mut indent = String::new();
    for ch in chars.iter().skip(start) {
        if *ch == ' ' || *ch == '\t' {
            indent.push(*ch);
        } else {
            break;
        }
    }
    indent
}

pub fn selected_text(text: &str, caret: usize, anchor: usize) -> Option<String> {
    let (lo, hi) = if anchor <= caret { (anchor, caret) } else { (caret, anchor) };
    if lo == hi {
        return None;
    }
    Some(text.chars().skip(lo).take(hi - lo).collect())
}

pub fn _unused(_: &Path, _: &Frame) {}
