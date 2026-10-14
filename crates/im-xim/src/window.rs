//! The candidate window.
//!
//! XIM has the *application* draw the preedit, and most clients ask for
//! `XIMPreeditNothing` — which means the input method is expected to draw the
//! composition itself, at the spot location the client reports. `st`, `xterm`
//! and GTK 3 all do this, so a daemon that only sends preedit strings is
//! invisible in them.
//!
//! So this is a small override-redirect window, drawn entirely by `im-xim`:
//!
//! ```text
//! ┌──────────────┐
//! │ vsgo         │  what was typed
//! │ 1 中国       │
//! │ 2 中         │  highlighted
//! │ 3 中国话     │
//! │ 1/2          │  only when there is more than one page
//! └──────────────┘
//! ```
//!
//! Rendering is `fontdue` to rasterise glyphs and `fontdb` to find a font, both
//! pure Rust, so the only thing this needs from the system is a font file with
//! Chinese coverage. Pixels reach the server in a single `PutImage` of a
//! depth-24 ZPixmap, which is the entire drawing API used here: no Render
//! extension, no cairo, no toolkit.

use anyhow::{Context as _, Result, bail};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    AtomEnum, ConfigureWindowAux, ConnectionExt as _, CreateWindowAux, EventMask,
    Gcontext, ImageFormat, PropMode, Screen, Visualtype, Window as XWindow,
    WindowClass,
};
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

/// Font size in pixels.
const FONT_PX: f32 = 18.0;
/// Space between the window edge and the text.
const PADDING: i32 = 6;
/// Horizontal space between a number and its word.
const COLUMN_GAP: i32 = 8;
/// Vertical space between lines.
const LINE_GAP: i32 = 3;
/// Distance between the caret and the top of the window.
const SPOT_OFFSET: i16 = 4;
/// Depth and bits per pixel of the pixmap format `PutImage` is given.
const DEPTH: u8 = 24;
const BYTES_PER_PIXEL: usize = 4;

/// The palette. A window has to pick colours somehow, and these read on a light
/// or a dark desktop without disappearing into either.
const BACKGROUND: Rgb = Rgb(0x1c, 0x1c, 0x1c);
const FOREGROUND: Rgb = Rgb(0xe6, 0xe6, 0xe6);
const DIM: Rgb = Rgb(0x8c, 0x8c, 0x8c);
const HIGHLIGHT: Rgb = Rgb(0x2d, 0x5a, 0x88);

/// A colour, as one would write it in HTML.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

/// One line of the window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// Number shown to the left of the word.
    pub number: usize,
    /// The word.
    pub text: String,
    /// Whether a selection would take this one.
    pub selected: bool,
}

/// What the window should show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Content {
    /// The keys typed so far.
    pub input: String,
    /// Candidates to list.
    pub items: Vec<Item>,
    /// Current page and page count; shown only when there is more than one.
    pub page: (usize, usize),
}

impl Content {
    /// True when there is nothing worth showing.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// The window itself.
pub struct CandidateWindow<'a> {
    connection: &'a RustConnection,
    root: XWindow,
    screen_size: (u16, u16),
    window: XWindow,
    gc: Gcontext,
    text: Text,
    format: PixelFormat,
    canvas: Canvas,
    mapped: bool,
}

impl<'a> CandidateWindow<'a> {
    /// Create the window, or explain why it cannot be drawn.
    ///
    /// A caller is expected to carry on without one: typing still works, it
    /// just gives no visual feedback.
    pub fn new(connection: &'a RustConnection, screen_number: usize) -> Result<Self> {
        let screen = connection
            .setup()
            .roots
            .get(screen_number)
            .context("no such screen")?
            .clone();
        let format = PixelFormat::from_screen(&screen)
            .context("the screen's visual is not a true colour visual")?;
        let text = Text::load(FONT_PX)?;

        let window = connection.generate_id().context("allocating a window id")?;
        connection
            .create_window(
                x11rb::COPY_DEPTH_FROM_PARENT,
                window,
                screen.root,
                0,
                0,
                1,
                1,
                0,
                WindowClass::INPUT_OUTPUT,
                screen.root_visual,
                &CreateWindowAux::new()
                    // The window manager must leave this alone: it is placed from
                    // the caret's position, not by a user.
                    .override_redirect(1)
                    .background_pixel(format.pixel(BACKGROUND))
                    .event_mask(EventMask::EXPOSURE),
            )
            .context("creating the candidate window")?
            .check()
            .context("creating the candidate window")?;
        let gc = connection.generate_id().context("allocating a gc id")?;
        connection
            .create_gc(gc, window, &Default::default())
            .context("creating a graphics context")?
            .check()
            .context("creating a graphics context")?;
        set_window_type(connection, window)?;

        Ok(Self {
            connection,
            root: screen.root,
            screen_size: (screen.width_in_pixels, screen.height_in_pixels),
            window,
            gc,
            text,
            format,
            canvas: Canvas::new(1, 1, format),
            mapped: false,
        })
    }

