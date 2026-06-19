//! Real PDF renderer for the Daily Close report.
//!
//! Earlier this wrote a plain-text file with a `.pdf` extension, which no PDF
//! viewer could open. This module produces an actual PDF (via `printpdf`)
//! laid out in the project's editorial visual language: a crimson eyebrow rule,
//! ink headings, tabular right-aligned money columns, and a dark "net" focal
//! block. Built-in Helvetica is used so we ship no font files; Latin-only by
//! design (Arabic glyphs would need an embedded font, tracked separately).
//!
//! Lives in infrastructure because it performs file I/O and depends on the
//! `printpdf` crate -- the domain layer stays free of both.

use std::io::BufWriter;
use std::path::Path;

use printpdf::path::{PaintMode, WindingOrder};
use printpdf::{
    BuiltinFont, Color, IndirectFontRef, Line, Mm, PdfDocument, PdfLayerReference, Point, Polygon,
    Rgb,
};

use crate::domains::reports::domain::entities::DailyClose;
use crate::error::{AppError, AppResult};

// ---- Page geometry (A4 portrait, in mm) -----------------------------------

const PAGE_W: f32 = 210.0;
const PAGE_H: f32 = 297.0;
const MARGIN_X: f32 = 18.0;
const CONTENT_W: f32 = PAGE_W - 2.0 * MARGIN_X;
const TOP_Y: f32 = PAGE_H - 20.0;
const BOTTOM_Y: f32 = 18.0;

// ---- Palette (mirrors .claude/rules/design-system.md tokens) --------------

fn ink() -> Color {
    Color::Rgb(Rgb::new(0.039, 0.071, 0.188, None)) // #0A1230
}
fn ink_3() -> Color {
    Color::Rgb(Rgb::new(0.369, 0.353, 0.306, None)) // #5E5A4E
}
fn crimson() -> Color {
    Color::Rgb(Rgb::new(0.753, 0.149, 0.227, None)) // #C0263A
}
fn paper_2() -> Color {
    Color::Rgb(Rgb::new(0.969, 0.961, 0.933, None)) // #F7F5EE
}
fn line_color() -> Color {
    Color::Rgb(Rgb::new(0.925, 0.910, 0.859, None)) // #ECE8DB
}
fn white() -> Color {
    Color::Rgb(Rgb::new(1.0, 1.0, 1.0, None))
}

/// Fonts loaded once and reused for the whole document.
struct Fonts {
    regular: IndirectFontRef,
    bold: IndirectFontRef,
    mono: IndirectFontRef,
}

/// A tiny stateful cursor over the current page/layer. Tracks the y position
/// so sections stack top-to-bottom and paginate when they run off the page.
struct Canvas<'a> {
    doc: &'a printpdf::PdfDocumentReference,
    layer: PdfLayerReference,
    fonts: &'a Fonts,
    y: f32,
}

impl<'a> Canvas<'a> {
    /// Start a fresh page and reset the cursor to the top margin.
    fn new_page(doc: &'a printpdf::PdfDocumentReference, fonts: &'a Fonts) -> Self {
        let (page, layer) = doc.add_page(Mm(PAGE_W), Mm(PAGE_H), "Layer 1");
        let layer = doc.get_page(page).get_layer(layer);
        Self {
            doc,
            layer,
            fonts,
            y: TOP_Y,
        }
    }

    /// Ensure at least `needed` mm of vertical room remains; otherwise break to
    /// a new page so the next block isn't clipped at the bottom edge.
    fn ensure_room(&mut self, needed: f32) {
        if self.y - needed < BOTTOM_Y {
            let fresh = Canvas::new_page(self.doc, self.fonts);
            self.layer = fresh.layer;
            self.y = fresh.y;
        }
    }

    fn text(&self, s: &str, size: f32, x: f32, y: f32, font: &IndirectFontRef, color: Color) {
        self.layer.set_fill_color(color);
        self.layer.use_text(s, size, Mm(x), Mm(y), font);
    }

