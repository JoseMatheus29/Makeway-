//! Liberar espaço: o que ocupa o disco (em categorias que qualquer um
//! entende), temporários, cópias repetidas e os maiores arquivos.
//! Só os temporários podem ser apagados, pela Lixeira e com confirmação.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use eframe::egui::{self, Align, Align2, Color32, FontId, Layout, Margin, Rect, RichText, Sense, Stroke, Ui, Vec2, pos2, vec2};
use nucleo::disco::{self, Contadores, Pasta, Varredura};
use nucleo::duplicados::{self, ResultadoDuplicados};
use nucleo::formato::decimal;

use super::tamanho_curto;
use crate::app::Contexto;
use crate::componentes::{
    apoio, barra, blocos_empilhados, botao_mostrar, botao_primario, botao_secundario, cartao, cartao_com, cortar,
    display, forte, medidor_em, nota, numero_grande, paragrafo, rotulo, separador, texto_espacado, titulo_da_pagina,
    titulo_da_secao,
};
use crate::icones::{self, Icone};
use crate::tarefa::{Tarefa, acompanhar};
use crate::tema::{self, paleta};

/// Pastas dentro de "Program Files" que são lojas de jogos.
const LOJAS: &[&str] = &["steam", "epic games", "ubisoft", "ea games", "gog galaxy", "riot games", "rockstar games", "battle.net"];

#[derive(Default)]
pub struct TelaEspaco {
    varredura: Option<Varredura>,
    tarefa_varredura: Option<Tarefa<Varredura>>,
    contadores: Arc<Contadores>,
    analisado_em: Option<Instant>,
    duplicados: Option<ResultadoDuplicados>,
    tarefa_duplicados: Option<Tarefa<ResultadoDuplicados>>,
    contadores_copias: Arc<Contadores>,
    revisar_copias: bool,
    pasta_por_pasta: bool,
    abertas: HashSet<PathBuf>,
}

impl TelaEspaco {
    pub fn ui(&mut self, ui: &mut Ui, c: &mut Contexto) {
        let ctx = ui.ctx().clone();
        let estava = self.tarefa_varredura.is_some();
        acompanhar(&mut self.tarefa_varredura, &mut self.varredura);
        if estava && self.tarefa_varredura.is_none() {
            self.analisado_em = Some(Instant::now());
        }
        acompanhar(&mut self.tarefa_duplicados, &mut self.duplicados);
        if self.tarefa_varredura.is_some() || self.tarefa_duplicados.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }

        let unidade = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
        let (livre, total) = c.dados.disco.unwrap_or((0, 0));
        cabecalho(ui, &unidade, livre, total);
        self.o_que_ocupa(ui, &ctx, &unidade, livre, total);
        ui.add_space(22.0);
        ui.columns(2, |colunas| {
            temporarios(&mut colunas[0], c);
            self.copias(&mut colunas[1], c, &ctx);
        });
        if self.revisar_copias && self.duplicados.is_some() {
            ui.add_space(22.0);
            self.lista_de_copias(ui);
        }
        if self.varredura.is_some() {
            ui.add_space(22.0);
            self.maiores_arquivos(ui);
            ui.add_space(22.0);
            self.pasta_por_pasta(ui);
        }
    }

    fn o_que_ocupa(&mut self, ui: &mut Ui, ctx: &egui::Context, unidade: &str, livre: u64, total: u64) {
        let p = paleta(ui);
        let usado = total.saturating_sub(livre);
        cartao(ui, |ui| {
            ui.horizontal(|ui| {
                titulo_da_secao(ui, "O que ocupa o disco");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if self.tarefa_varredura.is_some() {
                        if self.contadores.cancelado() {
                            ui.label(nota(ui, "cancelando…"));
                        } else if botao_secundario(ui, "Cancelar").clicked() {
                            self.contadores.cancelar();
                        }
                        ui.label(apoio(ui, format!("{} arquivos lidos…", decimal(self.contadores.arquivos_vistos() as f64, 0))));
                        ui.spinner();
                    } else {
                        let texto = if self.varredura.is_some() { "Analisar de novo" } else { "Analisar o disco" };
                        if botao_secundario(ui, texto).clicked() {
                            let contadores = Arc::new(Contadores::default());
                            self.contadores = contadores.clone();
                            let raiz = PathBuf::from(format!(r"{unidade}\"));
                            self.abertas.clear();
                            self.tarefa_varredura = Some(Tarefa::iniciar(ctx, move || disco::varrer(&raiz, 4, 12, &contadores)));
                        }
                    }
                });
            });
            ui.add_space(8.0);
            match &self.varredura {
                Some(v) => {
                    let [seus, jogos, windows, programas, outros] = categorias(&v.raiz, usado);
                    let s = p.series;
                    blocos_empilhados(
                        ui,
                        &[
                            (seus, s[0], format!("Seus arquivos: {}", tamanho_curto(seus))),
                            (jogos, s[1], format!("Jogos: {}", tamanho_curto(jogos))),
                            (windows, s[2], format!("Windows: {}", tamanho_curto(windows))),
                            (programas, s[3], format!("Programas: {}", tamanho_curto(programas))),
                            (outros, s[4], format!("Outros: {}", tamanho_curto(outros))),
                            (livre, p.livre, format!("Livre: {}", tamanho_curto(livre))),
                        ],
                        50,
                        30.0,
                    );
                    ui.add_space(10.0);
                    ui.columns(3, |col| {
                        for coluna in col.iter_mut() {
                            coluna.spacing_mut().item_spacing.y = 0.0;
                        }
                        linha_de_legenda(&mut col[0], s[0], "Seus arquivos", "Documentos, Downloads, projetos", seus);
                        linha_de_legenda(&mut col[1], s[1], "Jogos", "Steam, Epic, Riot, Xbox, Ubisoft", jogos);
                        linha_de_legenda(&mut col[2], s[2], "Windows", "O sistema em si", windows);
                        linha_de_legenda(&mut col[0], s[3], "Programas", "Apps instalados", programas);
                        linha_de_legenda(&mut col[1], s[4], "Outros", "Ferramentas, paginação, hibernação", outros);
                        linha_de_legenda(&mut col[2], p.livre, "Livre", "Pronto para usar", livre);
                    });
                    ui.add_space(6.0);
                    let quando = self.analisado_em.map_or(String::new(), |t| {
                        let min = t.elapsed().as_secs() / 60;
                        if min == 0 { "agora há pouco".into() } else { format!("há {min} min") }
                    });
                    let parcial = if v.cancelada { " A análise foi cancelada: os números estão incompletos." } else { "" };
                    ui.label(nota(ui, format!("Analisado {quando}: {} arquivos em {} s.{parcial}", decimal(v.raiz.arquivos as f64, 0), decimal(v.segundos, 0))));
                }
                None => {
                    blocos_empilhados(
                        ui,
                        &[
                            (usado, p.series[0], format!("Em uso: {}", tamanho_curto(usado))),
                            (livre, p.livre, format!("Livre: {}", tamanho_curto(livre))),
                        ],
                        50,
                        30.0,
                    );
                    ui.add_space(10.0);
                    ui.columns(2, |col| {
                        for coluna in col.iter_mut() {
                            coluna.spacing_mut().item_spacing.y = 0.0;
                        }
                        linha_de_legenda(&mut col[0], p.series[0], "Em uso", "Arquivos, jogos, programas e o Windows", usado);
                        linha_de_legenda(&mut col[1], p.livre, "Livre", "Pronto para usar", livre);
                    });
                    ui.add_space(6.0);
                    ui.label(nota(ui, "Analise o disco para ver o que ocupa espaço. Leva cerca de um minuto e não apaga nada."));
                }
            }
        });
    }

    fn copias(&mut self, ui: &mut Ui, c: &Contexto, ctx: &egui::Context) {
        let p = paleta(ui);
        cartao(ui, |ui| {
            ui.set_min_height(236.0);
            ui.spacing_mut().item_spacing.y = 12.0;
            ui.label(rotulo("Cópias repetidas", 12.0, p.txt3));
            if self.tarefa_duplicados.is_some() {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(apoio(ui, format!("{} arquivos lidos…", decimal(self.contadores_copias.arquivos_vistos() as f64, 0))));
                });
                if !self.contadores_copias.cancelado() && botao_secundario(ui, "Cancelar").clicked() {
                    self.contadores_copias.cancelar();
                }
                return;
            }
            let mut procurar = false;
            match &self.duplicados {
                None => {
                    ui.label(display("Procurar", 56.0).color(p.txt3));
                    paragrafo(
                        ui,
                        apoio(ui, "Arquivos iguais guardados em mais de um lugar na sua pasta de usuário. Leva cerca de um minuto e não apaga nada.").size(14.0),
                    );
                    procurar = botao_primario(ui, "Procurar cópias").clicked();
                }
                Some(r) => {
                    numero_grande(ui, &tamanho_curto(r.desperdicio_total), 56.0);
                    paragrafo(
                        ui,
                        apoio(
                            ui,
                            format!(
                                "{} grupos de arquivos iguais na sua pasta de usuário. Deixando só uma cópia de cada, esse espaço volta.",
                                decimal(r.grupos.len() as f64, 0)
                            ),
                        )
                        .size(14.0),
                    );
                    ui.horizontal(|ui| {
                        let texto = if self.revisar_copias { "Esconder as cópias" } else { "Revisar as cópias" };
                        if botao_secundario(ui, texto).clicked() {
                            self.revisar_copias = !self.revisar_copias;
                        }
                        procurar = botao_secundario(ui, "Procurar de novo").clicked();
                    });
                }
            }
            if procurar {
                let contadores = Arc::new(Contadores::default());
                self.contadores_copias = contadores.clone();
                let pasta = PathBuf::from(std::env::var("USERPROFILE").unwrap_or_default());
                let minimo = c.config.disco.duplicados_min_mb << 20;
                self.revisar_copias = false;
                self.tarefa_duplicados = Some(Tarefa::iniciar(ctx, move || duplicados::procurar(&pasta, minimo, &contadores)));
            }
        });
    }

    fn lista_de_copias(&mut self, ui: &mut Ui) {
        let Some(r) = &self.duplicados else { return };
        let p = paleta(ui);
        cartao(ui, |ui| {
            titulo_da_secao(ui, "Cópias repetidas");
            ui.label(apoio(ui, "Abra a pasta e apague à mão a cópia que não quiser. O Makeway não apaga nenhuma.").size(14.0));
            ui.add_space(6.0);
            for (i, g) in r.grupos.iter().take(80).enumerate() {
                let nome = g.arquivos.first().and_then(|a| a.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let mut titulo = egui::text::LayoutJob::default();
                titulo.append(
                    &format!("{} SOBRANDO", tamanho_curto(g.desperdicio)),
                    0.0,
                    egui::TextFormat { font_id: FontId::new(16.0, tema::display()), color: p.ciano, extra_letter_spacing: 0.8, ..Default::default() },
                );
                titulo.append(
                    &format!("{} cópias de {} · {}", g.arquivos.len(), tamanho_curto(g.tamanho), cortar(&nome, 60)),
                    12.0,
                    egui::TextFormat { font_id: FontId::proportional(14.0), color: p.txt2, valign: Align::Center, ..Default::default() },
                );
                egui::CollapsingHeader::new(titulo).id_salt(("copia", i)).show(ui, |ui| {
                    for arquivo in &g.arquivos {
                        ui.horizontal(|ui| {
                            botao_mostrar(ui, arquivo);
                            ui.label(nota(ui, arquivo.display().to_string()));
                        });
                    }
                });
            }
            if r.grupos.len() > 80 {
                ui.label(nota(ui, format!("E mais {} grupos menores.", r.grupos.len() - 80)));
            }
        });
    }

    fn maiores_arquivos(&mut self, ui: &mut Ui) {
        let Some(v) = &self.varredura else { return };
        let p = paleta(ui);
        let maior = v.maiores_arquivos.first().map_or(1, |a| a.tamanho.max(1));
        cartao_com(ui, Margin { left: 28, right: 28, top: 26, bottom: 14 }, |ui| {
            ui.horizontal(|ui| {
                titulo_da_secao(ui, "Os maiores arquivos");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(nota(ui, "Para você decidir. O Makeway não apaga nenhum."));
                });
            });
            ui.add_space(8.0);
            for (i, a) in v.maiores_arquivos.iter().take(8).enumerate() {
                separador(ui);
                let (nome, aviso) = descrever(&a.caminho);
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 18.0;
                    ui.allocate_ui_with_layout(vec2(34.0, 30.0), Layout::left_to_right(Align::Center), |ui| {
                        ui.set_min_width(34.0);
                        ui.label(RichText::new(format!("{:02}", i + 1)).family(tema::display()).size(22.0).color(p.lime));
                    });
                    let largura = (ui.available_width() - (150.0 + 86.0 + 170.0 + 18.0 * 3.0)).max(180.0);
                    ui.vertical(|ui| {
                        ui.set_width(largura);
                        ui.spacing_mut().item_spacing.y = 0.0;
                        ui.label(forte(cortar(&nome, 48), 15.0));
                        ui.label(nota(ui, cortar(&onde(&a.caminho), 64))).on_hover_text(a.caminho.display().to_string());
                    });
                    // Da direita para a esquerda: ação, tamanho, medidor. Sempre alinhados.
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.allocate_ui_with_layout(vec2(170.0, 42.0), Layout::left_to_right(Align::Center), |ui| {
                            ui.set_min_width(170.0);
                            match aviso {
                                Some(aviso) => {
                                    ui.spacing_mut().item_spacing.x = 8.0;
                                    let r = ui
                                        .horizontal(|ui| {
                                            icones::icone(ui, Icone::Cadeado, 16.0, p.aviso);
                                            ui.label(rotulo("Protegido", 13.0, p.aviso).family(tema::display()));
                                        })
                                        .response;
                                    r.on_hover_text(aviso);
                                }
                                None => botao_mostrar(ui, &a.caminho),
                            }
                        });
                        ui.allocate_ui_with_layout(vec2(86.0, 30.0), Layout::right_to_left(Align::Center), |ui| {
                            ui.set_min_width(86.0);
                            ui.label(RichText::new(tamanho_curto(a.tamanho)).family(tema::display()).size(20.0).color(p.txt));
                        });
                        medidor_em(ui, 150.0, a.tamanho as f32 / maior as f32, 10, 10.0, 2.0, p.ciano, false);
                    });
                });
                ui.add_space(12.0);
            }
        });
    }

    fn pasta_por_pasta(&mut self, ui: &mut Ui) {
        let p = paleta(ui);
        cartao_com(ui, Margin::symmetric(26, 4), |ui| {
            let (rect, resposta) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 72.0), Sense::click());
            if resposta.hovered() {
                ui.painter().rect_filled(rect.expand2(vec2(26.0, 0.0)), 0.0, p.sup2);
            }
            texto_espacado(ui.painter(), pos2(rect.left(), rect.top() + 25.0), Align2::LEFT_CENTER, "PASTA POR PASTA", FontId::new(16.0, tema::display()), 1.9, p.txt);
            ui.painter().text(pos2(rect.left(), rect.top() + 48.0), Align2::LEFT_CENTER, "Para quem quer ver os detalhes", FontId::proportional(14.0), p.txt2);
            let seta = Rect::from_center_size(pos2(rect.right() - 10.0, rect.center().y), Vec2::splat(20.0));
            icones::pintar(ui.painter(), seta, if self.pasta_por_pasta { Icone::ChevronBaixo } else { Icone::Chevron }, p.lime);
            if resposta.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                self.pasta_por_pasta = !self.pasta_por_pasta;
            }
            if self.pasta_por_pasta && let Some(v) = &self.varredura {
                separador(ui);
                ui.add_space(10.0);
                let total = v.raiz.tamanho.max(1);
                egui::Grid::new("pasta_por_pasta").num_columns(3).spacing([18.0, 6.0]).show(ui, |ui| {
                    linhas(ui, &v.raiz, total, 0, &mut self.abertas);
                });
                ui.add_space(14.0);
            }
        });
    }
}

