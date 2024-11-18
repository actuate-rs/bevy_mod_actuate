use actuate::prelude::*;
use bevy::prelude::*;
use bevy_mod_actuate::{compose, update, Runtime, Spawn};

#[derive(Clone, Component, Data)]
struct A;

#[derive(Debug, Resource)]
struct X(i32);

#[derive(Data)]
struct Ui;

impl Compose for Ui {
    fn compose(cx: Scope<Self>) -> impl Compose {
        Spawn::new(
            || NodeBundle {
                style: Style {
                    flex_direction: FlexDirection::Row,
                    ..default()
                },
                ..default()
            },
            (
                Spawn::new(|| TextBundle::from("Hello!"), ()),
                Spawn::new(|| TextBundle::from("World!"), ()),
            ),
        )
    }
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .insert_non_send_resource(Runtime::new(Ui))
        .insert_resource(X(0))
        .add_systems(Startup, setup)
        .add_systems(Update, (compose, update))
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2dBundle::default());
}
