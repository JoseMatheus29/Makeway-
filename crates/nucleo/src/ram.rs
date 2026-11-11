//! Análise de RAM (só leitura): memória do sistema, arquivo de paginação
//! e processos agrupados por app.

use std::collections::HashMap;
use std::io;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::sistema;

/// Memória física e confirmada, em bytes.
#[derive(Debug, Clone, Serialize)]
pub struct MemoriaSistema {
    /// RAM instalada nos pentes. `None` se o Windows não souber dizer.
    pub instalada: Option<u64>,
    /// RAM que o Windows pode usar (instalada menos a reservada para hardware).
    pub total: u64,
    pub disponivel: u64,
    pub em_uso: u64,
    pub carga_pct: u32,
    /// Memória confirmada (commit): o que os programas reservaram, em RAM ou na paginação.
    pub confirmada: u64,
    /// RAM + arquivo de paginação: o máximo que pode ser confirmado.
    pub limite_confirmada: u64,
    /// `None` se o Windows negar a leitura das listas de páginas.
    pub listas: Option<ListasMemoria>,
    pub pool_paginado: u64,
    pub pool_nao_paginado: u64,
}

/// Como a RAM que não está em uso se divide, em bytes.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct ListasMemoria {
    /// Cache de arquivos que pode ser descartado na hora (lista standby).
    pub standby: u64,
    /// Páginas alteradas que precisam ir para o disco antes de reaproveitar.
    pub modificada: u64,
    /// Livre de verdade, sem nada dentro.
    pub livre: u64,
}

