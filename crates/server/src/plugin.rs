use bevy::app::AppExit;
use bevy::ecs::prelude::{EventWriter, Res, ResMut, Resource};
use bevy::prelude::{App, FixedUpdate, IntoSystemConfigs, Plugin, Startup};
use bevy::time::{Fixed, Time};

#[derive(Resource, Default)]
pub struct TickCounter(pub u32);

/// Limite opcional de ticks. O padrão é `None`: o servidor roda até shutdown
/// externo (Ctrl+C). Testes de smoke inserem `Some(n)` para encerrar após n ticks.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct TickLimit(pub Option<u32>);

pub struct ServerPlugin;

impl Plugin for ServerPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(TickCounter::default());
        app.insert_resource(TickLimit::default());
        // ADR-0008: simulação a 30 Hz.
        app.insert_resource(Time::<Fixed>::from_hz(30.0));
        app.add_systems(Startup, setup);
        app.add_systems(FixedUpdate, tick.run_if(should_tick));
    }
}

fn setup() {
    tracing::info!("mareforge server starting");
}

pub fn should_tick(limit: Res<TickLimit>, counter: Res<TickCounter>) -> bool {
    match limit.0 {
        Some(max) => counter.0 < max,
        None => true,
    }
}

pub fn tick(
    mut counter: ResMut<TickCounter>,
    limit: Res<TickLimit>,
    mut exit: EventWriter<AppExit>,
) {
    counter.0 += 1;
    tracing::info!(tick = counter.0, "server tick");
    if limit.0.is_some_and(|max| counter.0 >= max) {
        exit.send(AppExit::Success);
    }
}
