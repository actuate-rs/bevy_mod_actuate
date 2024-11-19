use actuate::prelude::{Ref, *};
use bevy::prelude::*;
use bevy_mod_actuate::prelude::*;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Data)]
struct Breed<'a> {
    name: &'a String,
    families: &'a Vec<String>,
}

impl Compose for Breed<'_> {
    fn compose(cx: Scope<Self>) -> impl Compose {
        spawn_with(
            Node {
                flex_direction: FlexDirection::Row,
                ..default()
            },
            (
                spawn((
                    Text::new(cx.me().name),
                    Node {
                        width: Val::Px(300.0),
                        ..default()
                    },
                )),
                spawn_with(
                    Node {
                        flex_direction: FlexDirection::Column,
                        ..default()
                    },
                    compose::from_iter(Ref::map(cx.me(), |me| me.families), |family| {
                        spawn(Text::from(family.to_string()))
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

        spawn_with(
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
        .add_plugins((DefaultPlugins, ActuatePlugin))
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d::default());

    commands.spawn(Composition::new(BreedList));
}
