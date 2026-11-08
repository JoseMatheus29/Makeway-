//! Análise de disco (só leitura): maiores pastas e arquivos.
//!
//! A varredura usa `std::fs::read_dir` em paralelo (rayon). No Windows o
//! tamanho e os atributos de cada arquivo já vêm na listagem da pasta
//! (`FindNextFile`), sem nenhuma chamada extra por arquivo.

use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::fs::{self, DirEntry, Metadata};
use std::io;
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

use rayon::prelude::*;
use serde::Serialize;

const FILE_ATTRIBUTE_OFFLINE: u32 = 0x1000;
const FILE_ATTRIBUTE_RECALL_ON_OPEN: u32 = 0x4_0000;
const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS: u32 = 0x40_0000;

/// Arquivo que só existe na nuvem (OneDrive e parecidos). Abrir para ler
/// faria o Windows baixar o arquivo, então ele nunca é lido nem contado.
pub(crate) fn so_na_nuvem(atributos: u32) -> bool {
    atributos
        & (FILE_ATTRIBUTE_OFFLINE | FILE_ATTRIBUTE_RECALL_ON_OPEN | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS)
        != 0
}

pub(crate) enum Item {
    Pasta(PathBuf),
    Arquivo(Metadata),
    Nuvem(u64),
    /// Symlink ou junção: pulado para não contar a mesma coisa duas vezes
    /// nem entrar em loop.
    Atalho,
}

pub(crate) fn classificar(entrada: &DirEntry) -> io::Result<Item> {
    let tipo = entrada.file_type()?;
    if tipo.is_symlink() {
        return Ok(Item::Atalho);
    }
    if tipo.is_dir() {
        return Ok(Item::Pasta(entrada.path()));
    }
    let meta = entrada.metadata()?;
    if so_na_nuvem(meta.file_attributes()) {
        return Ok(Item::Nuvem(meta.len()));
    }
    Ok(Item::Arquivo(meta))
}

/// O que ficou de fora de uma varredura.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Pulados {
    /// Pastas e arquivos sem permissão de leitura.
    pub sem_acesso: u64,
    /// Alguns exemplos, para o usuário saber onde foi.
    pub exemplos_sem_acesso: Vec<PathBuf>,
    pub nuvem_arquivos: u64,
    pub nuvem_bytes: u64,
}

/// Contadores compartilhados entre as threads da varredura. Também servem
/// para mostrar o progresso enquanto ela roda.
#[derive(Default)]
pub struct Contadores {
    arquivos: AtomicU64,
    sem_acesso: AtomicU64,
    exemplos: Mutex<Vec<PathBuf>>,
    nuvem_arquivos: AtomicU64,
    nuvem_bytes: AtomicU64,
    cancelado: AtomicBool,
}

impl Contadores {
    /// Pede para a varredura parar. O que já foi lido fica no resultado.
    pub fn cancelar(&self) {
        self.cancelado.store(true, Ordering::Relaxed);
    }

    pub fn cancelado(&self) -> bool {
        self.cancelado.load(Ordering::Relaxed)
    }

    pub fn arquivos_vistos(&self) -> u64 {
        self.arquivos.load(Ordering::Relaxed)
    }

