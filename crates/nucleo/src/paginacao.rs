//! O limite de memória do Windows (RAM + arquivo de paginação) e como o
//! arquivo de paginação está configurado. Só leitura: o Makeway nunca muda
//! essa configuração, só explica o que fazer.
//!
//! Quando a memória confirmada chega no limite, o Windows não consegue dar
//! mais memória a ninguém e o jogo pode fechar sozinho com erro de "sem
//! memória", mesmo com RAM livre aparecendo no Gerenciador de Tarefas.

use winreg::RegKey;
use winreg::enums::HKEY_LOCAL_MACHINE;

/// Como o arquivo de paginação está configurado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Paginacao {
    /// "Gerenciar automaticamente" (o padrão do Windows): ele cresce sozinho.
    Automatica,
    /// Tamanho gerenciado pelo sistema em pelo menos uma unidade: também cresce.
    GerenciadaPeloSistema,
    /// Tamanho fixo escolhido à mão; `maximo` soma todas as unidades, em bytes.
    Fixa { maximo: u64 },
    /// Sem arquivo de paginação: o limite é só a RAM.
    Desligada,
}

impl Paginacao {
    /// O Windows aumenta o arquivo sozinho quando a memória aperta.
    pub fn cresce_sozinha(self) -> bool {
        matches!(self, Self::Automatica | Self::GerenciadaPeloSistema)
    }
}

/// Fração do limite a partir da qual a memória está "perto do limite".
pub const PERTO_DO_LIMITE: f64 = 0.85;

/// A memória confirmada chegou a 85% do limite ou mais.
pub fn perto_do_limite(confirmada: u64, limite: u64) -> bool {
    limite > 0 && confirmada as f64 >= limite as f64 * PERTO_DO_LIMITE
}

/// Lê a configuração no registro (não precisa de administrador).
/// `None` se não deu para ler.
pub fn configuracao() -> Option<Paginacao> {
    let chave = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey(r"SYSTEM\CurrentControlSet\Control\Session Manager\Memory Management")
        .ok()?;
    let linhas: Vec<String> = chave.get_value("PagingFiles").unwrap_or_default();
    Some(interpretar(&linhas))
}

/// Interpreta as linhas de `PagingFiles`:
/// - `?:\pagefile.sys`: automático em todas as unidades;
/// - `C:\pagefile.sys 0 0` ou sem números: tamanho gerenciado pelo sistema;
/// - `C:\pagefile.sys 1024 4096`: tamanho fixo (mínimo e máximo em MB).
pub fn interpretar(linhas: &[String]) -> Paginacao {
    let linhas: Vec<&str> = linhas.iter().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
    if linhas.is_empty() {
        return Paginacao::Desligada;
    }
    if linhas.iter().any(|l| l.starts_with("?:")) {
        return Paginacao::Automatica;
    }
    let mut maximo_mb = 0u64;
    for linha in &linhas {
        let partes: Vec<&str> = linha.split_whitespace().collect();
        let tamanhos = match partes.as_slice() {
            [.., minimo, maximo] if partes.len() >= 3 => (minimo.parse::<u64>(), maximo.parse::<u64>()),
            _ => return Paginacao::GerenciadaPeloSistema,
        };
        match tamanhos {
            (Ok(_), Ok(maximo)) if maximo > 0 => maximo_mb += maximo,
            _ => return Paginacao::GerenciadaPeloSistema,
        }
    }
    Paginacao::Fixa { maximo: maximo_mb << 20 }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn linhas(texto: &[&str]) -> Vec<String> {
        texto.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn entende_as_configuracoes_do_windows() {
        assert_eq!(interpretar(&linhas(&[r"?:\pagefile.sys"])), Paginacao::Automatica);
        assert_eq!(interpretar(&linhas(&[r"C:\pagefile.sys 0 0"])), Paginacao::GerenciadaPeloSistema);
        assert_eq!(interpretar(&linhas(&[r"C:\pagefile.sys"])), Paginacao::GerenciadaPeloSistema);
        assert_eq!(
            interpretar(&linhas(&[r"C:\pagefile.sys 1024 4096", r"D:\pagefile.sys 2048 2048"])),
            Paginacao::Fixa { maximo: 6144 << 20 }
        );
        // Uma unidade fixa e outra gerenciada: ainda cresce.
        assert_eq!(
            interpretar(&linhas(&[r"C:\pagefile.sys 1024 4096", r"D:\pagefile.sys 0 0"])),
            Paginacao::GerenciadaPeloSistema
        );
        assert_eq!(interpretar(&linhas(&[""])), Paginacao::Desligada);
        assert_eq!(interpretar(&[]), Paginacao::Desligada);
    }

    #[test]
    fn perto_do_limite_a_partir_de_85_por_cento() {
        assert!(!perto_do_limite(84, 100));
        assert!(perto_do_limite(85, 100));
        assert!(!perto_do_limite(10, 0));
    }

    #[test]
    fn le_a_configuracao_deste_pc() {
        assert!(configuracao().is_some());
    }
}
