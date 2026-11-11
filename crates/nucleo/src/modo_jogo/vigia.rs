//! O loop do vigia: procura o jogo, abre a sessão, anota amostras e,
//! quando o jogo fecha, desfaz tudo e grava o relatório.
//!
//! Fora do jogo, com administrador, o vigia dorme até o Windows avisar que
//! um processo abriu (WMI). Sem administrador, olha a lista a cada N s.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant, SystemTime};

use serde::{Deserialize, Serialize};

use super::deteccao::{self, FonteProcessos, Jogo, ProcessosDoWindows};
use super::eventos;
use super::protecao::{LAUNCHERS, Protecao};
use super::relatorio::{self, Amostrador};
use super::sessao::{ALTO_DESEMPENHO, AcoesWindows, MEMORIA_BAIXA, Sessao, restaurar_pendente};
use crate::config::{self, Config, Modo, Prioridade};
use crate::ram::{self, Processo};
use crate::sistema;

pub const NOME_DO_MUTEX: &str = r"Local\Makeway-Vigia";

/// Com os avisos do Windows ligados, fora do jogo o vigia só olha a lista
/// por conta própria de tanto em tanto tempo...
const ESPERA_COM_EVENTOS: u64 = 30;
/// ...a não ser que algum processo tenha aberto há pouco: jogos detectados
/// pela pasta só passam de 100 MB depois de alguns segundos carregando.
const ATENCAO_DEPOIS_DE_ABRIR: Duration = Duration::from_secs(90);

pub fn arquivo_de_estado(pasta_dados: &Path) -> PathBuf {
    pasta_dados.join("estado.json")
}

pub fn pasta_de_relatorios(pasta_dados: &Path) -> PathBuf {
    pasta_dados.join("relatorios")
}

pub fn arquivo_de_status(pasta_dados: &Path) -> PathBuf {
    pasta_dados.join("status.json")
}

/// `true` se um vigia está rodando agora.
pub fn vigia_rodando() -> bool {
    sistema::instancia_rodando(NOME_DO_MUTEX)
}

/// O que o vigia está fazendo. Vai para o ícone e para o `status.json`,
/// que o painel lê (os dois não conversam direto).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatusVigia {
    pub pid: u32,
    pub desde_unix: i64,
    pub ativo: bool,
    pub modo: Modo,
    pub administrador: bool,
    /// Recebe o aviso do Windows a cada processo aberto.
    pub eventos_do_windows: bool,
    pub intervalo_segundos: u64,
    pub jogo: Option<String>,
    pub jogo_desde_unix: Option<i64>,
}

/// O status gravado pelo vigia, se houver um vigia rodando.
pub fn ler_status(pasta_dados: &Path) -> Option<StatusVigia> {
    if !vigia_rodando() {
        return None;
    }
    serde_json::from_slice(&std::fs::read(arquivo_de_status(pasta_dados)).ok()?).ok()
}

enum Sinal {
    Acordar,
    ProcessoAbriu,
}

/// Acorda o loop do vigia de outra thread.
#[derive(Clone)]
pub struct Despertador(mpsc::Sender<Sinal>);

impl Despertador {
    /// Olha de novo agora (config mudou, pediram para sair...).
    pub fn acordar(&self) {
        let _ = self.0.send(Sinal::Acordar);
    }

    pub(crate) fn processo_abriu(&self) {
        let _ = self.0.send(Sinal::ProcessoAbriu);
    }
}

/// O lado do loop: espera até o tempo acabar ou alguém acordar.
pub struct Espera(mpsc::Receiver<Sinal>);

pub fn canal() -> (Despertador, Espera) {
    let (tx, rx) = mpsc::channel();
    (Despertador(tx), Espera(rx))
}

