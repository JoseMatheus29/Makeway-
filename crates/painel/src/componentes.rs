//! Peças reutilizadas pelas telas, no visual gamer do protótipo: cantos
//! cortados, verde-limão neon, títulos condensados em caixa alta.

use std::path::Path;

use eframe::egui::{
    self, Align, Align2, Color32, FontId, Frame, InnerResponse, Layout, Margin, Pos2, Rect, Response, RichText, Role,
    Sense, Shape, Stroke, Ui, Vec2, WidgetInfo, epaint, pos2, vec2,
};
use nucleo::plataforma;

use crate::icones::{self, Icone};
use crate::tema::{self, paleta};

/// Corte dos cartões grandes e dos pequenos (botões, chips).
pub const CORTE: f32 = 16.0;
pub const CORTE_P: f32 = 8.0;

// ---------- texto ----------

pub fn forte(texto: impl Into<String>, tamanho: f32) -> RichText {
    RichText::new(texto).size(tamanho).family(tema::forte())
}

/// Título condensado em caixa alta (Bahnschrift negrito).
pub fn display(texto: impl Into<String>, tamanho: f32) -> RichText {
    let texto: String = texto.into();
    RichText::new(texto.to_uppercase()).size(tamanho).family(tema::display())
}

/// Rótulo em caixa alta e espaçado ("RAM LIVRE").
pub fn rotulo(texto: impl Into<String>, tamanho: f32, cor: Color32) -> RichText {
    let texto: String = texto.into();
    RichText::new(texto.to_uppercase())
        .size(tamanho)
        .family(tema::rotulo())
        .extra_letter_spacing(tamanho * 0.2)
        .color(cor)
}

/// Texto secundário (explicações).
pub fn apoio(ui: &Ui, texto: impl Into<String>) -> RichText {
    RichText::new(texto).color(paleta(ui).txt2)
}

/// Texto pequeno e discreto.
pub fn nota(ui: &Ui, texto: impl Into<String>) -> RichText {
    RichText::new(texto).size(13.0).color(paleta(ui).txt3)
}

/// Rótulo que quebra linha quando não cabe.
pub fn paragrafo(ui: &mut Ui, texto: impl Into<egui::WidgetText>) -> Response {
    ui.add(egui::Label::new(texto).wrap())
}

/// Linha lime + rótulo lime ("— STATUS DO PC").
pub fn chamada(ui: &mut Ui, texto: &str) {
    let p = paleta(ui);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let (rect, _) = ui.allocate_exact_size(vec2(18.0, 14.0), Sense::hover());
        ui.painter().hline(rect.x_range(), rect.center().y, Stroke::new(2.0, p.lime));
        ui.label(rotulo(texto, 12.0, p.lime));
    });
}

/// Cabeçalho da página: chamada lime, título enorme e uma frase.
pub fn titulo_da_pagina(ui: &mut Ui, chamada_: &str, texto: &str, subtitulo: &str) {
    chamada(ui, chamada_);
    ui.add_space(-2.0);
    ui.label(display(texto, 52.0));
    ui.add_space(-4.0);
    ui.scope(|ui| {
        ui.set_max_width(640.0);
        paragrafo(ui, apoio(ui, subtitulo));
    });
    ui.add_space(14.0);
}

pub fn titulo_da_secao(ui: &mut Ui, texto: &str) {
    ui.label(display(texto, 24.0).extra_letter_spacing(1.4));
}

// ---------- formas ----------

/// Os seis pontos de um retângulo com dois cantos cortados (em cima à
/// esquerda e embaixo à direita).
pub fn cortado(rect: Rect, corte: f32) -> Vec<Pos2> {
    let c = corte.min(rect.width() / 2.0).min(rect.height() / 2.0);
    vec![
        pos2(rect.left() + c, rect.top()),
        rect.right_top(),
        pos2(rect.right(), rect.bottom() - c),
        pos2(rect.right() - c, rect.bottom()),
        rect.left_bottom(),
        pos2(rect.left(), rect.top() + c),
    ]
}