    pub(crate) fn arquivo(&self) {
        self.arquivos.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn sem_acesso(&self, caminho: &Path) {
        self.sem_acesso.fetch_add(1, Ordering::Relaxed);
        let mut exemplos = self.exemplos.lock().unwrap_or_else(|e| e.into_inner());
        if exemplos.len() < 5 {
            exemplos.push(caminho.to_owned());
        }
    }

    pub(crate) fn nuvem(&self, bytes: u64) {
        self.nuvem_arquivos.fetch_add(1, Ordering::Relaxed);
        self.nuvem_bytes.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn pulados(&self) -> Pulados {
        Pulados {
            sem_acesso: self.sem_acesso.load(Ordering::Relaxed),
            exemplos_sem_acesso: self.exemplos.lock().unwrap_or_else(|e| e.into_inner()).clone(),
            nuvem_arquivos: self.nuvem_arquivos.load(Ordering::Relaxed),
            nuvem_bytes: self.nuvem_bytes.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Pasta {
    pub caminho: PathBuf,
    /// Soma de todos os arquivos dentro, em qualquer nível.
    pub tamanho: u64,
    pub arquivos: u64,
    pub subpastas: u64,
    /// Só até a profundidade pedida, da maior para a menor.
    pub filhos: Vec<Pasta>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Arquivo {
    pub caminho: PathBuf,
    pub tamanho: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Varredura {
    pub raiz: Pasta,
    /// Do maior para o menor.
    pub maiores_arquivos: Vec<Arquivo>,
    pub pulados: Pulados,
    pub segundos: f64,
    /// Parou no meio a pedido do usuário: os números estão incompletos.
    pub cancelada: bool,
}

/// Varre `raiz` inteira. `profundidade` = quantos níveis de subpastas
/// guardar no resultado (o tamanho conta todos os níveis de qualquer jeito).
pub fn varrer(raiz: &Path, profundidade: usize, top_arquivos: usize, c: &Contadores) -> Varredura {
    let inicio = Instant::now();
    let maiores = MaioresArquivos::new(top_arquivos);
    let raiz = varrer_pasta(raiz.to_owned(), 0, profundidade, &maiores, c);
    Varredura {
        raiz,
        maiores_arquivos: maiores.em_ordem(),
        pulados: c.pulados(),
        segundos: inicio.elapsed().as_secs_f64(),
        cancelada: c.cancelado(),
    }
}

fn varrer_pasta(
    caminho: PathBuf,
    nivel: usize,
    profundidade: usize,
    maiores: &MaioresArquivos,
    c: &Contadores,
) -> Pasta {
    let mut pasta = Pasta { caminho, tamanho: 0, arquivos: 0, subpastas: 0, filhos: Vec::new() };
    if c.cancelado() {
        return pasta;
    }
    let leitura = match fs::read_dir(&pasta.caminho) {
        Ok(leitura) => leitura,
        Err(_) => {
            c.sem_acesso(&pasta.caminho);
            return pasta;
        }
    };
    let mut subpastas = Vec::new();
    for entrada in leitura {
        let Ok(entrada) = entrada else { continue };
        match classificar(&entrada) {
            Ok(Item::Pasta(p)) => subpastas.push(p),
            Ok(Item::Arquivo(meta)) => {
                c.arquivo();
                pasta.tamanho += meta.len();
                pasta.arquivos += 1;
                maiores.considerar(meta.len(), || entrada.path());
            }
            Ok(Item::Nuvem(bytes)) => c.nuvem(bytes),
            Ok(Item::Atalho) => {}
            Err(_) => c.sem_acesso(&entrada.path()),
        }
    }
    let filhos: Vec<Pasta> = subpastas
        .into_par_iter()
        .map(|p| varrer_pasta(p, nivel + 1, profundidade, maiores, c))
        .collect();
    for filho in filhos {
        pasta.tamanho += filho.tamanho;
        pasta.arquivos += filho.arquivos;
        pasta.subpastas += 1 + filho.subpastas;
        if nivel < profundidade {
            pasta.filhos.push(filho);
        }
    }
    pasta.filhos.sort_by_key(|f| std::cmp::Reverse(f.tamanho));
    pasta
}

/// Lista de todos os arquivos locais com pelo menos `minimo` bytes.
pub(crate) fn listar_arquivos(raiz: &Path, minimo: u64, c: &Contadores) -> Vec<(PathBuf, u64)> {
    if c.cancelado() {
        return Vec::new();
    }
    let mut lista = Vec::new();
    let leitura = match fs::read_dir(raiz) {
        Ok(leitura) => leitura,
        Err(_) => {
            c.sem_acesso(raiz);
            return lista;
        }
    };
    let mut subpastas = Vec::new();
    for entrada in leitura {
        let Ok(entrada) = entrada else { continue };
        match classificar(&entrada) {
            Ok(Item::Pasta(p)) => subpastas.push(p),
            Ok(Item::Arquivo(meta)) => {
                c.arquivo();
                if meta.len() >= minimo {
                    lista.push((entrada.path(), meta.len()));
                }
            }
            Ok(Item::Nuvem(bytes)) => c.nuvem(bytes),
            Ok(Item::Atalho) => {}
            Err(_) => c.sem_acesso(&entrada.path()),
        }
    }
    let filhos: Vec<Vec<(PathBuf, u64)>> =
        subpastas.par_iter().map(|p| listar_arquivos(p, minimo, c)).collect();
    lista.extend(filhos.into_iter().flatten());
    lista
}

/// Os N maiores arquivos vistos até agora. A trava só é usada quando o
/// arquivo é maior que o menor da lista, o que é raro depois do começo.
struct MaioresArquivos {
    limite: usize,
    menor: AtomicU64,
    heap: Mutex<BinaryHeap<Reverse<(u64, PathBuf)>>>,
}

impl MaioresArquivos {
    fn new(limite: usize) -> Self {
        Self { limite, menor: AtomicU64::new(0), heap: Mutex::new(BinaryHeap::new()) }
    }

    fn considerar(&self, tamanho: u64, caminho: impl FnOnce() -> PathBuf) {
        if self.limite == 0 || tamanho <= self.menor.load(Ordering::Relaxed) {
            return;
        }
        let mut heap = self.heap.lock().unwrap_or_else(|e| e.into_inner());
        heap.push(Reverse((tamanho, caminho())));
        if heap.len() > self.limite {
            heap.pop();
        }
        if heap.len() == self.limite
            && let Some(Reverse((menor, _))) = heap.peek()
        {
            self.menor.store(*menor, Ordering::Relaxed);
        }
    }

    fn em_ordem(self) -> Vec<Arquivo> {
        let heap = self.heap.into_inner().unwrap_or_else(|e| e.into_inner());
        heap.into_sorted_vec()
            .into_iter()
            .map(|Reverse((tamanho, caminho))| Arquivo { caminho, tamanho })
            .collect()
    }
}

#[cfg(test)]
pub(crate) mod testes {
    use super::*;

    /// Pasta temporária apagada no fim do teste.
    pub(crate) struct PastaTeste(pub PathBuf);

    impl PastaTeste {
        pub(crate) fn nova(nome: &str) -> Self {
            let caminho = std::env::temp_dir()
                .join(format!("makeway-teste-{nome}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&caminho);
            fs::create_dir_all(&caminho).unwrap();
            Self(caminho)
        }

        pub(crate) fn arquivo(&self, relativo: &str, bytes: usize) -> PathBuf {
            self.arquivo_com(relativo, &vec![b'x'; bytes])
        }

        pub(crate) fn arquivo_com(&self, relativo: &str, conteudo: &[u8]) -> PathBuf {
            let caminho = self.0.join(relativo);
            fs::create_dir_all(caminho.parent().unwrap()).unwrap();
            fs::write(&caminho, conteudo).unwrap();
            caminho
        }
    }

    impl Drop for PastaTeste {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn soma_tamanhos_e_guarda_os_maiores() {
        let p = PastaTeste::nova("disco");
        p.arquivo("a.bin", 1000);
        p.arquivo("sub/b.bin", 5000);
        p.arquivo("sub/fundo/c.bin", 3000);
        p.arquivo("outra/d.bin", 200);

        let c = Contadores::default();
        let v = varrer(&p.0, 1, 2, &c);

        assert_eq!(v.raiz.tamanho, 9200);
        assert_eq!(v.raiz.arquivos, 4);
        assert_eq!(v.raiz.subpastas, 3);
        // Nível 1 guardado, nível 2 só somado.
        assert_eq!(v.raiz.filhos.len(), 2);
        assert_eq!(v.raiz.filhos[0].caminho, p.0.join("sub"));
        assert_eq!(v.raiz.filhos[0].tamanho, 8000);
        assert!(v.raiz.filhos[0].filhos.is_empty());
        let maiores: Vec<u64> = v.maiores_arquivos.iter().map(|a| a.tamanho).collect();
        assert_eq!(maiores, [5000, 3000]);
        assert_eq!(c.arquivos_vistos(), 4);
    }

    #[test]
    fn pasta_inexistente_conta_como_sem_acesso() {
        let c = Contadores::default();
        let v = varrer(Path::new(r"C:\makeway-nao-existe-123"), 1, 5, &c);
        assert_eq!(v.raiz.tamanho, 0);
        assert_eq!(v.pulados.sem_acesso, 1);
    }

    #[test]
    fn reconhece_arquivo_so_na_nuvem() {
        assert!(so_na_nuvem(0x40_0000 | 0x400 | 0x20));
        assert!(!so_na_nuvem(0x20));
    }

    #[test]
    fn lista_so_arquivos_acima_do_minimo() {
        let p = PastaTeste::nova("listar");
        p.arquivo("pequeno.txt", 10);
        p.arquivo("x/grande.bin", 4096);
        let c = Contadores::default();
        let lista = listar_arquivos(&p.0, 1024, &c);
        assert_eq!(lista, vec![(p.0.join(r"x\grande.bin"), 4096)]);
    }
}
