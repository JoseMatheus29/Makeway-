//! Modo jogo: detecta o jogo, tira do caminho o que compete com ele e
//! desfaz tudo quando ele fecha, mesmo se o programa travar.

pub mod deteccao;
pub mod protecao;
pub mod relatorio;
pub mod sessao;
pub mod vigia;

pub use crate::sistema::{InstanciaUnica, avisar_limpeza_feita, instancia_unica, tratar_ctrl_c};
