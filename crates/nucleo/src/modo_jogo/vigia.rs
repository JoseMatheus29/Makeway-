//! O loop do vigia: procura o jogo, abre a sessão, anota amostras e,
//! quando o jogo fecha, desfaz tudo e grava o relatório.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime};

use super::deteccao::{self, FonteProcessos, Jogo, ProcessosDoWindows};
use super::protecao::Protecao;
use super::relatorio::{self, Amostrador};
use super::sessao::{ALTO_DESEMPENHO, AcoesWindows, Sessao, restaurar_pendente};
use crate::config::{self, Config, Modo, Prioridade};
use crate::ram::{self, Processo};
use crate::sistema;

pub const NOME_DO_MUTEX: &str = r"Local\Makeway-Vigia";

pub fn arquivo_de_estado(pasta_dados: &Path) -> PathBuf {
    pasta_dados.join("estado.json")
}

pub fn pasta_de_relatorios(pasta_dados: &Path) -> PathBuf {
    pasta_dados.join("relatorios")
}

/// `true` se um vigia está rodando agora.
pub fn vigia_rodando() -> bool {
    sistema::instancia_rodando(NOME_DO_MUTEX)
}

struct Partida {
    jogo: Jogo,
    sessao: Option<Sessao<AcoesWindows>>,
    amostrador: Amostrador,
    alteracoes: Vec<String>,
    avisos: Vec<String>,
}

/// Roda até `parar` virar `true`. `log` recebe cada linha de registro.
pub fn rodar(caminho_config: &Path, parar: &AtomicBool, log: &mut dyn FnMut(&str)) {
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
    if !sistema::e_administrador() {
        log("rodando sem administrador: apps elevados e alguns jogos não podem ser alterados");
    }
    log(&format!(
        "vigiando a cada {} s, modo {} (Ctrl+C para sair)",
        cfg.modo_jogo.intervalo_segundos,
        nome_do_modo(cfg.modo_jogo.modo)
    ));

    let mut fonte = ProcessosDoWindows::default();
    let mut partida: Option<Partida> = None;
    while !parar.load(Ordering::SeqCst) {
        // Config editado com o vigia aberto: vale a partir da próxima partida.
        if partida.is_none() && data_do_arquivo(caminho_config) != versao_config {
            (cfg, versao_config) = carregar(caminho_config, log);
            log("config recarregado");
        }
        let processos = match fonte.processos() {
            Ok(processos) => processos,
            Err(erro) => {
                log(&format!("não deu para listar os processos: {erro}"));
                esperar(cfg.modo_jogo.intervalo_segundos, parar);
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
        esperar(cfg.modo_jogo.intervalo_segundos, parar);
    }

    if let Some(p) = partida.take() {
        log(&format!("saindo com {} ainda aberto", p.jogo.nome));
        let processos = fonte.processos().unwrap_or_default();
        terminar(p, &cfg, &processos, &pasta_dados, log);
    }
    log("vigia encerrado");
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
    for (lista, fechar) in [(&m.fechar, true), (&m.pausar, false)] {
        let nomes: BTreeSet<String> = lista.iter().map(|n| n.to_lowercase()).collect();
        for nome in &nomes {
            let alvos: Vec<&Processo> = processos.iter().filter(|p| p.nome.to_lowercase() == *nome).collect();
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

/// Dorme em pedaços de 100 ms para responder rápido ao Ctrl+C.
fn esperar(segundos: u64, parar: &AtomicBool) {
    let fim = Instant::now() + Duration::from_secs(segundos.max(1));
    while Instant::now() < fim && !parar.load(Ordering::SeqCst) {
        std::thread::sleep(Duration::from_millis(100));
    }
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
