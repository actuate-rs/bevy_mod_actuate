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
            || NodeBundle {
                style: Style {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(30.),
                    ..default()
                },
                ..default()
            },
            compose::from_iter(breeds, |(name, families)| {
                spawn(
                    || NodeBundle {
                        style: Style {
                            flex_direction: FlexDirection::Row,
                            ..default()
                        },
                        ..default()
                    },
                    (
                        spawn(
                            || TextBundle {
                                text: Text::from_section(name.to_string(), TextStyle::default()),
                                style: Style {
                                    width: Val::Px(300.0),
                                    ..default()
                                },
                                ..default()
                            },
                            (),
                        ),
                        spawn(
                            || NodeBundle {
                                style: Style {
                                    flex_direction: FlexDirection::Column,
                                    ..default()
                                },
                                ..default()
                            },
                            compose::from_iter(*families, |family| {
                                spawn(|| TextBundle::from(family.to_string()), ())
                            }),
                        ),
                    ),
                )
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
