use actuate::prelude::*;
use bevy::prelude::*;
use bevy_mod_actuate::prelude::*;

// Timer composable.
#[derive(Data)]
struct Timer;

impl Compose for Timer {
    fn compose(cx: Scope<Self>) -> impl Compose {
        // Use the `Time` resource from the ECS world.
        // Changing a resource tracked with `use_resource` will cause the composable to re-compose.
        let time = use_resource::<Time>(&cx);

        // Spawn a `Text` component, updating it when this scope is re-composed.
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

    // Spawn a composition with a `Timer`, adding it to the Actuate runtime.
    commands.spawn(Composition::new(Timer));
}
