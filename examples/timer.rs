use actuate::prelude::*;
use bevy::prelude::*;
use bevy_mod_actuate::{compose, spawn, use_resource, Runtime};

#[derive(Data)]
struct Timer;

impl Compose for Timer {
    fn compose(cx: Scope<Self>) -> impl Compose {
        let time = use_resource::<Time>(&cx);

        spawn(Text::new(format!("Elapsed: {:?}", time.elapsed())))
    }
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .insert_non_send_resource(Runtime::new(Timer))
        .add_systems(Startup, setup)
        .add_systems(Update, compose)
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d::default());
}