    /// Right-align `s` so its end sits at `right_x`. printpdf has no text
    /// measurement, so we estimate width from an average glyph advance for the
    /// font size -- good enough for monospaced money columns.
    fn text_right(
        &self,
        s: &str,
        size: f32,
        right_x: f32,
        y: f32,
        font: &IndirectFontRef,
        color: Color,
    ) {
        let approx_w = (s.chars().count() as f32) * size * 0.20; // mm per char at pt size
        let x = (right_x - approx_w).max(MARGIN_X);
        self.text(s, size, x, y, font, color);
    }

    fn hline(&self, x0: f32, x1: f32, y: f32, color: Color, thickness: f32) {
        self.layer.set_outline_color(color);
        self.layer.set_outline_thickness(thickness);
        let line = Line {
            points: vec![
                (Point::new(Mm(x0), Mm(y)), false),
                (Point::new(Mm(x1), Mm(y)), false),
            ],
            is_closed: false,
        };
        self.layer.add_line(line);
    }

    /// A filled rectangle (used for the dark net block and zebra header rows).
    /// Uses a fill-only `Polygon` so no stray stroke leaks from the previously
    /// set outline color.
    fn rect(&self, x: f32, y_bottom: f32, w: f32, h: f32, fill: Color) {
        self.layer.set_fill_color(fill);
        let poly = Polygon {
            rings: vec![vec![
                (Point::new(Mm(x), Mm(y_bottom)), false),
                (Point::new(Mm(x + w), Mm(y_bottom)), false),
                (Point::new(Mm(x + w), Mm(y_bottom + h)), false),
                (Point::new(Mm(x), Mm(y_bottom + h)), false),
            ]],
            mode: PaintMode::Fill,
            winding_order: WindingOrder::NonZero,
        };
        self.layer.add_polygon(poly);
    }
}

/// Format an integer amount of IQD with thousands separators (e.g. 1234567 ->
/// "1,234,567"). Kept deterministic and locale-free for forensic stability.
fn fmt_iqd(n: i64) -> String {
    let neg = n < 0;
    let mut digits: Vec<char> = n.abs().to_string().chars().collect();
    let mut out = Vec::with_capacity(digits.len() + digits.len() / 3);
    let mut count = 0;
    while let Some(d) = digits.pop() {
        if count != 0 && count % 3 == 0 {
            out.push(',');
        }
        out.push(d);
        count += 1;
    }
    if neg {
        out.push('-');
    }
    out.iter().rev().collect()
}

/// Public entry point: render `close` to a real PDF at `path`. `clinic_name`
/// (if present) is printed as the document's masthead.
pub fn render(close: &DailyClose, clinic_name: Option<&str>, path: &Path) -> AppResult<()> {
    let (doc, page1, layer1) = PdfDocument::new("Daily Close", Mm(PAGE_W), Mm(PAGE_H), "Layer 1");

    let fonts = Fonts {
        regular: doc
            .add_builtin_font(BuiltinFont::Helvetica)
            .map_err(|e| AppError::Internal(format!("pdf font: {e}")))?,
        bold: doc
            .add_builtin_font(BuiltinFont::HelveticaBold)
            .map_err(|e| AppError::Internal(format!("pdf font: {e}")))?,
        mono: doc
            .add_builtin_font(BuiltinFont::Courier)
            .map_err(|e| AppError::Internal(format!("pdf font: {e}")))?,
    };

    let layer1 = doc.get_page(page1).get_layer(layer1);
    let mut c = Canvas {
        doc: &doc,
        layer: layer1,
        fonts: &fonts,
        y: TOP_Y,
    };

    draw_masthead(&mut c, close, clinic_name);
    draw_summary(&mut c, close);
    draw_net_block(&mut c, close);
    draw_doctors(&mut c, close);
    draw_operators(&mut c, close);
    draw_check_types(&mut c, close);
    draw_footer(&c, close);

    let bytes = {
        let mut buf = BufWriter::new(Vec::new());
        doc.save(&mut buf)
            .map_err(|e| AppError::Internal(format!("pdf save: {e}")))?;
        buf.into_inner()
            .map_err(|e| AppError::Internal(format!("pdf buffer: {e}")))?
    };
    write_atomic(path, &bytes)
}