/// Título da página com o espaço livre bem grande à direita.
fn cabecalho(ui: &mut Ui, unidade: &str, livre: u64, total: u64) {
    let p = paleta(ui);
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_width((ui.available_width() - 260.0).max(320.0));
            titulo_da_pagina(ui, &format!("Disco {unidade}"), "Liberar espaço", "O Makeway nunca apaga nada sem você confirmar.");
        });
        if total > 0 {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.with_layout(Layout::top_down(Align::Max), |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.add_space(10.0);
                    numero_grande(ui, &tamanho_curto(livre), 64.0);
                    ui.label(rotulo(format!("Livres de {}", tamanho_curto(total)), 13.0, p.txt3));
                });
            });
        }
    });
}

/// Linha da legenda: cor, nome, explicação e o tamanho à direita.
fn linha_de_legenda(ui: &mut Ui, cor: Color32, nome: &str, explicacao: &str, bytes: u64) {
    let p = paleta(ui);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 12.0;
        let (rect, _) = ui.allocate_exact_size(vec2(14.0, 52.0), Sense::hover());
        let quadrado = Rect::from_center_size(rect.center(), Vec2::splat(14.0));
        let cor = if cor == p.livre { p.sup3 } else { cor };
        ui.painter().rect_filled(quadrado, 0.0, cor);
        ui.painter().rect_stroke(quadrado, 0.0, Stroke::new(1.0, Color32::from_white_alpha(30)), egui::StrokeKind::Inside);
        ui.vertical(|ui| {
            ui.set_width((ui.available_width() - 90.0).max(80.0));
            ui.add_space(6.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.label(forte(nome, 15.0));
            ui.label(nota(ui, explicacao));
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new(tamanho_curto(bytes)).family(tema::display()).size(20.0).color(p.txt));
        });
    });
    separador(ui);
}

