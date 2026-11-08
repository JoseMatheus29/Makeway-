//! Vigia do Makeway (makeway-vigia.exe): fica perto do relógio, percebe
//! quando um jogo abre e cuida do modo jogo.
//!
//! Em release não tem janela de console; o registro vai para `vigia.log`.
//! Em debug o console continua, para acompanhar enquanto desenvolve.
//!
//! Uso: makeway-vigia [--config ARQUIVO]

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc;

use nucleo::config::{self, Modo};
use nucleo::formato::hora_agora;
use nucleo::icone;
use nucleo::modo_jogo::vigia::{self, StatusVigia};
use nucleo::plataforma;
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

/// Acima disso o registro antigo vira vigia.log.1 e um novo começa.
const LIMITE_DO_LOG: u64 = 1 << 20;

// Avisos mandados para o loop de mensagens da thread principal.
const AVISO_STATUS: u32 = 1;
const AVISO_MENU: u32 = 2;
const AVISO_ABRIR_PAINEL: u32 = 3;

fn main() -> ExitCode {
    let caminho = match caminho_do_config() {
        Ok(caminho) => caminho,
        Err(erro) => {
            eprintln!("makeway-vigia: {erro}");
            return ExitCode::FAILURE;
        }
    };
    // Uma instância só: duas tentando pausar e retomar os mesmos apps
    // se atrapalhariam.
    let _instancia = match plataforma::instancia_unica(vigia::NOME_DO_MUTEX) {
        Ok(Some(instancia)) => instancia,
        Ok(None) => {
            eprintln!("makeway-vigia: já tem um vigia rodando");
            return ExitCode::FAILURE;
        }
        Err(erro) => {
            eprintln!("makeway-vigia: não deu para criar o mutex: {erro}");
            return ExitCode::FAILURE;
        }
    };
    let pasta_dados = config::pasta_dos_dados_de(&caminho);
    let parar = plataforma::tratar_ctrl_c(); // só vale em debug, com console
    let (despertador, espera) = vigia::canal();
    {
        let despertador = despertador.clone();
        plataforma::definir_despertador(move || despertador.acordar());
    }

    // A fila de mensagens desta thread precisa existir antes de a outra
    // thread mandar avisos: a janela oculta e o ícone a criam.
    let principal = plataforma::id_da_thread();
    let _ = plataforma::criar_janela_de_encerramento();
    let (tx_menu, rx_menu) = mpsc::channel::<MenuId>();
    let bandeja = Bandeja::nova(principal, tx_menu);

    let (tx_status, rx_status) = mpsc::channel::<StatusVigia>();
    let trabalhador = {
        let caminho = caminho.clone();
        let despertador = despertador.clone();
        let pasta_dados = pasta_dados.clone();
        std::thread::spawn(move || {
            let mut arquivo_log = abrir_log(&pasta_dados);
            let mut log = |linha: &str| {
                let linha = format!("{} {linha}", hora_agora());
                println!("{linha}");
                if let Some(arquivo) = arquivo_log.as_mut() {
                    let _ = writeln!(arquivo, "{linha}");
                }
            };
            let mut status = |s: &StatusVigia| {
                let _ = tx_status.send(s.clone());
                plataforma::avisar_thread(principal, AVISO_STATUS);
            };
            vigia::rodar(&caminho, parar, espera, despertador, &mut log, &mut status);
            plataforma::avisar_limpeza_feita();
            plataforma::encerrar_loop(principal);
        })
    };

    plataforma::loop_de_mensagens(|aviso| match aviso {
        AVISO_STATUS => {
            if let (Some(b), Some(s)) = (&bandeja, rx_status.try_iter().last()) {
                b.mostrar(&s);
            }
        }
        AVISO_MENU => {
            for id in rx_menu.try_iter() {
                if let Some(b) = &bandeja {
                    b.ao_clicar(&id, &caminho, &pasta_dados, &despertador);
                }
            }
        }
        AVISO_ABRIR_PAINEL => abrir_painel(),
        _ => {}
    });
    let _ = trabalhador.join();
    drop(bandeja);
    ExitCode::SUCCESS
}

/// O ícone perto do relógio e o menu dele.
struct Bandeja {
    icone: TrayIcon,
    situacao: MenuItem,
    ligado: CheckMenuItem,
    otimizar: CheckMenuItem,
    painel: MenuItem,
    relatorios: MenuItem,
    sair: MenuItem,
}

impl Bandeja {
    /// `None` se o Windows não deixar criar o ícone; o vigia segue sem ele.
    fn nova(principal: u32, tx_menu: mpsc::Sender<MenuId>) -> Option<Self> {
        let situacao = MenuItem::new("Iniciando…", false, None);
        let ligado = CheckMenuItem::new("Modo jogo ligado", true, true, None);
        let otimizar = CheckMenuItem::new("Otimizar (desmarcado: só observa)", true, false, None);
        let painel = MenuItem::new("Abrir o Makeway", true, None);
        let relatorios = MenuItem::new("Abrir a pasta dos relatórios", true, None);
        let sair = MenuItem::new("Sair do Makeway", true, None);
        let menu = Menu::new();
        menu.append_items(&[
            &situacao,
            &PredefinedMenuItem::separator(),
            &ligado,
            &otimizar,
            &PredefinedMenuItem::separator(),
            &painel,
            &relatorios,
            &PredefinedMenuItem::separator(),
            &sair,
        ])
        .ok()?;

        MenuEvent::set_event_handler(Some(move |evento: MenuEvent| {
            let _ = tx_menu.send(evento.id);
            plataforma::avisar_thread(principal, AVISO_MENU);
        }));
        TrayIconEvent::set_event_handler(Some(move |evento: TrayIconEvent| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = evento {
                plataforma::avisar_thread(principal, AVISO_ABRIR_PAINEL);
            }
        }));

