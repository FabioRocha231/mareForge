//! Custódia de itens (PRD §29): separa **quem é dono** de **onde está**.
//! A localização viaja junto do item; contêineres (porão, baú de wreck)
//! garantem por construção que a localização batiza o contêiner certo.

use serde::{Deserialize, Serialize};

use crate::instance::ItemInstance;
use mareforge_shared::ids::{ItemInstanceId, MarketOrderId, RegionId, ShipInstanceId, WreckId};

/// Onde um item está fisicamente (PRD §29). Novas variantes entram quando o
/// jogo pedir (ex.: equipamento equipado).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ItemLocation {
    PortStorage(RegionId),
    ShipCargo(ShipInstanceId),
    MarketEscrow(MarketOrderId),
    Wreck(WreckId),
}

/// Um item instanciado junto da sua localização física.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Custody {
    pub instance: ItemInstance,
    pub location: ItemLocation,
}

impl Custody {
    pub fn new(instance: ItemInstance, location: ItemLocation) -> Self {
        Self { instance, location }
    }

    /// Identidade da instância (para rastrear a mesma instância entre
    /// contêineres).
    pub fn instance_id(&self) -> ItemInstanceId {
        self.instance.id
    }
}
