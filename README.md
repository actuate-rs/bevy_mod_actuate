# bevy_mod_actuate

Experimental reactivity for [Bevy](https://github.com/bevyengine/bevy) provided by [Actuate](https://github.com/actuate-rs/actuate)

```rs
use actuate::prelude::*;
use bevy::prelude::*;
use bevy_mod_actuate::{compose, spawn, Runtime};
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize)]
struct Response {
    message: HashMap<String, Vec<String>>,
}

#[derive(Data)]
struct Ui;

impl Compose for Ui {
    fn compose(cx: Scope<Self>) -> impl Compose {
        let breeds = use_mut(&cx, Vec::new);

        use_task(&cx, move || async move {
            let json: Response = reqwest::get("https://dog.ceo/api/breeds/list/all")
                .await
                .unwrap()
                .json()
                .await
                .unwrap();

            for (name, _) in json.message {
                breeds.update(|breeds| breeds.push(name));
            }
        });

        spawn(
            || NodeBundle {
                style: Style {
                    flex_direction: FlexDirection::Column,
                    ..default()
                },
                ..default()
            },
            compose::from_iter(breeds, |breed| {
                spawn(|| TextBundle::from(breed.to_string()), ())
            }),
        )
    }
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .insert_non_send_resource(Runtime::new(Ui))
        .add_systems(Startup, setup)
        .add_systems(Update, compose)
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2dBundle::default());
}
```