        let icone = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("Makeway")
            .with_icon(icone_de(icone::CIANO)?)
            .build()
            .ok()?;
        Some(Self { icone, situacao, ligado, otimizar, painel, relatorios, sair })
    }

    fn mostrar(&self, s: &StatusVigia) {
        let modo = match s.modo {
            Modo::Observar => "só observando",
            Modo::Otimizar => "otimizando",
        };
        let (texto, cor) = match (&s.jogo, s.ativo) {
            (_, false) => ("Modo jogo desligado".to_owned(), icone::CINZA),
            (Some(jogo), _) if s.modo == Modo::Otimizar => (format!("{jogo}: {modo}"), icone::LIME),
            (Some(jogo), _) => (format!("{jogo}: {modo}"), icone::AMBAR),
            (None, _) => (format!("Esperando um jogo abrir ({modo})"), icone::CIANO),
        };
        self.situacao.set_text(&texto);
        self.ligado.set_checked(s.ativo);
        self.otimizar.set_checked(s.modo == Modo::Otimizar);
        let dica: String = format!("Makeway: {texto}").chars().take(120).collect();
        let _ = self.icone.set_tooltip(Some(dica));
        let _ = self.icone.set_icon(icone_de(cor));
    }

    fn ao_clicar(&self, id: &MenuId, caminho: &Path, pasta_dados: &Path, despertador: &vigia::Despertador) {
        // O menu já marcou/desmarcou o item; o config segue o que ficou.
        let resultado = if id == self.ligado.id() {
            config::definir_ativo(caminho, self.ligado.is_checked()).map(|_| ())
        } else if id == self.otimizar.id() {
            let modo = if self.otimizar.is_checked() { Modo::Otimizar } else { Modo::Observar };
            config::definir_modo(caminho, modo).map(|_| ())
        } else if id == self.painel.id() {
            abrir_painel();
            Ok(())
        } else if id == self.relatorios.id() {
            let pasta = vigia::pasta_de_relatorios(pasta_dados);
            let _ = fs::create_dir_all(&pasta);
            plataforma::mostrar_no_explorer(&pasta);
            Ok(())
        } else if id == self.sair.id() {
            self.situacao.set_text("Saindo e desfazendo tudo…");
            plataforma::pedir_saida();
            Ok(())
        } else {
            Ok(())
        };
        match resultado {
            // O vigia relê o config na hora e manda o status novo.
            Ok(()) => despertador.acordar(),
            Err(erro) => {
                // Volta a marcação do item clicado: o config não mudou.
                for item in [&self.ligado, &self.otimizar] {
                    if id == item.id() {
                        item.set_checked(!item.is_checked());
                    }
                }
                self.situacao.set_text(format!("Erro no config: {erro}").chars().take(80).collect::<String>());
            }
        }
    }
}

fn icone_de(cor: [u8; 3]) -> Option<Icon> {
    Icon::from_rgba(icone::rgba(32, cor), 32, 32).ok()
}

/// Abre o painel (makeway.exe) que fica na mesma pasta. Se ele já estiver
/// aberto, ele mesmo só vem para a frente.
fn abrir_painel() {
    if let Ok(eu) = std::env::current_exe() {
        let _ = std::process::Command::new(eu.with_file_name("makeway.exe")).spawn();
    }
}

fn caminho_do_config() -> Result<PathBuf, String> {
    let mut args = std::env::args().skip(1);
    match (args.next().as_deref(), args.next()) {
        (None, _) => config::caminho_padrao().ok_or_else(|| "a variável APPDATA não existe".into()),
        (Some("--config"), Some(caminho)) => Ok(PathBuf::from(caminho)),
        _ => Err("uso: makeway-vigia [--config ARQUIVO]".into()),
    }
}

fn abrir_log(pasta: &Path) -> Option<File> {
    fs::create_dir_all(pasta).ok()?;
    let caminho = pasta.join("vigia.log");
    if fs::metadata(&caminho).is_ok_and(|m| m.len() > LIMITE_DO_LOG) {
        let _ = fs::rename(&caminho, pasta.join("vigia.log.1"));
    }
    let mut arquivo = OpenOptions::new().create(true).append(true).open(caminho).ok()?;
    // BOM no arquivo novo: o PowerShell 5.1 e editores antigos leem os acentos certo.
    if arquivo.metadata().is_ok_and(|m| m.len() == 0) {
        let _ = arquivo.write_all("\u{feff}".as_bytes());
    }
    Some(arquivo)
}
