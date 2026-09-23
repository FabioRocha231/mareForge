//! Terra firme (MF-058): costas, ilhas e rochedos. Geografia é obstáculo —
//! navio não atravessa terra e bala de canhão morre na pedra, então a costa
//! vira cobertura e o estreito vira emboscada. Forma: união de círculos,
//! a mesma geometria barata das zonas.

/// Um disco de terra. Costas e ilhas irregulares são uniões de discos.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LandMass {
    pub x: f32,
    pub y: f32,
    pub radius: f32,
}

impl LandMass {
    pub fn contains(&self, x: f32, y: f32, clearance: f32) -> bool {
        let reach = self.radius + clearance;
        let (dx, dy) = (x - self.x, y - self.y);
        dx * dx + dy * dy < reach * reach
    }
}

/// Empurra o ponto para fora de toda terra, mantendo `clearance` de folga
/// (meia boca do casco). `None` = já estava na água.
///
/// ponytail: poucas passadas resolvem discos sobrepostos na prática; um
/// canal mais estreito que o casco pode oscilar — desenhe o mapa sem isso.
pub fn push_out_of_land(masses: &[LandMass], x: f32, y: f32, clearance: f32) -> Option<(f32, f32)> {
    let (mut px, mut py) = (x, y);
    let mut moved = false;
    for _ in 0..4 {
        let Some(mass) = masses.iter().find(|m| m.contains(px, py, clearance)) else {
            break;
        };
        let (dx, dy) = (px - mass.x, py - mass.y);
        let dist = (dx * dx + dy * dy).sqrt();
        // Centro exato do disco: qualquer direção serve, escolhe +X.
        let (nx, ny) = if dist > f32::EPSILON {
            (dx / dist, dy / dist)
        } else {
            (1.0, 0.0)
        };
        let reach = mass.radius + clearance;
        px = mass.x + nx * reach;
        py = mass.y + ny * reach;
        moved = true;
    }
    moved.then_some((px, py))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROCK: LandMass = LandMass {
        x: 0.0,
        y: 0.0,
        radius: 10.0,
    };

    #[test]
    fn water_point_is_untouched() {
        assert_eq!(push_out_of_land(&[ROCK], 20.0, 0.0, 5.0), None);
    }

    #[test]
    fn point_inside_is_pushed_to_edge_plus_clearance() {
        let (x, y) = push_out_of_land(&[ROCK], 3.0, 0.0, 5.0).unwrap();
        assert!((x - 15.0).abs() < 1e-4 && y.abs() < 1e-4);
    }

    #[test]
    fn overlapping_discs_push_out_of_both() {
        let masses = [
            ROCK,
            LandMass {
                x: 14.0,
                y: 0.0,
                radius: 10.0,
            },
        ];
        let (x, y) = push_out_of_land(&masses, 7.0, 1.0, 2.0).unwrap();
        assert!(masses.iter().all(|m| !m.contains(x, y, 2.0 - 1e-3)));
    }
}
