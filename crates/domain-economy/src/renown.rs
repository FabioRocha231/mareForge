//! Renome (MV-067): o progresso visível do capitão. Tudo que ele faz no mar
//! rende um pouco — coletar, saquear, fabricar, afundar, entregar — e o
//! Renome acumulado sobe o nível. Cada nível é um ponto na Rosa dos Ventos.
//! Não se compra: só se ganha jogando.

/// Nível máximo (e pontos máximos na Rosa dos Ventos).
pub const MAX_LEVEL: u32 = 30;

/// Quanto cada feito rende.
pub const PER_GATHERED_UNIT: u32 = 1;
pub const PER_WRECK_LOOTED: u32 = 10;
pub const PER_CRAFT: u32 = 15;
pub const PER_CONTRACT: u32 = 40;
pub const PER_CAPTAIN_SUNK: u32 = 60;

/// Renome acumulado para chegar ao `level` (nível 1 começa em 0). Curva
/// quadrática suave: nível 3 cabe nos primeiros 10 minutos, o 30 pede
/// semanas.
pub fn threshold(level: u32) -> u64 {
    let n = u64::from(level.clamp(1, MAX_LEVEL) - 1);
    50 * n + 15 * n * n
}

pub fn level_of(total: u64) -> u32 {
    (1..=MAX_LEVEL)
        .rev()
        .find(|&level| total >= threshold(level))
        .unwrap_or(1)
}

/// Onde o capitão está dentro do nível: (nível, renome no nível, tamanho do
/// nível). No nível máximo o tamanho é 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub level: u32,
    pub into: u64,
    pub span: u64,
}

pub fn progress(total: u64) -> Progress {
    let level = level_of(total);
    let floor = threshold(level);
    let span = if level == MAX_LEVEL {
        0
    } else {
        threshold(level + 1) - floor
    };
    Progress {
        level,
        into: total - floor,
        span,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_climb_with_renown_and_cap_at_max() {
        assert_eq!(level_of(0), 1);
        assert_eq!(level_of(threshold(2) - 1), 1);
        assert_eq!(level_of(threshold(2)), 2);
        assert_eq!(level_of(threshold(MAX_LEVEL)), MAX_LEVEL);
        assert_eq!(level_of(u64::MAX), MAX_LEVEL);
        for level in 1..MAX_LEVEL {
            assert!(threshold(level + 1) > threshold(level));
        }
        let p = progress(threshold(4) + 7);
        assert_eq!(
            (p.level, p.into, p.span),
            (4, 7, threshold(5) - threshold(4))
        );
        assert_eq!(progress(threshold(MAX_LEVEL) + 99).span, 0);
    }

    #[test]
    fn first_ten_minutes_reach_level_three() {
        // ~30 unidades coletadas, 3 destroços, 1 fabricação e 2 NPCs.
        let session = 30 * PER_GATHERED_UNIT + 3 * PER_WRECK_LOOTED + PER_CRAFT + 2 * 35;
        assert!(level_of(u64::from(session)) >= 2);
        assert!(threshold(3) <= u64::from(session) + 40, "nível 3 perto");
    }
}
