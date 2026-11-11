//! Núcleo do Makeway, compartilhado pelo vigia e pelo painel.

#[cfg(not(windows))]
compile_error!("o Makeway só funciona no Windows");

pub mod config;
pub mod disco;
pub mod duplicados;
pub mod formato;
pub mod icone;
pub mod inicializacao;
pub mod inicio_automatico;
pub mod jogos;
pub mod modo_jogo;
pub mod nomes;
pub mod paginacao;
pub mod ram;
pub mod plataforma;
mod sistema;
pub mod temporarios;
