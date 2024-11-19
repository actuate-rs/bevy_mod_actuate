use actuate::prelude::*;
use bevy::prelude::*;
use bevy_mod_actuate::prelude::*;

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
        .add_plugins((DefaultPlugins, ActuatePlugin))
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d::default());

    commands.spawn(Composition::new(Timer));
}
