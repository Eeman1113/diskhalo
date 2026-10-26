//! Half-block pixel canvas.
//!
//! A terminal cell is roughly twice as tall as it is wide, so drawing circles
//! with whole cells produces ovals. Writing the upper-half-block glyph `▀` with
//! one colour as foreground and another as background splits each cell into two
//! stacked pixels — doubling vertical resolution and making pixels near-square,
//! which is what lets a sunburst actually look round.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect as TRect;
use ratatui::style::Color;

#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl From<Rgb> for Color {
    fn from(c: Rgb) -> Self {
        Color::Rgb(c.0, c.1, c.2)
    }
}

pub struct Canvas {
    pub w: usize,
    /// Pixel rows — always twice the cell height.
    pub h: usize,
    px: Vec<Option<Rgb>>,
}

impl Canvas {
    pub fn new(cells_w: u16, cells_h: u16) -> Self {
        let w = cells_w as usize;
        let h = cells_h as usize * 2;
        Canvas {
            w,
            h,
            px: vec![None; w * h],
        }
    }

    #[inline]
    pub fn set(&mut self, x: usize, y: usize, c: Rgb) {
        if x < self.w && y < self.h {
            self.px[y * self.w + x] = Some(c);
        }
    }

    #[inline]
    pub fn get(&self, x: usize, y: usize) -> Option<Rgb> {
        if x < self.w && y < self.h {
            self.px[y * self.w + x]
        } else {
            None
        }
    }

    /// Blit onto a ratatui buffer, pairing each two pixel rows into one cell.
    pub fn render(&self, area: TRect, buf: &mut Buffer) {
        for cy in 0..(self.h / 2) {
            let sy = area.y + cy as u16;
            if sy >= area.y + area.height {
                break;
            }
            for cx in 0..self.w {
                let sx = area.x + cx as u16;
                if sx >= area.x + area.width {
                    break;
                }
                let top = self.get(cx, cy * 2);
                let bot = self.get(cx, cy * 2 + 1);
                let cell = &mut buf[(sx, sy)];
                match (top, bot) {
                    (None, None) => {}
                    // Only one half painted: use the block that covers just it,
                    // leaving the other half as the terminal's own background.
                    (Some(t), None) => {
                        cell.set_char('\u{2580}').set_fg(t.into());
                    }
                    (None, Some(b)) => {
                        cell.set_char('\u{2584}').set_fg(b.into());
                    }
                    (Some(t), Some(b)) => {
                        cell.set_char('\u{2580}').set_fg(t.into()).set_bg(b.into());
                    }
                }
            }
        }
    }
}
