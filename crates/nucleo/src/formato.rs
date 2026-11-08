//! Números no formato brasileiro (1.234,5 MB), em base 1024 como o
//! Gerenciador de Tarefas.

const MB: f64 = 1024.0 * 1024.0;
const GB: f64 = MB * 1024.0;

/// Sempre em MB, com uma casa: "1.234,5 MB". Igual à aba Processos.
pub fn mb(bytes: u64) -> String {
    format!("{} MB", decimal(bytes as f64 / MB, 1))
}

/// Sempre em GB, com uma casa: "15,8 GB".
pub fn gb(bytes: u64) -> String {
    format!("{} GB", decimal(bytes as f64 / GB, 1))
}

/// GB a partir de 1 GB, senão MB.
pub fn tamanho(bytes: u64) -> String {
    if bytes as f64 >= GB { gb(bytes) } else { mb(bytes) }
}

/// Segundos desde 1970 → "08/11/2026", no fuso do Windows.
pub fn data(unix: i64) -> String {
    local(unix).map(|d| d.format("%d/%m/%Y").to_string()).unwrap_or_default()
}

/// Segundos desde 1970 → "08/11/2026 21:30", no fuso do Windows.
pub fn data_hora(unix: i64) -> String {
    local(unix).map(|d| d.format("%d/%m/%Y %H:%M").to_string()).unwrap_or_default()
}

/// Hora atual "21:30:05", para os registros do vigia.
pub fn hora_agora() -> String {
    chrono::Local::now().format("%d/%m %H:%M:%S").to_string()
}

/// Duração legível: "45 s", "12 min", "1 h 05 min".
pub fn duracao(segundos: u64) -> String {
    match segundos {
        0..60 => format!("{segundos} s"),
        60..3600 => format!("{} min", segundos / 60),
        _ => format!("{} h {:02} min", segundos / 3600, segundos % 3600 / 60),
    }
}

fn local(unix: i64) -> Option<chrono::DateTime<chrono::Local>> {
    Some(chrono::DateTime::from_timestamp(unix, 0)?.with_timezone(&chrono::Local))
}

/// "1234.56" com `casas` casas vira "1.234,56".
pub fn decimal(valor: f64, casas: usize) -> String {
    let texto = format!("{valor:.casas$}");
    let (inteiro, fracao) = texto.split_once('.').unwrap_or((&texto, ""));
    let mut com_pontos = String::with_capacity(inteiro.len() + inteiro.len() / 3);
    for (i, c) in inteiro.chars().enumerate() {
        if i > 0 && (inteiro.len() - i) % 3 == 0 {
            com_pontos.push('.');
        }
        com_pontos.push(c);
    }
    if fracao.is_empty() { com_pontos } else { format!("{com_pontos},{fracao}") }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn separadores_brasileiros() {
        assert_eq!(decimal(0.0, 1), "0,0");
        assert_eq!(decimal(999.94, 1), "999,9");
        assert_eq!(decimal(1234.5, 1), "1.234,5");
        assert_eq!(decimal(1234567.0, 0), "1.234.567");
        assert_eq!(decimal(100.0, 0), "100");
    }

    #[test]
    fn unidades() {
        assert_eq!(mb(1536 * 1024), "1,5 MB");
        assert_eq!(mb(3 << 30), "3.072,0 MB");
        assert_eq!(gb(16 << 30), "16,0 GB");
        assert_eq!(tamanho(512 << 20), "512,0 MB");
        assert_eq!(tamanho(2 << 30), "2,0 GB");
    }
}
