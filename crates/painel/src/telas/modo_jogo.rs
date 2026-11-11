//! Modo jogo: o botão de energia, os dois modos em cartões grandes, os
//! jogos reconhecidos e o que acontece enquanto você joga. Cada mudança é
//! salva na hora.

use eframe::egui::{self, Align, Align2, Color32, FontId, Frame, Layout, Margin, Rect, Response, Sense, Shape, Stroke, Ui, Vec2, pos2, vec2};
use nucleo::config::{ConfigModoJogo, Modo, Prioridade};

use super::{apps_para_escolher, tamanho_curto};
use crate::app::Contexto;
use crate::componentes::{
    CORTE, CORTE_P, apoio, bloco_icone, botao_adicionar, botao_secundario, brilho, cartao, cartao_com, chip_removivel, cortar, display, etiqueta, faixa, forma_cortada, forte, listras, moldura, nota, paragrafo, rotulo,
    separador, texto_espacado, titulo_da_secao,
};
use crate::icones::{self, Icone};
use crate::tema::{self, paleta};

#[derive(Default)]
pub struct TelaModoJogo {
    /// Mudança feita com o mouse ainda apertado (ex.: arrastando o slider):
    /// só grava quando soltar.
    pendente: Option<ConfigModoJogo>,
    escolhendo_jogo: bool,
    escolhendo_pausar: bool,
    escolhendo_fechar: bool,
    avancado: bool,
    texto_jogo: String,
    texto_ignorar: String,
    texto_pasta: String,
}

impl TelaModoJogo {
    pub fn ui(&mut self, ui: &mut Ui, c: &mut Contexto) {
        let ctx = ui.ctx().clone();
        let base = self.pendente.clone().unwrap_or_else(|| c.config.modo_jogo.clone());
        let mut m = base.clone();

        cabecalho(ui, &mut m);
        ui.add_space(8.0);
        modos(ui, &mut m);
        ui.add_space(22.0);
        self.seus_jogos(ui, c, &mut m);
        ui.add_space(22.0);
        self.enquanto_joga(ui, c, &mut m);
        ui.add_space(22.0);
        seguranca(ui);
        ui.add_space(22.0);
        self.mais_opcoes(ui, c, &ctx, &mut m);

        if m != base {
            self.pendente = Some(m);
        }
        let solto = !ui.input(|i| i.pointer.any_down());
        if solto && let Some(novo) = self.pendente.take() && novo != c.config.modo_jogo {
            c.salvar_modo_jogo(novo, "Salvo. O vigia usa a partir da próxima partida");
        }
    }

    fn seus_jogos(&mut self, ui: &mut Ui, c: &Contexto, m: &mut ConfigModoJogo) {
        let p = paleta(ui);
        let instalados = c.dados.jogos.as_deref().unwrap_or_default();
        let mostrados = if m.detectar_pelas_pastas { instalados.len() } else { 0 } + m.jogos.len();
        cartao(ui, |ui| {
            if !m.ativo {
                ui.multiply_opacity(0.45);
            }
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 14.0;
                titulo_da_secao(ui, "Seus jogos");
                if mostrados > 0 {
                    let texto = if mostrados == 1 { "1 encontrado".to_owned() } else { format!("{mostrados} encontrados") };
                    ui.label(rotulo(texto, 13.0, p.lime));
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let texto = if self.escolhendo_jogo { "Fechar" } else { "+ Adicionar jogo" };
                    if botao_secundario(ui, texto).clicked() {
                        self.escolhendo_jogo = !self.escolhendo_jogo;
                    }
                });
            });
            let texto = if m.detectar_pelas_pastas {
                "Reconhecidos sozinhos pelas pastas da Steam, Epic, Riot, Xbox e Ubisoft."
            } else {
                "Reconhecer pelas lojas está desligado em Avançado: só os jogos adicionados aqui contam."
            };
            ui.label(apoio(ui, texto).size(14.0));
            ui.add_space(6.0);