fn temporarios(ui: &mut Ui, c: &mut Contexto) {
    let p = paleta(ui);
    cartao(ui, |ui| {
        ui.set_min_height(236.0);
        ui.spacing_mut().item_spacing.y = 12.0;
        ui.label(rotulo("Arquivos temporários", 12.0, p.txt3));
        let Some(t) = &c.dados.temporarios else {
            ui.spinner();
            return;
        };
        numero_grande(ui, &tamanho_curto(t.bytes_antigos()), 56.0);
        paragrafo(
            ui,
            apoio(ui, format!("Sobras de instalações e de programas, com mais de {} dias. Nenhum programa precisa mais delas.", t.idade_minima_dias)).size(14.0),
        );
        if c.dados.limpando() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(apoio(ui, "Mandando para a Lixeira…"));
            });
        } else if t.antigos.is_empty() {
            match &c.dados.limpeza {
                Some(l) => {
                    ui.horizontal(|ui| {
                        icones::icone(ui, Icone::CheckSimples, 20.0, p.lime);
                        ui.label(rotulo(format!("{} na Lixeira", tamanho_curto(l.bytes)), 16.0, p.lime).family(tema::display()));
                    });
                }
                None => {
                    ui.label(nota(ui, "Nada antigo para limpar agora."));
                }
            }
        } else if botao_primario(ui, "Mandar para a Lixeira").clicked() {
            c.confirmar_limpeza = true;
        }
    });
}

