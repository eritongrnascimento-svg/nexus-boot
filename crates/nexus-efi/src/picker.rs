use alloc::{format, vec};
use ab_glyph::{Font, FontRef, PxScale, ScaleFont, point};
use uefi::{boot, proto::console::gop::{BltOp, BltPixel, BltRegion, GraphicsOutput}};

const DARK: BltPixel = BltPixel::new(13, 18, 28);
const PANEL: BltPixel = BltPixel::new(23, 32, 48);
const BORDER: BltPixel = BltPixel::new(45, 61, 82);
const SELECTED: BltPixel = BltPixel::new(20, 45, 54);
const ACCENT: BltPixel = BltPixel::new(91, 231, 207);
const LIGHT: BltPixel = BltPixel::new(238, 245, 253);
const MUTED: BltPixel = BltPixel::new(158, 177, 199);
const FONT: &[u8] = include_bytes!("../assets/Ubuntu-Light.ttf");

struct Canvas {
    width: usize,
    height: usize,
    pixels: alloc::vec::Vec<BltPixel>,
}

impl Canvas {
    fn round(&mut self, x: usize, y: usize, w: usize, h: usize, radius: usize, color: BltPixel) {
        if x >= self.width || y >= self.height { return; }
        let r = radius.min(w / 2).min(h / 2);
        for row in 0..h.min(self.height - y) {
            let dy = r.saturating_sub(row.min(h - 1 - row));
            let inset = (0..r).take_while(|i| (r - i).pow(2) + dy.pow(2) > r.pow(2)).count();
            let start = (x + inset).min(self.width);
            let end = (x + w - inset).min(self.width);
            let offset = (y + row) * self.width;
            self.pixels[offset + start..offset + end].fill(color);
        }
    }

    fn text(&mut self, x: usize, y: usize, max_width: usize, value: &str, size: f32, color: BltPixel) {
        let Ok(font) = FontRef::try_from_slice(FONT) else { return };
        let scale = PxScale::from(size);
        let font = font.as_scaled(scale);
        let mut left = x as f32;
        let baseline = y as f32 + font.ascent();
        for ch in value.chars() {
            let id = font.glyph_id(ch);
            let advance = font.h_advance(id);
            if left + advance > (x + max_width).min(self.width) as f32 { break; }
            let glyph = id.with_scale_and_position(scale, point(left, baseline));
            if let Some(glyph) = font.outline_glyph(glyph) {
                let bounds = glyph.px_bounds();
                glyph.draw(|gx, gy, alpha| {
                    let gx = bounds.min.x as i32 + gx as i32;
                    let gy = bounds.min.y as i32 + gy as i32;
                    if gx < 0 || gy < 0 || gx as usize >= self.width || gy as usize >= self.height { return; }
                    let dest = &mut self.pixels[gy as usize * self.width + gx as usize];
                    let blend = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * alpha) as u8;
                    *dest = BltPixel::new(blend(dest.red, color.red), blend(dest.green, color.green), blend(dest.blue, color.blue));
                });
            }
            left += advance;
        }
    }
}

pub fn draw(labels: &[&str], selected: usize, heading: &str, repairing: bool, notice: Option<&str>) -> bool {
    let Ok(handle) = boot::get_handle_for_protocol::<GraphicsOutput>() else { return false };
    let Ok(mut gop) = boot::open_protocol_exclusive::<GraphicsOutput>(handle) else { return false };
    let (width, height) = gop.current_mode_info().resolution();
    if width < 480 || height < 340 { return false; }
    let w = width.min(1000);
    let h = height.min(550);
    let mut ui = Canvas { width: w, height: h, pixels: vec![DARK; w * h] };
    let content = w - 48;
    ui.round(24, 18, 42, 42, 12, SELECTED);
    ui.round(36, 30, 18, 18, 5, ACCENT);
    ui.round(40, 34, 10, 10, 3, SELECTED);
    ui.text(78, 24, content - 54, "Nexus Boot", 30.0, LIGHT);
    ui.text(24, 76, content, heading, 21.0, MUTED);
    let available = (content / 182).clamp(1, 4);
    let first = selected / available * available;
    let visible = &labels[first.min(labels.len())..(first + available).min(labels.len())];
    let count = visible.len().max(1);
    let card_width = ((content - 14 * (count - 1)) / count).min(270);
    let origin = (w - count * card_width - (count - 1) * 14) / 2;
    let y = (h - 148) / 2;
    for (i, name) in visible.iter().enumerate() {
        let x = origin + i * (card_width + 14);
        let active = first + i == selected;
        let edge = if active { ACCENT } else { BORDER };
        let fill = if active { SELECTED } else { PANEL };
        if active {
            for spread in (2..=12).rev() {
                let glow = 14 + ((12 - spread) * 4) as u8;
                ui.round(x - spread, y - spread, card_width + spread * 2, 148 + spread * 2,
                    18 + spread, BltPixel::new(12, 29 + glow, 40 + glow));
            }
        }
        ui.round(x, y, card_width, 148, 18, edge);
        ui.round(x + 2, y + 2, card_width - 4, 144, 16, fill);
        ui.round(x + 19, y + 18, 46, 42, 10, DARK);
        ui.round(x + 29, y + 28, 26, 21, 4, edge);
        ui.round(x + 33, y + 32, 18, 10, 2, DARK);
        ui.text(x + 19, y + 70, card_width - 38, name, 25.0, LIGHT);
        ui.text(x + 19, y + 108, card_width - 38, if repairing || name.starts_with("Reparação") { "Enter para selecionar" } else if active { "Enter para iniciar" } else { "Carregador EFI" }, 17.0, if active { ACCENT } else { MUTED });
    }
    if labels.is_empty() {
        ui.text(24, y + 50, content, "Nenhum carregador EFI encontrado.", 22.0, LIGHT);
    } else {
        ui.text(24, y + 158, content, &format!("{} de {}  •  {}", selected + 1, labels.len(), labels[selected]), 16.0, MUTED);
    }
    if let Some(notice) = notice { ui.text(24, h - 64, content, notice, 16.0, LIGHT); }
    ui.round(24, h - 38, content, 1, 0, BORDER);
    ui.text(24, h - 30, content, if repairing { "Setas  Navegar      Enter  Selecionar      Esc  Voltar" } else { "Setas  Navegar      Enter  Selecionar      Esc  Firmware" }, 16.0, MUTED);
    gop.blt(BltOp::VideoFill { color: DARK, dest: (0, 0), dims: (width, height) }).is_ok()
        && gop.blt(BltOp::BufferToVideo { buffer: &ui.pixels, src: BltRegion::Full,
            dest: ((width - w) / 2, (height - h) / 2), dims: (w, h) }).is_ok()
}
