//! Memória: um anel com o que sobra para o jogo, quem está usando (com
//! "Pausar no jogo" na própria linha) e o que abre com o Windows. O
//! técnico fica escondido.

use std::f32::consts::{FRAC_PI_2, TAU};

use eframe::egui::{
    self, Align, Align2, Color32, FontId, Frame, Layout, Margin, Rect, Response, RichText, Sense, Shape, Stroke, Ui,
    Vec2, pos2, vec2,
};
use nucleo::formato::{decimal, mb};
use nucleo::modo_jogo::protecao::Protecao;
use nucleo::ram::RelatorioRam;

use super::{Nivel, tamanho_curto, veredito_da_memoria};
use crate::app::Contexto;
use crate::componentes::{
    CORTE, CORTE_P, apoio, botao_secundario, brilho, cartao, cartao_com, chip, cortar, forma_cortada, forte, link,
    listras, medidor_em, misturar, nota, paragrafo, rotulo, separador, texto_espacado, titulo_da_pagina,
    titulo_da_secao,
};
use crate::icones::{self, Icone};
use crate::tema::{self, paleta};

#[derive(Default)]
pub struct TelaMemoria {
    ver_todos: bool,
    detalhes: bool,
}

impl TelaMemoria {
    pub fn ui(&mut self, ui: &mut Ui, c: &mut Contexto) {
        let Some(r) = c.dados.ram.clone() else {
            titulo_da_pagina(ui, "Lendo…", "Memória", "Lendo a memória…");
            ui.spinner();
            return;
        };
        let m = &r.memoria;
        let reservada = m.instalada.map(|i| i.saturating_sub(m.total)).unwrap_or(0);
        let (chamada, subtitulo) = match m.instalada {
            Some(instalada) if reservada >= 256 << 20 => (
                format!("{} instalados", tamanho_curto(instalada)),
                format!(
                    "{} ficam para os programas. Os outros {} são da placa de vídeo integrada.",
                    tamanho_curto(m.total),
                    tamanho_curto(reservada)
                ),
            ),
            Some(instalada) => (format!("{} instalados", tamanho_curto(instalada)), "Toda ela fica para os programas.".to_owned()),
            None => (format!("{} para os programas", tamanho_curto(m.total)), "Quanto sobra para o jogo e quem está usando.".to_owned()),
        };
        titulo_da_pagina(ui, &chamada, "Memória", &subtitulo);
        agora(ui, &r);
        ui.add_space(22.0);
        self.quem_usa(ui, c, &r);
        ui.add_space(22.0);
        abrem_com_o_windows(ui, c);
        ui.add_space(22.0);
        self.detalhes_tecnicos(ui, &r);
    }

