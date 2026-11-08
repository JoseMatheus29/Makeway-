//! O visual gamer do protótipo: tema escuro único, verde-limão neon como
//! destaque e as fontes do próprio Windows (Segoe UI no texto, Bahnschrift
//! condensada e em negrito nos títulos e números).

use std::sync::Arc;

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, FontTweak, Margin, Stroke, TextStyle,
    Theme, ThemePreference, Ui, Vec2, Visuals,
};
use eframe::epaint::text::VariationCoords;

pub struct Paleta {
    pub bg: Color32,
    pub rail: Color32,
    pub sup: Color32,
    pub sup2: Color32,
    pub sup3: Color32,
    pub borda: Color32,
    pub borda2: Color32,
    pub txt: Color32,
    pub txt2: Color32,
    pub txt3: Color32,
    /// Destaque da marca.
    pub lime: Color32,
    /// Texto em cima do lime.
    pub na_lime: Color32,
    /// Lime a 10% sobre a superfície (faixas de destaque).
    pub lime_dim: Color32,
    pub ciano: Color32,
    pub aviso: Color32,
    pub perigo: Color32,
    pub cinza: Color32,
    /// Cores dos dados, sempre nesta ordem; validadas para o fundo escuro
    /// (faixa de luminosidade, daltonismo e contraste).
    pub series: [Color32; 5],
    /// Parte vazia/livre de uma composição.
    pub livre: Color32,
}

const fn hex(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

pub const PALETA: Paleta = Paleta {
    bg: hex(0x07080B),
    rail: hex(0x0B0D11),
    sup: hex(0x101217),
    sup2: hex(0x161A21),
    sup3: hex(0x1D222B),
    borda: hex(0x232833),
    borda2: hex(0x323A48),
    txt: hex(0xEEF1F5),
    txt2: hex(0xA3ACBA),
    txt3: hex(0x7C8596),
    lime: hex(0xC6FF3A),
    na_lime: hex(0x0A0C08),
    lime_dim: hex(0x222A1B),
    ciano: hex(0x4FD8F7),
    aviso: hex(0xFFB020),
    perigo: hex(0xFF4D6D),
    cinza: hex(0x5B6474),
    series: [hex(0x1E9FC7), hex(0xD9661F), hex(0x7F63E8), hex(0xB38600), hex(0xD23E80)],
    livre: hex(0x262B35),
};

pub fn paleta(_ui: &Ui) -> &'static Paleta {
    &PALETA
}

/// Segoe UI Semibold (rótulos fortes no texto corrido).
pub fn forte() -> FontFamily {
    FontFamily::Name("forte".into())
}

/// Bahnschrift negrito condensada: títulos e números grandes.
pub fn display() -> FontFamily {
    FontFamily::Name("display".into())
}

/// Bahnschrift seminegrito: rótulos em caixa alta, navegação, botões.
pub fn rotulo() -> FontFamily {
    FontFamily::Name("rotulo".into())
}

