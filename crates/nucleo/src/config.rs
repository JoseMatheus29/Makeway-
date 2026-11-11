//! Leitura do `config.toml`, compartilhado pelo vigia e pelo painel.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Modelo do arquivo, com comentários. Gravado na primeira vez que o
/// usuário pede (`painel config`). Um teste garante que ele bate com
/// os valores de [`Config::default`].
pub const MODELO: &str = include_str!("../config.padrao.toml");

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub ram: ConfigRam,
    pub disco: ConfigDisco,
    pub temporarios: ConfigTemporarios,
    pub modo_jogo: ConfigModoJogo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Modo {
    /// Só grava o relatório da partida.
    Observar,
    /// Aplica as mudanças e desfaz no fim.
    Otimizar,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Prioridade {
    Normal,
    AcimaDoNormal,
    Alta,
}

impl Prioridade {
    /// Valor da classe de prioridade do Windows.
    pub fn classe(self) -> u32 {
        match self {
            Self::Normal => 0x20,
            Self::AcimaDoNormal => 0x8000,
            Self::Alta => 0x80,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ConfigModoJogo {
    pub ativo: bool,
    pub modo: Modo,
    pub jogos: Vec<String>,
    pub detectar_pelas_pastas: bool,
    pub pastas_de_jogos: Vec<String>,
    pub ignorar: Vec<String>,
    pub intervalo_segundos: u64,
    pub prioridade: Prioridade,
    pub plano_alto_desempenho: bool,
    pub plano_somente_na_tomada: bool,
    pub memoria_baixa_nos_apps: bool,
    pub pausar: Vec<String>,
    pub fechar: Vec<String>,
}

impl Default for ConfigModoJogo {
    fn default() -> Self {
        Self {
            ativo: true,
            modo: Modo::Observar,
            jogos: Vec::new(),
            detectar_pelas_pastas: true,
            pastas_de_jogos: [
                r"\steamapps\common\",
                r"\Epic Games\",
                r"\Riot Games\",
                r"\XboxGames\",
                r"\Ubisoft Game Launcher\games\",
                r"\EA Games\",
                r"\GOG Galaxy\Games\",
            ]
            .map(String::from)
            .to_vec(),
            ignorar: Vec::new(),
            intervalo_segundos: 3,
            prioridade: Prioridade::Alta,
            plano_alto_desempenho: true,
            plano_somente_na_tomada: true,
            memoria_baixa_nos_apps: true,
            pausar: Vec::new(),
            fechar: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ConfigDisco {
    pub profundidade: usize,
    pub top: usize,
    pub duplicados_min_mb: u64,
}

impl Default for ConfigDisco {
    fn default() -> Self {
        Self { profundidade: 2, top: 10, duplicados_min_mb: 1 }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ConfigTemporarios {
    pub idade_minima_dias: u64,
}

impl Default for ConfigTemporarios {
    fn default() -> Self {
        Self { idade_minima_dias: 2 }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ConfigRam {
    /// Quantos apps listar no relatório de RAM.
    pub top_apps: usize,
}

impl Default for ConfigRam {
    fn default() -> Self {
        Self { top_apps: 15 }
    }
}

impl Modo {
    pub fn texto(self) -> &'static str {
        match self {
            Self::Observar => "observar",
            Self::Otimizar => "otimizar",
        }
    }
}

impl Prioridade {
    pub fn texto(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::AcimaDoNormal => "acima_do_normal",
            Self::Alta => "alta",
        }
    }
}

#[derive(Debug)]
pub enum ErroConfig {
    Ler { caminho: PathBuf, erro: io::Error },
    Formato { caminho: PathBuf, mensagem: String },
    Gravar { caminho: PathBuf, erro: io::Error },
}

impl fmt::Display for ErroConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ler { caminho, erro } => {
                write!(f, "não deu para ler {}: {erro}", caminho.display())
            }
            Self::Formato { caminho, mensagem } => {
                write!(f, "config inválido em {}:\n{mensagem}", caminho.display())
            }
            Self::Gravar { caminho, erro } => {
                write!(f, "não deu para gravar {}: {erro}", caminho.display())
            }
        }
    }
}

impl std::error::Error for ErroConfig {}

/// `%APPDATA%\Makeway`: config, estado do modo jogo e relatórios.
/// `None` se a variável `APPDATA` não existir.
pub fn pasta_dados() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    Some(PathBuf::from(appdata).join("Makeway"))
}

/// `%APPDATA%\Makeway\config.toml`.
pub fn caminho_padrao() -> Option<PathBuf> {
    Some(pasta_dados()?.join("config.toml"))
}

/// Pasta onde ficam o estado do modo jogo e os relatórios: a mesma do
/// config.toml. Assim um `--config` de teste não mexe nos dados reais.
pub fn pasta_dos_dados_de(caminho_config: &Path) -> PathBuf {
    caminho_config.parent().map(Path::to_path_buf).unwrap_or_default()
}

/// Lê o arquivo. Se ele não existir, devolve os valores padrão.
pub fn carregar(caminho: &Path) -> Result<Config, ErroConfig> {
    let texto = match std::fs::read_to_string(caminho) {
        Ok(texto) => texto,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Config::default()),
        Err(erro) => {
            return Err(ErroConfig::Ler { caminho: caminho.to_owned(), erro });
        }
    };
    toml::from_str(&texto).map_err(|erro| ErroConfig::Formato {
        caminho: caminho.to_owned(),
        mensagem: erro.to_string(),
    })
}

/// Muda uma seção do arquivo sem perder os comentários nem a ordem do
/// resto. Se o arquivo não existir, parte do modelo comentado. Só grava
/// se o resultado continuar válido, e devolve o config novo.
pub fn alterar_secao(
    caminho: &Path,
    secao: &str,
    mudar: impl FnOnce(&mut toml_edit::Table),
) -> Result<Config, ErroConfig> {
    let formato = |mensagem: String| ErroConfig::Formato { caminho: caminho.to_owned(), mensagem };
    let texto = match std::fs::read_to_string(caminho) {
        Ok(texto) => texto,
        Err(e) if e.kind() == io::ErrorKind::NotFound => MODELO.to_owned(),
        Err(erro) => return Err(ErroConfig::Ler { caminho: caminho.to_owned(), erro }),
    };
    let mut doc: toml_edit::DocumentMut = texto.parse().map_err(|e: toml_edit::TomlError| formato(e.to_string()))?;
    let tabela = doc
        .entry(secao)
        .or_insert_with(toml_edit::table)
        .as_table_mut()
        .ok_or_else(|| formato(format!("[{secao}] precisa ser uma seção")))?;
    mudar(tabela);
    let novo = doc.to_string();
    let config: Config = toml::from_str(&novo).map_err(|e| formato(e.to_string()))?;
    let gravar = |erro| ErroConfig::Gravar { caminho: caminho.to_owned(), erro };
    if let Some(pasta) = caminho.parent() {
        std::fs::create_dir_all(pasta).map_err(gravar)?;
    }
    std::fs::write(caminho, novo).map_err(gravar)?;
    Ok(config)
}

/// Grava todas as opções do modo jogo (usado pelo painel).
pub fn salvar_modo_jogo(caminho: &Path, m: &ConfigModoJogo) -> Result<Config, ErroConfig> {
    use toml_edit::value;
    alterar_secao(caminho, "modo_jogo", |t| {
        t["ativo"] = value(m.ativo);
        t["modo"] = value(m.modo.texto());
        t["jogos"] = lista(&m.jogos);
        t["detectar_pelas_pastas"] = value(m.detectar_pelas_pastas);
        t["pastas_de_jogos"] = lista(&m.pastas_de_jogos);
        t["ignorar"] = lista(&m.ignorar);
        t["intervalo_segundos"] = value(m.intervalo_segundos as i64);
        t["prioridade"] = value(m.prioridade.texto());
        t["plano_alto_desempenho"] = value(m.plano_alto_desempenho);
        t["plano_somente_na_tomada"] = value(m.plano_somente_na_tomada);
        t["memoria_baixa_nos_apps"] = value(m.memoria_baixa_nos_apps);
        t["pausar"] = lista(&m.pausar);
        t["fechar"] = lista(&m.fechar);
    })
}

/// Liga ou desliga o modo jogo (usado pelo menu do ícone).
pub fn definir_ativo(caminho: &Path, ativo: bool) -> Result<Config, ErroConfig> {
    alterar_secao(caminho, "modo_jogo", |t| t["ativo"] = toml_edit::value(ativo))
}

/// Troca entre observar e otimizar (usado pelo menu do ícone).
pub fn definir_modo(caminho: &Path, modo: Modo) -> Result<Config, ErroConfig> {
    alterar_secao(caminho, "modo_jogo", |t| t["modo"] = toml_edit::value(modo.texto()))
}

/// Lista TOML; com mais de 3 itens, um por linha.
fn lista(itens: &[String]) -> toml_edit::Item {
    let mut array = toml_edit::Array::new();
    for item in itens {
        array.push(item.as_str());
    }
    if itens.len() > 3 {
        for valor in array.iter_mut() {
            valor.decor_mut().set_prefix("\n    ");
        }
        array.set_trailing("\n");
        array.set_trailing_comma(true);
    }
    toml_edit::value(array)
}

/// Grava o modelo comentado se o arquivo ainda não existir.
/// Devolve `true` se criou o arquivo agora.
pub fn criar_se_faltar(caminho: &Path) -> io::Result<bool> {
    if caminho.exists() {
        return Ok(false);
    }
    if let Some(pasta) = caminho.parent() {
        std::fs::create_dir_all(pasta)?;
    }
    std::fs::write(caminho, MODELO)?;
    Ok(true)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn modelo_bate_com_os_padroes() {
        let lido: Config = toml::from_str(MODELO).unwrap();
        assert_eq!(lido, Config::default());
    }

    #[test]
    fn arquivo_vazio_usa_padroes() {
        let lido: Config = toml::from_str("").unwrap();
        assert_eq!(lido, Config::default());
    }

    #[test]
    fn alterar_mantem_comentarios_e_valida() {
        let pasta = crate::disco::testes::PastaTeste::nova("config");
        let caminho = pasta.0.join("config.toml");

        // Sem arquivo: parte do modelo comentado.
        let cfg = definir_modo(&caminho, Modo::Otimizar).unwrap();
        assert_eq!(cfg.modo_jogo.modo, Modo::Otimizar);
        let texto = std::fs::read_to_string(&caminho).unwrap();
        assert!(texto.contains("# Configuração do Makeway."));
        assert!(texto.contains("# Comece observando umas 3 partidas"));
        assert!(texto.contains("modo = \"otimizar\""));

        let mut m = cfg.modo_jogo.clone();
        m.jogos = vec!["VALORANT-Win64-Shipping.exe".into()];
        m.pausar = ["a.exe", "b.exe", "c.exe", "d.exe"].map(String::from).to_vec();
        m.prioridade = Prioridade::AcimaDoNormal;
        let cfg = salvar_modo_jogo(&caminho, &m).unwrap();
        assert_eq!(cfg.modo_jogo, m);
        assert_eq!(carregar(&caminho).unwrap().modo_jogo, m);
        let texto = std::fs::read_to_string(&caminho).unwrap();
        assert!(texto.contains("# Um app pausado não faz nada"));
        assert!(texto.contains("pausar = [\n    \"a.exe\""));
        // Caminhos com barra invertida continuam legíveis.
        assert!(texto.contains(r"'\steamapps\common\'"), "{texto}");

        assert!(!definir_ativo(&caminho, false).unwrap().modo_jogo.ativo);
    }

    #[test]
    fn opcao_parcial_mantem_o_resto() {
        let lido: Config = toml::from_str("[ram]\ntop_apps = 5\n").unwrap();
        assert_eq!(lido.ram.top_apps, 5);
    }

    #[test]
    fn nome_errado_e_recusado() {
        let erro = toml::from_str::<Config>("[ram]\ntop_app = 5\n").unwrap_err();
        assert!(erro.to_string().contains("top_app"));
    }

    #[test]
    fn arquivo_ausente_usa_padroes() {
        let caminho = std::env::temp_dir().join("otimizador-teste-nao-existe.toml");
        assert_eq!(carregar(&caminho).unwrap(), Config::default());
    }
}