            let mut blocos: Vec<(String, String, Option<usize>)> = Vec::new();
            if m.detectar_pelas_pastas {
                blocos.extend(instalados.iter().map(|j| (j.nome.clone(), j.loja.to_string(), None)));
            }
            blocos.extend(m.jogos.iter().enumerate().map(|(i, exe)| (exe.trim_end_matches(".exe").to_owned(), "Adicionado".to_owned(), Some(i))));
            let tirar = grade_de_jogos(ui, &blocos);
            if let Some(i) = tirar {
                m.jogos.remove(i);
            }
            if c.dados.jogos.is_none() {
                ui.spinner();
            } else if blocos.is_empty() {
                ui.label(nota(ui, "Nenhum jogo ainda. Abra o seu jogo e adicione ele pelo botão acima."));
            }
            if self.escolhendo_jogo {
                ui.add_space(6.0);
                ui.label(nota(ui, "Abra o jogo e escolha ele na lista, ou digite o nome do .exe (Gerenciador de Tarefas > Detalhes)."));
                if let Some(exe) = campo_de_adicionar(ui, &mut self.texto_jogo, "ex.: VALORANT-Win64-Shipping.exe") {
                    adicionar(&mut m.jogos, exe);
                }
                if let Some(exe) = escolher_entre_abertos(ui, c, &m.jogos) {
                    adicionar(&mut m.jogos, exe);
                    self.escolhendo_jogo = false;
                }
            }
        });
    }

    fn enquanto_joga(&mut self, ui: &mut Ui, c: &Contexto, m: &mut ConfigModoJogo) {
        let observando = m.modo == Modo::Observar || !m.ativo;
        cartao_com(ui, Margin { left: 28, right: 28, top: 26, bottom: 14 }, |ui| {
            if observando {
                ui.multiply_opacity(0.55);
            }
            titulo_da_secao(ui, "Enquanto você joga");
            let ajuda = if observando {
                "Só vale no modo Otimizar. Dá para deixar tudo pronto antes de ligar."
            } else {
                "Tudo isso é desfeito quando o jogo fecha."
            };
            ui.add_space(-6.0);
            ui.label(apoio(ui, ajuda).size(14.0));
            ui.add_space(2.0);

            let mut prioridade = m.prioridade != Prioridade::Normal;
            if linha_com_chave(ui, Icone::SetaCima, "Prioridade máxima para o jogo", "O Windows atende o jogo antes dos outros programas.", &mut prioridade) {
                m.prioridade = if prioridade { Prioridade::Alta } else { Prioridade::Normal };
            }
            separador(ui);
            linha_com_chave(
                ui,
                Icone::Raio,
                "Energia no máximo",
                "Usa o plano Alto desempenho do Windows. No notebook, só na tomada.",
                &mut m.plano_alto_desempenho,
            );
            separador(ui);
            linha_com_chave(
                ui,
                Icone::Chip,
                "Memória primeiro para o jogo",
                "Os apps que continuam abertos ficam em segundo plano na memória: quando ela aperta, o Windows tira deles e não do jogo. Eles continuam funcionando.",
                &mut m.memoria_baixa_nos_apps,
            );
            separador(ui);
            ui.add_space(16.0);
            let p = paleta(ui);
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 16.0;
                bloco_icone(ui, Icone::Pausa, p.lime, p.sup3, 42.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label(forte("Pausar apps", 16.0));
                    paragrafo(
                        ui,
                        apoio(ui, "Ficam parados e devolvem a memória para o jogo. Voltam sozinhos quando você fecha o jogo. Um app pausado não faz nada, nem chamada de voz.").size(14.0),
                    );
                    ui.add_space(10.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
                        let mut tirar = None;
                        for (i, exe) in m.pausar.iter().enumerate() {
                            if chip_removivel(ui, &c.dados.nome(exe), exe) {
                                tirar = Some(i);
                            }
                        }
                        if let Some(i) = tirar {
                            m.pausar.remove(i);
                        }
                        let texto = if self.escolhendo_pausar { "Fechar a lista" } else { "+ Adicionar app" };
                        if botao_adicionar(ui, texto).clicked() {
                            self.escolhendo_pausar = !self.escolhendo_pausar;
                        }
                    });
                    if self.escolhendo_pausar && let Some(exe) = escolher_entre_abertos(ui, c, &m.pausar) {
                        adicionar(&mut m.pausar, exe);
                        self.escolhendo_pausar = false;
                    }
                });
            });
            ui.add_space(12.0);
        });
    }

    fn mais_opcoes(&mut self, ui: &mut Ui, c: &mut Contexto, ctx: &egui::Context, m: &mut ConfigModoJogo) {
        let p = paleta(ui);
        let mut mudar_windows = None;
        cartao_com(ui, Margin::symmetric(26, 6), |ui| {
            let mut com_windows = c.dados.com_windows.unwrap_or(false);
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.set_width((ui.available_width() - 82.0).max(240.0));
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label(forte("Abrir com o Windows", 16.0));
                    paragrafo(
                        ui,
                        apoio(ui, "O Windows pede sua confirmação uma vez. Depois o vigia já fica pronto antes de você abrir o jogo.").size(14.0),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if c.dados.mudando_com_windows() {
                        ui.spinner();
                    } else if crate::componentes::chave(ui, &mut com_windows).changed() {
                        mudar_windows = Some(com_windows);
                    }
                });
            });
            ui.add_space(12.0);
            separador(ui);

            // Cabeçalho clicável de "Avançado".
            let (rect, resposta) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 72.0), Sense::click());
            if resposta.hovered() {
                ui.painter().rect_filled(rect.expand2(vec2(26.0, 0.0)), 0.0, p.sup2);
            }
            texto_espacado(ui.painter(), pos2(rect.left(), rect.top() + 25.0), Align2::LEFT_CENTER, "AVANÇADO", FontId::new(16.0, tema::display()), 1.9, p.txt);
            ui.painter().text(
                pos2(rect.left(), rect.top() + 48.0),
                Align2::LEFT_CENTER,
                "Fechar apps, exceções, pastas de jogos e de quanto em quanto tempo procurar",
                FontId::proportional(14.0),
                p.txt2,
            );
            let seta = Rect::from_center_size(pos2(rect.right() - 10.0, rect.center().y), Vec2::splat(20.0));
            let icone = if self.avancado { Icone::ChevronBaixo } else { Icone::Chevron };
            icones::pintar(ui.painter(), seta, icone, p.lime);
            if resposta.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                self.avancado = !self.avancado;
            }
            if self.avancado {
                separador(ui);
                self.avancado(ui, c, m);
                ui.add_space(14.0);
            }
        });
        if let Some(ligar) = mudar_windows {
            c.mudar_com_windows(ctx, ligar);
        }
    }

    fn avancado(&mut self, ui: &mut Ui, c: &Contexto, m: &mut ConfigModoJogo) {
        linha_com_chave(
            ui,
            Icone::Controle,
            "Reconhecer jogos pelas lojas",
            "Conta como jogo o que roda das pastas da Steam, Epic, Riot, Xbox e Ubisoft.",
            &mut m.detectar_pelas_pastas,
        );
        separador(ui);
        linha_com_chave(
            ui,
            Icone::Raio,
            "Energia no máximo só na tomada",
            "No notebook, não troca o plano de energia quando estiver na bateria.",
            &mut m.plano_somente_na_tomada,
        );
        separador(ui);
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            ui.label(forte("Procurar o jogo a cada", 15.0));
            ui.add(egui::Slider::new(&mut m.intervalo_segundos, 1..=10).suffix(" s"));
        });
        ui.label(nota(ui, "Com o vigia aberto com o Windows, ele é avisado na hora que um jogo abre e isso quase não importa."));
        ui.add_space(14.0);
        separador(ui);
        ui.add_space(14.0);

        ui.label(forte("Fechar quando o jogo abrir", 15.0));
        ui.label(apoio(ui, "Como clicar no X. Eles não reabrem sozinhos no fim.").size(14.0));
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
            let mut tirar = None;
            for (i, exe) in m.fechar.iter().enumerate() {
                if chip_removivel(ui, &c.dados.nome(exe), exe) {
                    tirar = Some(i);
                }
            }
            if let Some(i) = tirar {
                m.fechar.remove(i);
            }
            let texto = if self.escolhendo_fechar { "Fechar a lista" } else { "+ Adicionar app" };
            if botao_adicionar(ui, texto).clicked() {
                self.escolhendo_fechar = !self.escolhendo_fechar;
            }
        });
        if self.escolhendo_fechar && let Some(exe) = escolher_entre_abertos(ui, c, &m.fechar) {
            adicionar(&mut m.fechar, exe);
            self.escolhendo_fechar = false;
        }
        ui.add_space(14.0);

        ui.label(forte("Nunca considerar jogo", 15.0));
        ui.label(apoio(ui, "Para um programa pesado instalado numa pasta de jogos que não é jogo.").size(14.0));
        lista_de_texto(ui, &mut m.ignorar, &mut self.texto_ignorar, "nome do .exe");
        ui.add_space(14.0);

        ui.label(forte("Pastas de jogos", 15.0));
        ui.label(apoio(ui, "Um pedaço do caminho, por exemplo D:\\Jogos\\. As pastas das lojas já estão aqui.").size(14.0));
        lista_de_texto(ui, &mut m.pastas_de_jogos, &mut self.texto_pasta, "pedaço do caminho");
    }
}

