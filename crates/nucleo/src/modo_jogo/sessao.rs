//! Restauração primeiro.
//!
//! A [`Sessao`] é a guarda RAII do modo jogo: cada alteração é anotada no
//! `estado.json` ANTES de ser feita, e quando a sessão sai de escopo
//! (fim normal, erro ou panic) tudo é desfeito em ordem inversa. Se o
//! processo for morto à força, o `estado.json` fica no disco e a próxima
//! execução chama [`restaurar_pendente`].

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::ram::Processo;
use crate::sistema;

pub const ALTO_DESEMPENHO: u128 = 0x8c5e7fda_e8bf_4a96_9a85_a6e23a8c635c;

/// Nome conhecido de um plano de energia, ou o GUID.
pub fn nome_do_plano(plano: u128) -> String {
    match plano {
        ALTO_DESEMPENHO => "Alto desempenho".into(),
        0x381b4222_f694_41f0_9685_ff5bb260df2e => "Equilibrado".into(),
        0xa1841308_3541_4fab_bc81_f71556f20b4a => "Economia de energia".into(),
        0xe9a42b02_d5df_448d_aa00_03f14749eb61 => "Desempenho máximo".into(),
        outro => format!("{outro:032x}"),
    }
}

/// O que o modo jogo fez e como desfazer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum Alteracao {
    /// GUIDs em hexadecimal.
    PlanoEnergia { original: String, novo: String },
    Prioridade { pid: u32, criado_em: i64, nome: String, original: u32 },
    Pausa { pid: u32, criado_em: i64, nome: String },
    /// Não se desfaz: fica anotado para o relatório.
    Fechamento { pid: u32, nome: String },
    /// Prioridade de memória baixada em vários apps de uma vez: uma
    /// anotação só, para não regravar o arquivo centenas de vezes.
    MemoriaBaixa { processos: Vec<MemoriaOriginal> },
}

/// Um processo com a prioridade de memória baixada e a que ele tinha.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoriaOriginal {
    pub pid: u32,
    pub criado_em: i64,
    pub nome: String,
    pub original: u32,
}

/// Prioridade de memória usada nos apps de fundo durante o jogo
/// (`MEMORY_PRIORITY_LOW`). A "muito baixa" faria o Windows tirar memória
/// deles cedo demais, até do Discord no meio de uma chamada.
pub const MEMORIA_BAIXA: u32 = 2;

impl Alteracao {
    pub fn descricao(&self) -> String {
        match self {
            Self::PlanoEnergia { original, novo } => format!(
                "plano de energia: {} → {}",
                nome_do_plano(hex(original)),
                nome_do_plano(hex(novo))
            ),
            Self::Prioridade { nome, .. } => format!("prioridade alta para {nome}"),
            Self::Pausa { nome, pid, .. } => format!("pausou {nome} (PID {pid})"),
            Self::Fechamento { nome, pid } => format!("pediu para fechar {nome} (PID {pid})"),
            Self::MemoriaBaixa { processos } => {
                format!("prioridade de memória baixa para {} processos", processos.len())
            }
        }
    }
}

fn hex(texto: &str) -> u128 {
    u128::from_str_radix(texto, 16).unwrap_or_default()
}

/// O que a sessão faz no Windows. Nos testes entra uma versão falsa.
pub trait Acoes {
    /// O processo com este PID e esta hora de criação ainda existe.
    fn vivo(&mut self, pid: u32, criado_em: i64) -> bool;
    /// O mesmo para vários de uma vez (no Windows, uma foto só dos processos).
    fn vivos(&mut self, lista: &[(u32, i64)]) -> Vec<bool> {
        lista.iter().map(|&(pid, criado_em)| self.vivo(pid, criado_em)).collect()
    }
    fn prioridade_de_memoria(&mut self, pid: u32) -> io::Result<u32>;
    fn definir_prioridade_de_memoria(&mut self, pid: u32, nivel: u32) -> io::Result<()>;
    fn pausar(&mut self, pid: u32) -> io::Result<()>;
    fn retomar(&mut self, pid: u32) -> io::Result<()>;
    fn esvaziar_memoria(&mut self, pid: u32) -> io::Result<()>;
    fn prioridade(&mut self, pid: u32) -> io::Result<u32>;
    fn definir_prioridade(&mut self, pid: u32, classe: u32) -> io::Result<()>;
    fn plano_energia(&mut self) -> io::Result<u128>;
    fn definir_plano_energia(&mut self, plano: u128) -> io::Result<()>;
    fn fechar_janelas(&mut self, pid: u32) -> usize;
}

