//! Vigia do Makeway (makeway-vigia.exe): fica rodando, percebe quando um
//! jogo abre e cuida do modo jogo. O ícone perto do relógio chega na Fase 4.
//!
//! Uso: makeway-vigia [--config ARQUIVO]

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use nucleo::config;
use nucleo::formato::hora_agora;
use nucleo::modo_jogo::{self, vigia};

/// Acima disso o registro antigo vira vigia.log.1 e um novo começa.
const LIMITE_DO_LOG: u64 = 1 << 20;

fn main() -> ExitCode {
    let caminho = match caminho_do_config() {
        Ok(caminho) => caminho,
        Err(erro) => {
            eprintln!("makeway-vigia: {erro}");
            return ExitCode::FAILURE;
        }
    };
    // Uma instância só: duas tentando pausar e retomar os mesmos apps
    // se atrapalhariam.
    let _instancia = match modo_jogo::instancia_unica(vigia::NOME_DO_MUTEX) {
        Ok(Some(instancia)) => instancia,
        Ok(None) => {
            eprintln!("makeway-vigia: já tem um vigia rodando");
            return ExitCode::FAILURE;
        }
        Err(erro) => {
            eprintln!("makeway-vigia: não deu para criar o mutex: {erro}");
            return ExitCode::FAILURE;
        }
    };
    let parar = modo_jogo::tratar_ctrl_c();

    let mut arquivo_log = abrir_log(&config::pasta_dos_dados_de(&caminho));
    let mut log = |linha: &str| {
        let linha = format!("{} {linha}", hora_agora());
        println!("{linha}");
        if let Some(arquivo) = arquivo_log.as_mut() {
            let _ = writeln!(arquivo, "{linha}");
        }
    };
    vigia::rodar(&caminho, parar, &mut log);
    modo_jogo::avisar_limpeza_feita();
    ExitCode::SUCCESS
}

fn caminho_do_config() -> Result<PathBuf, String> {
    let mut args = std::env::args().skip(1);
    match (args.next().as_deref(), args.next()) {
        (None, _) => config::caminho_padrao().ok_or_else(|| "a variável APPDATA não existe".into()),
        (Some("--config"), Some(caminho)) => Ok(PathBuf::from(caminho)),
        _ => Err("uso: makeway-vigia [--config ARQUIVO]".into()),
    }
}

fn abrir_log(pasta: &Path) -> Option<File> {
    fs::create_dir_all(pasta).ok()?;
    let caminho = pasta.join("vigia.log");
    if fs::metadata(&caminho).is_ok_and(|m| m.len() > LIMITE_DO_LOG) {
        let _ = fs::rename(&caminho, pasta.join("vigia.log.1"));
    }
    let mut arquivo = OpenOptions::new().create(true).append(true).open(caminho).ok()?;
    // BOM no arquivo novo: o PowerShell 5.1 e editores antigos leem os acentos certo.
    if arquivo.metadata().is_ok_and(|m| m.len() == 0) {
        let _ = arquivo.write_all("\u{feff}".as_bytes());
    }
    Some(arquivo)
}
