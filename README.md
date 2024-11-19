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
use actuate::prelude::{Ref, *};
use bevy::prelude::*;
use bevy_mod_actuate::{compose, spawn, Runtime};
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Data)]
struct Breed<'a> {
    name: &'a String,
    families: &'a Vec<String>,
}

impl Compose for Breed<'_> {
    fn compose(cx: Scope<Self>) -> impl Compose {
        spawn(
            Node {
                flex_direction: FlexDirection::Row,
                ..default()
            },
            (
                spawn(
                    (
                        Text::new(cx.me().name),
                        Node {
                            width: Val::Px(300.0),
                            ..default()
                        },
                    ),
                    (),
                ),
                spawn(
                    Node {
                        flex_direction: FlexDirection::Column,
                        ..default()
                    },
                    compose::from_iter(Ref::map(cx.me(), |me| me.families), |family| {
                        spawn(Text::from(family.to_string()), ())
                    }),
                ),
            ),
        )
    }
}

#[derive(Deserialize)]
struct Response {
    message: HashMap<String, Vec<String>>,
}

#[derive(Data)]
struct BreedList;

impl Compose for BreedList {
    fn compose(cx: Scope<Self>) -> impl Compose {
        let breeds = use_mut(&cx, HashMap::new);

        use_task(&cx, move || async move {
            let json: Response = reqwest::get("https://dog.ceo/api/breeds/list/all")
                .await
                .unwrap()
                .json()
                .await
                .unwrap();

            breeds.update(|breeds| *breeds = json.message);
        });

        spawn(
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(30.),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            compose::from_iter(breeds, |(name, families)| Breed { name, families }),
        )
    }
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .insert_non_send_resource(Runtime::new(BreedList))
        .add_systems(Startup, setup)
        .add_systems(Update, compose)
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d::default());
}
```
