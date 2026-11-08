//! Painel do Makeway (makeway.exe). Por enquanto é uma ferramenta de
//! terminal; a interface em egui chega na Fase 4.

mod imprimir;

use std::error::Error;
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, SystemTime};

use clap::{Parser, Subcommand};
use nucleo::config::{self, Config};
use nucleo::disco::{self, Contadores};
use nucleo::formato::{decimal, tamanho};
use nucleo::modo_jogo::{relatorio, sessao, vigia};
use nucleo::{duplicados, inicializacao, ram, temporarios};
use serde::Serialize;

#[derive(Parser)]
#[command(name = "makeway", version, about = "Makeway: tira do caminho o que compete com o jogo")]
struct Cli {
    /// Caminho do config.toml [padrão: %APPDATA%\Makeway\config.toml]
    #[arg(long, global = true, value_name = "ARQUIVO")]
    config: Option<PathBuf>,

    #[command(subcommand)]
    comando: Comando,
}

#[derive(Subcommand)]
enum Comando {
    /// Memória do sistema, arquivo de paginação e apps que mais usam RAM
    Ram {
        /// Quantos itens mostrar [padrão: ram.top_apps do config]
        #[arg(long, value_name = "N")]
        top: Option<usize>,
        /// Lista processo a processo, sem agrupar (como a aba Detalhes)
        #[arg(long)]
        processos: bool,
        /// Saída em JSON, para outros programas lerem
        #[arg(long)]
        json: bool,
    },
    /// Maiores pastas e arquivos (só lê, não apaga nada)
    Disco {
        /// Pasta a varrer [padrão: a unidade do Windows inteira]
        pasta: Option<PathBuf>,
        /// Quantos níveis de subpastas mostrar [padrão: disco.profundidade]
        #[arg(long, value_name = "N")]
        profundidade: Option<usize>,
        /// Quantas pastas por nível e quantos arquivos [padrão: disco.top]
        #[arg(long, value_name = "N")]
        top: Option<usize>,
        #[arg(long)]
        json: bool,
    },
    /// Arquivos com o mesmo conteúdo (só lê, não apaga nada)
    Duplicados {
        /// Pasta a varrer [padrão: sua pasta de usuário]
        pasta: Option<PathBuf>,
        /// Ignora arquivos menores que isso [padrão: disco.duplicados_min_mb]
        #[arg(long, value_name = "MB")]
        min_mb: Option<u64>,
        /// Quantos grupos mostrar [padrão: disco.top]
        #[arg(long, value_name = "N")]
        top: Option<usize>,
        #[arg(long)]
        json: bool,
    },
    /// Temporários antigos; com --apagar, manda para a Lixeira depois de confirmar
    Temporarios {
        /// Pergunta e, se você confirmar, manda os itens antigos para a Lixeira
        #[arg(long, conflicts_with = "json")]
        apagar: bool,
        #[arg(long)]
        json: bool,
    },
    /// Programas que abrem com o Windows, inclusive os desativados
    Inicializacao {
        #[arg(long)]
        json: bool,
    },
    /// Relatórios das partidas gravados pelo vigia
    Sessoes {
        /// Mostra os detalhes de uma partida (1 = a mais recente)
        numero: Option<usize>,
        #[arg(long)]
        json: bool,
    },
    /// Desfaz o que o modo jogo deixou para trás se o vigia foi fechado à força
    Restaurar,
    /// Mostra onde fica o config.toml e o cria com os valores padrão se faltar
    Config,
}

fn main() -> ExitCode {
    match executar(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(erro) => {
            eprintln!("erro: {erro}");
            ExitCode::FAILURE
        }
    }
}