fn draw_masthead(c: &mut Canvas, close: &DailyClose, clinic_name: Option<&str>) {
    // Crimson eyebrow rule + uppercase meta (design-system §5.7).
    c.hline(MARGIN_X, MARGIN_X + 12.0, c.y + 1.5, crimson(), 1.2);
    let eyebrow = format!(
        "DAILY CLOSE  -  {} ({})",
        close.target_date.format("%d/%m/%Y"),
        close.tz_offset
    );
    c.text(&eyebrow, 9.0, MARGIN_X + 15.0, c.y, &c.fonts.bold, ink_3());
    c.y -= 9.0;

    // Masthead title: clinic name if configured, else a generic heading.
    let title = clinic_name
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("Daily Close Report");
    c.text(title, 22.0, MARGIN_X, c.y, &c.fonts.bold, ink());
    c.y -= 8.0;

    // Provisional banner -- a clear warning the figures may still move.
    if close.provisional {
        c.text(
            &format!(
                "PROVISIONAL  -  {} operation(s) not yet synced",
                close.pending_sync
            ),
            9.0,
            MARGIN_X,
            c.y,
            &c.fonts.bold,
            crimson(),
        );
        c.y -= 7.0;
    }

    c.hline(MARGIN_X, PAGE_W - MARGIN_X, c.y, line_color(), 0.5);
    c.y -= 8.0;
}

fn draw_summary(c: &mut Canvas, close: &DailyClose) {
    section_title(c, "SUMMARY");

    let rows: [(&str, String); 7] = [
        ("Locked visits", close.locked_count.to_string()),
        (
            "Voided visits",
            format!(
                "{} ({} IQD)",
                close.voided_count,
                fmt_iqd(close.voided_value_iqd)
            ),
        ),
        (
            "Revenue",
            format!("{} IQD", fmt_iqd(close.total_revenue_iqd)),
        ),
        (
            "Doctor cuts",
            format!("{} IQD", fmt_iqd(close.total_doctor_cuts_iqd)),
        ),
        (
            "Operator cuts",
            format!("{} IQD", fmt_iqd(close.total_operator_cuts_iqd)),
        ),
        (
            "Inventory consumption",
            format!(
                "{} IQD",
                fmt_iqd(close.total_inventory_consumption_value_iqd)
            ),
        ),
        ("Pending sync", close.pending_sync.to_string()),
    ];

    for (label, value) in rows {
        c.ensure_room(7.0);
        c.text(label, 10.0, MARGIN_X, c.y, &c.fonts.regular, ink_3());
        c.text_right(&value, 10.0, PAGE_W - MARGIN_X, c.y, &c.fonts.mono, ink());
        c.y -= 6.5;
    }
    c.y -= 4.0;
}

/// The dark "net" focal block (design-system §5.1: one dark ink card per page).
fn draw_net_block(c: &mut Canvas, close: &DailyClose) {
    c.ensure_room(24.0);
    let h = 20.0;
    let y_bottom = c.y - h + 5.0;
    c.rect(MARGIN_X, y_bottom, CONTENT_W, h, ink());

    let label_y = y_bottom + h - 7.0;
    let value_y = y_bottom + 4.5;
    c.text(
        "NET",
        9.0,
        MARGIN_X + 6.0,
        label_y,
        &c.fonts.bold,
        paper_2(),
    );
    let net = format!("{} IQD", fmt_iqd(close.net_iqd));
    c.text_right(
        &net,
        20.0,
        PAGE_W - MARGIN_X - 6.0,
        value_y,
        &c.fonts.bold,
        white(),
    );

    c.y = y_bottom - 8.0;
}

