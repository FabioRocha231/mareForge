use bevy::asset::{AssetPlugin, Handle};
use bevy::prelude::*;
use lightyear::prelude::{ClientId, ClientReceiveMessage};
use mareforge_client::assets::{layers, parts, GameAssets, HullSize};
use mareforge_client::net::{KnownWrecks, MyShip};
use mareforge_client::ship::{
    expire_stale_visuals, upsert_projectile_visuals, upsert_ship_visuals, upsert_wreck_visuals,
    DestroyedShips, ProjectileVisual, ShipVisual, WreckVisual,
};
use mareforge_domain_ships::ShipKind;
use mareforge_protocol::{ProjectileState, ShipState, WorldSnapshot, WreckState};

fn visual_app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<ColorMaterial>>()
        .insert_resource(MyShip(Some(1)))
        .insert_resource(DestroyedShips::default())
        .insert_resource(KnownWrecks::default())
        .insert_resource(test_assets())
        .add_event::<ClientReceiveMessage<WorldSnapshot>>()
        .add_systems(
            Update,
            (
                upsert_ship_visuals,
                upsert_projectile_visuals,
                upsert_wreck_visuals,
                expire_stale_visuals,
            )
                .chain(),
        );
    app
}

fn test_assets() -> GameAssets {
    GameAssets {
        ships: Handle::default(),
        ship_parts: Handle::default(),
        water_and_islands: Handle::default(),
        deco: Handle::default(),
        fort: Handle::default(),
        fort_parts: Handle::default(),
    }
}

fn ship_state(ship_id: u32, kind: ShipKind) -> ShipState {
    ShipState {
        ship_id,
        kind,
        x: 10.0,
        y: -20.0,
        heading: 1.25,
        speed: 5.0,
        cargo_weight: 0,
        hp: 100,
        max_hp: 100,
        max_speed: 30.0,
        weapon_damage: 10,
        weapon_range: 50.0,
        port_cooldown_secs: 0.0,
        starboard_cooldown_secs: 0.0,
        is_npc: false,
        cargo_capacity: 100,
        faction: mareforge_protocol::Faction::Player,
        notoriety_tier: 0,
    }
}

fn world_snapshot() -> WorldSnapshot {
    WorldSnapshot {
        tick: 1,
        ships: vec![
            ship_state(1, ShipKind::SmallMerchant),
            ship_state(2, ShipKind::Patrol),
            ship_state(3, ShipKind::Corsair),
        ],
        projectiles: vec![ProjectileState {
            projectile_id: 9,
            x: 30.0,
            y: 40.0,
            heading: 0.5,
        }],
        wrecks: vec![WreckState {
            wreck_id: 7,
            x: -5.0,
            y: 12.0,
            stack_count: 3,
        }],
    }
}

fn send_world(app: &mut App, snapshot: WorldSnapshot) {
    app.world_mut()
        .resource_mut::<Events<ClientReceiveMessage<WorldSnapshot>>>()
        .send(ClientReceiveMessage::new(snapshot, ClientId::Local(0)));
}

#[test]
fn snapshot_spawns_sprite_visuals_for_ships_projectiles_and_wrecks() {
    let mut app = visual_app();
    send_world(&mut app, world_snapshot());
    app.update();

    let world: &mut World = app.world_mut();
    let mut ships = world
        .query::<(&ShipVisual, &Transform, &Children)>()
        .iter(world)
        .map(|(visual, transform, children)| {
            (
                visual.target.ship_id,
                transform.translation.z,
                transform.scale.x,
                children.iter().copied().collect::<Vec<Entity>>(),
            )
        })
        .collect::<Vec<_>>();
    ships.sort_by_key(|ship| ship.0);
    assert_eq!(ships.len(), 3);

    // Cada navio é montado do atlas modular: o casco do tipo certo está
    // entre os filhos (MF-058).
    let expected_hulls = [
        parts::hull(HullSize::Medium, 1, false),
        parts::hull(HullSize::Large, 3, false),
        parts::hull(HullSize::Small, 2, false),
    ];
    for ((ship_id, z, scale, children), hull) in ships.iter().zip(expected_hulls) {
        assert_eq!(*z, layers::SHIPS, "navio {ship_id}");
        assert_eq!(*scale, mareforge_client::ship::WORLD_PER_PX);
        let indices: Vec<usize> = children
            .iter()
            .filter_map(|child| world.get::<Sprite>(*child))
            .filter_map(|sprite| sprite.texture_atlas.as_ref().map(|atlas| atlas.index))
            .collect();
        assert!(indices.contains(&hull), "navio {ship_id}: {indices:?}");
    }

    let projectiles = world
        .query::<(&ProjectileVisual, &Transform)>()
        .iter(world)
        .map(|(visual, transform)| (visual.target.projectile_id, transform.translation.z))
        .collect::<Vec<_>>();
    assert_eq!(projectiles, vec![(9, layers::PROJECTILES)]);

    let wrecks = world
        .query::<(&WreckVisual, &Transform, &Children)>()
        .iter(world)
        .map(|(visual, transform, children)| {
            (visual.wreck_num, transform.translation.z, children.len())
        })
        .collect::<Vec<_>>();
    assert_eq!(wrecks.len(), 1);
    assert_eq!(wrecks[0].0, 7);
    assert_eq!(wrecks[0].1, layers::WRECKS);
    assert!(wrecks[0].2 > 0, "destroço tem tábuas e baú");
}

#[test]
fn destroyed_ships_are_not_rendered_again() {
    let mut app = visual_app();
    app.world_mut().resource_mut::<DestroyedShips>().0.insert(2);
    send_world(&mut app, world_snapshot());
    app.update();

    let world: &mut World = app.world_mut();
    let rendered_ships = world
        .query::<&ShipVisual>()
        .iter(world)
        .map(|visual| visual.target.ship_id)
        .collect::<Vec<_>>();
    assert!(!rendered_ships.contains(&2));
    assert_eq!(rendered_ships.len(), 2);
}

#[test]
fn entities_absent_from_snapshot_decay_via_ttl() {
    // MF-031 last-known-state: visuals leave only after `STALE_VISUAL_TTL`
    // to avoid pop on AOI boundaries, so we age the visuals past the TTL
    // instead of expecting synchronous despawn on snapshot removal.
    use std::time::{Duration, Instant};

    let mut app = visual_app();
    send_world(&mut app, world_snapshot());
    app.update();

    let mut empty = world_snapshot();
    empty.ships.clear();
    empty.projectiles.clear();
    empty.wrecks.clear();
    send_world(&mut app, empty);
    app.update();

    {
        let world: &mut World = app.world_mut();
        let ttl = mareforge_client::ship::STALE_VISUAL_TTL;
        let aged_at = Instant::now() - Duration::from_secs_f32(ttl + 0.1);
        let mut ships = world.query::<&mut ShipVisual>();
        for mut visual in ships.iter_mut(world) {
            visual.last_seen = aged_at;
        }
        let mut wrecks = world.query::<&mut WreckVisual>();
        for mut visual in wrecks.iter_mut(world) {
            visual.last_seen = aged_at;
        }
    }
    app.update();

    let world: &mut World = app.world_mut();
    assert_eq!(world.query::<&ShipVisual>().iter(world).count(), 0);
    assert_eq!(world.query::<&ProjectileVisual>().iter(world).count(), 0);
    assert_eq!(world.query::<&WreckVisual>().iter(world).count(), 0);
}