impl Espera {
    /// Espera até `segundos` (ou até `parar`). Devolve `true` se um
    /// processo abriu nesse meio-tempo. Com `so_o_tempo`, avisos de
    /// processo não interrompem (as amostras da partida ficam regulares).
    fn esperar(&self, segundos: u64, parar: &AtomicBool, so_o_tempo: bool) -> bool {
        let fim = Instant::now() + Duration::from_secs(segundos.max(1));
        let mut abriu = false;
        while !parar.load(Ordering::SeqCst) {
            let falta = fim.saturating_duration_since(Instant::now());
            if falta.is_zero() {
                break;
            }
            // Pedaços de no máximo 1 s: o Ctrl+C liga a flag sem acordar.
            match self.0.recv_timeout(falta.min(Duration::from_secs(1))) {
                Ok(Sinal::Acordar) => break,
                Ok(Sinal::ProcessoAbriu) => {
                    abriu = true;
                    if !so_o_tempo {
                        break;
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => std::thread::sleep(falta.min(Duration::from_secs(1))),
            }
        }
        abriu
    }
}

struct Partida {
    jogo: Jogo,
    sessao: Option<Sessao<AcoesWindows>>,
    amostrador: Amostrador,
    alteracoes: Vec<String>,
    avisos: Vec<String>,
}

/// Roda até `parar` virar `true`. `log` recebe cada linha de registro e
/// `status` cada mudança de estado.
pub fn rodar(
    caminho_config: &Path,
    parar: &AtomicBool,
    espera: Espera,
    despertador: Despertador,
    log: &mut dyn FnMut(&str),
    status: &mut dyn FnMut(&StatusVigia),
) {
    let pasta_dados = config::pasta_dos_dados_de(caminho_config);
    let estado = arquivo_de_estado(&pasta_dados);

    match restaurar_pendente(&estado, &mut AcoesWindows) {
        Ok(None) => {}
        Ok(Some(r)) => {
            log(&format!("a última execução parou no meio de {}; desfazendo o que ela tinha feito:", r.jogo));
            for d in &r.desfeitas {
                log(&format!("  - {d}"));
            }
            for p in &r.problemas {
                log(&format!("  ! {p}"));
            }
        }
        Err(erro) => log(&format!("não deu para ler {}: {erro}", estado.display())),
    }

    let (mut cfg, mut versao_config) = carregar(caminho_config, log);
    let administrador = sistema::e_administrador();
    let eventos_do_windows = administrador
        && match eventos::assinar(despertador.clone()) {
            Ok(()) => true,
            Err(erro) => {
                log(&format!("sem os avisos do Windows ({erro})"));
                false
            }
        };
    if !administrador {
        log("rodando sem administrador: apps elevados e alguns jogos não podem ser alterados");
    }
    let procura = if eventos_do_windows {
        "o Windows avisa quando um processo abre".to_owned()
    } else {
        format!("procurando o jogo a cada {} s", cfg.modo_jogo.intervalo_segundos)
    };
    log(&format!("vigia iniciado, modo {}; {procura}", cfg.modo_jogo.modo.texto()));

    let mut atual = StatusVigia {
        pid: std::process::id(),
        desde_unix: chrono::Utc::now().timestamp(),
        ativo: cfg.modo_jogo.ativo,
        modo: cfg.modo_jogo.modo,
        administrador,
        eventos_do_windows,
        intervalo_segundos: cfg.modo_jogo.intervalo_segundos,
        jogo: None,
        jogo_desde_unix: None,
    };
    publicar(&pasta_dados, &atual, status);

    let mut fonte = ProcessosDoWindows::default();
    let mut partida: Option<Partida> = None;
    let mut ultimo_processo_aberto = Some(Instant::now());
    while !parar.load(Ordering::SeqCst) {
        // Config editado com o vigia aberto (painel ou ícone): vale na hora
        // para ligar/desligar, e na próxima partida para o resto.
        if data_do_arquivo(caminho_config) != versao_config {
            (cfg, versao_config) = carregar(caminho_config, log);
            log("config recarregado");
            if !cfg.modo_jogo.ativo
                && let Some(p) = partida.take()
            {
                log("modo jogo desligado com o jogo aberto");
                let processos = fonte.processos().unwrap_or_default();
                terminar(p, &cfg, &processos, &pasta_dados, log);
            }
        }
        let processos = match fonte.processos() {
            Ok(processos) => processos,
            Err(erro) => {
                log(&format!("não deu para listar os processos: {erro}"));
                espera.esperar(cfg.modo_jogo.intervalo_segundos, parar, false);
                continue;
            }
        };

        match &mut partida {
            None if cfg.modo_jogo.ativo => {
                if let Some(jogo) = deteccao::detectar(&cfg.modo_jogo, &processos, &mut fonte) {
                    let mut nova = comecar(jogo, &cfg, &processos, &estado, log);
                    // Primeira amostra já com os apps pausados, não da foto de antes.
                    if let Ok(depois) = fonte.processos() {
                        amostrar(&mut nova, &depois);
                    }
                    partida = Some(nova);
                }
            }
            None => {}
            Some(p) if p.jogo.aberto(&processos) => amostrar(p, &processos),
            Some(_) => {
                if let Some(p) = partida.take() {
                    log(&format!("{} fechou", p.jogo.nome));
                    terminar(p, &cfg, &processos, &pasta_dados, log);
                }
            }
        }

        let novo = StatusVigia {
            ativo: cfg.modo_jogo.ativo,
            modo: cfg.modo_jogo.modo,
            intervalo_segundos: cfg.modo_jogo.intervalo_segundos,
            jogo: partida.as_ref().map(|p| p.jogo.nome.clone()),
            jogo_desde_unix: match (&partida, &atual.jogo) {
                (Some(_), Some(_)) => atual.jogo_desde_unix,
                (Some(_), None) => Some(chrono::Utc::now().timestamp()),
                (None, _) => None,
            },
            ..atual.clone()
        };
        if novo != atual {
            atual = novo;
            publicar(&pasta_dados, &atual, status);
        }

        let intervalo = cfg.modo_jogo.intervalo_segundos;
        let atento = ultimo_processo_aberto.is_some_and(|t| t.elapsed() < ATENCAO_DEPOIS_DE_ABRIR);
        let segundos = if partida.is_none() && eventos_do_windows && !atento { ESPERA_COM_EVENTOS } else { intervalo };
        if espera.esperar(segundos, parar, partida.is_some()) {
            ultimo_processo_aberto = Some(Instant::now());
        }
    }

    if let Some(p) = partida.take() {
        log(&format!("saindo com {} ainda aberto", p.jogo.nome));
        let processos = fonte.processos().unwrap_or_default();
        terminar(p, &cfg, &processos, &pasta_dados, log);
    }
    let _ = std::fs::remove_file(arquivo_de_status(&pasta_dados));
    log("vigia encerrado");
}

fn publicar(pasta_dados: &Path, atual: &StatusVigia, status: &mut dyn FnMut(&StatusVigia)) {
    if let Ok(json) = serde_json::to_vec_pretty(atual) {
        let _ = std::fs::create_dir_all(pasta_dados);
        let _ = std::fs::write(arquivo_de_status(pasta_dados), json);
    }
    status(atual);
}

fn comecar(jogo: Jogo, cfg: &Config, processos: &[Processo], estado: &Path, log: &mut dyn FnMut(&str)) -> Partida {
    let m = &cfg.modo_jogo;
    log(&format!(
        "jogo detectado: {} (PID {}, pela {})",
        jogo.nome, jogo.pid, jogo.detectado_por
    ));
    let mut partida = Partida {
        amostrador: Amostrador::novo(&jogo),
        sessao: None,
        alteracoes: Vec::new(),
        avisos: Vec::new(),
        jogo,
    };
    if m.modo == Modo::Otimizar {
        let mut sessao = Sessao::nova(estado.to_path_buf(), &partida.jogo.nome, AcoesWindows);
        let (feitas, avisos) = aplicar(&mut sessao, &partida.jogo, cfg, processos);
        partida.alteracoes = feitas;
        partida.avisos = avisos;
        partida.sessao = Some(sessao);
        for f in &partida.alteracoes {
            log(&format!("  + {f}"));
        }
        for a in &partida.avisos {
            log(&format!("  ! {a}"));
        }
    } else {
        log("  modo observar: nada foi mudado, só anotando");
    }
    partida
}

/// Aplica as otimizações. Devolve (o que foi feito, avisos).
fn aplicar(sessao: &mut Sessao<AcoesWindows>, jogo: &Jogo, cfg: &Config, processos: &[Processo]) -> (Vec<String>, Vec<String>) {
    let m = &cfg.modo_jogo;
    let mut feitas = Vec::new();
    let mut avisos = Vec::new();

    if m.plano_alto_desempenho {
        if m.plano_somente_na_tomada && sistema::na_tomada() == Some(false) {
            avisos.push("na bateria: o plano de energia não foi trocado".into());
        } else {
            match sessao.trocar_plano(ALTO_DESEMPENHO) {
                Ok(true) => feitas.push("plano de energia: Alto desempenho".into()),
                Ok(false) => {}
                Err(erro) => avisos.push(format!("não deu para trocar o plano de energia: {erro}")),
            }
        }
    }

    if m.prioridade != Prioridade::Normal
        && let Some(p) = processos.iter().find(|p| p.pid == jogo.pid && p.criado_em == jogo.criado_em)
    {
        match sessao.definir_prioridade(p, m.prioridade.classe()) {
            Ok(true) => feitas.push(format!("prioridade {} para {}", nome_da_prioridade(m.prioridade), p.nome)),
            Ok(false) => {}
            Err(erro) => avisos.push(format!("o Windows não deixou mudar a prioridade do jogo ({erro}); seguiu sem mudar")),
        }
    }

    let protecao = Protecao::nova(processos, Some(jogo.pid));
    // Só os apps da conta de quem está jogando. Os de outra conta aberta ao
    // mesmo tempo ("Trocar de usuário") são de outra pessoa: nunca são mexidos.
    let da_conta = da_conta_do_jogo(processos, jogo);
    // Antes de pausar: assim a memória que os pausados devolvem também
    // entra na lista de espera com prioridade baixa e sai antes da do jogo.
    if m.memoria_baixa_nos_apps {
        let alvos = apps_de_fundo(&da_conta, &protecao, &jogo.nome, &m.fechar);
        match sessao.baixar_memoria(&alvos, MEMORIA_BAIXA) {
            Ok(0) => {}
            Ok(n) => feitas.push(format!("prioridade de memória baixa para {n} processos em segundo plano")),
            Err(erro) => avisos.push(format!("não deu para baixar a prioridade de memória dos apps: {erro}")),
        }
    }
    for (lista, fechar) in [(&m.fechar, true), (&m.pausar, false)] {
        let nomes: BTreeSet<String> = lista.iter().map(|n| n.to_lowercase()).collect();
        for nome in &nomes {
            let alvos: Vec<&Processo> = da_conta.iter().copied().filter(|p| p.nome.to_lowercase() == *nome).collect();
            if let Some(motivo) = alvos.iter().find_map(|p| protecao.motivo(p)) {
                avisos.push(format!("{nome} não foi mexido: {}", motivo.descricao()));
                continue;
            }
            let mut feitos = 0;
            let mut erros = Vec::new();
            for p in &alvos {
                if fechar {
                    // Um app com vários processos tem janela em um só; os outros falham.
                    if sessao.fechar(p).is_ok() {
                        feitos += 1;
                    }
                } else {
                    match sessao.pausar(p) {
                        Ok(None) => feitos += 1,
                        Ok(Some(erro)) => {
                            feitos += 1;
                            erros.push(format!("RAM não devolvida: {erro}"));
                        }
                        Err(erro) => erros.push(erro.to_string()),
                    }
                }
            }
            let verbo = if fechar { "pediu para fechar" } else { "pausou" };
            match (alvos.len(), feitos) {
                (0, _) => {}
                (_, 0) if fechar => avisos.push(format!("{nome} não tem janela aberta para fechar")),
                (_, 0) => avisos.push(format!("não deu para pausar {nome}: {}", erros.join("; "))),
                (total, feitos) => {
                    feitas.push(format!("{verbo} {nome} ({feitos} de {total} processos)"));
                    if !erros.is_empty() {
                        avisos.push(format!("{nome}: {}", erros.join("; ")));
                    }
                }
            }
        }
    }
    (feitas, avisos)
}

/// Os processos da mesma sessão do Windows que o jogo. Se o jogo já fechou,
/// nenhum.
fn da_conta_do_jogo<'a>(processos: &'a [Processo], jogo: &Jogo) -> Vec<&'a Processo> {
    let Some(sessao) = processos.iter().find(|p| p.pid == jogo.pid && p.criado_em == jogo.criado_em).map(|p| p.sessao)
    else {
        return Vec::new();
    };
    processos.iter().filter(|p| p.sessao == sessao).collect()
}

