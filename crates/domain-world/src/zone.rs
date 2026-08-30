//! Zona (PRD §8): a unidade geográfica de risco. Uma zona tem nome, tier e
//! uma forma; o mar inteiro do mundo jogável é coberto por zonas declaradas —
//! posição fora de todas é `UnknownZone` (§69, fail-closed).

use crate::risk::RiskTier;
use mareforge_shared::ids::ZoneId;

/// Forma de uma zona. O Vertical Slice usa só círculos: corredores e rotas
/// são cadeias de círculos encadeados — geometria suficiente, zero matemática
/// de rotação.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ZoneShape {
    Circle { x: f32, y: f32, radius: f32 },
}

impl ZoneShape {
    pub fn contains(&self, x: f32, y: f32) -> bool {
        match *self {
            ZoneShape::Circle {
                x: cx,
                y: cy,
                radius,
            } => {
                let dx = x - cx;
                let dy = y - cy;
                dx * dx + dy * dy <= radius * radius
            }
        }
    }
}

/// Uma área nomeada do mar com seu tier de risco.
///
/// Nomes são `&'static str` porque o mundo do slice é conteúdo estático;
/// mundo vindo de banco trocará por `String` sem mudar a semântica.
#[derive(Debug, Clone, PartialEq)]
pub struct Zone {
    pub id: ZoneId,
    pub name: &'static str,
    pub tier: RiskTier,
    pub shape: ZoneShape,
}

impl Zone {
    pub fn contains(&self, x: f32, y: f32) -> bool {
        self.shape.contains(x, y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn circle_zone(x: f32, y: f32, radius: f32) -> Zone {
        Zone {
            id: ZoneId::new(),
            name: "teste",
            tier: RiskTier::Frontier,
            shape: ZoneShape::Circle { x, y, radius },
        }
    }

    #[test]
    fn circle_contains_center_and_edge() {
        let zone = circle_zone(10.0, -5.0, 3.0);
        assert!(zone.contains(10.0, -5.0));
        assert!(zone.contains(13.0, -5.0)); // na borda: contém (<=)
        assert!(!zone.contains(13.1, -5.0));
    }

    #[test]
    fn circle_radius_boundaries() {
        let zone = circle_zone(0.0, 0.0, 10.0);
        assert!(zone.contains(6.0, 8.0)); // exatamente 10 do centro
        assert!(!zone.contains(60.0, 80.0)); // exatamente 100 do centro
    }
}
