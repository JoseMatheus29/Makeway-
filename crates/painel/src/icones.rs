//! Ícones de traço desenhados em código (grade de 24×24, como os do
//! protótipo). Nada de imagem nem emoji.

use eframe::egui::{Color32, CornerRadius, Painter, Pos2, Rect, Response, Sense, Shape, Stroke, StrokeKind, Ui, Vec2, pos2};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icone {
    Casa,
    Controle,
    Disco,
    Chip,
    Grafico,
    Check,
    CheckSimples,
    Pausa,
    Lixeira,
    Raio,
    Escudo,
    Olho,
    SetaCima,
    Lampada,
    Alerta,
    Chevron,
    ChevronBaixo,
    Fechar,
    /// Botão liga/desliga.
    Energia,
    Cadeado,
    /// Botão de gravar (bolinha cheia dentro de um círculo).
    Gravacao,
    /// Uma pessoa (cabeça e ombros).
    Conta,
}

/// Desenha o ícone ocupando `rect` (quadrado).
pub fn pintar(p: &Painter, rect: Rect, icone: Icone, cor: Color32) {
    let escala = rect.width() / 24.0;
    let o = rect.min;
    let pt = |x: f32, y: f32| -> Pos2 { pos2(o.x + x * escala, o.y + y * escala) };
    let traco = Stroke::new((1.8 * escala).max(1.2), cor);
    let linha = |pontos: &[(f32, f32)]| {
        p.add(Shape::line(pontos.iter().map(|&(x, y)| pt(x, y)).collect(), traco));
    };
    let fechado = |pontos: &[(f32, f32)]| {
        p.add(Shape::closed_line(pontos.iter().map(|&(x, y)| pt(x, y)).collect(), traco));
    };
    let retangulo = |x0: f32, y0: f32, x1: f32, y1: f32, raio: f32| {
        p.rect_stroke(Rect::from_min_max(pt(x0, y0), pt(x1, y1)), CornerRadius::same((raio * escala) as u8), traco, StrokeKind::Middle);
    };
    let circulo = |x: f32, y: f32, r: f32| p.circle_stroke(pt(x, y), r * escala, traco);
    let ponto = |x: f32, y: f32| p.circle_filled(pt(x, y), 1.3 * escala, cor);

    match icone {
        Icone::Casa => {
            linha(&[(3.0, 10.5), (12.0, 3.0), (21.0, 10.5)]);
            linha(&[(5.0, 9.0), (5.0, 20.0), (19.0, 20.0), (19.0, 9.0)]);
        }
        Icone::Controle => {
            retangulo(2.5, 6.5, 21.5, 17.5, 5.0);
            linha(&[(6.0, 12.0), (10.0, 12.0)]);
            linha(&[(8.0, 10.0), (8.0, 14.0)]);
            ponto(15.0, 12.5);
            ponto(17.8, 10.5);
        }
        Icone::Disco => {
            retangulo(2.0, 12.0, 22.0, 20.0, 2.0);
            linha(&[(2.0, 12.0), (5.5, 5.0), (18.5, 5.0), (22.0, 12.0)]);
            ponto(6.0, 16.0);
            ponto(10.0, 16.0);
        }
        Icone::Chip => {
            retangulo(5.0, 5.0, 19.0, 19.0, 2.0);
            retangulo(9.0, 9.0, 15.0, 15.0, 0.0);
            for v in [9.0, 15.0] {
                linha(&[(v, 2.0), (v, 5.0)]);
                linha(&[(v, 19.0), (v, 22.0)]);
                linha(&[(2.0, v), (5.0, v)]);
                linha(&[(19.0, v), (22.0, v)]);
            }
        }
        Icone::Grafico => {
            linha(&[(3.0, 3.0), (3.0, 21.0), (21.0, 21.0)]);
            linha(&[(7.0, 15.0), (11.0, 11.0), (14.0, 14.0), (19.0, 8.0)]);
        }
        Icone::Check => {
            circulo(12.0, 12.0, 9.0);
            linha(&[(8.0, 12.0), (11.0, 15.0), (16.0, 9.0)]);
        }
        Icone::CheckSimples => linha(&[(5.0, 12.5), (10.0, 17.0), (19.0, 7.0)]),
        Icone::Pausa => {
            retangulo(6.0, 5.0, 10.0, 19.0, 1.0);
            retangulo(14.0, 5.0, 18.0, 19.0, 1.0);
        }
        Icone::Lixeira => {
            linha(&[(3.0, 6.0), (21.0, 6.0)]);
            linha(&[(8.0, 6.0), (8.0, 4.0), (16.0, 4.0), (16.0, 6.0)]);
            linha(&[(6.0, 6.0), (7.0, 20.0), (17.0, 20.0), (18.0, 6.0)]);
        }
        Icone::Raio => fechado(&[(13.0, 2.0), (4.0, 14.0), (11.0, 14.0), (10.0, 22.0), (19.0, 10.0), (12.0, 10.0)]),
        Icone::Escudo => {
            fechado(&[(12.0, 3.0), (20.0, 6.0), (20.0, 12.0), (17.5, 17.5), (12.0, 21.0), (6.5, 17.5), (4.0, 12.0), (4.0, 6.0)]);
            linha(&[(9.0, 12.0), (11.0, 14.0), (15.0, 10.0)]);
        }
        Icone::Olho => {
            fechado(&[(2.0, 12.0), (5.0, 8.0), (9.0, 5.8), (15.0, 5.8), (19.0, 8.0), (22.0, 12.0), (19.0, 16.0), (15.0, 18.2), (9.0, 18.2), (5.0, 16.0)]);
            circulo(12.0, 12.0, 3.0);
        }
        Icone::SetaCima => {
            linha(&[(12.0, 19.0), (12.0, 5.0)]);
            linha(&[(5.0, 12.0), (12.0, 5.0), (19.0, 12.0)]);
        }
        Icone::Lampada => {
            linha(&[(9.0, 18.0), (15.0, 18.0)]);
            linha(&[(10.0, 21.0), (14.0, 21.0)]);
            linha(&[(9.5, 15.5), (8.0, 13.5), (6.5, 10.5), (7.0, 6.5), (9.5, 3.8), (12.0, 3.0), (14.5, 3.8), (17.0, 6.5), (17.5, 10.5), (16.0, 13.5), (14.5, 15.5)]);
        }
        Icone::Alerta => {
            fechado(&[(12.0, 3.0), (22.0, 20.0), (2.0, 20.0)]);
            linha(&[(12.0, 9.5), (12.0, 14.0)]);
            ponto(12.0, 17.0);
        }
        Icone::Chevron => linha(&[(9.0, 6.0), (15.0, 12.0), (9.0, 18.0)]),
        Icone::ChevronBaixo => linha(&[(6.0, 9.0), (12.0, 15.0), (18.0, 9.0)]),
        Icone::Fechar => {
            linha(&[(6.0, 6.0), (18.0, 18.0)]);
            linha(&[(18.0, 6.0), (6.0, 18.0)]);
        }
        Icone::Energia => {
            linha(&[(12.0, 3.0), (12.0, 12.0)]);
            // Arco aberto em cima, de ~-50° a 230°.
            let arco: Vec<(f32, f32)> = (0..=24)
                .map(|i| {
                    let a = (40.0 + i as f32 * (280.0 / 24.0)).to_radians();
                    (12.0 + 8.0 * a.sin(), 13.0 - 8.0 * a.cos())
                })
                .collect();
            linha(&arco);
        }
        Icone::Gravacao => {
            circulo(12.0, 12.0, 9.0);
            p.circle_filled(pt(12.0, 12.0), 4.0 * escala, cor);
        }
        Icone::Conta => {
            circulo(12.0, 8.0, 4.0);
            linha(&[(4.0, 21.0), (4.5, 18.0), (6.5, 15.0), (9.5, 13.6), (14.5, 13.6), (17.5, 15.0), (19.5, 18.0), (20.0, 21.0)]);
        }
        Icone::Cadeado => {
            retangulo(5.0, 11.0, 19.0, 21.0, 1.0);
            linha(&[(8.0, 11.0), (8.0, 7.5), (9.5, 4.5), (12.0, 3.5), (14.5, 4.5), (16.0, 7.5), (16.0, 11.0)]);
        }
    }
}

/// Reserva espaço e desenha o ícone.
pub fn icone(ui: &mut Ui, icone: Icone, tamanho: f32, cor: Color32) -> Response {
    let (rect, resposta) = ui.allocate_exact_size(Vec2::splat(tamanho), Sense::hover());
    pintar(ui.painter(), rect, icone, cor);
    resposta
}

/// O logo do Makeway: quadrado de cantos cortados com duas setas.
pub fn logo(ui: &mut Ui, tamanho: f32, cor: Color32, cor_setas: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(tamanho), Sense::hover());
    let p = ui.painter();
    p.add(crate::componentes::forma_cortada(rect, tamanho * 0.21, cor, Stroke::NONE));
    let dentro = Rect::from_center_size(rect.center(), Vec2::splat(tamanho * 0.63));
    p.extend(crate::componentes::setas(dentro, cor_setas, 3.0));
}
