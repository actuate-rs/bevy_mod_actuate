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

## Depreciated:
[Actuate](https://github.com/actuate-rs/actuate) now supports `Bevy` by default!

Declarative scenes and reactivity for [Bevy](https://github.com/bevyengine/bevy) powered by [Actuate](https://github.com/actuate-rs/actuate).

```rs
use actuate::prelude::{Mut, *};
use bevy::prelude::*;
use bevy_mod_actuate::prelude::*;

// Counter composable.
#[derive(Data)]
struct Counter {
    start: i32,
}

impl Compose for Counter {
    fn compose(cx: Scope<Self>) -> impl Compose {
        let count = use_mut(&cx, || cx.me().start);

        spawn_with(
            Node {
                flex_direction: FlexDirection::Column,
                ..default()
            },
            (
                spawn(Text::new(format!("High five count: {}", count))),
                spawn(Text::new("Up high")).observe(
                    move |_trigger: In<Trigger<Pointer<Click>>>| Mut::update(count, |x| *x += 1),
                ),
                spawn(Text::new("Down low")).observe(
                    move |_trigger: In<Trigger<Pointer<Click>>>| Mut::update(count, |x| *x -= 1),
                ),
                if *count == 0 {
                    Some(spawn(Text::new("Gimme five!")))
                } else {
                    None
                },
            ),
        )
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

    // Spawn a composition with a `Counter`, adding it to the Actuate runtime.
    commands.spawn((Node::default(), Composition::new(Counter { start: 0 })));
}
```
