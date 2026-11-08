//! Partidas: cada jogo com um carimbo ("RODOU LISO" / "FALTOU MEMÓRIA"),
//! três números, o gráfico da memória livre com a zona de travada e o
//! próximo passo com o botão do lado.

use std::path::PathBuf;

use eframe::egui::{self, Align, Align2, Color32, FontId, Layout, Margin, Pos2, Rect, RichText, Sense, Shape, Stroke, Ui, pos2, vec2};
use egui_plot::{HLine, HoverPosition, Line, LineStyle, Plot, PlotPoint, PlotPoints, Polygon, Text};
use nucleo::formato::{decimal, duracao};
use nucleo::modo_jogo::relatorio::{self, Amostra, RelatorioSessao};

use super::{partida_boa, quando, rival_da_partida, tamanho_curto};
use crate::app::Contexto;
use crate::componentes::{
    CORTE_P, botao_primario, cartao, cartao_com, cortado, cortar, faixa, forma_cortada, forte, medidor_em,
    moldura, nota, paragrafo, rotulo, titulo_da_pagina, vazio,
};
use crate::icones::{self, Icone};
use crate::tema::{self, paleta};

const GB: f64 = (1u64 << 30) as f64;

#[derive(Default)]
pub struct TelaPartidas {
    selecionada: usize,
    /// A mais nova da última vez: quando chega uma nova, ela é escolhida.
    mais_nova: Option<PathBuf>,
    amostras: Option<(PathBuf, Vec<Amostra>)>,
}

impl TelaPartidas {
    pub fn ui(&mut self, ui: &mut Ui, c: &mut Contexto) {
        titulo_da_pagina(ui, "Histórico", "Partidas", "Como o PC se saiu em cada jogo e o que dá para melhorar.");
        if c.dados.partidas.is_empty() {
            cartao(ui, |ui| {
                vazio(
                    ui,
                    Icone::Grafico,
                    "Nenhuma partida gravada ainda",
                    "Deixe o vigia ligado e jogue. Comece no modo Observar por umas 3 partidas: o que dá para melhorar aparece aqui.",
                );
            });
            return;
        }
        let lista = c.dados.partidas.clone();
        let primeira = lista.first().map(|(p, _)| p.clone());
        if primeira != self.mais_nova {
            self.selecionada = 0;
            self.mais_nova = primeira;
        }
        self.selecionada = self.selecionada.min(lista.len() - 1);

        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 18.0;
            ui.vertical(|ui| {
                ui.set_width(280.0);
                ui.spacing_mut().item_spacing.y = 8.0;
                for (i, (_, r)) in lista.iter().enumerate().take(40) {
                    if item_da_lista(ui, i == self.selecionada, r).clicked() {
                        self.selecionada = i;
                    }
                }
            });
            ui.vertical(|ui| {
                let (arquivo, r) = &lista[self.selecionada];
                if self.amostras.as_ref().is_none_or(|(p, _)| p != arquivo) {
                    self.amostras = Some((arquivo.clone(), relatorio::ler_amostras(arquivo).unwrap_or_default()));
                }
                let amostras = self.amostras.as_ref().map(|(_, a)| a.clone()).unwrap_or_default();
                detalhes(ui, c, r, &amostras);
            });
        });
    }
}

fn nome_do_jogo(r: &RelatorioSessao) -> &str {
    r.jogo.trim_end_matches(".exe")
}

fn modo(r: &RelatorioSessao) -> &'static str {
    if r.modo == "otimizar" { "Otimizado" } else { "Só observando" }
}

fn cor_do_veredito(ui: &Ui, r: &RelatorioSessao) -> Color32 {
    let p = paleta(ui);
    if partida_boa(r) { p.lime } else { p.perigo }
}

