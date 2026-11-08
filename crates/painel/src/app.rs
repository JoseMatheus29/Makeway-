//! A janela: barra lateral com as cinco seções, a tela escolhida, avisos
//! rápidos no canto e os dados que as telas compartilham.

use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

use eframe::egui::{self, Align2, Color32, FontId, Frame, Margin, RichText, ScrollArea, Sense, Stroke, Ui, Vec2, pos2};
use nucleo::config::{self, Config, ConfigModoJogo, Modo};
use nucleo::formato::tamanho;
use nucleo::inicializacao::{self, ProgramaInicial};
use nucleo::jogos::{self, JogoInstalado};
use nucleo::modo_jogo::relatorio::{self, RelatorioSessao};
use nucleo::modo_jogo::vigia::{self, StatusVigia};
use nucleo::ram::{self, RelatorioRam};
use nucleo::temporarios::{self, AnaliseTemporarios, ResultadoLimpeza};
use nucleo::{inicio_automatico, nomes, plataforma};

use crate::componentes::{
    CORTE, CORTE_P, apoio, botao_primario, botao_secundario, display, forma_cortada, grade_de_pontos, moldura,
    paragrafo, rotulo, texto_espacado,
};
use crate::icones::{self, Icone};
use crate::tarefa::{Tarefa, acompanhar};
use crate::tema::{self, paleta};
use crate::telas::{espaco::TelaEspaco, inicio::TelaInicio, memoria::TelaMemoria, modo_jogo::TelaModoJogo, partidas::TelaPartidas};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tela {
    Inicio,
    ModoJogo,
    Espaco,
    Memoria,
    Partidas,
}

impl Tela {
    pub fn do_nome(nome: &str) -> Option<Self> {
        Some(match nome {
            "inicio" => Self::Inicio,
            "jogo" | "modo_jogo" => Self::ModoJogo,
            "espaco" | "disco" => Self::Espaco,
            "memoria" | "ram" => Self::Memoria,
            "partidas" | "relatorios" => Self::Partidas,
            _ => return None,
        })
    }
}

/// Relatório de memória + nomes amigáveis descobertos junto.
type LeituraDaRam = (io::Result<RelatorioRam>, Vec<(String, String)>);

/// Dados lidos em segundo plano e compartilhados pelas telas.
#[derive(Default)]
pub struct Dados {
    pub ram: Option<RelatorioRam>,
    tarefa_ram: Option<Tarefa<LeituraDaRam>>,
    ram_em: Option<Instant>,
    /// .exe em minúsculas → nome amigável.
    pub nomes: HashMap<String, String>,
    pub temporarios: Option<AnaliseTemporarios>,
    tarefa_temporarios: Option<Tarefa<AnaliseTemporarios>>,
    pub limpeza: Option<ResultadoLimpeza>,
    tarefa_limpeza: Option<Tarefa<ResultadoLimpeza>>,
    pub inicializacao: Option<Vec<ProgramaInicial>>,
    tarefa_inicializacao: Option<Tarefa<Vec<ProgramaInicial>>>,
    pub partidas: Vec<(PathBuf, RelatorioSessao)>,
    partidas_em: Option<Instant>,
    /// (livre, total) do disco do Windows.
    pub disco: Option<(u64, u64)>,
    disco_em: Option<Instant>,
    pub jogos: Option<Vec<JogoInstalado>>,
    tarefa_jogos: Option<Tarefa<Vec<JogoInstalado>>>,
    pub com_windows: Option<bool>,
    tarefa_com_windows: Option<Tarefa<bool>>,
    tarefa_mudar_windows: Option<Tarefa<io::Result<bool>>>,
}

impl Dados {
    /// Nome amigável de um .exe ("Opera GX Internet Browser").
    pub fn nome(&self, exe: &str) -> String {
        self.nomes
            .get(&exe.to_lowercase())
            .cloned()
            .unwrap_or_else(|| nomes::amigavel(exe, None))
    }

    pub fn limpando(&self) -> bool {
        self.tarefa_limpeza.is_some()
    }

