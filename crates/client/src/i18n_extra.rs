//! Tabela PT-BR → inglês das telas de porto, mercado, guilda, oficina,
//! clima e feed (MV-062). Separada de `i18n::TABLE` (HUD, livreto, guia)
//! só para as duas crescerem sem pisar uma na outra; `i18n::translate`
//! consulta as duas.

pub const TABLE: &[(&str, &str)] = &[
    // Tela de porto: abas, cabeçalho, rodapé
    ("Porto", "Port"),
    ("Porto: ?", "Port: ?"),
    ("Porão", "Hold"),
    ("Equipamento", "Loadout"),
    ("Fabricação", "Crafting"),
    ("Estaleiro", "Shipyard"),
    ("Mercado", "Market"),
    ("Guilda", "Guild"),
    ("Contratos", "Contracts"),
    ("Desatracar [ESC]", "Undock [ESC]"),
    (
        "Tab/Shift+Tab: abas · Setas: escolher · Enter: executar · ESC: desatracar",
        "Tab/Shift+Tab: tabs · Arrows: choose · Enter: confirm · ESC: undock",
    ),
    ("ERRO", "ERROR"),
    // Tela de porto: ações e linhas
    ("Depositar tudo", "Deposit all"),
    ("Retirar tudo", "Withdraw all"),
    ("Desequipar {0}", "Unequip {0}"),
    ("Equipar", "Equip"),
    ("Construir {0}", "Build {0}"),
    ("Fabricar {0}", "Craft {0}"),
    ("receita", "recipe"),
    ("Desatracar", "Undock"),
    ("Casco", "Hull"),
    ("Velas", "Sails"),
    ("Armas", "Weapons"),
    ("Auxiliar", "Auxiliary"),
    ("Qualquer estação", "Any station"),
    ("Bancada", "Workbench"),
    ("Bigorna", "Anvil"),
    ("Doca", "Dock"),
    ("Porão: {0} / {1}", "Hold: {0} / {1}"),
    (
        "Armazém: conteúdo oculto — use Depositar/Retirar tudo",
        "Warehouse: contents hidden — use Deposit/Withdraw all",
    ),
    ("(vazio)", "(empty)"),
    (
        "Armazém: nada compatível com este casco",
        "Warehouse: nothing fits this hull",
    ),
    (
        "Receitas de casco — custos saem do armazém do porto.",
        "Hull recipes — costs come from the port warehouse.",
    ),
    ("rende {0} x{1}", "yields {0} x{1}"),
    ("Mercado regional", "Regional market"),
    // Mercado
    ("ORDENS DO MERCADO", "MARKET ORDERS"),
    ("Mercado: sem ordens", "Market: no orders"),
    ("VENDER", "SELL"),
    ("Qtd", "Qty"),
    ("Preço", "Price"),
    ("Criar ordem de venda", "Post sell order"),
    (
        "Clique no campo e digite · Shift+clique: ±10 · Enter envia",
        "Click a field and type · Shift+click: ±10 · Enter submits",
    ),
    ("Cancelar", "Cancel"),
    ("Comprar", "Buy"),
    ("MINHA", "MINE"),
    // Guilda e contratos
    (
        "GUILDA MERCANTE - compra do armazém deste porto (item vendido é destruído)",
        "MERCHANT GUILD - buys from this port's warehouse (sold items are destroyed)",
    ),
    (
        "Aguardando preços da guilda...",
        "Waiting for guild prices...",
    ),
    ("Armazém", "Warehouse"),
    ("Preço aqui", "Price here"),
    ("Vender 1", "Sell 1"),
    ("Tudo", "All"),
    (
        "Vender muito derruba o preço; ele se recupera com o tempo. Deposite o porão para vender.",
        "Selling a lot drops the price; it recovers over time. Deposit your hold to sell.",
    ),
    ("SEU CONTRATO", "YOUR CONTRACT"),
    ("{0} restantes - {1}", "{0} left - {1}"),
    ("abates {0}/{1}", "kills {0}/{1}"),
    (
        "entregue ao atracar no destino",
        "delivered on docking at the destination",
    ),
    ("Abandonar", "Abandon"),
    ("Nenhum contrato ativo.", "No active contract."),
    ("QUADRO DE CONTRATOS", "CONTRACT BOARD"),
    (
        "Quadro vazio - volte mais tarde.",
        "Board is empty - check back later.",
    ),
    ("Aceitar", "Accept"),
    (
        "1 contrato por vez. Entrega: a carga vai no porão e pode ser saqueada no caminho.",
        "One contract at a time. Deliveries ride in your hold and can be plundered en route.",
    ),
    (
        "CONTRATO: {0}\n{1} - {2} - {3}g",
        "CONTRACT: {0}\n{1} - {2} - {3}g",
    ),
    // Clima e munição
    ("VENTO FORTE", "STRONG WIND"),
    ("VENTO MODERADO", "MODERATE WIND"),
    ("VENTO FRACO", "LIGHT WIND"),
    ("TEMPESTADE!", "STORM!"),
    ("Popa", "Running"),
    ("Traves", "Beam reach"),
    ("Bolina", "Close-hauled"),
    ("Contra o vento", "In irons"),
    ("VELAS {0}%", "SAILS {0}%"),
    ("MUNIÇÃO: BALA", "AMMO: ROUND SHOT"),
    ("MUNIÇÃO: CORRENTE", "AMMO: CHAIN SHOT"),
    // Reputação e zonas
    ("PROCURADO", "WANTED"),
    ("PROCURADO - cabeça: {0}g", "WANTED - bounty: {0}g"),
    ("SUSPEITO - notoriedade {0}", "SUSPECT - notoriety {0}"),
    ("PvP desativado", "PvP off"),
    ("PvP ATIVO - full loot", "PvP ON - full loot"),
    // Lugares
    ("Porto da Serra", "Ridge Harbor"),
    ("Porto da Mina", "Mine Harbor"),
    ("Porto do Coral Negro", "Black Coral Harbor"),
    ("Porto das Gaivotas", "Gull Harbor"),
    ("Porto do Farol", "Lighthouse Harbor"),
    ("Porto da Areia Branca", "White Sand Harbor"),
    ("Porto Santa Luzia", "Santa Luzia Harbor"),
    ("Ilha do Coral Negro", "Black Coral Isle"),
    ("Águas Negras", "Black Waters"),
    ("ÁGUAS NEGRAS", "BLACK WATERS"),
    ("PASSAGEM DO SORVEDOURO", "MAELSTROM PASSAGE"),
    // Itens e recursos (nomes do catálogo do servidor)
    ("Madeira", "Timber"),
    ("Minério", "Ore"),
    ("Coral Negro", "Black Coral"),
    ("Casco Reforçado", "Reinforced Hull"),
    ("Velas de Corrida", "Racing Sails"),
    ("Canhão de Bronze", "Bronze Cannon"),
    ("Mapa do Tesouro", "Treasure Map"),
    ("Pérola Abissal", "Abyssal Pearl"),
    ("Essência da Cerração", "Fog Essence"),
    ("Âmbar Abissal", "Abyssal Amber"),
    ("Casco Negro", "Black Hull"),
    ("Velas de Cerração", "Fog Sails"),
    ("Canhões Abissais", "Abyssal Cannons"),
];

#[cfg(test)]
mod tests {
    use crate::i18n::{translate, trf_in, Lang};

    #[test]
    fn port_and_item_keys_translate() {
        assert_eq!(translate("Madeira", Lang::En), "Timber");
        assert_eq!(translate("Porão", Lang::En), "Hold");
        assert_eq!(translate("MUNIÇÃO: CORRENTE", Lang::En), "AMMO: CHAIN SHOT");
        assert_eq!(
            trf_in("Fabricar {0}", &["Bronze Cannon"], Lang::En),
            "Craft Bronze Cannon"
        );
    }

    #[test]
    fn templates_keep_their_placeholders() {
        for (pt, en) in super::TABLE {
            for index in 0..4 {
                let slot = format!("{{{index}}}");
                assert_eq!(pt.contains(&slot), en.contains(&slot), "{pt}");
            }
        }
    }
}
