//! Arquivos duplicados (só leitura), em três filtros do mais barato
//! para o mais caro:
//!
//! 1. mesmo tamanho (não abre nenhum arquivo);
//! 2. mesmo hash dos primeiros 16 KB;
//! 3. mesmo hash do conteúdo inteiro (blake3).
//!
//! Hard links (o mesmo arquivo com dois nomes) não contam como duplicado,
//! porque apagar um deles não libera espaço nenhum.

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use rayon::prelude::*;
use serde::Serialize;

use crate::disco::{self, Contadores, Pulados};
use crate::sistema;

const TAMANHO_COMECO: u64 = 16 * 1024;

#[derive(Debug, Clone, Serialize)]
pub struct GrupoDuplicado {
    /// Tamanho de cada cópia.
    pub tamanho: u64,
    pub arquivos: Vec<PathBuf>,
    /// Espaço que voltaria se ficasse só uma cópia.
    pub desperdicio: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResultadoDuplicados {
    /// Do grupo que mais desperdiça para o que menos desperdiça.
    pub grupos: Vec<GrupoDuplicado>,
    pub desperdicio_total: u64,
    /// Quantos arquivos passaram por cada filtro.
    pub candidatos_tamanho: u64,
    pub candidatos_comeco: u64,
    pub pulados: Pulados,
    /// Arquivos que não deu para abrir (em uso com trava exclusiva, sem permissão).
    pub nao_lidos: u64,
    pub segundos: f64,
    pub cancelada: bool,
}

pub fn procurar(raiz: &Path, tamanho_minimo: u64, c: &Contadores) -> ResultadoDuplicados {
    let inicio = Instant::now();
    let arquivos = disco::listar_arquivos(raiz, tamanho_minimo.max(1), c);

    let mut por_tamanho: HashMap<u64, Vec<PathBuf>> = HashMap::new();
    for (caminho, tamanho) in arquivos {
        por_tamanho.entry(tamanho).or_default().push(caminho);
    }
    let candidatos: Vec<(u64, Vec<PathBuf>)> =
        por_tamanho.into_iter().filter(|(_, v)| v.len() > 1).collect();

    let estatisticas = Estatisticas::default();
    estatisticas.tamanho.store(candidatos.iter().map(|(_, v)| v.len() as u64).sum(), Ordering::Relaxed);

    let mut grupos: Vec<GrupoDuplicado> = candidatos
        .into_par_iter()
        .flat_map_iter(|(tamanho, caminhos)| comparar_grupo(tamanho, caminhos, &estatisticas, c))
        .collect();
    grupos.sort_by(|a, b| b.desperdicio.cmp(&a.desperdicio).then_with(|| a.arquivos.cmp(&b.arquivos)));

    ResultadoDuplicados {
        desperdicio_total: grupos.iter().map(|g| g.desperdicio).sum(),
        grupos,
        candidatos_tamanho: estatisticas.tamanho.load(Ordering::Relaxed),
        candidatos_comeco: estatisticas.comeco.load(Ordering::Relaxed),
        pulados: c.pulados(),
        nao_lidos: estatisticas.nao_lidos.load(Ordering::Relaxed),
        segundos: inicio.elapsed().as_secs_f64(),
        cancelada: c.cancelado(),
    }
}

#[derive(Default)]
struct Estatisticas {
    tamanho: AtomicU64,
    comeco: AtomicU64,
    nao_lidos: AtomicU64,
}

/// Filtros 2 e 3 para arquivos que já têm o mesmo tamanho.
fn comparar_grupo(tamanho: u64, caminhos: Vec<PathBuf>, e: &Estatisticas, c: &Contadores) -> Vec<GrupoDuplicado> {
    if c.cancelado() {
        return Vec::new();
    }
    let mut identidades = HashSet::new();
    let mut por_comeco: HashMap<blake3::Hash, Vec<PathBuf>> = HashMap::new();
    for caminho in caminhos {
        let lido = File::open(&caminho).and_then(|mut arquivo| {
            let identidade = sistema::identidade_arquivo(&arquivo)?;
            Ok((identidade, hash_comeco(&mut arquivo)?))
        });
        match lido {
            // Mesma identidade = hard link de um arquivo já visto.
            Ok((identidade, _)) if !identidades.insert(identidade) => {}
            Ok((_, hash)) => por_comeco.entry(hash).or_default().push(caminho),
            Err(_) => {
                e.nao_lidos.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    let mut grupos = Vec::new();
    for (_, mesmos) in por_comeco.into_iter().filter(|(_, v)| v.len() > 1) {
        e.comeco.fetch_add(mesmos.len() as u64, Ordering::Relaxed);
        if tamanho <= TAMANHO_COMECO {
            // O começo já era o arquivo inteiro.
            grupos.push(grupo(tamanho, mesmos));
            continue;
        }
        let mut por_conteudo: HashMap<blake3::Hash, Vec<PathBuf>> = HashMap::new();
        for caminho in mesmos {
            match hash_inteiro(&caminho) {
                Ok(hash) => por_conteudo.entry(hash).or_default().push(caminho),
                Err(_) => {
                    e.nao_lidos.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
        grupos.extend(por_conteudo.into_values().filter(|v| v.len() > 1).map(|v| grupo(tamanho, v)));
    }
    grupos
}

fn grupo(tamanho: u64, mut arquivos: Vec<PathBuf>) -> GrupoDuplicado {
    arquivos.sort();
    GrupoDuplicado { tamanho, desperdicio: tamanho * (arquivos.len() as u64 - 1), arquivos }
}

fn hash_comeco(arquivo: &mut File) -> io::Result<blake3::Hash> {
    let mut comeco = Vec::with_capacity(TAMANHO_COMECO as usize);
    arquivo.take(TAMANHO_COMECO).read_to_end(&mut comeco)?;
    Ok(blake3::hash(&comeco))
}

fn hash_inteiro(caminho: &Path) -> io::Result<blake3::Hash> {
    let mut hasher = blake3::Hasher::new();
    hasher.update_reader(File::open(caminho)?)?;
    Ok(hasher.finalize())
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::disco::testes::PastaTeste;

    #[test]
    fn acha_duplicados_nos_tres_filtros() {
        let p = PastaTeste::nova("duplicados");
        let grande: Vec<u8> = (0..100_000u32).map(|i| (i % 251) as u8).collect();
        let mut fim_diferente = grande.clone();
        *fim_diferente.last_mut().unwrap() ^= 1;

        p.arquivo_com("a/foto.jpg", &grande);
        p.arquivo_com("b/foto (1).jpg", &grande);
        p.arquivo_com("c/foto editada.jpg", &fim_diferente); // passa no filtro 2, cai no 3
        p.arquivo_com("x.txt", b"igual");
        p.arquivo_com("y/x.txt", b"igual");
        p.arquivo_com("z.txt", b"outra"); // mesmo tamanho, conteúdo diferente
        p.arquivo_com("unico.bin", b"so tem um desse tamanho");

        let c = Contadores::default();
        let r = procurar(&p.0, 1, &c);

        assert_eq!(r.grupos.len(), 2);
        assert_eq!(r.grupos[0].tamanho, 100_000);
        assert_eq!(r.grupos[0].arquivos, [p.0.join(r"a\foto.jpg"), p.0.join(r"b\foto (1).jpg")]);
        assert_eq!(r.grupos[1].arquivos.len(), 2);
        assert_eq!(r.desperdicio_total, 100_000 + 5);
        assert_eq!(r.candidatos_tamanho, 6);
        assert_eq!(r.candidatos_comeco, 5);
    }

    #[test]
    fn hard_link_nao_e_duplicado() {
        let p = PastaTeste::nova("hardlink");
        let original = p.arquivo("original.bin", 50_000);
        std::fs::hard_link(&original, p.0.join("link.bin")).unwrap();
        let c = Contadores::default();
        assert!(procurar(&p.0, 1, &c).grupos.is_empty());
    }

    #[test]
    fn respeita_tamanho_minimo() {
        let p = PastaTeste::nova("minimo");
        p.arquivo("a.bin", 100);
        p.arquivo("b.bin", 100);
        let c = Contadores::default();
        assert!(procurar(&p.0, 1024, &c).grupos.is_empty());
    }
}
