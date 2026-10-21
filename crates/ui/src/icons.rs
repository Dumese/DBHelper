//! 线性图标集：用 canvas 绘制，避免位图与字体依赖，任意尺寸清晰。

use iced::widget::canvas::{self, Canvas, Frame, Geometry, Path, Stroke};
use iced::{mouse, Color, Element, Length, Point, Rectangle, Renderer, Theme};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Database,
    Table,
    Eye,
    Folder,
    ChevronRight,
    ChevronDown,
    Plus,
    Search,
    More,
    Refresh,
    Sun,
    Moon,
    Shield,
    Minus,
    Square,
    Close,
    Play,
    Stop,
    Wand,
    Save,
    Download,
    SortAsc,
    SortDesc,
    Gear,
    List,
    Key,
    Doc,
    Chart,
    Lock,
    Check,
    Copy,
    Pencil,
    Trash,
    Filter,
    Terminal,
    ArrowRight,
    External,
    Clock,
    Server,
    User,
}

/// 产品 Logo（狐狸头）：使用设计资源位图，自带透明通道，任意尺寸清晰。
pub fn fox_logo<'a, M: 'a>(size: f32) -> Element<'a, M> {
    const FOX_PNG: &[u8] = include_bytes!("../assets/fox_logo.png");
    iced::widget::image(iced::widget::image::Handle::from_bytes(FOX_PNG.to_vec()))
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .into()
}

pub fn icon<'a, M: 'a>(icon: Icon, size: f32, color: Color) -> Element<'a, M> {
    Canvas::new(IconCanvas { icon, color, size })
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .into()
}

struct IconCanvas {
    icon: Icon,
    color: Color,
    size: f32,
}

impl<M> canvas::Program<M> for IconCanvas {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let s = bounds.width.min(bounds.height);
        let k = s / 16.0;
        let p = |x: f32, y: f32| Point::new(x * k, y * k);
        let stroke: Stroke = Stroke::default().with_color(self.color).with_width((1.35 * k).max(1.0));
        let thin: Stroke = Stroke::default().with_color(self.color).with_width((1.1 * k).max(0.9));
        let c = self.color;

        let line = |pts: &[(f32, f32)]| {
            Path::new(|b| {
                if let Some((x, y)) = pts.first() {
                    b.move_to(p(*x, *y));
                }
                for (x, y) in pts.iter().skip(1) {
                    b.line_to(p(*x, *y));
                }
            })
        };
        let closed = |pts: &[(f32, f32)]| {
            Path::new(|b| {
                if let Some((x, y)) = pts.first() {
                    b.move_to(p(*x, *y));
                }
                for (x, y) in pts.iter().skip(1) {
                    b.line_to(p(*x, *y));
                }
                b.close();
            })
        };

