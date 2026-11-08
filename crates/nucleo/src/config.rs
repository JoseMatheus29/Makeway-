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

#[derive(Debug)]
pub enum ErroConfig {
    Ler { caminho: PathBuf, erro: io::Error },
    Formato { caminho: PathBuf, erro: Box<toml::de::Error> },
}

impl fmt::Display for ErroConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ler { caminho, erro } => {
                write!(f, "não deu para ler {}: {erro}", caminho.display())
            }
            Self::Formato { caminho, erro } => {
                write!(f, "config inválido em {}:\n{erro}", caminho.display())
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
        erro: Box::new(erro),
    })
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
