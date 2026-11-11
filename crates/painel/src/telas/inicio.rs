//! Início: a resposta primeiro ("PRONTO PARA JOGAR"), um resumo em três
//! cartões com medidores em blocos e os boosts com o botão do lado.

use eframe::egui::{self, Align, Color32, Frame, Layout, Margin, Rect, Shape, Stroke, Ui, Vec2, pos2, vec2};
use nucleo::config::Modo;
use nucleo::formato::duracao;
use nucleo::captura;
use nucleo::modo_jogo::relatorio::OUTRA_CONTA_PESADA;
use nucleo::paginacao::{Paginacao, perto_do_limite};

use super::{Nivel, maior_app_pausavel, partida_boa, quando, rival_da_partida, tamanho_curto, veredito_da_memoria};
use crate::app::{Contexto, Tela};
use crate::componentes::{
    CORTE, apoio, bloco_icone, botao_primario, carimbo, cartao, cartao_com, chamada, cortar, display, etiqueta,
    forma_cortada, forte, link, listras, medidor, numero_grande, paragrafo, rotulo, segmentado, setas,
    titulo_da_secao, vazio,
};
use crate::icones::Icone;
use crate::tema::{self, paleta};

#[derive(Default)]
pub struct TelaInicio;

enum Acao {
    Pausar(String),
    Limpar,
    Ir(Tela),
    ComWindows,
    /// Abre as Opções de desempenho do Windows (memória virtual).
    Desempenho,
    /// Abre uma página das Configurações do Windows.
    Configuracoes(&'static str),
}

struct Boost {
    icone: Icone,
    titulo: String,
    texto: String,
    ganho: String,
    botao: &'static str,
    acao: Acao,
}

impl TelaInicio {
    pub fn ui(&mut self, ui: &mut Ui, c: &mut Contexto) {
        destaque(ui, c);
        ui.add_space(16.0);
        ui.columns(3, |colunas| {
            for coluna in colunas.iter_mut() {
                coluna.spacing_mut().item_spacing.x = 16.0;
            }
            cartao_memoria(&mut colunas[0], c);
            cartao_espaco(&mut colunas[1], c);
            cartao_partida(&mut colunas[2], c);
        });
        ui.add_space(26.0);
        boosts(ui, c);
    }
}

fn texto_do_modo(modo: Modo) -> &'static str {
    match modo {
        Modo::Observar => "Quando um jogo abrir, o Makeway só observa e anota como foi. Nada muda no seu PC.",
        Modo::Otimizar => {
            "Quando um jogo abrir, o Makeway dá prioridade a ele e pausa os apps da sua lista. Tudo volta ao normal quando você fechar o jogo."
        }
    }
}