/// Quem fica com a prioridade de memória baixa durante o jogo: tudo o que
/// não é protegido (Windows, serviços, anti-cheat, o jogo e o que ele
/// abriu), nem outro processo com o nome do jogo, nem launcher ou overlay
/// (o overlay aparece em cima do jogo), nem o que vai ser fechado.
fn apps_de_fundo<'a>(processos: &[&'a Processo], protecao: &Protecao, jogo: &str, fechar: &[String]) -> Vec<&'a Processo> {
    processos
        .iter()
        .copied()
        .filter(|p| protecao.motivo(p).is_none())
        .filter(|p| {
            let nome = p.nome.to_lowercase();
            !nome.eq_ignore_ascii_case(jogo)
                && !LAUNCHERS.contains(&nome.as_str())
                && !fechar.iter().any(|f| f.eq_ignore_ascii_case(&nome))
        })
        .collect()
}

fn amostrar(p: &mut Partida, processos: &[Processo]) {
    if let Ok((memoria, paginacao)) = ram::memoria_e_paginacao() {
        p.amostrador.amostrar(processos, &memoria, paginacao);
    }
}

fn terminar(p: Partida, cfg: &Config, processos: &[Processo], pasta_dados: &Path, log: &mut dyn FnMut(&str)) {
    let mut avisos = p.avisos;
    if let Some(sessao) = p.sessao {
        let quantas = sessao.alteracoes().len();
        let problemas = sessao.encerrar();
        if quantas > 0 {
            log(&format!("  desfez {} alterações", quantas - problemas.len()));
        }
        for problema in problemas {
            log(&format!("  ! {problema}"));
            avisos.push(problema);
        }
    }
    let nas_listas: Vec<String> = cfg.modo_jogo.pausar.iter().chain(&cfg.modo_jogo.fechar).cloned().collect();
    let (r, amostras) =
        p.amostrador.finalizar(nome_do_modo(cfg.modo_jogo.modo), p.alteracoes, avisos, processos, &nas_listas);
    match relatorio::gravar(&pasta_de_relatorios(pasta_dados), &r, &amostras) {
        Ok(caminho) => log(&format!("  relatório: {}", caminho.display())),
        Err(erro) => log(&format!("  ! não deu para gravar o relatório: {erro}")),
    }
    for s in &r.sugestoes {
        log(&format!("  sugestão: {s}"));
    }
}