/// As ações de verdade.
pub struct AcoesWindows;

impl Acoes for AcoesWindows {
    fn vivo(&mut self, pid: u32, criado_em: i64) -> bool {
        sistema::listar_processos()
            .map(|lista| lista.iter().any(|p| p.pid == pid && p.criado_em == criado_em))
            .unwrap_or(false)
    }
    fn vivos(&mut self, lista: &[(u32, i64)]) -> Vec<bool> {
        let agora: std::collections::HashSet<(u32, i64)> = sistema::listar_processos()
            .map(|processos| processos.iter().map(|p| (p.pid, p.criado_em)).collect())
            .unwrap_or_default();
        lista.iter().map(|chave| agora.contains(chave)).collect()
    }
    fn prioridade_de_memoria(&mut self, pid: u32) -> io::Result<u32> {
        sistema::prioridade_de_memoria(pid)
    }
    fn definir_prioridade_de_memoria(&mut self, pid: u32, nivel: u32) -> io::Result<()> {
        sistema::definir_prioridade_de_memoria(pid, nivel)
    }
    fn pausar(&mut self, pid: u32) -> io::Result<()> {
        sistema::pausar_processo(pid)
    }
    fn retomar(&mut self, pid: u32) -> io::Result<()> {
        sistema::retomar_processo(pid)
    }
    fn esvaziar_memoria(&mut self, pid: u32) -> io::Result<()> {
        sistema::esvaziar_memoria(pid)
    }
    fn prioridade(&mut self, pid: u32) -> io::Result<u32> {
        sistema::prioridade(pid)
    }
    fn definir_prioridade(&mut self, pid: u32, classe: u32) -> io::Result<()> {
        sistema::definir_prioridade(pid, classe)
    }
    fn plano_energia(&mut self) -> io::Result<u128> {
        sistema::plano_energia_ativo()
    }
    fn definir_plano_energia(&mut self, plano: u128) -> io::Result<()> {
        sistema::definir_plano_energia(plano)
    }
    fn fechar_janelas(&mut self, pid: u32) -> usize {
        sistema::fechar_janelas(pid)
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Estado {
    jogo: String,
    alteracoes: Vec<Alteracao>,
}

pub struct Sessao<A: Acoes> {
    arquivo: PathBuf,
    estado: Estado,
    acoes: A,
    encerrada: bool,
}

impl<A: Acoes> Sessao<A> {
    /// `arquivo` = caminho do `estado.json`. Ele só é criado na primeira alteração.
    pub fn nova(arquivo: PathBuf, jogo: &str, acoes: A) -> Self {
        let estado = Estado { jogo: jogo.to_owned(), alteracoes: Vec::new() };
        Self { arquivo, estado, acoes, encerrada: false }
    }

    pub fn alteracoes(&self) -> &[Alteracao] {
        &self.estado.alteracoes
    }

    /// Anota, grava e só então faz. Se não der para gravar, não faz.
    /// Se fazer falhar, apaga a anotação.
    fn anotar_e_fazer(
        &mut self,
        alteracao: Alteracao,
        fazer: impl FnOnce(&mut A) -> io::Result<()>,
    ) -> io::Result<()> {
        self.estado.alteracoes.push(alteracao);
        if let Err(erro) = gravar(&self.arquivo, &self.estado) {
            self.estado.alteracoes.pop();
            return Err(erro);
        }
        if let Err(erro) = fazer(&mut self.acoes) {
            self.estado.alteracoes.pop();
            let _ = gravar(&self.arquivo, &self.estado);
            return Err(erro);
        }
        Ok(())
    }

    /// Troca o plano de energia. `Ok(false)` se ele já era o ativo.
    pub fn trocar_plano(&mut self, novo: u128) -> io::Result<bool> {
        let original = self.acoes.plano_energia()?;
        if original == novo {
            return Ok(false);
        }
        let alteracao = Alteracao::PlanoEnergia {
            original: format!("{original:032x}"),
            novo: format!("{novo:032x}"),
        };
        self.anotar_e_fazer(alteracao, |a| a.definir_plano_energia(novo))?;
        Ok(true)
    }

    /// Muda a prioridade. `Ok(false)` se ela já era essa.
    pub fn definir_prioridade(&mut self, p: &Processo, classe: u32) -> io::Result<bool> {
        let original = self.acoes.prioridade(p.pid)?;
        if original == classe {
            return Ok(false);
        }
        let alteracao = Alteracao::Prioridade { pid: p.pid, criado_em: p.criado_em, nome: p.nome.clone(), original };
        self.anotar_e_fazer(alteracao, |a| a.definir_prioridade(p.pid, classe))?;
        Ok(true)
    }

    /// Pausa o processo e devolve a RAM dele. Falhar ao devolver a RAM não
    /// desfaz a pausa: vira só um aviso (`Ok(Some(erro))`).
    pub fn pausar(&mut self, p: &Processo) -> io::Result<Option<io::Error>> {
        let alteracao = Alteracao::Pausa { pid: p.pid, criado_em: p.criado_em, nome: p.nome.clone() };
        self.anotar_e_fazer(alteracao, |a| a.pausar(p.pid))?;
        Ok(self.acoes.esvaziar_memoria(p.pid).err())
    }

    /// Baixa a prioridade de memória dos processos para `nivel`. Anota todos
    /// de uma vez, antes de mexer; os que o Windows não deixar mudar saem da
    /// anotação. Quem já tinha a prioridade igual ou menor fica de fora.
    /// Devolve quantos mudaram.
    pub fn baixar_memoria(&mut self, processos: &[&Processo], nivel: u32) -> io::Result<usize> {
        let mut lista: Vec<MemoriaOriginal> = processos
            .iter()
            .filter_map(|p| {
                let original = self.acoes.prioridade_de_memoria(p.pid).ok()?;
                (original > nivel).then(|| MemoriaOriginal {
                    pid: p.pid,
                    criado_em: p.criado_em,
                    nome: p.nome.clone(),
                    original,
                })
            })
            .collect();
        if lista.is_empty() {
            return Ok(0);
        }
        self.estado.alteracoes.push(Alteracao::MemoriaBaixa { processos: lista.clone() });
        if let Err(erro) = gravar(&self.arquivo, &self.estado) {
            self.estado.alteracoes.pop();
            return Err(erro);
        }
        lista.retain(|m| self.acoes.definir_prioridade_de_memoria(m.pid, nivel).is_ok());
        self.estado.alteracoes.pop();
        let mudaram = lista.len();
        if mudaram > 0 {
            self.estado.alteracoes.push(Alteracao::MemoriaBaixa { processos: lista });
        }
        // Se esta gravação falhar, fica no disco a lista maior: devolver a
        // prioridade original a quem não mudou não faz mal nenhum.
        let _ = gravar(&self.arquivo, &self.estado);
        Ok(mudaram)
    }

    /// Pede para as janelas fecharem. Devolve quantas janelas receberam o pedido.
    pub fn fechar(&mut self, p: &Processo) -> io::Result<usize> {
        let mut janelas = 0;
        let alteracao = Alteracao::Fechamento { pid: p.pid, nome: p.nome.clone() };
        self.anotar_e_fazer(alteracao, |a| {
            janelas = a.fechar_janelas(p.pid);
            if janelas == 0 { Err(io::Error::other("nenhuma janela aberta")) } else { Ok(()) }
        })?;
        Ok(janelas)
    }

    /// Desfaz tudo agora e devolve o que não deu para desfazer.
    pub fn encerrar(mut self) -> Vec<String> {
        self.desfazer_tudo()
    }

    fn desfazer_tudo(&mut self) -> Vec<String> {
        self.encerrada = true;
        let problemas = desfazer_lista(&mut self.acoes, &mut self.estado.alteracoes);
        finalizar_arquivo(&self.arquivo, &self.estado);
        problemas
    }
}

impl<A: Acoes> Drop for Sessao<A> {
    fn drop(&mut self) {
        if !self.encerrada {
            let _ = self.desfazer_tudo();
        }
    }
}

/// Desfaz da última para a primeira. O que falhar continua na lista, para
/// a próxima execução tentar de novo.
fn desfazer_lista<A: Acoes>(acoes: &mut A, alteracoes: &mut Vec<Alteracao>) -> Vec<String> {
    let mut problemas = Vec::new();
    let mut restantes = Vec::new();
    while let Some(alteracao) = alteracoes.pop() {
        if let Alteracao::MemoriaBaixa { processos } = alteracao {
            // Um por um: só os que falharem continuam anotados.
            let (sobra, erro) = devolver_memoria(acoes, processos);
            if let Some(erro) = erro {
                let alteracao = Alteracao::MemoriaBaixa { processos: sobra };
                problemas.push(format!("não deu para desfazer \"{}\": {erro}", alteracao.descricao()));
                restantes.push(alteracao);
            }
            continue;
        }
        if let Err(erro) = desfazer(acoes, &alteracao) {
            problemas.push(format!("não deu para desfazer \"{}\": {erro}", alteracao.descricao()));
            restantes.push(alteracao);
        }
    }
    restantes.reverse();
    *alteracoes = restantes;
    problemas
}

/// Devolve a prioridade de memória original aos que ainda estão abertos.
/// Devolve os que falharam e o último erro.
fn devolver_memoria<A: Acoes>(acoes: &mut A, processos: Vec<MemoriaOriginal>) -> (Vec<MemoriaOriginal>, Option<io::Error>) {
    let chaves: Vec<(u32, i64)> = processos.iter().map(|m| (m.pid, m.criado_em)).collect();
    let vivos = acoes.vivos(&chaves);
    let mut sobra = Vec::new();
    let mut ultimo_erro = None;
    for (m, vivo) in processos.into_iter().zip(vivos) {
        if !vivo {
            continue; // fechou: não há o que desfazer
        }
        if let Err(erro) = acoes.definir_prioridade_de_memoria(m.pid, m.original) {
            ultimo_erro = Some(erro);
            sobra.push(m);
        }
    }
    (sobra, ultimo_erro)
}

fn desfazer<A: Acoes>(acoes: &mut A, alteracao: &Alteracao) -> io::Result<()> {
    match alteracao {
        Alteracao::MemoriaBaixa { processos } => match devolver_memoria(acoes, processos.clone()).1 {
            Some(erro) => Err(erro),
            None => Ok(()),
        },
        Alteracao::PlanoEnergia { original, .. } => acoes.definir_plano_energia(hex(original)),
        // Se o processo fechou, não há o que desfazer.
        Alteracao::Prioridade { pid, criado_em, original, .. } => {
            if acoes.vivo(*pid, *criado_em) { acoes.definir_prioridade(*pid, *original) } else { Ok(()) }
        }
        Alteracao::Pausa { pid, criado_em, .. } => {
            if acoes.vivo(*pid, *criado_em) { acoes.retomar(*pid) } else { Ok(()) }
        }
        Alteracao::Fechamento { .. } => Ok(()),
    }
}

fn finalizar_arquivo(arquivo: &Path, estado: &Estado) {
    if estado.alteracoes.is_empty() {
        let _ = fs::remove_file(arquivo);
    } else {
        let _ = gravar(arquivo, estado);
    }
}

/// Grava num arquivo temporário e troca de nome, para nunca deixar um
/// `estado.json` pela metade se a energia cair no meio.
fn gravar(arquivo: &Path, estado: &Estado) -> io::Result<()> {
    if let Some(pasta) = arquivo.parent() {
        fs::create_dir_all(pasta)?;
    }
    let temporario = arquivo.with_extension("json.tmp");
    fs::write(&temporario, serde_json::to_vec_pretty(estado)?)?;
    fs::rename(&temporario, arquivo)
}

/// O que sobrou de uma execução anterior.
#[derive(Debug, Default)]
pub struct Restauracao {
    pub jogo: String,
    pub desfeitas: Vec<String>,
    pub problemas: Vec<String>,
}

/// Desfaz o que ficou no `estado.json` de uma execução que morreu no meio.
/// `Ok(None)` quando não havia nada pendente.
pub fn restaurar_pendente<A: Acoes>(arquivo: &Path, acoes: &mut A) -> io::Result<Option<Restauracao>> {
    let texto = match fs::read(arquivo) {
        Ok(texto) => texto,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let mut estado: Estado = serde_json::from_slice(&texto).map_err(io::Error::other)?;
    let descricoes: Vec<String> = estado.alteracoes.iter().map(Alteracao::descricao).collect();
    let problemas = desfazer_lista(acoes, &mut estado.alteracoes);
    finalizar_arquivo(arquivo, &estado);
    Ok(Some(Restauracao { jogo: estado.jogo, desfeitas: descricoes, problemas }))
}

/// Para os testes poderem passar `&mut AcoesFalsas` e continuar olhando.
impl<A: Acoes> Acoes for &mut A {
    fn vivo(&mut self, pid: u32, criado_em: i64) -> bool {
        (**self).vivo(pid, criado_em)
    }
    fn vivos(&mut self, lista: &[(u32, i64)]) -> Vec<bool> {
        (**self).vivos(lista)
    }
    fn prioridade_de_memoria(&mut self, pid: u32) -> io::Result<u32> {
        (**self).prioridade_de_memoria(pid)
    }
    fn definir_prioridade_de_memoria(&mut self, pid: u32, nivel: u32) -> io::Result<()> {
        (**self).definir_prioridade_de_memoria(pid, nivel)
    }
    fn pausar(&mut self, pid: u32) -> io::Result<()> {
        (**self).pausar(pid)
    }
    fn retomar(&mut self, pid: u32) -> io::Result<()> {
        (**self).retomar(pid)
    }
    fn esvaziar_memoria(&mut self, pid: u32) -> io::Result<()> {
        (**self).esvaziar_memoria(pid)
    }
    fn prioridade(&mut self, pid: u32) -> io::Result<u32> {
        (**self).prioridade(pid)
    }
    fn definir_prioridade(&mut self, pid: u32, classe: u32) -> io::Result<()> {
        (**self).definir_prioridade(pid, classe)
    }
    fn plano_energia(&mut self) -> io::Result<u128> {
        (**self).plano_energia()
    }
    fn definir_plano_energia(&mut self, plano: u128) -> io::Result<()> {
        (**self).definir_plano_energia(plano)
    }
    fn fechar_janelas(&mut self, pid: u32) -> usize {
        (**self).fechar_janelas(pid)
    }
}

#[cfg(test)]
pub(crate) mod testes {
    use super::*;
    use crate::disco::testes::PastaTeste;
    use crate::modo_jogo::protecao::testes::proc;

    /// Windows de mentira: guarda o que foi feito.
    #[derive(Default)]
    pub(crate) struct AcoesFalsas {
        pub vivos: Vec<(u32, i64)>,
        pub pausados: Vec<u32>,
        pub prioridades: std::collections::HashMap<u32, u32>,
        pub plano: u128,
        pub falhar_pausa: bool,
        /// Prioridade de memória de cada PID (5 = normal se não estiver aqui).
        pub memoria: std::collections::HashMap<u32, u32>,
        /// PIDs em que o Windows "não deixa" mexer na memória.
        pub memoria_negada: Vec<u32>,
        pub historico: Vec<String>,
    }

    impl Acoes for AcoesFalsas {
        fn vivo(&mut self, pid: u32, criado_em: i64) -> bool {
            self.vivos.contains(&(pid, criado_em))
        }
        fn prioridade_de_memoria(&mut self, pid: u32) -> io::Result<u32> {
            Ok(*self.memoria.get(&pid).unwrap_or(&5))
        }
        fn definir_prioridade_de_memoria(&mut self, pid: u32, nivel: u32) -> io::Result<()> {
            if self.memoria_negada.contains(&pid) {
                return Err(io::Error::from(io::ErrorKind::PermissionDenied));
            }
            self.memoria.insert(pid, nivel);
            self.historico.push(format!("memoria {pid} {nivel}"));
            Ok(())
        }
        fn pausar(&mut self, pid: u32) -> io::Result<()> {
            if self.falhar_pausa {
                return Err(io::Error::from(io::ErrorKind::PermissionDenied));
            }
            self.pausados.push(pid);
            self.historico.push(format!("pausar {pid}"));
            Ok(())
        }
        fn retomar(&mut self, pid: u32) -> io::Result<()> {
            self.pausados.retain(|&p| p != pid);
            self.historico.push(format!("retomar {pid}"));
            Ok(())
        }
        fn esvaziar_memoria(&mut self, _pid: u32) -> io::Result<()> {
            Ok(())
        }
        fn prioridade(&mut self, pid: u32) -> io::Result<u32> {
            Ok(*self.prioridades.get(&pid).unwrap_or(&0x20))
        }
        fn definir_prioridade(&mut self, pid: u32, classe: u32) -> io::Result<()> {
            self.prioridades.insert(pid, classe);
            self.historico.push(format!("prioridade {pid} {classe:#x}"));
            Ok(())
        }
        fn plano_energia(&mut self) -> io::Result<u128> {
            Ok(self.plano)
        }
        fn definir_plano_energia(&mut self, plano: u128) -> io::Result<()> {
            self.plano = plano;
            self.historico.push(format!("plano {}", nome_do_plano(plano)));
            Ok(())
        }
        fn fechar_janelas(&mut self, _pid: u32) -> usize {
            1
        }
    }

    const EQUILIBRADO: u128 = 0x381b4222_f694_41f0_9685_ff5bb260df2e;

    fn acoes() -> AcoesFalsas {
        AcoesFalsas { vivos: vec![(10, 10), (20, 20)], plano: EQUILIBRADO, ..Default::default() }
    }

    #[test]
    fn desfaz_tudo_em_ordem_inversa_ao_sair_de_escopo() {
        let pasta = PastaTeste::nova("sessao");
        let arquivo = pasta.0.join("estado.json");
        let jogo = proc(10, 1, "jogo.exe", 10);
        let app = proc(20, 1, "app.exe", 20);
        let mut acoes = acoes();
        {
            let mut sessao = Sessao::nova(arquivo.clone(), "jogo.exe", &mut acoes);
            assert!(sessao.trocar_plano(ALTO_DESEMPENHO).unwrap());
            assert!(sessao.definir_prioridade(&jogo, 0x80).unwrap());
            sessao.pausar(&app).unwrap();
            // Tudo anotado no disco antes de acontecer.
            let gravado: Estado = serde_json::from_slice(&fs::read(&arquivo).unwrap()).unwrap();
            assert_eq!(gravado.alteracoes.len(), 3);
        } // <- Drop
        assert_eq!(
            acoes.historico,
            [
                "plano Alto desempenho",
                "prioridade 10 0x80",
                "pausar 20",
                "retomar 20",
                "prioridade 10 0x20",
                "plano Equilibrado",
            ]
        );
        assert!(!arquivo.exists());
    }

    #[test]
    fn desfaz_mesmo_com_panic() {
        let pasta = PastaTeste::nova("panic");
        let arquivo = pasta.0.join("estado.json");
        let app = proc(20, 1, "app.exe", 20);
        let mut acoes = acoes();
        let resultado = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut sessao = Sessao::nova(arquivo.clone(), "jogo.exe", &mut acoes);
            sessao.pausar(&app).unwrap();
            panic!("erro no meio da partida");
        }));
        assert!(resultado.is_err());
        assert!(acoes.pausados.is_empty());
        assert!(!arquivo.exists());
    }

