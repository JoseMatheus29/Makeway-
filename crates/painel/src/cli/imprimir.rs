//! Saída em texto de cada comando.

use std::path::{Path, PathBuf};

use nucleo::disco::{Pasta, Pulados, Varredura};
use nucleo::duplicados::ResultadoDuplicados;
use nucleo::formato::{data, data_hora, decimal, duracao, gb, mb, tamanho};
use nucleo::modo_jogo::relatorio::RelatorioSessao;
use nucleo::modo_jogo::sessao::Restauracao;
use nucleo::inicializacao::ProgramaInicial;
use nucleo::ram::RelatorioRam;
use nucleo::temporarios::{AnaliseTemporarios, LIMITE_LIXEIRA, ResultadoLimpeza};

pub fn ram(r: &RelatorioRam, top: usize, por_processo: bool) {
    let m = &r.memoria;
    println!("Memória");
    if let Some(instalada) = m.instalada {
        let reservada = instalada.saturating_sub(m.total);
        println!("  Instalada            {:>12}   (reservada para hardware: {})", gb(instalada), tamanho(reservada));
    }
    println!("  Utilizável           {:>12}", gb(m.total));
    println!("  Em uso               {:>12}   ({}%)", gb(m.em_uso), m.carga_pct);
    println!("  Disponível           {:>12}", gb(m.disponivel));
    println!("  Confirmada           {:>12}   de {} possíveis", gb(m.confirmada), gb(m.limite_confirmada));
    if let Some(l) = &m.listas {
        println!(
            "  Em cache             {:>12}   (standby {}, modificada {})",
            gb(l.em_cache()),
            tamanho(l.standby),
            tamanho(l.modificada)
        );
        println!("  Livre                {:>12}", tamanho(l.livre));
    }
    println!("  Pool paginado        {:>12}", mb(m.pool_paginado));
    println!("  Pool não paginado    {:>12}", mb(m.pool_nao_paginado));

    println!();
    println!("Arquivo de paginação");
    if r.paginacao.is_empty() {
        println!("  Nenhum. Sem ele, a RAM confirmada não pode passar da RAM física.");
    }
    for a in &r.paginacao {
        println!(
            "  {}   em uso {} de {} (pico desde o boot: {})",
            a.caminho,
            tamanho(a.em_uso),
            tamanho(a.tamanho),
            tamanho(a.pico)
        );
    }

    println!();
    // (nome, quantidade de processos ou PID, memória privada)
    let linhas: Vec<(&str, String, u64)> = if por_processo {
        println!("Processos que mais usam RAM (memória privada, como na aba Detalhes)");
        println!("  {:<34} {:>7} {:>14}", "Processo", "PID", "Memória");
        r.processos.iter().map(|p| (p.nome.as_str(), p.pid.to_string(), p.ws_privado)).collect()
    } else {
        println!("Apps que mais usam RAM (memória privada, como no Gerenciador de Tarefas)");
        println!("  {:<34} {:>7} {:>14}", "App", "Proc.", "Memória");
        r.apps.iter().map(|a| (a.nome.as_str(), a.processos.to_string(), a.ws_privado)).collect()
    };
    for (nome, coluna, memoria) in linhas.iter().take(top) {
        println!("  {:<34} {:>7} {:>14}", cortar(nome, 34), coluna, mb(*memoria));
    }
    if linhas.len() > top {
        let resto = &linhas[top..];
        let soma: u64 = resto.iter().map(|l| l.2).sum();
        let rotulo = format!("Outros ({} itens)", decimal(resto.len() as f64, 0));
        println!("  {:<34} {:>7} {:>14}", rotulo, "", mb(soma));
    }
    let total = r.total_processos();
    let rotulo = format!("Total ({} processos)", decimal(r.processos.len() as f64, 0));
    println!("  {:<34} {:>7} {:>14}", rotulo, "", mb(total));
    println!();
    println!(
        "O resto de \"Em uso\" ({}) não aparece por processo: é memória compartilhada \
         entre processos (DLLs, arquivos abertos), o kernel, os drivers e a GPU integrada.",
        tamanho(m.em_uso.saturating_sub(total))
    );
}