/// Título da página e, à direita, o botão grande de ligar/desligar.
fn cabecalho(ui: &mut Ui, m: &mut ConfigModoJogo) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_width((ui.available_width() - 240.0).max(320.0));
            crate::componentes::titulo_da_pagina(
                ui,
                "Configuração",
                "Modo jogo",
                "Quando um jogo abre, o Makeway tira do caminho o que compete com ele. Quando o jogo fecha, tudo volta a ser como era.",
            );
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if botao_de_energia(ui, m.ativo).clicked() {
                m.ativo = !m.ativo;
            }
        });
    });
}

/// Botão grande de energia: lime e brilhando quando ligado.
fn botao_de_energia(ui: &mut Ui, ligado: bool) -> Response {
    let p = paleta(ui);
    let (rect, resposta) = ui.allocate_exact_size(vec2(210.0, 66.0), Sense::click());
    let (mut fundo, cor) = if ligado { (p.lime, p.na_lime) } else { (p.sup3, p.txt2) };
    if resposta.hovered() {
        fundo = crate::componentes::misturar(fundo, Color32::WHITE, 0.08);
    }
    if ligado {
        brilho(ui.painter(), rect, CORTE, p.lime, 1.4);
    }
    ui.painter().add(forma_cortada(rect, CORTE, fundo, Stroke::NONE));
    icones::pintar(ui.painter(), Rect::from_center_size(pos2(rect.left() + 22.0 + 13.0, rect.center().y), Vec2::splat(26.0)), Icone::Energia, cor);
    let x = rect.left() + 22.0 + 26.0 + 14.0;
    texto_espacado(
        ui.painter(),
        pos2(x, rect.center().y - 9.0),
        Align2::LEFT_CENTER,
        if ligado { "LIGADO" } else { "DESLIGADO" },
        FontId::new(22.0, tema::display()),
        2.6,
        cor,
    );
    ui.painter().text(
        pos2(x, rect.center().y + 13.0),
        Align2::LEFT_CENTER,
        if ligado { "clique para desligar" } else { "clique para ligar" },
        FontId::proportional(12.0),
        cor.gamma_multiply(0.8),
    );
    if resposta.has_focus() {
        ui.painter().add(forma_cortada(rect.expand(3.0), CORTE + 2.0, Color32::TRANSPARENT, Stroke::new(2.0, p.lime)));
    }
    resposta.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, true, ligado, "Modo jogo"));
    resposta.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Os dois modos lado a lado, em cartões grandes.