    #[test]
    fn processo_morto_a_forca_e_restaurado_na_proxima_execucao() {
        let pasta = PastaTeste::nova("morto");
        let arquivo = pasta.0.join("estado.json");
        let app = proc(20, 1, "app.exe", 20);
        let mut acoes = acoes();
        let mut sessao = Sessao::nova(arquivo.clone(), "jogo.exe", &mut acoes);
        sessao.trocar_plano(ALTO_DESEMPENHO).unwrap();
        sessao.pausar(&app).unwrap();
        std::mem::forget(sessao); // como um "Finalizar tarefa": o Drop não roda
        assert!(arquivo.exists());

        let mut acoes_depois = AcoesFalsas { vivos: vec![(20, 20)], plano: ALTO_DESEMPENHO, ..Default::default() };
        acoes_depois.pausados.push(20);
        let r = restaurar_pendente(&arquivo, &mut acoes_depois).unwrap().unwrap();
        assert_eq!(r.jogo, "jogo.exe");
        assert!(r.problemas.is_empty());
        assert!(acoes_depois.pausados.is_empty());
        assert_eq!(acoes_depois.plano, EQUILIBRADO);
        assert!(!arquivo.exists());
        assert!(restaurar_pendente(&arquivo, &mut acoes_depois).unwrap().is_none());
    }

