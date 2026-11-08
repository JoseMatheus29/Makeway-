//! Nomes que uma pessoa reconhece: "Opera GX" em vez de `opera.exe`,
//! "Antivírus do Windows" em vez de `MsMpEng.exe`.

use std::collections::HashMap;
use std::path::Path;

use crate::ram::{App, Processo};
use crate::sistema;

/// Partes do Windows com nome técnico demais.
fn conhecido(exe: &str) -> Option<&'static str> {
    Some(match exe {
        "memory compression" => "Memória compactada do Windows",
        "system" => "Windows (núcleo)",
        "secure system" => "Windows (segurança)",
        "registry" => "Windows (registro)",
        "svchost.exe" => "Serviços do Windows",
        "msmpeng.exe" => "Antivírus do Windows",
        "explorer.exe" => "Explorador de Arquivos",
        "dwm.exe" => "Janelas do Windows",
        "searchhost.exe" => "Pesquisa do Windows",
        "startmenuexperiencehost.exe" => "Menu Iniciar",
        "msedgewebview2.exe" => "Microsoft Edge WebView2",
        "wmiprvse.exe" => "Informações do sistema (WMI)",
        "makeway.exe" => "Makeway",
        "makeway-vigia.exe" => "Makeway (vigia)",
        _ => return None,
    })
}

/// Nome amigável de um .exe, usando a descrição gravada no arquivo.
pub fn amigavel(exe: &str, caminho: Option<&Path>) -> String {
    let minusculo = exe.to_lowercase();
    if let Some(nome) = conhecido(&minusculo) {
        return nome.to_owned();
    }
    if let Some(descricao) = caminho.and_then(sistema::descricao_do_executavel) {
        // Descrições genéricas não ajudam ninguém.
        if !matches!(descricao.to_lowercase().as_str(), "" | "application" | "app" | "main") {
            let curta = descricao.strip_suffix(" Internet Browser").unwrap_or(&descricao);
            return curta.chars().take(48).collect();
        }
    }
    exe.strip_suffix(".exe").or_else(|| exe.strip_suffix(".EXE")).unwrap_or(exe).to_owned()
}

/// Nomes amigáveis dos `quantos` maiores apps, pulando os que já estão em
/// `conhecidos` (a chave é o nome do .exe em minúsculas).
pub fn dos_apps(apps: &[App], processos: &[Processo], quantos: usize, conhecidos: &HashMap<String, String>) -> Vec<(String, String)> {
    apps.iter()
        .take(quantos)
        .filter(|a| !conhecidos.contains_key(&a.nome.to_lowercase()))
        .map(|a| {
            let caminho = a
                .pids
                .iter()
                .find_map(|pid| processos.iter().find(|p| p.pid == *pid))
                .and_then(|p| sistema::caminho_processo(p.pid).ok());
            (a.nome.to_lowercase(), amigavel(&a.nome, caminho.as_deref()))
        })
        .collect()
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn nomes_do_windows_e_descricao_do_arquivo() {
        assert_eq!(amigavel("MsMpEng.exe", None), "Antivírus do Windows");
        assert_eq!(amigavel("jogo.exe", None), "jogo");
        let notepad = Path::new(r"C:\Windows\System32\notepad.exe");
        if notepad.exists() {
            assert_ne!(amigavel("notepad.exe", Some(notepad)), "notepad");
        }
    }
}