fn modos(ui: &mut Ui, m: &mut ConfigModoJogo) {
    let ligado = m.ativo;
    ui.spacing_mut().item_spacing.x = 20.0;
    ui.columns(2, |colunas| {
        let definicoes = [
            (
                Modo::Observar,
                Icone::Olho,
                "Observar",
                "Anota como foi cada partida. Bom para as primeiras.",
                ["Anota memória, processador e travadas", "Não muda nada no seu PC", "Ideal para as 3 primeiras partidas"],
            ),
            (
                Modo::Otimizar,
                Icone::Raio,
                "Otimizar",
                "Tira do caminho o que compete com o jogo.",
                ["Prioridade máxima para o jogo", "Pausa os apps da sua lista", "Desfaz tudo quando o jogo fecha"],
            ),
        ];
        for (coluna, (modo, icone, nome, resumo, itens)) in colunas.iter_mut().zip(definicoes) {
            if !ligado {
                coluna.multiply_opacity(0.45);
            }
            if cartao_de_modo(coluna, m.modo == modo, icone, nome, resumo, &itens).clicked() {
                m.modo = modo;
            }
        }
    });
}

fn cartao_de_modo(ui: &mut Ui, escolhido: bool, icone: Icone, nome: &str, resumo: &str, itens: &[&str]) -> Response {
    let p = paleta(ui);
    let borda = ui.painter().add(Shape::Noop);
    let resposta = moldura(ui, p.sup, Stroke::NONE, CORTE, Margin::symmetric(26, 24), |ui| {
        ui.set_min_height(184.0);
        ui.spacing_mut().item_spacing.y = 10.0;
        ui.horizontal(|ui| {
            let (fundo, cor) = if escolhido { (p.lime, p.na_lime) } else { (p.sup3, p.txt3) };
            bloco_icone(ui, icone, cor, fundo, 46.0);
            if escolhido {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    etiqueta(ui, "Selecionado", p.lime, p.na_lime);
                });
            }
        });
        ui.label(display(nome, 28.0).extra_letter_spacing(1.7));
        ui.add_space(-6.0);
        ui.label(apoio(ui, resumo).size(14.0));
        ui.spacing_mut().item_spacing.y = 6.0;
        // Linhas baixas: sem a altura mínima de 36 px dos controles.
        ui.spacing_mut().interact_size.y = 0.0;
        for item in itens {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
                icones::pintar(ui.painter(), rect, Icone::CheckSimples, if escolhido { p.lime } else { p.txt3 });
                ui.label(egui::RichText::new(*item).size(14.0).color(p.txt));
            });
        }
    })
    .response;
    // Contorno de 2 px por fora (lime e com brilho quando escolhido).
    let fora = resposta.rect.expand(2.0);
    let contorno = if escolhido {
        let mut formas = Vec::new();
        for (i, alfa) in [0.08, 0.05, 0.03].iter().enumerate() {
            formas.push(forma_cortada(fora.expand(4.0 + i as f32 * 5.0), CORTE + 3.0 + i as f32 * 2.0, p.lime.gamma_multiply(*alfa), Stroke::NONE));
        }
        formas.push(forma_cortada(fora, CORTE + 1.0, p.lime, Stroke::NONE));
        Shape::Vec(formas)
    } else {
        forma_cortada(fora, CORTE + 1.0, p.borda, Stroke::NONE)
    };
    ui.painter().set(borda, contorno);
    let clique = ui.interact(resposta.rect, resposta.id.with("modo"), Sense::click());
    if clique.hovered() && !escolhido {
        ui.painter().add(forma_cortada(fora, CORTE + 1.0, Color32::TRANSPARENT, Stroke::new(2.0, p.borda2)));
    }
    clique.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Os jogos em blocos de 190 px ou mais, com a inicial gigante ao fundo.