fn draw_doctors(c: &mut Canvas, close: &DailyClose) {
    if close.per_doctor.is_empty() {
        return;
    }
    section_title(c, "BY DOCTOR");
    table_header(c, &["DOCTOR", "VISITS", "REVENUE", "DOCTOR CUT"]);
    let (mut tv, mut tr, mut tc) = (0i64, 0i64, 0i64);
    for d in &close.per_doctor {
        c.ensure_room(6.5);
        c.text(
            &truncate(&d.name, 34),
            9.5,
            MARGIN_X + 2.0,
            c.y,
            &c.fonts.regular,
            ink(),
        );
        c.text_right(
            &d.visits.to_string(),
            9.5,
            col_x(1),
            c.y,
            &c.fonts.mono,
            ink(),
        );
        c.text_right(
            &fmt_iqd(d.revenue_iqd),
            9.5,
            col_x(2),
            c.y,
            &c.fonts.mono,
            ink(),
        );
        c.text_right(
            &fmt_iqd(d.doctor_cut_iqd),
            9.5,
            col_x(3),
            c.y,
            &c.fonts.mono,
            ink(),
        );
        c.y -= 6.0;
        tv += d.visits;
        tr += d.revenue_iqd;
        tc += d.doctor_cut_iqd;
    }
    table_totals(c, &["TOTALS", &tv.to_string(), &fmt_iqd(tr), &fmt_iqd(tc)]);
}

fn draw_operators(c: &mut Canvas, close: &DailyClose) {
    if close.per_operator.is_empty() {
        return;
    }
    section_title(c, "BY OPERATOR");
    table_header(c, &["OPERATOR", "VISITS", "DYE", "OPERATOR CUT"]);
    let (mut tv, mut td, mut tc) = (0i64, 0i64, 0i64);
    for o in &close.per_operator {
        c.ensure_room(6.5);
        c.text(
            &truncate(&o.name, 34),
            9.5,
            MARGIN_X + 2.0,
            c.y,
            &c.fonts.regular,
            ink(),
        );
        c.text_right(
            &o.visits.to_string(),
            9.5,
            col_x(1),
            c.y,
            &c.fonts.mono,
            ink(),
        );
        c.text_right(
            &o.dye_visits.to_string(),
            9.5,
            col_x(2),
            c.y,
            &c.fonts.mono,
            ink(),
        );
        c.text_right(
            &fmt_iqd(o.operator_cut_iqd),
            9.5,
            col_x(3),
            c.y,
            &c.fonts.mono,
            ink(),
        );
        c.y -= 6.0;
        tv += o.visits;
        td += o.dye_visits;
        tc += o.operator_cut_iqd;
    }
    table_totals(
        c,
        &["TOTALS", &tv.to_string(), &td.to_string(), &fmt_iqd(tc)],
    );
}

fn draw_check_types(c: &mut Canvas, close: &DailyClose) {
    if close.per_check_type.is_empty() {
        return;
    }
    section_title(c, "BY CHECK TYPE");
    table_header(c, &["CHECK TYPE", "VISITS", "REVENUE", "DOCTOR CUT"]);
    let (mut tv, mut tr, mut tc) = (0i64, 0i64, 0i64);
    for ct in &close.per_check_type {
        c.ensure_room(6.5);
        let name = ct.name_en.clone().unwrap_or_else(|| ct.name_ar.clone());
        c.text(
            &truncate(&name, 34),
            9.5,
            MARGIN_X + 2.0,
            c.y,
            &c.fonts.regular,
            ink(),
        );
        c.text_right(
            &ct.visits.to_string(),
            9.5,
            col_x(1),
            c.y,
            &c.fonts.mono,
            ink(),
        );
        c.text_right(
            &fmt_iqd(ct.revenue_iqd),
            9.5,
            col_x(2),
            c.y,
            &c.fonts.mono,
            ink(),
        );
        c.text_right(
            &fmt_iqd(ct.doctor_cut_iqd),
            9.5,
            col_x(3),
            c.y,
            &c.fonts.mono,
            ink(),
        );
        c.y -= 6.0;
        tv += ct.visits;
        tr += ct.revenue_iqd;
        tc += ct.doctor_cut_iqd;
    }
    table_totals(c, &["TOTALS", &tv.to_string(), &fmt_iqd(tr), &fmt_iqd(tc)]);
}