    fn quem_usa(&mut self, ui: &mut Ui, c: &mut Contexto, r: &RelatorioRam) {
        let p = paleta(ui);
        let protecao = Protecao::nova(&r.processos, None);
        let quantos = if self.ver_todos { 25 } else { 7 };
        let maior = r.apps.first().map_or(1, |a| a.ws_privado.max(1));
        let mut alternar = None;
        cartao_com(ui, Margin { left: 28, right: 28, top: 26, bottom: 10 }, |ui| {
            ui.horizontal(|ui| {
                titulo_da_secao(ui, "Quem está usando agora");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(nota(ui, "Pausado no jogo, o app devolve a memória e volta sozinho no fim."));
                });
            });
            ui.add_space(8.0);
            for (i, a) in r.apps.iter().take(quantos).enumerate() {
                separador(ui);
                let protegido = r
                    .processos
                    .iter()
                    .filter(|x| a.pids.contains(&x.pid))
                    .find_map(|x| protecao.motivo(x));
                let nome = c.dados.nome(&a.nome);
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 16.0;
                    ui.allocate_ui_with_layout(vec2(32.0, 38.0), Layout::left_to_right(Align::Center), |ui| {
                        ui.set_min_width(32.0);
                        ui.label(RichText::new(format!("{:02}", i + 1)).family(tema::display()).size(20.0).color(p.lime));
                    });
                    avatar(ui, &nome);
                    let largura = (ui.available_width() - (140.0 + 82.0 + 184.0 + 16.0 * 3.0)).max(160.0);
                    ui.vertical(|ui| {
                        ui.set_width(largura);
                        ui.spacing_mut().item_spacing.y = 0.0;
                        ui.label(forte(cortar(&nome, 40), 15.0)).on_hover_text(&a.nome);
                        let detalhe = if a.processos == 1 { a.nome.clone() } else { format!("{} · {} processos", a.nome, a.processos) };
                        ui.label(nota(ui, cortar(&detalhe, 50)));
                    });
                    // Da direita para a esquerda: botão, tamanho, medidor. Sempre alinhados.
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        match protegido {
                            Some(motivo) => {
                                ui.allocate_ui_with_layout(vec2(184.0, 42.0), Layout::left_to_right(Align::Center), |ui| {
                                    ui.set_min_width(184.0);
                                    ui.spacing_mut().item_spacing.x = 8.0;
                                    ui.add_space(26.0);
                                    icones::icone(ui, Icone::Cadeado, 16.0, p.aviso);
                                    ui.label(rotulo("Protegido", 13.0, p.aviso).family(tema::display()))
                                        .on_hover_text(format!("O Makeway nunca mexe nele: {}", motivo.descricao()));
                                });
                            }
                            None => {
                                if botao_pausar(ui, c.pausado_nos_jogos(&a.nome)).clicked() {
                                    alternar = Some(a.nome.clone());
                                }
                            }
                        }
                        ui.allocate_ui_with_layout(vec2(82.0, 30.0), Layout::right_to_left(Align::Center), |ui| {
                            ui.set_min_width(82.0);
                            ui.label(RichText::new(tamanho_curto(a.ws_privado)).family(tema::display()).size(20.0).color(p.txt));
                        });
                        medidor_em(ui, 140.0, a.ws_privado as f32 / maior as f32, 10, 10.0, 2.0, p.ciano, false);
                    });
                });
                ui.add_space(10.0);
            }
            separador(ui);
            ui.add_space(8.0);
            let resto = &r.apps[quantos.min(r.apps.len())..];
            ui.horizontal(|ui| {
                let soma: u64 = resto.iter().map(|a| a.ws_privado).sum();
                ui.label(apoio(ui, format!("E mais {} apps e serviços, somando {}", resto.len(), tamanho_curto(soma))).size(14.0));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let texto = if self.ver_todos { "Ver menos" } else { "Ver todos" };
                    if link(ui, texto).clicked() {
                        self.ver_todos = !self.ver_todos;
                    }
                });
            });
            ui.add_space(4.0);
        });
        if let Some(exe) = alternar {
            c.alternar_pausa(&exe);
        }
    }

    fn detalhes_tecnicos(&mut self, ui: &mut Ui, r: &RelatorioRam) {
        let p = paleta(ui);
        let m = &r.memoria;
        cartao_com(ui, Margin::symmetric(28, 4), |ui| {
            let (rect, resposta) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 72.0), Sense::click());
            if resposta.hovered() {
                ui.painter().rect_filled(rect.expand2(vec2(28.0, 0.0)), 0.0, p.sup2);
            }
            texto_espacado(ui.painter(), pos2(rect.left(), rect.top() + 25.0), Align2::LEFT_CENTER, "DETALHES TÉCNICOS", FontId::new(16.0, tema::display()), 1.9, p.txt);
            ui.painter().text(
                pos2(rect.left(), rect.top() + 48.0),
                Align2::LEFT_CENTER,
                "Memória confirmada, arquivo de paginação, cache e pools do sistema",
                FontId::proportional(14.0),
                p.txt2,
            );
            let seta = Rect::from_center_size(pos2(rect.right() - 10.0, rect.center().y), Vec2::splat(20.0));
            icones::pintar(ui.painter(), seta, if self.detalhes { Icone::ChevronBaixo } else { Icone::Chevron }, p.lime);
            if resposta.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                self.detalhes = !self.detalhes;
            }
            if !self.detalhes {
                return;
            }
            separador(ui);
            ui.add_space(14.0);
            let mut itens: Vec<(&str, String)> = vec![
                ("Confirmada", format!("{} / {}", tamanho_curto(m.confirmada), tamanho_curto(m.limite_confirmada))),
                ("Em uso (Gerenciador)", format!("{}%", m.carga_pct)),
            ];
            for a in &r.paginacao {
                itens.push(("Paginação", format!("{} / {}", tamanho_curto(a.em_uso), tamanho_curto(a.tamanho))));
                itens.push(("Pico da paginação", tamanho_curto(a.pico)));
            }
            if let Some(l) = &m.listas {
                itens.push(("Em cache", tamanho_curto(l.standby)));
                itens.push(("Modificada / livre", format!("{} / {}", tamanho_curto(l.modificada), tamanho_curto(l.livre))));
            }
            itens.push(("Pools (pag. / não pag.)", format!("{} / {}", mb(m.pool_paginado), mb(m.pool_nao_paginado))));
            itens.push(("Processos abertos", decimal(r.processos.len() as f64, 0)));
            for linha in itens.chunks(4) {
                ui.columns(4, |colunas| {
                    for (coluna, (nome, valor)) in colunas.iter_mut().zip(linha) {
                        coluna.spacing_mut().item_spacing.y = 2.0;
                        coluna.label(rotulo(*nome, 12.0, p.txt3));
                        coluna.label(RichText::new(valor).family(tema::display()).size(20.0).color(p.txt));
                    }
                });
                ui.add_space(12.0);
            }
            ui.add_space(8.0);
        });
    }
}