pub fn disco(v: &Varredura, top: usize) {
    let raiz = &v.raiz;
    println!(
        "{}  {} em {} arquivos e {} pastas ({} s)",
        raiz.caminho.display(),
        tamanho(raiz.tamanho),
        decimal(raiz.arquivos as f64, 0),
        decimal(raiz.subpastas as f64, 0),
        decimal(v.segundos, 1)
    );
    println!();
    println!("Maiores pastas");
    pasta(raiz, raiz.tamanho, 0, top);

    println!();
    println!("Maiores arquivos");
    for a in &v.maiores_arquivos {
        println!("  {:>11}  {}", tamanho(a.tamanho), a.caminho.display());
    }
    pulados(&v.pulados);
    println!("Nada foi apagado.");
}

fn pasta(p: &Pasta, total: u64, nivel: usize, top: usize) {
    let recuo = "  ".repeat(nivel + 1);
    let largura = 44 - recuo.len().min(40);
    for filho in p.filhos.iter().take(top) {
        let nome = filho.caminho.file_name().map(|n| n.to_string_lossy()).unwrap_or_default();
        println!(
            "{recuo}{:<largura$} {:>11} {:>5}",
            cortar(&nome, largura),
            tamanho(filho.tamanho),
            porcentagem(filho.tamanho, total)
        );
        pasta(filho, total, nivel + 1, top);
    }
    if p.filhos.len() > top {
        let resto = &p.filhos[top..];
        let soma: u64 = resto.iter().map(|f| f.tamanho).sum();
        let rotulo = format!("(mais {} pastas)", decimal(resto.len() as f64, 0));
        println!("{recuo}{rotulo:<largura$} {:>11} {:>5}", tamanho(soma), porcentagem(soma, total));
    }
    let nas_subpastas: u64 = p.filhos.iter().map(|f| f.tamanho).sum();
    let soltos = p.tamanho.saturating_sub(nas_subpastas);
    if !p.filhos.is_empty() && soltos * 100 >= total {
        let rotulo = "(arquivos fora de subpastas)";
        println!("{recuo}{rotulo:<largura$} {:>11} {:>5}", tamanho(soltos), porcentagem(soltos, total));
    }
}

pub fn duplicados(pasta: &Path, min_mb: u64, r: &ResultadoDuplicados, top: usize) {
    let copias: usize = r.grupos.iter().map(|g| g.arquivos.len()).sum();
    println!("Duplicados em {} (arquivos a partir de {min_mb} MB, {} s)", pasta.display(), decimal(r.segundos, 1));
    println!(
        "  {} com tamanho repetido → {} com o mesmo começo → {} cópias iguais em {} grupos",
        decimal(r.candidatos_tamanho as f64, 0),
        decimal(r.candidatos_comeco as f64, 0),
        decimal(copias as f64, 0),
        decimal(r.grupos.len() as f64, 0)
    );
    println!("  Espaço que voltaria deixando uma cópia de cada: {}", tamanho(r.desperdicio_total));
    for g in r.grupos.iter().take(top) {
        println!();
        println!(
            "  {} sobrando ({} cópias de {})",
            tamanho(g.desperdicio),
            g.arquivos.len(),
            tamanho(g.tamanho)
        );
        for arquivo in &g.arquivos {
            println!("    {}", arquivo.display());
        }
    }
    if r.grupos.len() > top {
        let resto = &r.grupos[top..];
        let soma: u64 = resto.iter().map(|g| g.desperdicio).sum();
        println!();
        println!("  (mais {} grupos, {} sobrando; use --top para ver)", decimal(resto.len() as f64, 0), tamanho(soma));
    }
    if r.nao_lidos > 0 {
        println!("  {} arquivos não puderam ser lidos (em uso ou sem permissão).", decimal(r.nao_lidos as f64, 0));
    }
    pulados(&r.pulados);
    println!("Nada foi apagado. Confira cada grupo antes de apagar uma cópia à mão.");
}

