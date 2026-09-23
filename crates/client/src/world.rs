//! Cenário do vertical slice (MF-057D/E, MF-058). A geografia vem do
//! `WorldMap` — zonas, portos e terra são os mesmos do servidor; esta
//! camada só escolhe a arte. Mar e terra são um shader (`sea.wgsl`); portos,
//! vegetação e rochas são sprites do pack Scallywag por cima.

use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderRef};
use bevy::sprite::{Anchor, Material2d, Material2dPlugin};
use mareforge_domain_world::map::{MAELSTROM_POINTS, MAELSTROM_X, PIRATE_PORT};
use mareforge_domain_world::{LandMass, RiskTier, WorldMap, ZoneShape};

use crate::assets::{deco, fort, layers, GameAssets};
use crate::zone::CurrentZone;

/// Discos de terra enviados ao shader por quadro — só os perto da câmera
/// (o mundo inteiro tem 160+ com as instâncias).
const MAX_LAND: usize = 64;
const MAX_SAFE: usize = 4;

// O derive `ShaderType` (encase) gera uma fn `check` por campo que o rustc
// acusa como código morto; o allow fica restrito a este módulo.
#[allow(dead_code)]
mod uniform {
    use super::{MAX_LAND, MAX_SAFE};
    use bevy::math::Vec4;
    use bevy::render::render_resource::ShaderType;

    #[derive(ShaderType, Debug, Clone)]
    pub struct SeaParams {
        pub land: [Vec4; MAX_LAND],
        pub safe: [Vec4; MAX_SAFE],
        pub info: Vec4,
    }
}
pub use uniform::SeaParams;

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct SeaMaterial {
    #[uniform(0)]
    pub params: SeaParams,
}

impl Material2d for SeaMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/sea.wgsl".into()
    }
}

/// Material do mar + o mapa, para o tom de perigo acompanhar a zona e a
/// terra acompanhar a câmera.
#[derive(Resource)]
struct Sea {
    material: Handle<SeaMaterial>,
    map: WorldMap,
    streamed_at: Vec3,
}

/// Bandeira animada (porto ou navio): cor do atlas + fase própria.
#[derive(Component)]
pub struct WavingFlag {
    pub color: usize,
    pub phase: usize,
}

pub struct WorldVisualPlugin;

impl Plugin for WorldVisualPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(Material2dPlugin::<SeaMaterial>::default())
            // PostStartup: `GameAssets` é inserido por comando no Startup.
            .add_systems(PostStartup, spawn_vertical_slice_world)
            .add_systems(Update, (tint_sea_by_zone, stream_land, animate_flags));
    }
}

/// Parede de instância (cerração/Sorvedouro) é penhasco, não ilha com palmeira.
fn is_cliff(mass: &LandMass) -> bool {
    mass.x.abs() > 3000.0
}

/// Parâmetros do shader com a terra que cabe na vista em `center`.
pub fn sea_params(map: &WorldMap, center: Vec2, view_radius: f32) -> SeaParams {
    let mut land = [Vec4::ZERO; MAX_LAND];
    let visible = map
        .land()
        .iter()
        .filter(|mass| center.distance(Vec2::new(mass.x, mass.y)) < view_radius + mass.radius);
    let mut count = 0;
    for (slot, mass) in land.iter_mut().zip(visible) {
        *slot = Vec4::new(mass.x, mass.y, mass.radius, is_cliff(mass) as u8 as f32);
        count += 1;
    }
    let mut safe = [Vec4::ZERO; MAX_SAFE];
    let protected = map.zones().iter().filter(|z| z.tier == RiskTier::Protected);
    let mut safe_count = 0;
    for (slot, zone) in safe.iter_mut().zip(protected) {
        let ZoneShape::Circle { x, y, radius } = zone.shape;
        *slot = Vec4::new(x, y, radius, 0.0);
        safe_count += 1;
    }
    SeaParams {
        land,
        safe,
        info: Vec4::new(count as f32, safe_count as f32, 0.0, 0.0),
    }
}

fn atlas(image: &Handle<Image>, layout: &Handle<TextureAtlasLayout>, index: usize) -> Sprite {
    Sprite::from_atlas_image(
        image.clone(),
        TextureAtlas {
            layout: layout.clone(),
            index,
        },
    )
}

