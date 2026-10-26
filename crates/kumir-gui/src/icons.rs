//! Vector icons of the IDE.
//!
//! Every icon is a tiny SVG-like path set drawn with `egui::Shape`s, so it
//! scales crisply on any DPI, follows the current colour scheme and needs no
//! image files or font glyphs (emoji are not used anywhere in the interface).

use egui::epaint::CircleShape;
use egui::{Color32, CornerRadius, Painter, Pos2, Rect, Stroke, StrokeKind, Vec2};

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Folder,
    FolderUp,
    FolderOpen,
    File,
    FileKod,
    FileFil,
    Recent,
    Refresh,
    Plus,
    Close,
    ChevronLeft,
    ChevronRight,
    ChevronDown,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    ArrowDown,
    Run,
    Stop,
    Pause,
    Step,
    Check,
    Evaluate,
    Breakpoint,
    Save,
    SaveAs,
    Open,
    Search,
    Replace,
    Undo,
    Redo,
    ZoomIn,
    ZoomOut,
    Fit,
    Pen,
    Settings,
    Help,
    Info,
    About,
    Cut,
    Copy,
    Paste,
    SelectAll,
    Sparkles,
    Code,
    Format,
    Comment,
    Definition,
    Warning,
    Error,
    Trash,
    Keyboard,
    Palette,
    Brush,
    Wall,
    Mark,
    Hand,
    Robot,
    Turtle,
    Draw,
    Vodoley,
    Grasshopper,
    Painter,
    Grid,
    Download,
    Upload,
    Book,
    Star,
}