/// Devolve o índice (em `ConfigModoJogo::jogos`) do jogo a tirar.
fn grade_de_jogos(ui: &mut Ui, jogos: &[(String, String, Option<usize>)]) -> Option<usize> {
    if jogos.is_empty() {
        return None;
    }
    let p = paleta(ui);
    let espaco = 10.0;
    let largura = ui.available_width();
    let colunas = ((largura + espaco) / (190.0 + espaco)).floor().max(1.0) as usize;
    let w = (largura - espaco * (colunas as f32 - 1.0)) / colunas as f32;
    let mut tirar = None;
    for linha in jogos.chunks(colunas) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = espaco;
            for (nome, loja, indice) in linha {
                let (rect, resposta) = ui.allocate_exact_size(vec2(w, 96.0), Sense::hover());
                let fundo = if resposta.hovered() { crate::componentes::misturar(p.sup3, Color32::WHITE, 0.04) } else { p.sup3 };
                ui.painter().add(forma_cortada(rect, CORTE_P, fundo, Stroke::NONE));
                let pintor = ui.painter().with_clip_rect(rect);
                pintor.extend(listras(rect, rect.right() - CORTE_P, p.lime.gamma_multiply(0.05)));
                let inicial = nome.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default();
                pintor.text(pos2(rect.right() - 8.0, rect.top() - 14.0), Align2::RIGHT_TOP, inicial, FontId::new(86.0, tema::display()), Color32::from_white_alpha(13));
                let galley = ui.painter().layout(cortar(&nome.to_uppercase(), 48), FontId::new(18.0, tema::display()), p.txt, w - 32.0);
                let y_nome = rect.bottom() - 14.0 - galley.size().y;
                texto_espacado(&pintor, pos2(rect.left() + 16.0, y_nome - 2.0), Align2::LEFT_BOTTOM, &loja.to_uppercase(), FontId::new(11.0, tema::rotulo()), 2.2, p.txt3);
                pintor.galley(pos2(rect.left() + 16.0, y_nome), galley, p.txt);
                if let Some(i) = indice {
                    let x = Rect::from_center_size(pos2(rect.right() - 20.0, rect.top() + 20.0), Vec2::splat(30.0));
                    let clique = ui.interact(x, ui.scope_id().with(("tirar_jogo", *i)), Sense::click());
                    icones::pintar(ui.painter(), x.shrink(8.0), Icone::Fechar, if clique.hovered() { p.perigo } else { p.txt3 });
                    if clique.on_hover_text("Tirar da lista").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        tirar = Some(*i);
                    }
                }
            }
        });
    }
    tirar
}