    #[test]
    fn pid_reaproveitado_nao_e_retomado() {
        let pasta = PastaTeste::nova("reuso");
        let arquivo = pasta.0.join("estado.json");
        let mut acoes = acoes();
        let mut sessao = Sessao::nova(arquivo.clone(), "jogo.exe", &mut acoes);
        sessao.pausar(&proc(20, 1, "app.exe", 20)).unwrap();
        std::mem::forget(sessao);
        // Depois do reboot, o PID 20 é de outro processo (criado em 99).
        let mut depois = AcoesFalsas { vivos: vec![(20, 99)], ..Default::default() };
        restaurar_pendente(&arquivo, &mut depois).unwrap();
        assert!(depois.historico.is_empty());
    }

    #[test]
    fn memoria_baixa_anota_so_quem_mudou_e_volta_ao_original() {
        let pasta = PastaTeste::nova("memoria");
        let arquivo = pasta.0.join("estado.json");
        let (a, b, c) = (proc(20, 1, "a.exe", 20), proc(30, 1, "b.exe", 30), proc(40, 1, "c.exe", 40));
        let mut acoes = AcoesFalsas {
            vivos: vec![(20, 20), (30, 30), (40, 40)],
            memoria: [(40, 1)].into(), // já estava "muito baixa": fica de fora
            memoria_negada: vec![30],  // o Windows não deixa
            ..Default::default()
        };
        {
            let mut sessao = Sessao::nova(arquivo.clone(), "jogo.exe", &mut acoes);
            assert_eq!(sessao.baixar_memoria(&[&a, &b, &c], MEMORIA_BAIXA).unwrap(), 1);
            let gravado: Estado = serde_json::from_slice(&fs::read(&arquivo).unwrap()).unwrap();
            let [Alteracao::MemoriaBaixa { processos }] = gravado.alteracoes.as_slice() else { panic!("{gravado:?}") };
            assert_eq!(processos.iter().map(|m| m.pid).collect::<Vec<_>>(), [20]);
            assert_eq!(processos[0].original, 5);
        }
        assert_eq!(acoes.historico, ["memoria 20 2", "memoria 20 5"]);
        assert_eq!(acoes.memoria[&40], 1);
        assert!(!arquivo.exists());
    }

