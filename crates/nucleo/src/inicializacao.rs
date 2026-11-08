//! Programas que abrem com o Windows (só leitura): chaves Run do Registro,
//! pastas Inicializar e o estado que o Gerenciador de Tarefas grava quando
//! você desativa um deles (chaves `StartupApproved`).
//!
//! Ainda não cobre apps da Microsoft Store com inicialização própria nem
//! tarefas agendadas.

use std::path::PathBuf;

use serde::Serialize;
use winreg::RegKey;
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, REG_EXPAND_SZ, REG_SZ};
use winreg::types::FromRegValue;

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_32: &str = r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Run";
const APROVADOS: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved";

#[derive(Debug, Clone, Serialize)]
pub struct ProgramaInicial {
    pub nome: String,
    /// Linha de comando (Registro) ou caminho do atalho (pasta Inicializar).
    pub comando: String,
    pub origem: &'static str,
    pub ativo: bool,
    /// Quando foi desativado no Gerenciador de Tarefas, em segundos desde 1970.
    pub desativado_em_unix: Option<i64>,
}

pub fn listar() -> Vec<ProgramaInicial> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let mut lista = Vec::new();

    for (raiz, chave, aprovados, origem) in [
        (&hkcu, RUN, "Run", "Registro (só você)"),
        (&hklm, RUN, "Run", "Registro (todos os usuários)"),
        (&hklm, RUN_32, "Run32", "Registro (todos, 32 bits)"),
    ] {
        let Ok(chave) = raiz.open_subkey(chave) else { continue };
        let aprovados = raiz.open_subkey(format!(r"{APROVADOS}\{aprovados}")).ok();
        for (nome, valor) in chave.enum_values().flatten() {
            if !matches!(valor.vtype, REG_SZ | REG_EXPAND_SZ) {
                continue;
            }
            let comando = String::from_reg_value(&valor).unwrap_or_default();
            let (ativo, desativado_em_unix) = estado(aprovados.as_ref(), &nome);
            lista.push(ProgramaInicial { nome, comando, origem, ativo, desativado_em_unix });
        }
    }

    let pastas = [
        (
            std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join(r"Microsoft\Windows\Start Menu\Programs\Startup")),
            &hkcu,
            "Pasta Inicializar (só você)",
        ),
        (
            std::env::var_os("ProgramData").map(|a| PathBuf::from(a).join(r"Microsoft\Windows\Start Menu\Programs\StartUp")),
            &hklm,
            "Pasta Inicializar (todos)",
        ),
    ];
    for (pasta, raiz, origem) in pastas {
        let Some(Ok(leitura)) = pasta.map(std::fs::read_dir) else { continue };
        let aprovados = raiz.open_subkey(format!(r"{APROVADOS}\StartupFolder")).ok();
        for entrada in leitura.flatten() {
            let arquivo = entrada.file_name().to_string_lossy().into_owned();
            if arquivo.eq_ignore_ascii_case("desktop.ini") {
                continue;
            }
            let (ativo, desativado_em_unix) = estado(aprovados.as_ref(), &arquivo);
            let nome = arquivo.strip_suffix(".lnk").unwrap_or(&arquivo).to_owned();
            let comando = entrada.path().display().to_string();
            lista.push(ProgramaInicial { nome, comando, origem, ativo, desativado_em_unix });
        }
    }

    lista.sort_by(|a, b| b.ativo.cmp(&a.ativo).then_with(|| a.nome.to_lowercase().cmp(&b.nome.to_lowercase())));
    lista
}

/// Lê o valor que o Gerenciador de Tarefas grava em `StartupApproved`.
/// Sem valor = ativo.
fn estado(aprovados: Option<&RegKey>, nome: &str) -> (bool, Option<i64>) {
    let Some(valor) = aprovados.and_then(|chave| chave.get_raw_value(nome).ok()) else {
        return (true, None);
    };
    interpretar_aprovado(&valor.bytes)
}

/// Primeiro byte par (02, 06) = ativo; ímpar (03, 07) = desativado.
/// Quando desativado, os bytes 4..12 são o FILETIME do momento.
fn interpretar_aprovado(bytes: &[u8]) -> (bool, Option<i64>) {
    match bytes.first() {
        Some(primeiro) if primeiro & 1 == 1 => {
            let quando = bytes
                .get(4..12)
                .map(|b| u64::from_le_bytes(b.try_into().unwrap_or_default()))
                .filter(|&filetime| filetime != 0)
                .map(|filetime| (filetime / 10_000_000) as i64 - 11_644_473_600);
            (false, quando)
        }
        _ => (true, None),
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn le_o_formato_do_gerenciador_de_tarefas() {
        assert_eq!(interpretar_aprovado(&[2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]), (true, None));
        assert_eq!(interpretar_aprovado(&[]), (true, None));
        // 01/01/2025 00:00 UTC = 1735689600 = FILETIME 133801632000000000
        let filetime = 133_801_632_000_000_000u64.to_le_bytes();
        let mut desativado = vec![3, 0, 0, 0];
        desativado.extend_from_slice(&filetime);
        assert_eq!(interpretar_aprovado(&desativado), (false, Some(1_735_689_600)));
    }

    #[test]
    fn lista_sem_erro() {
        for programa in listar() {
            assert!(!programa.nome.is_empty());
        }
    }
}