/// O cartão grande do topo, com listras e as setas da marca ao fundo.
fn destaque(ui: &mut Ui, c: &mut Contexto) {
    let p = paleta(ui);
    let (ativo, modo) = (c.config.modo_jogo.ativo, c.config.modo_jogo.modo);
    // Título em duas partes: a segunda vem colorida.
    let (parte1, parte2, cor2, texto): (String, String, Color32, String) = match &c.status {
        None => ("Vigia".into(), "desligado".into(), p.aviso, "Ligue o vigia para o Makeway cuidar dos seus jogos. Ele fica perto do relógio.".into()),
        Some(_) if !ativo => ("Modo jogo".into(), "desligado".into(), p.txt3, "Nada acontece quando um jogo abre. Ligue para o Makeway entrar em ação.".into()),
        Some(s) => match &s.jogo {
            Some(jogo) => {
                let cor = if modo == Modo::Otimizar { p.lime } else { p.aviso };
                ("Jogando".into(), jogo.trim_end_matches(".exe").into(), cor, texto_do_modo(modo).into())
            }
            None => ("Pronto para".into(), "jogar".into(), p.lime, texto_do_modo(modo).into()),
        },
    };

    let fundo = ui.painter().add(Shape::Noop);
    let textura = ui.painter().add(Shape::Noop);
    let resposta = Frame::new().inner_margin(Margin { left: 34, right: 34, top: 34, bottom: 30 }).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            let largura_texto = (ui.available_width() - 340.0).max(300.0);
            ui.vertical(|ui| {
                ui.set_width(largura_texto);
                ui.spacing_mut().item_spacing.y = 8.0;
                chamada(ui, "Status do PC");
                let mut job = egui::text::LayoutJob::default();
                let formato = |cor: Color32| egui::TextFormat { font_id: egui::FontId::new(58.0, tema::display()), color: cor, extra_letter_spacing: 0.6, ..Default::default() };
                job.append(&format!("{} ", parte1.to_uppercase()), 0.0, formato(p.txt));
                job.append(&parte2.to_uppercase(), 0.0, formato(cor2));
                job.wrap.max_width = largura_texto;
                ui.add(egui::Label::new(job));
                ui.scope(|ui| {
                    ui.set_max_width(470.0);
                    paragrafo(ui, apoio(ui, texto));
                });
            });
            ui.with_layout(Layout::right_to_left(Align::Max), |ui| {
                ui.with_layout(Layout::top_down(Align::Min), |ui| {
                    ui.add_space(ui.available_height().max(0.0));
                    if c.status.is_none() {
                        if botao_primario(ui, "Ligar o vigia").clicked() {
                            c.iniciar_vigia();
                        }
                    } else if !ativo {
                        if botao_primario(ui, "Ligar o modo jogo").clicked() {
                            let mut novo = c.config.modo_jogo.clone();
                            novo.ativo = true;
                            c.salvar_modo_jogo(novo, "Modo jogo ligado");
                        }
                    } else {
                        ui.label(rotulo("Quando um jogo abrir", 12.0, p.txt3));
                        let mut escolhido = if modo == Modo::Observar { 0 } else { 1 };
                        if segmentado(ui, &["Observar", "Otimizar"], &mut escolhido, 56.0, 150.0) {
                            c.definir_modo(if escolhido == 0 { Modo::Observar } else { Modo::Otimizar });
                        }
                    }
                });
            });
        });
    });
    let rect = resposta.response.rect;
    ui.painter().set(fundo, forma_cortada(rect, CORTE, p.sup, Stroke::NONE));

    // Textura: listras na direita e as setas grandes, bem apagadas, por
    // baixo do texto e recortadas no cartão.
    let direita = Rect::from_min_max(pos2(rect.left() + rect.width() * 0.54, rect.top()), rect.max);
    let mut formas = listras(direita, rect.right() - CORTE, p.lime.gamma_multiply(0.06));
    let marca = Rect::from_min_size(pos2(rect.right() - 260.0, rect.top() - 60.0), Vec2::splat(300.0));
    formas.extend(setas(marca, p.lime.gamma_multiply(0.07), 2.4));
    ui.painter().with_clip_rect(rect).set(textura, Shape::Vec(formas));
}

/// Cabeçalho, número enorme, medidor e frase: o miolo dos três cartões.
fn cartao_de_resumo(ui: &mut Ui, conteudo: impl FnOnce(&mut Ui)) {
    cartao_com(ui, Margin::symmetric(24, 22), |ui| {
        ui.set_min_height(196.0);
        ui.spacing_mut().item_spacing.y = 10.0;
        conteudo(ui);
    });
}

fn cartao_memoria(ui: &mut Ui, c: &mut Contexto) {
    let p = paleta(ui);
    cartao_de_resumo(ui, |ui| {
        ui.label(rotulo("RAM livre", 12.0, p.txt3));
        let Some(r) = &c.dados.ram else {
            ui.spinner();
            return;
        };
        let m = &r.memoria;
        numero_grande(ui, &tamanho_curto(m.disponivel), 58.0);
        let livre = m.disponivel as f32 / m.total.max(1) as f32;
        let (_, frase, nivel) = veredito_da_memoria(m.disponivel);
        let cor = match nivel {
            Nivel::Bom => p.lime,
            Nivel::Atencao => p.aviso,
            Nivel::Ruim => p.perigo,
        };
        medidor(ui, livre, 20, 14.0, cor);
        paragrafo(ui, apoio(ui, format!("de {}. {frase}", tamanho_curto(m.total))).size(14.0));
        if link(ui, "Ver quem usa").clicked() {
            c.ir_para = Some(Tela::Memoria);
        }
    });
}