    /// The window handle, so an event loop can recognise its own events.
    pub fn id(&self) -> XWindow {
        self.window
    }

    /// Draw `content` and place the window near the caret.
    ///
    /// `spot` is the caret position reported by the client, in the client
    /// window's own coordinates.
    pub fn show(
        &mut self,
        content: &Content,
        client: XWindow,
        spot: (i16, i16),
    ) -> Result<()> {
        if content.is_empty() {
            return self.hide();
        }
        let layout = self.layout(content);
        self.canvas = Canvas::new(layout.width, layout.height, self.format);
        self.draw(&layout);

        let size = (layout.width as u16, layout.height as u16);
        let (x, y) = self.place(spot, client, size);
        let aux = ConfigureWindowAux::new()
            .x(i32::from(x))
            .y(i32::from(y))
            .width(u32::from(size.0))
            .height(u32::from(size.1));
        self.connection
            .configure_window(self.window, &aux)
            .context("placing the candidate window")?
            .check()
            .context("placing the candidate window")?;

        self.put_image()?;
        if !self.mapped {
            self.connection
                .map_window(self.window)
                .context("showing the candidate window")?
                .check()
                .context("showing the candidate window")?;
            self.mapped = true;
        }
        self.connection.flush().context("flushing X requests")?;
        Ok(())
    }

    /// Repaint after the server asked for an expose.
    pub fn repaint(&mut self) -> Result<()> {
        if !self.mapped {
            return Ok(());
        }
        self.put_image()?;
        self.connection.flush().context("flushing X requests")?;
        Ok(())
    }

    /// Take the window off the screen.
    pub fn hide(&mut self) -> Result<()> {
        if !self.mapped {
            return Ok(());
        }
        self.mapped = false;
        self.connection
            .unmap_window(self.window)
            .context("hiding the candidate window")?
            .check()
            .context("hiding the candidate window")?;
        self.connection.flush().context("flushing X requests")?;
        Ok(())
    }

    /// Measure the window and place every line in it.
    fn layout(&self, content: &Content) -> Layout {
        let line_height = self.text.line_height();
        let mut width = self.text.width(&content.input);
        for item in &content.items {
            let row = self.text.width(&item.number.to_string())
                + COLUMN_GAP
                + self.text.width(&item.text);
            width = width.max(row);
        }

        let mut lines = Vec::with_capacity(content.items.len() + 2);
        let mut y = PADDING + self.text.ascent;
        if !content.input.is_empty() {
            lines.push(Line::Input {
                y,
                text: content.input.clone(),
            });
            y += line_height + LINE_GAP;
        }
        for item in &content.items {
            lines.push(Line::Item {
                y,
                number: item.number.to_string(),
                text: item.text.clone(),
                selected: item.selected,
            });
            y += line_height + LINE_GAP;
        }
        if content.page.1 > 1 {
            let text = format!("{}/{}", content.page.0, content.page.1);
            lines.push(Line::Page { y, text });
            y += line_height + LINE_GAP;
        }

        Layout {
            width: width + PADDING * 2,
            height: y - LINE_GAP + PADDING,
            lines,
        }
    }

    /// Put the window where the text is, without letting it fall off the screen.
    fn place(&self, spot: (i16, i16), client: XWindow, size: (u16, u16)) -> (i16, i16) {
        let (dx, dy) = self
            .connection
            .translate_coordinates(client, self.root, spot.0, spot.1)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .map(|reply| (reply.dst_x, reply.dst_y))
            .unwrap_or(spot);

        let height = i16::try_from(size.1).unwrap_or(i16::MAX);
        let mut y = dy.saturating_add(SPOT_OFFSET);
        // If it would run off the bottom, put it above the caret instead.
        if i32::from(y) + i32::from(size.1) > i32::from(self.screen_size.1) {
            y = dy.saturating_sub(height.saturating_add(SPOT_OFFSET));
        }
        let width = i16::try_from(size.0).unwrap_or(i16::MAX);
        let max_x = i16::try_from(self.screen_size.0)
            .unwrap_or(i16::MAX)
            .saturating_sub(width);
        (dx.clamp(0, max_x.max(0)), y.max(0))
    }