fn prop(commands: &mut Commands, sprite: Sprite, at: Vec2, scale: f32, z: f32) {
    commands.spawn((
        sprite,
        Transform::from_translation(at.extend(z)).with_scale(Vec3::splat(scale)),
    ));
}

fn label(commands: &mut Commands, text: &str, at: Vec2, size: f32, color: Color) {
    commands.spawn((
        Text2d::new(text),
        TextLayout::new_with_no_wrap(),
        TextFont {
            font_size: size,
            ..default()
        },
        TextColor(color),
        Anchor::Center,
        Transform::from_translation(at.extend(layers::LABELS)),
    ));
}

/// Distância com sinal até a terra (negativa dentro), como no shader mas
/// sem o ruído da costa — por isso quem usa pede folga.
fn land_distance(land: &[LandMass], p: Vec2) -> f32 {
    land.iter()
        .map(|m| p.distance(Vec2::new(m.x, m.y)) - m.radius)
        .fold(f32::MAX, f32::min)
}

fn spawn_vertical_slice_world(
    mut commands: Commands,
    assets: Res<GameAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<SeaMaterial>>,
) {
    let map = WorldMap::vertical_slice();

    let sea = materials.add(SeaMaterial {
        params: sea_params(&map, Vec2::new(-560.0, 0.0), 1500.0),
    });
    // Cobre o mapa principal e as instâncias (cerrações a leste, Sorvedouro a oeste).
    commands.spawn((
        Mesh2d(meshes.add(Rectangle::new(11000.0, 6400.0))),
        MeshMaterial2d(sea.clone()),
        Transform::from_xyz(0.0, 300.0, layers::OCEAN),
    ));

    let ports: Vec<Vec2> = map
        .regions()
        .iter()
        .filter_map(|r| r.port.as_ref())
        .map(|p| Vec2::new(p.x, p.y))
        .collect();
    for port in map.regions().iter().filter_map(|r| r.port.as_ref()) {
        spawn_port(&mut commands, &assets, port.name, Vec2::new(port.x, port.y));
    }
    spawn_vegetation(&mut commands, &assets, map.land(), &ports);

    // Rótulos em ASCII: a fonte padrão não desenha acentos.
    let danger = Color::srgb(1.0, 0.82, 0.78);
    for (text, at) in [
        ("Ilha do Coral Negro", Vec2::new(0.0, 900.0)),
        ("AGUAS NEGRAS", Vec2::new(0.0, 1400.0)),
        (
            "PASSAGEM DO SORVEDOURO",
            Vec2::new(MAELSTROM_X, MAELSTROM_POINTS[0].1 - 180.0),
        ),
    ] {
        label(&mut commands, text, at, 16.0, danger);
    }
    commands.insert_resource(Sea {
        material: sea,
        map,
        streamed_at: Vec3::splat(f32::MAX),
    });
}

/// Envia ao shader só a terra perto da câmera; refaz quando a câmera anda
/// ou o zoom muda o bastante.
fn stream_land(
    sea: Option<ResMut<Sea>>,
    camera: Query<(&Transform, &OrthographicProjection), With<Camera2d>>,
    windows: Query<&Window>,
    mut materials: ResMut<Assets<SeaMaterial>>,
) {
    let (Some(mut sea), Ok((transform, projection))) = (sea, camera.get_single()) else {
        return;
    };
    let window = windows
        .get_single()
        .map(|w| Vec2::new(w.width(), w.height()))
        .unwrap_or(Vec2::new(1280.0, 720.0));
    let view = (
        transform.translation.x,
        transform.translation.y,
        projection.scale,
    );
    let moved = Vec2::new(view.0 - sea.streamed_at.x, view.1 - sea.streamed_at.y).length();
    if moved < 80.0 && (view.2 - sea.streamed_at.z).abs() < 0.05 {
        return;
    }
    sea.streamed_at = Vec3::new(view.0, view.1, view.2);
    let view_radius = window.length() * 0.5 * projection.scale + 200.0;
    let fresh = sea_params(&sea.map, Vec2::new(view.0, view.1), view_radius);
    if let Some(material) = materials.get_mut(&sea.material) {
        let danger = material.params.info.z;
        material.params = fresh;
        material.params.info.z = danger;
    }
}

