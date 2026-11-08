//! Trabalho pesado (varrer o disco, ler processos) roda numa thread à
//! parte, para a janela nunca congelar. A aba pergunta a cada quadro se
//! já terminou.

use std::sync::mpsc;

use eframe::egui;

pub struct Tarefa<T> {
    rx: mpsc::Receiver<T>,
}

impl<T: Send + 'static> Tarefa<T> {
    pub fn iniciar(ctx: &egui::Context, trabalho: impl FnOnce() -> T + Send + 'static) -> Self {
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(trabalho());
            ctx.request_repaint();
        });
        Self { rx }
    }

    /// O resultado, se já ficou pronto.
    pub fn pronta(&self) -> Option<T> {
        self.rx.try_recv().ok()
    }
}

/// Recebe o resultado de `tarefa` em `destino` quando ficar pronto.
/// Devolve `true` enquanto ainda está rodando.
pub fn acompanhar<T: Send + 'static>(tarefa: &mut Option<Tarefa<T>>, destino: &mut Option<T>) -> bool {
    let Some(t) = tarefa else { return false };
    match t.pronta() {
        Some(resultado) => {
            *destino = Some(resultado);
            *tarefa = None;
            false
        }
        None => true,
    }
}
