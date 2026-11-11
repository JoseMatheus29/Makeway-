//! As cinco telas e o que elas têm em comum.

pub mod espaco;
pub mod inicio;
pub mod memoria;
pub mod modo_jogo;
pub mod partidas;

use chrono::{Datelike, Local, TimeZone};
use eframe::egui::{Align, Layout, RichText, Ui};
use nucleo::formato::decimal;
use nucleo::modo_jogo::protecao::Protecao;
use nucleo::modo_jogo::relatorio::RelatorioSessao;
use nucleo::paginacao::Paginacao;
use nucleo::plataforma;
use nucleo::ram::RelatorioRam;

use crate::app::Contexto;
use crate::componentes::{apoio, botao_secundario, faixa, paragrafo, rotulo};
use crate::icones::{self, Icone};
use crate::tema::{self, paleta};

const GB: f64 = (1u64 << 30) as f64;

/// "4 GB", "4,1 GB", "627 MB": curto, para números grandes na tela.
pub fn tamanho_curto(bytes: u64) -> String {
    let gb = bytes as f64 / GB;
    if gb >= 10.0 {
        format!("{} GB", decimal(gb, 0))
    } else if gb >= 1.0 {
        format!("{} GB", decimal(gb, 1))
    } else {
        format!("{} MB", decimal(bytes as f64 / (1u64 << 20) as f64, 0))
    }
}

/// "hoje, 13:43", "ontem, 21:10" ou "05/11, 20:30".
pub fn quando(unix: i64) -> String {
    let Some(data) = Local.timestamp_opt(unix, 0).single() else { return String::new() };
    let hoje = Local::now().date_naive();
    let dia = data.date_naive();
    let hora = data.format("%H:%M");
    if dia == hoje {
        format!("hoje, {hora}")
    } else if Some(dia) == hoje.pred_opt() {
        format!("ontem, {hora}")
    } else if dia.year() == hoje.year() {
        format!("{}, {hora}", data.format("%d/%m"))
    } else {
        format!("{}, {hora}", data.format("%d/%m/%Y"))
    }
}

/// O que a memória disponível quer dizer para quem vai jogar.
pub fn veredito_da_memoria(disponivel: u64) -> (&'static str, &'static str, Nivel) {
    let gb = disponivel as f64 / GB;
    if gb >= 3.0 {
        ("Sobra memória para jogar", "Folga boa para jogar.", Nivel::Bom)
    } else if gb >= 1.5 {
        ("A memória está apertada", "Dá para jogar, mas está apertado.", Nivel::Atencao)
    } else {
        ("Pouca memória livre", "Um jogo pesado pode travar.", Nivel::Ruim)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Nivel {
    Bom,
    Atencao,
    Ruim,
}

/// Uma partida foi boa se a memória nunca acabou e o disco não virou memória.
pub fn partida_boa(r: &RelatorioSessao) -> bool {
    let cresceu = r.paginacao_max.saturating_sub(r.paginacao_inicio);
    r.ram_disponivel_min >= 1 << 30 && cresceu <= 256 << 20
}

/// Como o arquivo de paginação está, em poucas palavras.
pub fn nome_da_paginacao(paginacao: Paginacao) -> String {
    match paginacao {
        Paginacao::Automatica => "Automático".into(),
        Paginacao::GerenciadaPeloSistema => "Gerenciado pelo Windows".into(),
        Paginacao::Fixa { maximo } => format!("Fixo, até {}", tamanho_curto(maximo)),
        Paginacao::Desligada => "Desligado".into(),
    }
}

/// O que fazer quando a memória chega perto do limite, conforme a paginação.
pub fn conselho_do_limite(paginacao: Option<Paginacao>) -> String {
    match paginacao {
        Some(Paginacao::Desligada) => {
            "O arquivo de paginação está desligado, então o limite é só a RAM. Ligue de novo em Avançado > Memória virtual, com \"Gerenciar automaticamente\" marcado.".into()
        }
        Some(Paginacao::Fixa { maximo }) => format!(
            "O arquivo de paginação tem tamanho fixo (até {}) e não cresce. Em Avançado > Memória virtual, marque \"Gerenciar automaticamente\".",
            tamanho_curto(maximo)
        ),
        _ => "O Windows aumenta o arquivo de paginação sozinho, mas isso pode dar uma travada. Pausar apps durante o jogo deixa mais folga.".into(),
    }
}

/// O botão "Abrir no Windows" só ajuda quando dá para mudar algo lá.
pub fn paginacao_para_mudar(paginacao: Option<Paginacao>) -> bool {
    paginacao.is_some_and(|x| !x.cresce_sozinha())
}

/// Faixa vermelha: a memória chegou perto do limite (RAM + paginação),
/// o que isso quer dizer e o que fazer.
pub fn aviso_de_limite(ui: &mut Ui, titulo: &str, confirmada: u64, limite: u64, paginacao: Option<Paginacao>) {
    let p = paleta(ui);
    faixa(ui, p.perigo.gamma_multiply(0.12), |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 16.0;
            icones::icone(ui, Icone::Alerta, 26.0, p.perigo);
            let botao = paginacao_para_mudar(paginacao);
            ui.vertical(|ui| {
                ui.set_width((ui.available_width() - if botao { 200.0 } else { 0.0 }).max(220.0));
                ui.spacing_mut().item_spacing.y = 3.0;
                ui.label(rotulo(titulo, 14.0, p.perigo).family(tema::display()).extra_letter_spacing(2.2));
                paragrafo(
                    ui,
                    RichText::new(format!(
                        "{} de {} possíveis (RAM + arquivo de paginação). No limite, o jogo pode fechar sozinho por falta de memória.",
                        tamanho_curto(confirmada),
                        tamanho_curto(limite)
                    ))
                    .size(15.0)
                    .color(p.txt),
                );
                paragrafo(ui, apoio(ui, conselho_do_limite(paginacao)).size(14.0));
            });
            if botao {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if botao_secundario(ui, "Abrir no Windows").on_hover_text("Abre as Opções de desempenho do Windows").clicked() {
                        plataforma::abrir_opcoes_de_desempenho();
                    }
                });
            }
        });
    });
}