fn item_da_lista(ui: &mut Ui, escolhida: bool, r: &RelatorioSessao) -> egui::Response {
    let p = paleta(ui);
    let cor = cor_do_veredito(ui, r);
    let fundo = if escolhida { p.sup2 } else { p.sup };
    let resposta = moldura(ui, fundo, Stroke::NONE, CORTE_P, Margin::symmetric(16, 14), |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            let (barra, _) = ui.allocate_exact_size(vec2(10.0, 58.0), Sense::hover());
            let barra = Rect::from_center_size(barra.center(), vec2(10.0, 40.0));
            ui.painter().rect_filled(barra.expand(3.0), 0.0, cor.gamma_multiply(0.12));
            ui.painter().rect_filled(barra, 0.0, cor);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                ui.label(RichText::new(cortar(&nome_do_jogo(r).to_uppercase(), 22)).family(tema::display()).size(18.0).color(p.txt));
                ui.label(rotulo(if partida_boa(r) { "Rodou liso" } else { "Faltou memória" }, 12.0, cor).family(tema::display()));
                ui.label(RichText::new(format!("{} · {} · {}", quando(r.inicio_unix), duracao(r.duracao_segundos), modo(r))).size(12.0).color(p.txt3));
            });
        });
    })
    .response;
    if escolhida {
        ui.painter().add(Shape::closed_line(cortado(resposta.rect.shrink(1.0), CORTE_P), Stroke::new(2.0, cor)));
    }
    let clique = ui.interact(resposta.rect, resposta.id.with("partida"), Sense::click());
    if clique.hovered() && !escolhida {
        ui.painter().add(forma_cortada(resposta.rect, CORTE_P, Color32::TRANSPARENT, Stroke::new(1.0, p.borda2)));
    }
    clique.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Carimbo grande e torto do veredito, com brilho.
fn carimbo_grande(ui: &mut Ui, texto: &str, cor: Color32) {
    let fonte = FontId::new(34.0, tema::display());
    let mut job = egui::text::LayoutJob::default();
    job.append(&texto.to_uppercase(), 0.0, egui::TextFormat { font_id: fonte, color: cor, extra_letter_spacing: 3.4, ..Default::default() });
    let galley = ui.painter().layout_job(job);
    let tamanho = galley.size() + vec2(32.0, 16.0);
    let (lugar, _) = ui.allocate_exact_size(tamanho + vec2(6.0, 18.0), Sense::hover());
    let rect = Rect::from_center_size(lugar.center(), tamanho);
    let angulo = -2.0_f32.to_radians();
    let rot = egui::emath::Rot2::from_angle(angulo);
    let centro = rect.center();
    let girar = |pt: Pos2| centro + rot * (pt - centro);
    let pontos: Vec<Pos2> = cortado(rect, CORTE_P).into_iter().map(girar).collect();
    let pintor = ui.painter();
    pintor.add(Shape::convex_polygon(pontos.clone(), cor.gamma_multiply(0.06), Stroke::NONE));
    pintor.add(Shape::closed_line(pontos, Stroke::new(3.0, cor)));
    let inicio = girar(pos2(centro.x - galley.size().x / 2.0, centro.y - galley.size().y / 2.0));
    pintor.add(egui::epaint::TextShape::new(inicio, galley, cor).with_angle(angulo));
}