/// O cartão de cima: anel com o que está livre e o veredito ao lado.
fn agora(ui: &mut Ui, r: &RelatorioRam) {
    let p = paleta(ui);
    let m = &r.memoria;
    let (_, frase, nivel) = veredito_da_memoria(m.disponivel);
    let cor = match nivel {
        Nivel::Bom => p.lime,
        Nivel::Atencao => p.aviso,
        Nivel::Ruim => p.perigo,
    };
    let (destaque, resto) = match nivel {
        Nivel::Bom => ("Sobra", " memória para jogar"),
        Nivel::Atencao => ("Apertada:", " dá para jogar"),
        Nivel::Ruim => ("Pouca", " memória livre"),
    };
    let explicacao = match nivel {
        Nivel::Bom => format!("Com {} disponíveis, a maioria dos jogos roda sem precisar usar o disco como memória.", tamanho_curto(m.disponivel)),
        _ => format!("{frase} Pausar apps nos jogos ajuda: veja abaixo quem está usando mais."),
    };
    let em_uso = m.total.saturating_sub(m.disponivel);

    let fundo = ui.painter().add(Shape::Noop);
    let textura = ui.painter().add(Shape::Noop);
    let resposta = Frame::new().inner_margin(Margin::symmetric(32, 30)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 36.0;
            anel(ui, em_uso, m.disponivel, cor);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 14.0;
                let mut job = egui::text::LayoutJob::default();
                let formato = |c: Color32| egui::TextFormat { font_id: FontId::new(36.0, tema::display()), color: c, extra_letter_spacing: 0.4, ..Default::default() };
                job.append(&destaque.to_uppercase(), 0.0, formato(cor));
                job.append(&resto.to_uppercase(), 0.0, formato(p.txt));
                job.wrap.max_width = ui.available_width();
                ui.add(egui::Label::new(job));
                paragrafo(ui, apoio(ui, explicacao));
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 34.0;
                    legenda_do_anel(ui, p.series[0], &tamanho_curto(em_uso), "em uso pelos programas");
                    legenda_do_anel(ui, cor, &tamanho_curto(m.disponivel), "disponível para o jogo, cache incluso");
                });
            });
        });
    });
    let rect = resposta.response.rect;
    ui.painter().set(fundo, forma_cortada(rect, CORTE, p.sup, Stroke::NONE));
    let direita = Rect::from_min_max(pos2(rect.left() + rect.width() * 0.6, rect.top()), rect.max);
    ui.painter().with_clip_rect(rect).set(textura, Shape::Vec(listras(direita, rect.right() - CORTE, p.lime.gamma_multiply(0.06))));
}

/// Anel de 220 px: em uso (cor de dado) e livre (com brilho), a partir do topo.
fn anel(ui: &mut Ui, em_uso: u64, livre: u64, cor_livre: Color32) {
    let p = paleta(ui);
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(220.0), Sense::hover());
    let centro = rect.center();
    let raio = 90.0;
    let total = (em_uso + livre).max(1) as f32;
    let folga = 3.0 / raio; // ~3 px entre as partes
    let arco = |inicio: f32, fim: f32, largura: f32, cor: Color32| -> Shape {
        let passos = (((fim - inicio) / TAU) * 120.0).ceil().max(2.0) as usize;
        let pontos = (0..=passos)
            .map(|i| {
                let a = inicio + (fim - inicio) * i as f32 / passos as f32 - FRAC_PI_2;
                centro + raio * Vec2::angled(a)
            })
            .collect();
        Shape::line(pontos, Stroke::new(largura, cor))
    };
    let pintor = ui.painter();
    pintor.circle_stroke(centro, raio, Stroke::new(18.0, p.sup3));
    let fim_uso = TAU * em_uso as f32 / total;
    if em_uso > 0 {
        pintor.add(arco(0.0, (fim_uso - folga).max(0.0), 18.0, p.series[0]));
    }
    if livre > 0 {
        let (a, b) = (fim_uso, TAU - folga);
        pintor.add(arco(a, b, 30.0, cor_livre.gamma_multiply(0.08)));
        pintor.add(arco(a, b, 24.0, cor_livre.gamma_multiply(0.12)));
        pintor.add(arco(a, b, 18.0, cor_livre));
    }
    let (numero, unidade) = tamanho_curto(livre).rsplit_once(' ').map(|(n, u)| (n.to_owned(), u.to_owned())).unwrap_or_default();
    pintor.text(centro + vec2(0.0, -10.0), Align2::CENTER_CENTER, numero, FontId::new(56.0, tema::display()), cor_livre);
    texto_espacado(pintor, centro + vec2(0.0, 30.0), Align2::CENTER_CENTER, &format!("{unidade} LIVRES"), FontId::new(13.0, tema::rotulo()), 2.3, p.txt2);
}