fn draw_footer(c: &Canvas, close: &DailyClose) {
    // A thin hairline + the generated-at stamp and the full input hash so the
    // printout is self-describing and tamper-evident.
    c.hline(
        MARGIN_X,
        PAGE_W - MARGIN_X,
        BOTTOM_Y + 9.0,
        line_color(),
        0.5,
    );
    let stamp = format!(
        "Generated {}",
        close.generated_at.format("%d/%m/%Y %H:%M:%S UTC")
    );
    c.text(
        &stamp,
        7.5,
        MARGIN_X,
        BOTTOM_Y + 4.0,
        &c.fonts.regular,
        ink_3(),
    );
    let hash = format!("hash {}", close.input_hash);
    c.text(&hash, 6.5, MARGIN_X, BOTTOM_Y, &c.fonts.mono, ink_3());
}

// ---- shared block helpers -------------------------------------------------

fn section_title(c: &mut Canvas, title: &str) {
    c.ensure_room(12.0);
    c.hline(MARGIN_X, MARGIN_X + 8.0, c.y + 1.2, crimson(), 1.0);
    c.text(title, 9.0, MARGIN_X + 11.0, c.y, &c.fonts.bold, ink_3());
    c.y -= 7.5;
}

/// Four-column layout: a wide name column then three right-aligned numerics.
fn col_x(idx: usize) -> f32 {
    // Right edges of columns 1..3 (column 0 is the name, left-aligned).
    let right = PAGE_W - MARGIN_X;
    let num_w = (CONTENT_W - 70.0) / 3.0; // 70mm reserved for the name column
    match idx {
        1 => MARGIN_X + 70.0 + num_w,
        2 => MARGIN_X + 70.0 + 2.0 * num_w,
        _ => right,
    }
}

fn table_header(c: &mut Canvas, cols: &[&str; 4]) {
    c.ensure_room(8.0);
    let h = 6.5;
    c.rect(MARGIN_X, c.y - 1.5, CONTENT_W, h, paper_2());
    c.text(cols[0], 7.5, MARGIN_X + 2.0, c.y, &c.fonts.bold, ink_3());
    c.text_right(cols[1], 7.5, col_x(1), c.y, &c.fonts.bold, ink_3());
    c.text_right(cols[2], 7.5, col_x(2), c.y, &c.fonts.bold, ink_3());
    c.text_right(cols[3], 7.5, col_x(3), c.y, &c.fonts.bold, ink_3());
    c.y -= 8.0;
}

fn table_totals(c: &mut Canvas, cols: &[&str; 4]) {
    c.ensure_room(8.0);
    let h = 6.5;
    c.rect(MARGIN_X, c.y - 1.5, CONTENT_W, h, paper_2());
    c.text(cols[0], 7.5, MARGIN_X + 2.0, c.y, &c.fonts.bold, ink());
    c.text_right(cols[1], 9.0, col_x(1), c.y, &c.fonts.mono, ink());
    c.text_right(cols[2], 9.0, col_x(2), c.y, &c.fonts.mono, ink());
    c.text_right(cols[3], 9.0, col_x(3), c.y, &c.fonts.mono, ink());
    c.y -= 10.0;
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let kept: String = s.chars().take(max.saturating_sub(1)).collect();
    format!("{kept}\u{2026}")
}

