//! Relatório de cada partida: RAM, arquivo de paginação e quem competiu
//! com o jogo. Gravado em JSON (resumo) e CSV (uma linha por amostra).

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::{Deserialize, Serialize};

use super::deteccao::Jogo;
use super::protecao::Protecao;
use crate::formato::{decimal, tamanho};
use crate::ram::{MemoriaSistema, Processo};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Amostra {
    pub segundos: f64,
    pub ram_disponivel: u64,
    pub confirmada: u64,
    pub paginacao_em_uso: u64,
    pub ram_jogo: u64,
    pub cpu_jogo_pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Concorrente {
    pub nome: String,
    pub processos: usize,
    /// Maior soma de memória privada vista numa amostra.
    pub ram_max: u64,
    /// Média durante a partida, em % da CPU inteira (todos os núcleos).
    pub cpu_media_pct: f64,
    pub protegido: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelatorioSessao {
    pub jogo: String,
    pub caminho_jogo: Option<PathBuf>,
    pub detectado_por: String,
    /// "observar" ou "otimizar".
    pub modo: String,
    pub inicio_unix: i64,
    pub duracao_segundos: u64,
    pub amostras: usize,
    pub ram_total: u64,
    pub ram_disponivel_inicio: u64,
    pub ram_disponivel_min: u64,
    pub confirmada_max: u64,
    pub limite_confirmada: u64,
    pub paginacao_inicio: u64,
    pub paginacao_max: u64,
    pub ram_jogo_max: u64,
    pub cpu_jogo_media_pct: f64,
    /// Do que mais usou RAM para o que menos usou.
    pub concorrentes: Vec<Concorrente>,
    pub alteracoes: Vec<String>,
    pub avisos: Vec<String>,
    pub sugestoes: Vec<String>,
}

/// Junta as amostras enquanto o jogo roda.
pub struct Amostrador {
    jogo: Jogo,
    inicio: Instant,
    inicio_unix: i64,
    nucleos: f64,
    amostras: Vec<Amostra>,
    /// (pid, criado_em) → (nome, CPU na primeira vez que apareceu, CPU na última)
    cpu: HashMap<(u32, i64), (String, u64, u64)>,
    /// nome em minúsculas → (nome, maior RAM somada, maior nº de processos)
    ram_por_app: HashMap<String, (String, u64, usize)>,
    ultimo_jogo: Option<(f64, u64)>,
    ram_total: u64,
    limite_confirmada: u64,
}

impl Amostrador {
    pub fn novo(jogo: &Jogo) -> Self {
        Self {
            jogo: jogo.clone(),
            inicio: Instant::now(),
            inicio_unix: chrono::Utc::now().timestamp(),
            nucleos: std::thread::available_parallelism().map_or(1.0, |n| n.get() as f64),
            amostras: Vec::new(),
            cpu: HashMap::new(),
            ram_por_app: HashMap::new(),
            ultimo_jogo: None,
            ram_total: 0,
            limite_confirmada: 0,
        }
    }

    pub fn amostrar(&mut self, processos: &[Processo], memoria: &MemoriaSistema, paginacao_em_uso: u64) {
        let segundos = self.inicio.elapsed().as_secs_f64();
        self.ram_total = memoria.total;
        self.limite_confirmada = memoria.limite_confirmada;

        let mut ram_jogo = 0;
        let mut cpu_jogo_pct = 0.0;
        let mut por_app: HashMap<String, (String, u64, usize)> = HashMap::new();
        for p in processos {
            if p.pid == self.jogo.pid && p.criado_em == self.jogo.criado_em {
                ram_jogo = p.ws_privado;
                if let Some((t, cpu)) = self.ultimo_jogo {
                    let intervalo = segundos - t;
                    if intervalo > 0.0 {
                        let usado = p.cpu_100ns.saturating_sub(cpu) as f64 / 1e7;
                        cpu_jogo_pct = usado / (intervalo * self.nucleos) * 100.0;
                    }
                }
                self.ultimo_jogo = Some((segundos, p.cpu_100ns));
                continue;
            }
            let entrada = self.cpu.entry((p.pid, p.criado_em)).or_insert((p.nome.clone(), p.cpu_100ns, p.cpu_100ns));
            entrada.2 = p.cpu_100ns;
            let app = por_app.entry(p.nome.to_lowercase()).or_insert((p.nome.clone(), 0, 0));
            app.1 += p.ws_privado;
            app.2 += 1;
        }
        for (chave, (nome, ram, quantos)) in por_app {
            let maior = self.ram_por_app.entry(chave).or_insert((nome, 0, 0));
            maior.1 = maior.1.max(ram);
            maior.2 = maior.2.max(quantos);
        }
        self.amostras.push(Amostra {
            segundos,
            ram_disponivel: memoria.disponivel,
            confirmada: memoria.confirmada,
            paginacao_em_uso,
            ram_jogo,
            cpu_jogo_pct,
        });
    }

    pub fn finalizar(
        self,
        modo: &str,
        alteracoes: Vec<String>,
        avisos: Vec<String>,
        processos: &[Processo],
        nas_listas: &[String],
    ) -> (RelatorioSessao, Vec<Amostra>) {
        let duracao = self.inicio.elapsed().as_secs_f64().max(0.001);
        let a = &self.amostras;
        let primeira = a.first();

        let mut cpu_por_app: HashMap<String, f64> = HashMap::new();
        for (nome, inicio, fim) in self.cpu.values() {
            *cpu_por_app.entry(nome.to_lowercase()).or_default() += fim.saturating_sub(*inicio) as f64 / 1e7;
        }
        let protecao = Protecao::nova(processos, None);
        let protegido = |nome: &str| {
            processos
                .iter()
                .find(|p| p.nome.eq_ignore_ascii_case(nome))
                .is_some_and(|p| protecao.motivo(p).is_some())
        };
        let mut concorrentes: Vec<Concorrente> = self
            .ram_por_app
            .iter()
            .map(|(chave, (nome, ram_max, quantos))| Concorrente {
                nome: nome.clone(),
                processos: *quantos,
                ram_max: *ram_max,
                cpu_media_pct: cpu_por_app.get(chave).copied().unwrap_or(0.0) / (duracao * self.nucleos) * 100.0,
                protegido: protegido(nome),
            })
            .collect();
        concorrentes.sort_by_key(|c| std::cmp::Reverse(c.ram_max));
        concorrentes.truncate(25);

        let cpu_jogo: Vec<f64> = a.iter().skip(1).map(|x| x.cpu_jogo_pct).collect();
        let mut relatorio = RelatorioSessao {
            jogo: self.jogo.nome.clone(),
            caminho_jogo: self.jogo.caminho.clone(),
            detectado_por: self.jogo.detectado_por.clone(),
            modo: modo.to_owned(),
            inicio_unix: self.inicio_unix,
            duracao_segundos: duracao as u64,
            amostras: a.len(),
            ram_total: self.ram_total,
            ram_disponivel_inicio: primeira.map_or(0, |x| x.ram_disponivel),
            ram_disponivel_min: a.iter().map(|x| x.ram_disponivel).min().unwrap_or(0),
            confirmada_max: a.iter().map(|x| x.confirmada).max().unwrap_or(0),
            limite_confirmada: self.limite_confirmada,
            paginacao_inicio: primeira.map_or(0, |x| x.paginacao_em_uso),
            paginacao_max: a.iter().map(|x| x.paginacao_em_uso).max().unwrap_or(0),
            ram_jogo_max: a.iter().map(|x| x.ram_jogo).max().unwrap_or(0),
            cpu_jogo_media_pct: if cpu_jogo.is_empty() { 0.0 } else { cpu_jogo.iter().sum::<f64>() / cpu_jogo.len() as f64 },
            concorrentes,
            alteracoes,
            avisos,
            sugestoes: Vec::new(),
        };
        relatorio.sugestoes = sugestoes(&relatorio, nas_listas);
        (relatorio, self.amostras)
    }
}

/// Regras simples a partir dos números da partida. Apps protegidos e os
/// que já estão nas listas `pausar`/`fechar` não são sugeridos.
pub fn sugestoes(r: &RelatorioSessao, nas_listas: &[String]) -> Vec<String> {
    let mut lista = Vec::new();
    let livres: Vec<&Concorrente> = r
        .concorrentes
        .iter()
        .filter(|c| !c.protegido && !nas_listas.iter().any(|n| n.eq_ignore_ascii_case(&c.nome)))
        .collect();
    let crescimento = r.paginacao_max.saturating_sub(r.paginacao_inicio);

    if r.amostras < 3 {
        lista.push("Partida curta demais para tirar conclusões.".to_owned());
        return lista;
    }
    if r.ram_disponivel_min >= 2 << 30 {
        lista.push(format!(
            "A RAM disponível nunca caiu abaixo de {}: pausar apps quase não muda o FPS. \
             O ganho, se houver, vem de tirar quem usa CPU ou GPU.",
            tamanho(r.ram_disponivel_min)
        ));
    } else {
        let candidatos: Vec<String> = livres
            .iter()
            .filter(|c| c.ram_max >= 200 << 20)
            .take(3)
            .map(|c| format!("{} ({})", c.nome, tamanho(c.ram_max)))
            .collect();
        let situacao = if r.ram_disponivel_min < 1 << 30 || crescimento > 256 << 20 {
            "Faltou RAM"
        } else {
            "A RAM ficou apertada"
        };
        let mut frase = format!(
            "{situacao}: a disponível chegou a {} e o arquivo de paginação cresceu {}.",
            tamanho(r.ram_disponivel_min),
            tamanho(crescimento)
        );
        if !candidatos.is_empty() {
            frase.push_str(&format!(" Candidatos à lista `pausar`: {}.", candidatos.join(", ")));
        }
        lista.push(frase);
    }
    for c in livres.iter().filter(|c| c.cpu_media_pct >= 3.0) {
        lista.push(format!(
            "{} usou {}% da CPU em média durante o jogo: candidato a `pausar` ou `fechar`.",
            c.nome,
            decimal(c.cpu_media_pct, 1)
        ));
    }
    lista
}

/// Grava `<data>_<jogo>.json` e `.csv` em `pasta`. Devolve o caminho do JSON.
pub fn gravar(pasta: &Path, r: &RelatorioSessao, amostras: &[Amostra]) -> io::Result<PathBuf> {
    fs::create_dir_all(pasta)?;
    let quando = chrono::DateTime::from_timestamp(r.inicio_unix, 0)
        .map(|d| d.with_timezone(&chrono::Local).format("%Y-%m-%d_%H-%M-%S").to_string())
        .unwrap_or_default();
    let jogo: String = r
        .jogo
        .trim_end_matches(".exe")
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '_' })
        .collect();
    let base = pasta.join(format!("{quando}_{jogo}"));
    let json = base.with_extension("json");
    fs::write(&json, serde_json::to_vec_pretty(r)?)?;

    // Separador ";" e vírgula decimal: abre direto no Excel em português.
    let mut csv = String::from("segundos;ram_disponivel_mb;confirmada_mb;paginacao_mb;ram_jogo_mb;cpu_jogo_pct\n");
    for a in amostras {
        csv.push_str(&format!(
            "{};{};{};{};{};{}\n",
            a.segundos.round(),
            a.ram_disponivel >> 20,
            a.confirmada >> 20,
            a.paginacao_em_uso >> 20,
            a.ram_jogo >> 20,
            format!("{:.1}", a.cpu_jogo_pct).replace('.', ",")
        ));
    }
    fs::write(base.with_extension("csv"), csv)?;
    Ok(json)
}

