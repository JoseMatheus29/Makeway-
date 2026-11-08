//! Aviso do Windows a cada processo aberto (WMI, `Win32_ProcessStartTrace`).
//! Com ele o vigia não precisa olhar a lista de processos a cada 3 s fora
//! do jogo. Só funciona com administrador; sem ele, o vigia continua
//! procurando o jogo em intervalos.

use std::sync::mpsc;
use std::thread;

use serde::Deserialize;
use wmi::WMIConnection;

use super::vigia::Despertador;

#[derive(Deserialize)]
#[serde(rename = "Win32_ProcessStartTrace")]
struct ProcessoAberto {
    #[serde(rename = "SessionID")]
    sessao: u32,
}

/// Assina os avisos numa thread própria. `Err` com o motivo se o Windows
/// recusar (o normal sem administrador).
pub fn assinar(despertador: Despertador) -> Result<(), String> {
    let (pronto, resposta) = mpsc::channel();
    thread::Builder::new()
        .name("eventos-wmi".into())
        .spawn(move || {
            let conexao = match WMIConnection::new() {
                Ok(conexao) => conexao,
                Err(erro) => {
                    let _ = pronto.send(Err(erro.to_string()));
                    return;
                }
            };
            let eventos = match conexao.raw_notification::<ProcessoAberto>("SELECT SessionID FROM Win32_ProcessStartTrace") {
                Ok(eventos) => eventos,
                Err(erro) => {
                    let _ = pronto.send(Err(erro.to_string()));
                    return;
                }
            };
            let _ = pronto.send(Ok(()));
            // Serviços (sessão 0) abrem e fecham o tempo todo e nunca são o jogo.
            for evento in eventos.flatten() {
                if evento.sessao != 0 {
                    despertador.processo_abriu();
                }
            }
        })
        .map_err(|e| e.to_string())?;
    resposta.recv().unwrap_or_else(|_| Err("a thread de eventos parou".into()))
}
