//! Descobre se um jogo está aberto.

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::protecao::{LAUNCHERS, e_anti_cheat};
use crate::config::ConfigModoJogo;
use crate::ram::Processo;
use crate::sistema;

/// Processos menores que isso dentro das pastas de jogos são ajudantes.
const MINIMO_PELA_PASTA: u64 = 100 << 20;

/// De onde vêm os processos. Nos testes, uma lista falsa entra no lugar
/// do Windows, e a detecção e a restauração rodam em qualquer máquina.
pub trait FonteProcessos {
    fn processos(&mut self) -> io::Result<Vec<Processo>>;
    /// Caminho do .exe; `None` se o Windows negar.
    fn caminho(&mut self, p: &Processo) -> Option<PathBuf>;
}

/// Os processos de verdade. Guarda o caminho de cada processo para não
/// perguntar ao Windows de novo a cada 3 segundos.
#[derive(Default)]
pub struct ProcessosDoWindows {
    caminhos: HashMap<(u32, i64), Option<PathBuf>>,
}

impl FonteProcessos for ProcessosDoWindows {
    fn processos(&mut self) -> io::Result<Vec<Processo>> {
        let lista = sistema::listar_processos()?;
        let vivos: HashSet<(u32, i64)> = lista.iter().map(|p| (p.pid, p.criado_em)).collect();
        self.caminhos.retain(|chave, _| vivos.contains(chave));
        Ok(lista)
    }