fn executar(cli: Cli) -> Result<(), Box<dyn Error>> {
    let caminho = match cli.config {
        Some(caminho) => caminho,
        None => config::caminho_padrao().ok_or("a variável APPDATA não existe; use --config")?,
    };
    if let Comando::Config = cli.comando {
        let criado = config::criar_se_faltar(&caminho)?;
        config::carregar(&caminho)?; // confere se o arquivo atual é válido
        let situacao = if criado { "criado agora com os valores padrão" } else { "já existe e está válido" };
        println!("{} ({situacao})", caminho.display());
        return Ok(());
    }
    let cfg: Config = config::carregar(&caminho)?;

    match cli.comando {
        Comando::Config => unreachable!(),
        Comando::Ram { top, processos, json } => {
            let relatorio = ram::analisar()?;
            if json {
                return mostrar_json(&relatorio);
            }
            imprimir::ram(&relatorio, top.unwrap_or(cfg.ram.top_apps), processos);
        }
        Comando::Disco { pasta, profundidade, top, json } => {
            let pasta = pasta.unwrap_or_else(unidade_do_windows);
            let profundidade = profundidade.unwrap_or(cfg.disco.profundidade);
            let top = top.unwrap_or(cfg.disco.top);
            let c = Contadores::default();
            let varredura = com_progresso(&c, || disco::varrer(&pasta, profundidade, top, &c));
            if json {
                return mostrar_json(&varredura);
            }
            imprimir::disco(&varredura, top);
        }
        Comando::Duplicados { pasta, min_mb, top, json } => {
            let pasta = match pasta {
                Some(pasta) => pasta,
                None => std::env::var_os("USERPROFILE").map(PathBuf::from).ok_or("USERPROFILE não existe")?,
            };
            let min_mb = min_mb.unwrap_or(cfg.disco.duplicados_min_mb);
            let c = Contadores::default();
            let resultado = com_progresso(&c, || duplicados::procurar(&pasta, min_mb << 20, &c));
            if json {
                return mostrar_json(&resultado);
            }
            imprimir::duplicados(&pasta, min_mb, &resultado, top.unwrap_or(cfg.disco.top));
        }
        Comando::Temporarios { apagar, json } => {
            let pastas = temporarios::pastas_padrao();
            let analise = temporarios::analisar(&pastas, cfg.temporarios.idade_minima_dias, SystemTime::now());
            if json {
                return mostrar_json(&analise);
            }
            imprimir::temporarios(&analise, apagar);
            if apagar && !analise.antigos.is_empty() {
                apagar_temporarios(&analise)?;
            }
        }
        Comando::Inicializacao { json } => {
            let programas = inicializacao::listar();
            if json {
                return mostrar_json(&programas);
            }
            imprimir::inicializacao(&programas);
        }
        Comando::Sessoes { numero, json } => {
            let pasta = vigia::pasta_de_relatorios(&config::pasta_dos_dados_de(&caminho));
            let relatorios = relatorio::listar(&pasta);
            match numero {
                None if json => {
                    let lista: Vec<_> = relatorios.iter().map(|(_, r)| r).collect();
                    return mostrar_json(&lista);
                }
                None => imprimir::sessoes(&pasta, &relatorios),
                Some(n) => {
                    let (arquivo, r) = relatorios
                        .get(n.wrapping_sub(1))
                        .ok_or_else(|| format!("não existe a partida {n}; há {} gravadas", relatorios.len()))?;
                    if json {
                        return mostrar_json(r);
                    }
                    imprimir::sessao(arquivo, r);
                }
            }
        }
        Comando::Restaurar => {
            if vigia::vigia_rodando() {
                return Err("o vigia está rodando e desfaz tudo sozinho quando o jogo fecha. \
                            Feche o vigia (Ctrl+C) se quiser restaurar agora."
                    .into());
            }
            let estado = vigia::arquivo_de_estado(&config::pasta_dos_dados_de(&caminho));
            match sessao::restaurar_pendente(&estado, &mut sessao::AcoesWindows)? {
                None => println!("Nada pendente: o modo jogo não deixou nada para trás."),
                Some(r) => imprimir::restauracao(&r),
            }
        }
    }
    Ok(())
}

fn apagar_temporarios(analise: &temporarios::AnaliseTemporarios) -> Result<(), Box<dyn Error>> {
    print!(
        "\nMandar {} itens ({}) para a Lixeira? [s/N] ",
        decimal(analise.antigos.len() as f64, 0),
        tamanho(analise.bytes_antigos())
    );
    io::stdout().flush()?;
    let mut resposta = String::new();
    io::stdin().read_line(&mut resposta)?;
    if !matches!(resposta.trim().to_lowercase().as_str(), "s" | "sim") {
        println!("Nada foi apagado.");
        return Ok(());
    }
    let resultado = temporarios::mandar_para_lixeira(&analise.antigos);
    imprimir::limpeza(&resultado);
    Ok(())
}

/// Roda a varredura e mostra quantos arquivos já foram lidos, se a saída
/// de erro for um terminal (não suja a saída quando vira arquivo ou JSON).
fn com_progresso<T: Send>(c: &Contadores, tarefa: impl FnOnce() -> T + Send) -> T {
    let mostrar = io::stderr().is_terminal();
    std::thread::scope(|s| {
        let trabalho = s.spawn(tarefa);
        while !trabalho.is_finished() {
            std::thread::sleep(Duration::from_millis(100));
            if mostrar {
                eprint!("\r  {} arquivos lidos...", decimal(c.arquivos_vistos() as f64, 0));
            }
        }
        if mostrar {
            eprint!("\r{:60}\r", "");
        }
        trabalho.join().unwrap_or_else(|panico| std::panic::resume_unwind(panico))
    })
}

fn unidade_do_windows() -> PathBuf {
    let unidade = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
    PathBuf::from(format!(r"{unidade}\"))
}

fn mostrar_json(valor: &impl Serialize) -> Result<(), Box<dyn Error>> {
    println!("{}", serde_json::to_string_pretty(valor)?);
    Ok(())
}