/// Divide o disco em categorias a partir da varredura. Hard links (como os
/// do Windows) aparecem duas vezes numa varredura de pastas, então os
/// valores são ajustados para somar o espaço realmente usado.
fn categorias(raiz: &Pasta, usado: u64) -> [u64; 5] {
    let mut s = [0u64; 5]; // seus, jogos, windows, programas, outros
    let mut nas_pastas = 0;
    for f in &raiz.filhos {
        nas_pastas += f.tamanho;
        let nome = f.caminho.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
        match nome.as_str() {
            "users" => s[0] += f.tamanho,
            "windows" => s[2] += f.tamanho,
            "program files" | "program files (x86)" => {
                let mut lojas = 0;
                for g in &f.filhos {
                    let n = g.caminho.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
                    if LOJAS.contains(&n.as_str()) {
                        lojas += g.tamanho;
                    }
                }
                s[1] += lojas;
                s[3] += f.tamanho - lojas;
            }
            "riot games" | "xboxgames" | "games" => s[1] += f.tamanho,
            _ => s[4] += f.tamanho,
        }
    }
    s[4] += raiz.tamanho.saturating_sub(nas_pastas);
    let soma: u64 = s.iter().sum();
    if usado > 0 && soma > usado {
        for valor in &mut s {
            *valor = (*valor as f64 * usado as f64 / soma as f64) as u64;
        }
    }
    s
}