/// Relatórios gravados, do mais novo para o mais antigo.
pub fn listar(pasta: &Path) -> Vec<(PathBuf, RelatorioSessao)> {
    let Ok(leitura) = fs::read_dir(pasta) else { return Vec::new() };
    let mut lista: Vec<(PathBuf, RelatorioSessao)> = leitura
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .filter_map(|p| {
            let r = serde_json::from_slice(&fs::read(&p).ok()?).ok()?;
            Some((p, r))
        })
        .collect();
    lista.sort_by_key(|(_, r)| std::cmp::Reverse(r.inicio_unix));
    lista
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::disco::testes::PastaTeste;
    use crate::modo_jogo::protecao::testes::proc;

    fn memoria(disponivel_mb: u64) -> MemoriaSistema {
        MemoriaSistema {
            instalada: None,
            total: 16 << 30,
            disponivel: disponivel_mb << 20,
            em_uso: 0,
            carga_pct: 0,
            confirmada: 10 << 30,
            limite_confirmada: 32 << 30,
            listas: None,
            pool_paginado: 0,
            pool_nao_paginado: 0,
        }
    }

    #[test]
    fn junta_amostras_e_sugere_pelos_numeros() {
        let jogo_proc = proc(10, 1, "Jogo.exe", 10);
        let jogo = Jogo { pid: 10, criado_em: 10, nome: "Jogo.exe".into(), caminho: None, detectado_por: "lista de jogos".into() };
        let mut chrome = proc(20, 1, "chrome.exe", 20);
        chrome.ws_privado = 900 << 20;
        let mut chrome2 = proc(21, 1, "chrome.exe", 21);
        chrome2.ws_privado = 300 << 20;
        let explorer = proc(30, 1, "explorer.exe", 30);

        let mut a = Amostrador::novo(&jogo);
        for (i, disponivel) in [3000, 800, 1500].into_iter().enumerate() {
            let mut lista = vec![jogo_proc.clone(), chrome.clone(), chrome2.clone(), explorer.clone()];
            lista[0].ws_privado = (4000 + i as u64 * 100) << 20;
            a.amostrar(&lista, &memoria(disponivel), (100 + i as u64 * 200) << 20);
        }
        let lista = vec![jogo_proc, chrome, chrome2, explorer];
        let (r, amostras) = a.finalizar("observar", vec![], vec![], &lista, &[]);

        assert_eq!(amostras.len(), 3);
        assert_eq!(r.ram_disponivel_min, 800 << 20);
        assert_eq!(r.ram_disponivel_inicio, 3000 << 20);
        assert_eq!(r.paginacao_inicio, 100 << 20);
        assert_eq!(r.paginacao_max, 500 << 20);
        assert_eq!(r.ram_jogo_max, 4200 << 20);
        assert_eq!(r.concorrentes[0].nome, "chrome.exe");
        assert_eq!(r.concorrentes[0].ram_max, 1200 << 20);
        assert_eq!(r.concorrentes[0].processos, 2);
        assert!(!r.concorrentes.iter().any(|c| c.nome == "Jogo.exe"));
        assert!(r.concorrentes.iter().find(|c| c.nome == "explorer.exe").unwrap().protegido);
        assert!(r.sugestoes[0].starts_with("Faltou RAM"), "{:?}", r.sugestoes);
        assert!(r.sugestoes[0].contains("chrome.exe"));
        assert!(!r.sugestoes[0].contains("explorer.exe"));
        // Já está na lista `pausar`: não é sugerido de novo.
        assert!(!sugestoes(&r, &["CHROME.EXE".into()]).iter().any(|s| s.contains("chrome.exe")));

        let pasta = PastaTeste::nova("relatorio");
        let json = gravar(&pasta.0, &r, &amostras).unwrap();
        assert!(json.with_extension("csv").exists());
        let lidos = listar(&pasta.0);
        assert_eq!(lidos.len(), 1);
        assert_eq!(lidos[0].1.jogo, "Jogo.exe");
    }
}
