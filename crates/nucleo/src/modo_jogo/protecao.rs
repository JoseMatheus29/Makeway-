//! Processos que o modo jogo nunca pausa nem fecha, mesmo que estejam
//! nas listas do config.

use std::collections::{HashMap, HashSet};

use crate::ram::Processo;

/// Partes do Windows. Pausar qualquer um deles pode travar a tela, o som
/// ou a entrada.
const SISTEMA: &[&str] = &[
    "system", "registry", "memory compression", "secure system", "smss.exe", "csrss.exe",
    "wininit.exe", "winlogon.exe", "services.exe", "lsass.exe", "lsaiso.exe", "svchost.exe",
    "fontdrvhost.exe", "dwm.exe", "explorer.exe", "sihost.exe", "ctfmon.exe", "taskhostw.exe",
    "audiodg.exe", "conhost.exe", "openconsole.exe", "runtimebroker.exe", "dllhost.exe",
    "shellexperiencehost.exe", "startmenuexperiencehost.exe", "shellhost.exe", "searchhost.exe",
    "searchindexer.exe", "textinputhost.exe", "applicationframehost.exe", "lockapp.exe",
    "userinit.exe", "smartscreen.exe", "spoolsv.exe", "wudfhost.exe", "msmpeng.exe", "nissrv.exe",
    "mpdefendercoreservice.exe", "securityhealthservice.exe", "securityhealthsystray.exe",
    "wmiprvse.exe", "makeway.exe", "makeway-vigia.exe",
];

/// Anti-cheats. Mexer neles pode render banimento ou derrubar o jogo.
const ANTI_CHEATS: &[&str] = &[
    "vgc.exe", "vgtray.exe", "vgk.sys", "easyanticheat.exe", "easyanticheat_eos.exe",
    "beservice.exe", "beservice_x64.exe", "battleye.exe", "faceit.exe", "faceitservice.exe",
    "faceitclient.exe", "eaanticheat.gameservice.exe", "eaanticheat.gameservicelauncher.exe",
    "gamemon.exe", "gamemon64.exe", "gameguard.des", "pnkbstra.exe", "pnkbstrb.exe",
    "xigncode.exe", "nprotect.exe", "ricochet.exe", "anticheatexpert.exe", "ace-tray.exe",
    "sguard64.exe", "mhyprot.exe", "esportal.exe", "esea.exe", "eseaclient.exe",
];

/// Launchers e ajudantes de jogos: nunca são "o jogo".
pub const LAUNCHERS: &[&str] = &[
    "steam.exe", "steamwebhelper.exe", "steamservice.exe", "steamerrorreporter.exe",
    "steamerrorreporter64.exe", "gameoverlayui.exe", "gameoverlayui64.exe",
    "epicgameslauncher.exe", "epicwebhelper.exe", "epiconlineservices.exe",
    "epiconlineserviceshost.exe", "epiconlineservicesuserhelper.exe",
    "eosoverlayrenderer-win64-shipping.exe", "riotclientservices.exe", "riotclientux.exe",
    "riotclientuxrender.exe", "riotclientcrashhandler.exe", "leagueclient.exe",
    "leagueclientux.exe", "leagueclientuxrender.exe", "leaguecrashhandler64.exe", "upc.exe",
    "ubisoftconnect.exe", "uplaywebcore.exe", "ubisoftgamelauncher.exe", "eadesktop.exe",
    "eabackgroundservice.exe", "eaconnect_microsoft.exe", "battle.net.exe", "galaxyclient.exe",
    "galaxyclient helper.exe", "xboxpcapp.exe", "gamingservices.exe", "gamingservicesnet.exe",
    "gamebar.exe", "gamebarftserver.exe", "unitycrashhandler64.exe", "unitycrashhandler32.exe",
    "crashreportclient.exe", "unrealcefsubprocess.exe", "start_protected_game.exe",
    "easyanticheat_eos_setup.exe", "easyanticheat_setup.exe", "ue4prereqsetup_x64.exe",
    "vc_redist.x64.exe", "vc_redist.x86.exe", "dxsetup.exe",
];

/// Por que um processo é protegido.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motivo {
    Windows,
    Servico,
    AntiCheat,
    Jogo,
    LauncherDoJogo,
    ProcessoDoJogo,
}

impl Motivo {
    pub fn descricao(self) -> &'static str {
        match self {
            Self::Windows => "faz parte do Windows",
            Self::Servico => "é um serviço do Windows",
            Self::AntiCheat => "é um anti-cheat",
            Self::Jogo => "é o jogo",
            Self::LauncherDoJogo => "abriu o jogo",
            Self::ProcessoDoJogo => "foi aberto pelo jogo",
        }
    }
}

pub fn e_anti_cheat(nome: &str) -> bool {
    ANTI_CHEATS.contains(&nome.to_lowercase().as_str())
}