pub fn temporarios(a: &AnaliseTemporarios, vai_apagar: bool) {
    println!("Temporários (antigos = criados e alterados há mais de {} dias)", a.idade_minima_dias);
    for p in &a.pastas {
        if p.acessivel {
            println!("  {:<50} {:>11} no total", p.caminho.display().to_string(), tamanho(p.tamanho));
        } else {
            println!("  {:<50} sem permissão (precisa de administrador)", p.caminho.display().to_string());
        }
    }
    println!();
    println!(
        "Podem ir para a Lixeira: {} itens, {}",
        decimal(a.antigos.len() as f64, 0),
        tamanho(a.bytes_antigos())
    );
    for item in a.antigos.iter().take(15) {
        let tipo = if item.pasta { "pasta" } else { "arquivo" };
        println!("  {:>11}  {:<7} {:>4} dias  {}", tamanho(item.tamanho), tipo, item.idade_dias, item.caminho.display());
    }
    if a.antigos.len() > 15 {
        println!("  (mais {} itens)", decimal((a.antigos.len() - 15) as f64, 0));
    }
    println!(
        "Ficam: {} itens recentes ou que não deu para ler ({})",
        decimal(a.mantidos as f64, 0),
        tamanho(a.bytes_mantidos)
    );
    if !a.grandes_demais.is_empty() {
        println!();
        println!(
            "Antigos, mas com mais de {} (a Lixeira pode não aceitar). Apague à mão se quiser:",
            tamanho(LIMITE_LIXEIRA)
        );
        for item in &a.grandes_demais {
            println!("  {:>11}  {}", tamanho(item.tamanho), item.caminho.display());
        }
    }
    if !vai_apagar && !a.antigos.is_empty() {
        println!();
        println!("Nada foi apagado. Para mandar para a Lixeira: makeway-cli temporarios --apagar");
    }
}

pub fn limpeza(r: &ResultadoLimpeza) {
    println!("Na Lixeira: {} itens, {}.", decimal(r.enviados as f64, 0), tamanho(r.bytes));
    if !r.falhas.is_empty() {
        println!("Ficaram {} itens que o Windows não deixou mover (em uso ou sem permissão):", r.falhas.len());
        for (caminho, motivo) in r.falhas.iter().take(10) {
            println!("  {}  ({motivo})", caminho.display());
        }
    }
}

pub fn inicializacao(programas: &[ProgramaInicial]) {
    let (ativos, desativados): (Vec<_>, Vec<_>) = programas.iter().partition(|p| p.ativo);
    println!("Abrem com o Windows ({})", ativos.len());
    for p in &ativos {
        println!("  {:<32} {}", cortar(&p.nome, 32), p.origem);
        println!("      {}", p.comando);
    }
    println!();
    println!("Desativados no Gerenciador de Tarefas ({})", desativados.len());
    for p in &desativados {
        let desde = p.desativado_em_unix.map(|d| format!("desde {}", data(d))).unwrap_or_default();
        println!("  {:<32} {:<17} {}", cortar(&p.nome, 32), desde, p.origem);
    }
    println!();
    println!("Para ativar ou desativar: Gerenciador de Tarefas > Aplicativos de inicialização.");
}

pub fn sessoes(pasta: &Path, relatorios: &[(PathBuf, RelatorioSessao)]) {
    if relatorios.is_empty() {
        println!("Nenhuma partida gravada ainda em {}.", pasta.display());
        println!("Deixe o makeway-vigia aberto enquanto joga.");
        return;
    }
    println!("Partidas gravadas ({})", pasta.display());
    println!(
        "  {:>3}  {:<16}  {:<30} {:>9}  {:<9} {:>13} {:>11}",
        "#", "Início", "Jogo", "Duração", "Modo", "RAM disp. mín", "Paginação"
    );
    for (i, (_, r)) in relatorios.iter().enumerate() {
        let crescimento = r.paginacao_max.saturating_sub(r.paginacao_inicio);
        println!(
            "  {:>3}  {:<16}  {:<30} {:>9}  {:<9} {:>13} {:>11}",
            i + 1,
            data_hora(r.inicio_unix),
            cortar(&r.jogo, 30),
            duracao(r.duracao_segundos),
            r.modo,
            tamanho(r.ram_disponivel_min),
            format!("+{}", tamanho(crescimento))
        );
    }
    println!();
    println!("Detalhes de uma partida: makeway-cli sessoes 1");
}