/// Porto sobre a costa: cais de tábuas até a água, torres com bandeira,
/// carga no píer e lanternas. Serra é madeira e verde; Mina é pedra e canhão.
fn spawn_port(commands: &mut Commands, assets: &GameAssets, name: &str, dock: Vec2) {
    let pirate = name == PIRATE_PORT;
    let mina = name.contains("Mina");
    // Direção da terra a partir do cais: Serra a oeste, Mina a leste, o
    // porto pirata ao sul (na ilha, virado para as Águas Negras).
    let inland = if pirate {
        Vec2::NEG_Y
    } else if dock.x < 0.0 {
        Vec2::NEG_X
    } else {
        Vec2::X
    };
    let side = inland.perp();
    let at = |along: f32, across: f32| dock + inland * along + side * across;
    let facing = Quat::from_rotation_z(inland.to_angle());
    let fort_sprite = |index| atlas(&assets.fort, &assets.fort_parts, index);
    let deco_sprite = |index| atlas(&assets.water_and_islands, &assets.deco, index);

    // Cais: três seções de tábuas saindo da praia até a doca.
    for i in 0..3 {
        commands.spawn((
            fort_sprite(fort::DOCK),
            Transform::from_translation(at(18.0 + i as f32 * 31.0, 0.0).extend(layers::PROPS))
                .with_rotation(facing)
                .with_scale(Vec3::splat(0.5)),
        ));
    }
    // Plataforma de carga na praia.
    commands.spawn((
        fort_sprite(fort::BOARDWALK),
        Transform::from_translation(at(110.0, 0.0).extend(layers::PROPS))
            .with_rotation(facing)
            .with_scale(Vec3::splat(0.9)),
    ));

    // Torres: pedra na Mina (com canhões), madeira/pedra leve na Serra.
    let flag_color = if pirate {
        5
    } else if mina {
        4
    } else {
        2
    };
    let stone = mina || pirate;
    for (i, side_sign) in [-1.0_f32, 1.0].into_iter().enumerate() {
        let tower = at(70.0, side_sign * 46.0);
        let index = if stone {
            fort::TOWER
        } else {
            fort::TOWER_PLAIN
        };
        prop(
            commands,
            fort_sprite(index),
            tower,
            0.9,
            layers::PROPS + 0.1,
        );
        commands.spawn((
            fort_sprite(fort::FLAG + flag_color * 3),
            Transform::from_translation((tower + Vec2::new(4.0, 18.0)).extend(layers::PROPS + 0.3))
                .with_scale(Vec3::splat(1.6)),
            WavingFlag {
                color: flag_color,
                phase: i,
            },
        ));
        if stone {
            let mut cannon = fort_sprite(fort::CANNON);
            cannon.flip_x = inland.x > 0.0;
            prop(
                commands,
                cannon,
                tower - inland * 20.0,
                0.6,
                layers::PROPS + 0.2,
            );
        }
    }
    if mina {
        for side_sign in [-1.0_f32, 1.0] {
            for k in 0..3 {
                let block = at(98.0 + k as f32 * 28.0, side_sign * 62.0);
                prop(
                    commands,
                    fort_sprite(fort::WALL_BLOCK),
                    block,
                    0.9,
                    layers::PROPS,
                );
            }
        }
    }

    // Carga no píer: caixas e barris.
    for (offset, index, scale) in [
        (at(100.0, 10.0), fort::CRATE, 0.45),
        (at(118.0, -8.0), fort::CRATE, 0.4),
        (at(42.0, 9.0), fort::BARREL, 0.8),
        (at(48.0, -9.0), fort::BARREL, 0.8),
        (at(124.0, 14.0), fort::BARREL, 0.8),
    ] {
        prop(
            commands,
            fort_sprite(index),
            offset,
            scale,
            layers::PROPS + 0.2,
        );
    }
    // Lanternas na ponta do cais e bote amarrado.
    for side_sign in [-1.0_f32, 1.0] {
        prop(
            commands,
            deco_sprite(deco::LAMP),
            at(6.0, side_sign * 11.0),
            0.8,
            layers::PROPS + 0.3,
        );
    }
    prop(
        commands,
        deco_sprite(deco::ROWBOAT),
        at(30.0, -22.0),
        0.7,
        layers::PROPS,
    );

    label(
        commands,
        name,
        at(105.0, 96.0),
        16.0,
        Color::srgb(1.0, 0.95, 0.8),
    );
}