    fn caminho(&mut self, p: &Processo) -> Option<PathBuf> {
        self.caminhos
            .entry((p.pid, p.criado_em))
            .or_insert_with(|| sistema::caminho_processo(p.pid).ok())
            .clone()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Jogo {
    pub pid: u32,
    pub criado_em: i64,
    pub nome: String,
    pub caminho: Option<PathBuf>,
    /// "lista de jogos" ou "pasta de jogos".
    pub detectado_por: String,
}

impl Jogo {
    /// O processo do jogo ainda está aberto (mesmo PID e mesma hora de criação).
    pub fn aberto(&self, processos: &[Processo]) -> bool {
        processos.iter().any(|p| p.pid == self.pid && p.criado_em == self.criado_em)
    }
}

pub fn detectar(cfg: &ConfigModoJogo, processos: &[Processo], fonte: &mut impl FonteProcessos) -> Option<Jogo> {
    let na_lista = |nome: &str| cfg.jogos.iter().any(|j| j.eq_ignore_ascii_case(nome));
    let ignorado = |nome: &str| {
        let nome = nome.to_lowercase();
        LAUNCHERS.contains(&nome.as_str())
            || e_anti_cheat(&nome)
            || cfg.ignorar.iter().any(|i| i.eq_ignore_ascii_case(&nome))
    };

    // 1. Um .exe da lista do usuário. Se houver mais de um, o que usa mais RAM.
    if let Some(p) = processos
        .iter()
        .filter(|p| p.sessao != 0 && na_lista(&p.nome))
        .max_by_key(|p| p.ws_privado)
    {
        let caminho = fonte.caminho(p);
        return Some(jogo(p, caminho, "lista de jogos"));
    }

    // 2. O maior processo dentro de uma pasta de jogos.
    if !cfg.detectar_pelas_pastas {
        return None;
    }
    let pastas: Vec<String> = cfg.pastas_de_jogos.iter().map(|p| p.to_lowercase()).collect();
    let mut candidatos: Vec<&Processo> = processos
        .iter()
        .filter(|p| p.sessao != 0 && p.ws_privado >= MINIMO_PELA_PASTA && !ignorado(&p.nome))
        .collect();
    candidatos.sort_by_key(|p| std::cmp::Reverse(p.ws_privado));
    for p in candidatos {
        let Some(caminho) = fonte.caminho(p) else { continue };
        let texto = caminho.to_string_lossy().to_lowercase();
        if pastas.iter().any(|pasta| texto.contains(pasta.as_str())) {
            return Some(jogo(p, Some(caminho), "pasta de jogos"));
        }
    }
    None
}

fn jogo(p: &Processo, caminho: Option<PathBuf>, detectado_por: &str) -> Jogo {
    Jogo {
        pid: p.pid,
        criado_em: p.criado_em,
        nome: p.nome.clone(),
        caminho,
        detectado_por: detectado_por.to_owned(),
    }
}

#[cfg(test)]
pub(crate) mod testes {
    use super::*;
    use crate::modo_jogo::protecao::testes::proc;

    /// Lista falsa de processos com caminho.
    #[derive(Default)]
    pub(crate) struct FonteFalsa {
        pub lista: Vec<(Processo, PathBuf)>,
    }

    impl FonteFalsa {
        pub(crate) fn com(mut self, pid: u32, nome: &str, mb: u64, caminho: &str) -> Self {
            let mut p = proc(pid, 1, nome, i64::from(pid));
            p.ws_privado = mb << 20;
            self.lista.push((p, PathBuf::from(caminho)));
            self
        }
    }

    impl FonteProcessos for FonteFalsa {
        fn processos(&mut self) -> io::Result<Vec<Processo>> {
            Ok(self.lista.iter().map(|(p, _)| p.clone()).collect())
        }
        fn caminho(&mut self, p: &Processo) -> Option<PathBuf> {
            self.lista.iter().find(|(q, _)| q.pid == p.pid).map(|(_, c)| c.clone())
        }
    }

    fn detectar_em(cfg: &ConfigModoJogo, mut fonte: FonteFalsa) -> Option<Jogo> {
        let processos = fonte.processos().unwrap();
        detectar(cfg, &processos, &mut fonte)
    }

    #[test]
    fn acha_pela_pasta_da_steam_e_ignora_ajudantes() {
        let fonte = FonteFalsa::default()
            .com(1, "steam.exe", 300, r"C:\Program Files (x86)\Steam\steam.exe")
            .com(2, "UnityCrashHandler64.exe", 150, r"C:\Program Files (x86)\Steam\steamapps\common\Jogo\UnityCrashHandler64.exe")
            .com(3, "Helper.exe", 20, r"C:\Program Files (x86)\Steam\steamapps\common\Jogo\Helper.exe")
            .com(4, "Jogo.exe", 2500, r"C:\Program Files (x86)\Steam\steamapps\common\Jogo\Jogo.exe")
            .com(5, "chrome.exe", 4000, r"C:\Program Files\Google\Chrome\chrome.exe");
        let jogo = detectar_em(&ConfigModoJogo::default(), fonte).unwrap();
        assert_eq!(jogo.nome, "Jogo.exe");
        assert_eq!(jogo.detectado_por, "pasta de jogos");
    }

    #[test]
    fn lista_do_usuario_vale_mesmo_fora_das_pastas() {
        let fonte = FonteFalsa::default()
            .com(1, "jogo-pequeno.exe", 30, r"D:\Jogos\jogo-pequeno.exe")
            .com(2, "Outro.exe", 3000, r"C:\Riot Games\Outro\Outro.exe");
        let cfg = ConfigModoJogo { jogos: vec!["JOGO-PEQUENO.exe".into()], ..Default::default() };
        let jogo = detectar_em(&cfg, fonte).unwrap();
        assert_eq!(jogo.pid, 1);
        assert_eq!(jogo.detectado_por, "lista de jogos");
    }

    #[test]
    fn launcher_da_riot_nao_e_jogo() {
        let fonte = FonteFalsa::default().com(1, "RiotClientServices.exe", 400, r"C:\Riot Games\Riot Client\RiotClientServices.exe");
        assert!(detectar_em(&ConfigModoJogo::default(), fonte).is_none());
    }

    #[test]
    fn sem_detectar_pelas_pastas() {
        let fonte = FonteFalsa::default().com(1, "Jogo.exe", 2000, r"C:\XboxGames\Jogo\Jogo.exe");
        let cfg = ConfigModoJogo { detectar_pelas_pastas: false, ..Default::default() };
        assert!(detectar_em(&cfg, fonte).is_none());
    }
}