fn cartao_espaco(ui: &mut Ui, c: &mut Contexto) {
    let p = paleta(ui);
    cartao_de_resumo(ui, |ui| {
        let unidade = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
        ui.label(rotulo(format!("Disco {unidade} livre"), 12.0, p.txt3));
        let Some((livre, total)) = c.dados.disco else {
            ui.spinner();
            return;
        };
        numero_grande(ui, &tamanho_curto(livre), 58.0);
        let fracao = livre as f32 / total.max(1) as f32;
        medidor(ui, fracao, 20, 14.0, if fracao < 0.1 { p.aviso } else { p.lime });
        let liberar = c.dados.temporarios.as_ref().map_or(0, |t| t.bytes_antigos());
        let extra = if liberar >= 50 << 20 { format!(" Dá para liberar {} agora.", tamanho_curto(liberar)) } else { String::new() };
        paragrafo(ui, apoio(ui, format!("de {}.{extra}", tamanho_curto(total))).size(14.0));
        if link(ui, "Liberar espaço").clicked() {
            c.ir_para = Some(Tela::Espaco);
        }
    });
}

fn cartao_partida(ui: &mut Ui, c: &mut Contexto) {
    let p = paleta(ui);
    cartao_de_resumo(ui, |ui| {
        ui.label(rotulo("Última partida", 12.0, p.txt3));
        let Some((_, r)) = c.dados.partidas.first() else {
            ui.add_space(4.0);
            ui.label(display("Nenhuma ainda", 30.0));
            paragrafo(ui, apoio(ui, "Deixe o vigia ligado e jogue. Cada partida vira um relatório com o que dá para melhorar.").size(14.0));
            return;
        };
        ui.add_space(4.0);
        ui.label(display(cortar(r.jogo.trim_end_matches(".exe"), 18), 34.0));
        if partida_boa(r) {
            carimbo(ui, "Rodou liso", p.lime);
        } else {
            carimbo(ui, "Faltou memória", p.aviso);
        }
        let frase = if partida_boa(r) {
            "A memória sobrou o tempo todo.".to_owned()
        } else {
            format!("A memória livre chegou a {}.", tamanho_curto(r.ram_disponivel_min))
        };
        let quando = quando(r.inicio_unix);
        let mut quando_maiusculo = quando.chars();
        let quando = quando_maiusculo.next().map(|c| c.to_uppercase().collect::<String>() + quando_maiusculo.as_str()).unwrap_or_default();
        paragrafo(ui, apoio(ui, format!("{quando}, {}. {frase}", duracao(r.duracao_segundos))).size(14.0));
        if link(ui, "Ver partidas").clicked() {
            c.ir_para = Some(Tela::Partidas);
        }
    });
}