/// Calcula, para uma foto dos processos, quem é intocável.
pub struct Protecao {
    jogo: HashMap<u32, Motivo>,
    eu: u32,
}

impl Protecao {
    /// `jogo` = PID do jogo atual, se houver.
    pub fn nova(processos: &[Processo], jogo: Option<u32>) -> Self {
        let mut mapa = HashMap::new();
        if let Some(pid_jogo) = jogo {
            let por_pid: HashMap<u32, &Processo> = processos.iter().map(|p| (p.pid, p)).collect();
            mapa.insert(pid_jogo, Motivo::Jogo);

            // Subindo: quem abriu o jogo, quem abriu esse, e assim por diante.
            // Um "pai" que nasceu depois do filho é outro processo com o PID
            // reaproveitado, e ali a corrente para.
            let mut atual = por_pid.get(&pid_jogo).copied();
            let mut vistos = HashSet::from([pid_jogo]);
            while let Some(filho) = atual {
                let Some(pai) = por_pid.get(&filho.pid_pai).copied() else { break };
                if pai.criado_em > filho.criado_em || !vistos.insert(pai.pid) || pai.sessao == 0 {
                    break;
                }
                mapa.entry(pai.pid).or_insert(Motivo::LauncherDoJogo);
                atual = Some(pai);
            }

            // Descendo: tudo o que o jogo abriu.
            let mut fila = vec![pid_jogo];
            while let Some(pai) = fila.pop() {
                let criado_pai = por_pid.get(&pai).map_or(i64::MAX, |p| p.criado_em);
                for p in processos {
                    if p.pid_pai == pai && p.criado_em >= criado_pai && !mapa.contains_key(&p.pid) {
                        mapa.insert(p.pid, Motivo::ProcessoDoJogo);
                        fila.push(p.pid);
                    }
                }
            }
        }
        Self { jogo: mapa, eu: std::process::id() }
    }

    pub fn motivo(&self, p: &Processo) -> Option<Motivo> {
        if let Some(motivo) = self.jogo.get(&p.pid) {
            return Some(*motivo);
        }
        let nome = p.nome.to_lowercase();
        if p.pid <= 4 || p.pid == self.eu || SISTEMA.contains(&nome.as_str()) {
            return Some(Motivo::Windows);
        }
        if ANTI_CHEATS.contains(&nome.as_str()) {
            return Some(Motivo::AntiCheat);
        }
        if p.sessao == 0 {
            return Some(Motivo::Servico);
        }
        None
    }
}

#[cfg(test)]
pub(crate) mod testes {
    use super::*;

    pub(crate) fn proc(pid: u32, pai: u32, nome: &str, criado_em: i64) -> Processo {
        Processo {
            pid,
            pid_pai: pai,
            nome: nome.to_owned(),
            ws_privado: 50 << 20,
            working_set: 60 << 20,
            confirmada: 70 << 20,
            sessao: 1,
            criado_em,
            cpu_100ns: 0,
            suspenso: false,
        }
    }

    #[test]
    fn protege_launcher_jogo_e_filhos() {
        let lista = [
            proc(100, 4, "explorer.exe", 1),
            proc(200, 100, "RiotClientServices.exe", 2),
            proc(300, 200, "VALORANT-Win64-Shipping.exe", 3),
            proc(400, 300, "CrashHandler.exe", 4),
            proc(500, 100, "Discord.exe", 5),
            proc(600, 7, "vgc.exe", 6),
        ];
        let p = Protecao::nova(&lista, Some(300));
        assert_eq!(p.motivo(&lista[2]), Some(Motivo::Jogo));
        assert_eq!(p.motivo(&lista[1]), Some(Motivo::LauncherDoJogo));
        assert_eq!(p.motivo(&lista[3]), Some(Motivo::ProcessoDoJogo));
        // explorer.exe está na corrente, mas é do Windows de qualquer jeito.
        assert!(p.motivo(&lista[0]).is_some());
        assert_eq!(p.motivo(&lista[4]), None);
        assert_eq!(p.motivo(&lista[5]), Some(Motivo::AntiCheat));
    }

    #[test]
    fn pid_reaproveitado_nao_vira_launcher() {
        // O pai real (200) fechou e o PID foi reusado por um app aberto depois.
        let lista = [proc(200, 1, "Discord.exe", 50), proc(300, 200, "jogo.exe", 10)];
        let p = Protecao::nova(&lista, Some(300));
        assert_eq!(p.motivo(&lista[0]), None);
    }

    #[test]
    fn servicos_sao_protegidos() {
        let mut servico = proc(900, 1, "AlgumServico.exe", 1);
        servico.sessao = 0;
        assert_eq!(Protecao::nova(&[], None).motivo(&servico), Some(Motivo::Servico));
    }
}