/// Nome que qualquer um entende e, se for algo para não apagar, o motivo.
fn descrever(caminho: &Path) -> (String, Option<&'static str>) {
    let nome = caminho.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let minusculo = nome.to_lowercase();
    let texto = caminho.to_string_lossy().to_lowercase();
    let extensao = caminho.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    match minusculo.as_str() {
        "pagefile.sys" => return ("Arquivo de paginação do Windows".into(), Some("O Windows usa como memória extra")),
        "hiberfil.sys" => return ("Arquivo de hibernação do Windows".into(), Some("Parte do Windows")),
        "swapfile.sys" => return ("Arquivo de troca do Windows".into(), Some("Parte do Windows")),
        _ => {}
    }
    for marca in [r"\steamapps\common\", r"\epic games\", r"\riot games\", r"\xboxgames\", r"\ubisoft game launcher\games\"] {
        if let Some(i) = texto.find(marca) {
            let resto = &caminho.to_string_lossy()[i + marca.len()..];
            let jogo = resto.split('\\').next().unwrap_or("jogo");
            return (format!("Dados do {jogo}"), Some("Faz parte de um jogo instalado"));
        }
    }
    let tipo = match extensao.as_str() {
        "vhdx" | "vhd" if texto.contains("docker") => "Disco virtual do Docker",
        "vhdx" | "vhd" if texto.contains("wsl") || texto.contains("packages") => "Disco do Linux (WSL)",
        "vhdx" | "vhd" => "Disco virtual",
        "vdi" | "vmdk" => "Máquina virtual",
        "iso" => "Imagem de disco (ISO)",
        "nsp" | "xci" => "Jogo de Nintendo Switch",
        "zip" | "rar" | "7z" => "Arquivo compactado",
        "mp4" | "mkv" | "avi" | "mov" => "Vídeo",
        "exe" | "msi" => "Instalador",
        _ => return (nome, None),
    };
    let lugar = if texto.contains(r"\downloads\") { " em Downloads" } else { "" };
    (format!("{tipo}{lugar}"), None)
}

/// Pasta do arquivo, encurtada para "Sua pasta\..." quando der.
fn onde(caminho: &Path) -> String {
    let pasta = caminho.parent().unwrap_or(caminho);
    if let Ok(perfil) = std::env::var("USERPROFILE")
        && let Ok(resto) = pasta.strip_prefix(&perfil)
    {
        return format!("Sua pasta\\{}", resto.display());
    }
    pasta.display().to_string()
}

fn linhas(ui: &mut Ui, pasta: &Pasta, total: u64, nivel: usize, abertas: &mut HashSet<PathBuf>) {
    let p = paleta(ui);
    for filho in pasta.filhos.iter().take(25) {
        let aberta = abertas.contains(&filho.caminho);
        let nome = filho.caminho.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        ui.horizontal(|ui| {
            ui.add_space(nivel as f32 * 20.0);
            if filho.filhos.is_empty() {
                ui.add_space(24.0);
            } else {
                let (rect, resposta) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::click());
                let icone = if aberta { Icone::ChevronBaixo } else { Icone::Chevron };
                icones::pintar(ui.painter(), rect.shrink(4.0), icone, if resposta.hovered() { p.lime } else { p.txt2 });
                if resposta.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    if aberta {
                        abertas.remove(&filho.caminho);
                    } else {
                        abertas.insert(filho.caminho.clone());
                    }
                }
            }
            ui.label(nome).on_hover_text(filho.caminho.display().to_string());
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new(tamanho_curto(filho.tamanho)).family(tema::display()).size(16.0).color(p.txt));
        });
        barra(ui, filho.tamanho as f32 / total as f32, 180.0, p.ciano);
        ui.end_row();
        if aberta {
            linhas(ui, filho, total, nivel + 1, abertas);
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn pasta(caminho: &str, tamanho: u64, filhos: Vec<Pasta>) -> Pasta {
        Pasta { caminho: PathBuf::from(caminho), tamanho, arquivos: 1, subpastas: filhos.len() as u64, filhos }
    }

    #[test]
    fn separa_o_disco_em_categorias() {
        let raiz = pasta(
            r"C:\",
            1000,
            vec![
                pasta(r"C:\Users", 400, vec![]),
                pasta(r"C:\Windows", 100, vec![]),
                pasta(
                    r"C:\Program Files (x86)",
                    300,
                    vec![pasta(r"C:\Program Files (x86)\Steam", 250, vec![]), pasta(r"C:\Program Files (x86)\Outro", 30, vec![])],
                ),
                pasta(r"C:\Riot Games", 50, vec![]),
                pasta(r"C:\dev", 40, vec![]),
            ],
        );
        // seus, jogos, windows, programas, outros (+110 de arquivos soltos na raiz)
        assert_eq!(categorias(&raiz, 2000), [400, 300, 100, 50, 150]);
        // Varredura maior que o espaço usado (hard links): ajusta para somar o usado.
        let ajustado = categorias(&raiz, 500);
        assert!(ajustado.iter().sum::<u64>() <= 500);
        assert_eq!(ajustado[0], 200);
    }

    #[test]
    fn descreve_arquivos_grandes_em_portugues() {
        assert_eq!(descrever(Path::new(r"C:\pagefile.sys")).1, Some("O Windows usa como memória extra"));
        let (nome, aviso) = descrever(Path::new(r"C:\Program Files (x86)\Steam\steamapps\common\Overwatch\data\data.020"));
        assert_eq!(nome, "Dados do Overwatch");
        assert!(aviso.is_some());
        assert_eq!(descrever(Path::new(r"C:\Users\x\AppData\Local\Docker\wsl\disk\docker_data.vhdx")).0, "Disco virtual do Docker");
        assert_eq!(descrever(Path::new(r"C:\Users\x\Downloads\jogo.nsp")).0, "Jogo de Nintendo Switch em Downloads");
        assert_eq!(descrever(Path::new(r"C:\Users\x\notas.txt")), ("notas.txt".to_owned(), None));
    }
}
