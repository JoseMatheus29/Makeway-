//! O que os dois executáveis usam do Windows para existir como app:
//! instância única, loop de mensagens, saída educada, UAC.
//! A implementação (com `unsafe`) fica toda em `sistema`.

pub use crate::sistema::{
    InstanciaUnica, avisar_limpeza_feita, avisar_thread, criar_janela_de_encerramento,
    definir_despertador, e_administrador, encerrar_loop, id_da_thread, instancia_rodando,
    instancia_unica, loop_de_mensagens, pedido_de_saida, pedir_saida, trazer_para_frente,
    tratar_ctrl_c,
};

/// Abre o Explorer mostrando o arquivo selecionado (ou abre a pasta).
pub fn mostrar_no_explorer(caminho: &std::path::Path) {
    let mut comando = std::process::Command::new("explorer.exe");
    if caminho.is_dir() {
        comando.arg(caminho);
    } else {
        use std::os::windows::process::CommandExt;
        comando.raw_arg(format!("/select,\"{}\"", caminho.display()));
    }
    let _ = comando.spawn();
}

/// (livre, total) em bytes da unidade onde `caminho` está.
pub use crate::sistema::{espaco_no_disco, primeiro_nome};

/// Abre uma página de Configurações do Windows (ex.: "ms-settings:startupapps").
pub fn abrir_configuracoes(pagina: &str) {
    let _ = std::process::Command::new("explorer.exe").arg(pagina).spawn();
}