fn detalhes(ui: &mut Ui, c: &mut Contexto, r: &RelatorioSessao, amostras: &[Amostra]) {
    let p = paleta(ui);
    let boa = partida_boa(r);
    let cor = cor_do_veredito(ui, r);
    let cresceu = r.paginacao_max.saturating_sub(r.paginacao_inicio);
    let mut pausar = None;
    cartao_com(ui, Margin::symmetric(30, 28), |ui| {
        ui.spacing_mut().item_spacing.y = 22.0;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 18.0;
            carimbo_grande(ui, if boa { "Rodou liso" } else { "Faltou memória" }, cor);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.add_space(10.0);
                ui.label(RichText::new(cortar(&nome_do_jogo(r).to_uppercase(), 30)).family(tema::display()).size(22.0).color(p.txt));
                ui.label(nota(ui, format!("{} · {} · {}", quando(r.inicio_unix), duracao(r.duracao_segundos), modo(r))));
            });
        });

        let livre_ok = r.ram_disponivel_min >= 1 << 30;
        let disco_ok = cresceu <= 256 << 20;
        let fatos = [
            (
                "Mín. RAM livre",
                tamanho_curto(r.ram_disponivel_min),
                if livre_ok { "Sobrou o tempo todo." } else { "Pouco para um jogo pesado." },
                if livre_ok { p.lime } else { p.perigo },
            ),
            (
                "Disco como RAM",
                tamanho_curto(cresceu),
                if disco_ok { "Nada de travadinha por isso." } else { "É isso que causa travadinhas." },
                if disco_ok { p.lime } else { p.perigo },
            ),
            ("Pico do jogo", tamanho_curto(r.ram_jogo_max), "O máximo que o jogo usou.", p.txt),
        ];
        ui.columns(3, |colunas| {
            for (coluna, (nome, numero, texto, cor)) in colunas.iter_mut().zip(fatos) {
                moldura(coluna, p.sup2, Stroke::NONE, CORTE_P, Margin::symmetric(18, 16), |ui| {
                    ui.set_min_height(96.0);
                    ui.spacing_mut().item_spacing.y = 4.0;
                    ui.label(rotulo(nome, 11.0, p.txt3));
                    ui.label(RichText::new(numero).family(tema::display()).size(34.0).color(cor));
                    paragrafo(ui, RichText::new(texto).size(13.0).color(p.txt2));
                });
            }
        });

        if amostras.len() >= 2 {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                ui.label(rotulo("Memória livre durante a partida", 17.0, p.txt).family(tema::display()).extra_letter_spacing(2.0));
                grafico(ui, amostras);
            });
        }

        let rivais: Vec<_> = r
            .concorrentes
            .iter()
            .filter(|x| !x.protegido && !x.nome.eq_ignore_ascii_case(&r.jogo))
            .take(3)
            .collect();
        if !rivais.is_empty() {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 10.0;
                ui.label(rotulo("Quem mais disputou com o jogo", 17.0, p.txt).family(tema::display()).extra_letter_spacing(2.0));
                let maior = rivais.iter().map(|x| x.ram_max).max().unwrap_or(1).max(1);
                for x in rivais {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 14.0;
                        let largura = (ui.available_width() - 170.0 - 190.0 - 28.0).max(120.0);
                        ui.allocate_ui_with_layout(vec2(largura, 24.0), Layout::left_to_right(Align::Center), |ui| {
                            ui.set_min_width(largura);
                            ui.label(forte(cortar(&c.dados.nome(&x.nome), 34), 14.5)).on_hover_text(&x.nome);
                        });
                        medidor_em(ui, 170.0, x.ram_max as f32 / maior as f32, 10, 10.0, 2.0, p.ciano, false);
                        ui.allocate_ui_with_layout(vec2(190.0, 24.0), Layout::right_to_left(Align::Center), |ui| {
                            ui.set_min_width(190.0);
                            let uso = format!("{} · {}% CPU", tamanho_curto(x.ram_max), decimal(x.cpu_media_pct, 0));
                            ui.label(RichText::new(uso).family(tema::display()).size(15.0).color(p.txt2));
                        });
                    });
                }
            });
        }

        faixa(ui, p.lime_dim, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 16.0;
                icones::icone(ui, Icone::Lampada, 26.0, p.lime);
                let rival = rival_da_partida(c, r);
                let reservado = if rival.is_some() { 220.0 } else { 0.0 };
                ui.vertical(|ui| {
                    ui.set_width((ui.available_width() - reservado).max(220.0));
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label(rotulo("Próximo passo", 14.0, p.lime).family(tema::display()).extra_letter_spacing(2.2));
                    let texto = match &rival {
                        Some((exe, motivo)) => format!("O {} {motivo}. Pausado nos jogos, sobra mais para o {}.", c.dados.nome(exe), nome_do_jogo(r)),
                        None if boa => "Tudo certo: sobrou memória e processador. Nada a mudar.".to_owned(),
                        None => "Faltou memória, mas nenhum app grande pode ser pausado. Fechar abas do navegador antes de jogar ajuda.".to_owned(),
                    };
                    paragrafo(ui, RichText::new(texto).size(15.0).color(p.txt));
                });
                if let Some((exe, _)) = rival {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if botao_primario(ui, "Pausar nos jogos").clicked() {
                            pausar = Some(exe);
                        }
                    });
                }
            });
        });
    });
    if let Some(exe) = pausar {
        c.alternar_pausa(&exe);
    }
}