pub fn forma_cortada(rect: Rect, corte: f32, fundo: Color32, borda: Stroke) -> Shape {
    Shape::convex_polygon(cortado(rect, corte), fundo, borda)
}

/// Brilho neon: camadas cada vez maiores e mais transparentes.
pub fn brilho(painter: &egui::Painter, rect: Rect, corte: f32, cor: Color32, forca: f32) {
    for (i, alfa) in [0.10, 0.07, 0.045, 0.025].iter().enumerate() {
        let r = rect.expand(3.0 + i as f32 * 4.0);
        painter.add(forma_cortada(r, corte + 2.0 + i as f32 * 2.0, cor.gamma_multiply(alfa * forca), Stroke::NONE));
    }
}

/// Moldura com cantos cortados: o fundo é pintado depois do conteúdo,
/// quando o tamanho já é conhecido, por baixo dele.
pub fn moldura<R>(ui: &mut Ui, fundo: Color32, borda: Stroke, corte: f32, margem: Margin, conteudo: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
    let lugar = ui.painter().add(Shape::Noop);
    let resposta = Frame::new().inner_margin(margem).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.with_layout(Layout::top_down(Align::Min), conteudo).inner
    });
    ui.painter().set(lugar, forma_cortada(resposta.response.rect, corte, fundo, borda));
    InnerResponse::new(resposta.inner, resposta.response)
}

/// Cartão padrão: superfície, corte de 16 px.
pub fn cartao<R>(ui: &mut Ui, conteudo: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
    cartao_com(ui, Margin::symmetric(28, 26), conteudo)
}

pub fn cartao_com<R>(ui: &mut Ui, margem: Margin, conteudo: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
    let p = paleta(ui);
    moldura(ui, p.sup, Stroke::NONE, CORTE, margem, conteudo)
}

/// Caixa colorida (aviso de segurança, alerta).
pub fn faixa<R>(ui: &mut Ui, fundo: Color32, conteudo: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
    moldura(ui, fundo, Stroke::NONE, CORTE, Margin::symmetric(24, 18), conteudo)
}

/// Listras diagonais finas (subindo para a direita) em `area`, para pintar
/// com um recorte na área. São paralelas ao corte de baixo à direita: as
/// que começam depois de `limite_x` cairiam nele e ficam de fora.
pub fn listras(area: Rect, limite_x: f32, cor: Color32) -> Vec<Shape> {
    let h = area.height();
    let mut formas = Vec::new();
    let mut x = area.left() - h;
    while x <= limite_x {
        let mut a = pos2(x, area.bottom());
        if a.x < area.left() {
            a = pos2(area.left(), area.bottom() - (area.left() - x));
        }
        formas.push(Shape::line_segment([a, pos2(x + h, area.top())], Stroke::new(2.0, cor)));
        x += 12.0;
    }
    formas
}

/// Pontinhos de fundo da página, a cada 22 px.
pub fn grade_de_pontos(painter: &egui::Painter, rect: Rect, cor: Color32) {
    let passo = 22.0;
    let mut y = rect.top() + passo / 2.0;
    while y < rect.bottom() {
        let mut x = rect.left() + passo / 2.0;
        while x < rect.right() {
            painter.rect_filled(Rect::from_center_size(pos2(x, y), Vec2::splat(1.5)), 0.0, cor);
            x += passo;
        }
        y += passo;
    }
}

/// As duas setas da marca (">>"), dentro de `rect` (grade de 24).
pub fn setas(rect: Rect, cor: Color32, espessura: f32) -> Vec<Shape> {
    let e = rect.width() / 24.0;
    let pt = |x: f32, y: f32| pos2(rect.left() + x * e, rect.top() + y * e);
    let traco = Stroke::new(espessura * e, cor);
    vec![
        Shape::line(vec![pt(5.0, 5.0), pt(12.0, 12.0), pt(5.0, 19.0)], traco),
        Shape::line(vec![pt(12.0, 5.0), pt(19.0, 12.0), pt(12.0, 19.0)], traco),
    ]
}