impl ListasMemoria {
    /// O "Em cache" do Gerenciador de Tarefas.
    pub fn em_cache(&self) -> u64 {
        self.standby + self.modificada
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ArquivoPaginacao {
    pub caminho: String,
    pub tamanho: u64,
    pub em_uso: u64,
    /// Maior uso desde que o Windows ligou.
    pub pico: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Processo {
    pub pid: u32,
    pub pid_pai: u32,
    pub nome: String,
    /// Conjunto de trabalho privado: a coluna "Memória" do Gerenciador de Tarefas.
    pub ws_privado: u64,
    /// Conjunto de trabalho total, incluindo DLLs compartilhadas com outros processos.
    pub working_set: u64,
    /// Memória confirmada só deste processo (coluna "Tamanho da confirmação").
    pub confirmada: u64,
    /// Sessão do Windows: 0 = serviços, 1 em diante = usuários logados.
    pub sessao: u32,
    /// Momento em que o processo abriu (FILETIME). PID + este valor
    /// identificam o processo mesmo se o Windows reusar o PID depois.
    pub criado_em: i64,
    /// Tempo de CPU usado desde que abriu, em unidades de 100 ns.
    pub cpu_100ns: u64,
    /// Todas as threads paradas por suspensão.
    pub suspenso: bool,
}

/// Todos os processos com o mesmo .exe somados.
#[derive(Debug, Clone, Serialize)]
pub struct App {
    pub nome: String,
    pub processos: usize,
    pub ws_privado: u64,
    pub working_set: u64,
    pub confirmada: u64,
    pub pids: Vec<u32>,
}

/// Outra conta do Windows aberta ao mesmo tempo ("Trocar de usuário" deixa
/// tudo dela rodando e ocupando memória).
#[derive(Debug, Clone, Serialize)]
pub struct OutraConta {
    pub sessao: u32,
    /// Nome da conta, se o Windows disser.
    pub usuario: Option<String>,
    pub processos: usize,
    pub ws_privado: u64,
    /// Memória que os processos dela reservaram (conta no limite de memória).
    pub confirmada: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RelatorioRam {
    /// Segundos desde 1970 (UTC).
    pub instante_unix: u64,
    pub memoria: MemoriaSistema,
    pub paginacao: Vec<ArquivoPaginacao>,
    /// Os apps desta conta e dos serviços do Windows, do que mais usa RAM
    /// para o que menos usa. Os de outras contas ficam em `outras_contas`.
    pub apps: Vec<App>,
    /// Outras contas logadas, da que mais usa RAM para a que menos usa.
    pub outras_contas: Vec<OutraConta>,
    /// Todos os processos (de todas as contas), do que mais usa RAM para o que menos usa.
    pub processos: Vec<Processo>,
}

impl RelatorioRam {
    /// Soma do conjunto de trabalho privado de todos os processos.
    pub fn total_processos(&self) -> u64 {
        self.processos.iter().map(|p| p.ws_privado).sum()
    }
}

/// Um processo de outra conta logada: nem desta sessão, nem serviço (sessão 0).
pub fn de_outra_conta(p: &Processo, minha_sessao: u32) -> bool {
    p.sessao != 0 && p.sessao != minha_sessao
}

/// Sessão do Windows deste programa (a da conta que está usando o PC).
pub fn sessao_atual(processos: &[Processo]) -> Option<u32> {
    let eu = std::process::id();
    processos.iter().find(|p| p.pid == eu).map(|p| p.sessao)
}

/// Lê tudo do Windows e monta o relatório.
pub fn analisar() -> io::Result<RelatorioRam> {
    let (memoria, pagina) = sistema::memoria_sistema()?;
    let paginacao = sistema::arquivos_paginacao(pagina)?;
    let mut processos = sistema::listar_processos()?;
    processos.sort_by(|a, b| b.ws_privado.cmp(&a.ws_privado).then(a.pid.cmp(&b.pid)));
    let minha = sessao_atual(&processos).unwrap_or(0);
    let (meus, outros): (Vec<Processo>, Vec<Processo>) =
        processos.iter().cloned().partition(|p| !de_outra_conta(p, minha));
    let apps = agrupar_por_app(&meus);
    let mut outras_contas = agrupar_por_sessao(&outros);
    for conta in &mut outras_contas {
        conta.usuario = sistema::usuario_da_sessao(conta.sessao);
    }
    let instante_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    Ok(RelatorioRam { instante_unix, memoria, paginacao, apps, outras_contas, processos })
}

/// Soma os processos de cada sessão (sem o nome da conta).
pub fn agrupar_por_sessao(processos: &[Processo]) -> Vec<OutraConta> {
    let mut por_sessao: HashMap<u32, OutraConta> = HashMap::new();
    for p in processos {
        let conta = por_sessao
            .entry(p.sessao)
            .or_insert(OutraConta { sessao: p.sessao, usuario: None, processos: 0, ws_privado: 0, confirmada: 0 });
        conta.processos += 1;
        conta.ws_privado += p.ws_privado;
        conta.confirmada += p.confirmada;
    }
    let mut contas: Vec<OutraConta> = por_sessao.into_values().collect();
    contas.sort_by(|a, b| b.ws_privado.cmp(&a.ws_privado).then(a.sessao.cmp(&b.sessao)));
    contas
}

/// Memória do sistema e quanto do arquivo de paginação está em uso
/// (somando todos), para as amostras do modo jogo.
pub fn memoria_e_paginacao() -> io::Result<(MemoriaSistema, u64)> {
    let (memoria, pagina) = sistema::memoria_sistema()?;
    let em_uso = sistema::arquivos_paginacao(pagina)?.iter().map(|a| a.em_uso).sum();
    Ok((memoria, em_uso))
}

/// Agrupa pelo nome do .exe, sem diferenciar maiúsculas
/// (todas as abas do Chrome viram um "chrome.exe").
pub fn agrupar_por_app(processos: &[Processo]) -> Vec<App> {
    let mut por_nome: HashMap<String, App> = HashMap::new();
    for p in processos {
        let app = por_nome.entry(p.nome.to_lowercase()).or_insert_with(|| App {
            nome: p.nome.clone(),
            processos: 0,
            ws_privado: 0,
            working_set: 0,
            confirmada: 0,
            pids: Vec::new(),
        });
        app.processos += 1;
        app.ws_privado += p.ws_privado;
        app.working_set += p.working_set;
        app.confirmada += p.confirmada;
        app.pids.push(p.pid);
    }
    let mut apps: Vec<App> = por_nome.into_values().collect();
    apps.sort_by(|a, b| b.ws_privado.cmp(&a.ws_privado).then_with(|| a.nome.cmp(&b.nome)));
    apps
}

#[cfg(test)]
mod testes {
    use super::*;

    fn processo(pid: u32, nome: &str, mb: u64) -> Processo {
        Processo {
            pid,
            pid_pai: 1,
            nome: nome.to_owned(),
            ws_privado: mb << 20,
            working_set: (mb + 10) << 20,
            confirmada: (mb * 2) << 20,
            sessao: 1,
            criado_em: i64::from(pid),
            cpu_100ns: 0,
            suspenso: false,
        }
    }

    #[test]
    fn soma_processos_do_mesmo_exe() {
        let lista = [
            processo(10, "chrome.exe", 300),
            processo(11, "Chrome.exe", 200),
            processo(12, "Discord.exe", 400),
            processo(13, "chrome.exe", 100),
        ];
        let apps = agrupar_por_app(&lista);

        assert_eq!(apps.len(), 2);
        assert_eq!(apps[0].nome, "chrome.exe");
        assert_eq!(apps[0].processos, 3);
        assert_eq!(apps[0].ws_privado, 600 << 20);
        assert_eq!(apps[0].working_set, 630 << 20);
        assert_eq!(apps[0].confirmada, 1200 << 20);
        assert_eq!(apps[0].pids, vec![10, 11, 13]);
        assert_eq!(apps[1].nome, "Discord.exe");
    }

    #[test]
    fn empate_ordena_por_nome() {
        let lista = [processo(1, "b.exe", 50), processo(2, "a.exe", 50)];
        let nomes: Vec<_> = agrupar_por_app(&lista).into_iter().map(|a| a.nome).collect();
        assert_eq!(nomes, ["a.exe", "b.exe"]);
    }

    #[test]
    fn lista_vazia() {
        assert!(agrupar_por_app(&[]).is_empty());
    }

    #[test]
    fn separa_outras_contas_dos_servicos_e_desta() {
        let mut lista = [processo(1, "a.exe", 100), processo(2, "b.exe", 50), processo(3, "c.exe", 70)];
        lista[0].sessao = 2; // esta conta
        lista[1].sessao = 0; // serviço
        lista[2].sessao = 1; // outra conta
        assert!(!de_outra_conta(&lista[0], 2));
        assert!(!de_outra_conta(&lista[1], 2));
        assert!(de_outra_conta(&lista[2], 2));
        let contas = agrupar_por_sessao(&lista[2..]);
        assert_eq!(contas.len(), 1);
        assert_eq!((contas[0].sessao, contas[0].processos, contas[0].ws_privado), (1, 1, 70 << 20));
    }

    #[test]
    fn analisa_o_sistema_real() {
        let relatorio = analisar().unwrap();
        assert!(!relatorio.apps.is_empty());
        let soma_apps: u64 = relatorio.apps.iter().map(|a| a.ws_privado).sum();
        let soma_outras: u64 = relatorio.outras_contas.iter().map(|c| c.ws_privado).sum();
        assert_eq!(soma_apps + soma_outras, relatorio.total_processos());
        assert!(relatorio.total_processos() < relatorio.memoria.total);
        // Este processo é desta conta: nunca aparece como outra.
        let minha = sessao_atual(&relatorio.processos).unwrap();
        assert!(relatorio.outras_contas.iter().all(|c| c.sessao != minha && c.sessao != 0));
    }
}
