//! Todas as chamadas `unsafe` à API do Windows ficam neste módulo,
//! embrulhadas em funções seguras. Revisar este arquivo a cada fase.
//!
//! - Memória do sistema: `GlobalMemoryStatusEx` e `GetPerformanceInfo`
//!   (crate `windows`), os mesmos números do Gerenciador de Tarefas.
//! - Processos e arquivo de paginação: `NtQuerySystemInformation`
//!   (crate `ntapi`). Uma chamada só devolve todos os processos com o
//!   conjunto de trabalho privado, que é a coluna "Memória" do
//!   Gerenciador de Tarefas.

use std::ffi::OsString;
use std::fs::File;
use std::io;
use std::mem::{offset_of, size_of};
use std::os::windows::ffi::OsStringExt;
use std::os::windows::io::AsRawHandle;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use ntapi::ntexapi::{
    NtQuerySystemInformation, SYSTEM_INFORMATION_CLASS, SYSTEM_MEMORY_LIST_INFORMATION,
    SYSTEM_PAGEFILE_INFORMATION, SYSTEM_PROCESS_INFORMATION, SYSTEM_THREAD_INFORMATION,
    SystemMemoryListInformation, SystemPageFileInformation, SystemProcessInformation,
};
use ntapi::ntpsapi::{NtResumeProcess, NtSuspendProcess};
use ntapi::ntrtl::RtlNtStatusToDosError;
use ntapi::winapi::shared::ntdef::UNICODE_STRING;
use windows::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, ERROR_SUCCESS, GetLastError, HANDLE, HLOCAL, HWND, LPARAM,
    LocalFree, WPARAM,
};
use windows::Win32::Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation};
use windows::Win32::Storage::FileSystem::{BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle};
use windows::Win32::System::Console::{
    CTRL_CLOSE_EVENT, CTRL_LOGOFF_EVENT, CTRL_SHUTDOWN_EVENT, SetConsoleCtrlHandler,
};
use windows::Win32::System::Power::{
    GetSystemPowerStatus, PowerGetActiveScheme, PowerSetActiveScheme, SYSTEM_POWER_STATUS,
};
use windows::Win32::System::ProcessStatus::{
    EmptyWorkingSet, GetPerformanceInfo, PERFORMANCE_INFORMATION,
};
use windows::Win32::System::SystemInformation::{
    GetPhysicallyInstalledSystemMemory, GlobalMemoryStatusEx, MEMORYSTATUSEX,
};
use windows::Win32::System::Threading::{
    CreateMutexW, GetCurrentProcess, GetPriorityClass, OpenMutexW, OpenProcess, OpenProcessToken,
    PROCESS_ACCESS_RIGHTS, PROCESS_CREATION_FLAGS, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_INFORMATION, PROCESS_SET_QUOTA,
    PROCESS_SUSPEND_RESUME, QueryFullProcessImageNameW, SYNCHRONIZATION_SYNCHRONIZE,
    SetPriorityClass,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GW_OWNER, GetWindow, GetWindowThreadProcessId, IsWindowVisible, PostMessageW,
    WM_CLOSE,
};
use windows::core::{BOOL, GUID, HSTRING, PWSTR};

use crate::ram::{ArquivoPaginacao, ListasMemoria, MemoriaSistema, Processo};

const STATUS_INFO_LENGTH_MISMATCH: i32 = 0xC000_0004_u32 as i32;