/// Ícone dentro de um quadrado de cantos cortados.
pub fn bloco_icone(ui: &mut Ui, icone: Icone, cor: Color32, fundo: Color32, lado: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(lado), Sense::hover());
    ui.painter().add(forma_cortada(rect, CORTE_P, fundo, Stroke::NONE));
    let tamanho = lado * 0.48;
    icones::pintar(ui.painter(), Rect::from_center_size(rect.center(), Vec2::splat(tamanho)), icone, cor);
}

pub fn separador(ui: &mut Ui) {
    let p = paleta(ui);
    let largura = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(largura, 1.0), Sense::hover());
    ui.painter().hline(rect.x_range(), rect.center().y, Stroke::new(1.0, p.borda));
}

// ---------- controles ----------

/// Chave liga/desliga de cantos cortados. Área de toque de 66×44 px.
pub fn chave(ui: &mut Ui, ligada: &mut bool) -> Response {
    let (area, mut resposta) = ui.allocate_exact_size(Vec2::new(66.0, 44.0), Sense::click());
    if resposta.clicked() {
        *ligada = !*ligada;
        resposta.mark_changed();
    }
    let valor = *ligada;
    resposta.widget_info(|| WidgetInfo::selected(Role::CheckBox, true, valor, ""));
    let p = paleta(ui);
    let t = ui.ctx().animate_bool_responsive(resposta.id, *ligada);
    let trilho = Rect::from_center_size(area.center(), Vec2::new(56.0, 28.0));
    if t > 0.0 {
        brilho(ui.painter(), trilho, 6.0, p.lime, t * 0.8);
    }
    ui.painter().add(forma_cortada(trilho, 6.0, misturar(p.sup3, p.lime, t), Stroke::NONE));
    if resposta.hovered() && t < 0.5 {
        ui.painter().add(forma_cortada(trilho, 6.0, Color32::TRANSPARENT, Stroke::new(1.0, p.borda2)));
    }
    if resposta.has_focus() {
        ui.painter().add(forma_cortada(trilho.expand(3.0), 8.0, Color32::TRANSPARENT, Stroke::new(2.0, p.lime)));
    }
    let x = egui::lerp((trilho.left() + 14.0)..=(trilho.right() - 14.0), t);
    let bola = Rect::from_center_size(pos2(x, trilho.center().y), Vec2::splat(20.0));
    ui.painter().add(forma_cortada(bola, 5.0, misturar(p.txt3, p.na_lime, t), Stroke::NONE));
    resposta.on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub fn misturar(a: Color32, b: Color32, t: f32) -> Color32 {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

/// Botão desenhado à mão com cantos cortados.
#[allow(clippy::too_many_arguments)]
fn botao_cortado(ui: &mut Ui, texto: &str, tamanho: f32, altura: f32, largura_min: f32, fundo: Color32, cor: Color32, borda: Stroke, com_brilho: bool) -> Response {
    let fonte = FontId::new(tamanho, tema::display());
    let galley = ui.painter().layout_no_wrap(texto.to_uppercase(), fonte.clone(), cor);
    let espacamento = tamanho * 0.12;
    let letras = texto.chars().count().saturating_sub(1) as f32 * espacamento;
    let largura = (galley.size().x + letras + 40.0).max(largura_min);
    let (rect, resposta) = ui.allocate_exact_size(Vec2::new(largura, altura), Sense::click());
    if ui.is_rect_visible(rect) {
        let fundo = if resposta.is_pointer_button_down_on() {
            misturar(fundo, Color32::BLACK, 0.12)
        } else if resposta.hovered() {
            misturar(fundo, Color32::WHITE, 0.08)
        } else {
            fundo
        };
        if com_brilho {
            brilho(ui.painter(), rect, CORTE_P, fundo, 1.0);
        }
        ui.painter().add(forma_cortada(rect, CORTE_P, fundo, borda));
        texto_espacado(ui.painter(), rect.center(), Align2::CENTER_CENTER, &texto.to_uppercase(), fonte, espacamento, cor);
        if resposta.has_focus() {
            ui.painter().add(forma_cortada(rect.expand(3.0), CORTE_P + 2.0, Color32::TRANSPARENT, Stroke::new(2.0, paleta(ui).lime)));
        }
    }
    let texto = texto.to_owned();
    resposta.widget_info(|| WidgetInfo::labeled(Role::Button, true, &texto));
    resposta.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Texto com espaço extra entre as letras, sem quebrar linha.
pub fn texto_espacado(painter: &egui::Painter, pos: Pos2, ancora: Align2, texto: &str, fonte: FontId, espacamento: f32, cor: Color32) -> Rect {
    let mut job = epaint::text::LayoutJob::default();
    job.append(texto, 0.0, epaint::text::TextFormat { font_id: fonte, color: cor, extra_letter_spacing: espacamento, ..Default::default() });
    let galley = painter.layout_job(job);
    let rect = ancora.anchor_size(pos, galley.size());
    painter.galley(rect.min, galley, cor);
    rect
}

pub fn botao_primario(ui: &mut Ui, texto: &str) -> Response {
    let p = paleta(ui);
    botao_cortado(ui, texto, 15.0, 46.0, 120.0, p.lime, p.na_lime, Stroke::NONE, true)
}

pub fn botao_secundario(ui: &mut Ui, texto: &str) -> Response {
    let p = paleta(ui);
    botao_cortado(ui, texto, 14.0, 44.0, 0.0, p.sup3, p.txt, Stroke::NONE, false)
}

/// Botão de contorno tracejado em lime ("+ ADICIONAR APP").
pub fn botao_adicionar(ui: &mut Ui, texto: &str) -> Response {
    let p = paleta(ui);
    botao_cortado(ui, texto, 14.0, 42.0, 0.0, Color32::TRANSPARENT, p.lime, Stroke::new(1.0, p.borda2), false)
}

/// Botões lado a lado, um escolhido (em lime). Devolve `true` se mudou.
pub fn segmentado(ui: &mut Ui, opcoes: &[&str], escolhido: &mut usize, altura: f32, largura: f32) -> bool {
    let p = paleta(ui);
    let mut mudou = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        for (i, opcao) in opcoes.iter().enumerate() {
            let ativo = i == *escolhido;
            let (fundo, cor) = if ativo { (p.lime, p.na_lime) } else { (p.sup3, p.txt2) };
            let r = botao_cortado(ui, opcao, 16.0, altura, largura, fundo, cor, Stroke::NONE, ativo);
            let r = r.on_hover_text(if ativo { "Escolhido" } else { "Clique para escolher" });
            if r.clicked() && !ativo {
                *escolhido = i;
                mudou = true;
            }
        }
    });
    mudou
}

/// Link em caixa alta com seta ("VER QUEM USA →").
pub fn link(ui: &mut Ui, texto: &str) -> Response {
    let p = paleta(ui);
    let fonte = FontId::new(13.0, tema::rotulo());
    let mut job = epaint::text::LayoutJob::default();
    job.append(&format!("{} →", texto.to_uppercase()), 0.0, epaint::text::TextFormat { font_id: fonte, color: p.lime, extra_letter_spacing: 1.8, ..Default::default() });
    let galley = ui.painter().layout_job(job);
    let (rect, resposta) = ui.allocate_exact_size(Vec2::new(galley.size().x, 32.0), Sense::click());
    let cor = if resposta.hovered() { p.txt } else { p.lime };
    ui.painter().galley_with_override_text_color(pos2(rect.left(), rect.center().y - galley.size().y / 2.0), galley, cor);
    resposta.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Etiqueta cheia ("SELECIONADO", contador de boosts).
pub fn etiqueta(ui: &mut Ui, texto: &str, fundo: Color32, cor: Color32) {
    let fonte = FontId::new(12.0, tema::display());
    let mut job = epaint::text::LayoutJob::default();
    job.append(&texto.to_uppercase(), 0.0, epaint::text::TextFormat { font_id: fonte, color: cor, extra_letter_spacing: 1.6, ..Default::default() });
    let galley = ui.painter().layout_job(job);
    let (rect, _) = ui.allocate_exact_size(galley.size() + vec2(20.0, 10.0), Sense::hover());
    ui.painter().add(forma_cortada(rect, 5.0, fundo, Stroke::NONE));
    ui.painter().galley(rect.min + vec2(10.0, 5.0), galley, cor);
}

/// Carimbo torto com contorno ("RODOU LISO").
pub fn carimbo(ui: &mut Ui, texto: &str, cor: Color32) {
    let fonte = FontId::new(15.0, tema::display());
    let mut job = epaint::text::LayoutJob::default();
    job.append(&texto.to_uppercase(), 0.0, epaint::text::TextFormat { font_id: fonte, color: cor, extra_letter_spacing: 2.4, ..Default::default() });
    let galley = ui.painter().layout_job(job);
    let tamanho = galley.size() + vec2(26.0, 14.0);
    let (lugar, _) = ui.allocate_exact_size(tamanho + vec2(4.0, 10.0), Sense::hover());
    let rect = Rect::from_center_size(lugar.center(), tamanho);
    let angulo = -3.0_f32.to_radians();
    let rot = egui::emath::Rot2::from_angle(angulo);
    let centro = rect.center();
    let girar = |pt: Pos2| centro + rot * (pt - centro);
    let pontos: Vec<Pos2> = cortado(rect, 6.0).into_iter().map(girar).collect();
    ui.painter().add(Shape::closed_line(pontos, Stroke::new(2.0, cor)));
    let inicio = girar(pos2(centro.x - galley.size().x / 2.0, centro.y - galley.size().y / 2.0));
    ui.painter().add(epaint::TextShape::new(inicio, galley, cor).with_angle(angulo));
}

/// Pílula com texto e um X para tirar. Devolve `true` se clicaram no X.
/// Mede o texto antes de reservar espaço: numa linha que quebra
/// (`horizontal_wrapped`), uma pílula longa desce inteira para a próxima.
pub fn chip_removivel(ui: &mut Ui, texto: &str, dica: &str) -> bool {
    let p = paleta(ui);
    let texto = cortar(texto, 40);
    let galley = ui.painter().layout_no_wrap(texto.clone(), FontId::new(14.0, tema::forte()), p.txt);
    let tamanho = Vec2::new(6.0 + 28.0 + 10.0 + galley.size().x + 4.0 + 34.0 + 4.0, 40.0);
    let (rect, resposta) = ui.allocate_exact_size(tamanho, Sense::hover());
    resposta.on_hover_text(dica);
    ui.painter().add(forma_cortada(rect, CORTE_P, p.sup3, Stroke::NONE));
    let inicial = Rect::from_min_size(pos2(rect.left() + 6.0, rect.center().y - 14.0), Vec2::splat(28.0));
    ui.painter().rect_filled(inicial, 0.0, p.lime);
    let letra = texto.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default();
    ui.painter().text(inicial.center(), Align2::CENTER_CENTER, letra, FontId::new(15.0, tema::display()), p.na_lime);
    ui.painter().galley(pos2(inicial.right() + 10.0, rect.center().y - galley.size().y / 2.0), galley, p.txt);
    let botao = Rect::from_center_size(pos2(rect.right() - 21.0, rect.center().y), Vec2::splat(34.0));
    let clique = ui.interact(botao, ui.scope_id().with(("tirar", rect.min.x as i32, rect.min.y as i32)), Sense::click());
    let cor_x = if clique.hovered() { p.perigo } else { p.txt3 };
    icones::pintar(ui.painter(), Rect::from_center_size(botao.center(), Vec2::splat(14.0)), Icone::Fechar, cor_x);
    clique.on_hover_text("Tirar da lista").on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

/// Pílula só de leitura.
pub fn chip(ui: &mut Ui, texto: &str, cor_texto: Color32) {
    let p = paleta(ui);
    let galley = ui.painter().layout_no_wrap(cortar(texto, 44), FontId::new(14.0, tema::forte()), cor_texto);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(galley.size().x + 28.0, 36.0), Sense::hover());
    ui.painter().add(forma_cortada(rect, CORTE_P, p.sup3, Stroke::NONE));
    ui.painter().galley(pos2(rect.left() + 14.0, rect.center().y - galley.size().y / 2.0), galley, cor_texto);
}

/// Corta com reticências quando passa de `max` letras.
pub fn cortar(texto: &str, max: usize) -> String {
    if texto.chars().count() <= max {
        return texto.to_owned();
    }
    let mut curto: String = texto.chars().take(max.saturating_sub(1)).collect();
    curto.push('…');
    curto
}

/// Botão pequeno que abre o Explorer no arquivo.
pub fn botao_mostrar(ui: &mut Ui, caminho: &Path) {
    let p = paleta(ui);
    if botao_cortado(ui, "Mostrar na pasta", 13.0, 38.0, 0.0, p.sup3, p.txt, Stroke::NONE, false).clicked() {
        plataforma::mostrar_no_explorer(caminho);
    }
}

// ---------- dados ----------

/// Medidor em blocos na largura toda: a fração vira blocos acesos (com
/// brilho), o resto fica apagado.
pub fn medidor(ui: &mut Ui, fracao: f32, blocos: usize, altura: f32, cor: Color32) {
    let largura = ui.available_width();
    medidor_em(ui, largura, fracao, blocos, altura, 3.0, cor, true);
}

/// Medidor em blocos com largura fixa. Uma parte acesa nunca some: com
/// fração > 0, pelo menos um bloco acende.
#[allow(clippy::too_many_arguments)]
pub fn medidor_em(ui: &mut Ui, largura: f32, fracao: f32, blocos: usize, altura: f32, espaco: f32, cor: Color32, com_brilho: bool) {
    let p = paleta(ui);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(largura, altura), Sense::hover());
    let w = (rect.width() - espaco * (blocos as f32 - 1.0)) / blocos as f32;
    let fracao = fracao.clamp(0.0, 1.0);
    let mut acesos = (fracao * blocos as f32).round() as usize;
    if fracao > 0.0 {
        acesos = acesos.max(1);
    }
    for i in 0..blocos {
        let x = rect.left() + i as f32 * (w + espaco);
        let bloco = Rect::from_min_size(pos2(x, rect.top()), Vec2::new(w, altura));
        if i < acesos {
            if com_brilho {
                ui.painter().rect_filled(bloco.expand(2.0), 0.0, cor.gamma_multiply(0.12));
            }
            ui.painter().rect_filled(bloco, 0.0, cor);
        } else {
            ui.painter().rect_filled(bloco, 0.0, p.sup3);
        }
    }
}

/// Barra fina de proporção (ranking).
pub fn barra(ui: &mut Ui, fracao: f32, largura: f32, cor: Color32) {
    let p = paleta(ui);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(largura, 6.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, p.sup3);
    let fracao = fracao.clamp(0.0, 1.0);
    if fracao > 0.0 {
        let cheio = Rect::from_min_size(rect.min, Vec2::new((rect.width() * fracao).max(3.0), rect.height()));
        ui.painter().rect_filled(cheio, 0.0, cor);
    }
}

/// Composição em blocos (partes de um todo): cada parte pinta os blocos
/// que lhe cabem, valor de cada parte ao passar o mouse.
pub fn blocos_empilhados(ui: &mut Ui, partes: &[(u64, Color32, String)], blocos: usize, altura: f32) {
    let p = paleta(ui);
    let largura = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(largura, altura), Sense::hover());
    let total: u64 = partes.iter().map(|x| x.0).sum();
    if total == 0 {
        return;
    }
    // Quantos blocos cada parte ganha (parte > 0 ganha pelo menos 1).
    let mut cotas: Vec<usize> = partes
        .iter()
        .map(|x| if x.0 == 0 { 0 } else { ((x.0 as f64 / total as f64 * blocos as f64).round() as usize).max(1) })
        .collect();
    while cotas.iter().sum::<usize>() > blocos {
        let maior = cotas.iter().enumerate().max_by_key(|(_, c)| **c).map(|(i, _)| i).unwrap_or(0);
        cotas[maior] -= 1;
    }
    while cotas.iter().sum::<usize>() < blocos {
        let maior = cotas.iter().enumerate().max_by_key(|(_, c)| **c).map(|(i, _)| i).unwrap_or(0);
        cotas[maior] += 1;
    }
    let espaco = 3.0;
    let w = (rect.width() - espaco * (blocos as f32 - 1.0)) / blocos as f32;
    let mut i = 0;
    for (n, (cota, (_, cor, dica))) in cotas.iter().zip(partes).enumerate() {
        if *cota == 0 {
            continue;
        }
        let x0 = rect.left() + i as f32 * (w + espaco);
        for _ in 0..*cota {
            let x = rect.left() + i as f32 * (w + espaco);
            let cor = if *cor == p.livre { p.sup3 } else { *cor };
            ui.painter().rect_filled(Rect::from_min_size(pos2(x, rect.top()), Vec2::new(w, altura)), 0.0, cor);
            i += 1;
        }
        let x1 = rect.left() + i as f32 * (w + espaco) - espaco;
        let faixa = Rect::from_min_max(pos2(x0, rect.top()), pos2(x1, rect.bottom()));
        ui.interact(faixa, ui.scope_id().with(("parte", n)), Sense::hover()).on_hover_text(dica.as_str());
    }
}

