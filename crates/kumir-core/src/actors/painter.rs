//! The Painter actor (Рисователь): a raster sheet with a pen and a brush.
//!
//! The sheet is a plain RGBA raster: every drawing command paints pixels, and
//! the IDE uploads it as a texture.  Text is stored as separate items and is
//! rendered by the IDE; `ширина текста` therefore uses a deterministic
//! approximation instead of the fonts of the host system (the original used
//! Qt font metrics).

use std::path::Path;

use super::color::{self, Rgba};

pub const DEFAULT_WIDTH: i32 = 640;
pub const DEFAULT_HEIGHT: i32 = 480;
pub const MIN_SHEET: i32 = 20;

#[derive(Clone, Debug)]
pub struct PainterPage {
    pub width: i32,
    pub height: i32,
    pub background: Rgba,
    /// Row-major RGBA pixels, `width * height` entries.
    pub pixels: Vec<Rgba>,
}

impl PainterPage {
    pub fn new(width: i32, height: i32, background: Rgba) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        Self {
            width,
            height,
            background,
            pixels: vec![background; (width * height) as usize],
        }
    }

    pub fn index(&self, x: i32, y: i32) -> Option<usize> {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            None
        } else {
            Some((y * self.width + x) as usize)
        }
    }

    pub fn pixel(&self, x: i32, y: i32) -> Option<Rgba> {
        self.index(x, y).map(|i| self.pixels[i])
    }

    pub fn set(&mut self, x: i32, y: i32, color: Rgba) {
        if let Some(i) = self.index(x, y) {
            self.pixels[i] = color;
        }
    }
}

#[derive(Clone, Debug)]
pub struct TextItem {
    pub x: i32,
    pub y: i32,
    pub text: String,
    pub color: Rgba,
    pub size: i32,
    pub bold: bool,
    pub italic: bool,
    pub family: String,
}

#[derive(Clone, Debug)]
pub struct PainterState {
    pub page: PainterPage,
    pub initial: PainterPage,
    pub initial_texts: Vec<TextItem>,
    pub texts: Vec<TextItem>,
    pub pen_width: i32,
    pub pen_color: Rgba,
    /// `None` means "no brush": shapes are not filled.
    pub brush: Option<Rgba>,
    pub font_family: String,
    pub font_size: i32,
    pub bold: bool,
    pub italic: bool,
    pub pos: (i32, i32),
    pub density: i32,
    /// Incremented on every change so the IDE can cache the texture.
    pub version: u64,
}

impl Default for PainterState {
    fn default() -> Self {
        Self::new()
    }
}

impl PainterState {
    pub fn new() -> Self {
        let page = PainterPage::new(DEFAULT_WIDTH, DEFAULT_HEIGHT, Rgba::WHITE);
        Self {
            initial: page.clone(),
            page,
            initial_texts: Vec::new(),
            texts: Vec::new(),
            pen_width: 1,
            pen_color: Rgba::BLACK,
            brush: Some(Rgba::WHITE),
            font_family: "Sans".to_string(),
            font_size: 14,
            bold: false,
            italic: false,
            pos: (0, 0),
            density: 0,
            version: 1,
        }
    }

    pub fn reset(&mut self) {
        self.page = self.initial.clone();
        self.texts = self.initial_texts.clone();
        self.pen_width = 1;
        self.pen_color = Rgba::BLACK;
        self.brush = Some(Rgba::WHITE);
        self.font_family = "Sans".to_string();
        self.font_size = 14;
        self.bold = false;
        self.italic = false;
        self.pos = (0, 0);
        self.density = 0;
        self.touch();
    }

    /// Make the current sheet the initial one (used after loading a PNG).
    pub fn commit_sheet(&mut self) {
        self.initial = self.page.clone();
        self.initial_texts = self.texts.clone();
        self.touch();
    }

    fn touch(&mut self) {
        self.version = self.version.wrapping_add(1);
    }

    pub fn set_pen(&mut self, width: i32, color: Rgba) -> Result<(), String> {
        if width < 0 {
            return Err("Толщина пера не может быть отрицательной".to_string());
        }
        self.pen_width = width;
        self.pen_color = color;
        self.touch();
        Ok(())
    }

    pub fn set_brush(&mut self, brush: Option<Rgba>) {
        self.brush = brush.filter(|c| !c.is_transparent());
        self.touch();
    }

    pub fn set_font(
        &mut self,
        family: &str,
        size: i32,
        bold: bool,
        italic: bool,
    ) -> Result<(), String> {
        if size <= 0 {
            return Err("Размер шрифта должен быть положительным".to_string());
        }
        self.font_family = family.to_string();
        self.font_size = size;
        self.bold = bold;
        self.italic = italic;
        self.touch();
        Ok(())
    }

