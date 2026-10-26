//! Colour values of the Kumir actors.
//!
//! The original actors share the `_Colorer` module: a colour is a record with
//! `r`, `g`, `b` and `a` fields.  This implementation keeps colours as strings
//! (the historical Kumir syntax) and understands every textual form accepted by
//! the original: CSS names, `#RRGGBB[AA]`, `rgb()`, `rgba()`, `hsl()`, `hsla()`,
//! `hsv()`, `hsva()`, `cmyk()`, `cmyka()` and the Russian colour names.

/// An 8-bit RGBA colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const TRANSPARENT: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };
    pub const BLACK: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };
    pub const WHITE: Self = Self {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub fn is_transparent(self) -> bool {
        self.a == 0
    }

    /// `rgb(r, g, b)` or `rgba(r, g, b, a)` — the standard colour code of the
    /// original `_Colorer` module.
    pub fn to_kumir(self) -> String {
        if self.a == 255 {
            format!("rgb({}, {}, {})", self.r, self.g, self.b)
        } else {
            format!("rgba({}, {}, {}, {})", self.r, self.g, self.b, self.a)
        }
    }

    pub fn to_css(self) -> String {
        if self.a == 255 {
            format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
        } else {
            format!("#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, self.a)
        }
    }

    pub fn to_rgba_tuple(self) -> (u8, u8, u8, u8) {
        (self.r, self.g, self.b, self.a)
    }
}

fn clamp_byte(v: f64) -> u8 {
    v.round().clamp(0.0, 255.0) as u8
}

fn hex_digit(c: u8) -> Option<u8> {
    (c as char).to_digit(16).map(|v| v as u8)
}

fn parse_hex(text: &str) -> Option<Rgba> {
    let bytes = text.as_bytes();
    let mut digits = Vec::with_capacity(bytes.len());
    for &b in bytes {
        digits.push(hex_digit(b)?);
    }
    let (r, g, b, a) = match digits.len() {
        3 => (digits[0] * 17, digits[1] * 17, digits[2] * 17, 255),
        4 => (
            digits[0] * 17,
            digits[1] * 17,
            digits[2] * 17,
            digits[3] * 17,
        ),
        6 => (
            digits[0] * 16 + digits[1],
            digits[2] * 16 + digits[3],
            digits[4] * 16 + digits[5],
            255,
        ),
        8 => (
            digits[0] * 16 + digits[1],
            digits[2] * 16 + digits[3],
            digits[4] * 16 + digits[5],
            digits[6] * 16 + digits[7],
        ),
        _ => return None,
    };
    Some(Rgba { r, g, b, a })
}

/// A number from `rgb(...)`; `percent` is kept so HSL/HSV can interpret
/// percentages correctly (100% means 1.0 there, 255 in RGB).
fn split_components(inner: &str) -> Option<Vec<(f64, bool)>> {
    let mut result = Vec::new();
    for part in inner.split(',') {
        let t = part.trim();
        if t.is_empty() {
            return None;
        }
        if let Some(p) = t.strip_suffix('%') {
            result.push((p.trim().parse::<f64>().ok()?, true));
        } else {
            result.push((t.parse::<f64>().ok()?, false));
        }
    }
    Some(result)
}

fn hsl_to_rgb(h: f64, s: f64, l: f64) -> Rgba {
    let h = h.rem_euclid(360.0) / 360.0;
    let s = s.clamp(0.0, 1.0);
    let l = l.clamp(0.0, 1.0);
    let (r, g, b) = if s <= f64::EPSILON {
        (l, l, l)
    } else {
        let q = if l < 0.5 {
            l * (1.0 + s)
        } else {
            l + s - l * s
        };
        let p = 2.0 * l - q;
        let hue = |t: f64| {
            let t = t.rem_euclid(1.0);
            if t < 1.0 / 6.0 {
                p + (q - p) * 6.0 * t
            } else if t < 0.5 {
                q
            } else if t < 2.0 / 3.0 {
                p + (q - p) * (2.0 / 3.0 - t) * 6.0
            } else {
                p
            }
        };
        (hue(h + 1.0 / 3.0), hue(h), hue(h - 1.0 / 3.0))
    };
    Rgba {
        r: clamp_byte(r * 255.0),
        g: clamp_byte(g * 255.0),
        b: clamp_byte(b * 255.0),
        a: 255,
    }
}

fn hsv_to_rgb(h: f64, s: f64, v: f64) -> Rgba {
    let h = h.rem_euclid(360.0) / 60.0;
    let s = s.clamp(0.0, 1.0);
    let v = v.clamp(0.0, 1.0);
    let i = h.floor() as i64;
    let f = h - i as f64;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    let (r, g, b) = match i.rem_euclid(6) {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    Rgba {
        r: clamp_byte(r * 255.0),
        g: clamp_byte(g * 255.0),
        b: clamp_byte(b * 255.0),
        a: 255,
    }
}

fn cmyk_to_rgb(c: f64, m: f64, y: f64, k: f64) -> Rgba {
    let c = (c / 255.0).clamp(0.0, 1.0);
    let m = (m / 255.0).clamp(0.0, 1.0);
    let y = (y / 255.0).clamp(0.0, 1.0);
    let k = (k / 255.0).clamp(0.0, 1.0);
    Rgba {
        r: clamp_byte(255.0 * (1.0 - c) * (1.0 - k)),
        g: clamp_byte(255.0 * (1.0 - m) * (1.0 - k)),
        b: clamp_byte(255.0 * (1.0 - y) * (1.0 - k)),
        a: 255,
    }
}

fn parse_function(text: &str) -> Option<Rgba> {
    let open = text.find('(')?;
    let name = text[..open].trim().to_ascii_lowercase();
    let inner = text[open + 1..].strip_suffix(')')?;
    let v = split_components(inner)?;
    let channel = |i: usize| -> Option<f64> {
        let (value, percent) = *v.get(i)?;
        Some(if percent { value * 2.55 } else { value })
    };
    let ratio = |i: usize| -> Option<f64> {
        let (value, percent) = *v.get(i)?;
        Some(if percent { value / 100.0 } else { value })
    };
    let alpha = |i: usize| channel(i).map(clamp_byte).unwrap_or(255);
    let color = match name.as_str() {
        "rgb" => {
            if v.len() != 3 {
                return None;
            }
            Rgba::rgb(
                clamp_byte(channel(0)?),
                clamp_byte(channel(1)?),
                clamp_byte(channel(2)?),
            )
        }
        "rgba" => {
            if v.len() != 4 {
                return None;
            }
            Rgba {
                r: clamp_byte(channel(0)?),
                g: clamp_byte(channel(1)?),
                b: clamp_byte(channel(2)?),
                a: alpha(3),
            }
        }
        "hsl" | "hsv" | "hsla" | "hsva" => {
            let expected = if name.ends_with('a') { 4 } else { 3 };
            if v.len() != expected {
                return None;
            }
            let mut c = if name.starts_with("hsl") {
                hsl_to_rgb(ratio(0)?, ratio(1)?, ratio(2)?)
            } else {
                hsv_to_rgb(ratio(0)?, ratio(1)?, ratio(2)?)
            };
            if expected == 4 {
                c.a = alpha(3);
            }
            c
        }
        "cmyk" | "cmyka" => {
            let expected = if name == "cmyka" { 5 } else { 4 };
            if v.len() != expected {
                return None;
            }
            let mut c = cmyk_to_rgb(channel(0)?, channel(1)?, channel(2)?, channel(3)?);
            if expected == 5 {
                c.a = alpha(4);
            }
            c
        }
        _ => return None,
    };
    Some(color)
}

/// Parse a Kumir colour literal.  The empty string is transparent.
pub fn parse(text: &str) -> Option<Rgba> {
    let t = text.trim();
    if t.is_empty() {
        return Some(Rgba::TRANSPARENT);
    }
    if let Some(hex) = t.strip_prefix('#') {
        return parse_hex(hex);
    }
    let lower = t.to_lowercase();
    if let Some(color) = named(&lower) {
        return Some(color);
    }
    if lower.contains('(') {
        return parse_function(&lower);
    }
    // A bare hexadecimal number such as `ff0000`.
    if t.len() == 6 || t.len() == 8 {
        if let Some(c) = parse_hex(t) {
            return Some(c);
        }
    }
    None
}

/// Parse a colour, falling back to `fallback` for unknown names.
pub fn parse_or(text: &str, fallback: Rgba) -> Rgba {
    parse(text).unwrap_or(fallback)
}

fn named(name: &str) -> Option<Rgba> {
    let compact: String = name.chars().filter(|c| !c.is_whitespace()).collect();
    let compact = compact.as_str();
    let ru = match compact {
        "черный" | "чёрный" => Some(Rgba::rgb(0, 0, 0)),
        "белый" => Some(Rgba::rgb(255, 255, 255)),
        "серый" => Some(Rgba::rgb(128, 128, 128)),
        "фиолетовый" => Some(Rgba::rgb(128, 0, 128)),
        "синий" => Some(Rgba::rgb(0, 0, 255)),
        "голубой" => Some(Rgba::rgb(0, 128, 255)),
        "зеленый" | "зелёный" => Some(Rgba::rgb(0, 128, 0)),
        "желтый" | "жёлтый" => Some(Rgba::rgb(255, 255, 0)),
        "оранжевый" => Some(Rgba::rgb(255, 165, 0)),
        "красный" => Some(Rgba::rgb(255, 0, 0)),
        "прозрачный" => Some(Rgba::TRANSPARENT),
        _ => None,
    };
    if ru.is_some() {
        return ru;
    }
    CSS_NAMES
        .iter()
        .find(|(n, _)| *n == compact)
        .map(|(_, rgb)| Rgba::rgb((rgb >> 16) as u8, (rgb >> 8) as u8, *rgb as u8))
}

/// The W3C SVG colour keywords (as accepted by the original `_Colorer`).
const CSS_NAMES: &[(&str, u32)] = &[
    ("aliceblue", 0xf0f8ff),
    ("antiquewhite", 0xfaebd7),
    ("aqua", 0x00ffff),
    ("aquamarine", 0x7fffd4),
    ("azure", 0xf0ffff),
    ("beige", 0xf5f5dc),
    ("bisque", 0xffe4c4),
    ("black", 0x000000),
    ("blanchedalmond", 0xffebcd),
    ("blue", 0x0000ff),
    ("blueviolet", 0x8a2be2),
    ("brown", 0xa52a2a),
    ("burlywood", 0xdeb887),
    ("cadetblue", 0x5f9ea0),
    ("chartreuse", 0x7fff00),
    ("chocolate", 0xd2691e),
    ("coral", 0xff7f50),
    ("cornflowerblue", 0x6495ed),
    ("cornsilk", 0xfff8dc),
    ("crimson", 0xdc143c),
    ("cyan", 0x00ffff),
    ("darkblue", 0x00008b),
    ("darkcyan", 0x008b8b),
    ("darkgoldenrod", 0xb8860b),
    ("darkgray", 0xa9a9a9),
    ("darkgreen", 0x006400),
    ("darkgrey", 0xa9a9a9),
    ("darkkhaki", 0xbdb76b),
    ("darkmagenta", 0x8b008b),
    ("darkolivegreen", 0x556b2f),
    ("darkorange", 0xff8c00),
    ("darkorchid", 0x9932cc),
    ("darkred", 0x8b0000),
    ("darksalmon", 0xe9967a),
    ("darkseagreen", 0x8fbc8f),
    ("darkslateblue", 0x483d8b),
    ("darkslategray", 0x2f4f4f),
    ("darkslategrey", 0x2f4f4f),
    ("darkturquoise", 0x00ced1),
    ("darkviolet", 0x9400d3),
    ("deeppink", 0xff1493),
    ("deepskyblue", 0x00bfff),
    ("dimgray", 0x696969),
    ("dimgrey", 0x696969),
    ("dodgerblue", 0x1e90ff),
    ("firebrick", 0xb22222),
    ("floralwhite", 0xfffaf0),
    ("forestgreen", 0x228b22),
    ("fuchsia", 0xff00ff),
    ("gainsboro", 0xdcdcdc),
    ("ghostwhite", 0xf8f8ff),
    ("gold", 0xffd700),
    ("goldenrod", 0xdaa520),
    ("gray", 0x808080),
    ("grey", 0x808080),
    ("green", 0x008000),
    ("greenyellow", 0xadff2f),
    ("honeydew", 0xf0fff0),
    ("hotpink", 0xff69b4),
    ("indianred", 0xcd5c5c),
    ("indigo", 0x4b0082),
    ("ivory", 0xfffff0),
    ("khaki", 0xf0e68c),
    ("lavender", 0xe6e6fa),
    ("lavenderblush", 0xfff0f5),
    ("lawngreen", 0x7cfc00),
    ("lemonchiffon", 0xfffacd),
    ("lightblue", 0xadd8e6),
    ("lightcoral", 0xf08080),
    ("lightcyan", 0xe0ffff),
    ("lightgoldenrodyellow", 0xfafad2),
    ("lightgray", 0xd3d3d3),
    ("lightgreen", 0x90ee90),
    ("lightgrey", 0xd3d3d3),
    ("lightpink", 0xffb6c1),
    ("lightsalmon", 0xffa07a),
    ("lightseagreen", 0x20b2aa),
    ("lightskyblue", 0x87cefa),
    ("lightslategray", 0x778899),
    ("lightslategrey", 0x778899),
    ("lightsteelblue", 0xb0c4de),
    ("lightyellow", 0xffffe0),
    ("lime", 0x00ff00),
    ("limegreen", 0x32cd32),
    ("linen", 0xfaf0e6),
    ("magenta", 0xff00ff),
    ("maroon", 0x800000),
    ("mediumaquamarine", 0x66cdaa),
    ("mediumblue", 0x0000cd),
    ("mediumorchid", 0xba55d3),
    ("mediumpurple", 0x9370db),
    ("mediumseagreen", 0x3cb371),
    ("mediumslateblue", 0x7b68ee),
    ("mediumspringgreen", 0x00fa9a),
    ("mediumturquoise", 0x48d1cc),
    ("mediumvioletred", 0xc71585),
    ("midnightblue", 0x191970),
    ("mintcream", 0xf5fffa),
    ("mistyrose", 0xffe4e1),
    ("moccasin", 0xffe4b5),
    ("navajowhite", 0xffdead),
    ("navy", 0x000080),
    ("oldlace", 0xfdf5e6),
    ("olive", 0x808000),
    ("olivedrab", 0x6b8e23),
    ("orange", 0xffa500),
    ("orangered", 0xff4500),
    ("orchid", 0xda70d6),
    ("palegoldenrod", 0xeee8aa),
    ("palegreen", 0x98fb98),
    ("paleturquoise", 0xafeeee),
    ("palevioletred", 0xdb7093),
    ("papayawhip", 0xffefd5),
    ("peachpuff", 0xffdab9),
    ("peru", 0xcd853f),
    ("pink", 0xffc0cb),
    ("plum", 0xdda0dd),
    ("powderblue", 0xb0e0e6),
    ("purple", 0x800080),
    ("red", 0xff0000),
    ("rosybrown", 0xbc8f8f),
    ("royalblue", 0x4169e1),
    ("saddlebrown", 0x8b4513),
    ("salmon", 0xfa8072),
    ("sandybrown", 0xf4a460),
    ("seagreen", 0x2e8b57),
    ("seashell", 0xfff5ee),
    ("sienna", 0xa0522d),
    ("silver", 0xc0c0c0),
    ("skyblue", 0x87ceeb),
    ("slateblue", 0x6a5acd),
    ("slategray", 0x708090),
    ("slategrey", 0x708090),
    ("snow", 0xfffafa),
    ("springgreen", 0x00ff7f),
    ("steelblue", 0x4682b4),
    ("tan", 0xd2b48c),
    ("teal", 0x008080),
    ("thistle", 0xd8bfd8),
    ("tomato", 0xff6347),
    ("turquoise", 0x40e0d0),
    ("violet", 0xee82ee),
    ("wheat", 0xf5deb3),
    ("white", 0xffffff),
    ("whitesmoke", 0xf5f5f5),
    ("yellow", 0xffff00),
    ("yellowgreen", 0x9acd32),
];

/// Split a colour into RGB, used by `разложить в RGB`.
pub fn split_rgb(color: Rgba) -> (i64, i64, i64) {
    (color.r as i64, color.g as i64, color.b as i64)
}

pub fn split_cmyk(color: Rgba) -> (i64, i64, i64, i64) {
    let r = color.r as f64 / 255.0;
    let g = color.g as f64 / 255.0;
    let b = color.b as f64 / 255.0;
    let k = 1.0 - r.max(g).max(b);
    if k >= 1.0 {
        return (0, 0, 0, 255);
    }
    let c = (1.0 - r - k) / (1.0 - k);
    let m = (1.0 - g - k) / (1.0 - k);
    let y = (1.0 - b - k) / (1.0 - k);
    (
        clamp_byte(c * 255.0) as i64,
        clamp_byte(m * 255.0) as i64,
        clamp_byte(y * 255.0) as i64,
        clamp_byte(k * 255.0) as i64,
    )
}

pub fn split_hsl(color: Rgba) -> (i64, i64, i64) {
    let r = color.r as f64 / 255.0;
    let g = color.g as f64 / 255.0;
    let b = color.b as f64 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let d = max - min;
    let s = if d.abs() < f64::EPSILON {
        0.0
    } else {
        d / (1.0 - (2.0 * l - 1.0).abs())
    };
    let h = if d.abs() < f64::EPSILON {
        0.0
    } else if max == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    (
        h.round() as i64,
        (s * 100.0).round() as i64,
        (l * 100.0).round() as i64,
    )
}

pub fn split_hsv(color: Rgba) -> (i64, i64, i64) {
    let r = color.r as f64 / 255.0;
    let g = color.g as f64 / 255.0;
    let b = color.b as f64 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let v = max;
    let s = if max.abs() < f64::EPSILON {
        0.0
    } else {
        d / max
    };
    let h = if d.abs() < f64::EPSILON {
        0.0
    } else if max == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    (
        h.round() as i64,
        (s * 100.0).round() as i64,
        (v * 100.0).round() as i64,
    )
}

pub fn from_rgb(r: i64, g: i64, b: i64) -> Rgba {
    Rgba {
        r: clamp_byte(r as f64),
        g: clamp_byte(g as f64),
        b: clamp_byte(b as f64),
        a: 255,
    }
}

pub fn from_rgba(r: i64, g: i64, b: i64, a: i64) -> Rgba {
    Rgba {
        r: clamp_byte(r as f64),
        g: clamp_byte(g as f64),
        b: clamp_byte(b as f64),
        a: clamp_byte(a as f64),
    }
}

pub fn from_hsl(h: i64, s: i64, l: i64) -> Rgba {
    hsl_to_rgb(h as f64, s as f64 / 100.0, l as f64 / 100.0)
}

pub fn from_hsv(h: i64, s: i64, v: i64) -> Rgba {
    hsv_to_rgb(h as f64, s as f64 / 100.0, v as f64 / 100.0)
}

pub fn from_cmyk(c: i64, m: i64, y: i64, k: i64) -> Rgba {
    cmyk_to_rgb(c as f64, m as f64, y as f64, k as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_all_documented_forms() {
        assert_eq!(parse("#ff0000"), Some(Rgba::rgb(255, 0, 0)));
        assert_eq!(parse("#f00"), Some(Rgba::rgb(255, 0, 0)));
        assert_eq!(parse("#ff000080").map(|c| c.a), Some(0x80));
        assert_eq!(parse("red"), Some(Rgba::rgb(255, 0, 0)));
        assert_eq!(parse("красный"), Some(Rgba::rgb(255, 0, 0)));
        assert_eq!(parse("rgb(255, 0, 0)"), Some(Rgba::rgb(255, 0, 0)));
        assert_eq!(parse("rgba(0, 0, 0, 128)").map(|c| c.a), Some(128));
        assert_eq!(parse("hsl(0, 100%, 50%)"), Some(Rgba::rgb(255, 0, 0)));
        assert_eq!(
            parse("cmyk(0, 0, 255, 0)").map(|c| (c.r, c.g, c.b)),
            Some((255, 255, 0))
        );
        assert_eq!(parse(""), Some(Rgba::TRANSPARENT));
        assert_eq!(parse("нет такого цвета"), None);
    }

    #[test]
    fn standard_code_roundtrip() {
        let c = Rgba::rgb(12, 34, 56);
        assert_eq!(c.to_kumir(), "rgb(12, 34, 56)");
        assert_eq!(parse(&c.to_kumir()), Some(c));
        assert_eq!(parse(&c.to_css()), Some(c));
    }

    #[test]
    fn splits_back() {
        assert_eq!(split_rgb(Rgba::rgb(1, 2, 3)), (1, 2, 3));
        assert_eq!(split_cmyk(Rgba::rgb(255, 0, 0)), (0, 255, 255, 0));
        assert_eq!(split_hsv(Rgba::rgb(0, 255, 0)), (120, 100, 100));
    }
}