fn carregar(caminho: &Path, log: &mut dyn FnMut(&str)) -> (Config, Option<SystemTime>) {
    let versao = data_do_arquivo(caminho);
    match config::carregar(caminho) {
        Ok(cfg) => (cfg, versao),
        Err(erro) => {
            log(&format!("{erro}\nusando os valores padrão até o arquivo ser corrigido"));
            (Config::default(), versao)
        }
    }
}

fn data_do_arquivo(caminho: &Path) -> Option<SystemTime> {
    std::fs::metadata(caminho).and_then(|m| m.modified()).ok()
}

fn nome_do_modo(modo: Modo) -> &'static str {
    match modo {
        Modo::Observar => "observar",
        Modo::Otimizar => "otimizar",
    }
}

fn nome_da_prioridade(p: Prioridade) -> &'static str {
    match p {
        Prioridade::Normal => "normal",
        Prioridade::AcimaDoNormal => "acima do normal",
        Prioridade::Alta => "alta",
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::modo_jogo::protecao::testes::proc;

    #[test]
    fn so_apps_de_fundo_ficam_com_memoria_baixa() {
        let lista = [
            proc(100, 4, "explorer.exe", 1),
            proc(200, 100, "RiotClientServices.exe", 2),
            proc(300, 200, "VALORANT-Win64-Shipping.exe", 3),
            proc(310, 999, "VALORANT-Win64-Shipping.exe", 3),
            proc(400, 300, "CrashHandler.exe", 4),
            proc(500, 100, "Discord.exe", 5),
            proc(600, 7, "vgc.exe", 6),
            proc(700, 100, "opera.exe", 7),
            proc(800, 100, "steamwebhelper.exe", 8),
            proc(900, 100, "OneDrive.exe", 9),
        ];
        let protecao = Protecao::nova(&lista, Some(300));
        let fechar = ["onedrive.exe".to_owned()];
        let todos: Vec<&Processo> = lista.iter().collect();
        let alvos = apps_de_fundo(&todos, &protecao, "VALORANT-Win64-Shipping.exe", &fechar);
        let nomes: Vec<&str> = alvos.iter().map(|p| p.nome.as_str()).collect();
        assert_eq!(nomes, ["Discord.exe", "opera.exe"]);
    }

    #[test]
    fn nunca_mexe_em_apps_de_outra_conta() {
        let mut lista = vec![
            proc(300, 1, "jogo.exe", 3),
            proc(500, 1, "Discord.exe", 5),
            proc(700, 1, "OneDrive.exe", 7),
            proc(800, 1, "OneDrive.exe", 8),
        ];
        for p in &mut lista {
            p.sessao = 2;
        }
        lista[2].sessao = 1; // OneDrive de outra conta aberta ao mesmo tempo
        let jogo = Jogo { pid: 300, criado_em: 3, nome: "jogo.exe".into(), caminho: None, detectado_por: String::new() };
        let pids: Vec<u32> = da_conta_do_jogo(&lista, &jogo).iter().map(|p| p.pid).collect();
        assert_eq!(pids, [300, 500, 800]);
        // Jogo que já fechou: nada é mexido.
        let fechado = Jogo { pid: 999, ..jogo };
        assert!(da_conta_do_jogo(&lista, &fechado).is_empty());
    }
}