pub fn sessao(arquivo: &Path, r: &RelatorioSessao) {
    println!(
        "{}  {}, {}, modo {}",
        r.jogo,
        data_hora(r.inicio_unix),
        duracao(r.duracao_segundos),
        r.modo
    );
    if let Some(caminho) = &r.caminho_jogo {
        println!("  Detectado pela {}: {}", r.detectado_por, caminho.display());
    }
    println!();
    println!(
        "  RAM disponível       {} no início, mínimo {} (de {})",
        tamanho(r.ram_disponivel_inicio),
        tamanho(r.ram_disponivel_min),
        tamanho(r.ram_total)
    );
    println!(
        "  Arquivo de paginação {} → {} no pico",
        tamanho(r.paginacao_inicio),
        tamanho(r.paginacao_max)
    );
    println!("  Confirmada           pico de {} de {} possíveis", tamanho(r.confirmada_max), tamanho(r.limite_confirmada));
    println!(
        "  O jogo               até {} de RAM, {}% da CPU em média",
        tamanho(r.ram_jogo_max),
        decimal(r.cpu_jogo_media_pct, 1)
    );
    println!();
    println!("Quem mais usou RAM durante a partida");
    println!("  {:<34} {:>6} {:>14} {:>10}", "App", "Proc.", "RAM máx.", "CPU média");
    for c in r.concorrentes.iter().take(12) {
        let nome = if c.protegido { format!("{} *", c.nome) } else { c.nome.clone() };
        println!(
            "  {:<34} {:>6} {:>14} {:>9}%",
            cortar(&nome, 34),
            c.processos,
            mb(c.ram_max),
            decimal(c.cpu_media_pct, 1)
        );
    }
    println!("  * protegido: o modo jogo nunca mexe nele");
    for (titulo, itens) in [("Feito", &r.alteracoes), ("Avisos", &r.avisos), ("Sugestões", &r.sugestoes)] {
        if !itens.is_empty() {
            println!();
            println!("{titulo}");
            for item in itens {
                println!("  - {item}");
            }
        }
    }
    println!();
    println!("Arquivos: {} e .csv", arquivo.display());
}

pub fn restauracao(r: &Restauracao) {
    println!("A última sessão de {} ficou pela metade. Desfeito:", r.jogo);
    for d in &r.desfeitas {
        println!("  - {d}");
    }
    if !r.problemas.is_empty() {
        println!("Não deu para desfazer (fica anotado para a próxima tentativa):");
        for p in &r.problemas {
            println!("  ! {p}");
        }
    }
}

fn pulados(p: &Pulados) {
    if p.sem_acesso == 0 && p.nuvem_arquivos == 0 {
        println!();
        return;
    }
    println!();
    if p.sem_acesso > 0 {
        let exemplo = p.exemplos_sem_acesso.first().map(|e| format!(" (ex.: {})", e.display())).unwrap_or_default();
        println!("Sem permissão de leitura: {} itens{exemplo}.", decimal(p.sem_acesso as f64, 0));
    }
    if p.nuvem_arquivos > 0 {
        println!(
            "Só na nuvem, não contados nem baixados: {} arquivos ({}).",
            decimal(p.nuvem_arquivos as f64, 0),
            tamanho(p.nuvem_bytes)
        );
    }
}

fn porcentagem(parte: u64, total: u64) -> String {
    if total == 0 {
        return String::new();
    }
    let pct = parte as f64 * 100.0 / total as f64;
    if pct < 1.0 { "<1%".into() } else { format!("{}%", decimal(pct, 0)) }
}

fn cortar(texto: &str, largura: usize) -> String {
    if texto.chars().count() <= largura {
        return texto.to_owned();
    }
    let mut curto: String = texto.chars().take(largura.saturating_sub(1)).collect();
    curto.push('…');
    curto
}