    fn draw(&mut self, layout: &Layout) {
        self.canvas.fill(BACKGROUND);
        for line in &layout.lines {
            match line {
                Line::Input { y, text } => {
                    self.text.draw(&mut self.canvas, PADDING, *y, text, DIM);
                }
                Line::Item {
                    y,
                    number,
                    text,
                    selected,
                } => {
                    if *selected {
                        self.canvas.fill_rect(
                            0,
                            y - self.text.ascent - LINE_GAP / 2,
                            layout.width,
                            self.text.line_height() + LINE_GAP,
                            HIGHLIGHT,
                        );
                    }
                    self.text.draw(&mut self.canvas, PADDING, *y, number, DIM);
                    let x = PADDING + self.text.width(number) + COLUMN_GAP;
                    self.text.draw(&mut self.canvas, x, *y, text, FOREGROUND);
                }
                Line::Page { y, text } => {
                    self.text.draw(&mut self.canvas, PADDING, *y, text, DIM);
                }
            }
        }
    }

    fn put_image(&mut self) -> Result<()> {
        let connection = self.connection;
        connection
            .put_image(
                ImageFormat::Z_PIXMAP,
                self.window,
                self.gc,
                self.canvas.width() as u16,
                self.canvas.height() as u16,
                0,
                0,
                0,
                DEPTH,
                self.canvas.bytes(),
            )
            .context("sending the candidate window pixels")?
            .check()
            .context("sending the candidate window pixels")?;
        Ok(())
    }
}

/// A line of the window, with its text baseline already worked out.
enum Line {
    Input {
        y: i32,
        text: String,
    },
    Item {
        y: i32,
        number: String,
        text: String,
        selected: bool,
    },
    Page {
        y: i32,
        text: String,
    },
}

struct Layout {
    width: i32,
    height: i32,
    lines: Vec<Line>,
}

/// Tell the window manager this is a popup, not a window to focus or decorate.
fn set_window_type(connection: &RustConnection, window: XWindow) -> Result<()> {
    let atom = |name: &[u8]| -> Result<u32> {
        Ok(connection
            .intern_atom(false, name)
            .context("interning an atom")?
            .reply()
            .context("interning an atom")?
            .atom)
    };
    let kind = atom(b"_NET_WM_WINDOW_TYPE")?;
    let popup = atom(b"_NET_WM_WINDOW_TYPE_POPUP_MENU")?;
    connection
        .change_property32(PropMode::REPLACE, window, kind, AtomEnum::ATOM, &[popup])
        .context("setting the window type")?
        .check()
        .context("setting the window type")?;
    Ok(())
}

/// How a colour becomes a pixel value on this screen.
#[derive(Debug, Clone, Copy)]
struct PixelFormat {
    red_shift: u32,
    green_shift: u32,
    blue_shift: u32,
}

impl PixelFormat {
    fn from_screen(screen: &Screen) -> Option<Self> {
        let visual = visual_for(screen, screen.root_visual)?;
        if visual.red_mask == 0 || visual.green_mask == 0 || visual.blue_mask == 0 {
            return None;
        }
        Some(Self {
            red_shift: visual.red_mask.trailing_zeros(),
            green_shift: visual.green_mask.trailing_zeros(),
            blue_shift: visual.blue_mask.trailing_zeros(),
        })
    }

    fn pixel(self, Rgb(r, g, b): Rgb) -> u32 {
        (u32::from(r) << self.red_shift)
            | (u32::from(g) << self.green_shift)
            | (u32::from(b) << self.blue_shift)
    }
}

fn visual_for(screen: &Screen, id: u32) -> Option<&Visualtype> {
    screen
        .allowed_depths
        .iter()
        .flat_map(|depth| &depth.visuals)
        .find(|visual| visual.visual_id == id)
}

/// The pixels of one drawing, in the screen's own format.
struct Canvas {
    width: i32,
    height: i32,
    format: PixelFormat,
    /// Four bytes per pixel, native order, ready for `PutImage`.
    pixels: Vec<u8>,
}