    #[test]
    fn memoria_baixa_e_devolvida_depois_de_morto_a_forca() {
        let pasta = PastaTeste::nova("memoria-morto");
        let arquivo = pasta.0.join("estado.json");
        let mut acoes = acoes();
        let mut sessao = Sessao::nova(arquivo.clone(), "jogo.exe", &mut acoes);
        let apps = [proc(10, 1, "a.exe", 10), proc(20, 1, "b.exe", 20)];
        assert_eq!(sessao.baixar_memoria(&[&apps[0], &apps[1]], MEMORIA_BAIXA).unwrap(), 2);
        std::mem::forget(sessao);

        // Na próxima execução, o PID 10 já fechou; o 20 continua aberto.
        let mut depois = AcoesFalsas { vivos: vec![(20, 20)], memoria: [(20, 2)].into(), ..Default::default() };
        let r = restaurar_pendente(&arquivo, &mut depois).unwrap().unwrap();
        assert!(r.problemas.is_empty());
        assert_eq!(depois.historico, ["memoria 20 5"]);
        assert!(!arquivo.exists());
    }

    #[test]
    fn memoria_que_nao_volta_continua_anotada() {
        let pasta = PastaTeste::nova("memoria-falha");
        let arquivo = pasta.0.join("estado.json");
        let mut acoes = acoes();
        let mut sessao = Sessao::nova(arquivo.clone(), "jogo.exe", &mut acoes);
        sessao.baixar_memoria(&[&proc(10, 1, "a.exe", 10), &proc(20, 1, "b.exe", 20)], MEMORIA_BAIXA).unwrap();
        std::mem::forget(sessao);

        let mut depois = AcoesFalsas { vivos: vec![(10, 10), (20, 20)], memoria_negada: vec![20], ..Default::default() };
        let r = restaurar_pendente(&arquivo, &mut depois).unwrap().unwrap();
        assert_eq!(r.problemas.len(), 1);
        let gravado: Estado = serde_json::from_slice(&fs::read(&arquivo).unwrap()).unwrap();
        let [Alteracao::MemoriaBaixa { processos }] = gravado.alteracoes.as_slice() else { panic!("{gravado:?}") };
        assert_eq!(processos.iter().map(|m| m.pid).collect::<Vec<_>>(), [20]);
    }

