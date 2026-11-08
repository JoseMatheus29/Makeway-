//! Abrir o vigia junto com o Windows, com administrador e sem o aviso do
//! UAC a cada boot: uma tarefa no Agendador de Tarefas com "executar com
//! privilégios mais altos". Criar ou apagar a tarefa pede o UAC uma vez.
//!
//! A tarefa é criada por XML, e não só com parâmetros do schtasks, porque
//! o padrão dele mata a tarefa depois de 72 horas e não deixa ela começar
//! na bateria.

use std::fs;
use std::io;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Command, Output};

use crate::sistema;

pub const NOME_DA_TAREFA: &str = r"Makeway\Vigia";
const SEM_JANELA: u32 = 0x0800_0000; // CREATE_NO_WINDOW

fn schtasks(argumentos: &[&str]) -> io::Result<Output> {
    Command::new("schtasks.exe").args(argumentos).creation_flags(SEM_JANELA).output()
}

/// A tarefa existe no Agendador.
pub fn ativo() -> bool {
    existe(NOME_DA_TAREFA)
}

fn existe(nome: &str) -> bool {
    schtasks(&["/Query", "/TN", nome]).is_ok_and(|s| s.status.success())
}

/// Cria (ou recria) a tarefa que abre `vigia` quando você entra no Windows.
pub fn ativar(vigia: &Path) -> io::Result<()> {
    registrar(NOME_DA_TAREFA, &xml(vigia, &usuario_atual(), true))
}

pub fn desativar() -> io::Result<()> {
    if !ativo() {
        return Ok(());
    }
    rodar_schtasks(&format!("/Delete /TN \"{NOME_DA_TAREFA}\" /F"), true)
}

/// Abre o vigia agora pela tarefa (já com administrador, sem UAC).
pub fn iniciar_agora() -> io::Result<()> {
    rodar_schtasks(&format!("/Run /TN \"{NOME_DA_TAREFA}\""), false)
}

fn usuario_atual() -> String {
    let usuario = std::env::var("USERNAME").unwrap_or_default();
    match std::env::var("USERDOMAIN") {
        Ok(dominio) if !dominio.is_empty() => format!(r"{dominio}\{usuario}"),
        _ => usuario,
    }
}

fn registrar(nome: &str, xml: &str) -> io::Result<()> {
    let arquivo = std::env::temp_dir().join(format!("makeway-tarefa-{}.xml", std::process::id()));
    // O schtasks lê o XML em UTF-16 com BOM.
    let mut bytes = vec![0xFF, 0xFE];
    for unidade in xml.encode_utf16() {
        bytes.extend_from_slice(&unidade.to_le_bytes());
    }
    fs::write(&arquivo, bytes)?;
    let alta = xml.contains("<RunLevel>HighestAvailable</RunLevel>");
    let resultado = rodar_schtasks(&format!("/Create /TN \"{nome}\" /XML \"{}\" /F", arquivo.display()), alta);
    let _ = fs::remove_file(&arquivo);
    resultado
}

/// Roda o schtasks; com `com_administrador`, pede o UAC se ainda não for admin.
fn rodar_schtasks(argumentos: &str, com_administrador: bool) -> io::Result<()> {
    let codigo = if com_administrador && !sistema::e_administrador() {
        sistema::executar_como_admin("schtasks.exe", argumentos)?
    } else {
        let saida = Command::new("schtasks.exe").raw_arg(argumentos).creation_flags(SEM_JANELA).output()?;
        if !saida.status.success() {
            let motivo = String::from_utf8_lossy(&saida.stderr).trim().to_owned();
            return Err(io::Error::other(format!("o Agendador de Tarefas recusou: {motivo}")));
        }
        0
    };
    if codigo != 0 {
        return Err(io::Error::other(format!("o Agendador de Tarefas recusou (código {codigo})")));
    }
    Ok(())
}

/// XML da tarefa. `com_administrador` = "executar com privilégios mais altos".
pub fn xml(vigia: &Path, usuario: &str, com_administrador: bool) -> String {
    let nivel = if com_administrador { "HighestAvailable" } else { "LeastPrivilege" };
    let usuario = escapar(usuario);
    let comando = escapar(&vigia.display().to_string());
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Description>Makeway: vigia do modo jogo. Abre quando você entra no Windows.</Description>
  </RegistrationInfo>
  <Triggers>
    <LogonTrigger>
      <Enabled>true</Enabled>
      <UserId>{usuario}</UserId>
      <Delay>PT10S</Delay>
    </LogonTrigger>
  </Triggers>
  <Principals>
    <Principal id="Usuario">
      <UserId>{usuario}</UserId>
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>{nivel}</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowHardTerminate>true</AllowHardTerminate>
    <StartWhenAvailable>false</StartWhenAvailable>
    <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>
    <IdleSettings>
      <StopOnIdleEnd>false</StopOnIdleEnd>
      <RestartOnIdle>false</RestartOnIdle>
    </IdleSettings>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <Enabled>true</Enabled>
    <Hidden>false</Hidden>
    <RunOnlyIfIdle>false</RunOnlyIfIdle>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Priority>5</Priority>
  </Settings>
  <Actions Context="Usuario">
    <Exec>
      <Command>"{comando}"</Command>
    </Exec>
  </Actions>
</Task>
"#
    )
}

fn escapar(texto: &str) -> String {
    texto
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn xml_sem_limite_de_tempo_e_na_bateria() {
        let x = xml(Path::new(r"C:\Jogos & Cia\makeway-vigia.exe"), r"PC\Fulano", true);
        assert!(x.contains("<ExecutionTimeLimit>PT0S</ExecutionTimeLimit>"));
        assert!(x.contains("<DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>"));
        assert!(x.contains("<RunLevel>HighestAvailable</RunLevel>"));
        assert!(x.contains(r#"<Command>"C:\Jogos &amp; Cia\makeway-vigia.exe"</Command>"#));
    }

    /// Cria e apaga uma tarefa de teste de verdade (sem administrador, por
    /// isso com privilégio normal). `cargo test -p nucleo -- --ignored tarefa`
    #[test]
    #[ignore]
    fn tarefa_de_teste_e_aceita_pelo_agendador() {
        let nome = r"Makeway\Teste";
        let x = xml(Path::new(r"C:\Windows\System32\notepad.exe"), &usuario_atual(), false);
        registrar(nome, &x).unwrap();
        assert!(existe(nome));
        rodar_schtasks(&format!("/Delete /TN \"{nome}\" /F"), false).unwrap();
        assert!(!existe(nome));
    }
}