/// Palmeiras e arbustos espalhados de forma determinística pela terra
/// (longe da água e dos portos); rochas musgosas sobre os rochedos.
fn spawn_vegetation(
    commands: &mut Commands,
    assets: &GameAssets,
    land: &[LandMass],
    ports: &[Vec2],
) {
    let plants = [deco::PALM, deco::PALM_B, deco::BUSH, deco::BUSH_B];
    for (i, mass) in land.iter().enumerate() {
        let center = Vec2::new(mass.x, mass.y);
        if is_cliff(mass) {
            continue;
        }
        if mass.radius < 40.0 {
            let index = if i % 2 == 0 {
                deco::ROCK_MOSS
            } else {
                deco::ROCK_MOSS_B
            };
            prop(
                commands,
                atlas(&assets.water_and_islands, &assets.deco, index),
                center,
                mass.radius / 12.0,
                layers::PROPS,
            );
            continue;
        }
        // Espiral de ouro: pontos bem distribuídos dentro do disco.
        let count = (mass.radius / 7.0) as usize;
        for k in 0..count {
            let r = mass.radius * ((k as f32 + 0.5) / count as f32).sqrt();
            let angle = k as f32 * 2.399_963 + i as f32;
            let at = center + Vec2::from_angle(angle) * r;
            let clear_of_port = ports.iter().all(|port| port.distance(at) > 190.0);
            if land_distance(land, at) > -22.0 || !clear_of_port {
                continue;
            }
            let index = plants[(k + i) % plants.len()];
            prop(
                commands,
                atlas(&assets.water_and_islands, &assets.deco, index),
                at,
                1.4,
                layers::PROPS,
            );
        }
    }
}

/// Alto-mar sem lei escurece a água (transição suave ao cruzar a fronteira).
fn tint_sea_by_zone(
    time: Res<Time>,
    zone: Res<CurrentZone>,
    sea: Option<Res<Sea>>,
    mut materials: ResMut<Assets<SeaMaterial>>,
) {
    let Some(sea) = sea else { return };
    let sea = &sea.material;
    let goal = match zone.0.as_ref().map(|z| z.tier) {
        Some(RiskTier::Lawless) => 1.0,
        Some(RiskTier::Frontier) => 0.35,
        _ => 0.0,
    };
    let Some(material) = materials.get(sea) else {
        return;
    };
    let current = material.params.info.z;
    if (goal - current).abs() < 0.002 {
        return;
    }
    let next = current + (goal - current) * (1.0 - (-1.5 * time.delta_secs()).exp());
    if let Some(material) = materials.get_mut(sea) {
        material.params.info.z = next;
    }
}

fn animate_flags(time: Res<Time>, mut flags: Query<(&WavingFlag, &mut Sprite)>) {
    let tick = (time.elapsed_secs() * 8.0) as usize;
    for (flag, mut sprite) in &mut flags {
        if let Some(atlas) = sprite.texture_atlas.as_mut() {
            atlas.index = fort::FLAG + flag.color * 3 + (tick + flag.phase) % 3;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sea_params_stream_only_nearby_land() {
        let map = WorldMap::vertical_slice();
        let near_serra = sea_params(&map, Vec2::new(-600.0, 0.0), 900.0);
        let count = near_serra.info.x as usize;
        assert!(count > 0 && count <= MAX_LAND);
        assert_eq!(near_serra.info.y, 2.0);
        // Nada das instâncias longínquas entra na vista do porto.
        assert!(near_serra.land[..count].iter().all(|disc| disc.w == 0.0));
        // Dentro de uma cerração, as paredes são penhasco.
        let (x, y) = mareforge_domain_world::map::FOG_SLOTS[1];
        let fog = sea_params(&map, Vec2::new(x, y), 900.0);
        let count = fog.info.x as usize;
        assert!(count <= MAX_LAND);
        assert!(fog.land[..count].iter().any(|disc| disc.w == 1.0));
    }
}