impl Canvas {
    fn new(width: i32, height: i32, format: PixelFormat) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        Self {
            width,
            height,
            format,
            pixels: vec![0; (width * height) as usize * BYTES_PER_PIXEL],
        }
    }

    fn width(&self) -> i32 {
        self.width
    }

    fn height(&self) -> i32 {
        self.height
    }

    fn bytes(&self) -> &[u8] {
        &self.pixels
    }

    fn get(&self, index: usize) -> u32 {
        let at = index * BYTES_PER_PIXEL;
        let bytes = &self.pixels[at..at + BYTES_PER_PIXEL];
        u32::from_ne_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
    }

    fn set(&mut self, index: usize, value: u32) {
        let at = index * BYTES_PER_PIXEL;
        self.pixels[at..at + BYTES_PER_PIXEL].copy_from_slice(&value.to_ne_bytes());
    }

    fn fill(&mut self, colour: Rgb) {
        let pixel = self.format.pixel(colour);
        for index in 0..(self.width * self.height) as usize {
            self.set(index, pixel);
        }
    }

    fn fill_rect(&mut self, x: i32, y: i32, width: i32, height: i32, colour: Rgb) {
        let pixel = self.format.pixel(colour);
        for row in y.max(0)..(y + height).min(self.height) {
            for column in x.max(0)..(x + width).min(self.width) {
                self.set((row * self.width + column) as usize, pixel);
            }
        }
    }

    /// Blend one glyph pixel over whatever is already there.
    fn blend(&mut self, x: i32, y: i32, coverage: u8, colour: Rgb) {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return;
        }
        let index = (y * self.width + x) as usize;
        let existing = self.get(index);
        let format = self.format;
        let mix = |foreground: u8, background: u32| -> u32 {
            let alpha = u32::from(coverage);
            (u32::from(foreground) * alpha + background * (255 - alpha)) / 255
        };
        let Rgb(r, g, b) = colour;
        let pixel = (mix(r, (existing >> format.red_shift) & 0xff) << format.red_shift)
            | (mix(g, (existing >> format.green_shift) & 0xff) << format.green_shift)
            | (mix(b, (existing >> format.blue_shift) & 0xff) << format.blue_shift);
        self.set(index, pixel);
    }
}

/// The font, and the drawing that goes with it.
struct Text {
    font: fontdue::Font,
    px: f32,
    line_height: i32,
    ascent: i32,
}

impl Text {
    fn load(px: f32) -> Result<Self> {
        let font = load_font()?;
        let metrics =
            font.horizontal_line_metrics(px)
                .unwrap_or(fontdue::LineMetrics {
                    ascent: px,
                    descent: -px / 4.0,
                    line_gap: 0.0,
                    new_line_size: px,
                });
        Ok(Self {
            font,
            px,
            line_height: metrics.new_line_size.ceil() as i32,
            ascent: metrics.ascent.ceil() as i32,
        })
    }

    fn line_height(&self) -> i32 {
        self.line_height
    }

    fn width(&self, text: &str) -> i32 {
        text.chars()
            .map(|ch| self.font.metrics(ch, self.px).advance_width)
            .sum::<f32>()
            .ceil() as i32
    }

    /// Draw `text` with its left edge at `x` and its baseline at `baseline`.
    fn draw(
        &self,
        canvas: &mut Canvas,
        x: i32,
        baseline: i32,
        text: &str,
        colour: Rgb,
    ) {
        let mut pen = x;
        for ch in text.chars() {
            let (metrics, bitmap) = self.font.rasterize(ch, self.px);
            if metrics.width > 0 && metrics.height > 0 {
                let left = pen + metrics.xmin;
                let top = baseline - metrics.ymin - metrics.height as i32;
                for row in 0..metrics.height {
                    for column in 0..metrics.width {
                        let coverage = bitmap[row * metrics.width + column];
                        if coverage > 0 {
                            canvas.blend(
                                left + column as i32,
                                top + row as i32,
                                coverage,
                                colour,
                            );
                        }
                    }
                }
            }
            pen += metrics.advance_width.ceil() as i32;
        }
    }
}