fn montar_boosts(c: &Contexto) -> Vec<Boost> {
    let mut lista = Vec::new();

    // Outra conta do Windows aberta em segundo plano, segurando memória.
    if let Some(conta) = c.dados.ram.as_ref().and_then(|r| r.outras_contas.first()).filter(|x| x.ws_privado >= OUTRA_CONTA_PESADA) {
        let quem = conta.usuario.as_deref().map_or("a outra conta do Windows".to_owned(), |n| format!("a conta {n}"));
        lista.push(Boost {
            icone: Icone::Conta,
            titulo: format!("Sair de {quem}"),
            texto: format!(
                "Ela ficou aberta em segundo plano, com {} processos ocupando memória. Entre nela e use Sair (não Trocar de usuário).",
                conta.processos
            ),
            ganho: format!("+{} para o jogo", tamanho_curto(conta.ws_privado)),
            botao: "Ver",
            acao: Acao::Ir(Tela::Memoria),
        });
    }

    // "Gravar o que aconteceu" do Game Bar grava sem parar durante o jogo.
    if c.dados.gravacao_do_game_bar == Some(true) {
        lista.push(Boost {
            icone: Icone::Gravacao,
            titulo: "Desligar a gravação em segundo plano do Game Bar".into(),
            texto: "\"Gravar o que aconteceu\" está ligado: o Windows grava os últimos minutos sem parar enquanto você joga. Em vídeo integrado, isso divide memória e vídeo com o jogo.".into(),
            ganho: "Mais folga no vídeo".into(),
            botao: "Abrir",
            acao: Acao::Configuracoes(captura::PAGINA_DAS_CAPTURAS),
        });
    }

    // Um app para pausar: pela última partida ou, sem partidas, pela memória agora.
    let ultima = c.dados.partidas.first().map(|(_, r)| r);
    let rival = ultima.and_then(|r| rival_da_partida(c, r));
    if let (Some((exe, motivo)), Some(r)) = (rival, ultima) {
        let ram = r.concorrentes.iter().find(|x| x.nome.eq_ignore_ascii_case(&exe)).map_or(0, |x| x.ram_max);
        lista.push(Boost {
            icone: Icone::Pausa,
            titulo: format!("Pausar o {} durante os jogos", c.dados.nome(&exe)),
            texto: format!("Ele {motivo}."),
            ganho: if ram >= 100 << 20 { format!("+{} para o jogo", tamanho_curto(ram)) } else { "Menos disputa".into() },
            botao: "Ativar",
            acao: Acao::Pausar(exe),
        });
    } else if c.dados.partidas.is_empty()
        && let Some(r) = &c.dados.ram
        && let Some((exe, bytes)) = maior_app_pausavel(c, r)
        && bytes >= 700 << 20
    {
        lista.push(Boost {
            icone: Icone::Pausa,
            titulo: format!("Pausar o {} durante os jogos", c.dados.nome(&exe)),
            texto: format!("Está usando {} de memória agora. Pausado, devolve essa memória enquanto você joga.", tamanho_curto(bytes)),
            ganho: format!("+{} para o jogo", tamanho_curto(bytes)),
            botao: "Ativar",
            acao: Acao::Pausar(exe),
        });
    }

    if let Some(t) = &c.dados.temporarios
        && t.bytes_antigos() >= 50 << 20
    {
        lista.push(Boost {
            icone: Icone::Lixeira,
            titulo: "Limpar arquivos temporários".into(),
            texto: format!("Sobras de instalações com mais de {} dias. Vão para a Lixeira, dá para recuperar.", t.idade_minima_dias),
            ganho: format!("+{} no disco", tamanho_curto(t.bytes_antigos())),
            botao: "Limpar",
            acao: Acao::Limpar,
        });
    }

    if let Some(programas) = &c.dados.inicializacao {
        let ativos: Vec<&str> = programas.iter().filter(|p| p.ativo).map(|p| p.nome.as_str()).collect();
        if ativos.len() >= 3 {
            let exemplos = ativos.iter().take(3).copied().collect::<Vec<_>>().join(", ");
            let resto = ativos.len() - 3;
            let texto = if resto > 0 {
                format!("{exemplos} e mais {resto} abrem sozinhos toda vez.")
            } else {
                format!("{exemplos} abrem sozinhos toda vez.")
            };
            lista.push(Boost {
                icone: Icone::Raio,
                titulo: "Abrir menos programas com o Windows".into(),
                texto,
                ganho: "Liga mais rápido".into(),
                botao: "Revisar",
                acao: Acao::Ir(Tela::Memoria),
            });
        }
    }

    if c.dados.com_windows == Some(false) {
        lista.push(Boost {
            icone: Icone::Escudo,
            titulo: "Deixar o vigia abrir com o Windows".into(),
            texto: "Ele já fica pronto antes de você abrir o jogo. O Windows pede confirmação uma vez.".into(),
            ganho: "Sem esquecer".into(),
            botao: "Ativar",
            acao: Acao::ComWindows,
        });
    }

    // Arquivo de paginação que não cresce: sempre que desligado; com
    // tamanho fixo, só se a memória já chegou perto do limite.
    let perto_agora = c.dados.ram.as_ref().is_some_and(|r| perto_do_limite(r.memoria.confirmada, r.memoria.limite_confirmada));
    let perto_na_partida = ultima.is_some_and(|r| perto_do_limite(r.confirmada_max, r.limite_confirmada));
    match c.dados.paginacao {
        Some(Paginacao::Desligada) => lista.push(Boost {
            icone: Icone::Alerta,
            titulo: "Ligar o arquivo de paginação".into(),
            texto: "Sem ele, quando a memória acaba o jogo fecha sozinho. Deixe o Windows gerenciar o tamanho em Avançado > Memória virtual.".into(),
            ganho: "Sem fechar sozinho".into(),
            botao: "Abrir",
            acao: Acao::Desempenho,
        }),
        Some(Paginacao::Fixa { maximo }) if perto_agora || perto_na_partida => lista.push(Boost {
            icone: Icone::Alerta,
            titulo: "Deixar o arquivo de paginação crescer".into(),
            texto: format!(
                "A memória chegou perto do limite e o arquivo tem tamanho fixo (até {}). Marque \"Gerenciar automaticamente\" em Avançado > Memória virtual.",
                tamanho_curto(maximo)
            ),
            ganho: "Sem fechar sozinho".into(),
            botao: "Abrir",
            acao: Acao::Desempenho,
        }),
        _ => {}
    }
    lista
}