pub fn configurar(ctx: &egui::Context) {
    ctx.set_fonts(fontes());
    ctx.options_mut(|o| o.theme_preference = ThemePreference::Dark);
    ctx.set_visuals_of(Theme::Dark, visuais(&PALETA));
    ctx.all_styles_mut(|estilo| {
        estilo.text_styles = [
            (TextStyle::Small, FontId::new(13.0, FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(15.0, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(15.0, rotulo())),
            (TextStyle::Heading, FontId::new(24.0, display())),
            (TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace)),
        ]
        .into();
        let s = &mut estilo.spacing;
        s.item_spacing = Vec2::new(10.0, 10.0);
        s.button_padding = Vec2::new(16.0, 9.0);
        s.interact_size = Vec2::new(40.0, 36.0);
        s.menu_margin = Margin::same(8);
        s.window_margin = Margin::same(20);
        s.slider_width = 200.0;
    });
}

/// Fontes lidas de C:\Windows\Fonts (nada é baixado nem embutido). A
/// Bahnschrift é variável: a mesma fonte entra duas vezes, com peso e
/// largura fixados. Sem algum arquivo, ficam as fontes padrão do egui.
fn fontes() -> FontDefinitions {
    let mut fontes = FontDefinitions::default();
    let pasta = std::path::PathBuf::from(std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into())).join("Fonts");
    let mut carregar = |nome: &str, arquivo: &str, eixos: &[(&[u8; 4], f32)]| {
        let Ok(bytes) = std::fs::read(pasta.join(arquivo)) else { return false };
        let mut dados = FontData::from_owned(bytes);
        if !eixos.is_empty() {
            dados = dados.tweak(FontTweak { coords: VariationCoords::new(eixos.iter().map(|(t, v)| (*t, *v))), ..Default::default() });
        }
        fontes.font_data.insert(nome.to_owned(), Arc::new(dados));
        true
    };
    let normal = carregar("segoe", "segoeui.ttf", &[]);
    let semi = carregar("segoe-semi", "seguisb.ttf", &[]);
    let negrito = carregar("segoe-negrito", "segoeuib.ttf", &[]);
    let simbolos = carregar("segoe-simbolos", "seguisym.ttf", &[]);
    let bahn_display = carregar("bahn-display", "bahnschrift.ttf", &[(b"wght", 700.0), (b"wdth", 75.0)]);
    let bahn_rotulo = carregar("bahn-rotulo", "bahnschrift.ttf", &[(b"wght", 600.0), (b"wdth", 87.5)]);

    let padrao = fontes.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let familia = |principal: &[(&str, bool)]| -> Vec<String> {
        let mut lista: Vec<String> = principal.iter().filter(|(_, ok)| *ok).map(|(n, _)| (*n).to_owned()).collect();
        lista.extend(padrao.iter().cloned());
        if simbolos {
            lista.push("segoe-simbolos".into());
        }
        lista
    };
    fontes.families.insert(FontFamily::Proportional, familia(&[("segoe", normal)]));
    fontes.families.insert(forte(), familia(&[("segoe-semi", semi), ("segoe", normal)]));
    fontes.families.insert(display(), familia(&[("bahn-display", bahn_display), ("segoe-negrito", negrito)]));
    fontes.families.insert(rotulo(), familia(&[("bahn-rotulo", bahn_rotulo), ("segoe-semi", semi)]));
    fontes
}

fn visuais(p: &Paleta) -> Visuals {
    let mut v = Visuals::dark();
    v.panel_fill = p.bg;
    v.window_fill = p.sup;
    v.extreme_bg_color = p.bg;
    v.text_edit_bg_color = Some(p.bg);
    v.faint_bg_color = p.sup2;
    v.code_bg_color = p.sup2;
    v.hyperlink_color = p.lime;
    v.warn_fg_color = p.aviso;
    v.error_fg_color = p.perigo;
    v.weak_text_color = Some(p.txt3);
    v.window_stroke = Stroke::new(1.0, p.borda2);
    v.window_corner_radius = CornerRadius::same(4);
    v.menu_corner_radius = CornerRadius::same(4);
    v.selection.bg_fill = p.lime_dim;
    v.selection.stroke = Stroke::new(1.0, p.lime);

    let raio = CornerRadius::same(3);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.sup;
    w.noninteractive.weak_bg_fill = p.sup;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.borda);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.txt);
    w.noninteractive.corner_radius = raio;

    w.inactive.bg_fill = p.sup3;
    w.inactive.weak_bg_fill = p.sup3;
    w.inactive.bg_stroke = Stroke::NONE;
    w.inactive.fg_stroke = Stroke::new(1.0, p.txt);
    w.inactive.corner_radius = raio;

    w.hovered.bg_fill = p.borda2;
    w.hovered.weak_bg_fill = p.borda2;
    w.hovered.bg_stroke = Stroke::new(1.0, p.txt3);
    w.hovered.fg_stroke = Stroke::new(1.5, p.txt);
    w.hovered.corner_radius = raio;
    w.hovered.expansion = 0.0;

    w.active.bg_fill = p.borda2;
    w.active.weak_bg_fill = p.borda2;
    w.active.bg_stroke = Stroke::new(1.0, p.lime);
    w.active.fg_stroke = Stroke::new(2.0, p.txt);
    w.active.corner_radius = raio;
    w.active.expansion = 0.0;

    w.open = w.active;
    v
}