    pub fn mudando_com_windows(&self) -> bool {
        self.tarefa_mudar_windows.is_some() || self.tarefa_com_windows.is_some()
    }
}

/// O que todas as telas precisam.
pub struct Contexto {
    pub caminho_config: PathBuf,
    pub pasta_dados: PathBuf,
    /// `None` = vigia parado.
    pub status: Option<StatusVigia>,
    pub config: Config,
    pub dados: Dados,
    pub ir_para: Option<Tela>,
    pub confirmar_limpeza: bool,
    aviso: Option<(bool, String, Instant)>,
}

impl Contexto {
    pub fn avisar(&mut self, ok: bool, texto: impl Into<String>) {
        self.aviso = Some((ok, texto.into(), Instant::now()));
    }

    /// Grava as opções do modo jogo (o vigia relê sozinho).
    pub fn salvar_modo_jogo(&mut self, novo: ConfigModoJogo, mensagem: &str) {
        match config::salvar_modo_jogo(&self.caminho_config, &novo) {
            Ok(cfg) => {
                self.config = cfg;
                self.avisar(true, mensagem);
            }
            Err(erro) => self.avisar(false, format!("Não deu para salvar: {erro}")),
        }
    }

    pub fn definir_modo(&mut self, modo: Modo) {
        let mut m = self.config.modo_jogo.clone();
        m.modo = modo;
        let texto = match modo {
            Modo::Observar => "Agora o Makeway só observa as partidas",
            Modo::Otimizar => "Agora o Makeway otimiza as partidas",
        };
        self.salvar_modo_jogo(m, texto);
    }

    pub fn pausado_nos_jogos(&self, exe: &str) -> bool {
        self.config.modo_jogo.pausar.iter().any(|n| n.eq_ignore_ascii_case(exe))
    }

    /// Põe ou tira um app da lista de pausar durante os jogos.
    pub fn alternar_pausa(&mut self, exe: &str) {
        let nome = self.dados.nome(exe);
        let mut m = self.config.modo_jogo.clone();
        let texto = if self.pausado_nos_jogos(exe) {
            m.pausar.retain(|n| !n.eq_ignore_ascii_case(exe));
            format!("{nome} não vai mais ser pausado")
        } else {
            m.pausar.push(exe.to_owned());
            let extra = if m.modo == Modo::Observar { " (quando o modo Otimizar estiver ligado)" } else { "" };
            format!("{nome} vai ser pausado nos jogos{extra}")
        };
        self.salvar_modo_jogo(m, &texto);
    }

    pub fn iniciar_vigia(&mut self) {
        if self.dados.com_windows == Some(true) && inicio_automatico::iniciar_agora().is_ok() {
            self.avisar(true, "Vigia ligado, com permissão de administrador");
            return;
        }
        let exe = std::env::current_exe().unwrap_or_default().with_file_name("makeway-vigia.exe");
        match std::process::Command::new(exe).spawn() {
            Ok(_) => self.avisar(true, "Vigia ligado. Ele fica perto do relógio"),
            Err(erro) => self.avisar(false, format!("Não deu para ligar o vigia: {erro}")),
        }
    }

    /// Liga ou desliga "abrir com o Windows" (o Windows pede confirmação).
    pub fn mudar_com_windows(&mut self, ctx: &egui::Context, ligar: bool) {
        let vigia_exe = std::env::current_exe().unwrap_or_default().with_file_name("makeway-vigia.exe");
        self.dados.tarefa_mudar_windows = Some(Tarefa::iniciar(ctx, move || {
            if ligar { inicio_automatico::ativar(&vigia_exe) } else { inicio_automatico::desativar() }?;
            Ok(inicio_automatico::ativo())
        }));
    }

    pub fn limpar_temporarios(&mut self, ctx: &egui::Context) {
        if let Some(analise) = &self.dados.temporarios {
            let itens = analise.antigos.clone();
            self.dados.limpeza = None;
            self.dados.tarefa_limpeza = Some(Tarefa::iniciar(ctx, move || temporarios::mandar_para_lixeira(&itens)));
        }
    }