/// Memória do sistema e o tamanho da página (para converter páginas em bytes).
pub fn memoria_sistema() -> io::Result<(MemoriaSistema, u64)> {
    let mut estado = MEMORYSTATUSEX {
        dwLength: size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    // SAFETY: `estado` é uma struct válida com `dwLength` preenchido.
    unsafe { GlobalMemoryStatusEx(&mut estado) }.map_err(io::Error::from)?;

    let mut perf = PERFORMANCE_INFORMATION {
        cb: size_of::<PERFORMANCE_INFORMATION>() as u32,
        ..Default::default()
    };
    // SAFETY: `perf` é uma struct válida e `cb` é o tamanho dela.
    unsafe { GetPerformanceInfo(&mut perf, perf.cb) }.map_err(io::Error::from)?;

    let mut instalada_kb = 0u64;
    // SAFETY: ponteiro para um u64 válido. Falha em algumas VMs sem
    // tabela SMBIOS; nesse caso o valor fica de fora.
    let instalada = unsafe { GetPhysicallyInstalledSystemMemory(&mut instalada_kb) }
        .ok()
        .map(|()| instalada_kb * 1024);

    let pagina = perf.PageSize as u64;
    let memoria = MemoriaSistema {
        instalada,
        total: estado.ullTotalPhys,
        disponivel: estado.ullAvailPhys,
        em_uso: estado.ullTotalPhys.saturating_sub(estado.ullAvailPhys),
        carga_pct: estado.dwMemoryLoad,
        confirmada: perf.CommitTotal as u64 * pagina,
        limite_confirmada: perf.CommitLimit as u64 * pagina,
        // Funciona sem administrador no Windows 11; se algum sistema negar,
        // o relatório sai sem essa parte em vez de falhar inteiro.
        listas: listas_memoria(pagina).ok(),
        pool_paginado: perf.KernelPaged as u64 * pagina,
        pool_nao_paginado: perf.KernelNonpaged as u64 * pagina,
    };
    Ok((memoria, pagina))
}

/// Todos os processos em execução, menos o "Processo Ocioso do Sistema" (PID 0).
pub fn listar_processos() -> io::Result<Vec<Processo>> {
    let buffer = consultar_sistema(SystemProcessInformation, 512 * 1024)?;
    let mut processos = Vec::with_capacity(400);
    for entrada in Entradas::new(&buffer, offset_of!(SYSTEM_PROCESS_INFORMATION, Threads)) {
        // SAFETY: `Entradas` só devolve deslocamentos alinhados com espaço
        // para os campos lidos, dentro do buffer preenchido pelo kernel.
        let info = unsafe { &*(entrada as *const SYSTEM_PROCESS_INFORMATION) };
        let pid = info.UniqueProcessId as usize as u32;
        if pid == 0 {
            continue;
        }
        // SAFETY: LARGE_INTEGER é uma union; QuadPart é o valor inteiro.
        let (ws_privado, criado_em, usuario, kernel) = unsafe {
            (
                *info.WorkingSetPrivateSize.QuadPart(),
                *info.CreateTime.QuadPart(),
                *info.UserTime.QuadPart(),
                *info.KernelTime.QuadPart(),
            )
        };
        processos.push(Processo {
            pid,
            pid_pai: info.InheritedFromUniqueProcessId as usize as u32,
            // SAFETY: o nome aponta para dentro do mesmo buffer.
            nome: unsafe { texto(&info.ImageName) },
            ws_privado: ws_privado.max(0) as u64,
            working_set: info.WorkingSetSize as u64,
            confirmada: info.PrivatePageCount as u64,
            sessao: info.SessionId,
            criado_em,
            cpu_100ns: (usuario.max(0) + kernel.max(0)) as u64,
            suspenso: todas_threads_suspensas(info, entrada, &buffer),
        });
    }
    Ok(processos)
}

/// `true` se todas as threads do processo estão paradas por suspensão.
fn todas_threads_suspensas(info: &SYSTEM_PROCESS_INFORMATION, entrada: *const u8, buffer: &Buffer) -> bool {
    const ESPERANDO: u32 = 5; // KTHREAD_STATE::Waiting
    const SUSPENSO: u32 = 5; // KWAIT_REASON::Suspended
    const WR_SUSPENSO: u32 = 12; // KWAIT_REASON::WrSuspended
    let quantas = info.NumberOfThreads as usize;
    let deslocamento = entrada as usize - buffer.dados.as_ptr() as usize;
    let fim = deslocamento
        + offset_of!(SYSTEM_PROCESS_INFORMATION, Threads)
        + quantas * size_of::<SYSTEM_THREAD_INFORMATION>();
    if quantas == 0 || fim > buffer.usados {
        return false;
    }
    // SAFETY: as `quantas` threads cabem no buffer, conferido acima.
    let threads = unsafe { std::slice::from_raw_parts(info.Threads.as_ptr(), quantas) };
    threads
        .iter()
        .all(|t| t.ThreadState == ESPERANDO && matches!(t.WaitReason, SUSPENSO | WR_SUSPENSO))
}

/// Handle do Windows, fechado sozinho ao sair de escopo.
struct Handle(HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: o handle é nosso e ainda está aberto.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

fn abrir_processo(pid: u32, acesso: PROCESS_ACCESS_RIGHTS) -> io::Result<Handle> {
    // SAFETY: chamada simples; o handle devolvido é fechado pelo Drop.
    unsafe { OpenProcess(acesso, false, pid) }.map(Handle).map_err(io::Error::from)
}

/// Pausa todas as threads do processo (como o Monitor de Recursos faz).
pub fn pausar_processo(pid: u32) -> io::Result<()> {
    let h = abrir_processo(pid, PROCESS_SUSPEND_RESUME)?;
    // SAFETY: handle válido com o acesso pedido.
    let status = unsafe { NtSuspendProcess(h.0.0 as _) };
    if status < 0 { Err(erro_nt(status)) } else { Ok(()) }
}

pub fn retomar_processo(pid: u32) -> io::Result<()> {
    let h = abrir_processo(pid, PROCESS_SUSPEND_RESUME)?;
    // SAFETY: handle válido com o acesso pedido.
    let status = unsafe { NtResumeProcess(h.0.0 as _) };
    if status < 0 { Err(erro_nt(status)) } else { Ok(()) }
}

/// Devolve ao sistema a RAM física do processo. As páginas vão para a
/// lista standby/modificada e voltam se o processo precisar delas.
pub fn esvaziar_memoria(pid: u32) -> io::Result<()> {
    let h = abrir_processo(pid, PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SET_QUOTA)?;
    // SAFETY: handle válido com o acesso pedido.
    unsafe { EmptyWorkingSet(h.0) }.map_err(io::Error::from)
}

pub fn prioridade(pid: u32) -> io::Result<u32> {
    let h = abrir_processo(pid, PROCESS_QUERY_LIMITED_INFORMATION)?;
    // SAFETY: handle válido com o acesso pedido.
    match unsafe { GetPriorityClass(h.0) } {
        0 => Err(io::Error::last_os_error()),
        classe => Ok(classe),
    }
}

pub fn definir_prioridade(pid: u32, classe: u32) -> io::Result<()> {
    let h = abrir_processo(pid, PROCESS_SET_INFORMATION)?;
    // SAFETY: handle válido com o acesso pedido.
    unsafe { SetPriorityClass(h.0, PROCESS_CREATION_FLAGS(classe)) }.map_err(io::Error::from)
}

/// Caminho completo do .exe.
pub fn caminho_processo(pid: u32) -> io::Result<PathBuf> {
    let h = abrir_processo(pid, PROCESS_QUERY_LIMITED_INFORMATION)?;
    let mut texto = vec![0u16; 32_768];
    let mut tamanho = texto.len() as u32;
    // SAFETY: buffer gravável com `tamanho` caracteres.
    unsafe { QueryFullProcessImageNameW(h.0, PROCESS_NAME_WIN32, PWSTR(texto.as_mut_ptr()), &mut tamanho) }
        .map_err(io::Error::from)?;
    Ok(PathBuf::from(OsString::from_wide(&texto[..tamanho as usize])))
}

/// Pede para as janelas principais do processo fecharem, como clicar no X.
/// O app pode perguntar se quer salvar. Devolve quantas janelas recebeu o pedido.
pub fn fechar_janelas(pid: u32) -> usize {
    struct Busca {
        pid: u32,
        janelas: Vec<HWND>,
    }
    unsafe extern "system" fn coletar(janela: HWND, contexto: LPARAM) -> BOOL {
        // SAFETY: `contexto` é o ponteiro para `Busca` passado abaixo,
        // válido durante todo o EnumWindows.
        let busca = unsafe { &mut *(contexto.0 as *mut Busca) };
        let mut pid = 0;
        // SAFETY: janela válida durante a enumeração.
        unsafe { GetWindowThreadProcessId(janela, Some(&mut pid)) };
        let principal = pid == busca.pid
            && unsafe { IsWindowVisible(janela) }.as_bool()
            && unsafe { GetWindow(janela, GW_OWNER) }.is_err(); // sem janela dona
        if principal {
            busca.janelas.push(janela);
        }
        true.into()
    }
    let mut busca = Busca { pid, janelas: Vec::new() };
    // SAFETY: o callback só usa `busca`, que vive até o fim da chamada.
    let _ = unsafe { EnumWindows(Some(coletar), LPARAM(&mut busca as *mut Busca as isize)) };
    busca
        .janelas
        .iter()
        // SAFETY: PostMessage não bloqueia e aceita janela que já fechou.
        .filter(|&&janela| unsafe { PostMessageW(Some(janela), WM_CLOSE, WPARAM(0), LPARAM(0)) }.is_ok())
        .count()
}

/// GUID do plano de energia ativo, como número.
pub fn plano_energia_ativo() -> io::Result<u128> {
    let mut guid: *mut GUID = std::ptr::null_mut();
    // SAFETY: o Windows aloca o GUID; liberamos com LocalFree.
    let erro = unsafe { PowerGetActiveScheme(None, &mut guid) };
    if erro != ERROR_SUCCESS || guid.is_null() {
        return Err(io::Error::from_raw_os_error(erro.0 as i32));
    }
    // SAFETY: ponteiro válido devolvido pelo Windows.
    let valor = unsafe { (*guid).to_u128() };
    // SAFETY: memória alocada pelo PowerGetActiveScheme.
    unsafe { LocalFree(Some(HLOCAL(guid.cast()))) };
    Ok(valor)
}

pub fn definir_plano_energia(plano: u128) -> io::Result<()> {
    let guid = GUID::from_u128(plano);
    // SAFETY: ponteiro para um GUID válido na pilha.
    let erro = unsafe { PowerSetActiveScheme(None, Some(&guid)) };
    if erro != ERROR_SUCCESS {
        return Err(io::Error::from_raw_os_error(erro.0 as i32));
    }
    Ok(())
}

/// `Some(true)` na tomada, `Some(false)` na bateria, `None` se o Windows não souber.
pub fn na_tomada() -> Option<bool> {
    let mut estado = SYSTEM_POWER_STATUS::default();
    // SAFETY: struct gravável.
    unsafe { GetSystemPowerStatus(&mut estado) }.ok()?;
    match estado.ACLineStatus {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    }
}

/// Mutex nomeado que garante uma instância só. Solto ao sair de escopo.
pub struct InstanciaUnica(#[allow(dead_code)] Handle);

/// `Ok(None)` se outra instância já tem o mutex.
pub fn instancia_unica(nome: &str) -> io::Result<Option<InstanciaUnica>> {
    let nome = HSTRING::from(nome);
    // SAFETY: nome válido; o handle é fechado pelo Drop.
    let h = Handle(unsafe { CreateMutexW(None, false, &nome) }.map_err(io::Error::from)?);
    // SAFETY: lê o erro deixado pelo CreateMutexW logo acima.
    let ja_existia = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
    Ok(if ja_existia { None } else { Some(InstanciaUnica(h)) })
}

/// `true` se alguma instância está segurando o mutex agora.
pub fn instancia_rodando(nome: &str) -> bool {
    let nome = HSTRING::from(nome);
    // SAFETY: nome válido; o handle é fechado pelo Drop.
    unsafe { OpenMutexW(SYNCHRONIZATION_SYNCHRONIZE, false, &nome) }.map(Handle).is_ok()
}

pub fn e_administrador() -> bool {
    let mut token = HANDLE::default();
    // SAFETY: pseudo-handle do próprio processo; token fechado pelo Drop.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }.is_err() {
        return false;
    }
    let token = Handle(token);
    let mut elevacao = TOKEN_ELEVATION::default();
    let mut tamanho = 0;
    // SAFETY: buffer com o tamanho exato de TOKEN_ELEVATION.
    let ok = unsafe {
        GetTokenInformation(
            token.0,
            TokenElevation,
            Some((&mut elevacao as *mut TOKEN_ELEVATION).cast()),
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut tamanho,
        )
    };
    ok.is_ok() && elevacao.TokenIsElevated != 0
}

static PEDIU_PARA_SAIR: AtomicBool = AtomicBool::new(false);
static LIMPEZA_FEITA: AtomicBool = AtomicBool::new(false);

/// Troca o "mata na hora" do Ctrl+C e do X da janela do console por um
/// pedido educado: liga a flag devolvida, e o programa sai do loop e
/// desfaz tudo. Ao fechar a janela o Windows só espera uns 5 s, então
/// o handler segura até `avisar_limpeza_feita` ou 4,5 s.
pub fn tratar_ctrl_c() -> &'static AtomicBool {
    unsafe extern "system" fn ao_sinal(tipo: u32) -> BOOL {
        PEDIU_PARA_SAIR.store(true, Ordering::SeqCst);
        if matches!(tipo, CTRL_CLOSE_EVENT | CTRL_LOGOFF_EVENT | CTRL_SHUTDOWN_EVENT) {
            let inicio = std::time::Instant::now();
            while !LIMPEZA_FEITA.load(Ordering::SeqCst) && inicio.elapsed().as_millis() < 4500 {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
        true.into()
    }
    // SAFETY: a função vive o programa inteiro.
    let _ = unsafe { SetConsoleCtrlHandler(Some(ao_sinal), true) };
    &PEDIU_PARA_SAIR
}

pub fn avisar_limpeza_feita() {
    LIMPEZA_FEITA.store(true, Ordering::SeqCst);
}

/// Arquivos de paginação. Lista vazia se o Windows estiver sem nenhum.
pub fn arquivos_paginacao(pagina: u64) -> io::Result<Vec<ArquivoPaginacao>> {
    let buffer = consultar_sistema(SystemPageFileInformation, 4 * 1024)?;
    let mut arquivos = Vec::new();
    for entrada in Entradas::new(&buffer, size_of::<SYSTEM_PAGEFILE_INFORMATION>()) {
        // SAFETY: mesma garantia de `listar_processos`.
        let info = unsafe { &*(entrada as *const SYSTEM_PAGEFILE_INFORMATION) };
        // SAFETY: o nome aponta para dentro do mesmo buffer.
        let nome = unsafe { texto(&info.PageFileName) };
        arquivos.push(ArquivoPaginacao {
            caminho: nome.strip_prefix(r"\??\").unwrap_or(&nome).to_owned(),
            tamanho: info.TotalSize as u64 * pagina,
            em_uso: info.TotalInUse as u64 * pagina,
            pico: info.PeakUsage as u64 * pagina,
        });
    }
    Ok(arquivos)
}

/// Listas de páginas da RAM física: o que o Gerenciador de Tarefas chama
/// de "Em cache" (standby + modificada) e "Livre".
pub fn listas_memoria(pagina: u64) -> io::Result<ListasMemoria> {
    let mut info: SYSTEM_MEMORY_LIST_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: struct POD zerada, com o tamanho informado.
    let status = unsafe {
        NtQuerySystemInformation(
            SystemMemoryListInformation,
            (&mut info as *mut SYSTEM_MEMORY_LIST_INFORMATION).cast(),
            size_of::<SYSTEM_MEMORY_LIST_INFORMATION>() as u32,
            std::ptr::null_mut(),
        )
    };
    if status < 0 {
        return Err(erro_nt(status));
    }
    let paginas = |n: usize| n as u64 * pagina;
    Ok(ListasMemoria {
        standby: paginas(info.PageCountByPriority.iter().sum()),
        modificada: paginas(info.ModifiedPageCount),
        livre: paginas(info.FreePageCount + info.ZeroPageCount),
    })
}

fn erro_nt(status: i32) -> io::Error {
    // SAFETY: função pura de conversão de código de erro.
    let codigo = unsafe { RtlNtStatusToDosError(status) };
    io::Error::from_raw_os_error(codigo as i32)
}

/// (série do volume, índice do arquivo). Dois nomes com a mesma identidade
/// são o mesmo arquivo (hard link), não uma cópia.
pub fn identidade_arquivo(arquivo: &File) -> io::Result<(u32, u64)> {
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: o handle vale enquanto `arquivo` existir; `info` é gravável.
    unsafe { GetFileInformationByHandle(HANDLE(arquivo.as_raw_handle()), &mut info) }
        .map_err(io::Error::from)?;
    let indice = (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow);
    Ok((info.dwVolumeSerialNumber, indice))
}

/// Chama `NtQuerySystemInformation`, aumentando o buffer até caber.
/// O buffer é de `u64` para ficar alinhado a 8 bytes, como as structs pedem.
fn consultar_sistema(classe: SYSTEM_INFORMATION_CLASS, tamanho_inicial: usize) -> io::Result<Buffer> {
    let mut dados: Vec<u64> = vec![0; tamanho_inicial.div_ceil(8)];
    loop {
        let capacidade = u32::try_from(dados.len() * 8).unwrap_or(u32::MAX);
        let mut necessario = 0u32;
        // SAFETY: o ponteiro aponta para `capacidade` bytes graváveis.
        let status = unsafe {
            NtQuerySystemInformation(classe, dados.as_mut_ptr().cast(), capacidade, &mut necessario)
        };
        if status == STATUS_INFO_LENGTH_MISMATCH {
            // Folga de 64 KB: processos podem abrir entre uma chamada e outra.
            let novo = (necessario as usize).max(dados.len() * 8) + 64 * 1024;
            dados = vec![0; novo.div_ceil(8)];
            continue;
        }
        if status < 0 {
            return Err(erro_nt(status));
        }
        let usados = (necessario as usize).min(dados.len() * 8);
        return Ok(Buffer { dados, usados });
    }
}

struct Buffer {
    dados: Vec<u64>,
    usados: usize,
}

/// Percorre entradas encadeadas por `NextEntryOffset` (sempre o primeiro
/// campo, um u32), conferindo que cada uma cabe dentro do buffer.
struct Entradas<'a> {
    buffer: &'a Buffer,
    proxima: Option<usize>,
    tamanho_minimo: usize,
}

impl<'a> Entradas<'a> {
    fn new(buffer: &'a Buffer, tamanho_minimo: usize) -> Self {
        let proxima = (buffer.usados > 0).then_some(0);
        Self { buffer, proxima, tamanho_minimo }
    }
}

impl Iterator for Entradas<'_> {
    type Item = *const u8;

    fn next(&mut self) -> Option<*const u8> {
        let deslocamento = self.proxima?;
        if deslocamento % 8 != 0 || deslocamento + self.tamanho_minimo > self.buffer.usados {
            self.proxima = None;
            return None;
        }
        // SAFETY: deslocamento conferido acima, dentro do buffer.
        let entrada = unsafe { (self.buffer.dados.as_ptr() as *const u8).add(deslocamento) };
        // SAFETY: o u32 inicial cabe (tamanho_minimo >= 4) e está alinhado.
        let salto = unsafe { *(entrada as *const u32) } as usize;
        self.proxima = (salto != 0).then_some(deslocamento + salto);
        Some(entrada)
    }
}

/// Converte um UNICODE_STRING do kernel em String.
///
/// SAFETY: `Buffer` precisa ser nulo ou apontar para `Length` bytes válidos.
unsafe fn texto(s: &UNICODE_STRING) -> String {
    if s.Buffer.is_null() || s.Length == 0 {
        return String::new();
    }
    // SAFETY: garantido por quem chama.
    let unidades = unsafe { std::slice::from_raw_parts(s.Buffer, s.Length as usize / 2) };
    String::from_utf16_lossy(unidades)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn le_a_memoria_do_sistema() {
        let (memoria, pagina) = memoria_sistema().unwrap();
        assert!(pagina >= 4096);
        assert!(memoria.total > 0);
        assert!(memoria.disponivel <= memoria.total);
        assert!(memoria.limite_confirmada >= memoria.confirmada);
    }

    #[test]
    fn lista_processos_incluindo_este() {
        let processos = listar_processos().unwrap();
        let eu = std::process::id();
        let atual = processos.iter().find(|p| p.pid == eu).expect("processo atual");
        assert!(atual.nome.to_lowercase().ends_with(".exe"));
        assert!(atual.ws_privado > 0);
        assert!(atual.working_set >= atual.ws_privado);
    }

    #[test]
    fn le_listas_de_memoria() {
        let (memoria, pagina) = memoria_sistema().unwrap();
        let listas = listas_memoria(pagina).unwrap();
        // Disponível = standby + livre. As duas leituras não são no mesmo
        // instante, então aceita uma diferença pequena.
        let soma = listas.standby + listas.livre;
        assert!(soma.abs_diff(memoria.disponivel) < 256 << 20, "{listas:?} vs {}", memoria.disponivel);
    }

    #[test]
    fn le_arquivos_de_paginacao() {
        let (_, pagina) = memoria_sistema().unwrap();
        for arquivo in arquivos_paginacao(pagina).unwrap() {
            assert!(!arquivo.caminho.starts_with(r"\??\"));
            assert!(arquivo.em_uso <= arquivo.tamanho);
        }
    }
}
