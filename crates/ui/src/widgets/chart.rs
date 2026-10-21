//! 折线图（监控页）：面积填充 + 峰值虚线，无网格线，贴合原型。

use crate::kit::Ctx;
use iced::widget::canvas::{self, Canvas, Frame, Geometry, Path, Stroke};
use iced::{
    mouse, Color, Element, Length, Point, Rectangle, Renderer, Theme,
};

pub struct LineChart<M> {
    pub series: Vec<f32>,
    pub color: Color,
    pub height: f32,
    pub dashed: Option<f32>,
    pub ctx: Ctx,
    pub _msg: std::marker::PhantomData<M>,
}

impl<M> LineChart<M> {
    pub fn new(series: &[f32], color: Color, height: f32, dashed: Option<f32>, ctx: Ctx) -> Self {
        Self { series: series.to_vec(), color, height, dashed, ctx, _msg: std::marker::PhantomData }
    }

    pub fn view<'a>(self) -> Element<'a, M>
    where
        M: 'a,
    {
        let height = self.height;
        Canvas::new(self).width(Length::Fill).height(Length::Fixed(height)).into()
    }
}

impl<M> canvas::Program<M> for LineChart<M> {
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
        let w = bounds.width;
        let h = bounds.height;
        if self.series.len() < 2 {
            return vec![frame.into_geometry()];
        }
        let max = self
            .series
            .iter()
            .cloned()
            .fold(0.0f32, f32::max)
            .max(1.0);
        let pad_top = 10.0;
        let pad_bottom = 4.0;
        let usable = h - pad_top - pad_bottom;

        let point_at = |i: usize| {
            let x = w * i as f32 / (self.series.len() - 1) as f32;
            let y = pad_top + usable * (1.0 - self.series[i] / (max * 1.15));
            Point::new(x, y)
        };

        let line = Path::new(|b| {
            b.move_to(point_at(0));
            for i in 1..self.series.len() {
                b.line_to(point_at(i));
            }
        });
        frame.stroke(
            &line,
            Stroke::default().with_color(self.color).with_width(1.6),
        );

        // 面积填充
        let area = Path::new(|b| {
            b.move_to(Point::new(0.0, h));
            b.line_to(point_at(0));
            for i in 1..self.series.len() {
                b.line_to(point_at(i));
            }
            b.line_to(Point::new(w, h));
            b.close();
        });
        frame.fill(&area, Color { a: 0.10, ..self.color });

        // 峰值虚线
        if let Some(peak) = self.dashed {
            let y = pad_top + usable * (1.0 - peak / (max * 1.15));
            let mut x = 0.0;
            while x < w {
                let seg = Path::line(Point::new(x, y), Point::new((x + 4.0).min(w), y));
                frame.stroke(
                    &seg,
                    Stroke::default()
                        .with_color(Color { a: 0.5, ..self.ctx.t.text_secondary })
                        .with_width(1.0),
                );
                x += 8.0;
            }
        }

        vec![frame.into_geometry()]
    }
}