    /// Deterministic approximation of the text width used by `ширина текста`.
    pub fn text_width(&self, text: &str) -> i32 {
        let factor = if self.bold { 0.62 } else { 0.58 };
        (text.chars().count() as f64 * self.font_size as f64 * factor).round() as i32
    }

    fn stamp(&mut self, x: f64, y: f64, color: Rgba) {
        let width = self.pen_width.max(0);
        if width == 0 {
            return;
        }
        let radius = (width as f64 - 1.0) / 2.0;
        let cx = x.round() as i32;
        let cy = y.round() as i32;
        let r = radius.ceil() as i32;
        for dy in -r..=r {
            for dx in -r..=r {
                if (dx as f64).powi(2) + (dy as f64).powi(2) <= (radius + 0.5).powi(2) {
                    self.page.set(cx + dx, cy + dy, color);
                }
            }
        }
    }

    fn line_pixels(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, color: Rgba) {
        let dx = (x2 - x1).abs();
        let dy = (y2 - y1).abs();
        let steps = dx.max(dy).max(1.0).ceil() as i32;
        for i in 0..=steps {
            let t = i as f64 / steps as f64;
            self.stamp(x1 + (x2 - x1) * t, y1 + (y2 - y1) * t, color);
        }
    }

    pub fn move_to(&mut self, x: i32, y: i32) {
        if self.page.index(x, y).is_some() {
            self.pos = (x, y);
        }
    }

    pub fn line_to(&mut self, x: i32, y: i32) {
        let (x1, y1) = self.pos;
        let color = self.pen_color;
        self.line_pixels(x1 as f64, y1 as f64, x as f64, y as f64, color);
        self.pos = (x, y);
        self.touch();
    }

    pub fn line(&mut self, x1: i32, y1: i32, x2: i32, y2: i32) {
        let color = self.pen_color;
        self.line_pixels(x1 as f64, y1 as f64, x2 as f64, y2 as f64, color);
        self.pos = (x2, y2);
        self.touch();
    }

    fn fill_rect(&mut self, x1: i32, y1: i32, x2: i32, y2: i32, color: Rgba) {
        let (lx, hx) = (x1.min(x2), x1.max(x2));
        let (ly, hy) = (y1.min(y2), y1.max(y2));
        for y in ly..=hy {
            for x in lx..=hx {
                self.page.set(x, y, color);
            }
        }
    }

    pub fn rectangle(&mut self, x1: i32, y1: i32, x2: i32, y2: i32) {
        if let Some(brush) = self.brush {
            self.fill_rect(x1, y1, x2, y2, brush);
        }
        if self.pen_width > 0 {
            let color = self.pen_color;
            let (lx, hx) = (x1.min(x2), x1.max(x2));
            let (ly, hy) = (y1.min(y2), y1.max(y2));
            self.line_pixels(lx as f64, ly as f64, hx as f64, ly as f64, color);
            self.line_pixels(hx as f64, ly as f64, hx as f64, hy as f64, color);
            self.line_pixels(hx as f64, hy as f64, lx as f64, hy as f64, color);
            self.line_pixels(lx as f64, hy as f64, lx as f64, ly as f64, color);
        }
        self.pos = (x2, y2);
        self.touch();
    }

    fn ellipse_pixels(&mut self, x1: i32, y1: i32, x2: i32, y2: i32, filled: bool, stroke: bool) {
        let (lx, hx) = (x1.min(x2), x1.max(x2));
        let (ly, hy) = (y1.min(y2), y1.max(y2));
        let cx = (lx + hx) as f64 / 2.0;
        let cy = (ly + hy) as f64 / 2.0;
        let rx = ((hx - lx) as f64 / 2.0).max(0.5);
        let ry = ((hy - ly) as f64 / 2.0).max(0.5);
        if filled {
            if let Some(brush) = self.brush {
                for y in ly..=hy {
                    for x in lx..=hx {
                        let nx = (x as f64 + 0.5 - cx) / rx;
                        let ny = (y as f64 + 0.5 - cy) / ry;
                        if nx * nx + ny * ny <= 1.0 {
                            self.page.set(x, y, brush);
                        }
                    }
                }
            }
        }
        if stroke && self.pen_width > 0 {
            let color = self.pen_color;
            let steps = ((rx + ry) * 2.0).max(24.0) as i32;
            for i in 0..steps {
                let t = std::f64::consts::TAU * i as f64 / steps as f64;
                self.stamp(cx + rx * t.cos(), cy + ry * t.sin(), color);
            }
        }
    }