fn legenda_do_anel(ui: &mut Ui, cor: Color32, valor: &str, texto: &str) {
    let p = paleta(ui);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        let (rect, _) = ui.allocate_exact_size(vec2(14.0, 22.0), Sense::hover());
        ui.painter().rect_filled(Rect::from_min_size(rect.min + vec2(0.0, 5.0), Vec2::splat(14.0)), 0.0, cor);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.label(RichText::new(valor).family(tema::display()).size(20.0).color(p.txt));
            ui.label(nota(ui, texto));
        });
    });
}

fn abrem_com_o_windows(ui: &mut Ui, c: &Contexto) {
    let p = paleta(ui);
    cartao(ui, |ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.set_width((ui.available_width() - 240.0).max(240.0));
                ui.spacing_mut().item_spacing.y = 2.0;
                titulo_da_secao(ui, "Abrem com o Windows");
                let texto = match &c.dados.inicializacao {
                    Some(lista) => {
                        let desativados = lista.iter().filter(|x| !x.ativo).count();
                        if desativados > 0 {
                            format!("Cada um ocupa memória desde que o PC liga. Outros {desativados} você já desativou.")
                        } else {
                            "Cada um ocupa memória desde que o PC liga.".to_owned()
                        }
                    }
                    None => "Lendo…".to_owned(),
                };
                paragrafo(ui, apoio(ui, texto).size(14.0));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if botao_secundario(ui, "Escolher no Windows").clicked() {
                    nucleo::plataforma::abrir_configuracoes("ms-settings:startupapps");
                }
            });
        });
        ui.add_space(8.0);
        if let Some(lista) = &c.dados.inicializacao {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
                for programa in lista.iter().filter(|x| x.ativo) {
                    chip(ui, &programa.nome, p.txt);
                }
            });
        }
    });
}

/// Botão da linha: "PAUSAR NO JOGO" ou, já na lista, lime com um check.
fn botao_pausar(ui: &mut Ui, pausado: bool) -> Response {
    let p = paleta(ui);
    let (rect, resposta) = ui.allocate_exact_size(vec2(184.0, 42.0), Sense::click());
    let (mut fundo, cor) = if pausado { (p.lime, p.na_lime) } else { (p.sup3, p.txt) };
    if resposta.hovered() {
        fundo = misturar(fundo, Color32::WHITE, 0.08);
    }
    if pausado {
        brilho(ui.painter(), rect, CORTE_P, p.lime, 0.8);
    }
    ui.painter().add(forma_cortada(rect, CORTE_P, fundo, Stroke::NONE));
    let fonte = FontId::new(13.0, tema::display());
    if pausado {
        let texto = texto_espacado(ui.painter(), rect.center() - vec2(10.0, 0.0), Align2::CENTER_CENTER, "PAUSADO NO JOGO", fonte, 1.8, cor);
        let check = Rect::from_center_size(pos2(texto.right() + 12.0, rect.center().y), Vec2::splat(14.0));
        icones::pintar(ui.painter(), check, Icone::CheckSimples, cor);
    } else {
        texto_espacado(ui.painter(), rect.center(), Align2::CENTER_CENTER, "PAUSAR NO JOGO", fonte, 1.8, cor);
    }
    let dica = if pausado { "Na lista: é pausado enquanto você joga. Clique para tirar." } else { "Pausar este app enquanto você joga" };
    resposta.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, true, pausado, "Pausar no jogo"));
    resposta.on_hover_text(dica).on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Quadradinho de cantos cortados com a inicial do app.
fn avatar(ui: &mut Ui, nome: &str) {
    let p = paleta(ui);
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(38.0), Sense::hover());
    ui.painter().add(forma_cortada(rect, CORTE_P, p.sup3, Stroke::NONE));
    let inicial: String = nome.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default();
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, inicial, FontId::new(18.0, tema::display()), p.txt);
}