/// Write `bytes` to `path` atomically (temp file + rename) so a reader never
/// observes a half-written PDF.
fn write_atomic(path: &Path, bytes: &[u8]) -> AppResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::Validation("pdf path has no parent directory".into()))?;
    std::fs::create_dir_all(parent).map_err(AppError::from)?;
    let tmp = parent.join(format!(
        ".{}.tmp",
        path.file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("daily-close.pdf")
    ));
    std::fs::write(&tmp, bytes).map_err(AppError::from)?;
    std::fs::rename(&tmp, path).map_err(AppError::from)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domains::reports::domain::entities::{
        CheckTypeDailyRow, DoctorDailyRow, OperatorDailyRow,
    };
    use chrono::{NaiveDate, TimeZone, Utc};
    use uuid::Uuid;

    fn sample() -> DailyClose {
        DailyClose {
            tenant_id: "t".into(),
            target_date: NaiveDate::from_ymd_opt(2026, 6, 19).unwrap(),
            tz_offset: "+03:00".into(),
            total_revenue_iqd: 50_000,
            total_doctor_cuts_iqd: 1_500,
            total_operator_cuts_iqd: 4_000,
            total_inventory_consumption_value_iqd: 0,
            net_iqd: 44_500,
            locked_count: 2,
            voided_count: 0,
            voided_value_iqd: 0,
            per_doctor: vec![DoctorDailyRow {
                doctor_id: Some(Uuid::nil()),
                name: "Dr. Ahmed Hassan".into(),
                visits: 2,
                revenue_iqd: 50_000,
                doctor_cut_iqd: 1_500,
            }],
            per_operator: vec![OperatorDailyRow {
                operator_id: Uuid::nil(),
                name: "Hassan Tech".into(),
                visits: 2,
                dye_visits: 0,
                operator_cut_iqd: 4_000,
                hours_on_shift_milli: 263_400_000,
            }],
            per_check_type: vec![CheckTypeDailyRow {
                check_type_id: Uuid::nil(),
                name_ar: "اشعة".into(),
                name_en: Some("X-Ray".into()),
                visits: 2,
                revenue_iqd: 50_000,
                doctor_cut_iqd: 1_500,
                operator_cut_iqd: 4_000,
            }],
            pending_sync: 0,
            provisional: false,
            input_hash: "354e7b7d6ea3b156023be9ce69199e78d7d2b3ad21dc9111b5e02f7425c53f93".into(),
            generated_at: Utc.with_ymd_and_hms(2026, 6, 19, 19, 28, 45).unwrap(),
        }
    }

    #[test]
    fn fmt_iqd_groups_thousands() {
        assert_eq!(fmt_iqd(0), "0");
        assert_eq!(fmt_iqd(1_500), "1,500");
        assert_eq!(fmt_iqd(1_234_567), "1,234,567");
        assert_eq!(fmt_iqd(-44_500), "-44,500");
    }

    #[test]
    fn render_produces_a_real_pdf() {
        let dir = std::env::temp_dir().join(format!("idc-pdf-test-{}-real", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("daily-close.pdf");
        render(&sample(), Some("IDC Imaging Center"), &path).unwrap();

        let bytes = std::fs::read(&path).unwrap();
        // A real PDF starts with the "%PDF-" magic and ends with the EOF marker.
        assert!(bytes.starts_with(b"%PDF-"), "missing PDF magic header");
        assert!(
            bytes.windows(5).any(|w| w == b"%%EOF"),
            "missing PDF EOF marker"
        );
        assert!(
            bytes.len() > 800,
            "PDF unexpectedly tiny: {} bytes",
            bytes.len()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn render_handles_provisional_and_empty_groups() {
        let mut close = sample();
        close.provisional = true;
        close.pending_sync = 5;
        close.per_doctor.clear();
        close.per_operator.clear();
        close.per_check_type.clear();

        let dir = std::env::temp_dir().join(format!("idc-pdf-test-{}-prov", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("daily-close.pdf");
        render(&close, None, &path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert!(bytes.starts_with(b"%PDF-"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
