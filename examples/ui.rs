use actuate::prelude::*;
use bevy::prelude::*;
use bevy_mod_actuate::{compose, Runtime};

#[derive(Data)]
struct Ui;

impl Compose for Ui {
    fn compose(_cx: Scope<Self>) -> impl Compose {
        dbg!("App!");
    }
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .insert_non_send_resource(Runtime::new(Ui))
        .add_systems(Update, compose)
        .run();
}