/// Find a font that can draw Chinese.
fn load_font() -> Result<fontdue::Font> {
    let mut database = fontdb::Database::new();
    database.load_system_fonts();

    // Named fonts first: one chosen on purpose looks better than one that
    // merely happens to have the glyphs.
    const PREFERRED: &[&str] = &[
        "Noto Sans CJK SC",
        "Noto Sans CJK JP",
        "Noto Sans CJK TC",
        "Source Han Sans SC",
        "WenQuanYi Zen Hei",
        "WenQuanYi Micro Hei",
        "Droid Sans Fallback",
        "Noto Sans SC",
        "PingFang SC",
    ];
    for family in PREFERRED {
        let query = fontdb::Query {
            families: &[fontdb::Family::Name(family)],
            ..Default::default()
        };
        if let Some(font) = database.query(&query).and_then(|id| load(&database, id)) {
            log::debug!("candidate window font: {family}");
            return Ok(font);
        }
    }

    // Otherwise take the first font that can draw what we need.
    for face in database.faces() {
        let id = face.id;
        if let Some(font) = load(&database, id)
            && font.has_glyph('中')
            && font.has_glyph('a')
        {
            log::debug!("candidate window font: {}", face.post_script_name);
            return Ok(font);
        }
    }
    bail!(
        "no font with Chinese coverage found; install one, for example \
         fonts-noto-cjk"
    )
}

fn load(database: &fontdb::Database, id: fontdb::ID) -> Option<fontdue::Font> {
    database.with_face_data(id, |data, index| {
        fontdue::Font::from_bytes(
            data,
            fontdue::FontSettings {
                collection_index: index,
                ..fontdue::FontSettings::default()
            },
        )
        .ok()
    })?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn format() -> PixelFormat {
        PixelFormat {
            red_shift: 16,
            green_shift: 8,
            blue_shift: 0,
        }
    }

    #[test]
    fn pixels_are_little_endian_bgrx_for_the_usual_truecolour_visual() {
        let canvas = Canvas::new(1, 1, format());
        assert_eq!(canvas.bytes().len(), 4);
        let mut canvas = canvas;
        canvas.fill(Rgb(0x11, 0x22, 0x33));
        // 0x00112233 written in native order.
        assert_eq!(canvas.bytes(), &[0x33, 0x22, 0x11, 0x00]);
    }

    #[test]
    fn pixel_shifts_follow_the_visual() {
        let format = PixelFormat {
            red_shift: 0,
            green_shift: 8,
            blue_shift: 16,
        };
        assert_eq!(format.pixel(Rgb(1, 2, 3)), 0x0003_0201);
    }

    #[test]
    fn fill_rect_clips_to_the_canvas() {
        let mut canvas = Canvas::new(4, 4, format());
        canvas.fill(Rgb(0, 0, 0));
        // The rectangle covers x and y in [-2, 2), so it lights up the top left
        // corner only.
        canvas.fill_rect(-2, -2, 4, 4, Rgb(0xff, 0xff, 0xff));
        assert_eq!(canvas.get(0), 0x00ff_ffff);
        assert_eq!(canvas.get(1), 0x00ff_ffff);
        assert_eq!(canvas.get(2), 0, "outside the rectangle");
        assert_eq!(canvas.get(4), 0x00ff_ffff);
        canvas.fill_rect(2, 2, 100, 100, Rgb(0xff, 0, 0));
        assert_eq!(canvas.get(2 + 2 * 4), 0x00ff_0000);
    }

    #[test]
    fn blending_a_full_coverage_pixel_replaces_it() {
        let mut canvas = Canvas::new(1, 1, format());
        canvas.fill(Rgb(0, 0, 0));
        canvas.blend(0, 0, 255, Rgb(0x10, 0x20, 0x30));
        assert_eq!(canvas.get(0), 0x0010_2030);
    }

    #[test]
    fn blending_a_zero_coverage_pixel_leaves_it_alone() {
        let mut canvas = Canvas::new(1, 1, format());
        canvas.fill(Rgb(0x10, 0x20, 0x30));
        canvas.blend(0, 0, 0, Rgb(0xff, 0xff, 0xff));
        assert_eq!(canvas.get(0), 0x0010_2030);
    }

    #[test]
    fn blending_outside_the_canvas_is_ignored() {
        let mut canvas = Canvas::new(2, 2, format());
        canvas.fill(Rgb(0, 0, 0));
        canvas.blend(-1, 0, 255, Rgb(0xff, 0xff, 0xff));
        canvas.blend(0, 2, 255, Rgb(0xff, 0xff, 0xff));
        canvas.blend(2, 0, 255, Rgb(0xff, 0xff, 0xff));
        assert_eq!(canvas.bytes(), &[0u8; 16]);
    }

    #[test]
    fn empty_content_is_not_worth_a_window() {
        let empty = Content {
            input: "vsgo".to_owned(),
            items: Vec::new(),
            page: (1, 1),
        };
        assert!(empty.is_empty());
    }
}
