//! Região (PRD §6-§7, §29): a unidade econômica do mundo. `PortStorage` e o
//! mercado regional referenciam `RegionId` — itens guardados em uma região
//! não aparecem em outra. Regiões se especializam (Porto da Serra: madeira;
//! Porto da Mina: minério) para criar o triângulo de arbitragem.

use mareforge_shared::ids::RegionId;

/// Porto (PRD §5): área de serviços, não cidade caminhável. A área é um
/// círculo de serviço; dock, market e storage serão acessíveis dentro dela.
#[derive(Debug, Clone, PartialEq)]
pub struct Port {
    pub name: &'static str,
    pub x: f32,
    pub y: f32,
    /// Raio da área de serviço, em metros.
    pub service_radius: f32,
}

impl Port {
    /// O navio está dentro da área de serviço do porto?
    pub fn contains(&self, x: f32, y: f32) -> bool {
        let dx = x - self.x;
        let dy = y - self.y;
        dx * dx + dy * dy <= self.service_radius * self.service_radius
    }
}

/// Uma região do mundo: seu porto (se houver) e a especialização econômica
/// que alimenta a arbitragem (Pilar 2: o navio é correio e masmorra).
#[derive(Debug, Clone, PartialEq)]
pub struct Region {
    pub id: RegionId,
    pub name: &'static str,
    pub port: Option<Port>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn port() -> Port {
        Port {
            name: "Porto Teste",
            x: -600.0,
            y: 0.0,
            service_radius: 60.0,
        }
    }

    #[test]
    fn port_service_area_contains_dock_and_rejects_far_sea() {
        let port = port();
        assert!(port.contains(-600.0, 0.0));
        assert!(port.contains(-550.0, 30.0));
        assert!(!port.contains(0.0, 0.0));
    }
}