/// Número enorme + unidade menor ("3,6 GB"), os dois na mesma linha de base.
pub fn numero_grande(ui: &mut Ui, valor: &str, tamanho: f32) {
    let p = paleta(ui);
    let (numero, unidade) = valor.rsplit_once(' ').unwrap_or((valor, ""));
    let g_numero = ui.painter().layout_no_wrap(numero.to_owned(), FontId::new(tamanho, tema::display()), p.txt);
    let g_unidade = ui.painter().layout_no_wrap(unidade.to_owned(), FontId::new(tamanho * 0.38, tema::display()), p.txt2);
    let espaco = if unidade.is_empty() { 0.0 } else { 6.0 };
    // Corta o espaço que sobra abaixo da linha de base (letras sem perna).
    let base = linha_de_base(&g_numero);
    let altura = base + tamanho * 0.06;
    let (rect, _) = ui.allocate_exact_size(vec2(g_numero.size().x + espaco + g_unidade.size().x, altura), Sense::hover());
    let desvio = base - linha_de_base(&g_unidade);
    let x_unidade = rect.left() + g_numero.size().x + espaco;
    ui.painter().galley(rect.min, g_numero, p.txt);
    ui.painter().galley(pos2(x_unidade, rect.top() + desvio), g_unidade, p.txt2);
}

/// Distância do topo do texto até a linha de base da primeira linha.
fn linha_de_base(galley: &epaint::Galley) -> f32 {
    galley.rows.first().map_or(galley.size().y, |linha| {
        linha.pos.y + linha.row.glyphs.first().map_or(linha.row.size.y, |g| g.pos.y)
    })
}

/// Linha de tela vazia (sem dados ainda).
pub fn vazio(ui: &mut Ui, icone: Icone, nome: &str, explicacao: &str) {
    let p = paleta(ui);
    ui.horizontal_top(|ui| {
        bloco_icone(ui, icone, p.lime, p.sup3, 46.0);
        ui.add_space(4.0);
        ui.vertical(|ui| {
            ui.label(display(nome, 20.0));
            paragrafo(ui, apoio(ui, explicacao).size(14.0));
        });
    });
}
