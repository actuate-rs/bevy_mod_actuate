<div align="center">
  <h1>bevy_mod_actuate</h1>
  <a href="https://crates.io/crates/bevy_mod_actuate">
    <img src="https://img.shields.io/crates/v/bevy_mod_actuate?style=flat-square"
    alt="Crates.io version" />
  </a>
  <a href="https://docs.rs/bevy_mod_actuate">
    <img src="https://img.shields.io/badge/docs-latest-blue.svg?style=flat-square"
      alt="docs.rs docs" />
  </a>
   <a href="https://github.com/actuate-rs/bevy_mod_actuate/actions">
    <img src="https://github.com/actuate-rs/bevy_mod_actuate/actions/workflows/ci.yml/badge.svg"
      alt="CI status" />
  </a>
  <a href="https://discord.gg/AbyAdew3">
    <img src="https://img.shields.io/discord/1306713440873877576.svg?label=&logo=discord&logoColor=ffffff&color=7389D8&labelColor=6A7EC2" />
</div>

<div align="center">
 <a href="https://github.com/actuate-rs/bevy_mod_actuate/tree/main/examples">Examples</a>
</div>

<br />

Reactivity for [Bevy](https://github.com/bevyengine/bevy) powered by [Actuate](https://github.com/actuate-rs/actuate).

```rs
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
```