/// App que vale sugerir pausar depois de uma partida: o que mais disputou
/// o processador (3% ou mais) ou, se faltou memória, o que mais usou RAM.
/// Nunca um protegido ou um que já está na lista. (exe, motivo)
pub fn rival_da_partida(c: &Contexto, r: &RelatorioSessao) -> Option<(String, String)> {
    let livres = || {
        r.concorrentes
            .iter()
            .filter(|x| !x.protegido && !c.pausado_nos_jogos(&x.nome) && !x.nome.eq_ignore_ascii_case(&r.jogo))
    };
    if let Some(x) = livres().filter(|x| x.cpu_media_pct >= 3.0).max_by(|a, b| a.cpu_media_pct.total_cmp(&b.cpu_media_pct)) {
        return Some((x.nome.clone(), format!("usou {}% do processador durante a partida", decimal(x.cpu_media_pct, 0))));
    }
    if !partida_boa(r)
        && let Some(x) = livres().filter(|x| x.ram_max >= 300 << 20).max_by_key(|x| x.ram_max)
    {
        return Some((x.nome.clone(), format!("usou {} de memória durante a partida", tamanho_curto(x.ram_max))));
    }
    None
}

/// O app aberto agora que mais usa memória e pode ser pausado (exe, bytes).
pub fn maior_app_pausavel(c: &Contexto, r: &RelatorioRam) -> Option<(String, u64)> {
    let protecao = Protecao::nova(&r.processos, None);
    r.apps
        .iter()
        .filter(|a| !c.pausado_nos_jogos(&a.nome))
        .find(|a| {
            r.processos
                .iter()
                .filter(|p| a.pids.contains(&p.pid))
                .all(|p| protecao.motivo(p).is_none())
        })
        .map(|a| (a.nome.clone(), a.ws_privado))
}

/// Apps abertos agora que podem entrar numa lista, do que mais usa memória
/// para o que menos usa: (exe, bytes).
pub fn apps_para_escolher(r: &RelatorioRam, ja: &[String], quantos: usize) -> Vec<(String, u64)> {
    let protecao = Protecao::nova(&r.processos, None);
    r.apps
        .iter()
        .filter(|a| !ja.iter().any(|n| n.eq_ignore_ascii_case(&a.nome)))
        .filter(|a| {
            r.processos
                .iter()
                .filter(|p| a.pids.contains(&p.pid))
                .all(|p| protecao.motivo(p).is_none())
        })
        .take(quantos)
        .map(|a| (a.nome.clone(), a.ws_privado))
        .collect()
}