enum Prim {
    /// Closed filled polygon.
    Fill(&'static [(f32, f32)]),
    /// Open stroked polyline.
    Line(&'static [(f32, f32)]),
    /// Filled circle: `cx, cy, radius`.
    Circle(f32, f32, f32),
    /// Stroked circle.
    CircleLine(f32, f32, f32),
    /// Filled rectangle: `x, y, w, h`.
    Rect(f32, f32, f32, f32),
    /// Stroked rectangle.
    RectLine(f32, f32, f32, f32),
    /// Stroked rounded rectangle: `x, y, w, h, radius`.
    RoundLine(f32, f32, f32, f32, f32),
    /// Filled rounded rectangle.
    RoundFill(f32, f32, f32, f32, f32),
}

const fn fill(points: &'static [(f32, f32)]) -> Prim {
    Prim::Fill(points)
}
const fn line(points: &'static [(f32, f32)]) -> Prim {
    Prim::Line(points)
}
const fn circle(cx: f32, cy: f32, r: f32) -> Prim {
    Prim::Circle(cx, cy, r)
}
const fn circle_line(cx: f32, cy: f32, r: f32) -> Prim {
    Prim::CircleLine(cx, cy, r)
}
const fn rect(x: f32, y: f32, w: f32, h: f32) -> Prim {
    Prim::Rect(x, y, w, h)
}
const fn rect_line(x: f32, y: f32, w: f32, h: f32) -> Prim {
    Prim::RectLine(x, y, w, h)
}
const fn round_line(x: f32, y: f32, w: f32, h: f32, r: f32) -> Prim {
    Prim::RoundLine(x, y, w, h, r)
}
const fn round_fill(x: f32, y: f32, w: f32, h: f32, r: f32) -> Prim {
    Prim::RoundFill(x, y, w, h, r)
}

macro_rules! icons {
    ($($name:ident => [$( $prim:expr ),* $(,)?]),* $(,)?) => {
        fn prims(icon: Icon) -> &'static [Prim] {
            match icon {
                $(Icon::$name => {
                    const P: &[Prim] = &[$($prim),*];
                    P
                })*
            }
        }
    };
}

icons! {
    Folder => [
        fill(&[(3.0, 6.0), (10.0, 6.0), (12.0, 8.5), (21.0, 8.5), (21.0, 19.0), (3.0, 19.0)]),
    ],
    FolderUp => [
        line(&[(6.0, 20.0), (3.0, 20.0), (3.0, 6.0), (10.0, 6.0), (12.0, 8.5), (14.0, 8.5)]),
        line(&[(12.0, 17.0), (12.0, 10.0), (9.0, 13.0)]),
        line(&[(12.0, 10.0), (15.0, 13.0)]),
    ],
    FolderOpen => [
        fill(&[(3.0, 6.0), (10.0, 6.0), (12.0, 8.5), (20.0, 8.5), (20.0, 11.0), (3.0, 11.0)]),
        line(&[(3.0, 19.0), (6.0, 11.0), (22.0, 11.0), (19.0, 19.0), (3.0, 19.0)]),
    ],
    File => [
        round_line(5.0, 2.5, 14.0, 19.0, 2.0),
        line(&[(13.0, 2.5), (13.0, 7.0), (19.0, 7.0)]),
        line(&[(8.0, 12.0), (16.0, 12.0)]),
        line(&[(8.0, 16.0), (14.0, 16.0)]),
    ],
    FileKod => [
        round_line(4.5, 2.5, 15.0, 19.0, 2.0),
        circle_line(12.0, 12.0, 3.0),
        line(&[(12.0, 7.0), (12.0, 9.0)]),
        line(&[(12.0, 15.0), (12.0, 17.0)]),
        line(&[(7.0, 12.0), (9.0, 12.0)]),
        line(&[(15.0, 12.0), (17.0, 12.0)]),
    ],
    FileFil => [
        round_line(3.5, 4.0, 17.0, 16.0, 2.0),
        line(&[(3.5, 9.0), (20.5, 9.0)]),
        line(&[(9.0, 4.0), (9.0, 20.0)]),
        line(&[(15.0, 4.0), (15.0, 20.0)]),
        line(&[(3.5, 15.0), (20.5, 15.0)]),
    ],
    Recent => [
        circle_line(12.0, 12.0, 9.0),
        line(&[(12.0, 7.0), (12.0, 12.0), (16.0, 14.0)]),
    ],
    Refresh => [
        line(&[(20.0, 12.0), (20.0, 6.0), (17.0, 9.0)]),
        line(&[(20.0, 6.0), (14.0, 6.0), (16.5, 8.5)]),
        line(&[(4.0, 12.0), (4.0, 12.0)]),
        line(&[(4.0, 18.0), (7.0, 15.0)]),
        line(&[(4.0, 18.0), (10.0, 18.0), (7.5, 15.5)]),
        line(&[(3.5, 9.0), (6.0, 6.0), (10.5, 4.5)]),
        line(&[(20.5, 15.0), (18.0, 18.0), (13.5, 19.5)]),
    ],
    Plus => [line(&[(12.0, 5.0), (12.0, 19.0)]), line(&[(5.0, 12.0), (19.0, 12.0)])],
    Close => [line(&[(6.0, 6.0), (18.0, 18.0)]), line(&[(18.0, 6.0), (6.0, 18.0)])],
    ChevronLeft => [line(&[(15.0, 5.0), (9.0, 12.0), (15.0, 19.0)])],
    ChevronRight => [line(&[(9.0, 5.0), (15.0, 12.0), (9.0, 19.0)])],
    ChevronDown => [line(&[(5.0, 9.0), (12.0, 15.0), (19.0, 9.0)])],
    ArrowLeft => [line(&[(19.0, 12.0), (5.0, 12.0)]), line(&[(10.0, 7.0), (5.0, 12.0), (10.0, 17.0)])],
    ArrowRight => [line(&[(5.0, 12.0), (19.0, 12.0)]), line(&[(14.0, 7.0), (19.0, 12.0), (14.0, 17.0)])],
    ArrowUp => [line(&[(12.0, 19.0), (12.0, 5.0)]), line(&[(7.0, 10.0), (12.0, 5.0), (17.0, 10.0)])],
    ArrowDown => [line(&[(12.0, 5.0), (12.0, 19.0)]), line(&[(7.0, 14.0), (12.0, 19.0), (17.0, 14.0)])],
    Run => [fill(&[(7.0, 4.0), (20.0, 12.0), (7.0, 20.0)])],
    Stop => [round_fill(6.0, 6.0, 12.0, 12.0, 1.5)],
    Pause => [round_fill(6.0, 5.0, 4.0, 14.0, 1.0), round_fill(14.0, 5.0, 4.0, 14.0, 1.0)],
    Step => [fill(&[(6.0, 4.0), (16.0, 12.0), (6.0, 20.0)]), rect(17.5, 4.0, 2.5, 16.0)],
    Check => [line(&[(4.0, 12.5), (9.5, 18.0), (20.0, 6.0)])],
    Evaluate => [
        fill(&[(12.0, 3.0), (21.0, 8.0), (12.0, 13.0), (3.0, 8.0)]),
        line(&[(6.0, 10.5), (6.0, 15.0), (12.0, 18.5), (18.0, 15.0), (18.0, 10.5)]),
        line(&[(20.0, 8.5), (20.0, 16.0)]),
    ],
    Breakpoint => [circle(12.0, 12.0, 6.0), circle_line(12.0, 12.0, 9.0)],
    Save => [
        round_line(3.5, 3.5, 17.0, 17.0, 2.0),
        rect(7.0, 14.0, 10.0, 6.5),
        rect_line(7.0, 3.5, 8.0, 6.0),
    ],
    SaveAs => [
        round_line(3.0, 3.5, 14.0, 14.0, 2.0),
        rect(6.0, 12.0, 8.0, 5.5),
        line(&[(16.0, 20.0), (21.0, 15.0)]),
        line(&[(17.0, 20.5), (21.5, 20.5), (21.5, 16.0)]),
    ],
    Open => [
        line(&[(3.0, 19.0), (6.0, 10.0), (22.0, 10.0), (19.0, 19.0), (3.0, 19.0)]),
        line(&[(5.0, 10.0), (5.0, 5.0), (11.0, 5.0), (13.0, 7.5), (18.0, 7.5)]),
    ],
    Search => [circle_line(10.5, 10.5, 6.5), line(&[(15.5, 15.5), (21.0, 21.0)])],
    Replace => [
        rect_line(3.0, 3.0, 12.0, 9.0),
        rect_line(9.0, 12.0, 12.0, 9.0),
        line(&[(17.0, 5.0), (21.0, 5.0), (21.0, 12.0)]),
        line(&[(18.0, 8.0), (21.0, 5.0)]),
    ],
    Undo => [line(&[(8.0, 6.0), (4.0, 10.0), (8.0, 14.0)]), line(&[(4.0, 10.0), (14.0, 10.0), (19.0, 14.0), (18.0, 19.0)])],
    Redo => [line(&[(16.0, 6.0), (20.0, 10.0), (16.0, 14.0)]), line(&[(20.0, 10.0), (10.0, 10.0), (5.0, 14.0), (6.0, 19.0)])],
    ZoomIn => [circle_line(10.5, 10.5, 6.5), line(&[(15.5, 15.5), (21.0, 21.0)]), line(&[(10.5, 7.5), (10.5, 13.5)]), line(&[(7.5, 10.5), (13.5, 10.5)])],
    ZoomOut => [circle_line(10.5, 10.5, 6.5), line(&[(15.5, 15.5), (21.0, 21.0)]), line(&[(7.5, 10.5), (13.5, 10.5)])],
    Fit => [
        line(&[(4.0, 9.0), (4.0, 4.0), (9.0, 4.0)]),
        line(&[(15.0, 4.0), (20.0, 4.0), (20.0, 9.0)]),
        line(&[(20.0, 15.0), (20.0, 20.0), (15.0, 20.0)]),
        line(&[(9.0, 20.0), (4.0, 20.0), (4.0, 15.0)]),
    ],
    Pen => [fill(&[(4.0, 20.0), (6.0, 14.0), (17.0, 3.0), (21.0, 7.0), (10.0, 18.0)]), line(&[(6.0, 14.0), (10.0, 18.0)])],
    Settings => [
        circle_line(12.0, 12.0, 3.5),
        circle_line(12.0, 12.0, 8.5),
        line(&[(12.0, 1.5), (12.0, 4.5)]), line(&[(12.0, 19.5), (12.0, 22.5)]),
        line(&[(1.5, 12.0), (4.5, 12.0)]), line(&[(19.5, 12.0), (22.5, 12.0)]),
    ],
    Help => [
        line(&[(4.0, 18.0), (4.0, 4.0), (20.0, 4.0), (20.0, 18.0), (13.0, 18.0), (10.0, 21.0), (10.0, 18.0)]),
        line(&[(9.0, 9.0), (11.0, 8.0), (14.0, 8.5), (14.5, 11.0), (11.0, 12.5), (11.0, 14.0)]),
        circle(11.0, 16.0, 0.8),
    ],
    Info => [circle_line(12.0, 12.0, 9.0), circle(12.0, 7.5, 1.2), line(&[(12.0, 11.0), (12.0, 17.0)])],
    About => [circle_line(12.0, 12.0, 9.0), line(&[(9.0, 16.0), (15.0, 16.0)]), line(&[(12.0, 5.5), (12.0, 6.5)])],
    Cut => [
        line(&[(6.0, 4.0), (17.0, 18.0)]),
        line(&[(18.0, 4.0), (7.0, 18.0)]),
        circle_line(5.0, 19.5, 2.0),
        circle_line(19.0, 19.5, 2.0),
    ],
    Copy => [rect_line(4.0, 4.0, 11.0, 13.0), rect_line(9.0, 8.0, 11.0, 13.0)],
    Paste => [rect_line(5.0, 5.0, 14.0, 16.0), rect_line(8.5, 3.0, 7.0, 4.5), line(&[(8.0, 11.0), (16.0, 11.0)]), line(&[(8.0, 15.0), (13.0, 15.0)])],
    SelectAll => [rect_line(3.5, 3.5, 17.0, 17.0), line(&[(8.0, 12.0), (11.0, 15.0), (17.0, 7.0)])],
    Sparkles => [
        line(&[(8.0, 3.0), (8.0, 11.0)]), line(&[(4.0, 7.0), (12.0, 7.0)]),
        line(&[(17.0, 12.0), (17.0, 20.0)]), line(&[(13.0, 16.0), (21.0, 16.0)]),
        line(&[(17.0, 3.0), (17.0, 6.0)]), line(&[(15.5, 4.5), (18.5, 4.5)]),
    ],
    Code => [line(&[(8.0, 6.0), (2.0, 12.0), (8.0, 18.0)]), line(&[(16.0, 6.0), (22.0, 12.0), (16.0, 18.0)]), line(&[(14.0, 4.0), (10.0, 20.0)])],
    Format => [line(&[(4.0, 5.0), (20.0, 5.0)]), line(&[(8.0, 10.0), (20.0, 10.0)]), line(&[(8.0, 15.0), (20.0, 15.0)]), line(&[(8.0, 20.0), (16.0, 20.0)])],
    Comment => [line(&[(4.0, 5.0), (20.0, 5.0), (20.0, 16.0), (12.0, 16.0), (8.0, 20.0), (8.0, 16.0), (4.0, 16.0), (4.0, 5.0)]), line(&[(8.0, 10.0), (16.0, 10.0)])],
    Definition => [rect_line(3.0, 3.0, 11.0, 11.0), line(&[(14.0, 12.0), (21.0, 12.0)]), line(&[(17.0, 8.0), (21.0, 12.0), (17.0, 16.0)])],
    Warning => [line(&[(12.0, 3.0), (22.0, 20.0), (2.0, 20.0), (12.0, 3.0)]), line(&[(12.0, 9.0), (12.0, 14.0)]), circle(12.0, 17.0, 0.9)],
    Error => [circle_line(12.0, 12.0, 9.0), line(&[(8.5, 8.5), (15.5, 15.5)]), line(&[(15.5, 8.5), (8.5, 15.5)])],
    Trash => [line(&[(4.0, 7.0), (20.0, 7.0)]), line(&[(9.0, 4.0), (15.0, 4.0)]), line(&[(6.0, 7.0), (7.0, 20.0), (17.0, 20.0), (18.0, 7.0)]), line(&[(10.0, 11.0), (10.0, 17.0)]), line(&[(14.0, 11.0), (14.0, 17.0)])],
    Keyboard => [round_line(2.5, 6.0, 19.0, 12.0, 2.0), line(&[(7.0, 10.0), (17.0, 10.0)]), line(&[(7.0, 14.0), (13.0, 14.0)])],
    Palette => [
        line(&[(12.0, 3.0), (20.0, 7.0), (20.0, 16.0), (12.0, 21.0), (4.0, 16.0), (4.0, 7.0), (12.0, 3.0)]),
        circle(12.0, 12.0, 3.0),
    ],
    Brush => [line(&[(4.0, 20.0), (8.0, 16.0)]), fill(&[(9.0, 15.0), (14.0, 3.0), (21.0, 8.0), (11.0, 17.0)])],
    Wall => [
        rect_line(3.0, 4.0, 18.0, 16.0),
        line(&[(3.0, 9.5), (21.0, 9.5)]),
        line(&[(3.0, 15.0), (21.0, 15.0)]),
        line(&[(9.0, 4.0), (9.0, 9.5)]), line(&[(15.0, 4.0), (15.0, 9.5)]),
        line(&[(6.0, 9.5), (6.0, 15.0)]), line(&[(12.0, 9.5), (12.0, 15.0)]), line(&[(18.0, 9.5), (18.0, 15.0)]),
        line(&[(9.0, 15.0), (9.0, 20.0)]), line(&[(15.0, 15.0), (15.0, 20.0)]),
    ],
    Mark => [circle_line(12.0, 9.0, 5.0), line(&[(12.0, 14.0), (12.0, 21.0)]), circle(12.0, 9.0, 1.6)],
    Hand => [line(&[(6.0, 14.0), (6.0, 9.0), (8.0, 8.0), (9.0, 9.0)]), line(&[(9.0, 9.0), (9.0, 5.0), (11.0, 4.5), (12.0, 6.0)]), line(&[(12.0, 6.0), (12.0, 4.0), (14.0, 4.0), (15.0, 6.0)]), line(&[(15.0, 6.0), (15.0, 6.0), (16.0, 6.0), (17.0, 8.0)]), line(&[(17.0, 8.0), (18.0, 9.0), (18.0, 14.0), (14.0, 20.0), (8.0, 20.0), (6.0, 14.0)])],
    Robot => [
        round_line(4.0, 7.0, 16.0, 13.0, 3.0),
        line(&[(12.0, 7.0), (12.0, 4.0)]),
        circle(12.0, 3.0, 1.2),
        circle(9.0, 12.0, 1.4),
        circle(15.0, 12.0, 1.4),
        line(&[(9.0, 16.0), (15.0, 16.0)]),
    ],
    Turtle => [
        fill(&[(12.0, 13.0), (20.0, 8.0), (20.0, 18.0), (12.0, 13.0)]),
        circle(10.0, 13.0, 4.5),
        line(&[(10.0, 8.5), (10.0, 5.0)]),
        circle(10.0, 4.0, 1.5),
        line(&[(6.0, 19.0), (4.0, 21.0)]),
        line(&[(14.0, 19.0), (16.0, 21.0)]),
    ],
    Draw => [
        line(&[(3.0, 20.0), (21.0, 20.0)]),
        line(&[(3.0, 20.0), (3.0, 3.0)]),
        line(&[(7.0, 17.0), (17.0, 8.0)]),
        fill(&[(17.0, 8.0), (19.0, 3.0), (21.0, 5.0)]),
        line(&[(7.0, 17.0), (12.0, 17.5)]),
    ],
    Vodoley => [
        line(&[(4.0, 4.0), (10.0, 4.0), (10.0, 20.0), (4.0, 20.0), (4.0, 4.0)]),
        line(&[(14.0, 4.0), (20.0, 4.0), (20.0, 20.0), (14.0, 20.0), (14.0, 4.0)]),
        line(&[(4.0, 13.0), (10.0, 13.0)]),
        line(&[(14.0, 9.0), (20.0, 9.0)]),
    ],
    Grasshopper => [
        line(&[(3.0, 20.0), (21.0, 20.0)]),
        line(&[(7.0, 20.0), (7.0, 17.0)]), line(&[(12.0, 20.0), (12.0, 17.0)]), line(&[(17.0, 20.0), (17.0, 17.0)]),
        fill(&[(12.0, 9.0), (17.0, 14.0), (12.0, 19.0), (7.0, 14.0)]),
    ],
    Painter => [
        round_line(3.0, 3.0, 18.0, 15.0, 2.0),
        line(&[(3.0, 13.0), (9.0, 13.0), (12.0, 9.0), (16.0, 13.0), (21.0, 13.0)]),
        fill(&[(7.0, 7.0), (10.0, 7.0), (10.0, 10.0), (7.0, 10.0)]),
        line(&[(14.0, 7.0), (17.0, 7.0)]),
    ],
    Grid => [
        line(&[(3.0, 3.0), (21.0, 3.0), (21.0, 21.0), (3.0, 21.0), (3.0, 3.0)]),
        line(&[(9.0, 3.0), (9.0, 21.0)]), line(&[(15.0, 3.0), (15.0, 21.0)]),
        line(&[(3.0, 9.0), (21.0, 9.0)]), line(&[(3.0, 15.0), (21.0, 15.0)]),
    ],
    Download => [line(&[(12.0, 3.0), (12.0, 15.0)]), line(&[(7.0, 10.0), (12.0, 15.0), (17.0, 10.0)]), line(&[(4.0, 17.0), (4.0, 21.0), (20.0, 21.0), (20.0, 17.0)])],
    Upload => [line(&[(12.0, 16.0), (12.0, 4.0)]), line(&[(7.0, 9.0), (12.0, 4.0), (17.0, 9.0)]), line(&[(4.0, 17.0), (4.0, 21.0), (20.0, 21.0), (20.0, 17.0)])],
    Book => [
        line(&[(3.0, 5.0), (10.0, 3.5), (10.0, 19.0), (3.0, 20.0), (3.0, 5.0)]),
        line(&[(21.0, 5.0), (14.0, 3.5), (14.0, 19.0), (21.0, 20.0), (21.0, 5.0)]),
    ],
    Star => [line(&[
        (12.0, 3.0),
        (14.2, 9.2),
        (20.8, 9.5),
        (15.7, 13.5),
        (17.5, 19.9),
        (12.0, 16.2),
        (6.5, 19.9),
        (8.3, 13.5),
        (3.2, 9.5),
        (9.8, 9.2),
        (12.0, 3.0),
    ])],
}

/// Draw the icon inside `rect` (a square is recommended) with the given colour.
pub fn draw(painter: &Painter, icon: Icon, rect: Rect, color: Color32) {
    let scale = rect.width().min(rect.height()) / 24.0;
    if scale <= 0.0 {
        return;
    }
    let origin = rect.center() - Vec2::splat(12.0 * scale);
    let map = |(x, y): (f32, f32)| Pos2::new(origin.x + x * scale, origin.y + y * scale);
    let stroke = Stroke::new((1.9 * scale).max(1.0), color);
    for prim in prims(icon) {
        match prim {
            Prim::Fill(points) => {
                let pts: Vec<Pos2> = points.iter().map(|p| map(*p)).collect();
                if pts.len() >= 3 {
                    painter.add(egui::Shape::convex_polygon(pts, color, Stroke::NONE));
                }
            }
            Prim::Line(points) => {
                let pts: Vec<Pos2> = points.iter().map(|p| map(*p)).collect();
                if pts.len() >= 2 {
                    painter.add(egui::Shape::line(pts, stroke));
                }
            }
            Prim::Circle(cx, cy, r) => {
                painter.add(egui::Shape::Circle(CircleShape {
                    center: map((*cx, *cy)),
                    radius: r * scale,
                    fill: color,
                    stroke: Stroke::NONE,
                }));
            }
            Prim::CircleLine(cx, cy, r) => {
                painter.add(egui::Shape::Circle(CircleShape {
                    center: map((*cx, *cy)),
                    radius: r * scale,
                    fill: Color32::TRANSPARENT,
                    stroke,
                }));
            }
            Prim::Rect(x, y, w, h) => {
                painter.rect_filled(
                    Rect::from_min_size(map((*x, *y)), Vec2::new(w * scale, h * scale)),
                    CornerRadius::ZERO,
                    color,
                );
            }
            Prim::RectLine(x, y, w, h) => {
                painter.rect_stroke(
                    Rect::from_min_size(map((*x, *y)), Vec2::new(w * scale, h * scale)),
                    CornerRadius::ZERO,
                    stroke,
                    StrokeKind::Middle,
                );
            }
            Prim::RoundLine(x, y, w, h, r) => {
                painter.rect_stroke(
                    Rect::from_min_size(map((*x, *y)), Vec2::new(w * scale, h * scale)),
                    CornerRadius::same((r * scale) as u8),
                    stroke,
                    StrokeKind::Middle,
                );
            }
            Prim::RoundFill(x, y, w, h, r) => {
                painter.rect_filled(
                    Rect::from_min_size(map((*x, *y)), Vec2::new(w * scale, h * scale)),
                    CornerRadius::same((r * scale) as u8),
                    color,
                );
            }
        }
    }
}

/// A square icon button with a consistent look.
pub fn button(
    ui: &mut egui::Ui,
    icon: Icon,
    selected: bool,
    enabled: bool,
    size: f32,
    tooltip: &str,
) -> egui::Response {
    let enabled = enabled && ui.is_enabled();
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), sense);
    let response = response.on_hover_text(tooltip);
    let visuals = ui.style().interact_selectable(&response, selected);
    let fill = if selected {
        ui.visuals().selection.bg_fill
    } else if response.hovered() && enabled {
        visuals.bg_fill
    } else {
        Color32::TRANSPARENT
    };
    if fill != Color32::TRANSPARENT {
        ui.painter().rect_filled(rect, CornerRadius::same(5), fill);
    }
    let color = if !enabled {
        ui.visuals().weak_text_color()
    } else if selected {
        ui.visuals().selection.stroke.color
    } else {
        ui.visuals().text_color()
    };
    draw(
        ui.painter(),
        icon,
        Rect::from_center_size(rect.center(), Vec2::splat(size * 0.62)),
        color,
    );
    if enabled {
        response
    } else {
        response.on_hover_text("Недоступно")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_icons_have_primitives() {
        let icons = [
            Icon::Folder,
            Icon::FolderUp,
            Icon::FolderOpen,
            Icon::File,
            Icon::FileKod,
            Icon::FileFil,
            Icon::Recent,
            Icon::Refresh,
            Icon::Plus,
            Icon::Close,
            Icon::ChevronLeft,
            Icon::ChevronRight,
            Icon::ChevronDown,
            Icon::ArrowLeft,
            Icon::ArrowRight,
            Icon::ArrowUp,
            Icon::ArrowDown,
            Icon::Run,
            Icon::Stop,
            Icon::Pause,
            Icon::Step,
            Icon::Check,
            Icon::Evaluate,
            Icon::Breakpoint,
            Icon::Save,
            Icon::SaveAs,
            Icon::Open,
            Icon::Search,
            Icon::Replace,
            Icon::Undo,
            Icon::Redo,
            Icon::ZoomIn,
            Icon::ZoomOut,
            Icon::Fit,
            Icon::Pen,
            Icon::Settings,
            Icon::Help,
            Icon::Info,
            Icon::About,
            Icon::Cut,
            Icon::Copy,
            Icon::Paste,
            Icon::SelectAll,
            Icon::Sparkles,
            Icon::Code,
            Icon::Format,
            Icon::Comment,
            Icon::Definition,
            Icon::Warning,
            Icon::Error,
            Icon::Trash,
            Icon::Keyboard,
            Icon::Palette,
            Icon::Brush,
            Icon::Wall,
            Icon::Mark,
            Icon::Hand,
            Icon::Robot,
            Icon::Turtle,
            Icon::Draw,
            Icon::Vodoley,
            Icon::Grasshopper,
            Icon::Painter,
            Icon::Grid,
            Icon::Download,
            Icon::Upload,
            Icon::Book,
            Icon::Star,
        ];
        for icon in icons {
            assert!(!prims(icon).is_empty(), "{icon:?} is empty");
        }
    }

    #[test]
    fn drawing_does_not_panic() {
        egui::__run_test_ui(|ui| {
            ui.set_min_size(egui::vec2(100.0, 100.0));
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(64.0), egui::Sense::hover());
            for icon in [Icon::Run, Icon::Robot, Icon::Painter] {
                draw(ui.painter(), icon, rect, Color32::WHITE);
            }
        });
    }
}
