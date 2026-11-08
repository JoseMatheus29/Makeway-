//! Núcleo do Makeway, compartilhado pelo vigia e pelo painel.

#[cfg(not(windows))]
compile_error!("o Makeway só funciona no Windows");

pub mod config;
pub mod disco;
pub mod duplicados;
pub mod formato;
pub mod inicializacao;
pub mod modo_jogo;
pub mod ram;
mod sistema;
pub mod temporarios;
