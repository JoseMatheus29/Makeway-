//! Jogos instalados, lidos dos registros das lojas (só leitura, rápido:
//! nenhuma pasta é varrida).

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use winreg::RegKey;
use winreg::enums::HKEY_CURRENT_USER;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct JogoInstalado {
    pub nome: String,
    pub loja: &'static str,
    pub pasta: PathBuf,
}

/// Steam, Epic, Riot, Xbox e Ubisoft, em ordem alfabética.
pub fn instalados() -> Vec<JogoInstalado> {
    let mut lista = Vec::new();
    lista.extend(steam());
    lista.extend(epic());
    lista.extend(pastas(r"C:\Riot Games", "Riot", &["riot client"]));
    lista.extend(pastas(r"C:\XboxGames", "Xbox", &["gamesave"]));
    lista.extend(pastas(r"C:\Program Files (x86)\Ubisoft\Ubisoft Game Launcher\games", "Ubisoft", &[]));
    lista.sort_by_key(|j| j.nome.to_lowercase());
    let mut vistas: Vec<PathBuf> = Vec::new();
    lista.retain(|j| {
        let nova = !vistas.iter().any(|v| mesma_pasta(v, &j.pasta));
        vistas.push(j.pasta.clone());
        nova
    });
    lista
}

/// No Windows, maiúsculas e barras não mudam a pasta.
fn mesma_pasta(a: &Path, b: &Path) -> bool {
    let normal = |p: &Path| p.to_string_lossy().replace('/', "\\").trim_end_matches('\\').to_lowercase();
    normal(a) == normal(b)
}

fn steam() -> Vec<JogoInstalado> {
    let raiz = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Valve\Steam")
        .and_then(|k| k.get_value::<String, _>("SteamPath"))
        .map(|p| PathBuf::from(p.replace('/', "\\")))
        .unwrap_or_else(|_| PathBuf::from(r"C:\Program Files (x86)\Steam"));
    let mut bibliotecas = vec![raiz.clone()];
    if let Ok(texto) = fs::read_to_string(raiz.join(r"steamapps\libraryfolders.vdf")) {
        for caminho in valores(&texto, "path") {
            let caminho = PathBuf::from(caminho.replace("\\\\", "\\"));
            // O registro guarda o caminho em minúsculas e o .vdf não.
            if !bibliotecas.iter().any(|b| mesma_pasta(b, &caminho)) {
                bibliotecas.push(caminho);
            }
        }
    }
    let mut jogos = Vec::new();
    for biblioteca in bibliotecas {
        let Ok(leitura) = fs::read_dir(biblioteca.join("steamapps")) else { continue };
        for entrada in leitura.flatten() {
            let nome_arquivo = entrada.file_name().to_string_lossy().to_lowercase();
            if !(nome_arquivo.starts_with("appmanifest_") && nome_arquivo.ends_with(".acf")) {
                continue;
            }
            let Ok(texto) = fs::read_to_string(entrada.path()) else { continue };
            let (Some(nome), Some(pasta)) = (valores(&texto, "name").next(), valores(&texto, "installdir").next()) else {
                continue;
            };
            let pasta = biblioteca.join(r"steamapps\common").join(pasta);
            if pasta.is_dir() && !e_ferramenta(&nome) {
                jogos.push(JogoInstalado { nome, loja: "Steam", pasta });
            }
        }
    }
    jogos
}

/// Ferramentas que a Steam instala junto e que não são jogos.
fn e_ferramenta(nome: &str) -> bool {
    let n = nome.to_lowercase();
    ["redistributable", "steamworks", "proton", "steam linux runtime", "steamvr", "dedicated server", "sdk"]
        .iter()
        .any(|t| n.contains(t))
}

fn epic() -> Vec<JogoInstalado> {
    let pasta = Path::new(r"C:\ProgramData\Epic\EpicGamesLauncher\Data\Manifests");
    let Ok(leitura) = fs::read_dir(pasta) else { return Vec::new() };
    leitura
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("item")))
        .filter_map(|e| {
            let json: serde_json::Value = serde_json::from_slice(&fs::read(e.path()).ok()?).ok()?;
            let e_jogo = json["AppCategories"]
                .as_array()
                .is_some_and(|c| c.iter().any(|v| v.as_str() == Some("games")));
            let nome = json["DisplayName"].as_str()?.to_owned();
            let pasta = PathBuf::from(json["InstallLocation"].as_str()?);
            (e_jogo && pasta.is_dir()).then_some(JogoInstalado { nome, loja: "Epic", pasta })
        })
        .collect()
}

/// Cada subpasta é um jogo (Riot, Xbox, Ubisoft), menos as de `fora`.
fn pastas(raiz: &str, loja: &'static str, fora: &[&str]) -> Vec<JogoInstalado> {
    let Ok(leitura) = fs::read_dir(raiz) else { return Vec::new() };
    leitura
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| {
            let nome = e.file_name().to_string_lossy().into_owned();
            (!fora.contains(&nome.to_lowercase().as_str())).then_some(JogoInstalado { nome, loja, pasta: e.path() })
        })
        .collect()
}

/// Valores de uma chave nos arquivos de texto da Steam: `"chave"  "valor"`.
fn valores<'a>(texto: &'a str, chave: &'a str) -> impl Iterator<Item = String> + 'a {
    texto.lines().filter_map(move |linha| {
        let partes: Vec<&str> = linha.trim().split('"').collect();
        // ["", chave, "\t\t", valor, ""]
        (partes.len() >= 4 && partes[1].eq_ignore_ascii_case(chave)).then(|| partes[3].to_owned())
    })
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn le_os_arquivos_da_steam() {
        let acf = "\"AppState\"\n{\n\t\"appid\"\t\t\"2357570\"\n\t\"name\"\t\t\"Overwatch\"\n\t\"installdir\"\t\t\"Overwatch\"\n}";
        assert_eq!(valores(acf, "name").next().as_deref(), Some("Overwatch"));
        assert_eq!(valores(acf, "installdir").next().as_deref(), Some("Overwatch"));
        let vdf = "\"libraryfolders\"\n{\n\t\"0\"\n\t{\n\t\t\"path\"\t\t\"C:\\\\Program Files (x86)\\\\Steam\"\n\t}\n\t\"1\"\n\t{\n\t\t\"path\"\t\t\"D:\\\\SteamLibrary\"\n\t}\n}";
        let caminhos: Vec<String> = valores(vdf, "path").map(|c| c.replace("\\\\", "\\")).collect();
        assert_eq!(caminhos, [r"C:\Program Files (x86)\Steam", r"D:\SteamLibrary"]);
        assert!(e_ferramenta("Steamworks Common Redistributables"));
        assert!(!e_ferramenta("Overwatch"));
    }

    #[test]
    fn lista_sem_erro() {
        for jogo in instalados() {
            assert!(jogo.pasta.is_dir(), "{jogo:?}");
        }
    }
}