        match self.icon {
            Icon::Database => {
                let top = Path::new(|b| {
                    b.move_to(p(2.5, 4.2));
                    b.bezier_curve_to(p(2.5, 2.6), p(13.5, 2.6), p(13.5, 4.2));
                    b.bezier_curve_to(p(13.5, 5.8), p(2.5, 5.8), p(2.5, 4.2));
                });
                frame.stroke(&top, stroke.clone());
                frame.stroke(&line(&[(2.5, 4.2), (2.5, 11.8)]), stroke.clone());
                frame.stroke(&line(&[(13.5, 4.2), (13.5, 11.8)]), stroke.clone());
                let mid = Path::new(|b| {
                    b.move_to(p(2.5, 8.0));
                    b.bezier_curve_to(p(2.5, 9.6), p(13.5, 9.6), p(13.5, 8.0));
                });
                frame.stroke(&mid, thin.clone());
                let bottom = Path::new(|b| {
                    b.move_to(p(2.5, 11.8));
                    b.bezier_curve_to(p(2.5, 13.4), p(13.5, 13.4), p(13.5, 11.8));
                });
                frame.stroke(&bottom, stroke.clone());
            }
            Icon::Table => {
                frame.stroke(&closed(&[(2.6, 3.2), (13.4, 3.2), (13.4, 12.8), (2.6, 12.8)]), stroke.clone());
                frame.stroke(&line(&[(2.6, 6.4), (13.4, 6.4)]), thin.clone());
                frame.stroke(&line(&[(6.6, 6.4), (6.6, 12.8)]), thin.clone());
            }
            Icon::Eye => {
                let eye = Path::new(|b| {
                    b.move_to(p(1.8, 8.0));
                    b.bezier_curve_to(p(4.4, 4.2), p(11.6, 4.2), p(14.2, 8.0));
                    b.bezier_curve_to(p(11.6, 11.8), p(4.4, 11.8), p(1.8, 8.0));
                });
                frame.stroke(&eye, stroke.clone());
                frame.stroke(&Path::circle(p(8.0, 8.0), 1.9 * k), stroke.clone());
            }
            Icon::Folder => {
                frame.stroke(
                    &closed(&[(2.2, 4.4), (6.2, 4.4), (7.4, 6.0), (13.8, 6.0), (13.8, 12.4), (2.2, 12.4)]),
                    stroke.clone(),
                );
            }
            Icon::ChevronRight => {
                frame.stroke(&line(&[(6.2, 4.2), (10.0, 8.0), (6.2, 11.8)]), stroke.clone());
            }
            Icon::ChevronDown => {
                frame.stroke(&line(&[(4.2, 6.4), (8.0, 10.2), (11.8, 6.4)]), stroke.clone());
            }
            Icon::Plus => {
                frame.stroke(&line(&[(8.0, 3.4), (8.0, 12.6)]), stroke.clone());
                frame.stroke(&line(&[(3.4, 8.0), (12.6, 8.0)]), stroke.clone());
            }
            Icon::Search => {
                frame.stroke(&Path::circle(p(7.2, 7.2), 4.2 * k), stroke.clone());
                frame.stroke(&line(&[(10.3, 10.3), (13.4, 13.4)]), stroke.clone());
            }
            Icon::More => {
                frame.fill(&Path::circle(p(3.4, 8.0), 1.1 * k), c);
                frame.fill(&Path::circle(p(8.0, 8.0), 1.1 * k), c);
                frame.fill(&Path::circle(p(12.6, 8.0), 1.1 * k), c);
            }
            Icon::Refresh => {
                let arc = Path::new(|b| {
                    b.move_to(p(13.0, 8.0));
                    b.bezier_curve_to(p(13.0, 10.8), p(10.8, 13.0), p(8.0, 13.0));
                    b.bezier_curve_to(p(5.2, 13.0), p(3.0, 10.8), p(3.0, 8.0));
                    b.bezier_curve_to(p(3.0, 5.2), p(5.2, 3.0), p(8.0, 3.0));
                    b.bezier_curve_to(p(10.2, 3.0), p(12.0, 4.2), p(12.9, 6.0));
                });
                frame.stroke(&arc, stroke.clone());
                frame.stroke(&line(&[(10.4, 5.9), (13.0, 6.2), (13.3, 3.6)]), stroke.clone());
            }
            Icon::Sun => {
                frame.stroke(&Path::circle(p(8.0, 8.0), 3.0 * k), stroke.clone());
                for (dx, dy) in [(0.0, -5.6), (0.0, 5.6), (-5.6, 0.0), (5.6, 0.0), (-3.9, -3.9), (3.9, 3.9), (-3.9, 3.9), (3.9, -3.9)] {
                    let (x0, y0) = (8.0 + dx * 0.82, 8.0 + dy * 0.82);
                    let (x1, y1) = (8.0 + dx, 8.0 + dy);
                    frame.stroke(&line(&[(x0, y0), (x1, y1)]), thin.clone());
                }
            }
            Icon::Moon => {
                let m = Path::new(|b| {
                    b.move_to(p(9.6, 2.6));
                    b.bezier_curve_to(p(5.4, 3.2), p(3.4, 7.4), p(5.2, 10.8));
                    b.bezier_curve_to(p(7.0, 14.0), p(11.2, 14.0), p(13.0, 11.0));
                    b.bezier_curve_to(p(9.6, 11.0), p(6.8, 8.0), p(7.4, 5.0));
                    b.bezier_curve_to(p(7.6, 3.8), p(8.4, 3.0), p(9.6, 2.6));
                });
                frame.stroke(&m, stroke.clone());
            }
            Icon::Shield => {
                frame.stroke(
                    &closed(&[(8.0, 2.4), (13.0, 4.4), (13.0, 8.4), (8.0, 13.6), (3.0, 8.4), (3.0, 4.4)]),
                    stroke.clone(),
                );
                frame.stroke(&line(&[(5.9, 8.0), (7.4, 9.6), (10.2, 6.4)]), thin.clone());
            }
            Icon::Minus => {
                frame.stroke(&line(&[(3.4, 8.0), (12.6, 8.0)]), thin.clone());
            }
            Icon::Square => {
                frame.stroke(&closed(&[(3.6, 3.6), (12.4, 3.6), (12.4, 12.4), (3.6, 12.4)]), thin.clone());
            }
            Icon::Close => {
                frame.stroke(&line(&[(4.2, 4.2), (11.8, 11.8)]), thin.clone());
                frame.stroke(&line(&[(11.8, 4.2), (4.2, 11.8)]), thin.clone());
            }
            Icon::Play => {
                frame.fill(&closed(&[(5.4, 3.4), (12.4, 8.0), (5.4, 12.6)]), c);
            }
            Icon::Stop => {
                frame.fill(&closed(&[(5.0, 5.0), (11.0, 5.0), (11.0, 11.0), (5.0, 11.0)]), c);
            }
            Icon::Wand => {
                frame.stroke(&line(&[(3.2, 12.8), (10.4, 5.6)]), stroke.clone());
                frame.stroke(&line(&[(8.6, 3.8), (9.6, 2.4)]), thin.clone());
                frame.stroke(&line(&[(12.2, 7.4), (13.6, 6.6)]), thin.clone());
                frame.stroke(&line(&[(11.4, 11.4), (12.8, 12.8)]), thin.clone());
            }
            Icon::Save => {
                frame.stroke(
                    &closed(&[(3.2, 3.2), (10.6, 3.2), (12.8, 5.4), (12.8, 12.8), (3.2, 12.8)]),
                    stroke.clone(),
                );
                frame.stroke(&closed(&[(5.6, 3.2), (5.6, 6.6), (10.0, 6.6), (10.0, 3.2)]), thin.clone());
                frame.stroke(&closed(&[(5.6, 12.8), (5.6, 9.6), (10.4, 9.6), (10.4, 12.8)]), thin.clone());
            }
            Icon::Download => {
                frame.stroke(&line(&[(8.0, 3.0), (8.0, 10.2)]), stroke.clone());
                frame.stroke(&line(&[(4.8, 7.2), (8.0, 10.4), (11.2, 7.2)]), stroke.clone());
                frame.stroke(&line(&[(3.2, 13.0), (12.8, 13.0)]), stroke.clone());
            }
            Icon::SortAsc => {
                frame.stroke(&line(&[(8.0, 12.6), (8.0, 3.6)]), stroke.clone());
                frame.stroke(&line(&[(4.8, 6.8), (8.0, 3.4), (11.2, 6.8)]), stroke.clone());
            }
            Icon::SortDesc => {
                frame.stroke(&line(&[(8.0, 3.4), (8.0, 12.4)]), stroke.clone());
                frame.stroke(&line(&[(4.8, 9.2), (8.0, 12.6), (11.2, 9.2)]), stroke.clone());
            }
            Icon::Gear => {
                frame.stroke(&Path::circle(p(8.0, 8.0), 2.3 * k), stroke.clone());
                for i in 0..8 {
                    let a = i as f32 * std::f32::consts::PI / 4.0;
                    let (s, cs) = a.sin_cos();
                    frame.stroke(
                        &line(&[(8.0 + cs * 4.6, 8.0 + s * 4.6), (8.0 + cs * 6.2, 8.0 + s * 6.2)]),
                        thin.clone(),
                    );
                }
            }
            Icon::List => {
                for y in [4.6, 8.0, 11.4] {
                    frame.fill(&Path::circle(p(3.4, y), 0.9 * k), c);
                    frame.stroke(&line(&[(6.2, y), (13.0, y)]), thin.clone());
                }
            }
            Icon::Key => {
                frame.stroke(&Path::circle(p(5.6, 8.0), 2.6 * k), stroke.clone());
                frame.stroke(&line(&[(8.2, 8.0), (13.2, 8.0)]), stroke.clone());
                frame.stroke(&line(&[(11.2, 8.0), (11.2, 10.2)]), thin.clone());
            }
            Icon::Doc => {
                frame.stroke(
                    &closed(&[(4.0, 2.6), (9.4, 2.6), (12.0, 5.2), (12.0, 13.4), (4.0, 13.4)]),
                    stroke.clone(),
                );
                frame.stroke(&line(&[(9.2, 2.8), (9.2, 5.4), (11.8, 5.4)]), thin.clone());
                frame.stroke(&line(&[(5.8, 8.2), (10.2, 8.2)]), thin.clone());
                frame.stroke(&line(&[(5.8, 10.6), (9.0, 10.6)]), thin.clone());
            }
            Icon::Chart => {
                frame.stroke(&line(&[(3.0, 13.0), (13.0, 13.0)]), thin.clone());
                for (i, h) in [5.0f32, 7.4, 3.8, 9.2].iter().enumerate() {
                    let x = 4.2 + i as f32 * 2.4;
                    frame.stroke(&line(&[(x, 13.0), (x, 13.0 - h)]), stroke.clone());
                }
            }
            Icon::Lock => {
                frame.stroke(&closed(&[(4.2, 7.4), (11.8, 7.4), (11.8, 13.0), (4.2, 13.0)]), stroke.clone());
                let shackle = Path::new(|b| {
                    b.move_to(p(5.8, 7.4));
                    b.line_to(p(5.8, 5.4));
                    b.bezier_curve_to(p(5.8, 3.0), p(10.2, 3.0), p(10.2, 5.4));
                    b.line_to(p(10.2, 7.4));
                });
                frame.stroke(&shackle, stroke.clone());
            }
            Icon::Check => {
                frame.stroke(&line(&[(3.6, 8.4), (6.8, 11.6), (12.4, 4.6)]), stroke.clone());
            }
            Icon::Copy => {
                frame.stroke(&closed(&[(5.4, 5.4), (12.4, 5.4), (12.4, 12.4), (5.4, 12.4)]), stroke.clone());
                frame.stroke(&line(&[(3.6, 10.4), (3.6, 3.6), (10.4, 3.6)]), thin.clone());
            }
            Icon::Pencil => {
                frame.stroke(&line(&[(3.4, 12.6), (4.2, 9.6), (11.2, 2.6), (13.4, 4.8), (6.4, 11.8), (3.4, 12.6)]), stroke.clone());
            }
            Icon::Trash => {
                frame.stroke(&line(&[(3.4, 4.6), (12.6, 4.6)]), stroke.clone());
                frame.stroke(&closed(&[(5.4, 4.6), (5.4, 13.0), (10.6, 13.0), (10.6, 4.6)]), thin.clone());
                frame.stroke(&line(&[(6.6, 4.4), (6.6, 2.8), (9.4, 2.8), (9.4, 4.4)]), thin.clone());
            }
            Icon::Filter => {
                frame.stroke(&closed(&[(2.8, 3.6), (13.2, 3.6), (9.4, 8.2), (9.4, 12.8), (6.6, 11.4), (6.6, 8.2)]), stroke.clone());
            }
            Icon::Terminal => {
                frame.stroke(&closed(&[(2.6, 3.4), (13.4, 3.4), (13.4, 12.6), (2.6, 12.6)]), stroke.clone());
                frame.stroke(&line(&[(5.2, 6.6), (7.4, 8.4), (5.2, 10.2)]), thin.clone());
                frame.stroke(&line(&[(9.0, 10.2), (11.2, 10.2)]), thin.clone());
            }
            Icon::ArrowRight => {
                frame.stroke(&line(&[(3.6, 8.0), (12.0, 8.0)]), stroke.clone());
                frame.stroke(&line(&[(8.8, 4.8), (12.2, 8.0), (8.8, 11.2)]), stroke.clone());
            }
            Icon::External => {
                frame.stroke(&closed(&[(3.4, 6.4), (3.4, 12.6), (9.6, 12.6), (9.6, 10.6)]), thin.clone());
                frame.stroke(&line(&[(7.4, 8.6), (12.6, 3.4)]), stroke.clone());
                frame.stroke(&line(&[(9.0, 3.4), (12.6, 3.4), (12.6, 7.0)]), stroke.clone());
            }
            Icon::Clock => {
                frame.stroke(&Path::circle(p(8.0, 8.0), 5.2 * k), stroke.clone());
                frame.stroke(&line(&[(8.0, 4.8), (8.0, 8.2), (10.6, 9.6)]), thin.clone());
            }
            Icon::Server => {
                frame.stroke(&closed(&[(3.0, 3.4), (13.0, 3.4), (13.0, 7.0), (3.0, 7.0)]), stroke.clone());
                frame.stroke(&closed(&[(3.0, 9.0), (13.0, 9.0), (13.0, 12.6), (3.0, 12.6)]), stroke.clone());
                frame.fill(&Path::circle(p(5.2, 5.2), 0.7 * k), c);
                frame.fill(&Path::circle(p(5.2, 10.8), 0.7 * k), c);
            }
            Icon::User => {
                frame.stroke(&Path::circle(p(8.0, 5.6), 2.4 * k), stroke.clone());
                let body = Path::new(|b| {
                    b.move_to(p(3.6, 13.2));
                    b.bezier_curve_to(p(4.2, 10.2), p(11.8, 10.2), p(12.4, 13.2));
                });
                frame.stroke(&body, stroke.clone());
            }
        }

        vec![frame.into_geometry()]
    }
}
