use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use mareforge_server::plugin::{TickCounter, TickLimit};
use mareforge_server::ServerPlugin;
use std::time::Duration;

#[test]
fn stops_after_configured_tick_limit() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(ServerPlugin);
    app.insert_resource(TickLimit(Some(5)));
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
        100,
    )));

    for _ in 0..10 {
        app.update();
        let ticks = app.world().resource::<TickCounter>().0;
        if ticks >= 5 {
            break;
        }
    }

    let counter = app.world().resource::<TickCounter>().0;
    assert_eq!(counter, 5);

    for _ in 0..10 {
        app.update();
    }
    assert_eq!(app.world().resource::<TickCounter>().0, 5);
}

#[test]
fn ticks_past_five_without_limit() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(ServerPlugin);
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
        100,
    )));

    for _ in 0..8 {
        app.update();
    }

    let counter = app.world().resource::<TickCounter>().0;
    assert!(
        counter > 5,
        "expected ticks past 5 without TickLimit, got {counter}"
    );
}