    pub fn ellipse(&mut self, x1: i32, y1: i32, x2: i32, y2: i32) {
        self.ellipse_pixels(x1, y1, x2, y2, true, true);
        self.touch();
    }

    pub fn circle(&mut self, cx: i32, cy: i32, r: i32) -> Result<(), String> {
        if r < 0 {
            return Err("Радиус окружности не может быть отрицательным".to_string());
        }
        self.ellipse_pixels(cx - r, cy - r, cx + r, cy + r, true, true);
        self.touch();
        Ok(())
    }

    pub fn polygon(&mut self, xs: &[i64], ys: &[i64]) {
        let n = xs.len().min(ys.len());
        if n < 2 {
            return;
        }
        if let Some(brush) = self.brush {
            let y_min = ys[..n].iter().min().copied().unwrap_or(0) as i32;
            let y_max = ys[..n].iter().max().copied().unwrap_or(0) as i32;
            for y in y_min..=y_max {
                let mut crossings: Vec<f64> = Vec::new();
                for i in 0..n {
                    let j = (i + 1) % n;
                    let (x1, y1) = (xs[i] as f64, ys[i] as f64);
                    let (x2, y2) = (xs[j] as f64, ys[j] as f64);
                    let py = y as f64 + 0.5;
                    if (y1 <= py && y2 > py) || (y2 <= py && y1 > py) {
                        let t = (py - y1) / (y2 - y1);
                        crossings.push(x1 + t * (x2 - x1));
                    }
                }
                crossings.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                for pair in crossings.chunks(2) {
                    if let [left, right] = pair {
                        let lx = left.ceil() as i32;
                        let hx = right.floor() as i32;
                        for x in lx..=hx {
                            self.page.set(x, y, brush);
                        }
                    }
                }
            }
        }
        if self.pen_width > 0 {
            let color = self.pen_color;
            for i in 0..n {
                let j = (i + 1) % n;
                self.line_pixels(
                    xs[i] as f64,
                    ys[i] as f64,
                    xs[j] as f64,
                    ys[j] as f64,
                    color,
                );
            }
        }
        self.pos = (xs[0] as i32, ys[0] as i32);
        self.touch();
    }

    pub fn write(&mut self, x: i32, y: i32, text: &str) {
        self.texts.push(TextItem {
            x,
            y,
            text: text.to_string(),
            color: self.pen_color,
            size: self.font_size,
            bold: self.bold,
            italic: self.italic,
            family: self.font_family.clone(),
        });
        self.pos = (x, y);
        self.touch();
    }

    pub fn set_pixel(&mut self, x: i32, y: i32, color: Rgba) -> Result<(), String> {
        if self.page.index(x, y).is_none() {
            return Err(format!("Точка ({x}, {y}) вне листа"));
        }
        self.page.set(x, y, color);
        self.pos = (x, y);
        self.touch();
        Ok(())
    }

    /// `значение в точке`: the standard colour code of a pixel.
    pub fn pixel_color(&self, x: i32, y: i32) -> Result<String, String> {
        self.page
            .pixel(x, y)
            .map(|c| c.to_kumir())
            .ok_or_else(|| format!("Точка ({x}, {y}) вне листа"))
    }

    pub fn flood_fill(&mut self, x: i32, y: i32) {
        let Some(brush) = self.brush else {
            return;
        };
        let Some(start) = self.page.index(x, y) else {
            return;
        };
        let target = self.page.pixels[start];
        if target == brush {
            return;
        }
        let mut stack = vec![(x, y)];
        while let Some((px, py)) = stack.pop() {
            let Some(i) = self.page.index(px, py) else {
                continue;
            };
            if self.page.pixels[i] != target {
                continue;
            }
            self.page.pixels[i] = brush;
            stack.push((px + 1, py));
            stack.push((px - 1, py));
            stack.push((px, py + 1));
            stack.push((px, py - 1));
        }
        self.touch();
    }

    pub fn new_page(&mut self, width: i32, height: i32, background: Rgba) -> Result<(), String> {
        if width < MIN_SHEET || height < MIN_SHEET {
            return Err(format!(
                "Минимальный размер листа — {MIN_SHEET}×{MIN_SHEET}"
            ));
        }
        self.page = PainterPage::new(width, height, background);
        self.texts.clear();
        self.pos = (0, 0);
        self.touch();
        Ok(())
    }

