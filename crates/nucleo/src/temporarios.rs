//! Temporários antigos. A análise só lê; apagar é uma função separada,
//! que manda para a Lixeira e só deve ser chamada depois que o usuário
//! confirmar.
//!
//! Cada item do topo da pasta temporária (arquivo ou pasta inteira) entra
//! só se tudo dentro dele foi criado E alterado há mais de N dias. Um
//! arquivo extraído de um zip mantém a data de alteração antiga, mas a de
//! criação é nova, e por isso a criação também conta.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::Serialize;

use crate::disco::{Item, classificar};

/// Acima disso a Lixeira pode não aceitar o item e o Windows apagaria de
/// vez. Itens maiores ficam de fora e o usuário decide sozinho.
pub const LIMITE_LIXEIRA: u64 = 1 << 30;

#[derive(Debug, Clone, Serialize)]
pub struct ItemTemporario {
    pub caminho: PathBuf,
    pub tamanho: u64,
    pub pasta: bool,
    /// Dias desde a data mais recente (criação ou alteração) de qualquer coisa dentro.
    pub idade_dias: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PastaTemporaria {
    pub caminho: PathBuf,
    /// `false` quando o Windows nega a leitura (C:\Windows\Temp sem administrador).
    pub acessivel: bool,
    pub tamanho: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AnaliseTemporarios {
    pub idade_minima_dias: u64,
    pub pastas: Vec<PastaTemporaria>,
    /// Podem ir para a Lixeira, do maior para o menor.
    pub antigos: Vec<ItemTemporario>,
    /// Antigos, mas grandes demais para garantir a Lixeira.
    pub grandes_demais: Vec<ItemTemporario>,
    /// Recentes ou com algo que não deu para ler: ficam onde estão.
    pub mantidos: u64,
    pub bytes_mantidos: u64,
}

impl AnaliseTemporarios {
    pub fn bytes_antigos(&self) -> u64 {
        self.antigos.iter().map(|i| i.tamanho).sum()
    }
}

/// %TEMP% do usuário e a pasta Temp do Windows.
pub fn pastas_padrao() -> Vec<PathBuf> {
    let mut pastas: Vec<PathBuf> = Vec::new();
    for pasta in [
        Some(std::env::temp_dir()),
        std::env::var_os("SystemRoot").map(|r| PathBuf::from(r).join("Temp")),
    ]
    .into_iter()
    .flatten()
    {
        let chave = |p: &Path| p.to_string_lossy().trim_end_matches('\\').to_lowercase();
        if !pastas.iter().any(|p| chave(p) == chave(&pasta)) {
            pastas.push(pasta);
        }
    }
    pastas
}

pub fn analisar(pastas: &[PathBuf], idade_minima_dias: u64, agora: SystemTime) -> AnaliseTemporarios {
    let limite = agora - Duration::from_secs(idade_minima_dias * 86_400);
    let mut analise = AnaliseTemporarios {
        idade_minima_dias,
        pastas: Vec::new(),
        antigos: Vec::new(),
        grandes_demais: Vec::new(),
        mantidos: 0,
        bytes_mantidos: 0,
    };
    for pasta in pastas {
        let Ok(leitura) = fs::read_dir(pasta) else {
            analise.pastas.push(PastaTemporaria { caminho: pasta.clone(), acessivel: false, tamanho: 0 });
            continue;
        };
        let mut total = 0;
        for entrada in leitura.flatten() {
            let caminho = entrada.path();
            let Some(resumo) = resumir(&entrada) else {
                analise.mantidos += 1;
                continue;
            };
            total += resumo.tamanho;
            let antigo = resumo.mais_recente.is_some_and(|data| data < limite);
            if !antigo {
                analise.mantidos += 1;
                analise.bytes_mantidos += resumo.tamanho;
                continue;
            }
            let idade = resumo
                .mais_recente
                .and_then(|data| agora.duration_since(data).ok())
                .map_or(0, |d| d.as_secs() / 86_400);
            let item = ItemTemporario { caminho, tamanho: resumo.tamanho, pasta: resumo.pasta, idade_dias: idade };
            if item.tamanho > LIMITE_LIXEIRA {
                analise.grandes_demais.push(item);
            } else {
                analise.antigos.push(item);
            }
        }
        analise.pastas.push(PastaTemporaria { caminho: pasta.clone(), acessivel: true, tamanho: total });
    }
    analise.antigos.sort_by_key(|i| std::cmp::Reverse(i.tamanho));
    analise
}

struct Resumo {
    tamanho: u64,
    pasta: bool,
    /// `None` quando algo dentro não pôde ser lido: o item fica.
    mais_recente: Option<SystemTime>,
}

/// Tamanho e data mais recente de um item do topo. `None` para atalhos
/// (symlinks, junções), que nunca são apagados.
fn resumir(entrada: &fs::DirEntry) -> Option<Resumo> {
    match classificar(entrada).ok()? {
        Item::Atalho | Item::Nuvem(_) => None,
        Item::Arquivo(meta) => Some(Resumo {
            tamanho: meta.len(),
            pasta: false,
            mais_recente: data_mais_recente(&meta),
        }),
        Item::Pasta(caminho) => {
            let propria = entrada.metadata().ok().and_then(|m| data_mais_recente(&m));
            let (tamanho, dentro) = percorrer(&caminho);
            let mais_recente = match (propria, dentro) {
                (Some(a), Some(b)) => Some(a.max(b)),
                _ => None,
            };
            Some(Resumo { tamanho, pasta: true, mais_recente })
        }
    }
}

/// Soma e data mais recente de tudo dentro de uma pasta.
/// A data vira `None` se qualquer coisa lá dentro não puder ser lida.
fn percorrer(pasta: &Path) -> (u64, Option<SystemTime>) {
    let Ok(leitura) = fs::read_dir(pasta) else { return (0, None) };
    let mut tamanho = 0;
    let mut mais_recente = Some(SystemTime::UNIX_EPOCH);
    for entrada in leitura {
        let Ok(entrada) = entrada else { return (tamanho, None) };
        let data = match classificar(&entrada) {
            Ok(Item::Arquivo(meta)) => {
                tamanho += meta.len();
                data_mais_recente(&meta)
            }
            Ok(Item::Pasta(sub)) => {
                let propria = entrada.metadata().ok().and_then(|m| data_mais_recente(&m));
                let (t, dentro) = percorrer(&sub);
                tamanho += t;
                propria.zip(dentro).map(|(a, b)| a.max(b))
            }
            Ok(Item::Atalho | Item::Nuvem(_)) => entrada.metadata().ok().and_then(|m| data_mais_recente(&m)),
            Err(_) => None,
        };
        mais_recente = mais_recente.zip(data).map(|(a, b)| a.max(b));
    }
    (tamanho, mais_recente)
}

fn data_mais_recente(meta: &fs::Metadata) -> Option<SystemTime> {
    Some(meta.created().ok()?.max(meta.modified().ok()?))
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ResultadoLimpeza {
    pub enviados: u64,
    pub bytes: u64,
    /// Itens que o Windows recusou (em uso, sem permissão), com o motivo.
    pub falhas: Vec<(PathBuf, String)>,
}

/// Manda cada item para a Lixeira, um por vez, para que um arquivo em uso
/// não impeça os outros. Chamar só depois da confirmação do usuário.
pub fn mandar_para_lixeira(itens: &[ItemTemporario]) -> ResultadoLimpeza {
    let mut resultado = ResultadoLimpeza::default();
    for item in itens {
        if item.tamanho > LIMITE_LIXEIRA || !item.caminho.exists() {
            continue;
        }
        match trash::delete(&item.caminho) {
            Ok(()) => {
                resultado.enviados += 1;
                resultado.bytes += item.tamanho;
            }
            Err(erro) => resultado.falhas.push((item.caminho.clone(), erro.to_string())),
        }
    }
    resultado
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::disco::testes::PastaTeste;

    #[test]
    fn separa_antigos_de_recentes() {
        let p = PastaTeste::nova("temp");
        p.arquivo("solto.tmp", 100);
        p.arquivo("pasta/dentro.tmp", 300);
        p.arquivo("pasta/fundo/mais.tmp", 50);

        // Tudo foi criado agora: com 2 dias de idade mínima, nada sai.
        let agora = SystemTime::now();
        let hoje = analisar(std::slice::from_ref(&p.0), 2, agora);
        assert!(hoje.antigos.is_empty());
        assert_eq!(hoje.mantidos, 2);
        assert_eq!(hoje.bytes_mantidos, 450);

        // Três dias depois, os dois itens do topo saem; a pasta vai inteira.
        let depois = agora + Duration::from_secs(3 * 86_400);
        let a = analisar(std::slice::from_ref(&p.0), 2, depois);
        assert_eq!(a.antigos.len(), 2);
        assert_eq!(a.antigos[0].caminho, p.0.join("pasta"));
        assert!(a.antigos[0].pasta);
        assert_eq!(a.antigos[0].tamanho, 350);
        assert_eq!(a.antigos[0].idade_dias, 3);
        assert_eq!(a.bytes_antigos(), 450);
    }

    /// Manda um arquivo de teste de verdade para a Lixeira. Fica fora da
    /// rodada normal: `cargo test -p nucleo -- --ignored lixeira`.
    #[test]
    #[ignore]
    fn lixeira_recebe_o_antigo_e_pula_o_que_esta_em_uso() {
        use std::fs::{File, FileTimes, OpenOptions};
        use std::os::windows::fs::{FileTimesExt, OpenOptionsExt};

        let p = PastaTeste::nova("lixeira");
        let antigo = p.arquivo("makeway-teste-antigo.tmp", 1000);
        let em_uso = p.arquivo("makeway-teste-em-uso.tmp", 1000);
        let dez_dias = SystemTime::now() - Duration::from_secs(10 * 86_400);
        for caminho in [&antigo, &em_uso] {
            let tempos = FileTimes::new().set_modified(dez_dias).set_created(dez_dias);
            File::options().write(true).open(caminho).unwrap().set_times(tempos).unwrap();
        }
        // Trava exclusiva, como um programa que está com o arquivo aberto.
        let _trava = OpenOptions::new().read(true).share_mode(0).open(&em_uso).unwrap();

        let analise = analisar(std::slice::from_ref(&p.0), 2, SystemTime::now());
        assert_eq!(analise.antigos.len(), 2);
        let resultado = mandar_para_lixeira(&analise.antigos);

        assert_eq!(resultado.enviados, 1);
        assert!(!antigo.exists());
        assert_eq!(resultado.falhas.len(), 1);
        assert_eq!(resultado.falhas[0].0, em_uso);
        assert!(em_uso.exists());
    }

    #[test]
    fn pasta_inacessivel_e_marcada() {
        let a = analisar(&[PathBuf::from(r"C:\makeway-nao-existe-456")], 2, SystemTime::now());
        assert!(!a.pastas[0].acessivel);
        assert!(a.antigos.is_empty());
    }
}
