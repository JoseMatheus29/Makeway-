//! Makeway: a janela. Os comandos de terminal ficam no makeway-cli.exe.
//!
//! Uso: makeway [--config ARQUIVO] [--aba inicio|jogo|espaco|memoria|partidas]

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod componentes;
mod icones;
mod tarefa;
mod telas;
mod tema;

use std::path::PathBuf;

use eframe::egui;
use nucleo::{config, icone, plataforma};

pub const TITULO: &str = "Makeway";
const MUTEX: &str = r"Local\Makeway-Painel";

fn main() -> eframe::Result {
    // Uma janela só: abrir de novo (pelo ícone, por exemplo) traz a que já existe.
    let _instancia = match plataforma::instancia_unica(MUTEX) {
        Ok(None) => {
            plataforma::trazer_para_frente(TITULO);
            return Ok(());
        }
        Ok(Some(instancia)) => Some(instancia),
        Err(_) => None,
    };

    let mut caminho = config::caminho_padrao().unwrap_or_else(|| PathBuf::from("config.toml"));
    let mut tela = app::Tela::Inicio;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match (arg.as_str(), args.next()) {
            ("--config", Some(valor)) => caminho = PathBuf::from(valor),
            ("--aba", Some(valor)) => tela = app::Tela::do_nome(&valor).unwrap_or(tela),
            _ => {}
        }
    }

    let icone = egui::IconData { rgba: icone::rgba(64, icone::LIME), width: 64, height: 64 };
    let opcoes = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(TITULO)
            .with_inner_size([1240.0, 820.0])
            .with_min_inner_size([980.0, 600.0])
            .with_icon(icone),
        ..Default::default()
    };
    eframe::run_native(
        TITULO,
        opcoes,
        Box::new(move |cc| {
            tema::configurar(&cc.egui_ctx);
            Ok(Box::new(app::App::new(caminho, tela)))
        }),
    )
}
