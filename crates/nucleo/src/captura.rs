//! A gravação em segundo plano do Xbox Game Bar ("Gravar o que aconteceu").
//! Ligada, ela grava os últimos minutos sem parar enquanto você joga: usa o
//! codificador de vídeo e memória, que no vídeo integrado são divididos com
//! o próprio jogo. Só leitura: quem desliga é o usuário, nas Configurações.

use winreg::RegKey;
use winreg::enums::HKEY_CURRENT_USER;

/// Página das Configurações onde fica a opção (Jogos > Capturas).
pub const PAGINA_DAS_CAPTURAS: &str = "ms-settings:gaming-gamedvr";

/// `Some(true)` se a gravação em segundo plano está ligada para esta conta.
/// `None` se não deu para ler.
pub fn gravacao_em_segundo_plano() -> Option<bool> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let valor = |caminho: &str, nome: &str| -> Option<u32> { hkcu.open_subkey(caminho).ok()?.get_value(nome).ok() };
    let dvr = r"Software\Microsoft\Windows\CurrentVersion\GameDVR";
    Some(gravacao_ligada(
        valor(dvr, "HistoricalCaptureEnabled"),
        valor(dvr, "AppCaptureEnabled"),
        valor(r"System\GameConfigStore", "GameDVR_Enabled"),
    ))
}

/// Ligada quando "Gravar o que aconteceu" está marcado e as capturas não
/// foram desligadas por inteiro. Sem o valor, vale o padrão do Windows:
/// gravação desligada, capturas ligadas.
pub fn gravacao_ligada(historico: Option<u32>, capturas: Option<u32>, game_dvr: Option<u32>) -> bool {
    historico == Some(1) && capturas != Some(0) && game_dvr != Some(0)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn so_conta_como_ligada_com_tudo_ligado() {
        assert!(gravacao_ligada(Some(1), Some(1), Some(1)));
        assert!(gravacao_ligada(Some(1), None, None));
        assert!(!gravacao_ligada(None, Some(1), Some(1)));
        assert!(!gravacao_ligada(Some(0), Some(1), Some(1)));
        assert!(!gravacao_ligada(Some(1), Some(0), Some(1)));
        assert!(!gravacao_ligada(Some(1), Some(1), Some(0)));
    }

    #[test]
    fn le_a_configuracao_desta_conta() {
        assert!(gravacao_em_segundo_plano().is_some());
    }
}