fn seguranca(ui: &mut Ui) {
    let p = paleta(ui);
    faixa(ui, p.lime_dim, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 16.0;
            icones::icone(ui, Icone::Escudo, 28.0, p.lime);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.label(rotulo("Seguro para jogos online", 15.0, p.lime).family(tema::display()).extra_letter_spacing(2.1));
                paragrafo(
                    ui,
                    apoio(ui, "Nunca mexe no jogo, no anti-cheat, no launcher nem no Windows. Se o PC desligar no meio, tudo é desfeito na próxima vez que ele ligar.").size(14.0),
                );
            });
        });
    });
}

/// Ícone + título + explicação + chave. Devolve `true` se a chave mudou.
fn linha_com_chave(ui: &mut Ui, icone: Icone, nome: &str, explicacao: &str, valor: &mut bool) -> bool {
    let p = paleta(ui);
    let mut mudou = false;
    ui.add_space(14.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 16.0;
        bloco_icone(ui, icone, p.lime, p.sup3, 42.0);
        ui.vertical(|ui| {
            ui.set_width((ui.available_width() - 82.0).max(200.0));
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.label(forte(nome, 16.0));
            paragrafo(ui, apoio(ui, explicacao).size(14.0));
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            mudou = crate::componentes::chave(ui, valor).changed();
        });
    });
    ui.add_space(14.0);
    mudou
}

/// Lista dos apps abertos agora (sem os protegidos), para escolher um.
fn escolher_entre_abertos(ui: &mut Ui, c: &Contexto, ja: &[String]) -> Option<String> {
    let p = paleta(ui);
    let Some(r) = &c.dados.ram else {
        ui.spinner();
        return None;
    };
    let opcoes = apps_para_escolher(r, ja, 10);
    let mut escolhido = None;
    ui.add_space(4.0);
    ui.scope(|ui| {
        ui.set_max_width(460.0);
        moldura(ui, p.bg, Stroke::new(1.0, p.borda), CORTE, Margin::same(10), |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.add_space(10.0);
                ui.label(rotulo("Abertos agora · mais memória primeiro", 12.0, p.txt3));
            });
            ui.add_space(4.0);
            for (exe, bytes) in opcoes {
                let (rect, resposta) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 44.0), Sense::click());
                if resposta.hovered() {
                    ui.painter().rect_filled(rect, 0.0, p.sup3);
                }
                ui.painter().text(pos2(rect.left() + 10.0, rect.center().y), Align2::LEFT_CENTER, c.dados.nome(&exe), FontId::proportional(14.5), p.txt);
                ui.painter().text(pos2(rect.right() - 10.0, rect.center().y), Align2::RIGHT_CENTER, tamanho_curto(bytes), FontId::new(14.5, tema::display()), p.ciano);
                if resposta.on_hover_text(&exe).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    escolhido = Some(exe);
                }
            }
        });
    });
    escolhido
}

/// Campo de texto + "Pôr". Devolve o texto quando confirmado.
fn campo_de_adicionar(ui: &mut Ui, texto: &mut String, dica: &str) -> Option<String> {
    let p = paleta(ui);
    let mut novo = None;
    ui.horizontal(|ui| {
        let campo = Frame::new().fill(p.bg).stroke(Stroke::new(1.0, p.borda2)).show(ui, |ui| {
            ui.add(egui::TextEdit::singleline(texto).hint_text(dica).desired_width(320.0).frame(Frame::NONE).margin(Margin::symmetric(12, 11)))
        });
        let campo = campo.inner;
        let enter = campo.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if (botao_secundario(ui, "Pôr").clicked() || enter) && !texto.trim().is_empty() {
            novo = Some(texto.trim().to_owned());
            texto.clear();
        }
    });
    novo
}

/// Pílulas removíveis + campo para pôr mais.
fn lista_de_texto(ui: &mut Ui, itens: &mut Vec<String>, texto: &mut String, dica: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
        let mut tirar = None;
        for (i, item) in itens.iter().enumerate() {
            if chip_removivel(ui, item, item) {
                tirar = Some(i);
            }
        }
        if let Some(i) = tirar {
            itens.remove(i);
        }
    });
    if let Some(novo) = campo_de_adicionar(ui, texto, dica) {
        adicionar(itens, novo);
    }
}

fn adicionar(lista: &mut Vec<String>, item: String) {
    if !lista.iter().any(|x| x.eq_ignore_ascii_case(&item)) {
        lista.push(item);
    }
}