/// Memória livre (em GB) com brilho lime, a zona de travada abaixo de 1 GB
/// em vermelho e a marca tracejada.
fn grafico(ui: &mut Ui, amostras: &[Amostra]) {
    let p = paleta(ui);
    let duracao = amostras.last().map_or(0.0, |a| a.segundos);
    let (divisor, unidade) = if duracao < 120.0 { (1.0, "segundo") } else { (60.0, "minuto") };
    let xs: Vec<f64> = amostras.iter().map(|a| a.segundos / divisor).collect();
    let ys: Vec<f64> = amostras.iter().map(|a| a.ram_disponivel as f64 / GB).collect();
    let fim = xs.last().copied().unwrap_or(1.0);
    let maior = ys.iter().copied().fold(0.0, f64::max);
    let pontos = || -> PlotPoints<'static> { xs.iter().zip(&ys).map(|(x, y)| [*x, *y]).collect() };
    Plot::new("memoria_da_partida")
        .height(230.0)
        .show_background(false)
        .show_grid([false, true])
        .include_y(0.0)
        .include_y((maior * 1.15).max(2.0))
        .include_x(0.0)
        .allow_drag(false)
        .allow_zoom(false)
        .allow_scroll(false)
        .allow_boxed_zoom(false)
        .allow_double_click_reset(false)
        .x_axis_label(format!("{unidade}s de partida"))
        .x_axis_formatter(|marca, _| decimal(marca.value, 0))
        .y_axis_formatter(|marca, _| format!("{} GB", decimal(marca.value, 0)))
        .label_formatter(|posicao| match posicao {
            HoverPosition::NearDataPoint { plot_name, position, .. } if *plot_name == "Memória livre" => {
                Some(format!("{} {}: {} GB livres", unidade, decimal(position.x, 0), decimal(position.y, 1)))
            }
            _ => None,
        })
        .show(ui, |plot| {
            plot.polygon(
                Polygon::new("Zona de travada", PlotPoints::new(vec![[0.0, 0.0], [fim, 0.0], [fim, 1.0], [0.0, 1.0]]))
                    .fill_color(p.perigo.gamma_multiply(0.12))
                    .stroke(Stroke::NONE)
                    .allow_hover(false),
            );
            plot.hline(HLine::new("Limite", 1.0).color(p.perigo).width(1.5).style(LineStyle::dashed_loose()));
            plot.text(
                Text::new("rotulo_zona", PlotPoint::new(fim, 0.5), RichText::new("ZONA DE TRAVADA · ABAIXO DE 1 GB").family(tema::rotulo()).size(11.0).color(p.perigo))
                    .anchor(Align2::RIGHT_CENTER)
                    .color(p.perigo),
            );
            plot.line(Line::new("Brilho", pontos()).color(p.lime.gamma_multiply(0.25)).width(7.0).allow_hover(false));
            plot.line(Line::new("Memória livre", pontos()).color(p.lime).width(2.5).fill(0.0).fill_alpha(0.1));
        });
}