    /// Atualiza o que estiver velho. `precisa_ram`: a tela atual mostra memória.
    fn atualizar(&mut self, ctx: &egui::Context, precisa_ram: bool) {
        let d = &mut self.dados;

        let mut ram_nova = None;
        acompanhar(&mut d.tarefa_ram, &mut ram_nova);
        if let Some((resultado, novos)) = ram_nova {
            d.nomes.extend(novos);
            if let Ok(r) = resultado {
                d.ram = Some(r);
            }
        }
        if precisa_ram && d.tarefa_ram.is_none() && d.ram_em.is_none_or(|t| t.elapsed() >= Duration::from_secs(2)) {
            let conhecidos = d.nomes.clone();
            d.tarefa_ram = Some(Tarefa::iniciar(ctx, move || {
                let r = ram::analisar();
                let novos = match &r {
                    Ok(r) => nomes::dos_apps(&r.apps, &r.processos, 30, &conhecidos),
                    Err(_) => Vec::new(),
                };
                (r, novos)
            }));
            d.ram_em = Some(Instant::now());
        }

        let estava_limpando = d.tarefa_limpeza.is_some();
        acompanhar(&mut d.tarefa_limpeza, &mut d.limpeza);
        acompanhar(&mut d.tarefa_temporarios, &mut d.temporarios);
        let limpou = estava_limpando && d.tarefa_limpeza.is_none();
        if (d.temporarios.is_none() && d.tarefa_temporarios.is_none()) || limpou {
            let dias = self.config.temporarios.idade_minima_dias;
            d.tarefa_temporarios = Some(Tarefa::iniciar(ctx, move || {
                temporarios::analisar(&temporarios::pastas_padrao(), dias, SystemTime::now())
            }));
        }

        if d.inicializacao.is_none() && d.tarefa_inicializacao.is_none() {
            d.tarefa_inicializacao = Some(Tarefa::iniciar(ctx, inicializacao::listar));
        }
        acompanhar(&mut d.tarefa_inicializacao, &mut d.inicializacao);

        if d.jogos.is_none() && d.tarefa_jogos.is_none() {
            d.tarefa_jogos = Some(Tarefa::iniciar(ctx, jogos::instalados));
        }
        acompanhar(&mut d.tarefa_jogos, &mut d.jogos);

        if d.com_windows.is_none() && d.tarefa_com_windows.is_none() && d.tarefa_mudar_windows.is_none() {
            d.tarefa_com_windows = Some(Tarefa::iniciar(ctx, inicio_automatico::ativo));
        }
        acompanhar(&mut d.tarefa_com_windows, &mut d.com_windows);
        let mut mudou = None;
        acompanhar(&mut d.tarefa_mudar_windows, &mut mudou);

        if d.partidas_em.is_none_or(|t| t.elapsed() >= Duration::from_secs(5)) {
            d.partidas = relatorio::listar(&vigia::pasta_de_relatorios(&self.pasta_dados));
            d.partidas_em = Some(Instant::now());
        }
        if d.disco_em.is_none_or(|t| t.elapsed() >= Duration::from_secs(10)) {
            let unidade = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
            d.disco = plataforma::espaco_no_disco(&PathBuf::from(format!(r"{unidade}\"))).ok();
            d.disco_em = Some(Instant::now());
        }

        if limpou && let Some(r) = &self.dados.limpeza {
            let texto = if r.falhas.is_empty() {
                format!("{} foram para a Lixeira", tamanho(r.bytes))
            } else {
                format!("{} foram para a Lixeira. {} itens em uso ficaram", tamanho(r.bytes), r.falhas.len())
            };
            self.avisar(true, texto);
        }
        match mudou {
            Some(Ok(ligado)) => {
                self.dados.com_windows = Some(ligado);
                self.avisar(true, if ligado { "O vigia vai abrir junto com o Windows" } else { "O vigia não abre mais com o Windows" });
            }
            Some(Err(erro)) => self.avisar(false, format!("Não deu: {erro}")),
            None => {}
        }
        ctx.request_repaint_after(Duration::from_secs(2));
    }
}

#[derive(Default)]
struct Telas {
    inicio: TelaInicio,
    jogo: TelaModoJogo,
    espaco: TelaEspaco,
    memoria: TelaMemoria,
    partidas: TelaPartidas,
}

pub struct App {
    ctx: Contexto,
    tela: Tela,
    telas: Telas,
    status_lido_em: Option<Instant>,
}

impl App {
    pub fn new(caminho_config: PathBuf, tela: Tela) -> Self {
        let pasta_dados = config::pasta_dos_dados_de(&caminho_config);
        Self {
            ctx: Contexto {
                config: config::carregar(&caminho_config).unwrap_or_default(),
                caminho_config,
                pasta_dados,
                status: None,
                dados: Dados::default(),
                ir_para: None,
                confirmar_limpeza: false,
                aviso: None,
            },
            tela,
            telas: Telas::default(),
            status_lido_em: None,
        }
    }

    /// O vigia escreve o status num arquivo; a janela relê (com o config) a cada 2 s.
    fn atualizar_status(&mut self) {
        if self.status_lido_em.is_none_or(|t| t.elapsed() >= Duration::from_secs(2)) {
            self.ctx.status = vigia::ler_status(&self.ctx.pasta_dados);
            if let Ok(cfg) = config::carregar(&self.ctx.caminho_config) {
                self.ctx.config = cfg;
            }
            self.status_lido_em = Some(Instant::now());
        }
    }

    fn barra_lateral(&mut self, ui: &mut Ui) {
        let p = paleta(ui);
        ui.add_space(22.0);
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.spacing_mut().item_spacing.x = 12.0;
            icones::logo(ui, 38.0, p.lime, p.na_lime);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.label(display("Makeway", 21.0).extra_letter_spacing(2.1));
                ui.label(rotulo("Game booster", 11.0, p.txt3).extra_letter_spacing(2.6));
            });
        });
        ui.add_space(26.0);
        ui.spacing_mut().item_spacing.y = 2.0;
        for (tela, icone, nome) in [
            (Tela::Inicio, Icone::Casa, "Início"),
            (Tela::ModoJogo, Icone::Controle, "Modo jogo"),
            (Tela::Espaco, Icone::Disco, "Liberar espaço"),
            (Tela::Memoria, Icone::Chip, "Memória"),
            (Tela::Partidas, Icone::Grafico, "Partidas"),
        ] {
            if item_de_navegacao(ui, icone, nome, self.tela == tela).clicked() {
                self.tela = tela;
            }
        }
        // O cartão do vigia fica no pé da barra.
        let altura_do_cartao = if self.ctx.status.is_none() { 140.0 } else { 78.0 };
        let sobra = ui.available_height() - altura_do_cartao - 22.0;
        ui.add_space(sobra.max(16.0));
        self.cartao_do_vigia(ui);
    }

    fn cartao_do_vigia(&mut self, ui: &mut Ui) {
        let p = paleta(ui);
        let (cor, nome, detalhe, pulsa) = match &self.ctx.status {
            None => (p.cinza, "Vigia offline", "Os jogos não são vigiados".to_owned(), false),
            Some(s) if !s.ativo => (p.cinza, "Modo jogo off", "Nada acontece nos jogos".to_owned(), false),
            Some(s) => match (&s.jogo, s.modo) {
                (Some(jogo), Modo::Otimizar) => (p.lime, "Otimizando", jogo.trim_end_matches(".exe").to_owned(), true),
                (Some(jogo), Modo::Observar) => (p.aviso, "Observando", jogo.trim_end_matches(".exe").to_owned(), true),
                (None, Modo::Observar) => (p.lime, "Vigia online", "Esperando um jogo · só observa".to_owned(), true),
                (None, Modo::Otimizar) => (p.lime, "Vigia online", "Esperando um jogo · vai otimizar".to_owned(), true),
            },
        };
        let parado = self.ctx.status.is_none();
        moldura(ui, p.sup, Stroke::NONE, CORTE, Margin::same(16), |ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                let (rect, _) = ui.allocate_exact_size(Vec2::new(10.0, 18.0), Sense::hover());
                let centro = rect.center();
                if pulsa {
                    // Anel que cresce e some a cada 1,8 s (só com a janela em foco).
                    let t = (ui.input(|i| i.time) % 1.8 / 1.8) as f32;
                    ui.painter().circle_filled(centro, 4.5 + 6.0 * t, cor.gamma_multiply(0.5 * (1.0 - t)));
                    if ui.input(|i| i.viewport().focused.unwrap_or(false)) {
                        ui.ctx().request_repaint_after(Duration::from_millis(50));
                    }
                }
                ui.painter().circle_filled(centro, 4.5, cor);
                ui.label(display(nome, 13.0).extra_letter_spacing(2.0));
            });
            ui.label(RichText::new(detalhe).size(13.0).color(p.txt3));
            if parado {
                ui.add_space(6.0);
                if botao_primario(ui, "Ligar o vigia").clicked() {
                    self.ctx.iniciar_vigia();
                    self.status_lido_em = None;
                }
            }
        });
    }

    fn modal_de_limpeza(&mut self, ctx: &egui::Context) {
        if !self.ctx.confirmar_limpeza {
            return;
        }
        let Some(analise) = &self.ctx.dados.temporarios else {
            self.ctx.confirmar_limpeza = false;
            return;
        };
        let (quantos, bytes) = (analise.antigos.len(), analise.bytes_antigos());
        let mut decisao = None;
        let p = &tema::PALETA;
        let modal = egui::Modal::new(egui::Id::unique("confirmar_lixeira"))
            .backdrop_color(Color32::from_black_alpha(170))
            .frame(Frame::new().fill(p.sup).stroke(Stroke::new(1.0, p.borda2)).inner_margin(Margin::same(28)))
            .show(ctx, |ui| {
                ui.set_width(420.0);
                ui.label(display("Mandar para a Lixeira?", 26.0));
                ui.add_space(2.0);
                paragrafo(
                    ui,
                    apoio(ui, format!("{quantos} itens antigos ({}) vão para a Lixeira. Dá para recuperar qualquer um de lá. Arquivos em uso ficam onde estão.", tamanho(bytes))),
                );
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if botao_primario(ui, "Limpar agora").clicked() {
                        decisao = Some(true);
                    }
                    if botao_secundario(ui, "Cancelar").clicked() {
                        decisao = Some(false);
                    }
                });
            });
        match decisao {
            Some(true) => {
                self.ctx.confirmar_limpeza = false;
                self.ctx.limpar_temporarios(ctx);
            }
            Some(false) => self.ctx.confirmar_limpeza = false,
            None if modal.should_close() => self.ctx.confirmar_limpeza = false,
            None => {}
        }
    }

    fn aviso_rapido(&mut self, ctx: &egui::Context) {
        let Some((ok, texto, quando)) = &self.ctx.aviso else { return };
        if quando.elapsed() > Duration::from_secs(4) {
            self.ctx.aviso = None;
            return;
        }
        let (ok, texto) = (*ok, texto.clone());
        egui::Area::new(egui::Id::unique("aviso_rapido"))
            .anchor(Align2::RIGHT_BOTTOM, Vec2::new(-24.0, -24.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                let p = paleta(ui);
                let cor = if ok { p.lime } else { p.aviso };
                let lugar = ui.painter().add(egui::Shape::Noop);
                let resposta = Frame::new().inner_margin(Margin { left: 22, right: 18, top: 14, bottom: 14 }).show(ui, |ui| {
                    ui.set_max_width(420.0);
                    ui.horizontal(|ui| {
                        let icone = if ok { Icone::CheckSimples } else { Icone::Alerta };
                        icones::icone(ui, icone, 18.0, cor);
                        paragrafo(ui, RichText::new(texto).size(14.5).family(tema::forte()).color(p.txt));
                    });
                });
                let rect = resposta.response.rect;
                ui.painter().set(lugar, forma_cortada(rect, CORTE_P, p.sup2, Stroke::new(1.0, p.borda2)));
                // Faixa de cor na borda esquerda.
                let faixa = egui::Rect::from_min_size(pos2(rect.left(), rect.top() + CORTE_P), Vec2::new(3.0, rect.height() - CORTE_P));
                ui.painter().rect_filled(faixa, 0.0, cor);
            });
        ctx.request_repaint_after(Duration::from_millis(500));
    }
}