    pub fn rgba_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.page.pixels.len() * 4);
        for c in &self.page.pixels {
            out.extend_from_slice(&[c.r, c.g, c.b, c.a]);
        }
        out
    }

    pub fn save_png(&self, path: &Path) -> Result<(), String> {
        let file = std::fs::File::create(path)
            .map_err(|e| format!("Не удалось создать файл «{}»: {e}", path.display()))?;
        let writer = std::io::BufWriter::new(file);
        let mut encoder =
            png::Encoder::new(writer, self.page.width as u32, self.page.height as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|e| format!("Ошибка записи PNG: {e}"))?;
        writer
            .write_image_data(&self.rgba_bytes())
            .map_err(|e| format!("Ошибка записи PNG: {e}"))?;
        Ok(())
    }

    pub fn load_png(path: &Path) -> Result<Self, String> {
        let file = std::fs::File::open(path)
            .map_err(|e| format!("Не могу прочитать «{}»: {e}", path.display()))?;
        let decoder = png::Decoder::new(std::io::BufReader::new(file));
        let mut reader = decoder
            .read_info()
            .map_err(|e| format!("Неверный PNG: {e}"))?;
        let size = reader
            .output_buffer_size()
            .ok_or("Слишком большой PNG-файл".to_string())?;
        let mut buffer = vec![0u8; size];
        let info = reader
            .next_frame(&mut buffer)
            .map_err(|e| format!("Ошибка чтения PNG: {e}"))?;
        let width = info.width as i32;
        let height = info.height as i32;
        let mut page = PainterPage::new(width, height, Rgba::WHITE);
        let channels = match info.color_type {
            png::ColorType::Grayscale => 1,
            png::ColorType::GrayscaleAlpha => 2,
            png::ColorType::Rgb => 3,
            png::ColorType::Rgba => 4,
            png::ColorType::Indexed => {
                return Err("Индексированные PNG не поддерживаются".to_string())
            }
        };
        for i in 0..(width * height) as usize {
            let base = i * channels;
            let color = match channels {
                1 => {
                    let g = buffer[base];
                    Rgba::rgb(g, g, g)
                }
                2 => {
                    let g = buffer[base];
                    Rgba {
                        r: g,
                        g,
                        b: g,
                        a: buffer[base + 1],
                    }
                }
                3 => Rgba::rgb(buffer[base], buffer[base + 1], buffer[base + 2]),
                _ => Rgba {
                    r: buffer[base],
                    g: buffer[base + 1],
                    b: buffer[base + 2],
                    a: buffer[base + 3],
                },
            };
            page.pixels[i] = color;
        }
        let mut state = PainterState::new();
        state.page = page.clone();
        state.initial = page;
        state.commit_sheet();
        Ok(state)
    }
}

impl PainterState {
    /// Convenience used by the command dispatcher: parse a colour or fail with
    /// a Kumir-style message.
    pub fn parse_color(&self, text: &str) -> Result<Rgba, String> {
        color::parse(text).ok_or_else(|| format!("Недопустимый цвет: «{text}»"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitives_paint_pixels() {
        let mut p = PainterState::new();
        p.set_pen(3, Rgba::rgb(255, 0, 0)).unwrap();
        p.rectangle(10, 10, 30, 20);
        // The outline is painted, the middle is not (no brush).
        assert_eq!(p.page.pixel(10, 10), Some(Rgba::rgb(255, 0, 0)));
        assert_eq!(p.page.pixel(20, 15), Some(Rgba::WHITE));
        p.set_brush(Some(Rgba::rgb(0, 0, 255)));
        p.rectangle(40, 40, 50, 50);
        assert_eq!(p.page.pixel(45, 45), Some(Rgba::rgb(0, 0, 255)));
    }

    #[test]
    fn flood_fill_replaces_the_region() {
        let mut p = PainterState::new();
        p.set_pen(2, Rgba::BLACK).unwrap();
        p.set_brush(Some(Rgba::WHITE));
        p.rectangle(5, 5, 20, 20);
        p.set_brush(Some(Rgba::rgb(0, 200, 0)));
        p.flood_fill(12, 12);
        assert_eq!(p.page.pixel(12, 12), Some(Rgba::rgb(0, 200, 0)));
        assert_eq!(p.page.pixel(0, 0), Some(Rgba::WHITE));
    }

    #[test]
    fn png_roundtrip() {
        let mut p = PainterState::new();
        p.set_pixel(3, 4, Rgba::rgb(1, 2, 3)).unwrap();
        let path = std::env::temp_dir().join(format!("kumir-painter-{}.png", std::process::id()));
        p.save_png(&path).unwrap();
        let loaded = PainterState::load_png(&path).unwrap();
        assert_eq!(loaded.page.pixel(3, 4), Some(Rgba::rgb(1, 2, 3)));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn new_page_validates_size() {
        let mut p = PainterState::new();
        assert!(p.new_page(10, 10, Rgba::WHITE).is_err());
        assert!(p.new_page(100, 80, Rgba::WHITE).is_ok());
        assert_eq!((p.page.width, p.page.height), (100, 80));
    }
}
