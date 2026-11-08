//! O ícone do Makeway desenhado em código (sem arquivo de imagem): um
//! quadrado de cantos cortados com duas setas escuras para a frente,
//! "abrindo caminho". A cor mostra o estado do vigia perto do relógio.

/// Verde-limão da marca: otimizando (e a janela).
pub const LIME: [u8; 3] = [198, 255, 58];
/// Esperando um jogo abrir.
pub const CIANO: [u8; 3] = [79, 216, 247];
/// Observando uma partida.
pub const AMBAR: [u8; 3] = [255, 176, 32];
/// Modo jogo desligado.
pub const CINZA: [u8; 3] = [91, 100, 116];

/// Cor das setas, quase preta.
const SETAS: [u8; 3] = [10, 12, 8];

/// Imagem RGBA de `lado` x `lado` pixels.
pub fn rgba(lado: u32, cor: [u8; 3]) -> Vec<u8> {
    const AMOSTRAS: u32 = 4; // 4x4 por pixel, para bordas suaves
    let mut imagem = Vec::with_capacity((lado * lado * 4) as usize);
    let escala = 2.0 / lado as f32;
    let s = 0.94; // meio lado do quadrado
    let corte = 0.42 * s; // cantos cortados: em cima à esquerda e embaixo à direita
    // As setas ocupam 63% do quadrado, na grade de 24 do protótipo.
    let caixa = 0.63 * s;
    let p = |x: f32, y: f32| ((x / 24.0 - 0.5) * 2.0 * caixa, (y / 24.0 - 0.5) * 2.0 * caixa);
    let espessura = 3.0 / 24.0 * 2.0 * caixa;
    let setas = [[p(5.0, 5.0), p(12.0, 12.0), p(5.0, 19.0)], [p(12.0, 5.0), p(19.0, 12.0), p(12.0, 19.0)]];
    for y in 0..lado {
        for x in 0..lado {
            let (mut dentro, mut escuro) = (0.0f32, 0.0f32);
            for sy in 0..AMOSTRAS {
                for sx in 0..AMOSTRAS {
                    let u = (x as f32 + (sx as f32 + 0.5) / AMOSTRAS as f32) * escala - 1.0;
                    let v = (y as f32 + (sy as f32 + 0.5) / AMOSTRAS as f32) * escala - 1.0;
                    let no_quadrado = u.abs() <= s && v.abs() <= s;
                    let corte_de_cima = (u + s) + (v + s) < corte;
                    let corte_de_baixo = (s - u) + (s - v) < corte;
                    if no_quadrado && !corte_de_cima && !corte_de_baixo {
                        dentro += 1.0;
                        let perto = setas.iter().any(|t| {
                            distancia(u, v, t[0], t[1]) < espessura / 2.0 || distancia(u, v, t[1], t[2]) < espessura / 2.0
                        });
                        if perto {
                            escuro += 1.0;
                        }
                    }
                }
            }
            let total = (AMOSTRAS * AMOSTRAS) as f32;
            let (dentro, escuro) = (dentro / total, escuro / total);
            if dentro == 0.0 {
                imagem.extend_from_slice(&[0, 0, 0, 0]);
                continue;
            }
            let mistura = escuro / dentro;
            for (canal, seta) in cor.into_iter().zip(SETAS) {
                imagem.push((canal as f32 * (1.0 - mistura) + seta as f32 * mistura).round() as u8);
            }
            imagem.push((dentro * 255.0).round() as u8);
        }
    }
    imagem
}

fn distancia(px: f32, py: f32, (ax, ay): (f32, f32), (bx, by): (f32, f32)) -> f32 {
    let (dx, dy) = (bx - ax, by - ay);
    let t = (((px - ax) * dx + (py - ay) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
    let (cx, cy) = (ax + t * dx, ay + t * dy);
    ((px - cx).powi(2) + (py - cy).powi(2)).sqrt()
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn quadrado_colorido_com_cantos_cortados() {
        let img = rgba(32, LIME);
        assert_eq!(img.len(), 32 * 32 * 4);
        let pixel = |x: usize, y: usize| &img[(y * 32 + x) * 4..(y * 32 + x) * 4 + 4];
        assert_eq!(pixel(0, 0)[3], 0); // canto cortado em cima à esquerda
        assert_eq!(pixel(31, 31)[3], 0); // e embaixo à direita
        assert_eq!(pixel(30, 2)[3], 255); // o outro canto fica inteiro
        assert_eq!(pixel(16, 4), &[198, 255, 58, 255]); // cor cheia fora das setas
        let seta = pixel(15, 15); // na ponta da primeira seta
        assert!(seta[0] < 100 && seta[1] < 100, "seta escura: {seta:?}");
    }
}