fn item_de_navegacao(ui: &mut Ui, icone: Icone, nome: &str, ativo: bool) -> egui::Response {
    let p = paleta(ui);
    let (rect, resposta) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 46.0), Sense::click());
    let fundo = if ativo || resposta.hovered() { p.sup2 } else { Color32::TRANSPARENT };
    ui.painter().add(forma_cortada(rect, CORTE_P, fundo, Stroke::NONE));
    let mut x = rect.left() + 12.0;
    if ativo {
        let barra = egui::Rect::from_center_size(pos2(x + 2.0, rect.center().y), Vec2::new(4.0, 18.0));
        ui.painter().rect_filled(barra.expand(3.0), 0.0, p.lime.gamma_multiply(0.15));
        ui.painter().rect_filled(barra.expand(1.5), 0.0, p.lime.gamma_multiply(0.25));
        ui.painter().rect_filled(barra, 0.0, p.lime);
    }
    x += 16.0;
    let cor_icone = if ativo { p.lime } else if resposta.hovered() { p.txt } else { p.txt2 };
    let caixa = egui::Rect::from_center_size(pos2(x + 10.0, rect.center().y), Vec2::splat(20.0));
    icones::pintar(ui.painter(), caixa, icone, cor_icone);
    let fonte = FontId::new(15.0, if ativo { tema::display() } else { tema::rotulo() });
    let cor = if ativo || resposta.hovered() { p.txt } else { p.txt2 };
    texto_espacado(ui.painter(), pos2(x + 32.0, rect.center().y), Align2::LEFT_CENTER, &nome.to_uppercase(), fonte, 1.2, cor);
    let nome = nome.to_owned();
    resposta.widget_info(|| egui::WidgetInfo::selected(egui::Role::Tab, true, ativo, &nome));
    resposta.on_hover_cursor(egui::CursorIcon::PointingHand)
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.atualizar_status();
        let precisa_ram = self.tela != Tela::Espaco;
        self.ctx.atualizar(&ctx, precisa_ram);
        let p = paleta(ui);

        egui::Panel::left("navegacao")
            .resizable(false)
            .exact_size(244.0)
            .frame(Frame::new().fill(p.rail).inner_margin(Margin::symmetric(14, 0)).stroke(Stroke::new(1.0, p.borda)))
            .show(ui, |ui| self.barra_lateral(ui));

        egui::CentralPanel::default()
            .frame(Frame::new().fill(p.bg).inner_margin(Margin { left: 44, right: 28, top: 0, bottom: 0 }))
            .show(ui, |ui| {
                grade_de_pontos(ui.painter(), ui.max_rect().expand2(Vec2::new(44.0, 0.0)), Color32::from_white_alpha(9));
                ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    ui.add_space(40.0);
                    // Nunca mais larga que a janela; no máximo 1060 px em telas grandes.
                    ui.set_max_width(ui.available_width().min(1060.0));
                    let c = &mut self.ctx;
                    match self.tela {
                        Tela::Inicio => self.telas.inicio.ui(ui, c),
                        Tela::ModoJogo => self.telas.jogo.ui(ui, c),
                        Tela::Espaco => self.telas.espaco.ui(ui, c),
                        Tela::Memoria => self.telas.memoria.ui(ui, c),
                        Tela::Partidas => self.telas.partidas.ui(ui, c),
                    }
                    ui.add_space(56.0);
                });
            });

        self.modal_de_limpeza(&ctx);
        self.aviso_rapido(&ctx);
        if let Some(tela) = self.ctx.ir_para.take() {
            self.tela = tela;
        }
    }
}