fn boosts(ui: &mut Ui, c: &mut Contexto) {
    let p = paleta(ui);
    let lista = montar_boosts(c);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 12.0;
        titulo_da_secao(ui, "Boosts disponíveis");
        if !lista.is_empty() {
            etiqueta(ui, &lista.len().to_string(), p.lime, p.na_lime);
        }
    });
    ui.add_space(4.0);
    if lista.is_empty() {
        cartao(ui, |ui| {
            vazio(ui, Icone::Check, "Nada para melhorar agora", "Quando houver algo que valha a pena, aparece aqui.");
        });
        return;
    }
    let ctx = ui.ctx().clone();
    let mut fazer = None;
    ui.spacing_mut().item_spacing.y = 12.0;
    for (i, b) in lista.iter().enumerate() {
        cartao_com(ui, Margin::symmetric(22, 18), |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 18.0;
                bloco_icone(ui, b.icone, p.lime, p.sup3, 46.0);
                let galley_ganho = ui.painter().layout_no_wrap(b.ganho.to_uppercase(), egui::FontId::new(14.0, tema::display()), p.ciano);
                let reservado = 128.0 + galley_ganho.size().x + 18.0 * 2.0 + 16.0;
                let largura = (ui.available_width() - reservado).max(220.0);
                ui.vertical(|ui| {
                    ui.set_width(largura);
                    ui.spacing_mut().item_spacing.y = 3.0;
                    paragrafo(ui, forte(&b.titulo, 16.0));
                    paragrafo(ui, apoio(ui, &b.texto).size(14.0));
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let ocupado = matches!(b.acao, Acao::Limpar) && c.dados.limpando()
                        || matches!(b.acao, Acao::ComWindows) && c.dados.mudando_com_windows();
                    if ocupado {
                        ui.allocate_ui(vec2(128.0, 46.0), |ui| {
                            ui.centered_and_justified(|ui| ui.spinner());
                        });
                    } else if botao_primario(ui, b.botao).clicked() {
                        fazer = Some(i);
                    }
                    ui.label(rotulo(&b.ganho, 14.0, p.ciano).family(tema::display()).extra_letter_spacing(1.1));
                });
            });
        });
    }
    if let Some(i) = fazer {
        match &lista[i].acao {
            Acao::Pausar(exe) => c.alternar_pausa(exe),
            Acao::Limpar => c.confirmar_limpeza = true,
            Acao::Ir(tela) => c.ir_para = Some(*tela),
            Acao::ComWindows => c.mudar_com_windows(&ctx, true),
            Acao::Desempenho => nucleo::plataforma::abrir_opcoes_de_desempenho(),
            Acao::Configuracoes(pagina) => nucleo::plataforma::abrir_configuracoes(pagina),
        }
    }
}