    /// Com o Windows de verdade: baixa a memória de um processo filho
    /// descartável e confere que ela volta ao fim da sessão.
    #[test]
    fn memoria_baixa_de_verdade_num_processo_filho() {
        let pasta = PastaTeste::nova("memoria-real");
        let arquivo = pasta.0.join("estado.json");
        let mut filho = std::process::Command::new("ping")
            .args(["-n", "30", "127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let pid = filho.id();
        let processo = sistema::listar_processos().unwrap().into_iter().find(|p| p.pid == pid).unwrap();
        let original = sistema::prioridade_de_memoria(pid).unwrap();
        {
            let mut sessao = Sessao::nova(arquivo.clone(), "jogo.exe", AcoesWindows);
            assert_eq!(sessao.baixar_memoria(&[&processo], MEMORIA_BAIXA).unwrap(), 1);
            assert_eq!(sistema::prioridade_de_memoria(pid).unwrap(), MEMORIA_BAIXA);
        }
        assert_eq!(sistema::prioridade_de_memoria(pid).unwrap(), original);
        assert!(!arquivo.exists());
        let _ = filho.kill();
        let _ = filho.wait();
    }

    #[test]
    fn falha_ao_pausar_nao_fica_anotada() {
        let pasta = PastaTeste::nova("falha");
        let arquivo = pasta.0.join("estado.json");
        let mut acoes = AcoesFalsas { falhar_pausa: true, ..acoes() };
        let mut sessao = Sessao::nova(arquivo.clone(), "jogo.exe", &mut acoes);
        assert!(sessao.pausar(&proc(20, 1, "app.exe", 20)).is_err());
        assert!(sessao.alteracoes().is_empty());
        drop(sessao);
        assert!(acoes.historico.is_empty());
    }
}
