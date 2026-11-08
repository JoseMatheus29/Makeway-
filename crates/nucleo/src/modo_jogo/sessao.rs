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
}

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
        if let Err(erro) = desfazer(acoes, &alteracao) {
            problemas.push(format!("não deu para desfazer \"{}\": {erro}", alteracao.descricao()));
            restantes.push(alteracao);
        }
    }
    restantes.reverse();
    *alteracoes = restantes;
    problemas
}

fn desfazer<A: Acoes>(acoes: &mut A, alteracao: &Alteracao) -> io::Result<()> {
    match alteracao {
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
        pub historico: Vec<String>,
    }

    impl Acoes for AcoesFalsas {
        fn vivo(&mut self, pid: u32, criado_em: i64) -> bool {
            self.vivos.contains(&(pid, criado_em))
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
